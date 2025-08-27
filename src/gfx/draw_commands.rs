use crate::gfx::RectDIP;
use crate::layout::model::{BorderRadius, DropShadow};
use crate::widgets::Color;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::DirectWrite::IDWriteTextLayout;

/// A single drawing command that can be executed later
#[derive(Debug)]
pub enum DrawCommand {
    /// Clear the entire render target with a color
    Clear { color: Color },
    /// Fill a rectangle with a solid color
    FillRectangle { rect: RectDIP, color: Color },
    /// Fill a rounded rectangle with a solid color
    FillRoundedRectangle {
        rect: RectDIP,
        border_radius: BorderRadius,
        color: Color,
    },
    /// Draw a blurred shadow
    DrawBlurredShadow {
        rect: RectDIP,
        shadow: DropShadow,
        border_radius: Option<BorderRadius>,
    },
    /// Draw text using DirectWrite text layout
    DrawText {
        rect: RectDIP,
        layout: IDWriteTextLayout,
        color: Color,
    },
    /// Draw a rectangle outline (stroke)
    DrawRectangleOutline {
        rect: RectDIP,
        color: Color,
        stroke_width: f32,
    },
    /// Draw a rounded rectangle outline (stroke)
    DrawRoundedRectangleOutline {
        rect: RectDIP,
        border_radius: BorderRadius,
        color: Color,
        stroke_width: f32,
    },
    /// Draw a border with full border specification
    DrawBorder {
        rect: RectDIP,
        border_radius: Option<BorderRadius>,
        border: crate::layout::model::Border,
    },
    /// Draw a circle arc
    DrawCircleArc {
        center: windows_numerics::Vector2,
        radius: f32,
        start_angle_deg: f32,
        end_angle_deg: f32,
        stroke_width: f32,
        color: Color,
    },
    /// Push an axis-aligned clip rectangle
    PushAxisAlignedClip { rect: RectDIP },
    /// Push a layer with rounded rectangle clipping
    PushRoundedClip {
        rect: RectDIP,
        border_radius: BorderRadius,
    },
    /// Pop the current clip
    PopAxisAlignedClip,
    /// Pop the current rounded clip
    PopRoundedClip,
    /// Set brush color for subsequent operations
    SetBrushColor { color: D2D1_COLOR_F },
}

/// A list of drawing commands that can be generated and executed separately
#[derive(Debug, Default)]
pub struct DrawCommandList {
    pub commands: Vec<DrawCommand>,
}

impl DrawCommandList {
    /// Create a new empty command list
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Add a command to the list
    pub fn push(&mut self, command: DrawCommand) {
        self.commands.push(command);
    }

    /// Clear all commands
    pub fn clear(&mut self) {
        self.commands.clear();
    }

    /// Get the number of commands
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Check if the command list is empty
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Iterate over commands
    pub fn iter(&self) -> impl Iterator<Item = &DrawCommand> {
        self.commands.iter()
    }
}
