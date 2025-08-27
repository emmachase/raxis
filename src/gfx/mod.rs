use windows::Win32::Foundation::{HWND, RECT};

use crate::dips_scale;

pub mod circle_arc;
pub mod draw_commands;
pub mod command_recorder;
pub mod command_executor;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct PointDIP {
    pub x_dip: f32,
    pub y_dip: f32,
}

impl PointDIP {
    pub fn within(&self, rect: RectDIP) -> bool {
        self.x_dip >= rect.x_dip
            && self.x_dip <= rect.x_dip + rect.width_dip
            && self.y_dip >= rect.y_dip
            && self.y_dip <= rect.y_dip + rect.height_dip
    }
}

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
