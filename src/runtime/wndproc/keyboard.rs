use std::ops::DerefMut;
use windows::Win32::Foundation::{HWND, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;

use crate::runtime::util::{get_modifiers, state_mut_from_hwnd};
use crate::runtime::vkey::VKey;
use crate::widgets::Event;

/// Handle WM_CHAR
pub fn handle_char<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    wparam: WPARAM,
) -> LRESULT {
    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();

        // Suppress WM_CHAR while IME composition is active to avoid duplicate input
        let mut code = (wparam.0 & 0xFFFF) as u32;

        // Handle CR -> LF
        if code == 0x0D {
            code = 0x0A;
        }

        // Skip most control chars except TAB and LF
        if (code < 0x20 && code != 0x09 && code != 0x0A) || code == 0x7F {
            return LRESULT(0);
        }

        // Surrogate handling
        let is_high = (0xD800..=0xDBFF).contains(&(code as u16));
        let is_low = (0xDC00..=0xDFFF).contains(&(code as u16));
        let mut to_insert = String::new();

        if is_high {
            state.pending_high_surrogate = Some(code as u16);
        } else if is_low {
            if let Some(high) = state.pending_high_surrogate.take() {
                let u = 0x10000 + (((high as u32 - 0xD800) << 10) | ((code - 0xDC00) & 0x3FF));
                if let Some(ch) = char::from_u32(u) {
                    to_insert.push(ch);
                }
            }
        } else {
            state.pending_high_surrogate = None;
            if let Some(ch) = char::from_u32(code) {
                to_insert.push(ch);
            }
        }

        if !to_insert.is_empty() {
            state.shell.dispatch_event(
                hwnd,
                &mut state.ui_tree,
                Event::Char {
                    text: to_insert.into(),
                },
            );

            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
        }
    }
    LRESULT(0)
}

/// Handle WM_KEYDOWN
pub fn handle_keydown<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    wparam: WPARAM,
) -> LRESULT {
    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();
        let vk = wparam.0 as i32;

        let modifiers = get_modifiers();
        if let Ok(vk) = VKey::try_from(vk) {
            state.shell.dispatch_event(
                hwnd,
                &mut state.ui_tree,
                Event::KeyDown { key: vk, modifiers },
            );
        }

        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
        return LRESULT(0);
    }

    LRESULT(0)
}

/// Handle WM_KEYUP
pub fn handle_keyup<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    wparam: WPARAM,
) -> LRESULT {
    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();
        let vk = wparam.0 as i32;

        let modifiers = get_modifiers();
        if let Ok(vk) = VKey::try_from(vk) {
            state.shell.dispatch_event(
                hwnd,
                &mut state.ui_tree,
                Event::KeyUp { key: vk, modifiers },
            );
        }

        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
        return LRESULT(0);
    }

    LRESULT(0)
}
