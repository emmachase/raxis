use windows::Win32::{
    Foundation::{HWND, POINT, RECT},
    UI::{
        HiDpi::GetDpiForWindow,
        Input::Ime::{
            CANDIDATEFORM, CFS_POINT, CPS_COMPLETE, ImmGetContext, ImmNotifyIME, ImmReleaseContext,
            ImmSetCandidateWindow, NI_COMPOSITIONSTR,
        },
    },
};

use crate::{
    layout::{
        BorrowedUITree,
        visitors::{self, VisitAction},
    },
    widgets::{DragEvent, DropResult, Event, Operation, WidgetDragDropTarget, dispatch_operation},
};

pub mod clipboard;
pub mod dragdrop;
pub mod focus;
pub mod gfx;
pub mod layout;
pub mod math;
pub mod util;
pub mod widgets;

pub struct Shell {
    focus_manager: focus::FocusManager,
    input_method: InputMethod,

    event_captured: bool,

    /// Track which widget currently has drag focus for proper drag_leave handling
    current_drag_widget: Option<layout::model::UIKey>,

    operation_queue: Vec<Box<dyn Operation>>,
}

pub enum InputMethod {
    Disabled,
    Enabled { position: gfx::PointDIP },
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

impl Shell {
    pub fn new() -> Self {
        Self {
            focus_manager: focus::FocusManager::new(),
            input_method: InputMethod::Disabled,

            event_captured: false,
            current_drag_widget: None,
            operation_queue: Vec::new(),
        }
    }

    pub fn queue_operation(&mut self, operation: Box<dyn Operation>) {
        self.operation_queue.push(operation);
    }

    pub fn dispatch_event(&mut self, hwnd: HWND, ui_tree: BorrowedUITree, event: Event) {
        self.event_captured = false;

        // Handle regular events with reverse BFS traversal
        if let Some(root) = ui_tree.keys().next() {
            visitors::visit_reverse_bfs(ui_tree, root, |ui_tree, key, _| {
                let element = &mut ui_tree[key];
                let id = element.id;
                let bounds = element.bounds();
                if let Some(layout::model::ElementContent::Widget(ref mut widget)) = element.content
                {
                    widget.update(id, key, hwnd, self, &event, bounds);

                    if self.event_captured {
                        return VisitAction::Exit;
                    }
                }

                VisitAction::Continue
            });
        }
    }

    pub fn dispatch_operations(&mut self, ui_tree: BorrowedUITree) {
        for operation in self.operation_queue.drain(..) {
            dispatch_operation(ui_tree, &*operation);
        }
    }

    /// Dispatch drag/drop events to widgets based on position
    pub fn dispatch_drag_event(
        &mut self,
        ui_tree: BorrowedUITree,
        event: &DragEvent,
        position: gfx::PointDIP,
    ) -> Option<DropResult> {
        if let Some(root) = ui_tree.keys().next() {
            let mut result = None;
            let mut new_drag_widget = None;
            let prev_drag_widget = self.current_drag_widget;

            // First, handle drag_leave if we're moving away from a widget
            if matches!(event, DragEvent::DragOver { .. }) && prev_drag_widget.is_some() {
                // Check if we need to call drag_leave on the previous widget
                let mut should_call_drag_leave = false;
                let mut found_new_widget = false;

                // Quick check to see if we're still over the same widget or moved to a new one
                visitors::visit_reverse_bfs(ui_tree, root, |ui_tree, key, _| {
                    let element = &ui_tree[key];
                    let bounds = element.bounds();

                    if position.within(bounds) {
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
                        if let Some(prev_element) = ui_tree.get_mut(prev_key) {
                            let prev_bounds = prev_element.bounds();
                            if let Some(layout::model::ElementContent::Widget(
                                ref mut prev_widget,
                            )) = prev_element.content
                            {
                                if let Some(prev_text_input) = prev_widget.as_drop_target() {
                                    prev_text_input.drag_leave(prev_bounds);
                                }
                            }
                        }
                    }
                }
            }

            // Now find the widget under the current position and handle the event
            visitors::visit_reverse_bfs(ui_tree, root, |ui_tree, key, _| {
                let element = &mut ui_tree[key];
                let bounds = element.bounds();

                // Check if point is within widget bounds (except for DragLeave, which should be handled by all)
                if position.within(bounds) || matches!(event, DragEvent::DragLeave) {
                    if let Some(layout::model::ElementContent::Widget(ref mut widget)) =
                        element.content
                    {
                        if let Some(text_input) = widget.as_drop_target() {
                            new_drag_widget = Some(key);

                            match event {
                                DragEvent::DragEnter { drag_info } => {
                                    let effect = text_input.drag_enter(drag_info, bounds);
                                    result = Some(DropResult {
                                        effect,
                                        handled: true,
                                    });
                                }
                                DragEvent::DragOver { drag_info } => {
                                    if prev_drag_widget != Some(key) {
                                        // Moving to a new widget, call drag_enter
                                        let effect = text_input.drag_enter(drag_info, bounds);
                                        result = Some(DropResult {
                                            effect,
                                            handled: true,
                                        });
                                    } else {
                                        // Same widget, call drag_over
                                        let effect = text_input.drag_over(drag_info, bounds);
                                        result = Some(DropResult {
                                            effect,
                                            handled: true,
                                        });
                                    }
                                }
                                DragEvent::Drop { drag_info } => {
                                    result = Some(
                                        text_input.drop(element.id, key, self, drag_info, bounds),
                                    );
                                }
                                DragEvent::DragLeave => {
                                    text_input.drag_leave(bounds);
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
        } else {
            None
        }
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

    pub fn request_input_method(&mut self, hwnd: HWND, ime: InputMethod) {
        match self.input_method {
            InputMethod::Disabled => match ime {
                InputMethod::Disabled => { /* Nothing to do */ }
                InputMethod::Enabled { position } => unsafe {
                    let himc = ImmGetContext(hwnd);
                    if !himc.is_invalid() {
                        let to_dip = dips_scale(hwnd);
                        let cf = CANDIDATEFORM {
                            dwStyle: CFS_POINT,
                            ptCurrentPos: POINT {
                                x: (position.x_dip / to_dip).round() as i32,
                                y: (position.y_dip / to_dip).round() as i32,
                            },
                            rcArea: RECT::default(),
                            dwIndex: 0,
                        };
                        let _ = ImmSetCandidateWindow(himc, &cf);

                        let _ = ImmReleaseContext(hwnd, himc);
                    }

                    self.input_method = ime;
                },
            },
            InputMethod::Enabled { position } => match ime {
                InputMethod::Disabled => unsafe {
                    let himc = ImmGetContext(hwnd);
                    if !himc.is_invalid() {
                        let _ = ImmNotifyIME(himc, NI_COMPOSITIONSTR, CPS_COMPLETE, 0);
                    }
                },
                InputMethod::Enabled {
                    position: new_position,
                } => unsafe {
                    if position != new_position {
                        let himc = ImmGetContext(hwnd);
                        if !himc.is_invalid() {
                            let to_dip = dips_scale(hwnd);
                            let cf = CANDIDATEFORM {
                                dwStyle: CFS_POINT,
                                ptCurrentPos: POINT {
                                    x: (new_position.x_dip / to_dip).round() as i32,
                                    y: (new_position.y_dip / to_dip).round() as i32,
                                },
                                rcArea: RECT::default(),
                                dwIndex: 0,
                            };
                            let _ = ImmSetCandidateWindow(himc, &cf);

                            let _ = ImmReleaseContext(hwnd, himc);
                        }
                    }
                },
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
