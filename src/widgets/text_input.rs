use std::any::Any;
use std::fmt::Debug;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_HIT_TEST_METRICS, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, IDWriteFactory6, IDWriteTextFormat3, IDWriteTextLayout,
};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::System::Ole::{DROPEFFECT_COPY, DROPEFFECT_MOVE, DROPEFFECT_NONE};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_A, VK_BACK, VK_C, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_LEFT, VK_RIGHT, VK_UP,
    VK_V, VK_X, VK_Z,
};
use windows::Win32::UI::WindowsAndMessaging::STRSAFE_E_INSUFFICIENT_BUFFER;
use windows::core::Result;

use crate::gfx::command_recorder::CommandRecorder;
use crate::gfx::{PointDIP, RectDIP};
use crate::layout::UIArenas;
use crate::layout::model::ElementStyle;
use crate::runtime::clipboard::get_clipboard_text;
use crate::runtime::font_manager::{FontAxes, FontIdentifier, GlobalFontManager, LineSpacing};
use crate::runtime::vkey::VKey;
use crate::widgets::text::{ParagraphAlignment, TextAlignment};
use crate::widgets::{
    Bounds, DragData, DragInfo, DropResult, Instance, Widget, WidgetDragDropTarget, limit_response,
};
use crate::{DeferredControl, InputMethod, RedrawRequest, Shell, with_state};
use unicode_segmentation::UnicodeSegmentation;

const BLINK_TIME: f64 = 0.5;
const CARET_WIDTH: f32 = 1.0;
const LINE_OFFSET: f32 = 1.0;
const MAX_UNDO_LEVELS: usize = 100;
const UNDO_MERGE_TIME_MS: u128 = 1000; // 1 second

/// Represents a state that can be undone/redone
#[derive(Debug, Clone)]
struct UndoState {
    text: String,
    selection_anchor: u32,
    selection_active: u32,
    timestamp: u128,
    operation_type: UndoOperationType,
}

#[derive(Debug, Clone, PartialEq)]
enum UndoOperationType {
    CharacterInsertion,
    CharacterDeletion,
    WordDeletion,
    Other,
}

pub type TextInputEventHandler = Box<dyn Fn(&str) + 'static>;

/// A widget that renders selectable text using DirectWrite and draws
/// the selection highlight using Direct2D.
///
/// It encapsulates selection state, hit-testing, and cached layout bounds
/// for cursor hit-testing.
pub struct TextInput<Message> {
    _marker: std::marker::PhantomData<Message>,

    on_text_changed: Option<TextInputEventHandler>,
    pub text_alignment: TextAlignment,
    pub paragraph_alignment: ParagraphAlignment,
    pub font_size: f32,
    pub line_spacing: Option<LineSpacing>,
    pub font_id: FontIdentifier,
}

impl<Message> Default for TextInput<Message> {
    fn default() -> Self {
        Self {
            _marker: std::marker::PhantomData,
            on_text_changed: None,
            text_alignment: TextAlignment::Leading,
            paragraph_alignment: ParagraphAlignment::Top,
            font_size: 14.0,
            line_spacing: None,
            font_id: FontIdentifier::system("Segoe UI"),
        }
    }
}

impl<Message> Debug for TextInput<Message> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextInput")
            .field("on_text_changed", &self.on_text_changed.is_some())
            .finish()
    }
}

impl<Message: 'static> TextInput<Message> {
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
            on_text_changed: None,
            text_alignment: TextAlignment::Leading,
            paragraph_alignment: ParagraphAlignment::Top,
            font_size: 14.0,
            line_spacing: None,
            font_id: FontIdentifier::system("Segoe UI"),
        }
    }

    pub fn with_text_changed_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(&str) + 'static,
    {
        self.on_text_changed = Some(Box::new(handler));
        self
    }

    pub fn with_text_alignment(mut self, alignment: TextAlignment) -> Self {
        self.text_alignment = alignment;
        self
    }

    pub fn with_paragraph_alignment(mut self, alignment: ParagraphAlignment) -> Self {
        self.paragraph_alignment = alignment;
        self
    }

    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_font_family(mut self, font_id: FontIdentifier) -> Self {
        self.font_id = font_id;
        self
    }

    pub fn with_font_id(mut self, font_id: FontIdentifier) -> Self {
        self.font_id = font_id;
        self
    }

    pub fn get_text<'a>(&self, instance: &'a Instance) -> &'a str {
        &with_state!(instance as WidgetState<Message>).text
    }

    pub fn set_text(&self, instance: &mut Instance, text: String) {
        let state = with_state!(mut instance as WidgetState<Message>);
        state.text = text;
        state.selection_anchor = 0;
        state.selection_active = 0;
        state.recompute_text_boundaries();
        let _ = state.build_text_layout();
        let _ = state.recalc_metrics();
    }
}

struct WidgetState<Message> {
    _marker: std::marker::PhantomData<Message>,

    // DirectWrite objects (shared/cloneable COM interfaces)
    dwrite_factory: IDWriteFactory6,
    text_format: IDWriteTextFormat3,
    text: String,
    last_emitted_text: String,

    // Cached formatting properties
    cached_font_size: f32,
    cached_line_spacing: Option<LineSpacing>,
    cached_font_id: FontIdentifier,
    cached_text_alignment: TextAlignment,
    cached_paragraph_alignment: ParagraphAlignment,

    // layout
    bounds: RectDIP,
    layout: Option<IDWriteTextLayout>,

    // Selection state (UTF-16 code unit indices)
    selection_anchor: u32,
    selection_active: u32,
    is_dragging: bool,
    has_started_ole_drag: bool,
    drag_start_position: Option<PointDIP>,

    // caret_blink_timer: f64,
    // caret_visible: bool,
    focused_at: Instant,
    created_at: Instant,

    // Preferred horizontal position (DIPs) for vertical navigation (sticky X)
    sticky_x_dip: Option<f32>,

    // Cached layout bounds in DIPs (for cursor hit-testing)
    metric_bounds: RectDIP,

    // Cached segmentation data (recomputed when text changes)
    utf16_boundaries: Vec<u32>,
    word_starts_utf16: Vec<u32>,
    word_ranges_utf16: Vec<(u32, u32)>,

    // IME composition state (preedit). When Some, we draw `ime_text` at the
    // composition anchor position (start of current selection) with underline.
    ime_text: Option<String>,
    ime_cursor16: u32, // caret within ime_text in UTF-16 code units

    // Selection behavior
    selection_mode: SelectionMode,
    // Original drag-down location in UTF-16 units (for extending by word/paragraph)
    drag_origin16: u32,

    // OLE drag-over preview caret position (UTF-16 index). When Some, draw a caret
    // at this position to indicate the drop location during OLE drag-over.
    ole_drop_preview16: Option<u32>,
    can_drag_drop: bool,

    // Undo/redo system
    undo_stack: Vec<UndoState>,
    redo_stack: Vec<UndoState>,
}

impl<Message: 'static> WidgetState<Message> {
    pub fn into_any(self) -> Box<dyn Any> {
        Box::new(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionMode {
    Char,
    Word,
    Paragraph,
}

impl<Message: 'static> Widget<Message> for TextInput<Message> {
    fn state(
        &self,
        _arenas: &UIArenas,
        device_resources: &crate::runtime::DeviceResources,
    ) -> super::State {
        match WidgetState::<Message>::new(
            device_resources.dwrite_factory.clone(),
            &self.font_id,
            self.font_size,
            self.line_spacing,
            self.text_alignment,
            self.paragraph_alignment,
        ) {
            Ok(state) => Some(state.into_any()),
            Err(_) => None,
        }
    }

    fn limits_x(&self, _arenas: &UIArenas, instance: &mut Instance) -> limit_response::SizingForX {
        let state = with_state!(instance as WidgetState<Message>);
        if let Some(layout) = &state.layout {
            let min_width = unsafe { layout.DetermineMinWidth().unwrap() };

            unsafe {
                layout.SetMaxWidth(f32::INFINITY).unwrap();
            }

            let mut max_metrics = DWRITE_TEXT_METRICS::default();
            unsafe { layout.GetMetrics(&mut max_metrics).unwrap() };

            limit_response::SizingForX {
                min_width,
                preferred_width: max_metrics.widthIncludingTrailingWhitespace,
            }
        } else {
            limit_response::SizingForX {
                min_width: 0.0,
                preferred_width: 0.0,
            }
        }
    }

    fn limits_y(
        &self,
        _arenas: &UIArenas,
        instance: &mut Instance,
        _border_width: f32,
        content_width: f32,
    ) -> limit_response::SizingForY {
        let state = with_state!(instance as WidgetState<Message>);
        if let Some(layout) = &state.layout {
            unsafe {
                layout.SetMaxWidth(content_width).unwrap();
            }

            let mut max_metrics = DWRITE_TEXT_METRICS::default();
            unsafe { layout.GetMetrics(&mut max_metrics).unwrap() };

            limit_response::SizingForY {
                min_height: max_metrics.height,
                preferred_height: max_metrics.height,
            }
        } else {
            limit_response::SizingForY {
                min_height: 0.0,
                preferred_height: 0.0,
            }
        }
    }

    fn update(
        &mut self,
        _arenas: &mut UIArenas,
        instance: &mut Instance,
        hwnd: HWND,
        shell: &mut Shell<Message>,
        event: &super::Event,
        bounds: Bounds,
    ) {
        let state = with_state!(mut instance as WidgetState<Message>);

        let RectDIP {
            x: x_dip, y: y_dip, ..
        } = bounds.content_box;
        match event {
            super::Event::MouseButtonDown {
                x,
                y,
                click_count,
                modifiers,
            } => {
                // Complete composition before altering selection
                if state.ime_text.is_some() {
                    state.ime_end();
                    shell.request_input_method(InputMethod::Disabled);
                }

                if (PointDIP { x: *x, y: *y }).within(bounds.border_box) {
                    if let Ok(idx) = state.hit_test_index(x - x_dip, y - y_dip) {
                        shell.focus_manager.focus(instance.id);

                        // Store the drag start position for OLE drag detection
                        state.drag_start_position = Some(PointDIP {
                            x: x - x_dip,
                            y: y - y_dip,
                        });

                        // Selection mode by click count
                        let mode = match click_count {
                            1 => SelectionMode::Char,
                            x if x % 2 == 0 => SelectionMode::Word,
                            _ => SelectionMode::Paragraph,
                        };
                        state.set_selection_mode(mode);
                        state.begin_drag(idx, modifiers.shift);
                        if modifiers.shift {
                            state.update_drag(x - x_dip, y - y_dip);
                        }
                    }
                } else {
                    shell.focus_manager.release_focus(instance.id);
                }
            }
            super::Event::MouseButtonUp { x, y, .. } => {
                if let Ok(idx) = state.hit_test_index(x - x_dip, y - y_dip) {
                    state.end_drag(idx);
                } else {
                    state.end_drag(0);
                }

                // Reset OLE drag state
                state.has_started_ole_drag = false;
                state.drag_start_position = None;
            }
            super::Event::MouseMove { x, y }
            | super::Event::MouseEnter { x, y }
            | super::Event::MouseLeave { x, y } => {
                let widget_x = x - x_dip;
                let widget_y = y - y_dip;

                // Check if we should start an OLE drag operation
                if state.is_dragging && state.can_drag_drop && !state.has_started_ole_drag {
                    // Check if we have selected text and the drag started within the selection
                    if let Some(start_pos) = state.drag_start_position {
                        if let Some(drag_data) = state.can_ole_drag(start_pos) {
                            // Start OLE drag operation
                            state.handoff_ole_drag(instance.id, shell, &drag_data);

                            return; // Don't update text selection when starting OLE drag
                        }
                    }
                }

                if state.update_drag(widget_x, widget_y) {
                    let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
                }
            }
            super::Event::KeyDown { key, modifiers, .. } => {
                if shell.focus_manager.is_focused(instance.id) {
                    let shift_down = modifiers.shift;
                    let ctrl_down = modifiers.ctrl;
                    let _handled = match *key {
                        VKey::LEFT => {
                            if ctrl_down {
                                state.move_word_left(shift_down);
                            } else {
                                state.move_left(shift_down);
                            }
                            true
                        }
                        VKey::RIGHT => {
                            if ctrl_down {
                                state.move_word_right(shift_down);
                            } else {
                                state.move_right(shift_down);
                            }
                            true
                        }
                        VKey::UP => {
                            state.move_up(shift_down);
                            true
                        }
                        VKey::DOWN => {
                            state.move_down(shift_down);
                            true
                        }
                        VKey::HOME => {
                            state.move_to_start(shift_down);
                            true
                        }
                        VKey::END => {
                            state.move_to_end(shift_down);
                            true
                        }
                        VKey::BACK => {
                            if ctrl_down {
                                let _ = state.backspace_word();
                            } else {
                                let _ = state.backspace();
                            }
                            true
                        }
                        VKey::DELETE => {
                            if ctrl_down {
                                let _ = state.delete_word_forward();
                            } else {
                                let _ = state.delete_forward();
                            }
                            true
                        }
                        VKey::A if ctrl_down => {
                            state.select_all();
                            true
                        }
                        VKey::C if ctrl_down => {
                            if let Some(s) = state.selected_text() {
                                // let _ = set_clipboard_text(hwnd, &s);
                                shell.queue_deferred_control(DeferredControl::SetClipboardText(
                                    s.to_string(),
                                ));
                            }
                            true
                        }
                        VKey::X if ctrl_down => {
                            if let Some(s) = state.selected_text() {
                                // let _ = set_clipboard_text(hwnd, &s);
                                shell.queue_deferred_control(DeferredControl::SetClipboardText(
                                    s.to_string(),
                                ));
                                let _ = state.insert_str("");
                            }
                            true
                        }
                        VKey::V if ctrl_down => {
                            if !state.is_composing() {
                                if let Some(s) = get_clipboard_text(hwnd) {
                                    let _ = state.insert_str(&s);
                                }
                            }
                            true
                        }
                        VKey::Z if ctrl_down && shift_down => {
                            let _ = state.redo();
                            true
                        }
                        VKey::Z if ctrl_down => {
                            let _ = state.undo();
                            true
                        }
                        VKey::ESCAPE => {
                            if state.has_selection() {
                                state.clear_selection();
                            } else {
                                shell.focus_manager.release_focus(instance.id);
                            }
                            true
                        }
                        _ => false,
                    };
                }
            }
            super::Event::KeyUp { .. } => {}
            super::Event::Char { text } => {
                if shell.focus_manager.is_focused(instance.id) {
                    let _ = state.insert_str(text.as_str());
                }
            }
            super::Event::ImeStartComposition => {
                if shell.focus_manager.is_focused(instance.id) {
                    state.ime_begin();

                    if let Ok((c_x_dip, c_y_dip, h)) = state.caret_pos_dip(state.caret_active16()) {
                        shell.request_input_method(InputMethod::Enabled {
                            position: PointDIP {
                                x: x_dip + c_x_dip,
                                y: y_dip + c_y_dip + h,
                            },
                        });
                    }
                }
            }
            super::Event::ImeComposition { text, caret_units } => {
                if shell.focus_manager.is_focused(instance.id) {
                    state.ime_update(text.clone(), *caret_units);

                    if let Ok((c_x_dip, c_y_dip, h)) = state.caret_pos_dip(state.caret_active16()) {
                        shell.request_input_method(InputMethod::Enabled {
                            position: PointDIP {
                                x: x_dip + c_x_dip,
                                y: y_dip + c_y_dip + h,
                            },
                        });
                    }
                }
            }
            super::Event::ImeCommit { text } => {
                if shell.focus_manager.is_focused(instance.id) {
                    state.ime_commit(text.clone()).expect("ime commit failed");
                }
            }
            super::Event::ImeEndComposition => {
                if shell.focus_manager.is_focused(instance.id) {
                    state.ime_end();
                }
            }
            super::Event::Redraw { now } => {
                if shell.focus_manager.is_focused(instance.id) {
                    let next_blink =
                        (0.05 + BLINK_TIME) - (*now - state.focused_at).as_secs_f64() % BLINK_TIME;
                    shell.request_redraw(
                        hwnd,
                        RedrawRequest::At(*now + Duration::from_secs_f64(next_blink)),
                    );
                }
            }

            super::Event::DragFinish { effect } => {
                if state.has_started_ole_drag {
                    // If it was a move operation, delete the selected text
                    if (effect.0 & DROPEFFECT_MOVE.0) != 0 && state.can_drag_drop() {
                        let _ = state.insert_str("");
                    }

                    state.reset_drag_state();
                }
            }

            _ => {
                // Unhandled event
            }
        }

        if state.text != state.last_emitted_text {
            state.last_emitted_text = state.text.clone();
            if let Some(cb) = self.on_text_changed.as_ref() {
                cb(&state.text);
            }
        }
    }

    fn paint(
        &mut self,
        _arenas: &UIArenas,
        instance: &mut Instance,
        shell: &Shell<Message>,
        recorder: &mut CommandRecorder,
        style: ElementStyle,
        bounds: Bounds,
        _now: Instant,
    ) {
        let state = with_state!(mut instance as WidgetState<Message>);

        // Rebuild text format if needed
        if state.needs_text_format_rebuild(
            &self.font_id,
            self.font_size,
            self.line_spacing,
            self.text_alignment,
            self.paragraph_alignment,
        ) {
            let _ = state.rebuild_text_format(
                &self.font_id,
                self.font_size,
                self.line_spacing,
                self.text_alignment,
                self.paragraph_alignment,
            );
        }

        state
            .update_bounds(bounds.content_box)
            .expect("update bounds failed");

        state
            .draw(
                instance.id,
                shell,
                recorder,
                style,
                bounds.content_box,
                _now,
            )
            .expect("draw failed");
    }

    fn cursor(
        &self,
        _arenas: &UIArenas,
        _instance: &Instance,
        point: PointDIP,
        bounds: Bounds,
    ) -> Option<super::Cursor> {
        if point.within(bounds.border_box) {
            Some(super::Cursor::IBeam)
        } else {
            None
        }
    }

    fn as_drop_target(&mut self) -> Option<&mut dyn WidgetDragDropTarget<Message>> {
        Some(self)
    }
}

impl<Message: 'static> WidgetDragDropTarget<Message> for TextInput<Message> {
    fn drag_enter(
        &mut self,
        instance: &mut Instance,
        drag_info: &DragInfo,
        widget_bounds: Bounds,
    ) -> windows::Win32::System::Ole::DROPEFFECT {
        let state = with_state!(mut instance as WidgetState<Message>);
        match &drag_info.data {
            DragData::Text(_) => {
                let widget_x = drag_info.position.x - widget_bounds.content_box.x;
                let widget_y = drag_info.position.y - widget_bounds.content_box.y;

                if let Ok(idx) = state.hit_test_index(widget_x, widget_y) {
                    state.set_ole_drop_preview(Some(idx));
                    state.get_drop_effect(&drag_info.allowed_effects)
                } else {
                    DROPEFFECT_NONE
                }
            }
        }
    }

    fn drag_over(
        &mut self,
        instance: &mut Instance,
        drag_info: &DragInfo,
        widget_bounds: Bounds,
    ) -> windows::Win32::System::Ole::DROPEFFECT {
        let state = with_state!(mut instance as WidgetState<Message>);
        match &drag_info.data {
            DragData::Text(_) => {
                let widget_x = drag_info.position.x - widget_bounds.content_box.x;
                let widget_y = drag_info.position.y - widget_bounds.content_box.y;

                if let Ok(idx16) = state.hit_test_index(widget_x, widget_y) {
                    state.set_ole_drop_preview(Some(idx16));
                }

                state.get_drop_effect(&drag_info.allowed_effects)
            }
        }
    }

    fn drag_leave(&mut self, instance: &mut Instance, _widget_bounds: Bounds) {
        let state = with_state!(mut instance as WidgetState<Message>);
        state.set_ole_drop_preview(None);
    }

    fn drop(
        &mut self,
        instance: &mut Instance,
        shell: &mut Shell<Message>,
        drag_info: &DragInfo,
        widget_bounds: Bounds,
    ) -> DropResult {
        let state = with_state!(mut instance as WidgetState<Message>);
        match &drag_info.data {
            DragData::Text(text) => {
                // Convert client coordinates to widget-relative coordinates
                let widget_pos = PointDIP {
                    x: drag_info.position.x - widget_bounds.content_box.x,
                    y: drag_info.position.y - widget_bounds.content_box.y,
                };

                if let Ok(drop_idx) = state.hit_test_index(widget_pos.x, widget_pos.y) {
                    let effect = state.get_drop_effect(&drag_info.allowed_effects);

                    if let Some(result) = state.handle_text_drop(text, drop_idx, effect) {
                        state.set_ole_drop_preview(None);
                        shell.focus_manager.focus(instance.id);
                        result
                    } else {
                        DropResult::default()
                    }
                } else {
                    DropResult::default()
                }
            }
        }
    }
}

impl<Message> WidgetState<Message> {
    pub fn new(
        dwrite_factory: IDWriteFactory6,
        font_id: &FontIdentifier,
        font_size: f32,
        line_spacing: Option<LineSpacing>,
        text_alignment: TextAlignment,
        paragraph_alignment: ParagraphAlignment,
    ) -> Result<Self> {
        let text_format = GlobalFontManager::create_text_format(
            font_id,
            font_size,
            FontAxes::default(),
            line_spacing,
            "en-us",
        )?;

        let text_format = unsafe {
            // Set text alignment
            let dwrite_text_alignment = match text_alignment {
                TextAlignment::Leading => DWRITE_TEXT_ALIGNMENT_LEADING,
                TextAlignment::Center => {
                    windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_CENTER
                }
                TextAlignment::Trailing => {
                    windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_TRAILING
                }
            };
            text_format.SetTextAlignment(dwrite_text_alignment)?;

            // Set paragraph alignment
            let dwrite_paragraph_alignment = match paragraph_alignment {
                ParagraphAlignment::Top => DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
                ParagraphAlignment::Center => {
                    windows::Win32::Graphics::DirectWrite::DWRITE_PARAGRAPH_ALIGNMENT_CENTER
                }
                ParagraphAlignment::Bottom => {
                    windows::Win32::Graphics::DirectWrite::DWRITE_PARAGRAPH_ALIGNMENT_FAR
                }
            };
            text_format.SetParagraphAlignment(dwrite_paragraph_alignment)?;

            text_format
        };

        let mut s = Self {
            _marker: std::marker::PhantomData,
            dwrite_factory,
            text_format,
            text: String::new(),
            cached_font_size: font_size,
            cached_line_spacing: line_spacing,
            cached_font_id: font_id.clone(),
            cached_text_alignment: text_alignment,
            cached_paragraph_alignment: paragraph_alignment,
            bounds: RectDIP::default(),
            layout: None,
            selection_anchor: 0,
            selection_active: 0,
            is_dragging: false,
            has_started_ole_drag: false,
            drag_start_position: None,
            focused_at: Instant::now(),
            created_at: Instant::now(),
            sticky_x_dip: None,
            metric_bounds: RectDIP::default(),
            utf16_boundaries: Vec::new(),
            word_starts_utf16: Vec::new(),
            word_ranges_utf16: Vec::new(),
            ime_text: None,
            ime_cursor16: 0,
            selection_mode: SelectionMode::Char,
            drag_origin16: 0,
            ole_drop_preview16: None,
            can_drag_drop: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_emitted_text: String::new(),
        };
        s.recompute_text_boundaries();
        s.build_text_layout()?;
        Ok(s)
    }

    fn needs_text_format_rebuild(
        &self,
        font_id: &FontIdentifier,
        font_size: f32,
        line_spacing: Option<LineSpacing>,
        text_alignment: TextAlignment,
        paragraph_alignment: ParagraphAlignment,
    ) -> bool {
        self.cached_font_id != *font_id
            || self.cached_font_size != font_size
            || self.cached_line_spacing != line_spacing
            || self.cached_text_alignment != text_alignment
            || self.cached_paragraph_alignment != paragraph_alignment
    }

    fn rebuild_text_format(
        &mut self,
        font_id: &FontIdentifier,
        font_size: f32,
        line_spacing: Option<LineSpacing>,
        text_alignment: TextAlignment,
        paragraph_alignment: ParagraphAlignment,
    ) -> Result<()> {
        self.text_format = GlobalFontManager::create_text_format(
            font_id,
            font_size,
            FontAxes::default(),
            line_spacing,
            "en-us",
        )?;

        unsafe {
            // Set text alignment
            let dwrite_text_alignment = match text_alignment {
                TextAlignment::Leading => DWRITE_TEXT_ALIGNMENT_LEADING,
                TextAlignment::Center => {
                    windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_CENTER
                }
                TextAlignment::Trailing => {
                    windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT_TRAILING
                }
            };
            self.text_format.SetTextAlignment(dwrite_text_alignment)?;

            // Set paragraph alignment
            let dwrite_paragraph_alignment = match paragraph_alignment {
                ParagraphAlignment::Top => DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
                ParagraphAlignment::Center => {
                    windows::Win32::Graphics::DirectWrite::DWRITE_PARAGRAPH_ALIGNMENT_CENTER
                }
                ParagraphAlignment::Bottom => {
                    windows::Win32::Graphics::DirectWrite::DWRITE_PARAGRAPH_ALIGNMENT_FAR
                }
            };
            self.text_format
                .SetParagraphAlignment(dwrite_paragraph_alignment)?;
        }

        // Update cached values
        self.cached_font_id = font_id.clone();
        self.cached_font_size = font_size;
        self.cached_text_alignment = text_alignment;
        self.cached_paragraph_alignment = paragraph_alignment;

        Ok(())
    }

    /// Build a text layout for the given text and maximum size in DIPs.
    pub fn build_text_layout(&mut self) -> Result<()> {
        unsafe {
            let wtext: Vec<u16> = self.text.encode_utf16().collect();
            let layout = if self.is_composing() {
                let (start16, end16) = self.selection_range();
                let base_w: Vec<u16> = self.text.encode_utf16().collect();
                let ime_w: Vec<u16> = self.ime_text.as_ref().unwrap().encode_utf16().collect();
                let mut composed = Vec::with_capacity(base_w.len() + ime_w.len());
                composed.extend_from_slice(&base_w[..start16 as usize]);
                let underline_start = composed.len() as u32;
                composed.extend_from_slice(&ime_w);
                let underline_len = (composed.len() as u32).saturating_sub(underline_start);
                composed.extend_from_slice(&base_w[end16 as usize..]);

                let composed_layout = self.dwrite_factory.CreateTextLayout(
                    &composed,
                    &self.text_format,
                    self.bounds.width,
                    self.bounds.height,
                )?;
                let range = DWRITE_TEXT_RANGE {
                    startPosition: underline_start,
                    length: underline_len,
                };
                composed_layout.SetUnderline(true, range)?;
                composed_layout
            } else {
                self.dwrite_factory.CreateTextLayout(
                    &wtext,
                    &self.text_format,
                    self.bounds.width,
                    self.bounds.height,
                )?
            };

            self.layout = Some(layout);
            Ok(())
        }
    }

    pub fn update_bounds(&mut self, bounds: RectDIP) -> Result<()> {
        self.bounds = bounds;
        if self.layout.is_none() {
            self.build_text_layout()?;
        }

        unsafe {
            let layout = self.layout.as_ref().expect("layout not built");
            layout.SetMaxWidth(bounds.width).unwrap();
            layout.SetMaxHeight(bounds.height).unwrap();

            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics).unwrap();
            self.metric_bounds = RectDIP {
                x: metrics.left,
                y: metrics.top,
                width: metrics.width,
                height: metrics.height,
            };
        }

        Ok(())
    }

    /// Draw selection highlight behind the text for the currently selected range.
    fn draw_selection_with_recorder(
        &self,
        layout: &IDWriteTextLayout,
        recorder: &mut crate::gfx::command_recorder::CommandRecorder,
        bounds: RectDIP,
    ) -> Result<()> {
        unsafe {
            let sel_start = self.selection_anchor.min(self.selection_active);
            let sel_end = self.selection_anchor.max(self.selection_active);
            let sel_len = sel_end.saturating_sub(sel_start);
            if sel_len == 0 {
                return Ok(());
            }

            let mut needed: u32 = 0;
            match layout.HitTestTextRange(sel_start, sel_len, 0.0, 0.0, None, &mut needed) {
                Ok(()) => {
                    // Nothing visible to draw
                    Ok(())
                }
                Err(e) if e.code() == STRSAFE_E_INSUFFICIENT_BUFFER => {
                    let capacity = needed.max(1);
                    let mut runs = vec![DWRITE_HIT_TEST_METRICS::default(); capacity as usize];
                    let mut actual: u32 = 0;
                    match layout.HitTestTextRange(
                        sel_start,
                        sel_len,
                        0.0,
                        0.0,
                        Some(&mut runs),
                        &mut actual,
                    ) {
                        Ok(()) => {
                            // Selection color (light blue)
                            for m in runs.iter().take(actual as usize) {
                                recorder.fill_rectangle(
                                    &RectDIP {
                                        x: bounds.x + m.left,
                                        y: bounds.y + m.top,
                                        width: m.width,
                                        height: m.height,
                                    },
                                    crate::layout::model::Color {
                                        r: 0.2,
                                        g: 0.4,
                                        b: 1.0,
                                        a: 0.35,
                                    },
                                );
                            }
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                Err(e) => Err(e),
            }
        }
    }

    pub fn draw(
        &mut self,
        id: u64,
        shell: &Shell<Message>,
        recorder: &mut CommandRecorder,
        style: ElementStyle,
        bounds: RectDIP,
        now: Instant,
    ) -> Result<()> {
        unsafe {
            let layout = self.layout.as_ref().expect("layout not built");

            let caret_visible = (((now - self.focused_at).as_secs_f64()) / BLINK_TIME) % 2.0 < 1.0;

            // Normal rendering: selection, base text, caret
            self.draw_selection_with_recorder(layout, recorder, bounds)?;

            // Draw text using command recorder
            let color = style.color.unwrap_or_default();
            recorder.draw_text(&bounds, layout, color);

            // OLE drag-over preview caret
            if let Some(drop) = self.ole_drop_preview16 {
                let drop = self.snap_to_scalar_boundary(drop);
                let (src_start, src_end) = self.selection_range();
                if !(self.has_selection() && drop >= src_start && drop <= src_end) {
                    let mut x = 0.0f32;
                    let mut y = 0.0f32;
                    let mut m = DWRITE_HIT_TEST_METRICS::default();
                    layout.HitTestTextPosition(drop, false, &mut x, &mut y, &mut m)?;
                    let caret_rect = RectDIP {
                        x: bounds.x + x,
                        y: bounds.y + m.top,
                        width: CARET_WIDTH,
                        height: m.height,
                    };
                    recorder.fill_rectangle(&caret_rect, color);
                }
            } else {
                // Draw caret if there's no selection (1 DIP wide bar)
                let sel_start = self.selection_anchor.min(self.selection_active);
                let sel_end = self.selection_anchor.max(self.selection_active);
                if shell.focus_manager.is_focused(id) && caret_visible {
                    if self.is_composing() {
                        let ime_caret_pos = sel_start + self.ime_cursor16;
                        let mut x = 0.0f32;
                        let mut y = 0.0f32;
                        let mut m = DWRITE_HIT_TEST_METRICS::default();
                        layout.HitTestTextPosition(ime_caret_pos, false, &mut x, &mut y, &mut m)?;
                        let caret_rect = RectDIP {
                            x: bounds.x + x,
                            y: bounds.y + m.top,
                            width: CARET_WIDTH,
                            height: m.height,
                        };
                        recorder.fill_rectangle(&caret_rect, color);
                    } else if sel_start == sel_end {
                        let mut x = 0.0f32;
                        let mut y = 0.0f32;
                        let mut m = DWRITE_HIT_TEST_METRICS::default();

                        layout.HitTestTextPosition(
                            self.selection_active,
                            false,
                            &mut x,
                            &mut y,
                            &mut m,
                        )?;
                        let caret_rect = RectDIP {
                            x: bounds.x + x,
                            y: bounds.y + m.top,
                            width: CARET_WIDTH,
                            height: m.height,
                        };
                        recorder.fill_rectangle(&caret_rect, color);
                    }
                }
            }

            Ok(())
        }
    }

    /// Hit-test a point in DIPs against the given text with the provided layout bounds.
    pub fn hit_test_index(&self, x_dip: f32, y_dip: f32) -> Result<u32> {
        unsafe {
            let layout = self.layout.as_ref().expect("layout not built");
            let mut trailing = windows::core::BOOL(0);
            let mut inside = windows::core::BOOL(0);
            let mut metrics = DWRITE_HIT_TEST_METRICS::default();
            layout.HitTestPoint(x_dip, y_dip, &mut trailing, &mut inside, &mut metrics)?;

            let mut idx = if trailing.as_bool() {
                metrics.textPosition.saturating_add(metrics.length)
            } else {
                metrics.textPosition
            };
            let total_len = self.text.encode_utf16().count() as u32;
            if idx > total_len {
                idx = total_len;
            }
            Ok(idx)
        }
    }

    fn force_blink(&mut self) {
        self.focused_at = Instant::now();
    }

    fn clear_sticky_x(&mut self) {
        self.sticky_x_dip = None;
    }

    fn can_ole_drag(&self, position: PointDIP) -> Option<DragData> {
        // Check if we have selected text and the position is within the selection
        let (sel_start, sel_end) = self.selection_range();
        if sel_start != sel_end {
            if let Ok(idx) = self.hit_test_index(position.x, position.y) {
                if idx >= sel_start && idx <= sel_end {
                    if let Some(selected_text) = self.selected_text() {
                        return Some(DragData::Text(selected_text.to_owned()));
                    }
                }
            }
        }
        None
    }

    fn handoff_ole_drag(&mut self, instance_id: u64, shell: &mut Shell<Message>, data: &DragData) {
        // Mark that we can perform drag-to-move
        self.set_can_drag_drop(true);

        // Start OLE drag operation with the selected text
        let DragData::Text(text) = data;
        self.has_started_ole_drag = true;
        shell.queue_deferred_control(DeferredControl::StartDrag {
            data: DragData::Text(text.clone()),
            src_id: instance_id,
        });
    }

    // Drag/select helpers
    pub fn begin_drag(&mut self, idx: u32, extend: bool) {
        // Mouse interaction resets sticky X
        self.clear_sticky_x();
        let idx = self.snap_to_scalar_boundary(idx);
        self.can_drag_drop = false;

        if !extend {
            self.drag_origin16 = idx;

            // Drag-to-move applies only to Char mode. For Word/Paragraph clicks, always compute selection.
            match self.selection_mode {
                SelectionMode::Char => {
                    // If there is an existing non-empty selection and the drag starts inside it,
                    // switch to drag-to-move mode and keep the selection intact.
                    let (sel_start, sel_end) = self.selection_range();
                    if sel_end > sel_start && idx >= sel_start && idx < sel_end {
                        self.can_drag_drop = true;
                    } else {
                        self.selection_anchor = idx;
                        self.selection_active = idx;
                    }
                }
                SelectionMode::Word => {
                    let (ws, we) = self.word_range_at(idx);
                    self.selection_anchor = ws;
                    self.selection_active = we;
                }
                SelectionMode::Paragraph => {
                    let (ps, pe) = self.paragraph_range_at(idx);
                    self.selection_anchor = ps;
                    self.selection_active = pe;
                }
            }
        }

        self.is_dragging = true;
        self.force_blink();
    }

    pub fn update_drag(&mut self, x_dip: f32, y_dip: f32) -> bool {
        if !self.is_dragging {
            return false;
        }
        let Ok(idx) = self.hit_test_index(x_dip, y_dip) else {
            return false;
        };
        let idx = self.snap_to_scalar_boundary(idx);
        if self.can_drag_drop {
            false
        } else {
            let (old_a, old_b) = (self.selection_anchor, self.selection_active);
            match self.selection_mode {
                SelectionMode::Char => {
                    self.selection_active = idx;
                }
                SelectionMode::Word => {
                    let a = self.drag_origin16.min(idx);
                    let b = self.drag_origin16.max(idx);
                    let start = self.word_start_at(a);
                    let end = self.word_end_at(b);
                    self.selection_anchor = start;
                    self.selection_active = end;
                }
                SelectionMode::Paragraph => {
                    let a = self.drag_origin16.min(idx);
                    let b = self.drag_origin16.max(idx);
                    let (ps, _) = self.paragraph_range_at(a);
                    let (_, pe) = self.paragraph_range_at(b);
                    self.selection_anchor = ps;
                    self.selection_active = pe;
                }
            }
            self.clamp_sel_to_len();
            let changed = self.selection_anchor != old_a || self.selection_active != old_b;
            if changed {
                self.force_blink();
            }
            changed
        }
    }

    pub fn end_drag(&mut self, idx: u32) {
        self.is_dragging = false;

        if self.drag_origin16 == idx && self.can_drag_drop {
            // Click through to deselect at cursor
            self.selection_anchor = idx;
            self.selection_active = idx;
        }
    }

    pub fn set_can_drag_drop(&mut self, dragging: bool) {
        self.can_drag_drop = dragging;
    }

    pub fn can_drag_drop(&self) -> bool {
        self.can_drag_drop
    }

    /// Helper method to determine the appropriate drop effect based on allowed effects
    fn get_drop_effect(
        &self,
        allowed_effects: &windows::Win32::System::Ole::DROPEFFECT,
    ) -> windows::Win32::System::Ole::DROPEFFECT {
        if (allowed_effects.0 & DROPEFFECT_MOVE.0) != 0 {
            DROPEFFECT_MOVE
        } else if (allowed_effects.0 & DROPEFFECT_COPY.0) != 0 {
            DROPEFFECT_COPY
        } else {
            DROPEFFECT_NONE
        }
    }

    /// Reset all drag-related state
    fn reset_drag_state(&mut self) {
        self.set_ole_drop_preview(None);
        self.set_can_drag_drop(false);
        self.has_started_ole_drag = false;
        self.drag_start_position = None;
        self.end_drag(0);
    }

    /// Handle text drop logic, returning None if drop should be rejected
    fn handle_text_drop(
        &mut self,
        text: &str,
        drop_idx: u32,
        effect: windows::Win32::System::Ole::DROPEFFECT,
    ) -> Option<DropResult> {
        let is_same_widget = self.has_started_ole_drag;
        let (current_sel_start, current_sel_end) = self.selection_range();
        let dropping_over_selection = current_sel_start != current_sel_end
            && drop_idx >= current_sel_start
            && drop_idx <= current_sel_end;

        if is_same_widget && effect == DROPEFFECT_MOVE {
            // Handle move within the same widget
            if !dropping_over_selection
                && drop_idx >= current_sel_start
                && drop_idx <= current_sel_end
            {
                // Don't allow dropping within the dragged selection
                return None;
            }

            if dropping_over_selection {
                // Replace the existing selection with the dragged text
                self.insert_str(text).unwrap();
            } else {
                // Adjust drop position if it's after the dragged selection
                let adjusted_drop_idx = if drop_idx > current_sel_end {
                    drop_idx - (current_sel_end - current_sel_start)
                } else {
                    drop_idx
                };

                // Remove the dragged text first, then insert at adjusted position
                self.insert_str("").unwrap();
                self.move_caret_to(adjusted_drop_idx);
                self.insert_str(text).unwrap();
            }
        } else {
            // Normal drop (different widget or copy operation)
            if dropping_over_selection {
                // Replace the existing selection with the dropped text
                self.insert_str(text).unwrap();
            } else {
                // Insert at the drop position
                self.move_caret_to(drop_idx);
                self.insert_str(text).unwrap();
            }
        }

        self.move_caret_to(drop_idx);
        Some(DropResult {
            effect,
            handled: true,
        })
    }

    // ===== IME support =====
    pub fn is_composing(&self) -> bool {
        self.ime_text.is_some()
    }

    pub fn ime_begin(&mut self) {
        // Make sure to clear any selection before IME composition starts
        let (start16, end16) = self.selection_range();
        if start16 != end16 {
            self.insert_str("").unwrap();
        }

        self.ime_text = Some(String::new());
        self.ime_cursor16 = 0;
        self.force_blink();
        self.build_text_layout().unwrap();
    }

    pub fn ime_update(&mut self, s: String, cursor16: u32) {
        self.ime_text = Some(s);
        self.ime_cursor16 = cursor16;
        self.force_blink();
        self.build_text_layout().unwrap();
    }

    pub fn ime_commit(&mut self, s: String) -> Result<()> {
        // Commit replaces current selection with final string.
        self.insert_str(&s)?;
        self.force_blink();
        self.build_text_layout().unwrap();
        Ok(())
    }

    pub fn ime_end(&mut self) {
        self.ime_text = None;
        self.ime_cursor16 = 0;
        self.force_blink();
        self.build_text_layout().unwrap();
    }

    /// Caret DIP position for a given UTF-16 index in the base layout.
    pub fn caret_pos_dip(&self, idx16: u32) -> Result<(f32, f32, f32)> {
        unsafe {
            let layout = self.layout.as_ref().expect("layout not built");
            let mut x = 0.0f32;
            let mut y = 0.0f32;
            let mut m = DWRITE_HIT_TEST_METRICS::default();
            layout.HitTestTextPosition(idx16, false, &mut x, &mut y, &mut m)?;
            Ok((x, m.top, m.height))
        }
    }

    /// Get the active caret position in UTF-16 code units.
    pub fn caret_active16(&self) -> u32 {
        self.selection_active
    }

    /// Set or clear OLE drop preview caret. Returns true if it changed.
    pub fn set_ole_drop_preview(&mut self, idx: Option<u32>) -> bool {
        if self.ole_drop_preview16 != idx {
            self.ole_drop_preview16 = idx.map(|i| self.snap_to_scalar_boundary(i));
            self.force_blink();
            true
        } else {
            false
        }
    }

    /// Move caret (anchor and active) to an absolute UTF-16 index.
    pub fn move_caret_to(&mut self, idx16: u32) {
        self.clear_sticky_x();
        let idx = self.snap_to_scalar_boundary(idx16);
        self.selection_anchor = idx;
        self.selection_active = idx;
        self.clamp_sel_to_len();
        self.force_blink();
    }

    pub fn has_selection(&self) -> bool {
        self.selection_active != self.selection_anchor
    }

    pub fn clear_selection(&mut self) {
        self.selection_active = self.selection_anchor;
        self.force_blink();
    }

    // ===== Editing helpers =====
    fn utf16_len_of_str(s: &str) -> u32 {
        s.encode_utf16().count() as u32
    }

    fn byte_to_utf16_index(&self, byte_idx: usize) -> u32 {
        let s = &self.text[..byte_idx];
        Self::utf16_len_of_str(s)
    }

    fn word_range_at(&self, idx16: u32) -> (u32, u32) {
        for (ws, we) in &self.word_ranges_utf16 {
            if idx16 >= *ws && idx16 < *we {
                return (*ws, *we);
            }
            if idx16 < *ws {
                return (*ws, *we);
            }
        }
        // Fallback: last word or empty
        if let Some(&(s, e)) = self.word_ranges_utf16.last() {
            (s, e)
        } else {
            (0, 0)
        }
    }

    fn word_start_at(&self, idx16: u32) -> u32 {
        self.word_range_at(idx16).0
    }

    fn word_end_at(&self, idx16: u32) -> u32 {
        self.word_range_at(idx16).1
    }

    fn paragraph_range_at(&self, idx16: u32) -> (u32, u32) {
        // Find byte index equivalent to idx16
        let byte_idx = self.utf16_index_to_byte(idx16);
        let bytes = self.text.as_bytes();
        let mut start_byte = 0usize;
        if byte_idx > 0 {
            if let Some(pos) = bytes[..byte_idx].iter().rposition(|&c| c == b'\n') {
                start_byte = pos + 1;
            }
        }
        let mut end_byte = bytes.len();
        if let Some(off) = bytes[byte_idx..].iter().position(|&c| c == b'\n') {
            end_byte = byte_idx + off; // exclude newline
        }
        let start16 = self.byte_to_utf16_index(start_byte);
        let end16 = self.byte_to_utf16_index(end_byte);
        (start16, end16)
    }

    pub fn set_selection_mode(&mut self, mode: SelectionMode) {
        self.selection_mode = mode;
    }

    fn recompute_text_boundaries(&mut self) {
        // Recompute UTF-16 grapheme boundaries
        let mut boundaries: Vec<u32> = Vec::with_capacity(self.text.len().max(1));
        let mut acc16: u32 = 0;
        boundaries.push(0);
        for g in self.text.graphemes(true) {
            acc16 += g.encode_utf16().count() as u32;
            boundaries.push(acc16);
        }
        self.utf16_boundaries = boundaries;

        // Recompute word-like ranges in UTF-16 using Unicode word boundaries
        // and add separate selectable ranges for punctuation/symbol runs.
        // - unicode_words() yields proper words per UAX#29 (good for CJK, etc.)
        // - split_word_bounds() yields segments including punctuation and whitespace
        let mut starts: Vec<u32> = Vec::new();
        let mut ranges: Vec<(u32, u32)> = Vec::new();
        let mut acc16: u32 = 0;
        let mut words = self.text.unicode_words().peekable();
        for seg in self.text.split_word_bounds() {
            let seg_start = acc16;
            let seg_len16 = seg.encode_utf16().count() as u32;
            acc16 += seg_len16;

            // Skip pure-whitespace segments
            if seg.chars().all(|c| c.is_whitespace()) {
                continue;
            }

            // If this segment is the next Unicode word, record it as a word range
            if let Some(next_word) = words.peek() {
                if *next_word == seg {
                    starts.push(seg_start);
                    ranges.push((seg_start, seg_start + seg_len16));
                    let _ = words.next();
                    continue;
                }
            }

            // Otherwise, treat this non-whitespace segment (punct/symbol run)
            // as its own selectable block, kept distinct from adjacent words.
            starts.push(seg_start);
            ranges.push((seg_start, seg_start + seg_len16));
        }

        self.word_starts_utf16 = starts;
        self.word_ranges_utf16 = ranges;
    }

    fn clamp_sel_to_len(&mut self) {
        let total_len = self.text.encode_utf16().count() as u32;
        let anchor = self.selection_anchor.min(total_len);
        let active = self.selection_active.min(total_len);
        self.selection_anchor = self.snap_to_scalar_boundary(anchor);
        self.selection_active = self.snap_to_scalar_boundary(active);
    }

    fn utf16_index_to_byte(&self, idx16: u32) -> usize {
        // Walk chars accumulating UTF-16 code units until reaching idx16
        if idx16 == 0 {
            return 0;
        }
        let mut acc16: u32 = 0;
        for (byte_idx, ch) in self.text.char_indices() {
            let ch16 = ch.encode_utf16(&mut [0u16; 2]).len() as u32;
            if acc16 >= idx16 {
                return byte_idx;
            }
            acc16 += ch16;
            if acc16 >= idx16 {
                // Return boundary after this char
                return byte_idx + ch.len_utf8();
            }
        }
        self.text.len()
    }

    fn prev_word_index(&self, idx16: u32) -> u32 {
        // Move to the start of the current word if inside it; if at a word start,
        // move to the start of the previous word. If before the first word, return 0.
        let mut prev = 0u32;
        for &s in &self.word_starts_utf16 {
            if s >= idx16 {
                return prev;
            }
            prev = s;
        }
        prev
    }

    fn next_word_index(&self, idx16: u32) -> u32 {
        // Prefer the end of the current word; if in whitespace, jump to end of the next word.
        let ranges = &self.word_ranges_utf16;
        for (i, (start, end)) in ranges.iter().cloned().enumerate() {
            if idx16 < end && idx16 >= start {
                return end; // inside current word: go to its end (before trailing whitespace)
            }
            if idx16 < start {
                return end; // in whitespace before this word: go to its end
            }
            if idx16 == end {
                // exactly at end of a word: go to end of next word if any
                if let Some((_, next_end)) = ranges.get(i + 1).cloned() {
                    return next_end;
                }
            }
        }
        // No next movement; clamp to end
        self.text.encode_utf16().count() as u32
    }

    fn prev_scalar_index(&self, idx16: u32) -> u32 {
        let mut prev = 0u32;
        for &b in &self.utf16_boundaries {
            if b >= idx16 {
                return prev;
            }
            prev = b;
        }
        prev
    }

    fn next_scalar_index(&self, idx16: u32) -> u32 {
        for &b in &self.utf16_boundaries {
            if b > idx16 {
                return b;
            }
        }
        // Already at or beyond end
        self.text.encode_utf16().count() as u32
    }

    fn is_scalar_boundary(&self, idx16: u32) -> bool {
        self.utf16_boundaries.contains(&idx16)
    }

    fn snap_to_scalar_boundary(&self, idx16: u32) -> u32 {
        if self.is_scalar_boundary(idx16) {
            idx16
        } else {
            self.prev_scalar_index(idx16)
        }
    }

    fn recalc_metrics(&mut self) -> Result<()> {
        unsafe {
            let mut metrics = DWRITE_TEXT_METRICS::default();
            self.layout
                .as_ref()
                .expect("layout not built")
                .GetMetrics(&mut metrics)?;
            self.metric_bounds = RectDIP {
                x: metrics.left,
                y: metrics.top,
                width: metrics.width,
                height: metrics.height,
            };
            Ok(())
        }
    }

    /// Rebuild text layout and recalculate metrics in one operation
    fn rebuild_layout_and_metrics(&mut self) -> Result<()> {
        self.build_text_layout()?;
        self.recalc_metrics()
    }

    fn selection_range(&self) -> (u32, u32) {
        (
            self.selection_anchor.min(self.selection_active),
            self.selection_anchor.max(self.selection_active),
        )
    }

    pub fn select_all(&mut self) {
        self.clear_sticky_x();
        let len16 = self.text.encode_utf16().count() as u32;
        self.selection_anchor = 0;
        self.selection_active = len16;
    }

    /// Return the current selected text, if any.
    pub fn selected_text(&self) -> Option<&str> {
        let (start16, end16) = self.selection_range();
        if start16 == end16 {
            return None;
        }
        let start_byte = self.utf16_index_to_byte(start16);
        let end_byte = self.utf16_index_to_byte(end16);
        Some(&self.text[start_byte..end_byte])
    }

    pub fn insert_str(&mut self, s: &str) -> Result<()> {
        let (start16, end16) = self.selection_range();
        if s.is_empty() && start16 == end16 {
            return Ok(()); // nothing to do
        }

        // Save undo state before modification
        let operation_type = if s.len() == 1 && !s.chars().any(|c| c.is_whitespace()) {
            UndoOperationType::CharacterInsertion
        } else {
            UndoOperationType::Other
        };
        self.save_undo_state(operation_type);
        let start_byte = self.utf16_index_to_byte(start16);
        let end_byte = self.utf16_index_to_byte(end16);
        self.text.replace_range(start_byte..end_byte, s);
        self.recompute_text_boundaries();
        if s.is_empty() {
            // Deletion: caret at start of removed range
            self.selection_anchor = start16;
            self.selection_active = start16;
        } else {
            // Insertion: caret after inserted text
            let ins16 = Self::utf16_len_of_str(s);
            self.selection_anchor = start16 + ins16;
            self.selection_active = self.selection_anchor;
        }
        self.build_text_layout()?;
        self.recalc_metrics()?;

        self.force_blink();
        Ok(())
    }

    // pub fn insert_str_with_callback(
    //     &mut self,
    //     s: &str,
    //     callback: Option<&Box<dyn Fn(&str)>>,
    // ) -> Result<()> {
    //     self.insert_str(s)?;
    //     if let Some(cb) = callback {
    //         cb(&self.text);
    //     }
    //     Ok(())
    // }

    pub fn backspace(&mut self) -> Result<()> {
        let (start16, end16) = self.selection_range();
        if start16 != end16 {
            self.clear_sticky_x();
            self.force_blink();
            return self.insert_str("");
        }
        if start16 == 0 {
            return Ok(());
        }

        // Save undo state before modification
        self.save_undo_state(UndoOperationType::CharacterDeletion);
        // Delete previous Unicode scalar
        let prev16 = self.prev_scalar_index(start16);
        let prev_byte = self.utf16_index_to_byte(prev16);
        let caret_byte = self.utf16_index_to_byte(start16);
        self.text.replace_range(prev_byte..caret_byte, "");
        self.recompute_text_boundaries();
        self.selection_anchor = prev16;
        self.selection_active = prev16;
        self.build_text_layout()?;
        self.recalc_metrics()?;
        self.clear_sticky_x();

        self.force_blink();
        Ok(())
    }

    pub fn delete_forward(&mut self) -> Result<()> {
        let (start16, end16) = self.selection_range();
        if start16 != end16 {
            self.force_blink();
            return self.insert_str("");
        }
        let total16 = self.text.encode_utf16().count() as u32;
        if start16 >= total16 {
            return Ok(());
        }

        // Save undo state before modification
        self.save_undo_state(UndoOperationType::CharacterDeletion);
        // Delete next Unicode scalar
        let next16 = self.next_scalar_index(start16);
        let caret_byte = self.utf16_index_to_byte(start16);
        let next_byte = self.utf16_index_to_byte(next16);
        self.text.replace_range(caret_byte..next_byte, "");
        self.recompute_text_boundaries();
        // Caret stays at start16
        self.rebuild_layout_and_metrics()?;
        self.clear_sticky_x();
        self.force_blink();
        Ok(())
    }

    /// Delete the word to the left of the caret when no selection, or delete the selection.
    pub fn backspace_word(&mut self) -> Result<()> {
        let (start16, end16) = self.selection_range();
        if start16 != end16 {
            self.clear_sticky_x();
            self.force_blink();
            return self.insert_str("");
        }
        if start16 == 0 {
            return Ok(());
        }

        // Save undo state before modification
        self.save_undo_state(UndoOperationType::WordDeletion);
        let prev16 = self.prev_word_index(start16);
        let prev_byte = self.utf16_index_to_byte(prev16);
        let caret_byte = self.utf16_index_to_byte(start16);
        self.text.replace_range(prev_byte..caret_byte, "");
        self.recompute_text_boundaries();
        self.selection_anchor = prev16;
        self.selection_active = prev16;
        self.build_text_layout()?;
        self.recalc_metrics()?;
        self.clear_sticky_x();

        self.force_blink();
        Ok(())
    }

    /// Delete the word to the right of the caret when no selection, or delete the selection.
    pub fn delete_word_forward(&mut self) -> Result<()> {
        let (start16, end16) = self.selection_range();
        if start16 != end16 {
            self.clear_sticky_x();
            self.force_blink();
            return self.insert_str("");
        }
        let total16 = self.text.encode_utf16().count() as u32;
        if start16 >= total16 {
            return Ok(());
        }

        // Save undo state before modification
        self.save_undo_state(UndoOperationType::WordDeletion);
        let next16 = self.next_word_index(start16);
        let caret_byte = self.utf16_index_to_byte(start16);
        let next_byte = self.utf16_index_to_byte(next16);
        self.text.replace_range(caret_byte..next_byte, "");
        self.recompute_text_boundaries();
        // Caret stays at start16
        self.build_text_layout()?;
        self.recalc_metrics()?;
        self.clear_sticky_x();

        self.force_blink();
        Ok(())
    }

    fn move_to_target(&mut self, target: u32, extend: bool) {
        // Don't blink if we're already at the target
        if self.selection_active == target && (extend || self.selection_anchor == target) {
            return;
        }

        self.selection_active = target;
        if !extend {
            self.selection_anchor = target;
        }
        self.clamp_sel_to_len();

        self.force_blink();
    }

    pub fn move_left(&mut self, extend: bool) {
        self.clear_sticky_x();
        let (start16, end16) = self.selection_range();
        let target = if !extend && start16 != end16 {
            start16
        } else {
            self.prev_scalar_index(self.selection_active)
        };

        self.move_to_target(target, extend);
    }

    pub fn move_right(&mut self, extend: bool) {
        // Horizontal movement resets sticky X
        self.clear_sticky_x();
        let (start16, end16) = self.selection_range();
        let target = if !extend && start16 != end16 {
            end16
        } else {
            self.next_scalar_index(self.selection_active)
        };

        self.move_to_target(target, extend);
    }

    pub fn move_up(&mut self, extend: bool) {
        // If there is an active selection and we're not extending, collapse to start first
        let (start16, end16) = self.selection_range();
        let base = if !extend && start16 != end16 {
            start16
        } else {
            self.selection_active
        };

        if let Ok((curr_x, top, _h)) = self.caret_pos_dip(base) {
            // Initialize sticky X on first vertical move; otherwise use existing
            let desired_x = if let Some(sx) = self.sticky_x_dip {
                sx
            } else {
                self.sticky_x_dip = Some(curr_x);
                curr_x
            };
            let target_y = top - LINE_OFFSET; // just above current line
            if let Ok(idx) = self.hit_test_index(desired_x, target_y) {
                let idx = self.snap_to_scalar_boundary(idx);
                self.move_to_target(idx, extend);
            }
        }
    }

    pub fn move_down(&mut self, extend: bool) {
        // If there is an active selection and we're not extending, collapse to end first
        let (start16, end16) = self.selection_range();
        let base = if !extend && start16 != end16 {
            end16
        } else {
            self.selection_active
        };

        if let Ok((curr_x, top, h)) = self.caret_pos_dip(base) {
            // Initialize sticky X on first vertical move; otherwise use existing
            let desired_x = if let Some(sx) = self.sticky_x_dip {
                sx
            } else {
                self.sticky_x_dip = Some(curr_x);
                curr_x
            };
            let target_y = top + h + LINE_OFFSET; // just below current line
            if let Ok(idx) = self.hit_test_index(desired_x, target_y) {
                let idx = self.snap_to_scalar_boundary(idx);
                self.move_to_target(idx, extend);
            }
        }
    }

    pub fn move_word_left(&mut self, extend: bool) {
        self.clear_sticky_x();
        let (start16, end16) = self.selection_range();
        let target = if !extend && start16 != end16 {
            start16
        } else {
            self.prev_word_index(self.selection_active)
        };

        self.move_to_target(target, extend);
    }

    pub fn move_word_right(&mut self, extend: bool) {
        self.clear_sticky_x();
        let (start16, end16) = self.selection_range();
        let target = if !extend && start16 != end16 {
            end16
        } else {
            self.next_word_index(self.selection_active)
        };

        self.move_to_target(target, extend);
    }

    pub fn move_to_start(&mut self, extend: bool) {
        self.clear_sticky_x();
        self.move_to_target(0, extend);
    }

    pub fn move_to_end(&mut self, extend: bool) {
        self.clear_sticky_x();
        let total16 = self.text.encode_utf16().count() as u32;
        self.move_to_target(total16, extend);
    }

    // ===== Undo/Redo system =====

    /// Save the current state to the undo stack before making modifications
    fn save_undo_state(&mut self, operation_type: UndoOperationType) {
        let current_time = self.created_at.elapsed().as_millis();

        // Check if we can merge with the previous operation
        if let Some(last_state) = self.undo_stack.last_mut() {
            let time_diff = current_time - last_state.timestamp;

            // Merge successive character insertions/deletions within time window
            if time_diff < UNDO_MERGE_TIME_MS
                && last_state.operation_type == operation_type
                && (operation_type == UndoOperationType::CharacterInsertion
                    || operation_type == UndoOperationType::CharacterDeletion)
            {
                // Don't create a new undo state, the previous one will be updated
                // when the actual operation completes
                last_state.timestamp = current_time;
                return;
            }
        }

        let state = UndoState {
            text: self.text.clone(),
            selection_anchor: self.selection_anchor,
            selection_active: self.selection_active,
            timestamp: current_time,
            operation_type,
        };

        self.undo_stack.push(state);

        // Limit undo stack size
        if self.undo_stack.len() > MAX_UNDO_LEVELS {
            self.undo_stack.remove(0);
        }

        // Clear redo stack when new action is performed
        self.redo_stack.clear();
    }

    /// Undo the last action
    pub fn undo(&mut self) -> Result<bool> {
        if let Some(undo_state) = self.undo_stack.pop() {
            // Save current state to redo stack
            let current_state = UndoState {
                text: self.text.clone(),
                selection_anchor: self.selection_anchor,
                selection_active: self.selection_active,
                timestamp: self.created_at.elapsed().as_millis(),
                operation_type: UndoOperationType::Other,
            };
            self.redo_stack.push(current_state);

            // Restore the undo state
            self.text = undo_state.text;
            self.selection_anchor = undo_state.selection_anchor;
            self.selection_active = undo_state.selection_active;

            // Update internal state
            self.recompute_text_boundaries();
            self.build_text_layout()?;
            self.recalc_metrics()?;
            self.force_blink();

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Redo the last undone action
    pub fn redo(&mut self) -> Result<bool> {
        if let Some(redo_state) = self.redo_stack.pop() {
            // Save current state to undo stack
            let current_state = UndoState {
                text: self.text.clone(),
                selection_anchor: self.selection_anchor,
                selection_active: self.selection_active,
                timestamp: self.created_at.elapsed().as_millis(),
                operation_type: UndoOperationType::Other,
            };
            self.undo_stack.push(current_state);

            // Restore the redo state
            self.text = redo_state.text;
            self.selection_anchor = redo_state.selection_anchor;
            self.selection_active = redo_state.selection_active;

            // Update internal state
            self.recompute_text_boundaries();
            self.build_text_layout()?;
            self.recalc_metrics()?;
            self.force_blink();

            Ok(true)
        } else {
            Ok(false)
        }
    }
}
