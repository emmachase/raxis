use windows::Win32::Foundation::{HWND, RECT};

use crate::dips_scale;

pub mod circle_arc;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct RectDIP {
    pub x_dip: f32,
    pub y_dip: f32,
    pub width_dip: f32,
    pub height_dip: f32,
}

impl RectDIP {
    pub fn from(hwnd: HWND, rc: RECT) -> Self {
        let to_dip = dips_scale(hwnd);

        Self {
            x_dip: rc.left as f32 * to_dip,
            y_dip: rc.top as f32 * to_dip,
            width_dip: (rc.right - rc.left) as f32 * to_dip,
            height_dip: (rc.bottom - rc.top) as f32 * to_dip,
        }
    }
}
