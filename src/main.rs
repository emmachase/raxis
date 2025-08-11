// #![windows_subsystem = "windows"]

use glux::widgets::spinner::Spinner;
use std::sync::OnceLock;
use windows::{
    Win32::{
        Foundation::{D2DERR_RECREATE_TARGET, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{
            Direct2D::{
                Common::{
                    D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_UNKNOWN, D2D1_COLOR_F,
                    D2D1_PIXEL_FORMAT,
                },
                D2D1_DEBUG_LEVEL_NONE, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                D2D1_FACTORY_OPTIONS, D2D1_FACTORY_TYPE_SINGLE_THREADED,
                D2D1_FEATURE_LEVEL_DEFAULT, D2D1_HWND_RENDER_TARGET_PROPERTIES,
                D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES,
                D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1CreateFactory,
                ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
            },
            DirectWrite::{
                DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_WEIGHT_REGULAR, DWRITE_HIT_TEST_METRICS, DWRITE_MEASURING_MODE_NATURAL,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
                DWRITE_TEXT_METRICS, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
                IDWriteTextLayout,
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
            HiDpi::{
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
                SetProcessDpiAwarenessContext,
            },
            Input::KeyboardAndMouse::{ReleaseCapture, SetCapture},
            WindowsAndMessaging::{
                self as WAM, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW,
                DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetClientRect, GetCursorPos,
                GetMessageW, HCURSOR, HTCLIENT, IDC_ARROW, IDC_IBEAM, LoadCursorW, MSG,
                RegisterClassW, STRSAFE_E_INSUFFICIENT_BUFFER, SW_SHOW, SWP_NOACTIVATE,
                SWP_NOZORDER, SetCursor, SetWindowLongPtrW, SetWindowPos, ShowWindow,
                TranslateMessage, WINDOW_EX_STYLE, WM_CREATE, WM_DESTROY, WM_DISPLAYCHANGE,
                WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SETCURSOR, WM_SIZE,
                WNDCLASSW, WS_OVERLAPPEDWINDOW,
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

fn current_dpi(hwnd: HWND) -> f32 {
    unsafe { GetDpiForWindow(hwnd) as f32 }
}

fn dips_scale(hwnd: HWND) -> f32 {
    96.0f32 / current_dpi(hwnd).max(1.0)
}

fn apply_dpi_to_rt(rt: &ID2D1HwndRenderTarget, hwnd: HWND) {
    let dpi = current_dpi(hwnd);
    unsafe { rt.SetDpi(dpi, dpi) };
}

struct AppState {
    d2d_factory: ID2D1Factory,
    dwrite_factory: IDWriteFactory,
    text_format: IDWriteTextFormat,
    render_target: Option<ID2D1HwndRenderTarget>,
    black_brush: Option<ID2D1SolidColorBrush>,

    clock: f64,
    timing_info: DWM_TIMING_INFO,
    spinner: Spinner,

    // Text selection state (UTF-16 code unit indices)
    selection_anchor: u32,
    selection_active: u32,
    is_dragging: bool,

    // Cached text layout bounds in DIPs for cursor hit-testing
    text_left_dip: f32,
    text_top_dip: f32,
    text_width_dip: f32,
    text_height_dip: f32,
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

            Ok(Self {
                d2d_factory,
                dwrite_factory,
                text_format,
                render_target: None,
                black_brush: None,
                clock: 0.0,
                timing_info: DWM_TIMING_INFO::default(),
                spinner,
                selection_anchor: 0,
                selection_active: 0,
                is_dragging: false,
                text_left_dip: 0.0,
                text_top_dip: 0.0,
                text_width_dip: 0.0,
                text_height_dip: 0.0,
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

    fn build_text_layout(&self, hwnd: HWND) -> Result<IDWriteTextLayout> {
        unsafe {
            let rc = client_rect(hwnd)?;
            let to_dip = dips_scale(hwnd);
            let max_width = (rc.right - rc.left) as f32 * to_dip;
            let max_height = (rc.bottom - rc.top) as f32 * to_dip;

            let wtext: Vec<u16> = TEXT.encode_utf16().collect();
            let layout = self.dwrite_factory.CreateTextLayout(
                &wtext,
                &self.text_format,
                max_width,
                max_height,
            )?;
            Ok(layout)
        }
    }

    fn hit_test_index(&self, hwnd: HWND, x_dip: f32, y_dip: f32) -> Result<u32> {
        unsafe {
            let layout = self.build_text_layout(hwnd)?;
            let mut trailing = windows::core::BOOL(0);
            let mut inside = windows::core::BOOL(0);
            let mut metrics = DWRITE_HIT_TEST_METRICS::default();
            layout.HitTestPoint(x_dip, y_dip, &mut trailing, &mut inside, &mut metrics)?;

            let mut idx = if trailing.as_bool() {
                metrics.textPosition.saturating_add(metrics.length)
            } else {
                metrics.textPosition
            };
            let total_len = TEXT.encode_utf16().count() as u32;
            if idx > total_len {
                idx = total_len;
            }
            Ok(idx)
        }
    }

    fn on_paint(&mut self, hwnd: HWND) -> Result<()> {
        unsafe {
            self.create_device_resources(hwnd)?;
            // Refresh target DPI in case it changed (e.g. monitor move)
            self.update_dpi(hwnd);
            let mut ps = PAINTSTRUCT::default();
            BeginPaint(hwnd, &mut ps);

            if let (d2d_factory, _dwrite_factory, Some(rt), Some(brush)) = (
                &self.d2d_factory,
                &self.dwrite_factory,
                &self.render_target,
                &self.black_brush,
            ) {
                rt.BeginDraw();
                let white = D2D1_COLOR_F {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                };
                rt.Clear(Some(&white));

                let rc = client_rect(hwnd)?;
                // Convert client pixel size to DIPs for D2D drawing
                let to_dip = dips_scale(hwnd);

                brush.SetColor(&D2D1_COLOR_F {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                });

                // Build text layout sized to the window
                let text_layout = self.build_text_layout(hwnd)?;

                // Cache text layout metrics for cursor hit-testing
                let mut textmetrics = DWRITE_TEXT_METRICS::default();
                text_layout.GetMetrics(&mut textmetrics)?;
                self.text_left_dip = textmetrics.left;
                self.text_top_dip = textmetrics.top;
                self.text_width_dip = textmetrics.width;
                self.text_height_dip = textmetrics.height;

                // Draw selection highlight behind text if any
                let sel_start = self.selection_anchor.min(self.selection_active);
                let sel_end = self.selection_anchor.max(self.selection_active);
                let sel_len = sel_end.saturating_sub(sel_start);
                if sel_len > 0 {
                    let mut needed: u32 = 0;
                    // First call: expect ERROR_INSUFFICIENT_BUFFER (0x8007007A) with needed count.
                    match text_layout.HitTestTextRange(
                        sel_start,
                        sel_len,
                        0.0,
                        0.0,
                        None,
                        &mut needed,
                    ) {
                        Ok(()) => {
                            // No metrics to draw (nothing selected on screen)
                        }
                        Err(e) if e.code() == STRSAFE_E_INSUFFICIENT_BUFFER => {
                            // Allocate and retry; loop in case depth grows.
                            let capacity = needed.max(1);
                            loop {
                                let mut runs =
                                    vec![DWRITE_HIT_TEST_METRICS::default(); capacity as usize];
                                let mut actual: u32 = 0;
                                match text_layout.HitTestTextRange(
                                    sel_start,
                                    sel_len,
                                    0.0,
                                    0.0,
                                    Some(&mut runs),
                                    &mut actual,
                                ) {
                                    Ok(()) => {
                                        // Selection color (light blue)
                                        brush.SetColor(&D2D1_COLOR_F {
                                            r: 0.2,
                                            g: 0.4,
                                            b: 1.0,
                                            a: 0.35,
                                        });
                                        for m in runs.iter().take(actual as usize) {
                                            let rect = D2D_RECT_F {
                                                left: m.left,
                                                top: m.top,
                                                right: m.left + m.width,
                                                bottom: m.top + m.height,
                                            };
                                            rt.FillRectangle(&rect, brush);
                                        }
                                        // Restore brush to black for drawing text
                                        brush.SetColor(&D2D1_COLOR_F {
                                            r: 0.0,
                                            g: 0.0,
                                            b: 0.0,
                                            a: 1.0,
                                        });
                                        break;
                                    }
                                    Err(e) => return Err(e),
                                }
                            }
                        }
                        Err(e) => return Err(e),
                    }
                }

                rt.DrawTextLayout(
                    Vector2 { X: 0.0, Y: 0.0 },
                    &text_layout,
                    brush,
                    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                );

                // Spinner: arc grows to 90%, shrinks to 10%, start angle rotates slowly
                let width_dip = (rc.right - rc.left) as f32 * to_dip;
                let height_dip = (rc.bottom - rc.top) as f32 * to_dip;
                let center = Vector2 {
                    X: width_dip * 0.5,
                    Y: height_dip * 0.5,
                };
                let radius = width_dip.min(height_dip) * 0.3;
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
                    if let Ok(idx) = state.hit_test_index(hwnd, x, y) {
                        state.selection_anchor = idx;
                        state.selection_active = idx;
                        state.is_dragging = true;
                        let _ = SetCapture(hwnd);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    if state.is_dragging {
                        let x_px = (lparam.0 & 0xFFFF) as i16 as i32 as f32;
                        let y_px = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32 as f32;
                        let to_dip = dips_scale(hwnd);
                        let x = x_px * to_dip;
                        let y = y_px * to_dip;
                        if let Ok(idx) = state.hit_test_index(hwnd, x, y) {
                            if idx != state.selection_active {
                                state.selection_active = idx;
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
                    if let Ok(idx) = state.hit_test_index(hwnd, x, y) {
                        state.selection_active = idx;
                    }
                    state.is_dragging = false;
                    let _ = ReleaseCapture();
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_CREATE => match AppState::new() {
                Ok(mut state) => {
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

                    let boxed = Box::new(state);
                    let ptr = Box::into_raw(boxed) as isize;
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr);
                    LRESULT(0)
                }
                Err(_) => LRESULT(-1),
            },
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

                        if x_dip >= state.text_left_dip
                            && y_dip >= state.text_top_dip
                            && x_dip < state.text_left_dip + state.text_width_dip
                            && y_dip < state.text_top_dip + state.text_height_dip
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

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            PCWSTR(w!("DirectWrite Getting Started (Rust)").as_ptr()),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            800,
            600,
            None,
            None,
            Some(hinstance.into()),
            None,
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
