pub mod app_handle;
pub mod clipboard;
pub mod context_menu;
pub mod device;
pub mod dragdrop;
pub mod focus;
pub mod font_manager;
pub mod input;
pub mod scroll;
pub mod smooth_scroll;
pub mod syscommand;
pub mod task;
pub mod tray;
pub mod util;
pub mod vkey;
pub mod window;
pub mod wndproc;

// Re-exports for convenience
pub use app_handle::{ApplicationHandle, Result, RuntimeError};
pub use device::DeviceResources;
pub use input::{MiddleMouseScrollState, MouseState, ScrollbarDragState};
pub use window::{Application, Backdrop};

use crate::dips_scale;
use crate::gfx::PointDIP;
use crate::runtime::app_handle::PENDING_MESSAGE_PROCESSING;
use crate::runtime::context_menu::WM_SHOW_CONTEXT_MENU;
use crate::runtime::dragdrop::start_text_drag;
use crate::runtime::tray::WM_TRAYICON;
use crate::widgets::drop_target::DropTarget;
use crate::widgets::{DragData, DragEvent, Event};
use crate::{DeferredControl, RedrawRequest};
use std::ops::DerefMut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Dwm::{
    DWM_BB_ENABLE, DWM_BLURBEHIND, DWM_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW, DWMSBT_NONE,
    DWMSBT_TABBEDWINDOW, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DwmDefWindowProc, DwmEnableBlurBehindWindow,
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, FillRect, HDC};
use windows::Win32::System::Com::CoUninitialize;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::Input::Ime::{
    CANDIDATEFORM, CFS_POINT, CPS_COMPLETE, ImmNotifyIME, ImmSetCandidateWindow, NI_COMPOSITIONSTR,
};
use windows::Win32::UI::WindowsAndMessaging::{
    HTNOWHERE, NCCALCSIZE_PARAMS, PostMessageW, SM_CXFRAME, SM_CYFRAME, SWP_NOMOVE, WM_ACTIVATE,
    WM_DPICHANGED, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYUP, WM_MOUSEWHEEL, WM_NCCALCSIZE,
    WM_NCHITTEST, WM_SYSCOMMAND, WM_TIMER, WM_USER, WS_EX_NOREDIRECTIONBITMAP,
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{InvalidateRect, UpdateWindow},
        System::{
            Com::CoInitialize,
            LibraryLoader::GetModuleHandleW,
            Ole::{IDropTarget, OleInitialize, OleUninitialize, RegisterDragDrop},
        },
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            Input::Ime::{ImmGetContext, ImmReleaseContext},
            WindowsAndMessaging::{
                CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW,
                DispatchMessageW, GWLP_USERDATA, GetClientRect, GetMessageW, GetSystemMetrics,
                IDC_ARROW, LoadCursorW, MSG, RegisterClassW, SW_SHOW, SWP_NOACTIVATE, SWP_NOZORDER,
                SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, WINDOW_EX_STYLE,
                WM_CHAR, WM_DESTROY, WM_DISPLAYCHANGE, WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION,
                WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
                WM_MBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SETCURSOR, WM_SIZE, WNDCLASSW,
                WS_OVERLAPPEDWINDOW,
            },
        },
    },
    core::{PCWSTR, w},
};
use windows_core::BOOL;

pub const LINE_HEIGHT: u32 = 32;

// Custom message for async task results
const WM_ASYNC_MESSAGE: u32 = WM_USER + 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UncheckedHWND(pub HWND);
unsafe impl Send for UncheckedHWND {}
unsafe impl Sync for UncheckedHWND {}

// Re-export utility functions
use util::{client_rect, state_mut_from_hwnd};
use wndproc::hit_test_nca;

type WinUserData<State, Message> = Mutex<ApplicationHandle<State, Message>>;

type WndProc = dyn Fn(HWND, u32, WPARAM, LPARAM) -> LRESULT + Send + Sync;
static WNDPROC_IMPL: OnceLock<Box<WndProc>> = OnceLock::new();

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    WNDPROC_IMPL.get().unwrap()(hwnd, msg, wparam, lparam)
}

static REPLACE_TITLEBAR: AtomicBool = AtomicBool::new(false);
fn wndproc_impl<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let result = unsafe {
        if REPLACE_TITLEBAR.load(Ordering::Relaxed) {
            let mut l_ret = LRESULT(0);
            let mut skip_normal_handlers =
                DwmDefWindowProc(hwnd, msg, wparam, lparam, &mut l_ret).as_bool();

            match msg {
                WM_NCCALCSIZE if wparam.0 == 1 => {
                    let pncsp = lparam.0 as *mut NCCALCSIZE_PARAMS;

                    (*pncsp).rgrc[0].left += GetSystemMetrics(SM_CXFRAME);
                    (*pncsp).rgrc[0].right -= GetSystemMetrics(SM_CXFRAME);
                    (*pncsp).rgrc[0].bottom -= GetSystemMetrics(SM_CYFRAME);

                    return LRESULT(0);
                }
                WM_NCHITTEST if l_ret.0 == 0 => {
                    l_ret = LRESULT(hit_test_nca(hwnd, wparam, lparam) as isize);

                    if l_ret.0 != HTNOWHERE as isize {
                        skip_normal_handlers = true;
                    }
                }
                WM_ERASEBKGND => {
                    let hdc = HDC(wparam.0 as _);
                    let mut rc = RECT::default();
                    GetClientRect(hwnd, &mut rc).unwrap();
                    let brush = CreateSolidBrush(COLORREF(0x000000));
                    FillRect(hdc, &rc, brush);
                    let _ = DeleteObject(brush.into());
                    return LRESULT(1);
                }
                _ => {}
            }

            if skip_normal_handlers {
                return l_ret;
            }
        }

        // println!("msg: {}", msg);

        match msg {
            WM_ACTIVATE => wndproc::handle_activate::<State, Message>(hwnd),
            WM_SYSCOMMAND => {
                if let Some(result) = wndproc::handle_syscommand::<State, Message>(hwnd, wparam) {
                    return result;
                }
                // Default handling
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_TRAYICON => wndproc::handle_trayicon::<State, Message>(hwnd, lparam),
            WM_SHOW_CONTEXT_MENU => {
                wndproc::handle_show_context_menu::<State, Message>(hwnd, wparam)
            }
            WM_ASYNC_MESSAGE => wndproc::handle_async_message::<State, Message>(hwnd),
            WM_IME_STARTCOMPOSITION => {
                wndproc::handle_ime_start_composition::<State, Message>(hwnd)
            }
            WM_IME_COMPOSITION => wndproc::handle_ime_composition::<State, Message>(hwnd, lparam),
            WM_IME_ENDCOMPOSITION => wndproc::handle_ime_end_composition::<State, Message>(hwnd),
            WM_TIMER => wndproc::handle_timer::<State, Message>(hwnd, wparam),
            WM_LBUTTONDOWN => wndproc::handle_lbuttondown::<State, Message>(hwnd, lparam),
            WM_MOUSEMOVE => wndproc::handle_mousemove::<State, Message>(hwnd, lparam),
            WM_MOUSEWHEEL => wndproc::handle_mousewheel::<State, Message>(hwnd, wparam, lparam),
            WM_LBUTTONUP => wndproc::handle_lbuttonup::<State, Message>(hwnd, lparam),

            WM_MBUTTONDOWN => wndproc::handle_mbuttondown::<State, Message>(hwnd, lparam),

            WM_MBUTTONUP => wndproc::handle_mbuttonup::<State, Message>(hwnd),

            WM_CHAR => wndproc::handle_char::<State, Message>(hwnd, wparam),
            WM_KEYDOWN => wndproc::handle_keydown::<State, Message>(hwnd, wparam),
            WM_KEYUP => wndproc::handle_keyup::<State, Message>(hwnd, wparam),
            WM_SIZE => wndproc::handle_size::<State, Message>(hwnd, lparam),
            WM_DPICHANGED => wndproc::handle_dpichanged::<State, Message>(
                hwnd,
                lparam,
                REPLACE_TITLEBAR.load(Ordering::Relaxed),
            ),
            WM_SETCURSOR => {
                if let Some(result) = wndproc::handle_setcursor::<State, Message>(hwnd, lparam) {
                    return result;
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_GETMINMAXINFO => wndproc::handle_getminmaxinfo::<State, Message>(hwnd, lparam),
            WM_PAINT => wndproc::handle_paint::<State, Message>(hwnd),
            WM_DISPLAYCHANGE => wndproc::handle_displaychange::<State, Message>(hwnd),
            WM_DESTROY => wndproc::handle_destroy::<State, Message>(hwnd),
            _ => {
                // Check if this is the TaskbarCreated message
                if wndproc::get_taskbar_created_message() == Some(msg) {
                    return wndproc::handle_taskbar_created::<State, Message>(hwnd);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
    };

    let (deferred_controls, has_pending_messages) =
        if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
            let state = state.deref_mut();
            state.shell.dispatch_operations(&mut state.ui_tree);

            // TODO: Maybe move this into a deferred control
            // Schedule next frame if we have active animations
            if state.smooth_scroll_manager.has_any_active_animations() {
                state.shell.request_redraw(hwnd, RedrawRequest::Immediate);
            }

            if state.shell.redraw_request == RedrawRequest::Immediate {
                let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            }

            let pending_messages = state.shell.pending_messages;
            state.shell.pending_messages = false;

            (state.shell.drain_deferred_controls(), pending_messages)
        } else {
            (None, false)
        };

    if let Some(deferred_controls) = deferred_controls {
        for control in deferred_controls {
            match control {
                DeferredControl::StartDrag { data, src_id } => {
                    let DragData::Text(text) = data;

                    if let Ok(effect) = start_text_drag(&text, true) {
                        if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
                            let state = state.deref_mut();

                            let event = Event::DragFinish { effect };

                            state
                                .shell
                                .dispatch_event_to(hwnd, &mut state.ui_tree, event, src_id);
                        }
                    }
                }

                DeferredControl::DisableIME => unsafe {
                    let himc = ImmGetContext(hwnd);
                    if !himc.is_invalid() {
                        let _ = ImmNotifyIME(himc, NI_COMPOSITIONSTR, CPS_COMPLETE, 0);
                    }
                },

                DeferredControl::SetIMEPosition { position } => unsafe {
                    let himc = ImmGetContext(hwnd);
                    if !himc.is_invalid() {
                        let to_dip = dips_scale(hwnd);
                        let cf = CANDIDATEFORM {
                            dwStyle: CFS_POINT,
                            ptCurrentPos: POINT {
                                x: (position.x / to_dip).round() as i32,
                                y: (position.y / to_dip).round() as i32,
                            },
                            rcArea: RECT::default(),
                            dwIndex: 0,
                        };
                        let _ = ImmSetCandidateWindow(himc, &cf);

                        let _ = ImmReleaseContext(hwnd, himc);
                    }
                },

                DeferredControl::SetClipboardText(text) => {
                    let _ = clipboard::set_clipboard_text(hwnd, &text);
                }
            }
        }
    }

    if has_pending_messages {
        // If the UI thread is not processing messages, notify it
        if !PENDING_MESSAGE_PROCESSING.swap(true, Ordering::SeqCst) {
            unsafe {
                PostMessageW(Some(hwnd), WM_ASYNC_MESSAGE, WPARAM(0), LPARAM(0)).ok();
            }
        }
    }

    result
}

// // Hit test the frame for resizing and moving.
// LRESULT HitTestNCA(HWND hWnd, WPARAM wParam, LPARAM lParam)
// {
//     // Get the point coordinates for the hit test.
//     POINT ptMouse = { GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam)};

//     // Get the window rectangle.
//     RECT rcWindow;
//     GetWindowRect(hWnd, &rcWindow);

//     // Get the frame rectangle, adjusted for the style without a caption.
//     RECT rcFrame = { 0 };
//     AdjustWindowRectEx(&rcFrame, WS_OVERLAPPEDWINDOW & ~WS_CAPTION, FALSE, NULL);

//     // Determine if the hit test is for resizing. Default middle (1,1).
//     USHORT uRow = 1;
//     USHORT uCol = 1;
//     bool fOnResizeBorder = false;

//     // Determine if the point is at the top or bottom of the window.
//     if (ptMouse.y >= rcWindow.top && ptMouse.y < rcWindow.top + TOPEXTENDWIDTH)
//     {
//         fOnResizeBorder = (ptMouse.y < (rcWindow.top - rcFrame.top));
//         uRow = 0;
//     }
//     else if (ptMouse.y < rcWindow.bottom && ptMouse.y >= rcWindow.bottom - BOTTOMEXTENDWIDTH)
//     {
//         uRow = 2;
//     }

//     // Determine if the point is at the left or right of the window.
//     if (ptMouse.x >= rcWindow.left && ptMouse.x < rcWindow.left + LEFTEXTENDWIDTH)
//     {
//         uCol = 0; // left side
//     }
//     else if (ptMouse.x < rcWindow.right && ptMouse.x >= rcWindow.right - RIGHTEXTENDWIDTH)
//     {
//         uCol = 2; // right side
//     }

//     // Hit test (HTTOPLEFT, ... HTBOTTOMRIGHT)
//     LRESULT hitTests[3][3] =
//     {
//         { HTTOPLEFT,    fOnResizeBorder ? HTTOP : HTCAPTION,    HTTOPRIGHT },
//         { HTLEFT,       HTNOWHERE,     HTRIGHT },
//         { HTBOTTOMLEFT, HTBOTTOM, HTBOTTOMRIGHT },
//     };

//     return hitTests[uRow][uCol];
// }

impl<
    B: Fn(&State) -> Option<crate::runtime::task::Task<Message>> + 'static,
    State: 'static,
    Message: 'static + Send + Clone,
> window::Application<B, State, Message>
{
    pub fn run(self) -> Result<()> {
        use window::Application;

        let Application {
            view_fn,
            update_fn,
            event_mapper_fn,
            boot_fn,
            state,

            title,
            width,
            height,

            backdrop,
            replace_titlebar,

            tray_config,
            tray_event_handler,

            syscommand_handler,

            scrollbar_style,
        } = self;

        WNDPROC_IMPL
            .set(Box::new(wndproc_impl::<State, Message>))
            .map_err(|_| "WNDPROC_IMPL already initialized")
            .unwrap();

        unsafe {
            // Opt-in to Per-Monitor V2 DPI awareness for crisp rendering on high-DPI displays
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

            let _ = CoInitialize(None);

            // Initialize OLE for drag-and-drop
            let _ = OleInitialize(None);

            let hinstance = GetModuleHandleW(None).unwrap();
            let class_name = PCWSTR(w!("DWriteSampleWindow").as_ptr());

            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wndproc),
                hInstance: hinstance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                lpszClassName: class_name,
                ..Default::default()
            };
            RegisterClassW(&wc);

            // Register the TaskbarCreated message for tray icon restoration
            wndproc::register_taskbar_created_message();

            // Create window first without user data
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default() | WS_EX_NOREDIRECTIONBITMAP,
                class_name,
                PCWSTR(
                    title
                        .encode_utf16()
                        .chain(std::iter::once(0))
                        .collect::<Vec<u16>>()
                        .as_ptr(),
                ),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                width as i32,  // Will be adjusted after DPI calculation
                height as i32, // Will be adjusted after DPI calculation
                None,
                None,
                Some(hinstance.into()),
                None, // No user data yet
            )
            .map_err(RuntimeError::WindowCreationFailed)?;

            // Dark mode
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &BOOL(1) as *const _ as _,
                size_of::<BOOL>() as _,
            )
            .ok();

            // (¬_¬)
            #[link(name = "uxtheme.dll", kind = "raw-dylib", modifiers = "+verbatim")]
            unsafe extern "system" {
                #[link_ordinal(135)]
                pub fn SetPreferredAppMode(preferred_app_mode: i32) -> i32;
            }

            SetPreferredAppMode(2); // Force Dark

            // DwmSetWindowAttribute(
            //     hwnd,
            //     DWMWA_CAPTION_COLOR,
            //     &COLORREF(0x000000) as *const _ as _,
            //     size_of::<COLORREF>() as _,
            // )
            // .ok();

            // For Mica effect (Windows 11 only)
            let backdrop_result = DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &match backdrop {
                    Backdrop::None => DWMSBT_NONE,
                    Backdrop::Mica => DWMSBT_MAINWINDOW,
                    Backdrop::MicaAlt => DWMSBT_TABBEDWINDOW,
                    Backdrop::Acrylic => DWMSBT_TRANSIENTWINDOW,
                } as *const _ as _,
                size_of::<DWM_SYSTEMBACKDROP_TYPE>() as _,
            );

            // Check if backdrop setting succeeded
            let backdrop_supported = backdrop_result.is_ok();

            if backdrop_supported && !matches!(backdrop, Backdrop::None) {
                let bb = DWM_BLURBEHIND {
                    dwFlags: DWM_BB_ENABLE,
                    fEnable: true.into(),
                    ..Default::default()
                };
                DwmEnableBlurBehindWindow(hwnd, &bb).ok();
            }

            if replace_titlebar {
                REPLACE_TITLEBAR.store(true, Ordering::Relaxed);

                let margins = MARGINS {
                    cxLeftWidth: -1,
                    cxRightWidth: -1,
                    cyTopHeight: -1,
                    cyBottomHeight: -1,
                    // cxLeftWidth: 0,
                    // cxRightWidth: 0,
                    // cyBottomHeight: 0,
                    // cyTopHeight: compute_standard_caption_height_for_window(hwnd)?,
                };
                DwmExtendFrameIntoClientArea(hwnd, &margins).ok();
            }

            // Now create the app handle with the hwnd
            let mut app = ApplicationHandle::new(
                view_fn,
                update_fn,
                event_mapper_fn,
                boot_fn,
                state,
                hwnd,
                tray_config,
                tray_event_handler,
                syscommand_handler,
                scrollbar_style,
            )?;

            let dips = dips_scale(hwnd);

            // Register OLE drop target
            let dt: IDropTarget = DropTarget::new(hwnd, |hwnd, event| {
                // Dispatch drag/drop events to the Shell
                if let Some(mut app_state) = state_mut_from_hwnd::<State, Message>(hwnd) {
                    let app_state = app_state.deref_mut();
                    if let Some(result) = app_state.shell.dispatch_drag_event(
                        &mut app_state.ui_tree,
                        &event,
                        match &event {
                            DragEvent::DragEnter { drag_info }
                            | DragEvent::DragOver { drag_info }
                            | DragEvent::Drop { drag_info } => drag_info.position,
                            DragEvent::DragLeave => PointDIP { x: 0.0, y: 0.0 }, // Position not needed for DragLeave
                        },
                    ) {
                        // We don't get any other events while drag is ongoing, assume we need to redraw
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        result.effect
                    } else {
                        windows::Win32::System::Ole::DROPEFFECT_NONE
                    }
                } else {
                    windows::Win32::System::Ole::DROPEFFECT_NONE
                }
            })
            .into();
            let _ = RegisterDragDrop(hwnd, &dt);
            app.drop_target = Some(dt);

            let app = Mutex::new(app);
            let boxed = Box::new(app);
            let ptr = Box::into_raw(boxed) as isize;

            // Set the window's user data to point to our Application
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr);

            // Resize window based on DPI
            SetWindowPos(
                hwnd,
                None,
                0,
                0,
                (self.width as f32 / dips) as i32,
                (self.height as f32 / dips) as i32,
                SWP_NOZORDER | SWP_NOMOVE | SWP_NOACTIVATE,
            )
            .ok();

            // We don't care if the window was previously hidden or not
            let _ = ShowWindow(hwnd, SW_SHOW);
            UpdateWindow(hwnd).ok()?;

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                // We don't care if the message was translated or not
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            // Uninitialize OLE
            OleUninitialize();
            // Uninitialize COM
            CoUninitialize();
        }
        Ok(())
    }
}
