use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::Controls::{
    CS_ACTIVE, CloseThemeData, GetThemePartSize, OpenThemeData, TS_TRUE, WP_CAPTION,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, GWL_STYLE, GetWindowLongW, GetWindowRect, HTBOTTOM, HTBOTTOMLEFT,
    HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTCLOSE, HTLEFT, HTMAXBUTTON, HTMINBUTTON, HTNOWHERE,
    HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, WINDOW_EX_STYLE, WINDOW_STYLE, WS_CAPTION,
    WS_EX_NOREDIRECTIONBITMAP
};
use windows::core::w;

use crate::{dips_scale, runtime::Result};
use crate::runtime::titlebar_hit_test::get_titlebar_hit_regions;

/// Non-client area hit testing for custom titlebar
pub fn hit_test_nca(hwnd: HWND, _wparam: WPARAM, lparam: LPARAM) -> u32 {
    let x_px = (lparam.0 & 0xFFFF) as i16 as i32;
    let y_px = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

    if let Some(regions) = get_titlebar_hit_regions(hwnd) {
        if let Some(rc) = regions.close
            && x_px >= rc.left && x_px < rc.right && y_px >= rc.top && y_px < rc.bottom {
                return HTCLOSE;
            }
        if let Some(rc) = regions.maximize
            && x_px >= rc.left && x_px < rc.right && y_px >= rc.top && y_px < rc.bottom {
                return HTMAXBUTTON;
            }
        if let Some(rc) = regions.minimize
            && x_px >= rc.left && x_px < rc.right && y_px >= rc.top && y_px < rc.bottom {
                return HTMINBUTTON;
            }
    }

    let mut rc_window = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rc_window).unwrap() };

    if x_px < rc_window.left
        || x_px >= rc_window.right
        || y_px < rc_window.top
        || y_px >= rc_window.bottom
    {
        return HTNOWHERE;
    }

    let mut rc_frame = RECT::default();
    let window_style = WINDOW_STYLE(unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32);
    unsafe {
        AdjustWindowRectEx(
            &mut rc_frame,
            window_style & !WS_CAPTION,
            false,
            WS_EX_NOREDIRECTIONBITMAP,
        )
        .unwrap()
    };

    let mut u_row = 1;
    let mut u_col = 1;
    let mut f_on_resize_border = false;

    let dpi_scale = dips_scale(hwnd);

    let topextendwidth: i32 =
        compute_standard_caption_height_for_window(hwnd).unwrap_or((28.0 / dpi_scale) as i32);
    let bottomextendwidth: i32 = (10.0 / dpi_scale) as i32;
    let leftextendwidth: i32 = (10.0 / dpi_scale) as i32;
    let rightextendwidth: i32 = (10.0 / dpi_scale) as i32;

    if y_px >= rc_window.top && y_px < rc_window.top + topextendwidth {
        f_on_resize_border = y_px < (rc_window.top - rc_frame.top);
        u_row = 0;
    } else if y_px < rc_window.bottom && y_px >= rc_window.bottom - bottomextendwidth {
        u_row = 2;
    }

    if x_px >= rc_window.left && x_px < rc_window.left + leftextendwidth {
        u_col = 0;
    } else if x_px < rc_window.right && x_px >= rc_window.right - rightextendwidth {
        u_col = 2;
    }

    let hit_tests = [
        [
            HTTOPLEFT,
            if f_on_resize_border { HTTOP } else { HTCAPTION },
            HTTOPRIGHT,
        ],
        [HTLEFT, HTCLIENT, HTRIGHT],
        [HTBOTTOMLEFT, HTBOTTOM, HTBOTTOMRIGHT],
    ];

    hit_tests[u_row][u_col]
}

/// Compute the standard caption height for a window based on theme and DPI
pub fn compute_standard_caption_height_for_window(window_handle: HWND) -> Result<i32> {
    let accounting_for_borders = -1;
    let theme = unsafe { OpenThemeData(Some(window_handle), w!("WINDOW")) };
    let dpi = unsafe { GetDpiForWindow(window_handle) };
    let caption_size =
        unsafe { GetThemePartSize(theme, None, WP_CAPTION.0, CS_ACTIVE.0, None, TS_TRUE)? };
    unsafe { CloseThemeData(theme)? };

    let height = (caption_size.cy as f32 * dpi as f32) / 96.0;
    Ok((height as i32) + accounting_for_borders)
}
