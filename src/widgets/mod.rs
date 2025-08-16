use smol_str::SmolStr;
use windows::Win32::{
    Foundation::HWND,
    Graphics::Direct2D::{ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush},
};

use crate::{
    Shell,
    gfx::{PointDIP, RectDIP},
    layout::{
        BorrowedUITree, OwnedUITree,
        model::{ElementContent, UIElement, UIKey},
        visitors,
    },
};

pub use dragdrop::{DragData, DragDropWidget, DragInfo, DropResult};

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
    // Drag and drop events
    DragEnter {
        drag_info: DragInfo,
    },
    DragOver {
        drag_info: DragInfo,
    },
    DragLeave,
    Drop {
        drag_info: DragInfo,
    },
    DragStart {
        data: DragData,
    },
    DragEnd {
        data: DragData,
        effect: windows::Win32::System::Ole::DROPEFFECT,
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
        id: Option<u64>,
        ui_key: UIKey,
        shell: &Shell,
        renderer: &Renderer,
        bounds: RectDIP,
        dt: f64,
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

    /// Allow downcasting to concrete widget types
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
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
