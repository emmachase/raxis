use crate::gfx::RectDIP;
use crate::gfx::draw_commands::{DrawCommand, DrawCommandList};
use crate::layout::model::{BackdropFilter, BorderRadius, Color};
use crate::widgets::renderer::Renderer;
use std::mem::ManuallyDrop;
use windows::Win32::Graphics::Direct2D::{
    Common::{D2D_RECT_F, D2D1_COLOR_F, D2D1_COMPOSITE_MODE_SOURCE_OVER},
    D2D1_LAYER_PARAMETERS1,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
    D2D1_INTERPOLATION_MODE_LINEAR, ID2D1Brush, ID2D1Geometry, ID2D1Image,
};
use windows_core::Interface;
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
            for i in 0..commands.len() {
                Self::execute_command(renderer, commands, i)?;
            }
            return Ok(());
        };

        let mut skip_depth = 0u32; // Track depth of skipped clip regions

        for i in 0..commands.len() {
            let should_execute = if skip_depth > 0 {
                // We're inside a skipped clip region - only process clip commands to track nesting
                match commands[i] {
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
                Self::should_execute_command_simple(&commands[i], &bounds, &mut skip_depth)
            };

            if should_execute {
                Self::execute_command(renderer, commands, i)?;
            }
        }
        Ok(())
    }

    /// Check if a non-clip command should be executed based on screen bounds
    /// This excludes clip push/pop commands which are handled separately
    fn should_execute_command_simple(
        command: &DrawCommand,
        bounds: &RectDIP,
        skip_depth: &mut u32,
    ) -> bool {
        match command {
            // Commands that always execute regardless of bounds
            DrawCommand::Clear { .. } => true,
            DrawCommand::PopAxisAlignedClip => true,
            DrawCommand::PopRoundedClip => true,
            DrawCommand::PushLayer { .. } => true,
            DrawCommand::PopLayer => true,
            DrawCommand::SetBrushColor { .. } => true,

            // Commands with rectangles that can be culled
            DrawCommand::FillRectangle { rect, .. } => Self::rect_intersects_bounds(rect, bounds),
            DrawCommand::FillRoundedRectangle { rect, .. } => {
                Self::rect_intersects_bounds(rect, bounds)
            }
            DrawCommand::FillRectangleWithBackdropFilter { rect, .. } => {
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

            DrawCommand::PushAxisAlignedClip { rect } => {
                if Self::rect_intersects_bounds(rect, bounds) {
                    true // Execute the push
                } else {
                    *skip_depth += 1; // Start skipping
                    false // Skip this push
                }
            }
            DrawCommand::PushRoundedClip { rect, .. } => {
                if Self::rect_intersects_bounds(rect, bounds) {
                    true // Execute the push
                } else {
                    *skip_depth += 1; // Start skipping
                    false // Skip this push
                }
            }

            // Rectangle-based commands
            DrawCommand::DrawText { rect, .. } => Self::rect_intersects_bounds(rect, bounds),
            DrawCommand::DrawBitmap { rect, .. } => Self::rect_intersects_bounds(rect, bounds),
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
    pub fn execute_command(
        renderer: &Renderer,
        command_list: &[DrawCommand],
        command_index: usize,
    ) -> windows::core::Result<()> {
        let command = &command_list[command_index];

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
                    if border_radius.is_some() {
                        renderer.fill_rounded_rectangle(rect, border_radius, *color);
                    } else {
                        renderer.fill_rectangle(rect, *color);
                    }
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
                    text_shadows,
                    text_hash,
                } => {
                    // Draw text shadows first (in order, so first shadow is bottom-most)
                    for shadow in text_shadows.iter() {
                        let shadow_position = Vector2 {
                            X: rect.x + shadow.offset_x,
                            Y: rect.y + shadow.offset_y,
                        };

                        if shadow.blur_radius > 0.0 {
                            // Use Direct2D shadow effect for blurred shadows
                            renderer.draw_text_with_blurred_shadow(
                                &shadow_position,
                                layout,
                                shadow,
                                *text_hash,
                            );
                        } else {
                            // Simple unblurred shadow - just draw text with offset
                            renderer.brush.SetColor(&D2D1_COLOR_F {
                                r: shadow.color.r,
                                g: shadow.color.g,
                                b: shadow.color.b,
                                a: shadow.color.a,
                            });
                            renderer.render_target.DrawTextLayout(
                                shadow_position,
                                layout,
                                renderer.brush,
                                None,
                                0,
                                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                            );
                        }
                    }

                    // Draw the actual text
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

                DrawCommand::PushLayer { opacity } => {
                    let layer_params = D2D1_LAYER_PARAMETERS1 {
                        contentBounds: D2D_RECT_F {
                            left: -f32::MAX,
                            top: -f32::MAX,
                            right: f32::MAX,
                            bottom: f32::MAX,
                        },
                        geometricMask: ManuallyDrop::new(None),
                        maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                        maskTransform: Matrix3x2::identity(),
                        opacity: *opacity,
                        opacityBrush: ManuallyDrop::new(None),
                        layerOptions: Default::default(),
                    };

                    renderer.render_target.PushLayer(&layer_params, None);

                    // Why did they make it ManuallyDrop in the first place??? idk
                    drop(ManuallyDrop::<Option<ID2D1Geometry>>::into_inner(
                        layer_params.geometricMask,
                    ));

                    drop(ManuallyDrop::<Option<ID2D1Brush>>::into_inner(
                        layer_params.opacityBrush,
                    ));
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

                        renderer.render_target.PushLayer(&layer_params, None);

                        // Why did they make it ManuallyDrop in the first place??? idk
                        drop(ManuallyDrop::<Option<ID2D1Geometry>>::into_inner(
                            layer_params.geometricMask,
                        ));

                        drop(ManuallyDrop::<Option<ID2D1Brush>>::into_inner(
                            layer_params.opacityBrush,
                        ));
                    }
                }

                DrawCommand::PopAxisAlignedClip => {
                    renderer.render_target.PopAxisAlignedClip();
                }

                DrawCommand::PopRoundedClip | DrawCommand::PopLayer => {
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

                DrawCommand::DrawBitmap {
                    rect,
                    bitmap,
                    opacity,
                } => {
                    renderer.draw_bitmap(rect, bitmap, *opacity);
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

                DrawCommand::FillRectangleWithBackdropFilter {
                    rect,
                    border_radius,
                    color,
                    filter,
                } => {
                    // Backdrop-filter commands are handled separately in execute_commands_with_bounds
                    // This should not be reached in normal execution
                    Self::execute_backdrop_filter(
                        renderer,
                        command_list,
                        command_index,
                        *rect,
                        *color,
                        *border_radius,
                        filter.clone(),
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Execute a backdrop-filter command: render background, apply filter, composite, render element
    fn execute_backdrop_filter(
        renderer: &Renderer,
        commands: &[DrawCommand],
        command_index: usize,
        bounds: RectDIP,
        color: Color,
        border_radius: Option<BorderRadius>,
        filter: BackdropFilter,
    ) -> windows::core::Result<()> {
        // Find background commands (commands before this one that intersect filter bounds)
        let mut background_commands = Vec::new();
        let mut skip_depth = 0;
        for (index, command) in commands.iter().enumerate() {
            if index >= command_index {
                break;
            }

            let should_execute = if skip_depth > 0 {
                // We're inside a skipped clip region - only process clip commands to track nesting
                match commands[index] {
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
                Self::should_execute_command_simple(&commands[index], &bounds, &mut skip_depth)
            };

            if should_execute {
                background_commands.push((index, command));
            }
        }

        // Calculate padding for effects that expand (blur effects need padding)
        let blur_padding = match &filter {
            BackdropFilter::Blur { radius } => radius * 3.0,
            BackdropFilter::Custom(effect) => effect.input_padding(),
        };

        let expanded_bounds = RectDIP {
            x: bounds.x - blur_padding,
            y: bounds.y - blur_padding,
            width: bounds.width + blur_padding * 2.0,
            height: bounds.height + blur_padding * 2.0,
        };

        // Render background commands to offscreen bitmap
        let background_cmds: Vec<DrawCommand> = background_commands
            .iter()
            .map(|(_, cmd)| (*cmd).clone())
            .collect();
        let background_bitmap =
            renderer.render_commands_to_bitmap(&background_cmds, &expanded_bounds)?;

        // Apply the appropriate effect based on filter type
        let effect_output = Self::apply_backdrop_effect(renderer, &background_bitmap, filter)?;

        let clip_rect = D2D_RECT_F {
            left: bounds.x,
            top: bounds.y,
            right: bounds.x + bounds.width,
            bottom: bounds.y + bounds.height,
        };

        // Composite blurred background to main render target
        unsafe {
            // Push layer with border-radius mask if needed
            if let Some(border_radius) = border_radius {
                if let Ok(path_geometry) = renderer.factory.CreatePathGeometry()
                    && let Ok(sink) = path_geometry.Open()
                {
                    renderer.create_rounded_rectangle_path(&sink, &bounds, &border_radius);
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

                    let layer = None; // We don't need to keep the layer reference
                    renderer.render_target.PushLayer(&layer_params, layer);

                    let d2d_color = D2D1_COLOR_F {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    };
                    renderer.render_target.Clear(Some(&d2d_color));

                    // Draw filtered background
                    let blur_image: ID2D1Image = Interface::cast(&effect_output)?;
                    renderer.render_target.DrawImage(
                        &blur_image,
                        Some(&Vector2::new(expanded_bounds.x, expanded_bounds.y)),
                        None,
                        D2D1_INTERPOLATION_MODE_LINEAR,
                        D2D1_COMPOSITE_MODE_SOURCE_OVER,
                    );

                    // Fill the element on top of the blurred background (within the layer)
                    // renderer.fill_rounded_rectangle(&bounds, &border_radius, color);
                    renderer.fill_rectangle(&bounds, color);

                    renderer.render_target.PopLayer();

                    // Clean up ManuallyDrop
                    drop(ManuallyDrop::<Option<ID2D1Geometry>>::into_inner(
                        layer_params.geometricMask,
                    ));
                    drop(ManuallyDrop::<Option<ID2D1Brush>>::into_inner(
                        layer_params.opacityBrush,
                    ));
                }
            } else {
                // No border radius - just draw blurred background
                renderer
                    .render_target
                    .PushAxisAlignedClip(&clip_rect, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);

                let effect_image: ID2D1Image = Interface::cast(&effect_output)?;
                renderer.render_target.DrawImage(
                    &effect_image,
                    Some(&Vector2::new(expanded_bounds.x, expanded_bounds.y)),
                    None,
                    D2D1_INTERPOLATION_MODE_LINEAR,
                    D2D1_COMPOSITE_MODE_SOURCE_OVER,
                );

                // Render element fill on top (no border radius, so no layer needed)
                renderer.fill_rectangle(&bounds, color);

                renderer.render_target.PopAxisAlignedClip();
            }
        }

        Ok(())
    }

    /// Apply the appropriate effect based on the BackdropFilter variant.
    fn apply_backdrop_effect(
        renderer: &Renderer,
        bitmap: &windows::Win32::Graphics::Direct2D::ID2D1Bitmap,
        filter: BackdropFilter,
    ) -> windows::core::Result<windows::Win32::Graphics::Direct2D::ID2D1Effect> {
        match filter {
            BackdropFilter::Blur { radius } => renderer.apply_gaussian_blur(bitmap, radius),
            BackdropFilter::Custom(effect) => {
                // Create effect by CLSID from the trait object
                let clsid = effect.clsid();
                let d2d_effect = unsafe { renderer.render_target.CreateEffect(&clsid)? };
                unsafe {
                    d2d_effect.SetInput(0, Some(&bitmap.cast::<ID2D1Image>()?), false);
                }

                // Apply properties from the effect
                for prop in effect.properties() {
                    Self::set_effect_property(&d2d_effect, prop)?;
                }

                Ok(d2d_effect)
            }
        }
    }

    /// Set a single property on an effect.
    fn set_effect_property(
        effect: &windows::Win32::Graphics::Direct2D::ID2D1Effect,
        prop: crate::gfx::effects::EffectProperty,
    ) -> windows::core::Result<()> {
        use crate::gfx::effects::EffectProperty;
        use windows::Win32::Graphics::Direct2D::{
            D2D1_PROPERTY_TYPE_BOOL, D2D1_PROPERTY_TYPE_FLOAT, D2D1_PROPERTY_TYPE_INT32,
            D2D1_PROPERTY_TYPE_UINT32, D2D1_PROPERTY_TYPE_VECTOR2, D2D1_PROPERTY_TYPE_VECTOR3,
            D2D1_PROPERTY_TYPE_VECTOR4,
        };

        unsafe {
            match prop {
                EffectProperty::Float { index, value } => {
                    effect.SetValue(
                        index,
                        D2D1_PROPERTY_TYPE_FLOAT,
                        std::slice::from_raw_parts(
                            &value as *const f32 as *const u8,
                            std::mem::size_of::<f32>(),
                        ),
                    )?;
                }
                EffectProperty::Float2 { index, value } => {
                    effect.SetValue(
                        index,
                        D2D1_PROPERTY_TYPE_VECTOR2,
                        std::slice::from_raw_parts(
                            value.as_ptr() as *const u8,
                            std::mem::size_of::<[f32; 2]>(),
                        ),
                    )?;
                }
                EffectProperty::Float3 { index, value } => {
                    effect.SetValue(
                        index,
                        D2D1_PROPERTY_TYPE_VECTOR3,
                        std::slice::from_raw_parts(
                            value.as_ptr() as *const u8,
                            std::mem::size_of::<[f32; 3]>(),
                        ),
                    )?;
                }
                EffectProperty::Float4 { index, value } => {
                    effect.SetValue(
                        index,
                        D2D1_PROPERTY_TYPE_VECTOR4,
                        std::slice::from_raw_parts(
                            value.as_ptr() as *const u8,
                            std::mem::size_of::<[f32; 4]>(),
                        ),
                    )?;
                }
                EffectProperty::Int { index, value } => {
                    effect.SetValue(
                        index,
                        D2D1_PROPERTY_TYPE_INT32,
                        std::slice::from_raw_parts(
                            &value as *const i32 as *const u8,
                            std::mem::size_of::<i32>(),
                        ),
                    )?;
                }
                EffectProperty::UInt { index, value } => {
                    effect.SetValue(
                        index,
                        D2D1_PROPERTY_TYPE_UINT32,
                        std::slice::from_raw_parts(
                            &value as *const u32 as *const u8,
                            std::mem::size_of::<u32>(),
                        ),
                    )?;
                }
                EffectProperty::Bool { index, value } => {
                    let bool_val: i32 = if value { 1 } else { 0 };
                    effect.SetValue(
                        index,
                        D2D1_PROPERTY_TYPE_BOOL,
                        std::slice::from_raw_parts(
                            &bool_val as *const i32 as *const u8,
                            std::mem::size_of::<i32>(),
                        ),
                    )?;
                }
            }
        }
        Ok(())
    }
}
