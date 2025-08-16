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
    widgets::Event,
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
}

pub enum InputMethod {
    Disabled,
    Enabled { position: gfx::PointDIP },
}

impl Shell {
    pub fn new() -> Self {
        Self {
            focus_manager: focus::FocusManager::new(),
            input_method: InputMethod::Disabled,

            event_captured: false,
        }
    }

    pub fn dispatch_event(&mut self, hwnd: HWND, ui_tree: BorrowedUITree, event: Event) {
        self.event_captured = false;

        if let Some(root) = ui_tree.keys().next() {
            visitors::visit_reverse_bfs(ui_tree, root, |ui_tree, key, _| {
                let element = &mut ui_tree[key];
                let bounds = element.bounds();
                if let Some(layout::model::ElementContent::Widget(ref mut widget)) = element.content
                {
                    widget.update(key, hwnd, self, &event, bounds);

                    if self.event_captured {
                        return VisitAction::Exit;
                    }
                }

                VisitAction::Continue
            });
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
                            println!("new_position: {:?}", new_position);
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
