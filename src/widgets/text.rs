use std::any::Any;
use std::fmt::Debug;
use std::time::Instant;

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_REGULAR,
    DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_METRICS,
    IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::core::Result;
use windows_core::{PCWSTR, w};

use crate::gfx::RectDIP;
use crate::widgets::{Bounds, Color, Instance, Widget};
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
    pub text: String,
    pub font_size: f32,
    pub color: Color,
    pub text_alignment: TextAlignment,
    pub paragraph_alignment: ParagraphAlignment,
    pub font_family: String,
    pub word_wrap: bool,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            font_size: 14.0,
            color: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            text_alignment: TextAlignment::Leading,
            paragraph_alignment: ParagraphAlignment::Top,
            font_family: "Segoe UI".to_string(),
            word_wrap: true,
        }
    }

    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
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

    pub fn with_font_family(mut self, font_family: impl Into<String>) -> Self {
        self.font_family = font_family.into();
        self
    }

    pub fn with_word_wrap(mut self, word_wrap: bool) -> Self {
        self.word_wrap = word_wrap;
        self
    }
}

impl Default for Text {
    fn default() -> Self {
        Self::new("Text")
    }
}

struct TextWidgetState {
    // DirectWrite objects for text rendering
    dwrite_factory: IDWriteFactory,
    text_format: IDWriteTextFormat,
    text_layout: Option<IDWriteTextLayout>,
    cached_text: String,
    cached_font_size: f32,
    cached_font_family: String,
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
        dwrite_factory: IDWriteFactory,
        font_family: &str,
        font_size: f32,
        text_alignment: TextAlignment,
        paragraph_alignment: ParagraphAlignment,
        word_wrap: bool,
    ) -> Result<Self> {
        let text_format = unsafe {
            let font_family_wide: Vec<u16> = font_family.encode_utf16().chain(Some(0)).collect();
            let text_format = dwrite_factory.CreateTextFormat(
                PCWSTR(font_family_wide.as_ptr()),
                None,
                DWRITE_FONT_WEIGHT_REGULAR,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                font_size,
                PCWSTR(w!("en-us").as_ptr()),
            )?;

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
            cached_font_family: font_family.to_string(),
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
        font_family: &str,
        font_size: f32,
        text_alignment: TextAlignment,
        paragraph_alignment: ParagraphAlignment,
        word_wrap: bool,
    ) -> bool {
        self.cached_font_family != font_family
            || self.cached_font_size != font_size
            || self.cached_text_alignment != text_alignment
            || self.cached_paragraph_alignment != paragraph_alignment
            || self.cached_word_wrap != word_wrap
    }

    fn rebuild_text_format(
        &mut self,
        font_family: &str,
        font_size: f32,
        text_alignment: TextAlignment,
        paragraph_alignment: ParagraphAlignment,
        word_wrap: bool,
    ) -> Result<()> {
        let font_family_wide: Vec<u16> = font_family.encode_utf16().chain(Some(0)).collect();

        unsafe {
            self.text_format = self.dwrite_factory.CreateTextFormat(
                PCWSTR(font_family_wide.as_ptr()),
                None,
                DWRITE_FONT_WEIGHT_REGULAR,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                font_size,
                PCWSTR(w!("en-us").as_ptr()),
            )?;

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
        self.cached_font_family = font_family.to_string();
        self.cached_font_size = font_size;
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
                    bounds.width_dip.max(1.0),  // Ensure minimum width
                    bounds.height_dip.max(1.0), // Ensure minimum height
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
                    layout.SetMaxWidth(bounds.width_dip.max(1.0))?;
                    layout.SetMaxHeight(bounds.height_dip.max(1.0))?;

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
        if let Some(width) = self.cached_preferred_width {
            return Ok(width);
        }

        let temp_bounds = RectDIP {
            x_dip: 0.0,
            y_dip: 0.0,
            width_dip: f32::INFINITY,
            height_dip: f32::INFINITY,
        };

        self.build_text_layout(text, temp_bounds)?;
        let (preferred_width, _) = self.get_preferred_size();
        self.cached_preferred_width = Some(preferred_width);
        Ok(preferred_width)
    }

    fn get_preferred_height_for_width(&mut self, text: &str, width: f32) -> Result<f32> {
        if let Some((cached_width, cached_height)) = self.cached_preferred_height_for_width {
            if (cached_width - width).abs() < 0.1 {
                return Ok(cached_height);
            }
        }

        let temp_bounds = RectDIP {
            x_dip: 0.0,
            y_dip: 0.0,
            width_dip: width,
            height_dip: f32::INFINITY,
        };

        self.build_text_layout(text, temp_bounds)?;
        let (_, preferred_height) = self.get_preferred_size();
        self.cached_preferred_height_for_width = Some((width, preferred_height));
        Ok(preferred_height)
    }
}

impl<Message> Widget<Message> for Text {
    fn state(&self, device_resources: &crate::runtime::DeviceResources) -> super::State {
        match TextWidgetState::new(
            device_resources.dwrite_factory.clone(),
            &self.font_family,
            self.font_size,
            self.text_alignment,
            self.paragraph_alignment,
            self.word_wrap,
        ) {
            Ok(state) => Some(state.into_any()),
            Err(_) => None,
        }
    }

    fn limits_x(&self, instance: &mut Instance) -> super::limit_response::SizingForX {
        let state = with_state!(mut instance as TextWidgetState);

        if let Ok(preferred_width) = state.get_preferred_width(&self.text) {
            super::limit_response::SizingForX {
                min_width: preferred_width,
                preferred_width,
            }
        } else {
            super::limit_response::SizingForX {
                min_width: self.font_size * 1.2,
                preferred_width: self.font_size * 1.2,
            }
        }
    }

    fn limits_y(&self, instance: &mut Instance, width: f32) -> super::limit_response::SizingForY {
        let state = with_state!(mut instance as TextWidgetState);

        if let Ok(preferred_height) = state.get_preferred_height_for_width(&self.text, width) {
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
        instance: &mut Instance,
        _shell: &Shell<Message>,
        recorder: &mut crate::gfx::command_recorder::CommandRecorder,
        bounds: Bounds,
        _now: Instant,
    ) {
        let state = with_state!(mut instance as TextWidgetState);

        // Rebuild text format if properties changed
        if state.needs_text_format_rebuild(
            &self.font_family,
            self.font_size,
            self.text_alignment,
            self.paragraph_alignment,
            self.word_wrap,
        ) {
            let _ = state.rebuild_text_format(
                &self.font_family,
                self.font_size,
                self.text_alignment,
                self.paragraph_alignment,
                self.word_wrap,
            );
        }

        // Build text layout if needed
        let _ = state.build_text_layout(&self.text, bounds.content_box);

        // Draw the text
        if let Some(layout) = &state.text_layout {
            recorder.draw_text(&bounds.content_box, layout, self.color);
        }
    }

    fn cursor(
        &self,
        _instance: &Instance,
        _point: crate::gfx::PointDIP,
        _bounds: Bounds,
    ) -> Option<super::Cursor> {
        None // Text widget doesn't change cursor
    }
}
