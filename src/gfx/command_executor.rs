use crate::gfx::RectDIP;
use crate::gfx::draw_commands::{DrawCommand, DrawCommandList};
use crate::widgets::renderer::Renderer;
use std::mem::ManuallyDrop;
use windows::Win32::Graphics::Direct2D::{
    Common::{D2D_RECT_F, D2D1_COLOR_F},
    D2D1_LAYER_PARAMETERS1,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, ID2D1Geometry,
};
use windows_numerics::{Matrix3x2, Vector2};

/// Executes drawing commands using a Direct2D renderer
pub struct CommandExecutor;

impl CommandExecutor {
    /// Execute a list of drawing commands with optional screen bounds culling
    pub fn execute_commands(
        renderer: &Renderer,
        commands: &DrawCommandList,
    ) -> windows::core::Result<()> {
        Self::execute_commands_with_bounds(renderer, commands, None)
    }

    /// Execute a list of drawing commands with screen bounds culling
    pub fn execute_commands_with_bounds(
        renderer: &Renderer,
        commands: &DrawCommandList,
        screen_bounds: Option<RectDIP>,
    ) -> windows::core::Result<()> {
        let Some(bounds) = screen_bounds else {
            // No culling if no bounds provided - execute all commands
            for command in commands.iter() {
                Self::execute_command(renderer, command)?;
            }
            return Ok(());
        };

        let mut skip_depth = 0u32; // Track depth of skipped clip regions

        for command in commands.iter() {
            let should_execute = if skip_depth > 0 {
                // We're inside a skipped clip region - only process clip commands to track nesting
                match command {
                    DrawCommand::PushAxisAlignedClip { .. }
                    | DrawCommand::PushRoundedClip { .. } => {
                        skip_depth += 1;
                        false // Skip the push
                    }
                    DrawCommand::PopAxisAlignedClip | DrawCommand::PopRoundedClip => {
                        skip_depth -= 1;
                        false // Skip the pop
                    }
                    _ => false, // Skip all other commands
                }
            } else {
                // Not in a skipped region - check if we should execute normally
                match command {
                    DrawCommand::PushAxisAlignedClip { rect } => {
                        if Self::rect_intersects_bounds(rect, &bounds) {
                            true // Execute the push
                        } else {
                            skip_depth += 1; // Start skipping
                            false // Skip this push
                        }
                    }
                    DrawCommand::PushRoundedClip { rect, .. } => {
                        if Self::rect_intersects_bounds(rect, &bounds) {
                            true // Execute the push
                        } else {
                            skip_depth += 1; // Start skipping
                            false // Skip this push
                        }
                    }
                    _ => Self::should_execute_command_simple(command, &bounds),
                }
            };

            if should_execute {
                Self::execute_command(renderer, command)?;
            }
        }
        Ok(())
    }

    /// Check if a non-clip command should be executed based on screen bounds
    /// This excludes clip push/pop commands which are handled separately
    fn should_execute_command_simple(command: &DrawCommand, bounds: &RectDIP) -> bool {
        match command {
            // Commands that always execute regardless of bounds
            DrawCommand::Clear { .. } => true,
            DrawCommand::PopAxisAlignedClip => true,
            DrawCommand::PopRoundedClip => true,
            DrawCommand::SetBrushColor { .. } => true,

            // Commands with rectangles that can be culled
            DrawCommand::FillRectangle { rect, .. } => Self::rect_intersects_bounds(rect, bounds),
            DrawCommand::FillRoundedRectangle { rect, .. } => {
                Self::rect_intersects_bounds(rect, bounds)
            }
            DrawCommand::DrawBlurredShadow { rect, shadow, .. } => {
                // Expand rect by shadow blur radius for proper culling
                let expanded_rect = Self::expand_rect_for_shadow(rect, shadow.blur_radius);
                Self::rect_intersects_bounds(&expanded_rect, bounds)
            }
            DrawCommand::DrawRectangleOutline {
                rect, stroke_width, ..
            } => {
                // Expand rect by half stroke width on each side
                let expanded_rect = Self::expand_rect_for_stroke(rect, *stroke_width);
                Self::rect_intersects_bounds(&expanded_rect, bounds)
            }
            DrawCommand::DrawRoundedRectangleOutline {
                rect, stroke_width, ..
            } => {
                let expanded_rect = Self::expand_rect_for_stroke(rect, *stroke_width);
                Self::rect_intersects_bounds(&expanded_rect, bounds)
            }
            DrawCommand::DrawBorder { rect, .. } => Self::rect_intersects_bounds(rect, bounds),

            // Clip commands should not be processed here
            DrawCommand::PushAxisAlignedClip { .. } => {
                panic!("Clip push commands should be handled in execute_commands_with_bounds")
            }
            DrawCommand::PushRoundedClip { .. } => {
                panic!("Clip push commands should be handled in execute_commands_with_bounds")
            }

            // Rectangle-based commands
            DrawCommand::DrawText { rect, .. } => Self::rect_intersects_bounds(rect, bounds),
            DrawCommand::DrawCircleArc { center, radius, .. } => {
                // Check if circle intersects with bounds
                let circle_rect = RectDIP {
                    x: center.X - radius,
                    y: center.Y - radius,
                    width: radius * 2.0,
                    height: radius * 2.0,
                };
                Self::rect_intersects_bounds(&circle_rect, bounds)
            }
            DrawCommand::DrawSvg { rect, .. } => Self::rect_intersects_bounds(rect, bounds),
            DrawCommand::FillPathGeometry { rect, .. } => {
                Self::rect_intersects_bounds(rect, bounds)
            }
            DrawCommand::StrokePathGeometry { rect, .. } => {
                Self::rect_intersects_bounds(rect, bounds)
            }
            DrawCommand::DrawLine {
                start_x,
                start_y,
                end_x,
                end_y,
                stroke_width,
                ..
            } => {
                // Create a bounding rect for the line, expanded by stroke width
                let min_x = start_x.min(*end_x) - stroke_width / 2.0;
                let min_y = start_y.min(*end_y) - stroke_width / 2.0;
                let max_x = start_x.max(*end_x) + stroke_width / 2.0;
                let max_y = start_y.max(*end_y) + stroke_width / 2.0;
                
                let line_rect = RectDIP {
                    x: min_x,
                    y: min_y,
                    width: max_x - min_x,
                    height: max_y - min_y,
                };
                Self::rect_intersects_bounds(&line_rect, bounds)
            }
        }
    }

    /// Check if a rectangle intersects with screen bounds
    fn rect_intersects_bounds(rect: &RectDIP, bounds: &RectDIP) -> bool {
        !(rect.x >= bounds.x + bounds.width
            || rect.x + rect.width <= bounds.x
            || rect.y >= bounds.y + bounds.height
            || rect.y + rect.height <= bounds.y)
    }

    /// Expand rectangle for shadow blur radius
    fn expand_rect_for_shadow(rect: &RectDIP, blur_radius: f32) -> RectDIP {
        RectDIP {
            x: rect.x - blur_radius,
            y: rect.y - blur_radius,
            width: rect.width + blur_radius * 2.0,
            height: rect.height + blur_radius * 2.0,
        }
    }

    /// Expand rectangle for stroke width
    fn expand_rect_for_stroke(rect: &RectDIP, stroke_width: f32) -> RectDIP {
        let half_stroke = stroke_width * 0.5;
        RectDIP {
            x: rect.x - half_stroke,
            y: rect.y - half_stroke,
            width: rect.width + stroke_width,
            height: rect.height + stroke_width,
        }
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
                    rect,
                    layout,
                    color,
                } => {
                    renderer.brush.SetColor(&D2D1_COLOR_F {
                        r: color.r,
                        g: color.g,
                        b: color.b,
                        a: color.a,
                    });
                    let position = Vector2 {
                        X: rect.x,
                        Y: rect.y,
                    };
                    renderer.render_target.DrawTextLayout(
                        position,
                        layout,
                        renderer.brush,
                        None,
                        0,
                        D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                    );
                }

                DrawCommand::PushAxisAlignedClip { rect } => {
                    let clip_rect = D2D_RECT_F {
                        left: rect.x,
                        top: rect.y,
                        right: rect.x + rect.width,
                        bottom: rect.y + rect.height,
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
                        left: rect.x,
                        top: rect.y,
                        right: rect.x + rect.width,
                        bottom: rect.y + rect.height,
                    };

                    if let Ok(path_geometry) = renderer.factory.CreatePathGeometry()
                        && let Ok(sink) = path_geometry.Open()
                    {
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

                DrawCommand::PopAxisAlignedClip => {
                    renderer.render_target.PopAxisAlignedClip();
                }

                DrawCommand::PopRoundedClip => {
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

                DrawCommand::DrawSvg { rect, svg_document } => {
                    renderer.draw_svg(rect, svg_document);
                }

                DrawCommand::FillPathGeometry {
                    rect,
                    path_geometry,
                    color,
                    scale_x,
                    scale_y,
                } => {
                    renderer.fill_path_geometry(rect, path_geometry, *color, *scale_x, *scale_y);
                }

                DrawCommand::StrokePathGeometry {
                    rect,
                    path_geometry,
                    color,
                    stroke_width,
                    scale_x,
                    scale_y,
                    stroke_cap,
                    stroke_join,
                } => {
                    renderer.stroke_path_geometry(
                        rect,
                        path_geometry,
                        *color,
                        *stroke_width,
                        *scale_x,
                        *scale_y,
                        *stroke_cap,
                        *stroke_join,
                    );
                }

                DrawCommand::DrawLine {
                    start_x,
                    start_y,
                    end_x,
                    end_y,
                    color,
                    stroke_width,
                    dash_style,
                    stroke_cap,
                } => {
                    renderer.draw_line(
                        *start_x,
                        *start_y,
                        *end_x,
                        *end_y,
                        *color,
                        *stroke_width,
                        dash_style.as_ref(),
                        *stroke_cap,
                    );
                }
            }
        }
        Ok(())
    }
}
