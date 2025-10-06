use std::any::Any;
use std::fmt::Debug;
use std::panic::Location;
use std::time::Instant;

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_METRICS,
    IDWriteFactory6, IDWriteTextFormat3, IDWriteTextLayout,
};
use windows::core::Result;

use crate::gfx::RectDIP;
use crate::gfx::command_recorder::CommandRecorder;
use crate::layout::UIArenas;
use crate::layout::model::{Color, Element, ElementStyle};
use crate::runtime::font_manager::{
    FontAxes, FontIdentifier, FontStyle, FontWeight, FontWidth, GlobalFontManager, LineSpacing,
};
use crate::util::str::StableString;
use crate::util::unique::{combine_id, id_from_location};
use crate::widgets::{Bounds, Instance, Widget, widget};
use crate::{Shell, with_state};

/// Text alignment options
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlignment {
    Leading,  // Left-aligned
    Center,   // Center-aligned
    Trailing, // Right-aligned
}

/// Paragraph alignment options
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParagraphAlignment {
    Top,
    Center,
    Bottom,
}

/// Simple text display widget for showing read-only text
#[derive(Debug)]
pub struct Text {
    pub text: StableString,
    pub font_size: f32,
    pub line_spacing: Option<LineSpacing>,
    pub color: Option<Color>,
    pub text_alignment: TextAlignment,
    pub paragraph_alignment: ParagraphAlignment,
    pub font_id: FontIdentifier,
    pub font_axes: FontAxes,
    pub word_wrap: bool,
    pub caller: &'static Location<'static>,

    pub assisted_width: Option<f32>,
    pub assisted_id: Option<u64>,
}

impl Text {
    #[track_caller]
    pub fn new(text: impl Into<StableString>) -> Self {
        Self {
            text: text.into(),
            font_size: 14.0,
            line_spacing: None,
            color: None,
            text_alignment: TextAlignment::Leading,
            paragraph_alignment: ParagraphAlignment::Top,
            font_id: FontIdentifier::system("Segoe UI"),
            font_axes: FontAxes::default(),
            word_wrap: true,
            caller: Location::caller(),

            assisted_width: None,
            assisted_id: None,
        }
    }

    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_line_spacing(mut self, line_spacing: LineSpacing) -> Self {
        self.line_spacing = Some(line_spacing);
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
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

    pub fn with_font_family(mut self, font_id: FontIdentifier) -> Self {
        self.font_id = font_id;
        self
    }

    pub fn with_word_wrap(mut self, word_wrap: bool) -> Self {
        self.word_wrap = word_wrap;
        self
    }

    pub fn with_assisted_width(mut self, width: f32) -> Self {
        self.assisted_width = Some(width);
        self
    }

    pub fn with_assisted_id(mut self, id: u64) -> Self {
        self.assisted_id = Some(id);
        self
    }

    pub fn with_font_axes(mut self, font_axes: FontAxes) -> Self {
        self.font_axes = font_axes;
        self
    }

    pub fn with_font_weight(mut self, weight: FontWeight) -> Self {
        self.font_axes.weight = weight;
        self
    }

    pub fn with_font_style(mut self, style: FontStyle) -> Self {
        self.font_axes.style = style;
        self
    }

    pub fn with_font_width(mut self, width: FontWidth) -> Self {
        self.font_axes.width = width;
        self
    }

    pub fn bold(mut self) -> Self {
        self.font_axes.weight = FontWeight::Bold;
        self
    }

    pub fn italic(mut self) -> Self {
        self.font_axes.style = FontStyle::Italic;
        self
    }

    pub fn as_element<Message>(self) -> Element<Message> {
        let id = id_from_location(self.caller);
        Element {
            id: Some(combine_id(combine_id(id, &self.text), self.assisted_id)),
            content: widget(self),
            ..Default::default()
        }
    }
}

impl<Message> From<Text> for Element<Message> {
    fn from(text: Text) -> Element<Message> {
        text.as_element()
    }
}

impl Default for Text {
    fn default() -> Self {
        Self::new(StableString::Static("Text"))
    }
}

struct TextWidgetState {
    // DirectWrite objects for text rendering
    dwrite_factory: IDWriteFactory6,
    text_format: IDWriteTextFormat3,
    text_layout: Option<IDWriteTextLayout>,
    cached_text: String,
    cached_font_size: f32,
    cached_line_spacing: Option<LineSpacing>,
    cached_font_id: FontIdentifier,
    cached_font_axes: FontAxes,
    cached_text_alignment: TextAlignment,
    cached_paragraph_alignment: ParagraphAlignment,
    cached_word_wrap: bool,

    // Layout caching
    cached_bounds: RectDIP,
    text_metrics: Option<DWRITE_TEXT_METRICS>,
    layout_invalidated: bool,

    // Sizing cache for limits_x/limits_y
    cached_preferred_width: Option<f32>,
    cached_preferred_height_for_width: Option<(f32, f32)>, // (width, height)
}

impl TextWidgetState {
    pub fn new(
        dwrite_factory: IDWriteFactory6,
        font_id: &FontIdentifier,
        font_size: f32,
        font_axes: FontAxes,
        line_spacing: Option<LineSpacing>,
        text_alignment: TextAlignment,
        paragraph_alignment: ParagraphAlignment,
        word_wrap: bool,
    ) -> Result<Self> {
        let text_format = GlobalFontManager::create_text_format(
            font_id,
            font_size,
            font_axes,
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

            // Set word wrapping
            let word_wrapping = if word_wrap {
                windows::Win32::Graphics::DirectWrite::DWRITE_WORD_WRAPPING_WRAP
            } else {
                windows::Win32::Graphics::DirectWrite::DWRITE_WORD_WRAPPING_NO_WRAP
            };
            text_format.SetWordWrapping(word_wrapping)?;

            text_format
        };

        let mut state = Self {
            dwrite_factory,
            text_format,
            text_layout: None,
            cached_text: String::new(),
            cached_font_size: font_size,
            cached_line_spacing: line_spacing,
            cached_font_id: font_id.clone(),
            cached_font_axes: font_axes,
            cached_text_alignment: text_alignment,
            cached_paragraph_alignment: paragraph_alignment,
            cached_word_wrap: word_wrap,
            cached_bounds: RectDIP::default(),
            text_metrics: None,
            layout_invalidated: true,
            cached_preferred_width: None,
            cached_preferred_height_for_width: None,
        };

        state.build_text_layout("", RectDIP::default())?;
        Ok(state)
    }

    pub fn into_any(self) -> Box<dyn Any> {
        Box::new(self)
    }

    fn needs_text_format_rebuild(
        &self,
        font_id: &FontIdentifier,
        font_size: f32,
        font_axes: FontAxes,
        line_spacing: Option<LineSpacing>,
        text_alignment: TextAlignment,
        paragraph_alignment: ParagraphAlignment,
        word_wrap: bool,
    ) -> bool {
        self.cached_font_id != *font_id
            || self.cached_font_size != font_size
            || self.cached_font_axes != font_axes
            || self.cached_line_spacing != line_spacing
            || self.cached_text_alignment != text_alignment
            || self.cached_paragraph_alignment != paragraph_alignment
            || self.cached_word_wrap != word_wrap
    }

    fn rebuild_text_format(
        &mut self,
        font_id: &FontIdentifier,
        font_size: f32,
        font_axes: FontAxes,
        line_spacing: Option<LineSpacing>,
        text_alignment: TextAlignment,
        paragraph_alignment: ParagraphAlignment,
        word_wrap: bool,
    ) -> Result<()> {
        self.text_format = GlobalFontManager::create_text_format(
            font_id,
            font_size,
            font_axes,
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

            // Set word wrapping
            let word_wrapping = if word_wrap {
                windows::Win32::Graphics::DirectWrite::DWRITE_WORD_WRAPPING_WRAP
            } else {
                windows::Win32::Graphics::DirectWrite::DWRITE_WORD_WRAPPING_NO_WRAP
            };
            self.text_format.SetWordWrapping(word_wrapping)?;
        }

        // Update cached values
        self.cached_font_id = font_id.clone();
        self.cached_font_size = font_size;
        self.cached_font_axes = font_axes;
        self.cached_line_spacing = line_spacing;
        self.cached_text_alignment = text_alignment;
        self.cached_paragraph_alignment = paragraph_alignment;
        self.cached_word_wrap = word_wrap;

        // Invalidate layout and sizing cache since format changed
        self.layout_invalidated = true;
        self.invalidate_sizing_cache();

        Ok(())
    }

    fn build_text_layout(&mut self, text: &str, bounds: RectDIP) -> Result<()> {
        // Check if we need to rebuild the text layout (text or format changed)
        let text_changed = text != self.cached_text;
        let needs_layout_rebuild = text_changed || self.layout_invalidated;

        if needs_layout_rebuild {
            unsafe {
                let wtext: Vec<u16> = text.encode_utf16().collect();
                self.text_layout = Some(self.dwrite_factory.CreateTextLayout(
                    &wtext,
                    &self.text_format,
                    bounds.width.max(1.0),  // Ensure minimum width
                    bounds.height.max(1.0), // Ensure minimum height
                )?);
            }
            self.cached_text = text.to_string();
            self.layout_invalidated = false;
            self.invalidate_sizing_cache();
        }

        // Check if we need to update bounds (cheaper operation)
        let bounds_changed = bounds != self.cached_bounds;
        if bounds_changed || needs_layout_rebuild {
            if let Some(layout) = &self.text_layout {
                unsafe {
                    layout.SetMaxWidth(bounds.width.max(1.0))?;
                    layout.SetMaxHeight(bounds.height.max(1.0))?;

                    // Get text metrics for sizing calculations
                    let mut metrics = DWRITE_TEXT_METRICS::default();
                    layout.GetMetrics(&mut metrics)?;
                    self.text_metrics = Some(metrics);
                }
            }
            self.cached_bounds = bounds;
        }

        Ok(())
    }

    fn get_preferred_size(&self) -> (f32, f32) {
        if let Some(metrics) = &self.text_metrics {
            (metrics.width, metrics.height)
        } else {
            (0.0, self.cached_font_size * 1.2) // Fallback height based on font size
        }
    }

    fn invalidate_sizing_cache(&mut self) {
        self.cached_preferred_width = None;
        self.cached_preferred_height_for_width = None;
    }

    fn get_preferred_width(&mut self, text: &str) -> Result<f32> {
        if let Some(width) = self.cached_preferred_width
            && text == self.cached_text
        {
            return Ok(width);
        }

        let temp_bounds = RectDIP {
            x: 0.0,
            y: 0.0,
            width: f32::INFINITY,
            height: f32::INFINITY,
        };

        self.build_text_layout(text, temp_bounds)?;
        let (preferred_width, _) = self.get_preferred_size();
        self.cached_preferred_width = Some(preferred_width);
        Ok(preferred_width)
    }

    fn get_preferred_height_for_width(&mut self, text: &str, width: f32) -> Result<f32> {
        if let Some((cached_width, cached_height)) = self.cached_preferred_height_for_width
            && text == self.cached_text
            && (cached_width - width).abs() < 0.0001
        {
            return Ok(cached_height);
        }

        let temp_bounds = RectDIP {
            x: 0.0,
            y: 0.0,
            width,
            height: f32::INFINITY,
        };

        self.build_text_layout(text, temp_bounds)?;
        let (_, preferred_height) = self.get_preferred_size();
        self.cached_preferred_height_for_width = Some((width, preferred_height));
        Ok(preferred_height)
    }
}

impl<Message> Widget<Message> for Text {
    fn state(
        &self,
        _arenas: &UIArenas,
        device_resources: &crate::runtime::DeviceResources,
    ) -> super::State {
        match TextWidgetState::new(
            device_resources.dwrite_factory.clone(),
            &self.font_id,
            self.font_size,
            self.font_axes,
            self.line_spacing,
            self.text_alignment,
            self.paragraph_alignment,
            self.word_wrap,
        ) {
            Ok(state) => Some(state.into_any()),
            Err(_) => None,
        }
    }

    fn limits_x(
        &self,
        arenas: &UIArenas,
        instance: &mut Instance,
    ) -> super::limit_response::SizingForX {
        let state = with_state!(mut instance as TextWidgetState);

        if let Some(text) = self.text.resolve(arenas)
            && let Ok(preferred_width) = state.get_preferred_width(text)
        {
            let preferred_width = self
                .assisted_width
                .unwrap_or(preferred_width)
                .max(preferred_width);

            super::limit_response::SizingForX {
                min_width: if self.word_wrap { 0.0 } else { preferred_width },
                preferred_width,
            }
        } else {
            super::limit_response::SizingForX {
                min_width: 0.0,
                preferred_width: self.font_size * 1.2,
            }
        }
    }

    fn limits_y(
        &self,
        arenas: &UIArenas,
        instance: &mut Instance,
        _border_width: f32,
        content_width: f32,
    ) -> super::limit_response::SizingForY {
        let state = with_state!(mut instance as TextWidgetState);

        if let Some(text) = self.text.resolve(arenas)
            && let Ok(preferred_height) = state.get_preferred_height_for_width(text, content_width)
        {
            super::limit_response::SizingForY {
                min_height: preferred_height,
                preferred_height,
            }
        } else {
            super::limit_response::SizingForY {
                min_height: self.font_size * 1.2,
                preferred_height: self.font_size * 1.2,
            }
        }
    }

    fn update(
        &mut self,
        _arenas: &mut UIArenas,
        _instance: &mut Instance,
        _hwnd: HWND,
        _shell: &mut Shell<Message>,
        _event: &super::Event,
        _bounds: Bounds,
    ) {
        // Text widget doesn't handle any events - it's read-only
    }

    fn paint(
        &mut self,
        arenas: &UIArenas,
        instance: &mut Instance,
        _shell: &Shell<Message>,
        recorder: &mut CommandRecorder,
        style: ElementStyle,
        bounds: Bounds,
        _now: Instant,
    ) {
        let state = with_state!(mut instance as TextWidgetState);

        // Rebuild text format if properties changed
        if state.needs_text_format_rebuild(
            &self.font_id,
            self.font_size,
            self.font_axes,
            self.line_spacing,
            self.text_alignment,
            self.paragraph_alignment,
            self.word_wrap,
        ) {
            let _ = state.rebuild_text_format(
                &self.font_id,
                self.font_size,
                self.font_axes,
                self.line_spacing,
                self.text_alignment,
                self.paragraph_alignment,
                self.word_wrap,
            );
        }

        // Build text layout if needed
        let _ = state.build_text_layout(
            self.text.resolve(arenas).expect("intern string missing"),
            bounds.content_box,
        );

        // Draw the text
        if let Some(layout) = &state.text_layout {
            recorder.draw_text(
                &bounds.content_box,
                layout,
                self.color.unwrap_or(style.color.unwrap_or_default()),
            );
        }
    }

    fn cursor(
        &self,
        _arenas: &UIArenas,
        _instance: &Instance,
        _point: crate::gfx::PointDIP,
        _bounds: Bounds,
    ) -> Option<super::Cursor> {
        None // Text widget doesn't change cursor
    }
}
