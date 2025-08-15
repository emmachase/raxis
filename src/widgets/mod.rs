use smol_str::SmolStr;
use windows::Win32::{
    Foundation::HWND,
    Graphics::Direct2D::{ID2D1HwndRenderTarget, ID2D1SolidColorBrush},
};

use crate::{gfx::RectDIP, layout::model::UIElement};

pub mod selectable_text;
pub mod spinner;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub min: Size,
    pub max: Size,
}

pub enum Event {
    // ImeStartComposition,
    // ImeComposition {
    //     text: SmolStr,
    //     caret_units: u32,
    //     position: OneShot
    // },
    // ImeEndComposition,
    MouseButtonDown { x: f32, y: f32, click_count: u32 },
    MouseButtonUp { x: f32, y: f32, click_count: u32 },
    MouseMove { x: f32, y: f32 },
    KeyDown { key: u32 },
    KeyUp { key: u32 },
    Char { text: SmolStr },
}

pub trait Widget: std::fmt::Debug {
    fn limits(&self, available: Limits) -> Limits;

    fn paint(
        &mut self, // TODO: this shouldnt need to be mut right
        render_target: &ID2D1HwndRenderTarget,
        brush: &ID2D1SolidColorBrush,
        bounds: RectDIP,
        dt: f64,
    );

    fn update(&mut self, hwnd: HWND, event: Event, bounds: RectDIP);
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
