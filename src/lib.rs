use windows::Win32::{Foundation::HWND, UI::HiDpi::GetDpiForWindow};

pub mod gfx;
pub mod math;
pub mod util;
pub mod widgets;
pub mod dragdrop;

pub fn current_dpi(hwnd: HWND) -> f32 {
    unsafe { GetDpiForWindow(hwnd) as f32 }
}

pub fn dips_scale(hwnd: HWND) -> f32 {
    dips_scale_for_dpi(current_dpi(hwnd))
}

pub fn dips_scale_for_dpi(dpi: f32) -> f32 {
    96.0f32 / dpi.max(1.0)
}
