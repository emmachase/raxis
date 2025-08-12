// #![windows_subsystem = "windows"]

use glux::{
    current_dpi, dips_scale, dips_scale_for_dpi,
    gfx::RectDIP,
    widgets::{selectable_text::SelectableText, spinner::Spinner},
};
use std::{ffi::c_void, sync::OnceLock};
use windows::{
    Win32::{
        Foundation::{
            D2DERR_RECREATE_TARGET, GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, POINT,
            RECT, WPARAM,
        },
        Graphics::{
            Direct2D::{
                Common::{D2D_SIZE_U, D2D1_ALPHA_MODE_UNKNOWN, D2D1_COLOR_F, D2D1_PIXEL_FORMAT},
                D2D1_DEBUG_LEVEL_NONE, D2D1_FACTORY_OPTIONS, D2D1_FACTORY_TYPE_SINGLE_THREADED,
                D2D1_FEATURE_LEVEL_DEFAULT, D2D1_HWND_RENDER_TARGET_PROPERTIES,
                D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES,
                D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1CreateFactory,
                ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
            },
            DirectWrite::{
                DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_WEIGHT_REGULAR, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
                DWRITE_TEXT_ALIGNMENT_CENTER, DWriteCreateFactory, IDWriteFactory,
                IDWriteTextFormat,
            },
            Dwm::{DWM_TIMING_INFO, DwmGetCompositionTimingInfo},
            Dxgi::Common::DXGI_FORMAT_UNKNOWN,
            Gdi::{
                BeginPaint, EndPaint, GetDC, GetDeviceCaps, InvalidateRect, PAINTSTRUCT, ReleaseDC,
                ScreenToClient, UpdateWindow, VREFRESH,
            },
        },
        System::{
            DataExchange::{
                CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
                OpenClipboard, SetClipboardData,
            },
            LibraryLoader::GetModuleHandleW,
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
            Ole::CF_UNICODETEXT,
        },
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            Input::{
                Ime::{
                    CANDIDATEFORM, CFS_FORCE_POSITION, CFS_POINT, CPS_COMPLETE, GCS_COMPSTR,
                    GCS_CURSORPOS, GCS_RESULTSTR, ImmGetCompositionStringW, ImmGetContext,
                    ImmNotifyIME, ImmReleaseContext, ImmSetCandidateWindow, NI_COMPOSITIONSTR,
                },
                KeyboardAndMouse::{
                    GetKeyState, ReleaseCapture, SetCapture, SetFocus, VK_A, VK_BACK, VK_C,
                    VK_CONTROL, VK_DELETE, VK_END, VK_HOME, VK_LEFT, VK_RIGHT, VK_SHIFT, VK_V,
                    VK_X,
                },
            },
            WindowsAndMessaging::{
                self as WAM, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW,
                DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetClientRect, GetCursorPos,
                GetMessageW, HCURSOR, HTCLIENT, IDC_ARROW, IDC_IBEAM, LoadCursorW, MSG,
                RegisterClassW, SW_SHOW, SWP_NOACTIVATE, SWP_NOZORDER, SetCursor,
                SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, WINDOW_EX_STYLE,
                WM_CHAR, WM_COPY, WM_CREATE, WM_CUT, WM_DESTROY, WM_DISPLAYCHANGE,
                WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_STARTCOMPOSITION, WM_KEYDOWN,
                WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_PASTE, WM_SETCURSOR,
                WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
            },
        },
    },
    core::{PCWSTR, Result, w},
};
use windows_numerics::Vector2;

pub struct SafeCursor(pub HCURSOR);
unsafe impl Send for SafeCursor {}
unsafe impl Sync for SafeCursor {}

static IBEAM_CURSOR: OnceLock<Option<SafeCursor>> = OnceLock::new();

const TEXT: &str = "Hello, בְּרֵאשִׁ֖ית בָּרָ֣א אֱלֹהִ֑ים אֵ֥ת הַשָּׁמַ֖יִם וְאֵ֥ת הָאָֽרֶץ.  DirectWrite! こんにちは 😍";

// Small helpers to reduce duplication and centralize Win32/DPI logic.
fn state_mut_from_hwnd(hwnd: HWND) -> Option<&'static mut AppState> {
    unsafe {
        let ptr = WAM::GetWindowLongPtrW(hwnd, GWLP_USERDATA);
        if ptr != 0 {
            Some(&mut *(ptr as *mut AppState))
        } else {
            None
        }
    }
}

fn client_rect(hwnd: HWND) -> Result<RECT> {
    unsafe {
        let mut rc = RECT::default();
        GetClientRect(hwnd, &mut rc)?;
        Ok(rc)
    }
}

fn apply_dpi_to_rt(rt: &ID2D1HwndRenderTarget, hwnd: HWND) {
    let dpi = current_dpi(hwnd);
    unsafe { rt.SetDpi(dpi, dpi) };
}

// ===== Clipboard helpers (Unicode) =====
fn set_clipboard_text(hwnd: HWND, s: &str) -> Result<()> {
    unsafe {
        if OpenClipboard(Some(hwnd)).is_ok() {
            let _ = EmptyClipboard();
            // Use CRLF per CF_UNICODETEXT expectations
            let crlf = s.replace('\n', "\r\n");
            let mut w: Vec<u16> = crlf.encode_utf16().collect();
            w.push(0);
            let bytes = (w.len() * 2) as usize;
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

fn get_clipboard_text(hwnd: HWND) -> Option<String> {
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT.0.into()).is_ok() {
            if OpenClipboard(Some(hwnd)).is_ok() {
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
        }
        None
    }
}

struct AppState {
    d2d_factory: ID2D1Factory,
    _dwrite_factory: IDWriteFactory,
    _text_format: IDWriteTextFormat,
    render_target: Option<ID2D1HwndRenderTarget>,
    black_brush: Option<ID2D1SolidColorBrush>,

    clock: f64,
    timing_info: DWM_TIMING_INFO,
    spinner: Spinner,

    // Selectable text widget encapsulating layout, selection, and bounds
    text_widget: SelectableText,

    // For combining UTF-16 surrogate pairs from WM_CHAR
    pending_high_surrogate: Option<u16>,
}

impl AppState {
    fn new() -> Result<Self> {
        unsafe {
            let options = D2D1_FACTORY_OPTIONS {
                debugLevel: D2D1_DEBUG_LEVEL_NONE,
            };
            let d2d_factory: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, Some(&options))?;

            let dwrite_factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;

            let text_format = dwrite_factory.CreateTextFormat(
                PCWSTR(w!("Segoe UI").as_ptr()),
                None,
                DWRITE_FONT_WEIGHT_REGULAR,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                72.0,
                PCWSTR(w!("en-us").as_ptr()),
            )?;
            text_format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            text_format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;

            let spinner = Spinner::new(6.0, 200.0, 1.6);

            // Build selectable text widget using shared DWrite factory/format
            let text_widget = SelectableText::new(
                dwrite_factory.clone(),
                text_format.clone(),
                TEXT.to_string(),
            );

            Ok(Self {
                d2d_factory,
                _dwrite_factory: dwrite_factory,
                _text_format: text_format,
                render_target: None,
                black_brush: None,
                clock: 0.0,
                timing_info: DWM_TIMING_INFO::default(),
                spinner,
                text_widget,
                pending_high_surrogate: None,
            })
        }
    }

    fn create_device_resources(&mut self, hwnd: HWND) -> Result<()> {
        unsafe {
            if self.render_target.is_none() {
                let rc = client_rect(hwnd)?;
                let size = D2D_SIZE_U {
                    width: (rc.right - rc.left) as u32,
                    height: (rc.bottom - rc.top) as u32,
                };

                let rt_props = D2D1_RENDER_TARGET_PROPERTIES {
                    r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_UNKNOWN,
                        alphaMode: D2D1_ALPHA_MODE_UNKNOWN,
                    },
                    dpiX: 0.0,
                    dpiY: 0.0,
                    usage: D2D1_RENDER_TARGET_USAGE_NONE,
                    minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
                };
                let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                    hwnd,
                    pixelSize: size,
                    presentOptions: D2D1_PRESENT_OPTIONS_NONE,
                };

                let rt = self
                    .d2d_factory
                    .CreateHwndRenderTarget(&rt_props, &hwnd_props)?;
                // Ensure the render target renders at the window's DPI for crisp output
                apply_dpi_to_rt(&rt, hwnd);
                self.render_target = Some(rt);
            }
            if self.black_brush.is_none() {
                if let Some(rt) = &self.render_target {
                    let black = D2D1_COLOR_F {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    };
                    let brush = rt.CreateSolidColorBrush(&black, None)?;
                    self.black_brush = Some(brush);
                }
            }
            Ok(())
        }
    }

    fn discard_device_resources(&mut self) {
        self.black_brush = None;
        self.render_target = None;
    }

    fn update_dpi(&mut self, hwnd: HWND) {
        if let Some(rt) = &self.render_target {
            apply_dpi_to_rt(rt, hwnd);
        }
    }

    fn on_paint(&mut self, hwnd: HWND) -> Result<()> {
        let dt = self.timing_info.rateCompose.uiDenominator as f64
            / self.timing_info.rateCompose.uiNumerator as f64;

        unsafe {
            self.create_device_resources(hwnd)?;
            // Refresh target DPI in case it changed (e.g. monitor move)
            self.update_dpi(hwnd);
            let mut ps = PAINTSTRUCT::default();
            BeginPaint(hwnd, &mut ps);

            if let (d2d_factory, Some(rt), Some(brush)) =
                (&self.d2d_factory, &self.render_target, &self.black_brush)
            {
                rt.BeginDraw();
                let white = D2D1_COLOR_F {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                };
                rt.Clear(Some(&white));

                let rc = client_rect(hwnd)?;
                let rc_dip = RectDIP::from(hwnd, rc);
                let to_dip = dips_scale(hwnd);

                brush.SetColor(&D2D1_COLOR_F {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                });

                let _ = self.text_widget.update_bounds(rc_dip);
                let _ = self.text_widget.draw(rt, brush, dt);

                let center = Vector2 {
                    X: 100.0 * to_dip,
                    Y: 100.0 * to_dip,
                };
                let radius = 64.0 * to_dip;
                self.spinner.set_layout(center, radius);
                let dt = self.timing_info.rateCompose.uiDenominator as f32
                    / self.timing_info.rateCompose.uiNumerator as f32;
                self.spinner.update(dt);
                self.spinner.draw(d2d_factory, rt, brush)?;

                // Spinner drawn above uses the current brush color.

                let end = rt.EndDraw(None, None);
                if let Err(e) = end {
                    if e.code() == D2DERR_RECREATE_TARGET {
                        self.discard_device_resources();
                    }
                }
            }

            let _ = EndPaint(hwnd, &ps);

            // Request a paint so that we can animate
            let _ = InvalidateRect(Some(hwnd), None, false);
        }

        self.clock += dt;

        Ok(())
    }

    fn on_resize(&mut self, width: u32, height: u32) -> Result<()> {
        if let Some(rt) = &self.render_target {
            unsafe {
                rt.Resize(&D2D_SIZE_U { width, height })?;
            }
        }
        Ok(())
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_IME_STARTCOMPOSITION => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    state.text_widget.ime_begin();
                    // Position IME window at current caret
                    let to_dip = dips_scale(hwnd);
                    if let Ok((x_dip, y_dip, h)) = state
                        .text_widget
                        .caret_pos_dip(state.text_widget.caret_active16())
                    {
                        let x_px = (x_dip / to_dip).round() as i32;
                        let y_px = ((y_dip + h) / to_dip).round() as i32;
                        let himc = ImmGetContext(hwnd);
                        if !himc.is_invalid() {
                            let cf = CANDIDATEFORM {
                                dwStyle: CFS_POINT,
                                ptCurrentPos: POINT { x: x_px, y: y_px },
                                rcArea: RECT::default(),
                                dwIndex: 0,
                            };
                            let _ = ImmSetCandidateWindow(himc, &cf);

                            let _ = ImmReleaseContext(hwnd, himc);
                        }
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_IME_COMPOSITION => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    let himc = ImmGetContext(hwnd);
                    if !himc.is_invalid() {
                        let flags = lparam.0 as u32;

                        // Handle result string (committed text)
                        if flags & GCS_RESULTSTR.0 != 0 {
                            let bytes = ImmGetCompositionStringW(himc, GCS_RESULTSTR, None, 0);
                            if bytes > 0 {
                                let mut buf: Vec<u16> = vec![0; (bytes as usize) / 2];
                                let _ = ImmGetCompositionStringW(
                                    himc,
                                    GCS_RESULTSTR,
                                    Some(buf.as_mut_ptr() as *mut _),
                                    bytes as u32,
                                );
                                let s = String::from_utf16_lossy(&buf);
                                let _ = state.text_widget.ime_commit(s);
                            }
                        }

                        // Handle ongoing composition string
                        if flags & GCS_COMPSTR.0 != 0 {
                            let bytes = ImmGetCompositionStringW(himc, GCS_COMPSTR, None, 0);
                            let mut comp = String::new();
                            if bytes > 0 {
                                let mut buf: Vec<u16> = vec![0; (bytes as usize) / 2];
                                let _ = ImmGetCompositionStringW(
                                    himc,
                                    GCS_COMPSTR,
                                    Some(buf.as_mut_ptr() as *mut _),
                                    bytes as u32,
                                );
                                comp = String::from_utf16_lossy(&buf);
                            }
                            // Caret within comp string (UTF-16 units)
                            let caret_units = {
                                let v = ImmGetCompositionStringW(himc, GCS_CURSORPOS, None, 0);
                                if v < 0 { 0 } else { v as u32 }
                            };
                            state.text_widget.ime_update(comp, caret_units);

                            // Reposition IME window at composition caret
                            let to_dip = dips_scale(hwnd);
                            if let Ok((x_dip, y_dip, h)) = state.text_widget.ime_caret_pos_dip() {
                                let x_px = (x_dip / to_dip).round() as i32;
                                let y_px = ((y_dip + h) / to_dip).round() as i32;
                                let cf = CANDIDATEFORM {
                                    dwStyle: CFS_FORCE_POSITION,
                                    ptCurrentPos: POINT { x: x_px, y: y_px },
                                    rcArea: RECT::default(),
                                    dwIndex: 0,
                                };
                                let _ = ImmSetCandidateWindow(himc, &cf);
                            }

                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }

                        let _ = ImmReleaseContext(hwnd, himc);
                    }
                }
                LRESULT(0)
            }
            WM_IME_ENDCOMPOSITION => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    state.text_widget.ime_end();
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    // Extract mouse position in client pixels
                    let x_px = (lparam.0 & 0xFFFF) as i16 as i32 as f32;
                    let y_px = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32 as f32;
                    let to_dip = dips_scale(hwnd);
                    let x = x_px * to_dip;
                    let y = y_px * to_dip;
                    if let Ok(idx) = state.text_widget.hit_test_index(x, y) {
                        if state.text_widget.is_composing() {
                            let himc = ImmGetContext(hwnd);
                            if !himc.is_invalid() {
                                // Notify IME to complete composition so that we can move the cursor
                                // to the clicked position.
                                let _ = ImmNotifyIME(himc, NI_COMPOSITIONSTR, CPS_COMPLETE, 0);
                            }
                        }

                        state.text_widget.begin_drag(idx);
                        // Ensure we receive keyboard input
                        let _ = SetFocus(Some(hwnd));
                        let _ = SetCapture(hwnd);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    if state.text_widget.is_dragging() {
                        let x_px = (lparam.0 & 0xFFFF) as i16 as i32 as f32;
                        let y_px = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32 as f32;
                        let to_dip = dips_scale(hwnd);
                        let x = x_px * to_dip;
                        let y = y_px * to_dip;
                        if let Ok(idx) = state.text_widget.hit_test_index(x, y) {
                            if state.text_widget.update_drag_index(idx) {
                                let _ = InvalidateRect(Some(hwnd), None, false);
                            }
                        }
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    let x_px = (lparam.0 & 0xFFFF) as i16 as i32 as f32;
                    let y_px = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32 as f32;
                    let to_dip = dips_scale(hwnd);
                    let x = x_px * to_dip;
                    let y = y_px * to_dip;
                    if let Ok(idx) = state.text_widget.hit_test_index(x, y) {
                        state.text_widget.end_drag(idx);
                    } else {
                        // Even on failure, ensure drag ends
                        state.text_widget.end_drag(0);
                    }
                    let _ = ReleaseCapture();
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_COPY => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    if let Some(s) = state.text_widget.selected_text() {
                        let _ = set_clipboard_text(hwnd, &s);
                    }
                }
                LRESULT(0)
            }
            WM_CUT => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    if let Some(s) = state.text_widget.selected_text() {
                        let _ = set_clipboard_text(hwnd, &s);
                        let _ = state.text_widget.insert_str("");
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_PASTE => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    if !state.text_widget.is_composing() {
                        if let Some(s) = get_clipboard_text(hwnd) {
                            let _ = state.text_widget.insert_str(&s);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                    }
                }
                LRESULT(0)
            }
            WM_CHAR => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    // Suppress WM_CHAR while IME composition is active to avoid duplicate input
                    if state.text_widget.is_composing() {
                        return LRESULT(0);
                    }
                    let mut code = (wparam.0 & 0xFFFF) as u32;
                    // Handle CR -> LF
                    if code == 0x0D {
                        code = 0x0A;
                    }
                    // Skip most control chars except TAB and LF
                    if (code < 0x20 && code != 0x09 && code != 0x0A) || code == 0x7F {
                        return LRESULT(0);
                    }

                    // Surrogate handling
                    let is_high = (0xD800..=0xDBFF).contains(&(code as u16));
                    let is_low = (0xDC00..=0xDFFF).contains(&(code as u16));
                    let mut to_insert = String::new();
                    if is_high {
                        state.pending_high_surrogate = Some(code as u16);
                    } else if is_low {
                        if let Some(high) = state.pending_high_surrogate.take() {
                            let u = 0x10000
                                + (((high as u32 - 0xD800) << 10)
                                    | ((code as u32 - 0xDC00) & 0x3FF));
                            if let Some(ch) = char::from_u32(u) {
                                to_insert.push(ch);
                            }
                        }
                    } else {
                        state.pending_high_surrogate = None;
                        if let Some(ch) = char::from_u32(code) {
                            to_insert.push(ch);
                        }
                    }
                    if !to_insert.is_empty() {
                        let _ = state.text_widget.insert_str(&to_insert);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_KEYDOWN => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    let vk = wparam.0 as u32;
                    let shift_down = GetKeyState(VK_SHIFT.0 as i32) < 0;
                    let ctrl_down = GetKeyState(VK_CONTROL.0 as i32) < 0;
                    let handled = match vk {
                        x if x == VK_LEFT.0 as u32 => {
                            if ctrl_down {
                                state.text_widget.move_word_left(shift_down);
                            } else {
                                state.text_widget.move_left(shift_down);
                            }
                            true
                        }
                        x if x == VK_RIGHT.0 as u32 => {
                            if ctrl_down {
                                state.text_widget.move_word_right(shift_down);
                            } else {
                                state.text_widget.move_right(shift_down);
                            }
                            true
                        }
                        x if x == VK_HOME.0 as u32 => {
                            state.text_widget.move_to_start(shift_down);
                            true
                        }
                        x if x == VK_END.0 as u32 => {
                            state.text_widget.move_to_end(shift_down);
                            true
                        }
                        x if x == VK_BACK.0 as u32 => {
                            let _ = state.text_widget.backspace();
                            true
                        }
                        x if x == VK_DELETE.0 as u32 => {
                            let _ = state.text_widget.delete_forward();
                            true
                        }
                        x if x == VK_A.0 as u32 && ctrl_down => {
                            state.text_widget.select_all();
                            true
                        }
                        x if x == VK_C.0 as u32 && ctrl_down => {
                            if let Some(s) = state.text_widget.selected_text() {
                                let _ = set_clipboard_text(hwnd, &s);
                            }
                            true
                        }
                        x if x == VK_X.0 as u32 && ctrl_down => {
                            if let Some(s) = state.text_widget.selected_text() {
                                let _ = set_clipboard_text(hwnd, &s);
                                let _ = state.text_widget.insert_str("");
                            }
                            true
                        }
                        x if x == VK_V.0 as u32 && ctrl_down => {
                            if !state.text_widget.is_composing() {
                                if let Some(s) = get_clipboard_text(hwnd) {
                                    let _ = state.text_widget.insert_str(&s);
                                }
                            }
                            true
                        }
                        _ => false,
                    };
                    if handled {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_CREATE => {
                let pcs = lparam.0 as *const CREATESTRUCTW;
                let ptr = (*pcs).lpCreateParams;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);

                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    // Get the composition refresh rate. If the DWM isn't running,
                    // get the refresh rate from GDI -- probably going to be 60Hz
                    let timing_info = &mut state.timing_info;
                    if DwmGetCompositionTimingInfo(hwnd, timing_info).is_err() {
                        println!("Failed to get composition timing info");
                        let hdc = GetDC(Some(hwnd));
                        timing_info.rateCompose.uiDenominator = 1;
                        timing_info.rateCompose.uiNumerator =
                            GetDeviceCaps(Some(hdc), VREFRESH) as u32;
                        ReleaseDC(Some(hwnd), hdc);
                    }

                    println!(
                        "Refresh rate: num {} / den {}",
                        timing_info.rateCompose.uiNumerator as f64,
                        timing_info.rateCompose.uiDenominator as f64
                    );
                }

                LRESULT(0)
            }
            WM_SIZE => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    let width = (lparam.0 & 0xFFFF) as u32;
                    let height = ((lparam.0 >> 16) & 0xFFFF) as u32;
                    if let Err(e) = state.on_resize(width, height) {
                        eprintln!("Failed to resize: {e}");
                    }
                }
                LRESULT(0)
            }
            WAM::WM_DPICHANGED => {
                // Resize window to the suggested rect and update render target DPI
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    let suggested = &*(lparam.0 as *const RECT);
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        suggested.left,
                        suggested.top,
                        suggested.right - suggested.left,
                        suggested.bottom - suggested.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                    state.update_dpi(hwnd);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_SETCURSOR => {
                // Set I-beam cursor when hovering over visible text bounds (in client area)
                let hit_test = (lparam.0 & 0xFFFF) as u32;
                if hit_test == HTCLIENT {
                    if let Some(state) = state_mut_from_hwnd(hwnd) {
                        // Get mouse in client pixels and convert to DIPs
                        let mut pt = POINT { x: 0, y: 0 };
                        let _ = GetCursorPos(&mut pt);
                        let _ = ScreenToClient(hwnd, &mut pt);
                        let to_dip = dips_scale(hwnd);
                        let x_dip = (pt.x as f32) * to_dip;
                        let y_dip = (pt.y as f32) * to_dip;
                        let RectDIP {
                            x_dip: left,
                            y_dip: top,
                            width_dip: width,
                            height_dip: height,
                        } = state.text_widget.metric_bounds();
                        if x_dip >= left
                            && y_dip >= top
                            && x_dip < left + width
                            && y_dip < top + height
                        {
                            if let Some(h) = IBEAM_CURSOR
                                .get_or_init(|| LoadCursorW(None, IDC_IBEAM).ok().map(SafeCursor))
                                .as_ref()
                            {
                                let _ = SetCursor(Some(h.0));
                                return LRESULT(1);
                            }
                        }
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_PAINT => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    if let Err(e) = state.on_paint(hwnd) {
                        eprintln!("Failed to paint: {e}");
                    }
                }
                LRESULT(0)
            }
            WM_DISPLAYCHANGE => {
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_DESTROY => {
                let ptr = WAM::GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if ptr != 0 {
                    let _ = Box::from_raw(ptr as *mut AppState); // drop
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                WAM::PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn main() -> Result<()> {
    unsafe {
        // Opt-in to Per-Monitor V2 DPI awareness for crisp rendering on high-DPI displays
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hinstance = GetModuleHandleW(None)?;
        let class_name = PCWSTR(w!("DWriteSampleWindow").as_ptr());

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassW(&wc);

        let app = AppState::new()?;
        let mut dpi_x = 0.0f32;
        let mut dpi_y = 0.0f32;
        app.d2d_factory.GetDesktopDpi(&mut dpi_x, &mut dpi_y);

        let boxed = Box::new(app);
        let ptr = Box::into_raw(boxed) as isize;

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            PCWSTR(w!("DirectWrite Getting Started (Rust)").as_ptr()),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            (800.0 / dips_scale_for_dpi(dpi_x)) as i32,
            (600.0 / dips_scale_for_dpi(dpi_y)) as i32,
            None,
            None,
            Some(hinstance.into()),
            Some(ptr as *const c_void),
        )?;

        // We don't care if the window was previously hidden or not
        let _ = ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd).ok()?;

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            // We don't care if the message was translated or not
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
