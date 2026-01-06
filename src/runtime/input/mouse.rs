use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageTime, GetSystemMetrics, SM_CXDOUBLECLK, SM_CYDOUBLECLK,
};

/// Tracks multi-click state for mouse button events
#[derive(Debug, Clone, Copy)]
pub struct MouseState {
    pub last_click_time: u32,
    pub last_click_pos: POINT,
    pub click_count: u32,
}

impl Default for MouseState {
    fn default() -> Self {
        Self {
            last_click_time: 0,
            last_click_pos: POINT { x: 0, y: 0 },
            click_count: 0,
        }
    }
}

impl MouseState {
    /// Updates the click count based on double-click timing and distance thresholds.
    /// Returns the updated click count.
    pub fn update_click_count(&mut self, pos: POINT) -> u32 {
        unsafe {
            let now = GetMessageTime() as u32;
            let thresh = GetDoubleClickTime();
            let dx = (pos.x - self.last_click_pos.x).unsigned_abs();
            let dy = (pos.y - self.last_click_pos.y).unsigned_abs();
            let w = GetSystemMetrics(SM_CXDOUBLECLK) as u32 / 2;
            let h = GetSystemMetrics(SM_CYDOUBLECLK) as u32 / 2;
            let within_rect = dx <= w && dy <= h;
            let within_time = now.wrapping_sub(self.last_click_time) <= thresh;

            if within_time && within_rect {
                self.click_count = self.click_count.saturating_add(1);
            } else {
                self.click_count = 1;
            }

            self.last_click_time = now;
            self.last_click_pos = pos;

            self.click_count
        }
    }
}
