use crate::gfx::RectDIP;
use crate::layout::model::{
    BackdropFilter, BorderRadius, Color, DropShadow, StrokeDashStyle, StrokeLineCap,
    StrokeLineJoin, TextShadow,
};
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::Direct2D::{ID2D1Bitmap, ID2D1PathGeometry, ID2D1SvgDocument};
use windows::Win32::Graphics::DirectWrite::IDWriteTextLayout;

/// A single drawing command that can be executed later
#[derive(Clone, Debug)]
pub enum DrawCommand {
    /// Clear the entire render target with a color
    Clear {
        color: Color,
    },
    /// Fill a rectangle with a solid color
    FillRectangle {
        rect: RectDIP,
        color: Color,
    },
    /// Fill a rounded rectangle with a solid color
    FillRoundedRectangle {
        rect: RectDIP,
        border_radius: BorderRadius,
        color: Color,
    },
    /// Fill a rectangle with backdrop filter applied to content behind it
    FillRectangleWithBackdropFilter {
        rect: RectDIP,
        color: Color,
        filter: BackdropFilter,
        border_radius: Option<BorderRadius>,
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
        text_shadows: Vec<TextShadow>,
        /// Hash of the text layout content for cache identity
        text_hash: u64,
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
    PushAxisAlignedClip {
        rect: RectDIP,
    },
    /// Push a layer with rounded rectangle clipping
    PushRoundedClip {
        rect: RectDIP,
        border_radius: BorderRadius,
    },
    /// Pop the current clip
    PopAxisAlignedClip,
    /// Pop the current rounded clip
    PopRoundedClip,
    PushLayer {
        opacity: f32,
    },
    PopLayer,
    /// Set brush color for subsequent operations
    SetBrushColor {
        color: D2D1_COLOR_F,
    },
    /// Draw SVG document
    DrawSvg {
        rect: RectDIP,
        svg_document: ID2D1SvgDocument,
    },
    /// Fill a path geometry
    FillPathGeometry {
        rect: RectDIP,
        path_geometry: ID2D1PathGeometry,
        color: Color,
        scale_x: f32,
        scale_y: f32,
    },
    /// Stroke a path geometry
    StrokePathGeometry {
        rect: RectDIP,
        path_geometry: ID2D1PathGeometry,
        color: Color,
        stroke_width: f32,
        scale_x: f32,
        scale_y: f32,
        stroke_cap: Option<StrokeLineCap>,
        stroke_join: Option<StrokeLineJoin>,
    },
    /// Draw a line
    DrawLine {
        start_x: f32,
        start_y: f32,
        end_x: f32,
        end_y: f32,
        color: Color,
        stroke_width: f32,
        dash_style: Option<StrokeDashStyle>,
        stroke_cap: Option<StrokeLineCap>,
    },
    /// Draw a bitmap
    DrawBitmap {
        rect: RectDIP,
        bitmap: ID2D1Bitmap,
        opacity: f32,
    },
}

/// A list of drawing commands that can be generated and executed separately
pub type DrawCommandList = Vec<DrawCommand>;
