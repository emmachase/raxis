use std::ops::DerefMut;
use std::sync::OnceLock;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, RegisterWindowMessageW, SWP_FRAMECHANGED, SetWindowPos,
};
use windows_core::PCWSTR;

use crate::runtime::context_menu::{ContextMenu, ContextMenuRequest};
use crate::runtime::syscommand::{SystemCommand, SystemCommandResponse};
use crate::runtime::tray::TrayIcon;
use crate::runtime::util::state_mut_from_hwnd;

/// Store the registered TaskbarCreated message ID
static TASKBAR_CREATED_MSG: OnceLock<u32> = OnceLock::new();

/// Register the TaskbarCreated window message
/// This message is sent when the taskbar is recreated (e.g., when explorer.exe restarts)
pub fn register_taskbar_created_message() -> u32 {
    *TASKBAR_CREATED_MSG.get_or_init(|| {
        let msg_name: Vec<u16> = "TaskbarCreated"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe { RegisterWindowMessageW(PCWSTR(msg_name.as_ptr())) }
    })
}

/// Get the registered TaskbarCreated message ID
pub fn get_taskbar_created_message() -> Option<u32> {
    TASKBAR_CREATED_MSG.get().copied()
}

/// Handle WM_ACTIVATE
pub fn handle_activate<State: 'static, Message: 'static + Send + Clone>(hwnd: HWND) -> LRESULT {
    let _ = unsafe { InvalidateRect(Some(hwnd), None, true) };

    unsafe {
        let mut rc_client = RECT::default();
        if GetWindowRect(hwnd, &mut rc_client).is_ok() {
            SetWindowPos(
                hwnd,
                None,
                rc_client.left,
                rc_client.top,
                rc_client.right - rc_client.left,
                rc_client.bottom - rc_client.top,
                SWP_FRAMECHANGED,
            )
            .ok();
        }
    }

    LRESULT(0)
}

/// Handle WM_SYSCOMMAND
pub fn handle_syscommand<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    wparam: WPARAM,
) -> Option<LRESULT> {
    // Handle system commands (minimize, maximize, close, etc.)
    let command = SystemCommand::from_wparam(wparam.0);

    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();
        if let Some(ref handler) = state.syscommand_handler {
            match handler(&state.user_state, command) {
                SystemCommandResponse::Allow => {
                    // Fall through to default handling
                }
                SystemCommandResponse::AllowWith(message) => {
                    state.shell.message_sender.send(message).ok();
                    state.shell.pending_messages = true;
                }
                SystemCommandResponse::Prevent => {
                    // Application handled the command, prevent default behavior
                    return Some(LRESULT(0));
                }
                SystemCommandResponse::PreventWith(message) => {
                    state.shell.message_sender.send(message).ok();
                    state.shell.pending_messages = true;
                    return Some(LRESULT(0));
                }
            }
        }
    }

    None
}

/// Handle WM_TRAYICON
pub fn handle_trayicon<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    lparam: LPARAM,
) -> LRESULT {
    // Handle tray icon events
    if let Some(event) = TrayIcon::parse_message(lparam) {
        if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
            let state = state.deref_mut();
            if let Some(ref handler) = state.tray_event_handler {
                if let Some(task) = handler(&state.user_state, event) {
                    state.task_sender.send(task).ok();
                }
            }
        }
    }
    LRESULT(0)
}

/// Handle WM_SHOW_CONTEXT_MENU
pub fn handle_show_context_menu<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    wparam: WPARAM,
) -> LRESULT {
    // Handle context menu request from async thread
    let request_ptr = wparam.0 as *mut ContextMenuRequest;
    if !request_ptr.is_null() {
        let request = unsafe { Box::from_raw(request_ptr) };

        // Show the menu on the UI thread
        let result =
            unsafe { ContextMenu::show_sync_on_ui_thread(&request.items, hwnd, request.position) };

        // Send the result back through the channel
        let _ = request.sender.send(result);
    }
    LRESULT(0)
}

/// Handle WM_ASYNC_MESSAGE
pub fn handle_async_message<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
) -> LRESULT {
    // Handle async messages from executor thread
    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();
        state.process_async_messages(hwnd);
    }
    LRESULT(0)
}

/// Handle WM_TIMER
pub fn handle_timer<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    wparam: WPARAM,
) -> LRESULT {
    let timer_id = wparam.0;
    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();
        state.shell.kill_redraw_timer(hwnd, timer_id);
    }
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    LRESULT(0)
}

/// Handle TaskbarCreated message
/// This is sent when Windows Explorer restarts and the taskbar is recreated
pub fn handle_taskbar_created<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
) -> LRESULT {
    // Re-add the tray icon when the taskbar is recreated
    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();
        if let Some(ref mut tray_icon) = state._tray_icon {
            let _ = tray_icon.add(); // Ignore errors, best effort
        }
    }
    LRESULT(0)
}
