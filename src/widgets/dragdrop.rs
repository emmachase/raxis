use windows::Win32::{
    Foundation::POINTL,
    System::{
        Com::IDataObject,
        Ole::{DROPEFFECT, DROPEFFECT_NONE},
    },
};

use crate::{Shell, gfx::PointDIP, layout::model::UIKey};

/// Represents the data being dragged
#[derive(Debug, Clone)]
pub enum DragData {
    Text(String),
    // Future: Files(Vec<PathBuf>), Custom(Box<dyn Any>), etc.
}

/// Information about a drag operation
#[derive(Debug, Clone)]
pub struct DragInfo {
    pub data: DragData,
    pub position: PointDIP,
    pub allowed_effects: DROPEFFECT,
}

/// Result of a drag/drop operation
#[derive(Debug, Clone)]
pub struct DropResult {
    pub effect: DROPEFFECT,
    pub handled: bool,
}

impl Default for DropResult {
    fn default() -> Self {
        Self {
            effect: DROPEFFECT_NONE,
            handled: false,
        }
    }
}

/// Trait for widgets that can participate in drag and drop operations
pub trait DragDropWidget {
    /// Called when a drag operation enters the widget's bounds
    /// Returns the drop effect that would be applied if dropped here
    fn drag_enter(
        &mut self,
        drag_info: &DragInfo,
        widget_bounds: crate::gfx::RectDIP,
    ) -> DROPEFFECT {
        let _ = (drag_info, widget_bounds);
        DROPEFFECT_NONE
    }

    /// Called when a drag operation moves within the widget's bounds
    /// Returns the drop effect that would be applied if dropped here
    fn drag_over(
        &mut self,
        drag_info: &DragInfo,
        widget_bounds: crate::gfx::RectDIP,
    ) -> DROPEFFECT {
        let _ = (drag_info, widget_bounds);
        DROPEFFECT_NONE
    }

    /// Called when a drag operation leaves the widget's bounds
    fn drag_leave(&mut self, _widget_bounds: crate::gfx::RectDIP) {}

    /// Called when data is dropped on the widget
    /// Returns the actual effect that was applied and whether it was handled
    fn drop(
        &mut self,
        _id: Option<u64>,
        _key: UIKey,
        _shell: &mut Shell,
        drag_info: &DragInfo,
        _widget_bounds: crate::gfx::RectDIP,
    ) -> DropResult {
        let _ = drag_info;
        DropResult::default()
    }

    /// Called to check if the widget can initiate a drag operation at the given point
    /// Returns Some(DragData) if a drag can be started, None otherwise
    fn can_drag(&self, position: PointDIP) -> Option<DragData> {
        let _ = position;
        None
    }

    /// Called when a drag operation initiated by this widget begins
    fn drag_start(&mut self, data: &DragData) {
        let _ = data;
    }

    /// Called when a drag operation initiated by this widget ends
    /// `effect` indicates what happened (copy, move, cancel, etc.)
    fn drag_end(&mut self, data: &DragData, effect: DROPEFFECT) {
        let _ = (data, effect);
    }
}

/// Helper to extract DragData from IDataObject
pub fn extract_drag_data(data_object: &IDataObject) -> Option<DragData> {
    use windows::Win32::System::{
        Com::{DVASPECT_CONTENT, FORMATETC, STGMEDIUM, TYMED_HGLOBAL},
        Memory::{GlobalLock, GlobalUnlock},
        Ole::{CF_UNICODETEXT, ReleaseStgMedium},
    };

    unsafe {
        // Try to get Unicode text
        let fmt = FORMATETC {
            cfFormat: CF_UNICODETEXT.0,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };

        if let Ok(mut medium) = data_object.GetData(&fmt) {
            let h = medium.u.hGlobal;
            let ptr = GlobalLock(h) as *const u16;
            if !ptr.is_null() {
                // Read until NUL
                let mut out: Vec<u16> = Vec::new();
                let mut i = 0isize;
                loop {
                    let v = *ptr.offset(i);
                    if v == 0 {
                        break;
                    }
                    out.push(v);
                    i += 1;
                }
                let _ = GlobalUnlock(h);
                let s = String::from_utf16_lossy(&out);
                ReleaseStgMedium(&mut medium as *mut STGMEDIUM);
                return Some(DragData::Text(s.replace("\r\n", "\n")));
            }
            ReleaseStgMedium(&mut medium as *mut STGMEDIUM);
        }
    }

    None
}

/// Helper to convert screen coordinates to widget-relative coordinates
pub fn screen_to_widget_coords(
    hwnd: windows::Win32::Foundation::HWND,
    screen_point: &POINTL,
    widget_bounds: crate::gfx::RectDIP,
) -> PointDIP {
    use windows::Win32::{Foundation::POINT, Graphics::Gdi::ScreenToClient};

    unsafe {
        let mut p = POINT {
            x: screen_point.x,
            y: screen_point.y,
        };
        let _ = ScreenToClient(hwnd, &mut p);
        let to_dip = crate::dips_scale(hwnd);
        PointDIP {
            x_dip: (p.x as f32) * to_dip - widget_bounds.x_dip,
            y_dip: (p.y as f32) * to_dip - widget_bounds.y_dip,
        }
    }
}
