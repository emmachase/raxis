use std::fmt::Debug;
use std::time::Instant;

use windows::Win32::Foundation::HWND;

use crate::gfx::PointDIP;
use crate::layout::UIArenas;
use crate::layout::model::{Element, ElementStyle};
use crate::runtime::DeviceResources;
use crate::widgets::{Bounds, Cursor, Event, Instance, Modifiers, State, Widget, widget};
use crate::{Shell, with_state};

/// Events that can be emitted by the MouseArea widget
#[derive(Debug, Clone)]
pub enum MouseAreaEvent {
    /// Mouse button pressed down
    MouseButtonDown {
        x: f32,
        y: f32,
        click_count: u32,
        modifiers: Modifiers,
    },
    /// Mouse button released
    MouseButtonUp {
        x: f32,
        y: f32,
        click_count: u32,
        modifiers: Modifiers,

        inside: bool,
    },
    /// Mouse moved within the area
    MouseMove { x: f32, y: f32, inside: bool },
    /// Mouse wheel scrolled within the area
    MouseWheel {
        x: f32,
        y: f32,
        wheel_delta: f32,
        modifiers: Modifiers,

        inside: bool,
    },
    /// Mouse entered the area (synthetic event)
    MouseEntered { x: f32, y: f32 },
    /// Mouse left the area (synthetic event)
    MouseLeft { x: f32, y: f32 },
}

/// Internal state for MouseArea to track mouse presence
#[derive(Debug, Default)]
struct MouseAreaState {
    mouse_inside: bool,
    mouse_held: bool,
    last_mouse_pos: Option<(f32, f32)>,
}

#[derive(Debug)]
pub struct CaptureFor {
    pub mouse_up: bool,
    pub mouse_move: bool,
    pub mouse_wheel: bool,
}

impl CaptureFor {
    pub fn none() -> Self {
        Self {
            mouse_up: false,
            mouse_move: false,
            mouse_wheel: false,
        }
    }

    pub fn all() -> Self {
        Self {
            mouse_up: true,
            mouse_move: true,
            mouse_wheel: true,
        }
    }
}

impl Default for CaptureFor {
    fn default() -> Self {
        Self::all()
    }
}

pub type OnMouseAreaEventFn<Message> =
    dyn Fn(MouseAreaEvent, &mut Shell<Message>) -> Option<Message>;

/// MouseArea widget - invisible container that captures mouse events
pub struct MouseArea<Message> {
    event_handler: Option<Box<OnMouseAreaEventFn<Message>>>,
    capture_for: CaptureFor,
}

impl<Message> Debug for MouseArea<Message> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MouseArea").finish()
    }
}

impl<Message: 'static> MouseArea<Message> {
    pub fn new<F>(handler: F) -> Self
    where
        F: Fn(MouseAreaEvent, &mut Shell<Message>) -> Option<Message> + 'static,
    {
        Self {
            event_handler: Some(Box::new(handler)),
            capture_for: CaptureFor::all(),
        }
    }

    pub fn capture_for(&mut self, capture_for: CaptureFor) {
        self.capture_for = capture_for;
    }

    /// Convert framework Event to MouseAreaEvent if applicable
    fn map_event(
        &self,
        state: &MouseAreaState,
        event: &Event,
        bounds: &Bounds,
    ) -> Option<MouseAreaEvent> {
        match event {
            Event::MouseButtonDown {
                x,
                y,
                click_count,
                modifiers,
            } => {
                let point = PointDIP { x: *x, y: *y };
                let inside = point.within(bounds.border_box);
                if inside {
                    Some(MouseAreaEvent::MouseButtonDown {
                        x: *x,
                        y: *y,
                        click_count: *click_count,
                        modifiers: *modifiers,
                    })
                } else {
                    None
                }
            }
            Event::MouseButtonUp {
                x,
                y,
                click_count,
                modifiers,
            } => {
                let point = PointDIP { x: *x, y: *y };
                let inside = point.within(bounds.border_box);
                if inside || (state.mouse_held && self.capture_for.mouse_up) {
                    Some(MouseAreaEvent::MouseButtonUp {
                        x: *x,
                        y: *y,
                        click_count: *click_count,
                        modifiers: *modifiers,
                        inside,
                    })
                } else {
                    None
                }
            }
            Event::MouseMove { x, y } => {
                let point = PointDIP { x: *x, y: *y };
                let inside = point.within(bounds.border_box);
                if inside || (state.mouse_held && self.capture_for.mouse_move) {
                    Some(MouseAreaEvent::MouseMove {
                        x: *x,
                        y: *y,
                        inside,
                    })
                } else {
                    None
                }
            }
            Event::MouseWheel {
                x,
                y,
                wheel_delta,
                modifiers,
            } => {
                let point = PointDIP { x: *x, y: *y };
                let inside = point.within(bounds.border_box);
                if inside || (state.mouse_held && self.capture_for.mouse_wheel) {
                    Some(MouseAreaEvent::MouseWheel {
                        x: *x,
                        y: *y,
                        wheel_delta: *wheel_delta,
                        modifiers: *modifiers,
                        inside,
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Handle synthetic enter/leave events based on mouse movement
    fn handle_synthetic_events(
        &self,
        state: &mut MouseAreaState,
        event: &Event,
        bounds: &Bounds,
        shell: &mut Shell<Message>,
    ) {
        if let Event::MouseMove { x, y } = event {
            let point = PointDIP { x: *x, y: *y };
            let inside = point.within(bounds.border_box);

            if inside != state.mouse_inside {
                state.mouse_inside = inside;
                state.last_mouse_pos = Some((*x, *y));

                let synthetic_event = if inside {
                    MouseAreaEvent::MouseEntered { x: *x, y: *y }
                } else {
                    MouseAreaEvent::MouseLeft { x: *x, y: *y }
                };

                if let Some(ref handler) = self.event_handler
                    && let Some(message) = handler(synthetic_event, shell)
                {
                    shell.publish(message);
                }
            } else if inside {
                state.last_mouse_pos = Some((*x, *y));
            }
        } else if let Event::MouseLeave { x, y } = event
            && state.mouse_inside
        {
            state.mouse_inside = false;
            state.last_mouse_pos = Some((*x, *y));

            if let Some(ref handler) = self.event_handler
                && let Some(message) = handler(MouseAreaEvent::MouseLeft { x: *x, y: *y }, shell)
            {
                shell.publish(message);
            }
        }

        // TODO: Replace synthetic handling with framework enter/leave events
    }

    pub fn as_element(self, id: u64, children: impl Into<Element<Message>>) -> Element<Message> {
        Element {
            id: Some(id),
            children: vec![children.into()],
            content: widget(self),
            ..Default::default()
        }
    }
}

impl<Message> Widget<Message> for MouseArea<Message>
where
    Message: 'static,
{
    fn limits_x(
        &self,
        _arenas: &UIArenas,
        _instance: &mut Instance,
    ) -> crate::widgets::limit_response::SizingForX {
        crate::widgets::limit_response::SizingForX {
            min_width: 0.0,
            preferred_width: 0.0,
        }
    }

    fn limits_y(
        &self,
        _arenas: &UIArenas,
        _instance: &mut Instance,
        _border_width: f32,
        _content_width: f32,
    ) -> crate::widgets::limit_response::SizingForY {
        crate::widgets::limit_response::SizingForY {
            min_height: 0.0,
            preferred_height: 0.0,
        }
    }

    fn state(&self, _arenas: &UIArenas, _device_resources: &DeviceResources) -> State {
        Some(Box::new(MouseAreaState::default()))
    }

    fn paint(
        &mut self,
        _arenas: &UIArenas,
        _instance: &mut Instance,
        _shell: &mut Shell<Message>,
        _recorder: &mut crate::gfx::command_recorder::CommandRecorder,
        _style: ElementStyle,
        _bounds: Bounds,
        _now: Instant,
    ) {
        // MouseArea is invisible - no drawing
    }

    fn update(
        &mut self,
        _arenas: &mut UIArenas,
        instance: &mut Instance,
        _hwnd: HWND,
        shell: &mut Shell<Message>,
        event: &Event,
        bounds: Bounds,
    ) {
        let state = with_state!(mut instance as MouseAreaState);

        if matches!(event, Event::MouseButtonDown { .. }) {
            state.mouse_held = true;
        }

        // Handle synthetic enter/leave events first
        self.handle_synthetic_events(state, event, &bounds, shell);

        // Map and handle core mouse events
        if let Some(mouse_event) = self.map_event(state, event, &bounds)
            && let Some(ref handler) = self.event_handler
            && let Some(message) = handler(mouse_event, shell)
        {
            shell.publish(message);
        }

        if matches!(event, Event::MouseButtonUp { .. }) {
            state.mouse_held = false;
        }
    }

    fn cursor(
        &self,
        _arenas: &UIArenas,
        _instance: &Instance,
        _point: PointDIP,
        _bounds: Bounds,
    ) -> Option<Cursor> {
        None
    }
}
