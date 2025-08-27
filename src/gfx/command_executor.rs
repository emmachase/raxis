use crate::gfx::draw_commands::{DrawCommand, DrawCommandList};
use crate::widgets::Renderer;
use std::mem::ManuallyDrop;
use windows::Win32::Graphics::Direct2D::{
    Common::{D2D_RECT_F, D2D1_COLOR_F},
    D2D1_LAYER_PARAMETERS1,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, ID2D1Geometry,
};
use windows_numerics::Matrix3x2;

/// Executes drawing commands using a Direct2D renderer
pub struct CommandExecutor;

impl CommandExecutor {
    /// Execute a list of drawing commands
    pub fn execute_commands(
        renderer: &Renderer,
        commands: &DrawCommandList,
    ) -> windows::core::Result<()> {
        for command in commands.iter() {
            Self::execute_command(renderer, command)?;
        }
        Ok(())
    }

    /// Execute a single drawing command
    fn execute_command(renderer: &Renderer, command: &DrawCommand) -> windows::core::Result<()> {
        unsafe {
            match command {
                DrawCommand::Clear { color } => {
                    let d2d_color = D2D1_COLOR_F {
                        r: color.r,
                        g: color.g,
                        b: color.b,
                        a: color.a,
                    };
                    renderer.render_target.Clear(Some(&d2d_color));
                }

                DrawCommand::FillRectangle { rect, color } => {
                    renderer.fill_rectangle(rect, *color);
                }

                DrawCommand::FillRoundedRectangle {
                    rect,
                    border_radius,
                    color,
                } => {
                    renderer.fill_rounded_rectangle(rect, border_radius, *color);
                }

                DrawCommand::DrawBlurredShadow {
                    rect,
                    shadow,
                    border_radius,
                } => {
                    renderer.draw_blurred_shadow(rect, shadow, border_radius.as_ref());
                }

                DrawCommand::DrawText {
                    position,
                    layout,
                    color,
                } => {
                    renderer.brush.SetColor(&D2D1_COLOR_F {
                        r: color.r,
                        g: color.g,
                        b: color.b,
                        a: color.a,
                    });
                    renderer.render_target.DrawTextLayout(
                        *position,
                        layout,
                        renderer.brush,
                        None,
                        0,
                        D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                    );
                }

                DrawCommand::PushAxisAlignedClip { rect } => {
                    let clip_rect = D2D_RECT_F {
                        left: rect.x_dip,
                        top: rect.y_dip,
                        right: rect.x_dip + rect.width_dip,
                        bottom: rect.y_dip + rect.height_dip,
                    };
                    renderer
                        .render_target
                        .PushAxisAlignedClip(&clip_rect, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                }

                DrawCommand::PushRoundedClip {
                    rect,
                    border_radius,
                } => {
                    let clip_rect = D2D_RECT_F {
                        left: rect.x_dip,
                        top: rect.y_dip,
                        right: rect.x_dip + rect.width_dip,
                        bottom: rect.y_dip + rect.height_dip,
                    };

                    if let Ok(path_geometry) = renderer.factory.CreatePathGeometry() {
                        if let Ok(sink) = path_geometry.Open() {
                            renderer.create_rounded_rectangle_path(&sink, rect, border_radius);
                            let _ = sink.Close();

                            let layer_params = D2D1_LAYER_PARAMETERS1 {
                                contentBounds: clip_rect,
                                geometricMask: ManuallyDrop::new(Some(path_geometry.into())),
                                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                                maskTransform: Matrix3x2::identity(),
                                opacity: 1.0,
                                opacityBrush: ManuallyDrop::new(None),
                                layerOptions: Default::default(),
                            };

                            if let Ok(layer) = renderer.render_target.CreateLayer(None) {
                                renderer.render_target.PushLayer(&layer_params, &layer);
                            }

                            // Why did they make it ManuallyDrop in the first place??? idk
                            drop(ManuallyDrop::<Option<ID2D1Geometry>>::into_inner(
                                layer_params.geometricMask,
                            ));
                        }
                    }
                }

                DrawCommand::PopClip => {
                    // Try to pop layer first, then axis-aligned clip
                    // Note: In a real implementation, we'd need to track what type of clip was pushed
                    // For now, we'll assume the caller manages this correctly
                    renderer.render_target.PopLayer();
                }

                DrawCommand::DrawRectangleOutline {
                    rect,
                    color,
                    stroke_width,
                } => {
                    renderer.draw_rectangle(rect, *color, *stroke_width);
                }

                DrawCommand::DrawRoundedRectangleOutline {
                    rect,
                    border_radius,
                    color,
                    stroke_width,
                } => {
                    renderer.brush.SetColor(&D2D1_COLOR_F {
                        r: color.r,
                        g: color.g,
                        b: color.b,
                        a: color.a,
                    });
                    renderer.draw_rounded_rectangle_stroked(
                        rect,
                        border_radius,
                        *stroke_width,
                        None,
                    );
                }

                DrawCommand::DrawBorder {
                    rect,
                    border_radius,
                    border,
                } => {
                    renderer.draw_border(rect, border_radius.as_ref(), border);
                }

                DrawCommand::SetBrushColor { color } => {
                    renderer.brush.SetColor(color);
                }

                DrawCommand::DrawCircleArc {
                    center,
                    radius,
                    start_angle_deg,
                    end_angle_deg,
                    stroke_width,
                    color,
                } => {
                    renderer.brush.SetColor(&D2D1_COLOR_F {
                        r: color.r,
                        g: color.g,
                        b: color.b,
                        a: color.a,
                    });
                    // Create and draw circle arc using existing CircleArc utility
                    let arc = crate::gfx::circle_arc::CircleArc::new(
                        *center,
                        *radius,
                        *start_angle_deg,
                        *end_angle_deg,
                    );
                    if let Ok(geom) = arc.paint(renderer.factory) {
                        renderer.render_target.DrawGeometry(
                            &geom,
                            renderer.brush,
                            *stroke_width,
                            None,
                        );
                    }
                }
            }
        }
        Ok(())
    }
}
