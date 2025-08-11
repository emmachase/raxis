// #![windows_subsystem = "windows"]

use glux::{
    current_dpi, dips_scale, dips_scale_for_dpi,
    gfx::RectDIP,
    widgets::{selectable_text::SelectableText, spinner::Spinner},
};
use std::{ffi::c_void, sync::OnceLock};
use windows::{
    Win32::{
        Foundation::{D2DERR_RECREATE_TARGET, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
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
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            Input::KeyboardAndMouse::{ReleaseCapture, SetCapture},
            WindowsAndMessaging::{
                self as WAM, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW,
                DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetClientRect, GetCursorPos,
                GetMessageW, HCURSOR, HTCLIENT, IDC_ARROW, IDC_IBEAM, LoadCursorW, MSG,
                RegisterClassW, SW_SHOW, SWP_NOACTIVATE, SWP_NOZORDER, SetCursor,
                SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, WINDOW_EX_STYLE,
                WM_CREATE, WM_DESTROY, WM_DISPLAYCHANGE, WM_LBUTTONDOWN, WM_LBUTTONUP,
                WM_MOUSEMOVE, WM_PAINT, WM_SETCURSOR, WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
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

struct AppState {
    d2d_factory: ID2D1Factory,
    dwrite_factory: IDWriteFactory,
    _text_format: IDWriteTextFormat,
    render_target: Option<ID2D1HwndRenderTarget>,
    black_brush: Option<ID2D1SolidColorBrush>,

    clock: f64,
    timing_info: DWM_TIMING_INFO,
    spinner: Spinner,

    // Selectable text widget encapsulating layout, selection, and bounds
    text_widget: SelectableText,
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
                dwrite_factory,
                _text_format: text_format,
                render_target: None,
                black_brush: None,
                clock: 0.0,
                timing_info: DWM_TIMING_INFO::default(),
                spinner,
                text_widget,
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
                let _ = self.text_widget.draw(rt, brush);

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

        self.clock += self.timing_info.rateCompose.uiDenominator as f64
            / self.timing_info.rateCompose.uiNumerator as f64;

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
            WM_LBUTTONDOWN => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    // Extract mouse position in client pixels
                    let x_px = (lparam.0 & 0xFFFF) as i16 as i32 as f32;
                    let y_px = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32 as f32;
                    let to_dip = dips_scale(hwnd);
                    let x = x_px * to_dip;
                    let y = y_px * to_dip;
                    if let Ok(idx) = state.text_widget.hit_test_index(x, y) {
                        state.text_widget.begin_drag(idx);
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
