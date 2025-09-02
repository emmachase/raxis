use crate::gfx::{
    RectDIP,
    draw_commands::{DrawCommand, DrawCommandList},
};
use crate::layout::model::{BorderRadius, DropShadow};
use crate::widgets::Color;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::Direct2D::{ID2D1PathGeometry, ID2D1SvgDocument};
use windows::Win32::Graphics::DirectWrite::IDWriteTextLayout;

/// Records drawing operations as commands instead of executing them immediately
pub struct CommandRecorder {
    commands: DrawCommandList,
}

impl Default for CommandRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandRecorder {
    /// Create a new command recorder
    pub fn new() -> Self {
        Self {
            commands: DrawCommandList::new(),
        }
    }

    /// Clear the command list
    pub fn clear(&mut self) {
        self.commands.clear();
    }

    /// Get the recorded commands
    pub fn take_commands(&mut self) -> DrawCommandList {
        std::mem::take(&mut self.commands)
    }

    /// Get a reference to the commands
    pub fn commands(&self) -> &DrawCommandList {
        &self.commands
    }

    /// Record a clear operation
    pub fn clear_background(&mut self, color: impl Into<Color>) {
        self.commands.push(DrawCommand::Clear {
            color: color.into(),
        });
    }

    /// Record a rectangle fill operation
    pub fn fill_rectangle(&mut self, rect: &RectDIP, color: impl Into<Color>) {
        self.commands.push(DrawCommand::FillRectangle {
            rect: *rect,
            color: color.into(),
        });
    }

    /// Record a rounded rectangle fill operation
    pub fn fill_rounded_rectangle(
        &mut self,
        rect: &RectDIP,
        border_radius: &BorderRadius,
        color: impl Into<Color>,
    ) {
        self.commands.push(DrawCommand::FillRoundedRectangle {
            rect: *rect,
            border_radius: *border_radius,
            color: color.into(),
        });
    }

    /// Record a blurred shadow drawing operation
    pub fn draw_blurred_shadow(
        &mut self,
        rect: &RectDIP,
        shadow: &DropShadow,
        border_radius: Option<&BorderRadius>,
    ) {
        self.commands.push(DrawCommand::DrawBlurredShadow {
            rect: *rect,
            shadow: *shadow,
            border_radius: border_radius.cloned(),
        });
    }

    /// Record a text drawing operation
    pub fn draw_text(
        &mut self,
        rect: &RectDIP,
        layout: &IDWriteTextLayout,
        color: impl Into<Color>,
    ) {
        self.commands.push(DrawCommand::DrawText {
            rect: *rect,
            layout: layout.clone(),
            color: color.into(),
        });
    }

    /// Record pushing an axis-aligned clip rectangle
    pub fn push_axis_aligned_clip(&mut self, rect: &RectDIP) {
        self.commands
            .push(DrawCommand::PushAxisAlignedClip { rect: *rect });
    }

    /// Record pushing a rounded clip rectangle
    pub fn push_rounded_clip(&mut self, rect: &RectDIP, border_radius: &BorderRadius) {
        self.commands.push(DrawCommand::PushRoundedClip {
            rect: *rect,
            border_radius: *border_radius,
        });
    }

    /// Record popping the current axis-aligned clip
    pub fn pop_axis_aligned_clip(&mut self) {
        self.commands.push(DrawCommand::PopAxisAlignedClip);
    }

    /// Record popping the current rounded clip
    pub fn pop_rounded_clip(&mut self) {
        self.commands.push(DrawCommand::PopRoundedClip);
    }

    /// Record a rectangle outline drawing command
    pub fn draw_rectangle(&mut self, rect: &RectDIP, color: impl Into<Color>, thickness: f32) {
        self.commands.push(DrawCommand::DrawRectangleOutline {
            rect: *rect,
            color: color.into(),
            stroke_width: thickness,
        });
    }

    /// Record a circle arc drawing command
    pub fn draw_circle_arc(
        &mut self,
        center: windows_numerics::Vector2,
        radius: f32,
        start_angle_deg: f32,
        end_angle_deg: f32,
        stroke_width: f32,
        color: impl Into<Color>,
    ) {
        self.commands.push(DrawCommand::DrawCircleArc {
            center,
            radius,
            start_angle_deg,
            end_angle_deg,
            stroke_width,
            color: color.into(),
        });
    }

    /// Record drawing a rounded rectangle outline
    pub fn draw_rounded_rectangle_stroked(
        &mut self,
        rect: &RectDIP,
        border_radius: &BorderRadius,
        color: impl Into<Color>,
        stroke_width: f32,
    ) {
        self.commands
            .push(DrawCommand::DrawRoundedRectangleOutline {
                rect: *rect,
                border_radius: *border_radius,
                color: color.into(),
                stroke_width,
            });
    }

    /// Record drawing a border
    pub fn draw_border(
        &mut self,
        rect: &RectDIP,
        border_radius: Option<&BorderRadius>,
        border: &crate::layout::model::Border,
    ) {
        self.commands.push(DrawCommand::DrawBorder {
            rect: *rect,
            border_radius: border_radius.cloned(),
            border: border.clone(),
        });
    }

    /// Record setting brush color
    pub fn set_brush_color(&mut self, color: D2D1_COLOR_F) {
        self.commands.push(DrawCommand::SetBrushColor { color });
    }

    /// Record drawing an SVG document
    pub fn draw_svg(&mut self, rect: &RectDIP, svg_document: &ID2D1SvgDocument) {
        self.commands.push(DrawCommand::DrawSvg {
            rect: *rect,
            svg_document: svg_document.clone(),
        });
    }

    /// Record filling a path geometry
    pub fn fill_path_geometry(
        &mut self,
        rect: &RectDIP,
        path_geometry: &ID2D1PathGeometry,
        color: impl Into<Color>,
        scale_x: f32,
        scale_y: f32,
    ) {
        self.commands.push(DrawCommand::FillPathGeometry {
            rect: *rect,
            path_geometry: path_geometry.clone(),
            color: color.into(),
            scale_x,
            scale_y,
        });
    }

    /// Record stroking a path geometry
    pub fn stroke_path_geometry(
        &mut self,
        rect: &RectDIP,
        path_geometry: &ID2D1PathGeometry,
        color: impl Into<Color>,
        stroke_width: f32,
        scale_x: f32,
        scale_y: f32,
        stroke_cap: Option<crate::layout::model::StrokeCap>,
        stroke_join: Option<crate::layout::model::StrokeLineJoin>,
    ) {
        self.commands.push(DrawCommand::StrokePathGeometry {
            rect: *rect,
            path_geometry: path_geometry.clone(),
            color: color.into(),
            stroke_width,
            scale_x,
            scale_y,
            stroke_cap,
            stroke_join,
        });
    }
}
