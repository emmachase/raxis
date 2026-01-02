use std::any::Any;
use std::collections::hash_map::DefaultHasher;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::panic::Location;
use std::time::Instant;

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext6;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_HIT_TEST_METRICS, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_METRICS, IDWriteFactory6, IDWriteTextFormat3, IDWriteTextLayout,
};
use windows::core::Result;

use crate::gfx::RectDIP;
use crate::gfx::command_recorder::CommandRecorder;
use crate::layout::UIArenas;
use crate::layout::model::{Color, Element, ElementStyle, TextShadow};
use crate::runtime::font_manager::{
    FontAxes, FontIdentifier, FontStyle, FontWeight, FontWidth, GlobalFontManager, LineSpacing,
};
use crate::util::str::StableString;
use crate::util::unique::{combine_id, id_from_location};
use crate::widgets::svg_path::ColorChoice;
use crate::widgets::{Bounds, Instance, Widget, widget};
use crate::{RedrawRequest, Shell, with_state};

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

/// Text span with styling and optional hyperlink
#[derive(Debug, Clone, PartialEq)]
pub struct TextSpan {
    pub start: usize,
    pub end: usize,
    pub color: Color,
    /// Optional hover color (used when this span is a hyperlink)
    pub hover_color: Option<Color>,
    /// Optional URL - if set, this span becomes a clickable hyperlink
    pub url: Option<String>,
}

impl TextSpan {
    pub fn new(start: usize, end: usize, color: Color) -> Self {
        Self {
            start,
            end,
            color,
            hover_color: None,
            url: None,
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn with_hover_color(mut self, color: Color) -> Self {
        self.hover_color = Some(color);
        self
    }

    pub fn is_hyperlink(&self) -> bool {
        self.url.is_some()
    }
}

impl Hash for TextSpan {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.start.hash(state);
        self.end.hash(state);
        self.color.r.to_bits().hash(state);
        self.color.g.to_bits().hash(state);
        self.color.b.to_bits().hash(state);
        self.color.a.to_bits().hash(state);
        if let Some(color) = &self.hover_color {
            color.r.to_bits().hash(state);
            color.g.to_bits().hash(state);
            color.b.to_bits().hash(state);
            color.a.to_bits().hash(state);
        }
        self.url.hash(state);
    }
}

/// A segment of colored text for easier construction
#[derive(Debug, Clone)]
pub struct ColoredTextSegment {
    pub text: String,
    pub color: Color,
}

impl ColoredTextSegment {
    pub fn new(text: impl Into<String>, color: Color) -> Self {
        Self {
            text: text.into(),
            color,
        }
    }
}

/// Simple text display widget for showing read-only text
#[derive(Debug)]
pub struct Text {
    pub text: StableString,
    pub font_size: f32,
    pub line_spacing: Option<LineSpacing>,
    pub color: Option<ColorChoice>,
    pub text_shadows: Vec<TextShadow>,
    pub text_alignment: TextAlignment,
    pub paragraph_alignment: ParagraphAlignment,
    pub font_id: FontIdentifier,
    pub font_axes: FontAxes,
    pub word_wrap: bool,
    pub caller: &'static Location<'static>,
    pub spans: Vec<TextSpan>,

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
            text_shadows: Vec::new(),
            text_alignment: TextAlignment::Leading,
            paragraph_alignment: ParagraphAlignment::Top,
            font_id: FontIdentifier::system("Segoe UI"),
            font_axes: FontAxes::default(),
            word_wrap: true,
            caller: Location::caller(),
            spans: Vec::new(),

            assisted_width: None,
            assisted_id: None,
        }
    }

    #[track_caller]
    pub fn new_with_spans(text: impl Into<StableString>, spans: Vec<TextSpan>) -> Self {
        Self {
            text: text.into(),
            font_size: 14.0,
            line_spacing: None,
            color: None,
            text_shadows: Vec::new(),
            text_alignment: TextAlignment::Leading,
            paragraph_alignment: ParagraphAlignment::Top,
            font_id: FontIdentifier::system("Segoe UI"),
            font_axes: FontAxes::default(),
            word_wrap: true,
            caller: Location::caller(),
            spans,

            assisted_width: None,
            assisted_id: None,
        }
    }

    #[track_caller]
    pub fn new_with_colored_segments(segments: Vec<ColoredTextSegment>) -> Self {
        let mut full_text = String::new();
        let mut spans = Vec::new();

        for segment in segments {
            let start = full_text.len();
            full_text.push_str(&segment.text);
            let end = full_text.len();

            spans.push(TextSpan::new(start, end, segment.color));
        }

        Self {
            text: StableString::Heap(full_text),
            font_size: 14.0,
            line_spacing: None,
            color: None,
            text_shadows: Vec::new(),
            text_alignment: TextAlignment::Leading,
            paragraph_alignment: ParagraphAlignment::Top,
            font_id: FontIdentifier::system("Segoe UI"),
            font_axes: FontAxes::default(),
            word_wrap: true,
            caller: Location::caller(),
            spans,

            assisted_width: None,
            assisted_id: None,
        }
    }

    pub fn with_spans(mut self, spans: Vec<TextSpan>) -> Self {
        self.spans = spans;
        self
    }

    pub fn with_span(mut self, span: TextSpan) -> Self {
        self.spans.push(span);
        self
    }

    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_line_spacing(mut self, line_spacing: LineSpacing) -> Self {
        self.line_spacing = Some(line_spacing);
        self
    }

    pub fn with_color(mut self, color: impl Into<ColorChoice>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn with_text_shadow(mut self, text_shadow: TextShadow) -> Self {
        self.text_shadows = vec![text_shadow];
        self
    }

    pub fn with_text_shadows(mut self, text_shadows: Vec<TextShadow>) -> Self {
        self.text_shadows = text_shadows;
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

    pub fn with_font_family(mut self, font_id: impl Into<FontIdentifier>) -> Self {
        self.font_id = font_id.into();
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
            text_shadows: self.text_shadows.clone(),
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
    d2d_device_context: ID2D1DeviceContext6,
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

    // Span caching - None means not yet applied, Some(hash) means applied with that hash
    cached_spans_hash: Option<u64>,

    // Hyperlink hover state
    hovered_hyperlink_index: Option<usize>,
}

impl TextWidgetState {
    pub fn new(
        dwrite_factory: IDWriteFactory6,
        d2d_device_context: ID2D1DeviceContext6,
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
            d2d_device_context,
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
            cached_spans_hash: None,
            hovered_hyperlink_index: None,
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
            self.cached_spans_hash = None;
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

    fn spans_changed(&mut self, new_spans: &[TextSpan]) -> bool {
        let mut hasher = DefaultHasher::new();
        new_spans.hash(&mut hasher);
        let new_hash = hasher.finish();

        let changed = self.cached_spans_hash != Some(new_hash);
        if changed {
            self.cached_spans_hash = Some(new_hash);
        }
        changed
    }

    fn apply_spans(&self, layout: &IDWriteTextLayout, spans: &[TextSpan]) -> Result<()> {
        use windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_RANGE;

        unsafe {
            for span in spans {
                let range = DWRITE_TEXT_RANGE {
                    startPosition: span.start as u32,
                    length: (span.end - span.start) as u32,
                };

                // Create a solid color brush for this span's color
                let brush = self.d2d_device_context.CreateSolidColorBrush(
                    &windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
                        r: span.color.r,
                        g: span.color.g,
                        b: span.color.b,
                        a: span.color.a,
                    },
                    None,
                )?;

                layout.SetDrawingEffect(&brush, range)?;
            }
        }
        Ok(())
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

    /// Hit-test a point in DIPs against the text layout, returning the UTF-16 character index.
    fn hit_test_index(&self, x_dip: f32, y_dip: f32) -> Result<u32> {
        unsafe {
            let layout = self.text_layout.as_ref().expect("layout not built");
            let mut trailing = windows::core::BOOL(0);
            let mut inside = windows::core::BOOL(0);
            let mut metrics = DWRITE_HIT_TEST_METRICS::default();
            layout.HitTestPoint(x_dip, y_dip, &mut trailing, &mut inside, &mut metrics)?;

            let mut idx = if trailing.as_bool() {
                metrics.textPosition.saturating_add(metrics.length)
            } else {
                metrics.textPosition
            };
            let total_len = self.cached_text.encode_utf16().count() as u32;
            if idx > total_len {
                idx = total_len;
            }
            Ok(idx)
        }
    }

    /// Find which hyperlink span (if any) contains the given UTF-16 character index.
    fn find_hyperlink_at_index(&self, idx: u32, spans: &[TextSpan]) -> Option<usize> {
        // Convert UTF-16 index to byte index for comparison with span ranges
        let byte_idx = self.utf16_to_byte_index(idx as usize);
        
        for (i, span) in spans.iter().enumerate() {
            if span.is_hyperlink() && byte_idx >= span.start && byte_idx < span.end {
                return Some(i);
            }
        }
        None
    }

    /// Convert UTF-16 code unit index to byte index in the cached text.
    fn utf16_to_byte_index(&self, utf16_idx: usize) -> usize {
        let mut byte_idx = 0;
        let mut utf16_count = 0;
        
        for ch in self.cached_text.chars() {
            if utf16_count >= utf16_idx {
                break;
            }
            utf16_count += ch.len_utf16();
            byte_idx += ch.len_utf8();
        }
        byte_idx
    }

    fn apply_hyperlink_underlines(
        &self,
        layout: &IDWriteTextLayout,
        spans: &[TextSpan],
        hovered_index: Option<usize>,
    ) -> Result<()> {
        use windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_RANGE;

        unsafe {
            for (i, span) in spans.iter().enumerate() {
                if !span.is_hyperlink() {
                    continue;
                }

                // Convert byte indices to UTF-16 indices for DirectWrite
                let start_utf16 = self.byte_to_utf16_index(span.start);
                let end_utf16 = self.byte_to_utf16_index(span.end);
                
                let range = DWRITE_TEXT_RANGE {
                    startPosition: start_utf16 as u32,
                    length: (end_utf16 - start_utf16) as u32,
                };

                // Determine color based on hover state
                let color = if Some(i) == hovered_index {
                    span.hover_color.unwrap_or(span.color)
                } else {
                    span.color
                };

                let brush = self.d2d_device_context.CreateSolidColorBrush(
                    &windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
                        r: color.r,
                        g: color.g,
                        b: color.b,
                        a: color.a,
                    },
                    None,
                )?;

                layout.SetDrawingEffect(&brush, range)?;

                // Apply underline to hyperlinks
                layout.SetUnderline(true, range)?;
            }
        }
        Ok(())
    }

    /// Convert byte index to UTF-16 code unit index.
    fn byte_to_utf16_index(&self, byte_idx: usize) -> usize {
        let mut utf16_count = 0;
        let mut current_byte = 0;
        
        for ch in self.cached_text.chars() {
            if current_byte >= byte_idx {
                break;
            }
            current_byte += ch.len_utf8();
            utf16_count += ch.len_utf16();
        }
        utf16_count
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
            device_resources.d2d_device_context.clone(),
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
        instance: &mut Instance,
        hwnd: HWND,
        shell: &mut Shell<Message>,
        event: &super::Event,
        bounds: Bounds,
    ) {
        // Check if we have any hyperlinks in spans
        let has_hyperlinks = self.spans.iter().any(|s| s.is_hyperlink());
        if !has_hyperlinks {
            return;
        }

        let state = with_state!(mut instance as TextWidgetState);
        let content_box = bounds.content_box;

        match event {
            super::Event::MouseMove { x, y }
            | super::Event::MouseEnter { x, y } => {
                let widget_x = x - content_box.x;
                let widget_y = y - content_box.y;

                if let Ok(idx) = state.hit_test_index(widget_x, widget_y) {
                    let new_hovered = state.find_hyperlink_at_index(idx, &self.spans);
                    if new_hovered != state.hovered_hyperlink_index {
                        state.hovered_hyperlink_index = new_hovered;
                        // Invalidate to redraw with new hover state
                        state.cached_spans_hash = None;
                        shell.request_redraw(hwnd, RedrawRequest::Immediate);
                    }
                } else {
                    if state.hovered_hyperlink_index.is_some() {
                        state.hovered_hyperlink_index = None;
                        state.cached_spans_hash = None;
                        shell.request_redraw(hwnd, RedrawRequest::Immediate);
                    }
                }
            }
            super::Event::MouseLeave { .. } => {
                if state.hovered_hyperlink_index.is_some() {
                    state.hovered_hyperlink_index = None;
                    state.cached_spans_hash = None;
                    shell.request_redraw(hwnd, RedrawRequest::Immediate);
                }
            }
            super::Event::MouseButtonUp { x, y, .. } => {
                let widget_x = x - content_box.x;
                let widget_y = y - content_box.y;

                if let Ok(idx) = state.hit_test_index(widget_x, widget_y) {
                    if let Some(span_idx) = state.find_hyperlink_at_index(idx, &self.spans) {
                        if let Some(url) = &self.spans[span_idx].url {
                            shell.open_url(url);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn paint(
        &mut self,
        arenas: &UIArenas,
        instance: &mut Instance,
        _shell: &mut Shell<Message>,
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

        // Apply span styles (colors and hyperlink underlines) if they changed or layout was rebuilt
        if !self.spans.is_empty() && state.spans_changed(&self.spans)
            && let Some(layout) = &state.text_layout {
                let _ = state.apply_spans(layout, &self.spans);
                // Apply hyperlink underlines for spans with URLs
                let _ = state.apply_hyperlink_underlines(layout, &self.spans, state.hovered_hyperlink_index);
            }

        // Draw the text
        if let Some(layout) = &state.text_layout {
            // Combine widget shadows with style shadows (widget shadows have priority)
            let shadows = if !self.text_shadows.is_empty() {
                &self.text_shadows
            } else {
                &style.text_shadows
            };

            recorder.draw_text(
                &bounds.content_box,
                layout,
                self.color.unwrap_or(ColorChoice::CurrentColor).or_current_color(style.color).unwrap_or_default(),
                shadows,
            );
        }
    }

    fn cursor(
        &self,
        _arenas: &UIArenas,
        instance: &Instance,
        point: crate::gfx::PointDIP,
        bounds: Bounds,
    ) -> Option<super::Cursor> {
        // Check if we have any hyperlinks in spans
        let has_hyperlinks = self.spans.iter().any(|s| s.is_hyperlink());
        if !has_hyperlinks {
            return None;
        }

        let state = with_state!(instance as TextWidgetState);
        let content_box = bounds.content_box;
        let widget_x = point.x - content_box.x;
        let widget_y = point.y - content_box.y;

        if let Ok(idx) = state.hit_test_index(widget_x, widget_y) {
            if state.find_hyperlink_at_index(idx, &self.spans).is_some() {
                return Some(super::Cursor::Pointer);
            }
        }
        None // Text widget doesn't change cursor otherwise
    }
}
