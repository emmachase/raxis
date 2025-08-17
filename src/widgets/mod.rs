use std::time::Instant;

use smol_str::SmolStr;
use windows::Win32::{
    Foundation::HWND,
    Graphics::Direct2D::{
        Common::{D2D_RECT_F, D2D1_COLOR_F},
        ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
    },
};

use crate::{
    Shell,
    gfx::{PointDIP, RectDIP},
    layout::{
        BorrowedUITree,
        model::{ElementContent, UIElement, UIKey},
        visitors,
    },
};

pub use dragdrop::{DragData, DragInfo, DropResult, WidgetDragDropTarget};

pub mod dragdrop;
pub mod integrated_drop_target;
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
    Redraw {
        now: Instant,
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
    pub render_target: &'a ID2D1HwndRenderTarget,
    pub brush: &'a ID2D1SolidColorBrush,
}

pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
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
    pub fn fill_rectangle<C: Into<Color>>(&self, rect: &RectDIP, color: C) {
        unsafe {
            let color = color.into();
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

pub trait Widget: std::fmt::Debug {
    fn limits(&self, available: Limits) -> Limits;

    fn paint(
        &mut self, // TODO: this shouldnt need to be mut right
        id: Option<u64>,
        ui_key: UIKey,
        shell: &Shell,
        renderer: &Renderer,
        bounds: RectDIP,
        now: Instant,
    );

    fn update(
        &mut self,
        id: Option<u64>,
        key: UIKey,
        hwnd: HWND,
        shell: &mut Shell,
        event: &Event,
        bounds: RectDIP,
    );

    #[allow(unused)]
    fn cursor(
        &self,
        id: Option<u64>,
        key: UIKey,
        point: PointDIP,
        bounds: RectDIP,
    ) -> Option<Cursor> {
        None
    }

    #[allow(unused)]
    fn operate(&mut self, id: Option<u64>, key: UIKey, operation: &dyn Operation) {}

    fn as_drop_target(&mut self) -> Option<&mut dyn WidgetDragDropTarget> {
        None
    }
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
    let root = ui_tree.keys().next().unwrap();
    visitors::visit_bfs(ui_tree, root, |ui_tree, key, _| {
        let element = &mut ui_tree[key];
        if let Some(ElementContent::Widget(widget)) = element.content.as_mut() {
            widget.operate(element.id, key, operation);
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
