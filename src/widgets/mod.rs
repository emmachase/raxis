use std::{any::Any, time::Instant};

use smol_str::SmolStr;
use windows::Win32::{
    Foundation::HWND,
    Graphics::Direct2D::{
        CLSID_D2D1Shadow,
        Common::{
            D2D_RECT_F, D2D_SIZE_F, D2D_VECTOR_3F, D2D1_COLOR_F, D2D1_COMPOSITE_MODE_SOURCE_OVER,
        },
        D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE, D2D1_INTERPOLATION_MODE_LINEAR,
        D2D1_PROPERTY_TYPE_FLOAT, D2D1_PROPERTY_TYPE_VECTOR4, D2D1_RENDER_TARGET_USAGE_NONE,
        D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION, D2D1_SHADOW_PROP_COLOR, ID2D1DeviceContext7,
        ID2D1Factory, ID2D1Image, ID2D1SolidColorBrush,
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
    pub fn draw_blurred_shadow(&self, rect: &RectDIP, shadow: &crate::layout::model::DropShadow) {
        unsafe {
            if shadow.blur_radius <= 0.0 {
                // Simple shadow without blur
                let shadow_rect = RectDIP {
                    x_dip: rect.x_dip + shadow.offset_x,
                    y_dip: rect.y_dip + shadow.offset_y,
                    width_dip: rect.width_dip,
                    height_dip: rect.height_dip,
                };
                let shadow_color = Color::from(shadow.color);
                self.fill_rectangle(&shadow_rect, shadow_color);
                return;
            }

            // Create a bitmap render target for the shadow
            let expanded_size = D2D_SIZE_F {
                width: rect.width_dip + shadow.spread_radius * 2.0, //+ shadow.blur_radius * 6.0,
                height: rect.height_dip + shadow.spread_radius * 2.0, //+ shadow.blur_radius * 6.0,
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
                    a: 1.0,
                }));

                // // Get brush from bitmap render target
                // if let Ok(shadow_brush) = bitmap_rt.CreateSolidColorBrush(
                //     &D2D1_COLOR_F {
                //         r: 0.0,
                //         g: 0.0,
                //         b: 0.0,
                //         a: 1.0, // Full opacity for the shadow shape
                //     },
                //     None,
                // ) {
                //     let shadow_rect_in_bitmap = D2D_RECT_F {
                //         left: shadow.blur_radius * 3.0,
                //         top: shadow.blur_radius * 3.0,
                //         right: shadow.blur_radius * 3.0 + rect.width_dip,
                //         bottom: shadow.blur_radius * 3.0 + rect.height_dip,
                //     };

                //     bitmap_rt.FillRectangle(&shadow_rect_in_bitmap, &shadow_brush);
                // }

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
                        //     windows::Win32::Graphics::Direct2D::D2D1_PRIMITIVE_BLEND_SOURCE_OVER,
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
                        //         windows::Win32::Graphics::Direct2D::D2D1_INTERPOLATION_MODE_LINEAR,
                        //         windows::Win32::Graphics::Direct2D::D2D1_COMPOSITE_MODE_SOURCE_OVER,
                        //     );
                        // } else {
                        //     // Fallback: draw without color modulation
                        //     device_context.DrawImage(
                        //         &blur_effect,
                        //         Some(&dest_point),
                        //         None,
                        //         windows::Win32::Graphics::Direct2D::D2D1_INTERPOLATION_MODE_LINEAR,
                        //         windows::Win32::Graphics::Direct2D::D2D1_COMPOSITE_MODE_SOURCE_OVER,
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
    fn limits_x(&self, instance: &Instance) -> limit_response::SizingForX;
    fn limits_y(&self, instance: &Instance, width: f32) -> limit_response::SizingForY;

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
