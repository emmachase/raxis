use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;

#[derive(Clone, Copy, Debug, Default)]
pub struct TitlebarHitRegions {
    pub minimize: Option<RECT>,
    pub maximize: Option<RECT>,
    pub close: Option<RECT>,
}

static TITLEBAR_HIT_REGIONS: OnceLock<Mutex<HashMap<usize, TitlebarHitRegions>>> = OnceLock::new();

fn map() -> &'static Mutex<HashMap<usize, TitlebarHitRegions>> {
    TITLEBAR_HIT_REGIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn set_titlebar_hit_regions(hwnd: HWND, regions: TitlebarHitRegions) {
    let mut guard = map().lock().unwrap();
    guard.insert(hwnd.0 as usize, regions);
}

pub fn clear_titlebar_hit_regions(hwnd: HWND) {
    let mut guard = map().lock().unwrap();
    guard.remove(&(hwnd.0 as usize));
}

pub fn get_titlebar_hit_regions(hwnd: HWND) -> Option<TitlebarHitRegions> {
    let guard = map().lock().unwrap();
    guard.get(&(hwnd.0 as usize)).copied()
}

pub fn client_origin_screen_px(hwnd: HWND) -> Option<POINT> {
    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        ClientToScreen(hwnd, &mut pt).ok().ok()?;
        Some(pt)
    }
}
