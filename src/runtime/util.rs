use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use std::sync::{Mutex, MutexGuard};

use super::{ApplicationHandle, Result};

/// Helper to get mutable application state from HWND user data
pub fn state_mut_from_hwnd<State, Message>(
    hwnd: HWND,
) -> Option<MutexGuard<'static, ApplicationHandle<State, Message>>> {
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GWLP_USERDATA};
        use log::warn;
        
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);

        if ptr != 0 {
            let mutex = &*(ptr as *const Mutex<ApplicationHandle<State, Message>>);
            if mutex.try_lock().is_err() {
                warn!("event-loop mutex was locked, skipping event");
                return None;
            }

            Some(mutex.lock().unwrap())
        } else {
            None
        }
    }
}

/// Get the client rectangle of a window
pub fn client_rect(hwnd: HWND) -> Result<RECT> {
    unsafe {
        let mut rc = RECT::default();
        GetClientRect(hwnd, &mut rc)?;
        Ok(rc)
    }
}

/// Get the window rectangle in screen coordinates
pub fn window_rect(hwnd: HWND) -> Result<RECT> {
    unsafe {
        let client_rc = client_rect(hwnd)?;
        let mut offset = POINT { x: 0, y: 0 };
        ClientToScreen(hwnd, &mut offset).ok()?;

        Ok(RECT {
            left: offset.x,
            top: offset.y,
            right: offset.x + client_rc.right,
            bottom: offset.y + client_rc.bottom,
        })
    }
}

/// Get keyboard modifiers state
pub fn get_modifiers() -> crate::widgets::Modifiers {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
    
    unsafe {
        let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
        let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
        let alt = GetKeyState(VK_MENU.0 as i32) < 0;

        crate::widgets::Modifiers { ctrl, shift, alt }
    }
}
