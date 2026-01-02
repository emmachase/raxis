use log::{error, warn};
use windows::Win32::UI::HiDpi::GetSystemMetricsForDpi;
use std::ops::DerefMut;
use std::time::Instant;
use windows::Win32::Foundation::{D2DERR_RECREATE_TARGET, HWND, LPARAM, LRESULT, RECT};
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::Graphics::Dxgi::DXGI_PRESENT;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT, UpdateWindow,
};
use windows::Win32::System::Ole::RevokeDragDrop;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::{
    MINMAXINFO, PostQuitMessage, SM_CXFRAME, SM_CXPADDEDBORDER, SM_CYFRAME, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos
};

use crate::gfx::RectDIP;
use crate::gfx::command_executor::CommandExecutor;
use crate::runtime::util::{client_rect, state_mut_from_hwnd};
use crate::runtime::titlebar_hit_test::clear_titlebar_hit_regions;
use crate::widgets::renderer::Renderer;
use crate::widgets::Event;
use crate::{RedrawRequest, current_dpi, dips_scale_for_dpi};


/// Handle WM_SIZE
pub fn handle_size<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    lparam: LPARAM,
) -> LRESULT {
    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();
        let width = (lparam.0 & 0xFFFF) as u32;
        let height = ((lparam.0 >> 16) & 0xFFFF) as u32;
        if let Err(e) = state.on_resize(width, height) {
            error!("Failed to resize: {e}");
        }
    }
    let _ = unsafe { UpdateWindow(hwnd) };
    LRESULT(0)
}

/// Handle WM_DPICHANGED
pub fn handle_dpichanged<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    lparam: LPARAM,
    replace_titlebar: bool,
) -> LRESULT {
    let suggested = unsafe { &*(lparam.0 as *const RECT) };
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            suggested.left,
            suggested.top,
            suggested.right - suggested.left,
            suggested.bottom - suggested.top,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )
        .ok()
    };

    if replace_titlebar {
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins).ok() };
    }

    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();
        state
            .device_resources
            .borrow_mut()
            .discard_device_resources();

        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    }
    LRESULT(0)
}

/// Handle WM_GETMINMAXINFO
pub fn handle_getminmaxinfo<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    lparam: LPARAM,
) -> LRESULT {
    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();

        let min_max_info = lparam.0 as *mut MINMAXINFO;

        // Calculate the min size from the layout
        let root_node = state.ui_tree.slots[state.ui_tree.root].children[0];
        let sentinel_node = &state.ui_tree.slots[root_node];

        let dpi = current_dpi(hwnd);
        let dpi_scale = dips_scale_for_dpi(dpi);

        unsafe {
            let padding = GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);

            // Set the minimum size of the window
            (*min_max_info).ptMinTrackSize.x = (sentinel_node.min_width / dpi_scale).floor() as i32
                + GetSystemMetricsForDpi(SM_CXFRAME, dpi) * 2 + padding * 2;
            (*min_max_info).ptMinTrackSize.y = (sentinel_node.min_height / dpi_scale).floor()
                as i32
                + GetSystemMetricsForDpi(SM_CYFRAME, dpi) + padding;
        }
    }
    LRESULT(0)
}

/// Handle WM_PAINT
pub fn handle_paint<State: 'static, Message: 'static + Send + Clone>(hwnd: HWND) -> LRESULT {
    let commands = if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();
        state.shell.replace_redraw_request(RedrawRequest::Wait);

        let now = Instant::now();
        state
            .shell
            .dispatch_event(hwnd, &mut state.ui_tree, Event::Redraw { now });

        match state.on_paint(hwnd) {
            Ok(commands) => {
                let device_resources = state.device_resources.clone();
                let redraw_request = state.shell.redraw_request;

                Some((commands, device_resources, redraw_request))
            }
            Err(e) => {
                error!("Failed to paint: {e}");
                None
            }
        }
    } else {
        None
    };

    if let Some((commands, device_resources, redraw_request)) = commands {
        let mut ps = PAINTSTRUCT::default();
        let _ = unsafe { BeginPaint(hwnd, &mut ps) };

        let rc = client_rect(hwnd).unwrap();
        let device_width = rc.right.try_into().unwrap();
        let device_height = rc.bottom.try_into().unwrap();

        let mut device_resources = device_resources.borrow_mut();
        device_resources
            .create_device_resources(hwnd, device_width, device_height)
            .expect("Failed to create device resources");

        if let (rt, Some(brush)) = (
            &device_resources.d2d_device_context,
            &device_resources.solid_brush,
        ) {
            unsafe { rt.BeginDraw() };
            let white = D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            };
            unsafe { rt.Clear(Some(&white)) };

            let bounds = RectDIP::from(hwnd, rc);

            let renderer = Renderer::new(
                &device_resources.d2d_factory,
                rt,
                brush,
                &device_resources.shadow_cache,
            );

            // Start frame for cache management
            renderer.start_frame();

            CommandExecutor::execute_commands_with_bounds(&renderer, &commands, Some(bounds)).ok();

            // Evict unused cache entries before ending the frame
            renderer.evict_unused_cache_entries();

            let end = unsafe { rt.EndDraw(None, None) };
            if let Err(e) = end
                && e.code() == D2DERR_RECREATE_TARGET {
                    warn!("Recreating D2D target");
                    device_resources.discard_device_resources();
                    device_resources
                        .create_device_resources(hwnd, device_width, device_height)
                        .expect("Failed to recreate device resources");
                }
        }

        if let Some(ref swap_chain) = device_resources.dxgi_swapchain {
            let _ = unsafe { swap_chain.Present(0, DXGI_PRESENT::default()) };

            unsafe {
                device_resources
                    .dcomp_device
                    .Commit()
                    .expect("Failed to commit DirectComposition");
            }
        }

        let _ = unsafe { EndPaint(hwnd, &ps) };

        if matches!(redraw_request, RedrawRequest::Immediate) {
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
        }
    }

    LRESULT(0)
}

/// Handle WM_DISPLAYCHANGE
pub fn handle_displaychange<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
) -> LRESULT {
    let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    LRESULT(0)
}

/// Handle WM_DESTROY
pub fn handle_destroy<State: 'static, Message: 'static + Send + Clone>(hwnd: HWND) -> LRESULT {
    let _ = unsafe { RevokeDragDrop(hwnd) };

    clear_titlebar_hit_regions(hwnd);

    unsafe { PostQuitMessage(0) };
    LRESULT(0)
}
