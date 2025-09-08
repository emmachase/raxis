use windows::{
    Win32::{
        Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND},
        System::{
            DataExchange::{
                CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
                OpenClipboard, SetClipboardData,
            },
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
            Ole::CF_UNICODETEXT,
        },
    },
    core::Result,
};

// ===== Clipboard helpers (Unicode) =====
pub fn set_clipboard_text(hwnd: HWND, s: &str) -> Result<()> {
    unsafe {
        if OpenClipboard(Some(hwnd)).is_ok() {
            let _ = EmptyClipboard();
            // Use CRLF per CF_UNICODETEXT expectations
            let crlf = s.replace('\n', "\r\n");
            let mut w: Vec<u16> = crlf.encode_utf16().collect();
            w.push(0);
            let bytes = w.len() * 2;
            let hmem: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, bytes)?;
            if !hmem.is_invalid() {
                let ptr = GlobalLock(hmem) as *mut u16;
                if !ptr.is_null() {
                    std::ptr::copy_nonoverlapping(w.as_ptr(), ptr, w.len());
                    let _ = GlobalUnlock(hmem);
                    if SetClipboardData(CF_UNICODETEXT.0.into(), Some(HANDLE(hmem.0))).is_err() {
                        let _ = GlobalFree(Some(hmem));
                    }
                    // On success, ownership is transferred to the clipboard
                } else {
                    let _ = GlobalFree(Some(hmem));
                }
            }
            let _ = CloseClipboard();
        }
    }
    Ok(())
}

pub fn get_clipboard_text(hwnd: HWND) -> Option<String> {
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT.0.into()).is_ok()
            && OpenClipboard(Some(hwnd)).is_ok()
        {
            let h = GetClipboardData(CF_UNICODETEXT.0.into());
            if let Ok(h) = h {
                let hg = HGLOBAL(h.0);
                let ptr = GlobalLock(hg) as *const u16;
                if !ptr.is_null() {
                    // Read until NUL terminator
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
                    let _ = GlobalUnlock(hg);
                    let _ = CloseClipboard();
                    let s = String::from_utf16_lossy(&out);
                    // Normalize CRLF to LF for internal text
                    return Some(s.replace("\r\n", "\n"));
                }
            }
            let _ = CloseClipboard();
        }
        None
    }
}
