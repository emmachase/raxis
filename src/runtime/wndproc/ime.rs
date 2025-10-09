use std::ops::DerefMut;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::UI::Input::Ime::{
    GCS_COMPSTR, GCS_CURSORPOS, GCS_RESULTSTR, ImmGetCompositionStringW, ImmGetContext,
    ImmReleaseContext,
};
use crate::runtime::util::state_mut_from_hwnd;
use crate::widgets::Event;

/// Handle WM_IME_STARTCOMPOSITION
pub fn handle_ime_start_composition<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
) -> LRESULT {
    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();

        state
            .shell
            .dispatch_event(hwnd, &mut state.ui_tree, Event::ImeStartComposition);
    }
    LRESULT(0)
}

/// Handle WM_IME_COMPOSITION
pub fn handle_ime_composition<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    lparam: LPARAM,
) -> LRESULT {
    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();

        let himc = unsafe { ImmGetContext(hwnd) };
        if !himc.is_invalid() {
            let flags = lparam.0 as u32;

            // Handle result string (committed text)
            if flags & GCS_RESULTSTR.0 != 0 {
                let bytes = unsafe { ImmGetCompositionStringW(himc, GCS_RESULTSTR, None, 0) };
                if bytes > 0 {
                    let mut buf: Vec<u16> = vec![0; (bytes as usize) / 2];
                    let _ = unsafe {
                        ImmGetCompositionStringW(
                            himc,
                            GCS_RESULTSTR,
                            Some(buf.as_mut_ptr() as *mut _),
                            bytes as u32,
                        )
                    };
                    let s = String::from_utf16_lossy(&buf);

                    state.shell.dispatch_event(
                        hwnd,
                        &mut state.ui_tree,
                        Event::ImeComposition {
                            text: s.clone(),
                            caret_units: 0,
                        },
                    );

                    state.shell.dispatch_event(
                        hwnd,
                        &mut state.ui_tree,
                        Event::ImeCommit { text: s.clone() },
                    );
                }
            }
            // Handle ongoing composition string
            else if flags & GCS_COMPSTR.0 != 0 {
                let bytes = unsafe { ImmGetCompositionStringW(himc, GCS_COMPSTR, None, 0) };
                let mut comp = String::new();
                if bytes > 0 {
                    let mut buf: Vec<u16> = vec![0; (bytes as usize) / 2];
                    let _ = unsafe {
                        ImmGetCompositionStringW(
                            himc,
                            GCS_COMPSTR,
                            Some(buf.as_mut_ptr() as *mut _),
                            bytes as u32,
                        )
                    };
                    comp = String::from_utf16_lossy(&buf);
                }
                // Caret within comp string (UTF-16 units)
                let caret_units = {
                    let v = unsafe { ImmGetCompositionStringW(himc, GCS_CURSORPOS, None, 0) };
                    if v < 0 {
                        0
                    } else {
                        v as u32
                    }
                };

                state.shell.dispatch_event(
                    hwnd,
                    &mut state.ui_tree,
                    Event::ImeComposition {
                        text: comp.clone(),
                        caret_units,
                    },
                );

                let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            }

            let _ = unsafe { ImmReleaseContext(hwnd, himc) };
        }
    }
    LRESULT(0)
}

/// Handle WM_IME_ENDCOMPOSITION
pub fn handle_ime_end_composition<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
) -> LRESULT {
    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();

        state
            .shell
            .dispatch_event(hwnd, &mut state.ui_tree, Event::ImeEndComposition);

        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    }
    LRESULT(0)
}
