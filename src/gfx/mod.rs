use windows::Win32::Foundation::{HWND, RECT};

use crate::dips_scale;

pub mod circle_arc;
pub mod command_executor;
pub mod command_recorder;
pub mod draw_commands;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct PointDIP {
    pub x: f32,
    pub y: f32,
}

impl PointDIP {
    pub fn within(&self, rect: RectDIP) -> bool {
        self.x >= rect.x
            && self.x <= rect.x + rect.width
            && self.y >= rect.y
            && self.y <= rect.y + rect.height
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct RectDIP {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RectDIP {
    pub fn from(hwnd: HWND, rc: RECT) -> Self {
        let to_dip = dips_scale(hwnd);

        Self {
            x: rc.left as f32 * to_dip,
            y: rc.top as f32 * to_dip,
            width: (rc.right - rc.left) as f32 * to_dip,
            height: (rc.bottom - rc.top) as f32 * to_dip,
        }
    }
}
