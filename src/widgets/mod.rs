use smol_str::SmolStr;
use windows::Win32::{
    Foundation::HWND,
    Graphics::Direct2D::{ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush},
};

use crate::{
    Shell,
    gfx::{PointDIP, RectDIP},
    layout::model::{UIElement, UIKey},
};

pub mod spinner;
pub mod text_input;

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
}

pub struct Renderer<'a> {
    pub factory: &'a ID2D1Factory,
    pub render_target: &'a ID2D1HwndRenderTarget,
    pub brush: &'a ID2D1SolidColorBrush,
}

pub enum Cursor {
    Arrow,
    IBeam,
}

pub trait Widget: std::fmt::Debug {
    fn limits(&self, available: Limits) -> Limits;

    fn paint(
        &mut self, // TODO: this shouldnt need to be mut right
        renderer: &Renderer,
        bounds: RectDIP,
        dt: f64,
    );

    fn update(&mut self, key: UIKey, hwnd: HWND, shell: &mut Shell, event: &Event, bounds: RectDIP);

    #[allow(unused)]
    fn cursor(&self, key: UIKey, point: PointDIP, bounds: RectDIP) -> Option<Cursor> {
        None
    }
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
