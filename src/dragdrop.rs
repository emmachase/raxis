use std::mem::ManuallyDrop;

use windows::{
    Win32::{
        Foundation::{
            DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, DV_E_FORMATETC,
            E_NOTIMPL, HWND, S_OK,
        },
        System::{
            Com::{IAdviseSink, IDataObject, IDataObject_Impl, STGMEDIUM, TYMED, TYMED_HGLOBAL},
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
            Ole::{
                DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_MOVE, DoDragDrop, IDropSource,
                IDropSource_Impl,
            },
            SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS},
        },
    },
    core::{HRESULT, implement},
};

use windows::Win32::Foundation::{E_OUTOFMEMORY, E_POINTER};
use windows::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, IEnumFORMATETC};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows_core::BOOL;

// Minimal IDropSource implementation: default cursors and basic continue/cancel/drop policy
#[implement(IDropSource)]
struct DropSource;

impl DropSource {
    fn new() -> Self {
        Self
    }
}

#[allow(non_snake_case)]
impl IDropSource_Impl for DropSource_Impl {
    fn QueryContinueDrag(&self, fEscapePressed: BOOL, grfKeyState: MODIFIERKEYS_FLAGS) -> HRESULT {
        if fEscapePressed.as_bool() {
            // Cancel drag
            return DRAGDROP_S_CANCEL;
        }
        // Drop when left button released
        if (grfKeyState.0 as u32 & MK_LBUTTON.0 as u32) == 0 {
            return DRAGDROP_S_DROP;
        }

        // Continue dragging
        S_OK
    }

    fn GiveFeedback(&self, _dwEffect: DROPEFFECT) -> HRESULT {
        // Use default cursors
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

// Simple text data object that serves CF_UNICODETEXT from an internal buffer
#[implement(IDataObject)]
struct TextDataObject {
    text_w: Vec<u16>, // zero-terminated UTF-16
}

impl TextDataObject {
    fn new(text: &str) -> Self {
        let mut v: Vec<u16> = text.encode_utf16().collect();
        v.push(0);
        Self { text_w: v }
    }
}

#[allow(non_snake_case)]
impl IDataObject_Impl for TextDataObject_Impl {
    fn GetData(&self, pformatetcIn: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        unsafe {
            let fmt = pformatetcIn
                .as_ref()
                .ok_or_else(|| windows::core::Error::from(E_POINTER))?;
            if fmt.cfFormat != CF_UNICODETEXT.0
                || fmt.tymed != TYMED_HGLOBAL.0 as u32
                || fmt.dwAspect != DVASPECT_CONTENT.0 as u32
                || fmt.lindex != -1
            {
                return Err(DV_E_FORMATETC.into());
            }

            let bytes = (self.text_w.len() * 2) as usize;
            let hglobal = GlobalAlloc(GMEM_MOVEABLE, bytes);
            if hglobal.is_err() {
                return Err(E_OUTOFMEMORY.into());
            }
            let ptr = GlobalLock(hglobal.as_ref().unwrap().clone()) as *mut u8;
            if ptr.is_null() {
                return Err(E_OUTOFMEMORY.into());
            }
            std::ptr::copy_nonoverlapping(self.text_w.as_ptr() as *const u8, ptr, bytes);
            let _ = GlobalUnlock(hglobal.as_ref().unwrap().clone());

            Ok(STGMEDIUM {
                tymed: TYMED_HGLOBAL.0 as u32,
                u: windows::Win32::System::Com::STGMEDIUM_0 {
                    hGlobal: hglobal.unwrap(),
                },
                pUnkForRelease: ManuallyDrop::new(None),
            })
        }
    }

    fn GetDataHere(
        &self,
        _pformatetc: *const FORMATETC,
        _pmedium: *mut STGMEDIUM,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> HRESULT {
        unsafe {
            let fmt = pformatetc
                .as_ref()
                .ok_or_else(|| windows::core::Error::from(E_POINTER))
                .unwrap();
            if fmt.cfFormat == CF_UNICODETEXT.0
                && fmt.tymed & TYMED_HGLOBAL.0 as u32 != 0
                && fmt.dwAspect == DVASPECT_CONTENT.0 as u32
                && fmt.lindex == -1
            {
                S_OK
            } else {
                DV_E_FORMATETC
            }
        }
    }

    fn GetCanonicalFormatEtc(
        &self,
        _pformatectIn: *const FORMATETC,
        _pformatectOut: *mut FORMATETC,
    ) -> HRESULT {
        E_NOTIMPL
    }

    fn SetData(
        &self,
        _pformatetc: *const FORMATETC,
        _pmedium: *const STGMEDIUM,
        _fRelease: BOOL,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumFormatEtc(&self, _dwDirection: u32) -> windows::core::Result<IEnumFORMATETC> {
        Err(E_NOTIMPL.into())
    }

    fn DAdvise(
        &self,
        _pformatetc: *const FORMATETC,
        _advf: u32,
        _pAdvSink: windows_core::Ref<'_, IAdviseSink>,
    ) -> windows::core::Result<u32> {
        Err(E_NOTIMPL.into())
    }

    fn DUnadvise(&self, _dwConnection: u32) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumDAdvise(&self) -> windows::core::Result<windows::Win32::System::Com::IEnumSTATDATA> {
        Err(E_NOTIMPL.into())
    }
}

// Public helper: start a text drag with COPY and MOVE allowed/forbidden
pub fn start_text_drag(text: &str, allow_move: bool) -> windows::core::Result<DROPEFFECT> {
    unsafe {
        let data: IDataObject = TextDataObject::new(text).into();
        let src: IDropSource = DropSource::new().into();
        let mut effect = DROPEFFECT(0);
        let allowed = if allow_move {
            DROPEFFECT_COPY.0 | DROPEFFECT_MOVE.0
        } else {
            DROPEFFECT_COPY.0
        };
        let hr = DoDragDrop(&data, &src, DROPEFFECT(allowed), &mut effect);
        // DoDragDrop returns DRAGDROP_S_DROP, DRAGDROP_S_CANCEL, or error; we ignore hr and return effect
        let _ = hr; // suppress unused
        println!("DragDrop result: {:?}", hr);
        Ok(effect)
    }
}

// end
