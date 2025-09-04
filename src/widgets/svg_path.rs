use crate::{
    Shell,
    gfx::command_recorder::CommandRecorder,
    layout::{
        UIArenas,
        model::{Element, StrokeLineCap, StrokeLineJoin},
    },
    runtime::DeviceResources,
    widgets::{
        Bounds, Color, Cursor, Event, Instance, State, Widget, limit_response, svg::ViewBox, widget,
    },
    with_state,
};
use raxis_core::SvgPathCommands;
use std::{any::Any, time::Instant};
use windows::Win32::{
    Foundation::HWND,
    Graphics::Direct2D::{ID2D1Factory8, ID2D1PathGeometry},
};

/// SVG Path widget for rendering procedurally generated SVG paths
#[derive(Debug)]
pub struct SvgPath {
    /// The SVG path to render
    svg_path: SvgPathCommands,
    /// ViewBox defining the intrinsic coordinate system and size
    viewbox: ViewBox,
    /// Stroke color
    stroke_color: Option<Color>,
    /// Fill color (optional)
    fill_color: Option<Color>,
    /// Stroke width
    stroke_width: f32,
    /// Stroke cap style
    stroke_cap: Option<StrokeLineCap>,
    /// Stroke Line join style
    stroke_join: Option<StrokeLineJoin>,
    /// Width for layout calculations
    width: Option<f32>,
    /// Height for layout calculations
    height: Option<f32>,
}

/// State for SVG Path widget that caches the Direct2D geometry
struct SvgPathWidgetState {
    /// Direct2D factory for creating geometry
    d2d_factory: ID2D1Factory8,
    /// Cached Direct2D path geometry
    path_geometry: Option<ID2D1PathGeometry>,
    /// Whether the geometry needs to be recreated
    geometry_dirty: bool,
}

impl SvgPathWidgetState {
    pub fn new(d2d_factory: ID2D1Factory8, svg_path: &SvgPathCommands) -> Self {
        // Create the geometry immediately during state creation
        let path_geometry = svg_path.create_geometry(&d2d_factory).ok();

        Self {
            d2d_factory,
            path_geometry,
            geometry_dirty: false,
        }
    }

    pub fn into_any(self) -> Box<dyn Any> {
        Box::new(self)
    }

    /// Ensure path geometry is created
    fn ensure_path_geometry(&mut self, svg_path: &SvgPathCommands) -> windows::core::Result<()> {
        if self.geometry_dirty || self.path_geometry.is_none() {
            let geometry = svg_path.create_geometry(&self.d2d_factory)?;
            self.path_geometry = Some(geometry);
            self.geometry_dirty = false;
        }
        Ok(())
    }
}

impl SvgPath {
    /// Create a new SVG Path widget
    pub fn new(svg_path: SvgPathCommands, viewbox: ViewBox) -> Self {
        Self {
            svg_path,
            viewbox,
            stroke_color: None,
            fill_color: None,
            stroke_width: 1.0,
            stroke_cap: None,
            stroke_join: None,
            width: None,
            height: None,
        }
    }

    /// Set fill color
    pub fn with_fill(mut self, fill_color: impl Into<Color>) -> Self {
        self.fill_color = Some(fill_color.into());
        self
    }

    /// Set stroke width
    pub fn with_stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = width;
        self
    }

    /// Set stroke color
    pub fn with_stroke(mut self, stroke_color: impl Into<Color>) -> Self {
        self.stroke_color = Some(stroke_color.into());
        self
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

    /// Set stroke cap style
    pub fn with_stroke_cap(mut self, stroke_cap: StrokeLineCap) -> Self {
        self.stroke_cap = Some(stroke_cap);
        self
    }

    /// Set line join style
    pub fn with_stroke_join(mut self, stroke_join: StrokeLineJoin) -> Self {
        self.stroke_join = Some(stroke_join);
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

impl<Message> Widget<Message> for SvgPath {
    fn limits_x(&self, _arenas: &UIArenas, _instance: &mut Instance) -> limit_response::SizingForX {
        if let Some(width) = self.width {
            limit_response::SizingForX {
                min_width: width,
                preferred_width: width,
            }
        } else {
            // Use ViewBox width as intrinsic size
            limit_response::SizingForX {
                min_width: 0.0,
                preferred_width: self.viewbox.width,
            }
        }
    }

    fn limits_y(
        &self,
        _arenas: &UIArenas,
        _instance: &mut Instance,
        _border_width: f32,
        content_width: f32,
    ) -> limit_response::SizingForY {
        if let Some(height) = self.height {
            limit_response::SizingForY {
                min_height: height,
                preferred_height: height,
            }
        } else {
            // Use ViewBox to maintain proper aspect ratio
            let aspect_ratio = self.viewbox.width / self.viewbox.height;
            let preferred_height = if self.width.is_some() {
                content_width / aspect_ratio
            } else {
                // Scale ViewBox height proportionally to content width
                self.viewbox.height * (content_width / self.viewbox.width)
            };
            limit_response::SizingForY {
                min_height: 0.0,
                preferred_height,
            }
        }
    }

    fn state(&self, _arenas: &UIArenas, device_resources: &DeviceResources) -> State {
        Some(
            SvgPathWidgetState::new(device_resources.d2d_factory.clone(), &self.svg_path)
                .into_any(),
        )
    }

    fn paint(
        &mut self,
        _arenas: &UIArenas,
        instance: &mut Instance,
        _shell: &Shell<Message>,
        recorder: &mut CommandRecorder,
        bounds: Bounds,
        _now: Instant,
    ) {
        let state = with_state!(mut instance as SvgPathWidgetState);

        // Calculate scale factors based on viewport vs ViewBox
        let viewport_width = bounds.content_box.width;
        let viewport_height = bounds.content_box.height;
        let scale_x = viewport_width / self.viewbox.width;
        let scale_y = viewport_height / self.viewbox.height;

        // Record path drawing commands using cached geometry with scale factors
        if state.ensure_path_geometry(&self.svg_path).is_ok()
            && let Some(ref geometry) = state.path_geometry
        {
            if let Some(fill_color) = self.fill_color {
                recorder.fill_path_geometry(
                    &bounds.content_box,
                    geometry,
                    fill_color,
                    scale_x,
                    scale_y,
                );
            }

            if let Some(stroke_color) = self.stroke_color {
                recorder.stroke_path_geometry(
                    &bounds.content_box,
                    geometry,
                    stroke_color,
                    self.stroke_width,
                    scale_x,
                    scale_y,
                    self.stroke_cap,
                    self.stroke_join,
                );
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
        // SVG Path widgets don't handle events by default
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
