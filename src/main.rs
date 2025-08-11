use glux::widgets::spinner::Spinner;
use windows::{
    Win32::{
        Foundation::{D2DERR_RECREATE_TARGET, HWND, LPARAM, LRESULT, RECT, WPARAM},
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
                DWRITE_FONT_WEIGHT_REGULAR, DWRITE_MEASURING_MODE_NATURAL,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
                DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
            },
            Dwm::{DWM_TIMING_INFO, DwmGetCompositionTimingInfo},
            Dxgi::Common::DXGI_FORMAT_UNKNOWN,
            Gdi::{
                BeginPaint, EndPaint, GetDC, GetDeviceCaps, InvalidateRect, PAINTSTRUCT, ReleaseDC,
                UpdateWindow, VREFRESH,
            },
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
                SetProcessDpiAwarenessContext,
            },
            WindowsAndMessaging::{
                self as WAM, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW,
                DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetClientRect, GetMessageW,
                IDC_ARROW, LoadCursorW, MSG, RegisterClassW, SW_SHOW, SWP_NOACTIVATE, SWP_NOZORDER,
                SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, WINDOW_EX_STYLE,
                WM_CREATE, WM_DESTROY, WM_DISPLAYCHANGE, WM_PAINT, WM_SIZE, WNDCLASSW,
                WS_OVERLAPPEDWINDOW,
            },
        },
    },
    core::{PCWSTR, Result, w},
};
use windows_numerics::Vector2;

const TEXT: &str = "Hello, DirectWrite! こんにちは 😍";

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
    text_format: IDWriteTextFormat,
    render_target: Option<ID2D1HwndRenderTarget>,
    black_brush: Option<ID2D1SolidColorBrush>,

    clock: f64,
    timing_info: DWM_TIMING_INFO,
    spinner: Spinner,
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
                text_format,
                render_target: None,
                black_brush: None,
                clock: 0.0,
                timing_info: DWM_TIMING_INFO::default(),
                spinner,
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

            if let (factory, Some(rt), Some(brush)) =
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
                // Convert client pixel size to DIPs for D2D drawing
                let to_dip = dips_scale(hwnd);
                let layout = D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: (rc.right - rc.left) as f32 * to_dip,
                    bottom: (rc.bottom - rc.top) as f32 * to_dip,
                };

                brush.SetColor(&D2D1_COLOR_F {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                });

                // Convert TEXT (&str) to UTF-16; DrawText takes explicit length from slice
                let wtext: Vec<u16> = TEXT.encode_utf16().collect();
                rt.DrawText(
                    &wtext,
                    &self.text_format,
                    &layout,
                    brush,
                    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                    DWRITE_MEASURING_MODE_NATURAL,
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
                self.spinner.draw(factory, rt, brush)?;

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
