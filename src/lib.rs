use windows::Win32::{Foundation::HWND, UI::HiDpi::GetDpiForWindow};

pub mod gfx;
pub mod math;
pub mod util;
pub mod widgets;

pub fn current_dpi(hwnd: HWND) -> f32 {
    unsafe { GetDpiForWindow(hwnd) as f32 }
}

pub fn dips_scale(hwnd: HWND) -> f32 {
    96.0f32 / current_dpi(hwnd).max(1.0)
}
