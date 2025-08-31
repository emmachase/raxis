use std::{any::Any, cell::RefCell, sync::mpsc, time::Instant};

use windows::Win32::{
    Foundation::HWND,
    Graphics::Gdi::InvalidateRect,
    UI::{
        HiDpi::GetDpiForWindow,
        WindowsAndMessaging::{KillTimer, SetTimer},
    },
};

use crate::{
    layout::{
        BorrowedUITree,
        model::Element,
        visitors::{self, VisitAction},
    },
    runtime::focus,
    widgets::{DragData, DragEvent, DropResult, Event, Operation, dispatch_operation},
};

pub mod gfx;
pub mod layout;
pub mod math;
pub mod runtime;
pub mod util;
pub mod widgets;

#[derive(Default)]
pub struct HookState {
    // TODO: Discriminate by TypeId?
    hooks: Vec<RefCell<Box<dyn Any>>>,
}

pub struct HookManager<'a, Message> {
    ui_tree: BorrowedUITree<'a, Message>,
}

pub struct HookInstance<'a> {
    state: &'a mut HookState,
    position: usize,
}

impl<'a> HookInstance<'a> {
    pub fn use_hook<T: 'static>(&mut self, initializer: impl FnOnce() -> T) -> &mut T {
        if self.position >= self.state.hooks.len() {
            self.state.hooks.push(RefCell::new(Box::new(initializer())));
        }
        let hook = self.state.hooks[self.position]
            .get_mut()
            .downcast_mut::<T>()
            .unwrap();
        self.position += 1;
        hook
    }
}

impl<Message> HookManager<'_, Message> {
    pub fn instance(&mut self, id: u64) -> HookInstance {
        let state = self.ui_tree.hook_state.entry(id).or_insert_with(|| {
            println!("Creating hook state for {id}");
            HookState::default()
        });

        HookInstance { state, position: 0 }
    }
}

pub type ViewFn<State, Message> = dyn Fn(&State, HookManager<Message>) -> Element<Message>;
pub type UpdateFn<State, Message> =
    dyn Fn(&mut State, Message) -> Option<crate::runtime::task::Task<Message>>;

pub enum DeferredControl {
    StartDrag { data: DragData, src_id: u64 },

    SetIMEPosition { position: gfx::PointDIP },
    DisableIME,
}

pub struct Shell<Message> {
    focus_manager: focus::FocusManager,
    input_method: InputMethod,

    event_captured: bool,

    /// Track which widget currently has drag focus for proper drag_leave handling
    current_drag_widget: Option<layout::model::UIKey>,

    operation_queue: Vec<Box<dyn Operation>>,

    // requested_next_redraw: bool,
    // redraw_timers: Vec<usize>,
    // next_timer_id: usize,
    redraw_request: RedrawRequest,

    deferred_controls: Vec<DeferredControl>,

    message_sender: mpsc::Sender<Message>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum RedrawRequest {
    Immediate,
    At(Instant),
    Wait,
}

pub enum InputMethod {
    Disabled,
    Enabled { position: gfx::PointDIP },
}

pub const REDRAW_TIMER_ID: usize = 50;

impl<Message> Shell<Message> {
    pub fn new(message_sender: mpsc::Sender<Message>) -> Self {
        Self {
            focus_manager: focus::FocusManager::new(),
            input_method: InputMethod::Disabled,

            event_captured: false,
            current_drag_widget: None,
            operation_queue: Vec::new(),
            redraw_request: RedrawRequest::Wait,
            deferred_controls: Vec::new(),
            message_sender,
        }
    }

    pub fn queue_operation(&mut self, operation: Box<dyn Operation>) {
        self.operation_queue.push(operation);
    }

    pub fn queue_deferred_control(&mut self, control: DeferredControl) {
        self.deferred_controls.push(control);
    }

    pub fn drain_deferred_controls(&mut self) -> Option<Vec<DeferredControl>> {
        if self.deferred_controls.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.deferred_controls))
        }
    }

    /// Replaces the current redraw request with the given request.
    /// This will override any existing request, you almost never want to use this.
    pub fn replace_redraw_request(&mut self, request: RedrawRequest) {
        self.redraw_request = request;
    }

    pub fn request_redraw(&mut self, hwnd: HWND, request: RedrawRequest) {
        if request < self.redraw_request {
            self.redraw_request = request;

            if matches!(request, RedrawRequest::Immediate) {
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            } else {
                let uelapse = match request {
                    RedrawRequest::Wait => return,
                    RedrawRequest::At(instant) => {
                        instant.duration_since(Instant::now()).as_millis() as u32
                    }
                    _ => 0,
                };
                unsafe { SetTimer(Some(hwnd), REDRAW_TIMER_ID, uelapse, None) };
            }
        }
    }

    pub fn kill_redraw_timer(&mut self, hwnd: HWND, timer_id: usize) {
        if timer_id == REDRAW_TIMER_ID {
            self.redraw_request = RedrawRequest::Wait;
            unsafe {
                let _ = KillTimer(Some(hwnd), timer_id);
            }
        }
    }

    pub fn dispatch_event(&mut self, hwnd: HWND, ui_tree: BorrowedUITree<Message>, event: Event) {
        self.event_captured = false;

        // Handle regular events with reverse BFS traversal
        visitors::visit_reverse_bfs(ui_tree, ui_tree.root, |ui_tree, key, _| {
            let element = &mut ui_tree.slots[key];
            let bounds = element.bounds();
            if let Some(layout::model::ElementContent::Widget(ref mut widget)) = element.content {
                if let Some(id) = element.id {
                    let instance = ui_tree.widget_state.get_mut(&id).unwrap();
                    widget.update(&mut ui_tree.arenas, instance, hwnd, self, &event, bounds);
                }

                if self.event_captured {
                    return VisitAction::Exit;
                }
            }

            VisitAction::Continue
        });
    }

    pub fn dispatch_event_to(
        &mut self,
        hwnd: HWND,
        ui_tree: BorrowedUITree<Message>,
        event: Event,
        target_id: u64,
    ) {
        self.event_captured = false;

        // TODO: Make some map for id to avoid a traversal

        // Handle regular events with reverse BFS traversal
        visitors::visit_reverse_bfs(ui_tree, ui_tree.root, |ui_tree, key, _| {
            let element = &mut ui_tree.slots[key];
            let bounds = element.bounds();
            if let Some(layout::model::ElementContent::Widget(ref mut widget)) = element.content {
                if let Some(id) = element.id
                    && target_id == id
                {
                    let instance = ui_tree.widget_state.get_mut(&id).unwrap();
                    widget.update(&mut ui_tree.arenas, instance, hwnd, self, &event, bounds);
                }

                if self.event_captured {
                    return VisitAction::Exit;
                }
            }

            VisitAction::Continue
        });
    }

    pub fn publish(&self, message: Message) {
        self.message_sender.send(message).unwrap();
    }

    pub fn dispatch_operations(&mut self, ui_tree: BorrowedUITree<Message>) {
        for operation in self.operation_queue.drain(..) {
            dispatch_operation(ui_tree, &*operation);
        }
    }

    /// Dispatch drag/drop events to widgets based on position
    pub fn dispatch_drag_event(
        &mut self,
        ui_tree: BorrowedUITree<Message>,
        event: &DragEvent,
        position: gfx::PointDIP,
    ) -> Option<DropResult> {
        let mut result = None;
        let mut new_drag_widget = None;
        let prev_drag_widget = self.current_drag_widget;

        // First, handle drag_leave if we're moving away from a widget
        if matches!(event, DragEvent::DragOver { .. }) && prev_drag_widget.is_some() {
            // Check if we need to call drag_leave on the previous widget
            let mut should_call_drag_leave = false;
            let mut found_new_widget = false;

            // Quick check to see if we're still over the same widget or moved to a new one
            visitors::visit_reverse_bfs(ui_tree, ui_tree.root, |ui_tree, key, _| {
                let element = &ui_tree.slots[key];
                let bounds = element.bounds();

                if position.within(bounds.border_box) {
                    if let Some(layout::model::ElementContent::Widget(_)) = element.content {
                        if key != prev_drag_widget.unwrap() {
                            should_call_drag_leave = true;
                        }
                        found_new_widget = true;
                        return VisitAction::Exit;
                    }
                }
                VisitAction::Continue
            });

            if should_call_drag_leave || !found_new_widget {
                // Call drag_leave on the previous widget
                if let Some(prev_key) = prev_drag_widget {
                    if let Some(prev_element) = ui_tree.slots.get_mut(prev_key) {
                        let prev_bounds = prev_element.bounds();
                        if let Some(layout::model::ElementContent::Widget(ref mut prev_widget)) =
                            prev_element.content
                        {
                            if let Some(prev_text_input) = prev_widget.as_drop_target()
                                && let Some(id) = prev_element.id
                                && let Some(instance) = ui_tree.widget_state.get_mut(&id)
                            {
                                prev_text_input.drag_leave(instance, prev_bounds);
                            }
                        }
                    }
                }
            }
        }

        // Now find the widget under the current position and handle the event
        visitors::visit_reverse_bfs(ui_tree, ui_tree.root, |ui_tree, key, _| {
            let element = &mut ui_tree.slots[key];
            let bounds = element.bounds();

            // Check if point is within widget bounds (except for DragLeave, which should be handled by all)
            if position.within(bounds.border_box) || matches!(event, DragEvent::DragLeave) {
                if let Some(layout::model::ElementContent::Widget(ref mut widget)) = element.content
                {
                    if let Some(text_input) = widget.as_drop_target()
                        && let Some(id) = element.id
                        && let Some(instance) = ui_tree.widget_state.get_mut(&id)
                    {
                        new_drag_widget = Some(key);

                        match event {
                            DragEvent::DragEnter { drag_info } => {
                                let effect = text_input.drag_enter(instance, drag_info, bounds);
                                result = Some(DropResult {
                                    effect,
                                    handled: true,
                                });
                            }
                            DragEvent::DragOver { drag_info } => {
                                if prev_drag_widget != Some(key) {
                                    // Moving to a new widget, call drag_enter
                                    let effect = text_input.drag_enter(instance, drag_info, bounds);
                                    result = Some(DropResult {
                                        effect,
                                        handled: true,
                                    });
                                } else {
                                    // Same widget, call drag_over
                                    let effect = text_input.drag_over(instance, drag_info, bounds);
                                    result = Some(DropResult {
                                        effect,
                                        handled: true,
                                    });
                                }
                            }
                            DragEvent::Drop { drag_info } => {
                                result = Some(text_input.drop(instance, self, drag_info, bounds));
                            }
                            DragEvent::DragLeave => {
                                text_input.drag_leave(instance, bounds);
                            }
                        }

                        if !matches!(event, DragEvent::DragLeave) {
                            return VisitAction::Exit;
                        }
                    }
                }
            }
            VisitAction::Continue
        });

        // Update the current drag widget
        self.current_drag_widget = new_drag_widget;

        result
    }

    /// Captures the event, preventing further traversal.
    ///
    /// No ancestor widget will receive the event.
    ///
    /// Returns true if the event was captured.
    pub fn capture_event(&mut self) -> bool {
        if self.event_captured {
            return false;
        }

        self.event_captured = true;
        true
    }

    pub fn request_input_method(&mut self, ime: InputMethod) {
        match self.input_method {
            InputMethod::Disabled => match ime {
                InputMethod::Disabled => { /* Nothing to do */ }
                InputMethod::Enabled { position } => {
                    self.deferred_controls
                        .push(DeferredControl::SetIMEPosition { position });

                    self.input_method = ime;
                }
            },
            InputMethod::Enabled { position } => match ime {
                InputMethod::Disabled => {
                    self.deferred_controls.push(DeferredControl::DisableIME);
                }
                InputMethod::Enabled {
                    position: new_position,
                } => {
                    if position != new_position {
                        self.deferred_controls
                            .push(DeferredControl::SetIMEPosition {
                                position: new_position,
                            });
                    }
                }
            },
        }
    }
}

pub fn current_dpi(hwnd: HWND) -> f32 {
    unsafe { GetDpiForWindow(hwnd) as f32 }
}

pub fn dips_scale(hwnd: HWND) -> f32 {
    dips_scale_for_dpi(current_dpi(hwnd))
}

pub fn dips_scale_for_dpi(dpi: f32) -> f32 {
    96.0f32 / dpi.max(1.0)
}
