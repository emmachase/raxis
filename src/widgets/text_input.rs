use crate::dragdrop::start_text_drag;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_HIT_TEST_METRICS, DWRITE_LINE_METRICS, DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE,
    IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::System::Ole::{DROPEFFECT_COPY, DROPEFFECT_MOVE, DROPEFFECT_NONE};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_A, VK_BACK, VK_C, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_LEFT, VK_RIGHT, VK_UP,
    VK_V, VK_X,
};
use windows::Win32::UI::WindowsAndMessaging::STRSAFE_E_INSUFFICIENT_BUFFER;
use windows::core::Result;
use windows_numerics::Vector2;

use crate::clipboard::{get_clipboard_text, set_clipboard_text};
use crate::gfx::{PointDIP, RectDIP};
use crate::layout::model::UIKey;
use crate::widgets::{DragData, DragDropWidget, DragInfo, DropResult, Renderer, Widget};
use crate::{InputMethod, Shell};
use unicode_segmentation::UnicodeSegmentation;

const BLINK_TIME: f64 = 0.5;

/// A widget that renders selectable text using DirectWrite and draws
/// the selection highlight using Direct2D.
///
/// It encapsulates selection state, hit-testing, and cached layout bounds
/// for cursor hit-testing.
#[derive(Debug, Clone)]
pub struct TextInput {
    // DirectWrite objects (shared/cloneable COM interfaces)
    dwrite_factory: IDWriteFactory,
    text_format: IDWriteTextFormat,
    text: String,

    // layout
    bounds: RectDIP,
    layout: Option<IDWriteTextLayout>,

    // Selection state (UTF-16 code unit indices)
    selection_anchor: u32,
    selection_active: u32,
    is_dragging: bool,
    has_started_ole_drag: bool,
    drag_start_position: Option<PointDIP>,
    caret_blink_timer: f64,
    caret_visible: bool,

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionMode {
    Char,
    Word,
    Paragraph,
}

impl Widget for TextInput {
    fn limits(&self, available: super::Limits) -> super::Limits {
        if let Some(layout) = &self.layout {
            unsafe {
                layout.SetMaxWidth(available.max.width).unwrap();
                layout.SetMaxHeight(available.max.height).unwrap();
            }

            let mut max_metrics = DWRITE_TEXT_METRICS::default();
            unsafe { layout.GetMetrics(&mut max_metrics).unwrap() };

            let min_width = unsafe { layout.DetermineMinWidth().unwrap() };

            super::Limits {
                min: super::Size {
                    width: min_width,
                    height: max_metrics.height,
                },
                max: super::Size {
                    width: max_metrics.widthIncludingTrailingWhitespace,
                    height: max_metrics.height,
                },
            }
        } else {
            super::Limits {
                min: available.min,
                max: available.min,
            }
        }
    }

    // fn operate(&mut self, id: Option<u64>, key: UIKey, operation: &dyn super::Operation) {
    //     operation.focusable(self, id, key);
    // }

    fn update(
        &mut self,
        id: Option<u64>,
        ui_key: UIKey,
        hwnd: HWND,
        shell: &mut Shell,
        event: &super::Event,
        bounds: RectDIP,
    ) {
        let RectDIP { x_dip, y_dip, .. } = bounds;
        match event {
            super::Event::MouseButtonDown {
                x, y, click_count, ..
            } => {
                // Complete composition before altering selection
                if let Some(ref ime_text) = self.ime_text {
                    let _ = self.ime_commit(ime_text.clone());
                    self.ime_end();
                    shell.request_input_method(hwnd, InputMethod::Disabled);
                }

                if (PointDIP {
                    x_dip: *x,
                    y_dip: *y,
                })
                .within(bounds)
                {
                    if let Ok(idx) = self.hit_test_index(x - x_dip, y - y_dip) {
                        shell.focus_manager.focus(id, ui_key);

                        // Store the drag start position for OLE drag detection
                        self.drag_start_position = Some(PointDIP {
                            x_dip: x - x_dip,
                            y_dip: y - y_dip,
                        });

                        // Selection mode by click count
                        let mode = match click_count {
                            1 => SelectionMode::Char,
                            x if x % 2 == 0 => SelectionMode::Word,
                            _ => SelectionMode::Paragraph,
                        };
                        self.set_selection_mode(mode);
                        self.begin_drag(idx);
                    }
                } else {
                    shell.focus_manager.release_focus(id, ui_key);
                }
            }
            super::Event::MouseButtonUp { x, y, .. } => {
                if let Ok(idx) = self.hit_test_index(x - x_dip, y - y_dip) {
                    self.end_drag(idx);
                } else {
                    self.end_drag(0);
                }

                // Reset OLE drag state
                self.has_started_ole_drag = false;
                self.drag_start_position = None;
            }
            super::Event::MouseMove { x, y } => {
                let widget_x = x - x_dip;
                let widget_y = y - y_dip;

                // Check if we should start an OLE drag operation
                if self.is_dragging && self.can_drag_drop && !self.has_started_ole_drag {
                    // Check if we have selected text and the drag started within the selection
                    if let Some(start_pos) = self.drag_start_position {
                        if let Some(drag_data) = self.can_drag(start_pos) {
                            // Start OLE drag operation
                            self.drag_start(&drag_data);

                            return; // Don't update text selection when starting OLE drag
                        }
                    }
                }

                if self.update_drag(widget_x, widget_y) {
                    let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
                }
            }
            super::Event::KeyDown { key, modifiers, .. } => {
                if shell.focus_manager.is_focused(id, ui_key) {
                    let shift_down = modifiers.shift;
                    let ctrl_down = modifiers.ctrl;
                    let _handled = match *key {
                        x if x == VK_LEFT.0 as u32 => {
                            if ctrl_down {
                                self.move_word_left(shift_down);
                            } else {
                                self.move_left(shift_down);
                            }
                            true
                        }
                        x if x == VK_RIGHT.0 as u32 => {
                            if ctrl_down {
                                self.move_word_right(shift_down);
                            } else {
                                self.move_right(shift_down);
                            }
                            true
                        }
                        x if x == VK_UP.0 as u32 => {
                            self.move_up(shift_down);
                            true
                        }
                        x if x == VK_DOWN.0 as u32 => {
                            self.move_down(shift_down);
                            true
                        }
                        x if x == VK_HOME.0 as u32 => {
                            self.move_to_start(shift_down);
                            true
                        }
                        x if x == VK_END.0 as u32 => {
                            self.move_to_end(shift_down);
                            true
                        }
                        x if x == VK_BACK.0 as u32 => {
                            if ctrl_down {
                                let _ = self.backspace_word();
                            } else {
                                let _ = self.backspace();
                            }
                            true
                        }
                        x if x == VK_DELETE.0 as u32 => {
                            if ctrl_down {
                                let _ = self.delete_word_forward();
                            } else {
                                let _ = self.delete_forward();
                            }
                            true
                        }
                        x if x == VK_A.0 as u32 && ctrl_down => {
                            self.select_all();
                            true
                        }
                        x if x == VK_C.0 as u32 && ctrl_down => {
                            if let Some(s) = self.selected_text() {
                                let _ = set_clipboard_text(hwnd, &s);
                            }
                            true
                        }
                        x if x == VK_X.0 as u32 && ctrl_down => {
                            if let Some(s) = self.selected_text() {
                                let _ = set_clipboard_text(hwnd, &s);
                                let _ = self.insert_str("");
                            }
                            true
                        }
                        x if x == VK_V.0 as u32 && ctrl_down => {
                            if !self.is_composing() {
                                if let Some(s) = get_clipboard_text(hwnd) {
                                    let _ = self.insert_str(&s);
                                }
                            }
                            true
                        }
                        x if x == VK_ESCAPE.0 as u32 => {
                            if self.has_selection() {
                                self.clear_selection();
                            } else {
                                shell.focus_manager.release_focus(id, ui_key);
                            }
                            true
                        }
                        _ => false,
                    };
                }
            }
            super::Event::KeyUp { .. } => {}
            super::Event::Char { text } => {
                if shell.focus_manager.is_focused(id, ui_key) {
                    let _ = self.insert_str(text.as_str());
                }
            }
            super::Event::ImeStartComposition => {
                if shell.focus_manager.is_focused(id, ui_key) {
                    self.ime_begin();

                    if let Ok((c_x_dip, c_y_dip, h)) = self.caret_pos_dip(self.caret_active16()) {
                        shell.request_input_method(
                            hwnd,
                            InputMethod::Enabled {
                                position: PointDIP {
                                    x_dip: x_dip + c_x_dip,
                                    y_dip: y_dip + c_y_dip + h,
                                },
                            },
                        );
                    }
                }
            }
            super::Event::ImeComposition { text, caret_units } => {
                if shell.focus_manager.is_focused(id, ui_key) {
                    self.ime_update(text.clone(), *caret_units);

                    if let Ok((c_x_dip, c_y_dip, h)) = self.caret_pos_dip(self.caret_active16()) {
                        shell.request_input_method(
                            hwnd,
                            InputMethod::Enabled {
                                position: PointDIP {
                                    x_dip: x_dip + c_x_dip,
                                    y_dip: y_dip + c_y_dip + h,
                                },
                            },
                        );
                    }
                }
            }
            super::Event::ImeCommit { text } => {
                if shell.focus_manager.is_focused(id, ui_key) {
                    self.ime_commit(text.clone()).expect("ime commit failed");
                }
            }
            super::Event::ImeEndComposition => {
                if shell.focus_manager.is_focused(id, ui_key) {
                    self.ime_end();
                }
            }

            _ => {
                // Unhandled event
            }
        }
    }

    fn paint(
        &mut self,
        id: Option<u64>,
        ui_key: UIKey,
        shell: &Shell,
        renderer: &Renderer,
        bounds: RectDIP,
        dt: f64,
    ) {
        self.update_bounds(bounds).expect("update bounds failed");

        self.draw(
            id,
            ui_key,
            shell,
            renderer.render_target,
            renderer.brush,
            bounds,
            dt,
        )
        .expect("draw failed");
    }

    fn cursor(
        &self,
        _id: Option<u64>,
        _key: UIKey,
        point: PointDIP,
        bounds: RectDIP,
    ) -> Option<super::Cursor> {
        if point.within(bounds) {
            Some(super::Cursor::IBeam)
        } else {
            None
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl DragDropWidget for TextInput {
    fn drag_enter(
        &mut self,
        drag_info: &DragInfo,
        widget_bounds: RectDIP,
    ) -> windows::Win32::System::Ole::DROPEFFECT {
        use windows::Win32::System::Ole::{DROPEFFECT_COPY, DROPEFFECT_MOVE, DROPEFFECT_NONE};

        match &drag_info.data {
            DragData::Text(_) => {
                // Convert global position to widget-relative position
                let widget_x = drag_info.position.x_dip - widget_bounds.x_dip;
                let widget_y = drag_info.position.y_dip - widget_bounds.y_dip;

                if let Ok(idx) = self.hit_test_index(widget_x, widget_y) {
                    self.set_ole_drop_preview(Some(idx));

                    // Choose effect based on allowed effects
                    if (drag_info.allowed_effects.0 & DROPEFFECT_MOVE.0) != 0 {
                        DROPEFFECT_MOVE
                    } else if (drag_info.allowed_effects.0 & DROPEFFECT_COPY.0) != 0 {
                        DROPEFFECT_COPY
                    } else {
                        DROPEFFECT_NONE
                    }
                } else {
                    DROPEFFECT_NONE
                }
            }
        }
    }

    fn drag_over(
        &mut self,
        drag_info: &DragInfo,
        widget_bounds: RectDIP,
    ) -> windows::Win32::System::Ole::DROPEFFECT {
        match &drag_info.data {
            DragData::Text(_) => {
                // Convert client coordinates to widget-relative coordinates
                let widget_pos = PointDIP {
                    x_dip: drag_info.position.x_dip - widget_bounds.x_dip,
                    y_dip: drag_info.position.y_dip - widget_bounds.y_dip,
                };

                if let Ok(idx16) = self.hit_test_index(widget_pos.x_dip, widget_pos.y_dip) {
                    self.set_ole_drop_preview(Some(idx16));
                }

                // Return the appropriate effect based on what's allowed
                if (drag_info.allowed_effects.0 & DROPEFFECT_MOVE.0) != 0 {
                    DROPEFFECT_MOVE
                } else if (drag_info.allowed_effects.0 & DROPEFFECT_COPY.0) != 0 {
                    DROPEFFECT_COPY
                } else {
                    DROPEFFECT_NONE
                }
            }
        }
    }

    fn drag_leave(&mut self, _widget_bounds: RectDIP) {
        self.set_ole_drop_preview(None);
    }

    fn drop(
        &mut self,
        id: Option<u64>,
        key: UIKey,
        shell: &mut Shell,
        drag_info: &DragInfo,
        widget_bounds: RectDIP,
    ) -> DropResult {
        match &drag_info.data {
            DragData::Text(text) => {
                // Convert client coordinates to widget-relative coordinates
                let widget_pos = PointDIP {
                    x_dip: drag_info.position.x_dip - widget_bounds.x_dip,
                    y_dip: drag_info.position.y_dip - widget_bounds.y_dip,
                };

                if let Ok(drop_idx) = self.hit_test_index(widget_pos.x_dip, widget_pos.y_dip) {
                    let effect = if (drag_info.allowed_effects.0 & DROPEFFECT_MOVE.0) != 0 {
                        DROPEFFECT_MOVE
                    } else if (drag_info.allowed_effects.0 & DROPEFFECT_COPY.0) != 0 {
                        DROPEFFECT_COPY
                    } else {
                        DROPEFFECT_NONE
                    };

                    // Check if this is a drop onto the same widget that initiated the drag
                    let is_same_widget = self.has_started_ole_drag;

                    // Check if we're dropping over an existing selection
                    let (current_sel_start, current_sel_end) = self.selection_range();
                    let dropping_over_selection = current_sel_start != current_sel_end
                        && drop_idx >= current_sel_start
                        && drop_idx <= current_sel_end;

                    if is_same_widget && effect == DROPEFFECT_MOVE {
                        // Handle move within the same widget
                        let (drag_sel_start, drag_sel_end) = self.selection_range();

                        // Don't allow dropping within the dragged selection (but allow over different selections)
                        if !dropping_over_selection
                            && drop_idx >= drag_sel_start
                            && drop_idx <= drag_sel_end
                        {
                            self.set_ole_drop_preview(None);
                            return DropResult {
                                effect: DROPEFFECT_NONE,
                                handled: false,
                            };
                        }

                        if dropping_over_selection {
                            // Replace the existing selection with the dragged text
                            self.insert_str(text).unwrap();
                        } else {
                            // Adjust drop position if it's after the dragged selection
                            let adjusted_drop_idx = if drop_idx > drag_sel_end {
                                drop_idx - (drag_sel_end - drag_sel_start)
                            } else {
                                drop_idx
                            };

                            // Remove the dragged text first
                            self.insert_str("").unwrap();

                            // Insert at the adjusted position
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
                    self.set_ole_drop_preview(None);
                    shell.focus_manager.focus(id, key);

                    DropResult {
                        effect,
                        handled: true,
                    }
                } else {
                    DropResult::default()
                }
            }
        }
    }

    fn can_drag(&self, position: PointDIP) -> Option<DragData> {
        // Check if we have selected text and the position is within the selection
        let (sel_start, sel_end) = self.selection_range();
        if sel_start != sel_end {
            if let Ok(idx) = self.hit_test_index(position.x_dip, position.y_dip) {
                if idx >= sel_start && idx <= sel_end {
                    if let Some(selected_text) = self.selected_text() {
                        return Some(DragData::Text(selected_text));
                    }
                }
            }
        }
        None
    }

    fn drag_start(&mut self, data: &DragData) {
        // Mark that we can perform drag-to-move
        self.set_can_drag_drop(true);

        // Start OLE drag operation with the selected text
        let DragData::Text(text) = data;
        self.has_started_ole_drag = true;
        if let Ok(effect) = start_text_drag(text, true) {
            self.drag_end(data, effect);
        }
    }

    fn drag_end(&mut self, _data: &DragData, effect: windows::Win32::System::Ole::DROPEFFECT) {
        use windows::Win32::System::Ole::DROPEFFECT_MOVE;

        // If it was a move operation, delete the selected text
        if (effect.0 & DROPEFFECT_MOVE.0) != 0 && self.can_drag_drop() {
            let _ = self.insert_str("");
        }

        // Clear drop preview in case drag was cancelled
        self.set_ole_drop_preview(None);

        self.set_can_drag_drop(false);
        self.has_started_ole_drag = false;
        self.drag_start_position = None;
        self.end_drag(0);
    }
}

impl TextInput {
    pub fn new(
        dwrite_factory: IDWriteFactory,
        text_format: IDWriteTextFormat,
        text: String,
    ) -> Self {
        let mut s = Self {
            dwrite_factory,
            text_format,
            text,
            bounds: RectDIP::default(),
            layout: None,
            selection_anchor: 0,
            selection_active: 0,
            is_dragging: false,
            has_started_ole_drag: false,
            drag_start_position: None,
            caret_blink_timer: 0.0,
            caret_visible: true,
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
        };
        s.recompute_text_boundaries();
        s
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
                    self.bounds.width_dip,
                    self.bounds.height_dip,
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
                    self.bounds.width_dip,
                    self.bounds.height_dip,
                )?
            };

            self.layout = Some(layout);
            Ok(())
        }
    }

    pub fn update_bounds(&mut self, bounds: RectDIP) -> Result<()> {
        if bounds != self.bounds {
            self.bounds = bounds;
            self.build_text_layout()?;

            unsafe {
                let mut metrics = DWRITE_TEXT_METRICS::default();
                self.layout
                    .as_ref()
                    .expect("layout not built")
                    .GetMetrics(&mut metrics)?;
                self.metric_bounds = RectDIP {
                    x_dip: metrics.left,
                    y_dip: metrics.top,
                    width_dip: metrics.width,
                    height_dip: metrics.height,
                };
            }
        }

        Ok(())
    }

    /// Draw selection highlight behind the text for the currently selected range.
    /// Restores the brush color to black afterwards to match typical text color.
    fn draw_selection(
        &self,
        layout: &IDWriteTextLayout,
        rt: &ID2D1HwndRenderTarget,
        bounds: RectDIP,
        brush: &ID2D1SolidColorBrush,
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
                            brush.SetColor(&D2D1_COLOR_F {
                                r: 0.2,
                                g: 0.4,
                                b: 1.0,
                                a: 0.35,
                            });
                            for m in runs.iter().take(actual as usize) {
                                let rect = D2D_RECT_F {
                                    left: bounds.x_dip + m.left,
                                    top: bounds.y_dip + m.top,
                                    right: bounds.x_dip + m.left + m.width,
                                    bottom: bounds.y_dip + m.top + m.height,
                                };
                                rt.FillRectangle(&rect, brush);
                            }
                            // Restore brush to black for drawing text
                            brush.SetColor(&D2D1_COLOR_F {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 1.0,
                            });
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
        id: Option<u64>,
        ui_key: UIKey,
        shell: &Shell,
        rt: &ID2D1HwndRenderTarget,
        brush: &ID2D1SolidColorBrush,
        bounds: RectDIP,
        dt: f64,
    ) -> Result<()> {
        unsafe {
            let layout = self.layout.as_ref().expect("layout not built");

            // {
            //     layout.SetMaxWidth(bounds.width_dip).unwrap();
            //     layout.SetMaxHeight(bounds.height_dip).unwrap();
            // }

            self.caret_blink_timer += dt;
            if self.caret_blink_timer >= BLINK_TIME {
                self.caret_blink_timer = 0.0;
                self.caret_visible = !self.caret_visible;
            }

            // Normal rendering: selection, base text, caret
            self.draw_selection(layout, rt, bounds, brush)?;

            brush.SetColor(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            });
            rt.DrawTextLayout(
                Vector2 {
                    X: bounds.x_dip,
                    Y: bounds.y_dip,
                },
                layout,
                brush,
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
            );

            // OLE drag-over preview caret
            if let Some(drop) = self.ole_drop_preview16 {
                let drop = self.snap_to_scalar_boundary(drop);
                let (src_start, src_end) = self.selection_range();
                if !(drop >= src_start && drop <= src_end) {
                    let mut x = 0.0f32;
                    let mut y = 0.0f32;
                    let mut m = DWRITE_HIT_TEST_METRICS::default();
                    layout.HitTestTextPosition(drop, false, &mut x, &mut y, &mut m)?;
                    let caret_rect = D2D_RECT_F {
                        left: bounds.x_dip + x,
                        top: bounds.y_dip + m.top,
                        right: bounds.x_dip + x + 1.0,
                        bottom: bounds.y_dip + m.top + m.height,
                    };
                    rt.FillRectangle(&caret_rect, brush);
                }
            } else {
                // Draw caret if there's no selection (1 DIP wide bar)
                let sel_start = self.selection_anchor.min(self.selection_active);
                let sel_end = self.selection_anchor.max(self.selection_active);
                if shell.focus_manager.is_focused(id, ui_key) && self.caret_visible {
                    if self.is_composing() {
                        // let (start16, end16) = self.selection_range();
                        let ime_caret_pos = sel_start + self.ime_cursor16;
                        let mut x = 0.0f32;
                        let mut y = 0.0f32;
                        let mut m = DWRITE_HIT_TEST_METRICS::default();
                        layout.HitTestTextPosition(ime_caret_pos, false, &mut x, &mut y, &mut m)?;
                        let caret_rect = D2D_RECT_F {
                            left: bounds.x_dip + x,
                            top: bounds.y_dip + m.top,
                            right: bounds.x_dip + x + 1.0,
                            bottom: bounds.y_dip + m.top + m.height,
                        };
                        rt.FillRectangle(&caret_rect, brush);
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
                        let caret_rect = D2D_RECT_F {
                            left: bounds.x_dip + x,
                            top: bounds.y_dip + m.top,
                            right: bounds.x_dip + x + 1.0,
                            bottom: bounds.y_dip + m.top + m.height,
                        };
                        rt.FillRectangle(&caret_rect, brush);
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
        self.caret_blink_timer = 0.0;
        self.caret_visible = true;
    }

    fn clear_sticky_x(&mut self) {
        self.sticky_x_dip = None;
    }

    // Drag/select helpers
    pub fn begin_drag(&mut self, idx: u32) {
        // Mouse interaction resets sticky X
        self.clear_sticky_x();
        let idx = self.snap_to_scalar_boundary(idx);
        self.drag_origin16 = idx;

        self.can_drag_drop = false;

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

    /// Caret DIP position during IME composition (within preedit).
    pub fn ime_caret_pos_dip(&self) -> Result<(f32, f32, f32)> {
        unsafe {
            if self.ime_text.is_none() {
                return self.caret_pos_dip(self.selection_active);
            }
            let (start16, end16) = self.selection_range();
            let base_w: Vec<u16> = self.text.encode_utf16().collect();
            let ime_w: Vec<u16> = self.ime_text.as_ref().unwrap().encode_utf16().collect();
            let mut composed = Vec::with_capacity(base_w.len() + ime_w.len());
            composed.extend_from_slice(&base_w[..start16 as usize]);
            let underline_start = composed.len() as u32;
            composed.extend_from_slice(&ime_w);
            composed.extend_from_slice(&base_w[end16 as usize..]);

            let layout = self.dwrite_factory.CreateTextLayout(
                &composed,
                &self.text_format,
                self.bounds.width_dip,
                self.bounds.height_dip,
            )?;
            let caret_idx = underline_start;
            let mut x = 0.0f32;
            let mut y = 0.0f32;
            let mut m = DWRITE_HIT_TEST_METRICS::default();
            layout.HitTestTextPosition(caret_idx, false, &mut x, &mut y, &mut m)?;
            Ok((x, m.top, m.height))
        }
    }

    /// Get the active caret position in UTF-16 code units.
    pub fn caret_active16(&self) -> u32 {
        self.selection_active
    }

    pub fn is_dragging(&self) -> bool {
        self.is_dragging
    }

    /// Abort any ongoing manual drag/drag-move without altering selection or text.
    pub fn cancel_drag(&mut self) {
        self.is_dragging = false;
        self.force_blink();
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

    pub fn metric_bounds(&self) -> RectDIP {
        self.metric_bounds
    }

    pub fn has_selection(&self) -> bool {
        self.selection_active != self.selection_anchor
    }

    pub fn clear_selection(&mut self) {
        self.selection_active = self.selection_anchor;
        self.force_blink();
    }

    /// Select the word containing or following the given UTF-16 index.
    pub fn select_word_at(&mut self, idx16: u32) {
        let (s, e) = self.word_range_at(idx16);
        self.selection_anchor = self.snap_to_scalar_boundary(s);
        self.selection_active = self.snap_to_scalar_boundary(e);
        self.force_blink();
    }

    /// Select the entire wrapped line containing the given UTF-16 index.
    /// This uses DirectWrite line metrics to honor wrapping and explicit newlines.
    pub fn select_line_at(&mut self, idx16: u32) {
        unsafe {
            let layout = match self.layout.as_ref() {
                Some(l) => l,
                None => return,
            };
            // Query required line metrics count (expected to return an error but set count)
            let mut needed: u32 = 0;
            let _ = layout.GetLineMetrics(None, &mut needed);
            if needed == 0 {
                return;
            }
            let mut lines = vec![DWRITE_LINE_METRICS::default(); needed as usize];
            let mut actual: u32 = 0;
            let _ = layout.GetLineMetrics(Some(&mut lines), &mut actual);
            if actual == 0 {
                return;
            }
            let mut pos: u32 = 0;
            for m in lines.iter().take(actual as usize) {
                let line_start = pos;
                let line_end_no_newline = pos.saturating_add(m.length);
                let line_consumed = m.length.saturating_add(m.newlineLength);
                let line_total_end = pos.saturating_add(line_consumed);
                // Consider the newline as part of hit for containment test,
                // but exclude it from the selected range.
                if idx16 < line_total_end {
                    self.selection_anchor = self.snap_to_scalar_boundary(line_start);
                    self.selection_active = self.snap_to_scalar_boundary(line_end_no_newline);
                    self.force_blink();
                    return;
                }
                pos = line_total_end;
            }
            // If not found, clamp to end
            self.selection_anchor = self.snap_to_scalar_boundary(pos);
            self.selection_active = self.snap_to_scalar_boundary(pos);
            self.force_blink();
        }
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
                x_dip: metrics.left,
                y_dip: metrics.top,
                width_dip: metrics.width,
                height_dip: metrics.height,
            };
            Ok(())
        }
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
    pub fn selected_text(&self) -> Option<String> {
        let (start16, end16) = self.selection_range();
        if start16 == end16 {
            return None;
        }
        let start_byte = self.utf16_index_to_byte(start16);
        let end_byte = self.utf16_index_to_byte(end16);
        Some(self.text[start_byte..end_byte].to_string())
    }

    pub fn finish_ole_drop(&mut self, s: &str, internal_move: bool) -> Result<()> {
        let (start16, end16) = self.selection_range();
        let start_byte = self.utf16_index_to_byte(start16);
        let end_byte = self.utf16_index_to_byte(end16);

        if let Some(drop_idx16) = self.ole_drop_preview16 {
            if drop_idx16 >= start16 && drop_idx16 <= end16 {
                // Drop inside existing selection: replace
                self.text.replace_range(start_byte..end_byte, s);

                let end16 = start16 + Self::utf16_len_of_str(s);
                self.selection_anchor = end16;
                self.selection_active = end16;
            } else {
                // Drop outside existing selection: insert
                if internal_move {
                    // Delete original selection on successful MOVE drop
                    self.text.replace_range(start_byte..end_byte, "");

                    // Adjust drop index to account for deleted text
                    let drop_idx16 = if drop_idx16 > end16 {
                        drop_idx16 - (end16 - start16)
                    } else {
                        drop_idx16
                    };
                    self.move_caret_to(drop_idx16);
                    self.insert_str(s)?;
                } else {
                    self.move_caret_to(drop_idx16);
                    self.insert_str(s)?;
                }
            }
        }

        self.build_text_layout()?;
        self.recalc_metrics()?;

        self.force_blink();
        Ok(())
    }

    pub fn insert_str(&mut self, s: &str) -> Result<()> {
        let (start16, end16) = self.selection_range();
        if s.is_empty() && start16 == end16 {
            return Ok(()); // nothing to do
        }
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
        // Delete next Unicode scalar
        let next16 = self.next_scalar_index(start16);
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
            let target_y = top - 1.0; // just above current line
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
            let target_y = top + h + 1.0; // just below current line
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
}
