// #![windows_subsystem = "windows"]

use raxis::gfx::PointDIP;
use raxis::layout::model::{
    Axis, ElementContent, HorizontalAlignment, ScrollConfig, Sizing, UIElement, VerticalAlignment,
};
use raxis::layout::scroll_manager::{ScrollPosition, ScrollStateManager};
use raxis::layout::visitors::VisitAction;
use raxis::layout::{
    self, OwnedUITree, ScrollDirection, can_scroll_further, compute_scrollbar_geom, visitors,
};
use raxis::widgets::integrated_drop_target::IntegratedDropTarget;
use raxis::widgets::{Cursor, DragEvent, Event, Modifiers, Renderer};
use raxis::{RedrawRequest, Shell, w_id};
use raxis::{
    current_dpi, dips_scale, dips_scale_for_dpi,
    gfx::RectDIP,
    widgets::{spinner::Spinner, text_input::TextInput},
};
use slotmap::DefaultKey;
use std::ffi::c_void;
use std::time::Instant;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_LEADING,
};
use windows::Win32::System::Com::CoUninitialize;
use windows::Win32::System::SystemServices::MK_SHIFT;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_MENU;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, SPI_GETWHEELSCROLLLINES, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SystemParametersInfoW, WM_DPICHANGED, WM_KEYUP, WM_MOUSEWHEEL, WM_TIMER,
};
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
                DWRITE_FONT_WEIGHT_REGULAR, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
            },
            Dwm::{DWM_TIMING_INFO, DwmGetCompositionTimingInfo},
            Dxgi::Common::DXGI_FORMAT_UNKNOWN,
            Gdi::{
                BeginPaint, EndPaint, GetDC, GetDeviceCaps, InvalidateRect, PAINTSTRUCT, ReleaseDC,
                ScreenToClient, UpdateWindow, VREFRESH,
            },
        },
        System::{
            Com::CoInitialize,
            LibraryLoader::GetModuleHandleW,
            Ole::{IDropTarget, OleInitialize, OleUninitialize, RegisterDragDrop, RevokeDragDrop},
        },
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            Input::{
                Ime::{
                    GCS_COMPSTR, GCS_CURSORPOS, GCS_RESULTSTR, ImmGetCompositionStringW,
                    ImmGetContext, ImmReleaseContext,
                },
                KeyboardAndMouse::{
                    GetDoubleClickTime, GetKeyState, ReleaseCapture, SetCapture, SetFocus,
                    VK_CONTROL, VK_SHIFT,
                },
            },
            WindowsAndMessaging::{
                self as WAM, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW,
                DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetClientRect, GetCursorPos,
                GetMessageTime, GetMessageW, GetSystemMetrics, HCURSOR, HTCLIENT, IDC_ARROW,
                IDC_IBEAM, LoadCursorW, MSG, RegisterClassW, SM_CXDOUBLECLK, SM_CYDOUBLECLK,
                SW_SHOW, SWP_NOACTIVATE, SWP_NOZORDER, SetCursor, SetWindowLongPtrW, SetWindowPos,
                ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WM_CHAR, WM_CREATE, WM_DESTROY,
                WM_DISPLAYCHANGE, WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION,
                WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
                WM_PAINT, WM_SETCURSOR, WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
            },
        },
    },
    core::{PCWSTR, Result, w},
};

pub const LINE_HEIGHT: u32 = 16;

pub struct SafeCursor(pub HCURSOR);
unsafe impl Send for SafeCursor {}
unsafe impl Sync for SafeCursor {}

// const TEXT: &str = "Hello, בְּרֵאשִׁ֖ית בָּרָ֣א אֱלֹהִ֑ים אֵ֥ת הַשָּׁמַ֖יִם וְאֵ֥ת הָאָֽרֶץ.  DirectWrite! こんにちは 😍";
const TEXT: &str = "Hello, World!";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug)]
struct ScrollDragState {
    element_id: u64,
    axis: DragAxis,
    // Offset within the thumb (along the drag axis) where the pointer grabbed, in DIPs
    grab_offset: f32,
}

// ===== OLE Drop Target integration =====
// The old DropTarget implementation has been replaced with IntegratedDropTarget
// which properly integrates with the widget system

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

fn window_rect(hwnd: HWND) -> Result<RECT> {
    unsafe {
        let mut rc = RECT::default();
        GetWindowRect(hwnd, &mut rc)?;
        Ok(rc)
    }
}

fn apply_dpi_to_rt(rt: &ID2D1HwndRenderTarget, hwnd: HWND) {
    let dpi = current_dpi(hwnd);
    unsafe { rt.SetDpi(dpi, dpi) };
}

struct AppState {
    d2d_factory: ID2D1Factory,
    _dwrite_factory: IDWriteFactory,
    _text_format: IDWriteTextFormat,
    render_target: Option<ID2D1HwndRenderTarget>,
    black_brush: Option<ID2D1SolidColorBrush>,

    clock: f64,
    timing_info: DWM_TIMING_INFO,
    // spinner: Spinner,
    ui_tree: OwnedUITree,

    shell: Shell,

    scroll_state_manager: ScrollStateManager,

    // Selectable text widget encapsulating layout, selection, and bounds
    // text_widget: SelectableText,
    // text_widget_ui_key: DefaultKey,

    // Keep the window's OLE drop target alive for the lifetime of the window
    drop_target: Option<IDropTarget>,

    // For combining UTF-16 surrogate pairs from WM_CHAR
    pending_high_surrogate: Option<u16>,

    // Mouse multi-click tracking (running click count within time/rect)
    last_click_time: u32,
    last_click_pos: POINT,
    click_count: u32,

    // Active scrollbar dragging state (if any)
    scroll_drag: Option<ScrollDragState>,
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
                // 72.0,
                12.0,
                PCWSTR(w!("en-us").as_ptr()),
            )?;
            text_format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
            text_format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;

            let spinner = Spinner::new(6.0, 200.0, 1.6, 64.0);

            // Build selectable text widget using shared DWrite factory/format
            // let text_widget = ;

            let mut ui_tree = OwnedUITree::new();

            let root = ui_tree.insert(UIElement {
                id: Some(w_id!()),

                background_color: Some(0xFF000044),

                width: Sizing::Fixed { px: 800.0 },
                height: Sizing::Fixed { px: 200.0 },

                child_gap: 10.0,

                ..Default::default()
            });

            let child = ui_tree.insert(UIElement {
                id: Some(w_id!()),

                background_color: Some(0x00FF00FF),

                width: Sizing::Grow {
                    min: 32.0,
                    max: f32::INFINITY,
                },
                height: Sizing::grow(),

                ..Default::default()
            });
            ui_tree[root].children.push(child);

            let child = ui_tree.insert(UIElement {
                id: Some(w_id!()),

                direction: layout::model::Direction::TopToBottom,
                child_gap: 16.0,

                background_color: Some(0xFFFF00FF),

                scroll: Some(ScrollConfig {
                    horizontal: Some(true),
                    vertical: Some(true),

                    sticky_right: Some(true),
                    sticky_bottom: Some(true),

                    ..Default::default()
                }),

                horizontal_alignment: HorizontalAlignment::Center,
                vertical_alignment: VerticalAlignment::Center,

                width: Sizing::grow(),
                height: Sizing::grow(),

                ..Default::default()
            });
            ui_tree[root].children.push(child);

            let text_widget_ui_key = ui_tree.insert(UIElement {
                id: Some(w_id!()),
                background_color: Some(0x00FFFFFF),

                vertical_alignment: VerticalAlignment::Center,

                width: Sizing::grow(),

                // content: Some(ElementContent::Text {
                //     layout: dwrite_factory
                //         .CreateTextLayout(
                //             &w!("Hello, World!").as_wide(),
                //             Some(&text_format),
                //             f32::INFINITY,
                //             f32::INFINITY,
                //         )
                //         .ok(),
                // }),
                content: Some(ElementContent::Widget(Box::new(TextInput::new(
                    dwrite_factory.clone(),
                    text_format.clone(),
                    TEXT.to_string(),
                )))),

                color: Some(0x6030F0FF),

                ..Default::default()
            });
            ui_tree[child].children.push(text_widget_ui_key);

            let text_widget_ui_key = ui_tree.insert(UIElement {
                id: Some(w_id!()),
                background_color: Some(0x00FFFFFF),

                vertical_alignment: VerticalAlignment::Center,

                // content: Some(ElementContent::Text {
                //     layout: dwrite_factory
                //         .CreateTextLayout(
                //             &w!("Hello, World!").as_wide(),
                //             Some(&text_format),
                //             f32::INFINITY,
                //             f32::INFINITY,
                //         )
                //         .ok(),
                // }),
                content: Some(ElementContent::Widget(Box::new(TextInput::new(
                    dwrite_factory.clone(),
                    text_format.clone(),
                    TEXT.to_string(),
                )))),

                color: Some(0x6030F0FF),

                ..Default::default()
            });
            ui_tree[child].children.push(text_widget_ui_key);

            let spinner_ui_key = ui_tree.insert(UIElement {
                id: Some(w_id!()),

                vertical_alignment: VerticalAlignment::Center,

                // content: Some(ElementContent::Text {
                //     layout: dwrite_factory
                //         .CreateTextLayout(
                //             &w!("Hello, World!").as_wide(),
                //             Some(&text_format),
                //             f32::INFINITY,
                //             f32::INFINITY,
                //         )
                //         .ok(),
                // }),
                content: Some(ElementContent::Widget(Box::new(spinner))),

                color: Some(0x6030F0FF),

                ..Default::default()
            });
            ui_tree[child].children.push(spinner_ui_key);

            let child = ui_tree.insert(UIElement {
                id: Some(w_id!()),

                background_color: Some(0x00FFFFFF),

                width: Sizing::Grow {
                    min: 32.0,
                    max: f32::INFINITY,
                },
                height: Sizing::grow(),

                ..Default::default()
            });
            ui_tree[root].children.push(child);

            let shell = Shell::new();

            Ok(Self {
                d2d_factory,
                _dwrite_factory: dwrite_factory,
                _text_format: text_format,
                render_target: None,
                black_brush: None,
                clock: 0.0,
                timing_info: DWM_TIMING_INFO::default(),
                ui_tree,
                shell,
                scroll_state_manager: ScrollStateManager::default(),
                drop_target: None,
                pending_high_surrogate: None,
                last_click_time: 0,
                last_click_pos: POINT { x: 0, y: 0 },
                click_count: 0,
                scroll_drag: None,
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

            if let (Some(rt), Some(brush)) = (&self.render_target, &self.black_brush) {
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

                brush.SetColor(&D2D1_COLOR_F {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                });

                let root = self.ui_tree.keys().next().unwrap();
                self.ui_tree[root].width = Sizing::fixed(rc_dip.width_dip);
                self.ui_tree[root].height = Sizing::fixed(rc_dip.height_dip);

                layout::layout(&mut self.ui_tree, root, &mut self.scroll_state_manager);
                layout::paint(
                    &self.shell,
                    &Renderer {
                        factory: &self.d2d_factory,
                        render_target: rt,
                        brush,
                    },
                    &mut self.ui_tree,
                    root,
                    &mut self.scroll_state_manager,
                    0.0,
                    0.0,
                );

                // Spinner drawn above uses the current brush color.

                let end = rt.EndDraw(None, None);
                if let Err(e) = end {
                    if e.code() == D2DERR_RECREATE_TARGET {
                        self.discard_device_resources();
                    }
                }
            }

            let _ = EndPaint(hwnd, &ps);
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

    // Depth-first traversal: visit children first (post-order for scrollbar z-order),
    // then compute scrollbar thumb rects for hit-testing and return the last (topmost) hit.
    fn hit_test_scrollbar_thumb(&self, x: f32, y: f32) -> Option<ScrollDragState> {
        fn dfs(
            state: &AppState,
            key: DefaultKey,
            x: f32,
            y: f32,
            out: &mut Option<ScrollDragState>,
        ) {
            let element = &state.ui_tree[key];
            // Recurse into children first
            let children: Vec<DefaultKey> = element.children.clone();
            for child in children {
                dfs(state, child, x, y, out);
            }

            // Then evaluate current element so it overrides children (matches z-order in paint)
            if element.id.is_none() {
                return;
            }

            // Use centralized geometry helpers for hit-testing
            if let Some(geom) =
                compute_scrollbar_geom(element, Axis::Y, &state.scroll_state_manager)
            {
                let tr = geom.thumb_rect;
                if x >= tr.x_dip
                    && x < tr.x_dip + tr.width_dip
                    && y >= tr.y_dip
                    && y < tr.y_dip + tr.height_dip
                {
                    let grab_offset = y - tr.y_dip;
                    *out = Some(ScrollDragState {
                        element_id: element.id.unwrap(),
                        axis: DragAxis::Vertical,
                        grab_offset,
                    });
                }
            }

            if let Some(geom) =
                compute_scrollbar_geom(element, Axis::X, &state.scroll_state_manager)
            {
                let tr = geom.thumb_rect;
                if x >= tr.x_dip
                    && x < tr.x_dip + tr.width_dip
                    && y >= tr.y_dip
                    && y < tr.y_dip + tr.height_dip
                {
                    let grab_offset = x - tr.x_dip;
                    *out = Some(ScrollDragState {
                        element_id: element.id.unwrap(),
                        axis: DragAxis::Horizontal,
                        grab_offset,
                    });
                }
            }
        }

        let root = self.ui_tree.keys().next()?;
        let mut result = None;
        dfs(self, root, x, y, &mut result);
        result
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let result = unsafe {
        match msg {
            WM_IME_STARTCOMPOSITION => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    state.shell.dispatch_event(
                        hwnd,
                        &mut state.ui_tree,
                        Event::ImeStartComposition,
                    );
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

                                state.shell.dispatch_event(
                                    hwnd,
                                    &mut state.ui_tree,
                                    Event::ImeComposition {
                                        text: s.clone(),
                                        caret_units: 0,
                                    },
                                );

                                state.shell.dispatch_event(
                                    hwnd,
                                    &mut state.ui_tree,
                                    Event::ImeCommit { text: s.clone() },
                                );
                            }
                        }
                        // Handle ongoing composition string
                        else if flags & GCS_COMPSTR.0 != 0 {
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
                            // state.text_widget.ime_update(comp, caret_units);
                            state.shell.dispatch_event(
                                hwnd,
                                &mut state.ui_tree,
                                Event::ImeComposition {
                                    text: comp.clone(),
                                    caret_units,
                                },
                            );

                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }

                        let _ = ImmReleaseContext(hwnd, himc);
                    }
                }
                LRESULT(0)
            }
            WM_IME_ENDCOMPOSITION => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    state
                        .shell
                        .dispatch_event(hwnd, &mut state.ui_tree, Event::ImeEndComposition);

                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_TIMER => {
                let timer_id = wparam.0;
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    state.shell.kill_redraw_timer(hwnd, timer_id);
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    // Capture mouse & keyboard input
                    let _ = SetFocus(Some(hwnd));
                    let _ = SetCapture(hwnd);

                    // Extract mouse position in client pixels
                    let xi = (lparam.0 & 0xFFFF) as i16 as i32;
                    let yi = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                    let x_px = xi as f32;
                    let y_px = yi as f32;
                    let to_dip = dips_scale(hwnd);
                    let x = x_px * to_dip;
                    let y = y_px * to_dip;
                    // First, check scrollbar thumb hit-testing
                    if state.scroll_drag.is_none() {
                        if let Some(drag) = state.hit_test_scrollbar_thumb(x, y) {
                            state.scroll_drag = Some(drag);
                            // Ensure we receive keyboard input and mouse moves
                            let _ = SetFocus(Some(hwnd));
                            let _ = SetCapture(hwnd);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            return LRESULT(0);
                        }
                    }

                    // Compute running click count within system double-click time/rect
                    let now = GetMessageTime() as u32;
                    let thresh = GetDoubleClickTime();
                    let dx = (xi - state.last_click_pos.x).unsigned_abs();
                    let dy = (yi - state.last_click_pos.y).unsigned_abs();
                    let w = GetSystemMetrics(SM_CXDOUBLECLK) as u32 / 2;
                    let h = GetSystemMetrics(SM_CYDOUBLECLK) as u32 / 2;
                    let within_rect = dx <= w && dy <= h;
                    let within_time = now.wrapping_sub(state.last_click_time) <= thresh;

                    if within_time && within_rect {
                        state.click_count = state.click_count.saturating_add(1);
                    } else {
                        state.click_count = 1;
                    }

                    let modifiers = get_modifiers();
                    state.shell.dispatch_event(
                        hwnd,
                        &mut state.ui_tree,
                        Event::MouseButtonDown {
                            x,
                            y,
                            click_count: state.click_count,
                            modifiers,
                        },
                    );

                    state.last_click_time = now;
                    state.last_click_pos = POINT { x: xi, y: yi };

                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    // Current mouse in pixels
                    let xi = (lparam.0 & 0xFFFF) as i16 as i32;
                    let yi = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

                    // Handle scrollbar dragging if active
                    if let Some(drag) = state.scroll_drag {
                        let to_dip = dips_scale(hwnd);
                        let x = (xi as f32) * to_dip;
                        let y = (yi as f32) * to_dip;

                        // Find element by id
                        let mut found_key: Option<DefaultKey> = None;
                        for k in state.ui_tree.keys() {
                            if state.ui_tree[k].id == Some(drag.element_id) {
                                found_key = Some(k);
                                break;
                            }
                        }

                        if let Some(k) = found_key {
                            let el = &state.ui_tree[k];
                            let axis = match drag.axis {
                                DragAxis::Vertical => Axis::Y,
                                DragAxis::Horizontal => Axis::X,
                            };
                            if let Some(geom) =
                                compute_scrollbar_geom(el, axis, &state.scroll_state_manager)
                            {
                                let pos_along = match drag.axis {
                                    DragAxis::Vertical => y,
                                    DragAxis::Horizontal => x,
                                };
                                let rel = (pos_along - geom.track_start - drag.grab_offset)
                                    .clamp(0.0, geom.range);
                                let progress = if geom.range > 0.0 {
                                    rel / geom.range
                                } else {
                                    0.0
                                };
                                let new_scroll = progress * geom.max_scroll;
                                let cur = state
                                    .scroll_state_manager
                                    .get_scroll_position(drag.element_id);
                                match drag.axis {
                                    DragAxis::Vertical => {
                                        state.scroll_state_manager.set_scroll_position(
                                            drag.element_id,
                                            ScrollPosition {
                                                x: cur.x,
                                                y: new_scroll,
                                            },
                                        );
                                    }
                                    DragAxis::Horizontal => {
                                        state.scroll_state_manager.set_scroll_position(
                                            drag.element_id,
                                            ScrollPosition {
                                                x: new_scroll,
                                                y: cur.y,
                                            },
                                        );
                                    }
                                }
                                let _ = InvalidateRect(Some(hwnd), None, false);
                            }
                        }
                        return LRESULT(0);
                    }

                    // Continue manual drag (selection or preview drop position)
                    let x_px = xi as f32;
                    let y_px = yi as f32;
                    let to_dip = dips_scale(hwnd);
                    let x = x_px * to_dip;
                    let y = y_px * to_dip;

                    state
                        .shell
                        .dispatch_event(hwnd, &mut state.ui_tree, Event::MouseMove { x, y });
                }
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let wheel_delta = (wparam.0 >> 16) as i16;
                let modifiers = (wparam.0 & 0xFFFF) as u16;
                let x = (lparam.0 & 0xFFFF) as i16 as i32 as f32;
                let y = (lparam.0 >> 16) as i16 as i32 as f32;

                let shift = (modifiers & MK_SHIFT.0 as u16) != 0;
                let axis = if shift { Axis::X } else { Axis::Y };

                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    let rect = window_rect(hwnd).unwrap();

                    let to_dip = dips_scale(hwnd);
                    let x_dip = (x - rect.left as f32) * to_dip;
                    let y_dip = (y - rect.top as f32) * to_dip;
                    let wheel_delta = -wheel_delta as f32 / 120.0;
                    let modifiers = get_modifiers();
                    state.shell.dispatch_event(
                        hwnd,
                        &mut state.ui_tree,
                        Event::MouseWheel {
                            x: x_dip,
                            y: y_dip,
                            wheel_delta,
                            modifiers,
                        },
                    );

                    if state.shell.capture_event() {
                        let point = PointDIP { x_dip, y_dip };

                        let root = state.ui_tree.keys().next().unwrap();
                        visitors::visit_reverse_bfs(&mut state.ui_tree, root, |ui_tree, key, _| {
                            let element = &mut ui_tree[key];
                            let bounds = element.bounds();
                            if point.within(bounds)
                                && let Some(ref config) = element.scroll
                                && let Some(element_id) = element.id
                            {
                                // If the point is within the scrollable area, scroll
                                if config.vertical == Some(true)
                                    && can_scroll_further(
                                        element,
                                        axis,
                                        if wheel_delta > 0.0 {
                                            ScrollDirection::Positive
                                        } else {
                                            ScrollDirection::Negative
                                        },
                                        &state.scroll_state_manager,
                                    )
                                {
                                    let mut scroll_lines = 3;
                                    SystemParametersInfoW(
                                        SPI_GETWHEELSCROLLLINES,
                                        0,
                                        Some(&mut scroll_lines as *mut i32 as *mut _),
                                        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
                                    )
                                    .unwrap();

                                    let wheel_delta =
                                        wheel_delta * LINE_HEIGHT as f32 * scroll_lines as f32;

                                    let (delta_x, delta_y) = if axis == Axis::Y {
                                        (0.0, wheel_delta)
                                    } else {
                                        (wheel_delta, 0.0)
                                    };

                                    state
                                        .scroll_state_manager
                                        .update_scroll_position(element_id, delta_x, delta_y);

                                    return VisitAction::Exit;
                                }
                            }

                            VisitAction::Continue
                        });
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    if state.scroll_drag.take().is_some() {
                        let _ = ReleaseCapture();
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        return LRESULT(0);
                    }
                    let x_px = (lparam.0 & 0xFFFF) as i16 as i32 as f32;
                    let y_px = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32 as f32;
                    let to_dip = dips_scale(hwnd);
                    let x = x_px * to_dip;
                    let y = y_px * to_dip;

                    let modifiers = get_modifiers();
                    state.shell.dispatch_event(
                        hwnd,
                        &mut state.ui_tree,
                        Event::MouseButtonUp {
                            x,
                            y,
                            click_count: state.click_count,
                            modifiers,
                        },
                    );

                    // Release mouse capture
                    let _ = ReleaseCapture();
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }

            WM_CHAR => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    // Suppress WM_CHAR while IME composition is active to avoid duplicate input
                    // TODO
                    // if state.text_widget.is_composing() {
                    //     return LRESULT(0);
                    // }
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
                                + (((high as u32 - 0xD800) << 10) | ((code - 0xDC00) & 0x3FF));
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
                        state.shell.dispatch_event(
                            hwnd,
                            &mut state.ui_tree,
                            Event::Char {
                                text: to_insert.into(),
                            },
                        );

                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_KEYDOWN => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    let vk = wparam.0 as u32;

                    let modifiers = get_modifiers();
                    state.shell.dispatch_event(
                        hwnd,
                        &mut state.ui_tree,
                        Event::KeyDown { key: vk, modifiers },
                    );

                    // if handled {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                    // }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_KEYUP => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    let vk = wparam.0 as u32;

                    let modifiers = get_modifiers();
                    state.shell.dispatch_event(
                        hwnd,
                        &mut state.ui_tree,
                        Event::KeyUp { key: vk, modifiers },
                    );

                    // if handled {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                    // }
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

                    // Register OLE drop target
                    if state.drop_target.is_none() {
                        let dt: IDropTarget = IntegratedDropTarget::new(hwnd, |hwnd, event| {
                            // Dispatch drag/drop events to the Shell
                            if let Some(app_state) = state_mut_from_hwnd(hwnd) {
                                if let Some(result) = app_state.shell.dispatch_drag_event(
                                    &mut app_state.ui_tree,
                                    &event,
                                    match &event {
                                        DragEvent::DragEnter { drag_info }
                                        | DragEvent::DragOver { drag_info }
                                        | DragEvent::Drop { drag_info } => drag_info.position,
                                        DragEvent::DragLeave => PointDIP {
                                            x_dip: 0.0,
                                            y_dip: 0.0,
                                        }, // Position not needed for DragLeave
                                    },
                                ) {
                                    // We don't get any other events while drag is ongoing, assume we need to redraw
                                    let _ = InvalidateRect(Some(hwnd), None, false);
                                    result.effect
                                } else {
                                    windows::Win32::System::Ole::DROPEFFECT_NONE
                                }
                            } else {
                                windows::Win32::System::Ole::DROPEFFECT_NONE
                            }
                        })
                        .into();
                        let _ = RegisterDragDrop(hwnd, &dt);
                        state.drop_target = Some(dt);
                    }
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
            WM_DPICHANGED => {
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
                        let point = PointDIP { x_dip, y_dip };

                        let mut cursor = None;
                        let root = state.ui_tree.keys().next().unwrap();
                        visitors::visit_reverse_bfs(
                            &mut state.ui_tree,
                            root,
                            |slots, element, _| {
                                let bounds = slots[element].bounds();
                                if let Some(ElementContent::Widget(ref widget)) =
                                    slots[element].content
                                {
                                    if point.within(bounds) {
                                        cursor = widget.cursor(
                                            slots[element].id,
                                            element,
                                            point,
                                            bounds,
                                        );
                                    }
                                }
                            },
                        );

                        if let Some(cursor) = cursor {
                            match cursor {
                                Cursor::Arrow => {
                                    let _ = SetCursor(Some(LoadCursorW(None, IDC_ARROW).unwrap()));
                                }
                                Cursor::IBeam => {
                                    let _ = SetCursor(Some(LoadCursorW(None, IDC_IBEAM).unwrap()));
                                }
                            }

                            return LRESULT(1);
                        }
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_PAINT => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    state.shell.replace_redraw_request(RedrawRequest::Wait);

                    if let Err(e) = state.on_paint(hwnd) {
                        eprintln!("Failed to paint: {e}");
                    }

                    let now = Instant::now();
                    state
                        .shell
                        .dispatch_event(hwnd, &mut state.ui_tree, Event::Redraw { now });
                }
                LRESULT(0)
            }
            WM_DISPLAYCHANGE => {
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_DESTROY => {
                // Revoke drop target first
                let _ = RevokeDragDrop(hwnd);
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    // state.text_widget.set_ole_drop_preview(None);
                    state.drop_target = None;
                }
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
    };

    if let Some(state) = state_mut_from_hwnd(hwnd) {
        state.shell.dispatch_operations(&mut state.ui_tree);
    }

    result
}

fn get_modifiers() -> Modifiers {
    let shift_down = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
    let ctrl_down = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
    let alt_down = unsafe { GetKeyState(VK_MENU.0 as i32) } < 0;
    Modifiers {
        shift: shift_down,
        ctrl: ctrl_down,
        alt: alt_down,
    }
}

fn main() -> Result<()> {
    unsafe {
        // Opt-in to Per-Monitor V2 DPI awareness for crisp rendering on high-DPI displays
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let _ = CoInitialize(None);

        // Initialize OLE for drag-and-drop
        let _ = OleInitialize(None);

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
        // Uninitialize OLE
        OleUninitialize();
        // Uninitialize COM
        CoUninitialize();
    }
    Ok(())
}
