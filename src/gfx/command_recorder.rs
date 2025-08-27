use crate::gfx::{
    RectDIP,
    draw_commands::{DrawCommand, DrawCommandList},
};
use crate::layout::model::{BorderRadius, DropShadow};
use crate::widgets::Color;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::DirectWrite::IDWriteTextLayout;
use windows_numerics::Vector2;

/// Records drawing operations as commands instead of executing them immediately
pub struct CommandRecorder {
    commands: DrawCommandList,
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
            border_radius: border_radius.clone(),
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
            shadow: shadow.clone(),
            border_radius: border_radius.cloned(),
        });
    }

    /// Record a text drawing operation
    pub fn draw_text(
        &mut self,
        position: Vector2,
        layout: &IDWriteTextLayout,
        color: impl Into<Color>,
    ) {
        self.commands.push(DrawCommand::DrawText {
            position,
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
            border_radius: border_radius.clone(),
        });
    }

    /// Record popping the current clip
    pub fn pop_clip(&mut self) {
        self.commands.push(DrawCommand::PopClip);
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
                border_radius: border_radius.clone(),
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
}
