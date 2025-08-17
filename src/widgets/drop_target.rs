use windows::{
    Win32::{
        Foundation::{HWND, POINTL},
        System::{
            Com::IDataObject,
            Ole::{DROPEFFECT, DROPEFFECT_NONE, IDropTarget, IDropTarget_Impl},
            SystemServices::{MK_CONTROL, MODIFIERKEYS_FLAGS},
        },
    },
    core::{Result, implement},
};

use crate::{
    gfx::PointDIP,
    widgets::{DragEvent, DragInfo, dragdrop::extract_drag_data},
};

/// Integrated drop target that works with the Shell and widget system
#[implement(IDropTarget)]
pub struct DropTarget {
    hwnd: HWND,
    /// Callback to dispatch events to the Shell, receives HWND as parameter
    event_dispatcher: Box<dyn Fn(HWND, DragEvent) -> DROPEFFECT + Send>,
}

impl DropTarget {
    pub fn new<F>(hwnd: HWND, event_dispatcher: F) -> Self
    where
        F: Fn(HWND, DragEvent) -> DROPEFFECT + Send + 'static,
    {
        Self {
            hwnd,
            event_dispatcher: Box::new(event_dispatcher),
        }
    }

    fn choose_effect(&self, keys: MODIFIERKEYS_FLAGS) -> DROPEFFECT {
        if (keys.0 & MK_CONTROL.0) != 0 {
            windows::Win32::System::Ole::DROPEFFECT_COPY
        } else {
            windows::Win32::System::Ole::DROPEFFECT_MOVE
        }
    }

    fn screen_to_client_dip(&self, screen_point: &POINTL) -> PointDIP {
        use windows::Win32::{Foundation::POINT, Graphics::Gdi::ScreenToClient};

        unsafe {
            let mut p = POINT {
                x: screen_point.x,
                y: screen_point.y,
            };
            let _ = ScreenToClient(self.hwnd, &mut p);
            let to_dip = crate::dips_scale(self.hwnd);
            let x_dip = (p.x as f32) * to_dip;
            let y_dip = (p.y as f32) * to_dip;
            PointDIP { x_dip, y_dip }
        }
    }

    fn create_drag_info(
        &self,
        data_object: &IDataObject,
        point: &POINTL,
        allowed_effects: DROPEFFECT,
    ) -> Option<DragInfo> {
        if let Some(data) = extract_drag_data(data_object) {
            let position = self.screen_to_client_dip(point);
            Some(DragInfo {
                data,
                position,
                allowed_effects,
            })
        } else {
            None
        }
    }
}

// We don't control the trait so we can't mark the functions unsafe
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[allow(non_snake_case)]
impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(
        &self,
        pDataObj: windows_core::Ref<'_, IDataObject>,
        grfKeyState: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdwEffect: *mut DROPEFFECT,
    ) -> Result<()> {
        let effect = if let Some(data_obj) = pDataObj.as_ref() {
            if let Some(drag_info) =
                self.create_drag_info(data_obj, pt, self.choose_effect(grfKeyState))
            {
                let event = DragEvent::DragEnter { drag_info };
                (self.event_dispatcher)(self.hwnd, event)
            } else {
                DROPEFFECT_NONE
            }
        } else {
            DROPEFFECT_NONE
        };

        if !pdwEffect.is_null() {
            unsafe {
                *pdwEffect = effect;
            }
        }
        Ok(())
    }

    fn DragOver(
        &self,
        grfKeyState: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdwEffect: *mut DROPEFFECT,
    ) -> Result<()> {
        // For DragOver, we create a placeholder drag info since we don't have the data object
        let position = self.screen_to_client_dip(pt);
        let drag_info = DragInfo {
            data: crate::widgets::DragData::Text(String::new()), // Placeholder
            position,
            allowed_effects: self.choose_effect(grfKeyState),
        };

        let event = DragEvent::DragOver { drag_info };
        let effect = (self.event_dispatcher)(self.hwnd, event);

        if !pdwEffect.is_null() {
            unsafe {
                *pdwEffect = effect;
            }
        }
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        let event = DragEvent::DragLeave;
        let _ = (self.event_dispatcher)(self.hwnd, event);
        Ok(())
    }

    fn Drop(
        &self,
        pDataObj: windows_core::Ref<'_, IDataObject>,
        grfKeyState: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdwEffect: *mut DROPEFFECT,
    ) -> Result<()> {
        let effect = if let Some(data_obj) = pDataObj.as_ref() {
            if let Some(drag_info) =
                self.create_drag_info(data_obj, pt, self.choose_effect(grfKeyState))
            {
                let event = DragEvent::Drop { drag_info };
                (self.event_dispatcher)(self.hwnd, event)
            } else {
                DROPEFFECT_NONE
            }
        } else {
            DROPEFFECT_NONE
        };

        if !pdwEffect.is_null() {
            unsafe {
                *pdwEffect = effect;
            }
        }
        Ok(())
    }
}
