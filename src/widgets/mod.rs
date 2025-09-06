use std::{any::Any, time::Instant};

use smol_str::SmolStr;
use windows::Win32::{Foundation::HWND, System::Ole::DROPEFFECT};

use crate::{
    Shell,
    gfx::{PointDIP, RectDIP},
    layout::{
        BorrowedUITree, UIArenas,
        model::{UIElement, WidgetContent},
        visitors,
    },
    runtime::DeviceResources,
};

pub use dragdrop::{DragData, DragInfo, DropResult, WidgetDragDropTarget};

pub mod button;
pub mod dragdrop;
pub mod drop_target;
pub mod mouse_area;
pub mod renderer;
pub mod spinner;
pub mod svg;
pub mod svg_path;
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

#[derive(Debug, Clone, Copy)]
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

#[derive(Debug)]
pub enum DragEvent {
    DragEnter { drag_info: DragInfo },
    DragOver { drag_info: DragInfo },
    DragLeave,
    Drop { drag_info: DragInfo },
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
    pub fn new<Message>(
        id: u64,
        widget: &dyn Widget<Message>,
        arenas: &UIArenas,
        device_resources: &DeviceResources,
    ) -> Self {
        Self {
            id,
            state: widget.state(arenas, device_resources),
        }
    }
}

#[allow(unused)]
pub trait Widget<Message>: std::fmt::Debug {
    fn limits_x(&self, arenas: &UIArenas, instance: &mut Instance) -> limit_response::SizingForX;
    fn limits_y(
        &self,
        arenas: &UIArenas,
        instance: &mut Instance,
        border_width: f32,
        content_width: f32,
    ) -> limit_response::SizingForY;

    fn state(&self, arenas: &UIArenas, device_resources: &DeviceResources) -> State {
        None
    }

    fn paint(
        &mut self,
        arenas: &UIArenas,
        instance: &mut Instance,
        shell: &Shell<Message>,
        recorder: &mut crate::gfx::command_recorder::CommandRecorder,
        bounds: Bounds,
        now: Instant,
    );

    fn update(
        &mut self,
        arenas: &mut UIArenas,
        instance: &mut Instance,
        hwnd: HWND,
        shell: &mut Shell<Message>,
        event: &Event,
        bounds: Bounds,
    );

    fn cursor(
        &self,
        arenas: &UIArenas,
        instance: &Instance,
        point: PointDIP,
        bounds: Bounds,
    ) -> Option<Cursor> {
        None
    }

    fn operate(&mut self, arenas: &UIArenas, instance: &mut Instance, operation: &dyn Operation) {}

    fn as_drop_target(&mut self) -> Option<&mut dyn WidgetDragDropTarget<Message>> {
        None
    }

    // fn capture_device_resources(
    //     &mut self,
    //     instance: &mut Instance,
    // ) {
    // }
}

pub fn widget<Message>(widget: impl Widget<Message> + 'static) -> Option<WidgetContent<Message>> {
    Some(Box::new(widget))
}

// pub trait Focusable {
//     fn focus(&mut self);
//     fn unfocus(&mut self);
// }

#[allow(unused)]
pub trait Operation {
    // fn focusable(&self, focusable: &mut dyn Focusable, id: Option<u64>, key: UIKey) {}
}

pub fn dispatch_operation<Message>(ui_tree: BorrowedUITree<Message>, operation: &dyn Operation) {
    visitors::visit_bfs(ui_tree, ui_tree.root, |ui_tree, key, _| {
        let element = &mut ui_tree.slots[key];
        if let Some(widget) = element.content.as_mut() {
            if let Some(id) = element.id {
                let instance = ui_tree.widget_state.get_mut(&id).unwrap();
                widget.operate(&ui_tree.arenas, instance, operation);
            }
        }
    });
}

pub struct Bounds {
    pub content_box: RectDIP,
    pub border_box: RectDIP,
}

impl<Message> UIElement<Message> {
    pub fn bounds(&self) -> Bounds {
        Bounds {
            content_box: RectDIP {
                x: self.x + self.padding.left,
                y: self.y + self.padding.top,
                width: self.computed_width - self.padding.left - self.padding.right,
                height: self.computed_height - self.padding.top - self.padding.bottom,
            },
            border_box: RectDIP {
                x: self.x,
                y: self.y,
                width: self.computed_width,
                height: self.computed_height,
            },
        }
    }
}
