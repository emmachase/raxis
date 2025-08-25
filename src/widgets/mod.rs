use std::{any::Any, time::Instant};

use smol_str::SmolStr;
use windows::Win32::{
    Foundation::HWND,
    Graphics::Direct2D::{
        CLSID_D2D1Shadow,
        Common::{
            D2D_RECT_F, D2D_SIZE_F, D2D1_COLOR_F, D2D1_COMPOSITE_MODE_SOURCE_OVER,
            D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
        },
        D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_SMALL, D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE,
        D2D1_INTERPOLATION_MODE_LINEAR, D2D1_PROPERTY_TYPE_FLOAT, D2D1_PROPERTY_TYPE_VECTOR4,
        D2D1_ROUNDED_RECT, D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION, D2D1_SHADOW_PROP_COLOR,
        D2D1_SWEEP_DIRECTION_CLOCKWISE, ID2D1DeviceContext7, ID2D1Factory, ID2D1GeometrySink,
        ID2D1Image, ID2D1SolidColorBrush,
    },
    System::Ole::DROPEFFECT,
};
use windows_core::{IUnknown, Interface};
use windows_numerics::{Vector2, Vector4};

use crate::{
    Shell,
    gfx::{PointDIP, RectDIP},
    layout::{
        BorrowedUITree,
        model::{ElementContent, UIElement},
        visitors,
    },
    runtime::DeviceResources,
};

pub use dragdrop::{DragData, DragInfo, DropResult, WidgetDragDropTarget};

pub mod button;
pub mod dragdrop;
pub mod drop_target;
pub mod spinner;
pub mod text;
pub mod text_input;

pub mod limit_response {
    pub struct SizingForX {
        pub min_width: f32,
        pub preferred_width: f32,
    }

    pub struct SizingForY {
        pub min_height: f32,
        pub preferred_height: f32,
    }
}

pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

pub enum Event {
    ImeStartComposition,
    ImeComposition {
        text: String,
        caret_units: u32,
    },
    ImeCommit {
        text: String,
    },
    ImeEndComposition,
    MouseButtonDown {
        x: f32,
        y: f32,
        click_count: u32,
        modifiers: Modifiers,
    },
    MouseButtonUp {
        x: f32,
        y: f32,
        click_count: u32,
        modifiers: Modifiers,
    },
    MouseMove {
        x: f32,
        y: f32,
    },
    MouseWheel {
        x: f32,
        y: f32,
        wheel_delta: f32,
        modifiers: Modifiers,
    },
    KeyDown {
        key: u32,
        modifiers: Modifiers,
    },
    KeyUp {
        key: u32,
        modifiers: Modifiers,
    },
    Char {
        text: SmolStr,
    },
    Redraw {
        now: Instant,
    },
    DragFinish {
        effect: DROPEFFECT,
    },
}

pub enum DragEvent {
    DragEnter { drag_info: DragInfo },
    DragOver { drag_info: DragInfo },
    DragLeave,
    Drop { drag_info: DragInfo },
}

pub struct Renderer<'a> {
    pub factory: &'a ID2D1Factory,
    pub render_target: &'a ID2D1DeviceContext7,
    pub brush: &'a ID2D1SolidColorBrush,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Default for Color {
    fn default() -> Self {
        BLACK
    }
}

pub const BLACK: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

impl From<u32> for Color {
    fn from(color: u32) -> Self {
        Color {
            r: (0xFF & (color >> 24)) as f32 / 255.0,
            g: (0xFF & (color >> 16)) as f32 / 255.0,
            b: (0xFF & (color >> 8)) as f32 / 255.0,
            a: (0xFF & color) as f32 / 255.0,
        }
    }
}

impl Renderer<'_> {
    pub fn draw_blurred_shadow(
        &self,
        rect: &RectDIP,
        shadow: &crate::layout::model::DropShadow,
        border_radius: Option<&crate::layout::model::BorderRadius>,
    ) {
        unsafe {
            if shadow.blur_radius <= 0.0 {
                // Simple shadow without blur
                let shadow_rect = RectDIP {
                    x_dip: rect.x_dip + shadow.offset_x - shadow.spread_radius,
                    y_dip: rect.y_dip + shadow.offset_y - shadow.spread_radius,
                    width_dip: rect.width_dip + shadow.spread_radius * 2.0,
                    height_dip: rect.height_dip + shadow.spread_radius * 2.0,
                };
                if let Some(border_radius) = border_radius {
                    self.fill_rounded_rectangle(&shadow_rect, border_radius, shadow.color);
                } else {
                    self.fill_rectangle(&shadow_rect, shadow.color);
                }
                return;
            }

            // Create a bitmap render target for the shadow
            let expanded_size = D2D_SIZE_F {
                width: rect.width_dip + shadow.spread_radius * 2.0,
                height: rect.height_dip + shadow.spread_radius * 2.0,
            };

            if let Ok(bitmap_rt) = self.render_target.CreateCompatibleRenderTarget(
                Some(&expanded_size),
                None,
                None,
                D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE,
            ) {
                // Draw the shadow shape to the bitmap render target
                bitmap_rt.BeginDraw();
                bitmap_rt.Clear(Some(&D2D1_COLOR_F {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0, // Transparent background
                }));

                // Get brush from bitmap render target for drawing the shadow shape
                if let Ok(shadow_brush) = bitmap_rt.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: 1.0, // Full opacity white for the shadow shape
                    },
                    None,
                ) {
                    let shadow_rect_in_bitmap = RectDIP {
                        x_dip: 0.0,
                        y_dip: 0.0,
                        width_dip: rect.width_dip + shadow.spread_radius * 2.0,
                        height_dip: rect.height_dip + shadow.spread_radius * 2.0,
                    };

                    if let Some(border_radius) = border_radius {
                        // Draw rounded rectangle shadow shape
                        if border_radius.top_left == border_radius.top_right
                            && border_radius.top_right == border_radius.bottom_right
                            && border_radius.bottom_right == border_radius.bottom_left
                        {
                            // Use simple rounded rectangle
                            let rounded_rect =
                                windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                                    rect: D2D_RECT_F {
                                        left: shadow_rect_in_bitmap.x_dip,
                                        top: shadow_rect_in_bitmap.y_dip,
                                        right: shadow_rect_in_bitmap.x_dip
                                            + shadow_rect_in_bitmap.width_dip,
                                        bottom: shadow_rect_in_bitmap.y_dip
                                            + shadow_rect_in_bitmap.height_dip,
                                    },
                                    radiusX: border_radius.top_left,
                                    radiusY: border_radius.top_left,
                                };
                            bitmap_rt.FillRoundedRectangle(&rounded_rect, &shadow_brush);
                        } else {
                            // Create path geometry for complex rounded rectangle
                            if let Ok(path_geometry) = self.factory.CreatePathGeometry() {
                                if let Ok(sink) = path_geometry.Open() {
                                    self.create_rounded_rectangle_path(
                                        &sink,
                                        &shadow_rect_in_bitmap,
                                        border_radius,
                                    );
                                    let _ = sink.Close();
                                    bitmap_rt.FillGeometry(&path_geometry, &shadow_brush, None);
                                }
                            }
                        }
                    } else {
                        // Draw regular rectangle shadow shape
                        let shadow_rect_d2d = D2D_RECT_F {
                            left: shadow_rect_in_bitmap.x_dip,
                            top: shadow_rect_in_bitmap.y_dip,
                            right: shadow_rect_in_bitmap.x_dip + shadow_rect_in_bitmap.width_dip,
                            bottom: shadow_rect_in_bitmap.y_dip + shadow_rect_in_bitmap.height_dip,
                        };
                        bitmap_rt.FillRectangle(&shadow_rect_d2d, &shadow_brush);
                    }
                }

                let _ = bitmap_rt.EndDraw(None, None);

                // Get the bitmap from the render target
                if let Ok(bitmap) = bitmap_rt.GetBitmap() {
                    // Create Gaussian blur effect
                    if let Ok(blur_effect) = self.render_target.CreateEffect(&CLSID_D2D1Shadow) {
                        blur_effect.SetInput(0, &bitmap, true);
                        blur_effect
                            .SetValue(
                                D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION.0 as u32,
                                D2D1_PROPERTY_TYPE_FLOAT,
                                // Docs say this should be 3.0 but it seems more accurate at 2.0
                                &(shadow.blur_radius / 2.0).to_le_bytes(),
                            )
                            .unwrap();

                        blur_effect
                            .SetValue(
                                D2D1_SHADOW_PROP_COLOR.0 as u32,
                                D2D1_PROPERTY_TYPE_VECTOR4,
                                &(std::mem::transmute::<
                                    Vector4,
                                    [u8; std::mem::size_of::<Vector4>()],
                                >(Vector4 {
                                    X: shadow.color.r,
                                    Y: shadow.color.g,
                                    Z: shadow.color.b,
                                    W: shadow.color.a,
                                })),
                            )
                            .unwrap();

                        self.render_target.DrawImage(
                            &IUnknown::from(blur_effect).cast::<ID2D1Image>().unwrap(),
                            Some(&Vector2::new(
                                rect.x_dip + shadow.offset_x - shadow.spread_radius,
                                rect.y_dip + shadow.offset_y - shadow.spread_radius,
                            )),
                            None,
                            D2D1_INTERPOLATION_MODE_LINEAR,
                            D2D1_COMPOSITE_MODE_SOURCE_OVER,
                        );

                        // // Set blur radius (standard deviation)
                        // let blur_std_dev = shadow.blur_radius / 3.0;
                        // let _ = blur_effect.SetValue(
                        //     0, // D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION
                        //     &blur_std_dev as *const f32 as *const _,
                        //     std::mem::size_of::<f32>() as u32,
                        // );

                        // // Draw the blurred shadow with proper color
                        // let dest_point = D2D_POINT_2F {
                        //     x: rect.x_dip + shadow.offset_x - shadow.blur_radius * 3.0,
                        //     y: rect.y_dip + shadow.offset_y - shadow.blur_radius * 3.0,
                        // };

                        // // Set the shadow color by modulating the effect output
                        // let shadow_color = Color::from(shadow.color);
                        // device_context.SetPrimitiveBlend(
                        //     D2D1_PRIMITIVE_BLEND_SOURCE_OVER,
                        // );

                        // // Create a color matrix effect to apply the shadow color
                        // if let Ok(color_effect) =
                        //     device_context.CreateEffect(&CLSID_D2D1ColorMatrix)
                        // {
                        //     color_effect.SetInput(0, &blur_effect, true);

                        //     // Color matrix to apply shadow color
                        //     #[rustfmt::skip]
                        //     let color_matrix = [
                        //         0.0, 0.0, 0.0, 0.0,
                        //         0.0, 0.0, 0.0, 0.0,
                        //         0.0, 0.0, 0.0, 0.0,
                        //         shadow_color.r, shadow_color.g, shadow_color.b, shadow_color.a,
                        //         0.0, 0.0, 0.0, 0.0,
                        //     ];

                        //     let _ = color_effect.SetValue(
                        //         0, // D2D1_COLORMATRIX_PROP_COLOR_MATRIX
                        //         color_matrix.as_ptr() as *const _,
                        //         std::mem::size_of_val(&color_matrix) as u32,
                        //     );

                        //     device_context.DrawImage(
                        //         &color_effect,
                        //         Some(&dest_point),
                        //         None,
                        //         D2D1_INTERPOLATION_MODE_LINEAR,
                        //         D2D1_COMPOSITE_MODE_SOURCE_OVER,
                        //     );
                        // } else {
                        //     // Fallback: draw without color modulation
                        //     device_context.DrawImage(
                        //         &blur_effect,
                        //         Some(&dest_point),
                        //         None,
                        //         D2D1_INTERPOLATION_MODE_LINEAR,
                        //         D2D1_COMPOSITE_MODE_SOURCE_OVER,
                        //     );
                        // }
                    }
                }
            }
        }
    }

    // fn draw_simple_blurred_shadow(
    //     &self,
    //     rect: &RectDIP,
    //     shadow: &crate::layout::model::DropShadow,
    // ) {
    //     // Enhanced blur simulation using multiple layers with gaussian-like falloff
    //     let shadow_color = Color::from(shadow.color);
    //     let blur_steps = (shadow.blur_radius * 2.0).max(4.0) as i32;

    //     // Draw from outside to inside for proper layering
    //     for i in (0..blur_steps).rev() {
    //         let progress = i as f32 / blur_steps as f32;
    //         let offset = progress * shadow.blur_radius;

    //         // Gaussian-like falloff for more realistic blur
    //         let gaussian_factor = (-progress * progress * 4.0).exp();
    //         let alpha = shadow_color.a * gaussian_factor * 0.3; // Reduce overall opacity

    //         let blurred_rect = RectDIP {
    //             x_dip: rect.x_dip + shadow.offset_x - offset,
    //             y_dip: rect.y_dip + shadow.offset_y - offset,
    //             width_dip: rect.width_dip + offset * 2.0,
    //             height_dip: rect.height_dip + offset * 2.0,
    //         };

    //         let blurred_color = Color {
    //             r: shadow_color.r,
    //             g: shadow_color.g,
    //             b: shadow_color.b,
    //             a: alpha,
    //         };

    //         self.fill_rectangle(&blurred_rect, blurred_color);
    //     }
    // }

    pub fn draw_rectangle<C: Into<Color>>(&self, rect: &RectDIP, color: C, stroke_width: f32) {
        unsafe {
            let color = color.into();
            self.brush.SetColor(&D2D1_COLOR_F {
                r: color.r,
                g: color.g,
                b: color.b,
                a: color.a,
            });
            self.render_target.DrawRectangle(
                &D2D_RECT_F {
                    left: rect.x_dip,
                    top: rect.y_dip,
                    right: rect.x_dip + rect.width_dip,
                    bottom: rect.y_dip + rect.height_dip,
                },
                self.brush,
                stroke_width,
                None,
            );
        }
    }

    pub fn fill_rectangle<C: Into<Color>>(&self, rect: &RectDIP, color: C) {
        unsafe {
            let color: Color = color.into();
            self.brush.SetColor(&D2D1_COLOR_F {
                r: color.r,
                g: color.g,
                b: color.b,
                a: color.a,
            });
            self.render_target.FillRectangle(
                &D2D_RECT_F {
                    left: rect.x_dip,
                    top: rect.y_dip,
                    right: rect.x_dip + rect.width_dip,
                    bottom: rect.y_dip + rect.height_dip,
                },
                self.brush,
            );
        }
    }

    pub fn fill_rounded_rectangle<C: Into<Color>>(
        &self,
        rect: &RectDIP,
        border_radius: &crate::layout::model::BorderRadius,
        color: C,
    ) {
        unsafe {
            let color: Color = color.into();
            self.brush.SetColor(&D2D1_COLOR_F {
                r: color.r,
                g: color.g,
                b: color.b,
                a: color.a,
            });

            // Check if all corners have the same radius for simple case
            if border_radius.top_left == border_radius.top_right
                && border_radius.top_right == border_radius.bottom_right
                && border_radius.bottom_right == border_radius.bottom_left
            {
                // Use simple rounded rectangle
                let rounded_rect = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: rect.x_dip,
                        top: rect.y_dip,
                        right: rect.x_dip + rect.width_dip,
                        bottom: rect.y_dip + rect.height_dip,
                    },
                    radiusX: border_radius.top_left,
                    radiusY: border_radius.top_left,
                };
                self.render_target
                    .FillRoundedRectangle(&rounded_rect, self.brush);
            } else {
                // Create path geometry for complex rounded rectangle with different corner radii
                if let Ok(path_geometry) = self.factory.CreatePathGeometry() {
                    if let Ok(sink) = path_geometry.Open() {
                        self.create_rounded_rectangle_path(&sink, rect, border_radius);
                        let _ = sink.Close();
                        self.render_target
                            .FillGeometry(&path_geometry, self.brush, None);
                    }
                }
            }
        }
    }

    fn create_rounded_rectangle_path(
        &self,
        sink: &ID2D1GeometrySink,
        rect: &RectDIP,
        border_radius: &crate::layout::model::BorderRadius,
    ) {
        unsafe {
            let left = rect.x_dip;
            let top = rect.y_dip;
            let right = rect.x_dip + rect.width_dip;
            let bottom = rect.y_dip + rect.height_dip;

            // Clamp radii to prevent overlapping
            let max_radius_x = rect.width_dip / 2.0;
            let max_radius_y = rect.height_dip / 2.0;

            let tl = border_radius.top_left.min(max_radius_x).min(max_radius_y);
            let tr = border_radius.top_right.min(max_radius_x).min(max_radius_y);
            let br = border_radius
                .bottom_right
                .min(max_radius_x)
                .min(max_radius_y);
            let bl = border_radius
                .bottom_left
                .min(max_radius_x)
                .min(max_radius_y);

            // Start from top-left corner (after the radius)
            sink.BeginFigure(
                Vector2 {
                    X: left + tl,
                    Y: top,
                },
                D2D1_FIGURE_BEGIN_FILLED,
            );

            // Top edge to top-right corner
            if tr > 0.0 {
                sink.AddLine(Vector2 {
                    X: right - tr,
                    Y: top,
                });
                // Top-right arc
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: Vector2 {
                        X: right,
                        Y: top + tr,
                    },
                    size: D2D_SIZE_F {
                        width: tr,
                        height: tr,
                    },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
            } else {
                sink.AddLine(Vector2 { X: right, Y: top });
            }

            // Right edge to bottom-right corner
            if br > 0.0 {
                sink.AddLine(Vector2 {
                    X: right,
                    Y: bottom - br,
                });
                // Bottom-right arc
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: Vector2 {
                        X: right - br,
                        Y: bottom,
                    },
                    size: D2D_SIZE_F {
                        width: br,
                        height: br,
                    },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
            } else {
                sink.AddLine(Vector2 {
                    X: right,
                    Y: bottom,
                });
            }

            // Bottom edge to bottom-left corner
            if bl > 0.0 {
                sink.AddLine(Vector2 {
                    X: left + bl,
                    Y: bottom,
                });
                // Bottom-left arc
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: Vector2 {
                        X: left,
                        Y: bottom - bl,
                    },
                    size: D2D_SIZE_F {
                        width: bl,
                        height: bl,
                    },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
            } else {
                sink.AddLine(Vector2 { X: left, Y: bottom });
            }

            // Left edge to top-left corner
            if tl > 0.0 {
                sink.AddLine(Vector2 {
                    X: left,
                    Y: top + tl,
                });
                // Top-left arc
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: Vector2 {
                        X: left + tl,
                        Y: top,
                    },
                    size: D2D_SIZE_F {
                        width: tl,
                        height: tl,
                    },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
            } else {
                sink.AddLine(Vector2 { X: left, Y: top });
            }

            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        }
    }
}

pub enum Cursor {
    Arrow,
    IBeam,
}

pub type State = Option<Box<dyn Any>>;

#[macro_export]
macro_rules! with_state {
    ($instance:ident as $state:ty) => {
        $instance
            .state
            .as_ref()
            .unwrap()
            .downcast_ref::<$state>()
            .unwrap()
    };

    (mut $instance:ident as $state:ty) => {
        $instance
            .state
            .as_mut()
            .unwrap()
            .downcast_mut::<$state>()
            .unwrap()
    };
}

pub struct Instance {
    id: u64,
    state: State,
}

impl Instance {
    pub fn new(id: u64, widget: &dyn Widget, device_resources: &DeviceResources) -> Self {
        Self {
            id,
            state: widget.state(device_resources),
        }
    }
}

#[allow(unused)]
pub trait Widget: std::fmt::Debug {
    fn limits_x(&self, instance: &mut Instance) -> limit_response::SizingForX;
    fn limits_y(&self, instance: &mut Instance, width: f32) -> limit_response::SizingForY;

    fn state(&self, device_resources: &DeviceResources) -> State {
        None
    }

    fn paint(
        &mut self, // TODO: this shouldnt need to be mut right
        // id: Option<u64>,
        // ui_key: UIKey,
        instance: &mut Instance,
        shell: &Shell,
        renderer: &Renderer,
        bounds: RectDIP,
        now: Instant,
    );

    fn update(
        &mut self,
        instance: &mut Instance,
        hwnd: HWND,
        shell: &mut Shell,
        event: &Event,
        bounds: RectDIP,
    );

    fn cursor(&self, instance: &Instance, point: PointDIP, bounds: RectDIP) -> Option<Cursor> {
        None
    }

    fn operate(&mut self, instance: &mut Instance, operation: &dyn Operation) {}

    fn as_drop_target(&mut self) -> Option<&mut dyn WidgetDragDropTarget> {
        None
    }

    // fn capture_device_resources(
    //     &mut self,
    //     instance: &mut Instance,
    // ) {
    // }
}

// pub trait Focusable {
//     fn focus(&mut self);
//     fn unfocus(&mut self);
// }

#[allow(unused)]
pub trait Operation {
    // fn focusable(&self, focusable: &mut dyn Focusable, id: Option<u64>, key: UIKey) {}
}

pub fn dispatch_operation(ui_tree: BorrowedUITree, operation: &dyn Operation) {
    visitors::visit_bfs(ui_tree, ui_tree.root, |ui_tree, key, _| {
        let element = &mut ui_tree.slots[key];
        if let Some(ElementContent::Widget(widget)) = element.content.as_mut() {
            if let Some(id) = element.id {
                let instance = ui_tree.widget_state.get_mut(&id).unwrap();
                widget.operate(instance, operation);
            }
        }
    });
}

impl UIElement {
    pub fn bounds(&self) -> RectDIP {
        RectDIP {
            x_dip: self.x,
            y_dip: self.y,
            width_dip: self.computed_width,
            height_dip: self.computed_height,
        }
    }
}
