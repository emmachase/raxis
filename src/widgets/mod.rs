use std::{any::Any, time::Instant};

use smol_str::SmolStr;
use windows::Win32::{
    Foundation::HWND,
    System::Ole::DROPEFFECT,
    UI::WindowsAndMessaging::{
        IDC_APPSTARTING, IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_HELP, IDC_IBEAM, IDC_NO, IDC_SIZEALL,
        IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, IDC_UPARROW, IDC_WAIT, LoadCursorW,
        SetCursor,
    },
};

use crate::{
    Shell,
    gfx::{PointDIP, RectDIP, command_recorder::CommandRecorder},
    layout::{
        BorrowedUITree, UIArenas,
        model::{ElementStyle, UIElement, WidgetContent},
        visitors,
    },
    runtime::{DeviceResources, vkey::VKey},
};

pub use dragdrop::{DragData, DragInfo, DropResult, WidgetDragDropTarget};

pub mod button;
pub mod dragdrop;
pub mod drop_target;
pub mod image;
pub mod mouse_area;
pub mod renderer;
pub mod rule;
pub mod slider;
pub mod spinner;
pub mod svg;
pub mod svg_path;
pub mod text;
pub mod text_input;
pub mod titlebar_controls;
pub mod toggle;

pub mod limit_response {
    #[derive(Debug, Default, Clone, Copy)]
    pub struct SizingForX {
        pub min_width: f32,
        pub preferred_width: f32,
    }

    #[derive(Debug, Default, Clone, Copy)]
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

#[non_exhaustive]
#[derive(Debug)]
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
    MouseEnter {
        x: f32,
        y: f32,
    },
    MouseLeave {
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
        key: VKey,
        modifiers: Modifiers,
    },
    KeyUp {
        key: VKey,
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

impl Event {
    pub fn is_mouse_event(&self) -> bool {
        matches!(
            self,
            Event::MouseButtonDown { .. }
                | Event::MouseButtonUp { .. }
                | Event::MouseMove { .. }
                | Event::MouseEnter { .. }
                | Event::MouseLeave { .. }
                | Event::MouseWheel { .. }
        )
    }

    pub fn mouse_position(&self) -> Option<(f32, f32)> {
        match self {
            Event::MouseButtonDown { x, y, .. } => Some((*x, *y)),
            Event::MouseButtonUp { x, y, .. } => Some((*x, *y)),
            Event::MouseMove { x, y } => Some((*x, *y)),
            Event::MouseEnter { x, y } => Some((*x, *y)),
            Event::MouseLeave { x, y } => Some((*x, *y)),
            Event::MouseWheel { x, y, .. } => Some((*x, *y)),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum DragEvent {
    DragEnter { drag_info: DragInfo },
    DragOver { drag_info: DragInfo },
    DragLeave,
    Drop { drag_info: DragInfo },
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub enum Cursor {
    Arrow,
    IBeam,
    Pointer,
    Cross,
    Hand,
    Help,
    No,
    SizeNS,
    SizeEW,
    SizeNWSE,
    SizeNESW,
    SizeAll,
    UpArrow,
    Wait,
    AppStarting,
}

impl Cursor {
    pub fn set(self) {
        unsafe {
            match self {
                Cursor::Arrow => {
                    let _ = SetCursor(Some(LoadCursorW(None, IDC_ARROW).unwrap()));
                }
                Cursor::IBeam => {
                    let _ = SetCursor(Some(LoadCursorW(None, IDC_IBEAM).unwrap()));
                }
                Cursor::Pointer => {
                    let _ = SetCursor(Some(LoadCursorW(None, IDC_HAND).unwrap()));
                }
                Cursor::Cross => {
                    let _ = SetCursor(Some(LoadCursorW(None, IDC_CROSS).unwrap()));
                }
                Cursor::Hand => {
                    let _ = SetCursor(Some(LoadCursorW(None, IDC_HAND).unwrap()));
                }
                Cursor::Help => {
                    let _ = SetCursor(Some(LoadCursorW(None, IDC_HELP).unwrap()));
                }
                Cursor::No => {
                    let _ = SetCursor(Some(LoadCursorW(None, IDC_NO).unwrap()));
                }
                Cursor::SizeNS => {
                    let _ = SetCursor(Some(LoadCursorW(None, IDC_SIZENS).unwrap()));
                }
                Cursor::SizeEW => {
                    let _ = SetCursor(Some(LoadCursorW(None, IDC_SIZEWE).unwrap()));
                }
                Cursor::SizeNWSE => {
                    let _ = SetCursor(Some(LoadCursorW(None, IDC_SIZENWSE).unwrap()));
                }
                Cursor::SizeNESW => {
                    let _ = SetCursor(Some(LoadCursorW(None, IDC_SIZENESW).unwrap()));
                }
                Cursor::SizeAll => {
                    let _ = SetCursor(Some(LoadCursorW(None, IDC_SIZEALL).unwrap()));
                }
                Cursor::UpArrow => {
                    let _ = SetCursor(Some(LoadCursorW(None, IDC_UPARROW).unwrap()));
                }
                Cursor::Wait => {
                    let _ = SetCursor(Some(LoadCursorW(None, IDC_WAIT).unwrap()));
                }
                Cursor::AppStarting => {
                    let _ = SetCursor(Some(LoadCursorW(None, IDC_APPSTARTING).unwrap()));
                }
            }
        }
    }
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
    pub id: u64,
    pub state: State,
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

pub enum PaintOwnership {
    Contents,
    Full,
}

#[allow(unused)]
pub trait Widget<Message>: std::fmt::Debug {
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn limits_x(&self, arenas: &UIArenas, instance: &mut Instance) -> limit_response::SizingForX {
        limit_response::SizingForX::default()
    }

    fn limits_y(
        &self,
        arenas: &UIArenas,
        instance: &mut Instance,
        border_width: f32,
        content_width: f32,
    ) -> limit_response::SizingForY {
        limit_response::SizingForY::default()
    }

    fn state(&self, arenas: &UIArenas, device_resources: &DeviceResources) -> State {
        None
    }

    fn paint_ownership(&self) -> PaintOwnership {
        PaintOwnership::Contents
    }

    fn adjust_style(
        &mut self,
        instance: &mut Instance,
        shell: &mut Shell<Message>,
        style: ElementStyle,
    ) -> ElementStyle {
        style
    }

    fn paint(
        &mut self,
        arenas: &UIArenas,
        instance: &mut Instance,
        shell: &mut Shell<Message>,
        recorder: &mut CommandRecorder,
        style: ElementStyle,
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

    /// Returns the rectangle that should be kept visible when this widget is focused.
    /// Used by scroll containers to automatically scroll focused content into view.
    /// The returned rect should be in widget-local coordinates (relative to content_box origin).
    fn focus_rect(&self, instance: &Instance) -> Option<RectDIP> {
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
        if let Some(widget) = element.content.as_mut()
            && let Some(id) = element.id
        {
            let instance = ui_tree.widget_state.get_mut(&id).unwrap();
            widget.operate(&ui_tree.arenas, instance, operation);
        }
    });
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
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
