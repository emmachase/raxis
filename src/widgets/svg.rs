use crate::{
    Shell,
    gfx::command_recorder::CommandRecorder,
    layout::{UIArenas, model::Element},
    runtime::DeviceResources,
    util::str::StableString,
    widgets::{Bounds, Cursor, Event, Instance, State, Widget, limit_response, widget},
    with_state,
};
use std::{any::Any, time::Instant};
use windows::Win32::{
    Foundation::HWND,
    Graphics::Direct2D::{
        D2D1_SVG_ATTRIBUTE_POD_TYPE_VIEWBOX, D2D1_SVG_VIEWBOX, ID2D1DeviceContext7,
        ID2D1SvgDocument,
    },
    UI::Shell::SHCreateMemStream,
};

/// SVG widget for displaying SVG documents
#[derive(Debug)]
pub struct Svg {
    /// SVG content as stable string (used for creating document in state)
    svg_content: StableString,
    /// Width for layout calculations
    width: Option<f32>,
    /// Height for layout calculations  
    height: Option<f32>,
}

/// Parsed viewBox information from SVG
#[derive(Debug, Clone)]
pub struct ViewBox {
    pub width: f32,
    pub height: f32,
}

impl ViewBox {
    /// Create a new ViewBox with the specified width and height
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    fn from_svg_document(svg_document: &ID2D1SvgDocument) -> Option<Self> {
        unsafe {
            // Get the root SVG element
            let root_element = svg_document.GetRoot().ok()?;

            // Get viewBox attribute as D2D1_SVG_VIEWBOX
            let mut viewbox = D2D1_SVG_VIEWBOX::default();
            let result = root_element.GetAttributeValue2(
                windows::core::w!("viewBox"),
                D2D1_SVG_ATTRIBUTE_POD_TYPE_VIEWBOX,
                &mut viewbox as *mut _ as *mut std::ffi::c_void,
                std::mem::size_of::<D2D1_SVG_VIEWBOX>() as u32,
            );

            if result.is_ok() {
                let width = viewbox.width;
                let height = viewbox.height;
                if width > 0.0 && height > 0.0 {
                    Some(ViewBox { width, height })
                } else {
                    None
                }
            } else {
                None
            }
        }
    }
}

/// State for SVG widget that caches the expensive SVG document
struct SvgWidgetState {
    /// Device context for creating SVG document
    device_context: ID2D1DeviceContext7,
    /// Cached SVG document
    svg_document: Option<ID2D1SvgDocument>,
    /// Cached SVG content string to detect changes
    cached_svg_content: String,
    /// Parsed viewBox for intrinsic sizing
    viewbox: Option<ViewBox>,
}

impl SvgWidgetState {
    pub fn new(device_context: ID2D1DeviceContext7) -> Self {
        Self {
            device_context,
            svg_document: None,
            cached_svg_content: String::new(),
            viewbox: None,
        }
    }

    pub fn into_any(self) -> Box<dyn Any> {
        Box::new(self)
    }

    /// Create or update SVG document if content has changed
    fn ensure_svg_document(&mut self, svg_content: &str) -> windows::core::Result<()> {
        // Only recreate if content changed
        if self.cached_svg_content != svg_content || self.svg_document.is_none() {
            unsafe {
                let svg_bytes = svg_content.as_bytes();

                let stream = SHCreateMemStream(Some(svg_bytes)).unwrap();

                let svg_document = self.device_context.CreateSvgDocument(
                    &stream,
                    windows::Win32::Graphics::Direct2D::Common::D2D_SIZE_F {
                        width: 100.0, // Default size, will be overridden by viewport
                        height: 100.0,
                    },
                )?;

                // Parse viewBox using Direct2D SVG API
                self.viewbox = ViewBox::from_svg_document(&svg_document);

                self.svg_document = Some(svg_document);
                self.cached_svg_content = svg_content.to_string();
            }
        }
        Ok(())
    }
}

impl Svg {
    /// Create a new SVG widget
    pub fn new(svg_content: impl Into<StableString>) -> Self {
        Self {
            svg_content: svg_content.into(),
            width: None,
            height: None,
        }
    }

    /// Set explicit width for layout
    pub fn with_width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Set explicit height for layout
    pub fn with_height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Set both width and height for layout
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    pub fn as_element<Message>(self, id: u64) -> Element<Message> {
        Element {
            id: Some(id),
            content: widget(self),
            ..Default::default()
        }
    }
}

impl Default for Svg {
    fn default() -> Self {
        Self::new(StableString::Static(""))
    }
}

impl<Message> Widget<Message> for Svg {
    fn limits_x(&self, _arenas: &UIArenas, instance: &mut Instance) -> limit_response::SizingForX {
        // Use explicit width if set, otherwise derive from viewBox
        if let Some(width) = self.width {
            limit_response::SizingForX {
                min_width: width,
                preferred_width: width,
            }
        } else {
            // Get viewBox from state for intrinsic sizing
            let state = with_state!(instance as SvgWidgetState);
            if let Some(ref viewbox) = state.viewbox {
                limit_response::SizingForX {
                    min_width: 0.0,
                    preferred_width: viewbox.width,
                }
            } else {
                // Fallback to default size
                limit_response::SizingForX {
                    min_width: 0.0,
                    preferred_width: 100.0,
                }
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
        // Use explicit height if set
        if let Some(height) = self.height {
            limit_response::SizingForY {
                min_height: height,
                preferred_height: height,
            }
        } else {
            // Get viewBox from state for intrinsic sizing
            let state = with_state!(instance as SvgWidgetState);
            if let Some(ref viewbox) = state.viewbox {
                // If we have explicit width, maintain aspect ratio
                let preferred_height = if self.width.is_some() {
                    content_width / (viewbox.width / viewbox.height)
                } else {
                    // Use intrinsic height, scaled proportionally to content width
                    viewbox.height * (content_width / viewbox.width)
                };
                limit_response::SizingForY {
                    min_height: 0.0,
                    preferred_height,
                }
            } else {
                // Fallback to default size
                limit_response::SizingForY {
                    min_height: 0.0,
                    preferred_height: 100.0,
                }
            }
        }
    }

    fn state(&self, _arenas: &UIArenas, device_resources: &DeviceResources) -> State {
        Some(SvgWidgetState::new(device_resources.d2d_device_context.clone()).into_any())
    }

    fn paint(
        &mut self,
        arenas: &UIArenas,
        instance: &mut Instance,
        _shell: &Shell<Message>,
        recorder: &mut CommandRecorder,
        bounds: Bounds,
        _now: Instant,
    ) {
        if let Some(svg_content) = self.svg_content.resolve(arenas) {
            let state = with_state!(mut instance as SvgWidgetState);

            // Ensure SVG document is created/cached in state
            if state.ensure_svg_document(svg_content).is_ok() {
                if let Some(ref svg_document) = state.svg_document {
                    recorder.draw_svg(&bounds.content_box, svg_document);
                }
            }
        }
    }

    fn update(
        &mut self,
        _arenas: &mut UIArenas,
        _instance: &mut Instance,
        _hwnd: HWND,
        _shell: &mut Shell<Message>,
        _event: &Event,
        _bounds: Bounds,
    ) {
        // SVG widgets don't handle events by default
    }

    fn cursor(
        &self,
        _arenas: &UIArenas,
        _instance: &Instance,
        _point: crate::gfx::PointDIP,
        _bounds: Bounds,
    ) -> Option<Cursor> {
        None
    }
}
