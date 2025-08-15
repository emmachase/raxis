// #![windows_subsystem = "windows"]

use raxis::dragdrop::start_text_drag;
use raxis::layout::model::{
    Axis, ElementContent, HorizontalAlignment, ScrollConfig, Sizing, UIElement, VerticalAlignment,
};
use raxis::layout::scroll_manager::{ScrollPosition, ScrollStateManager};
use raxis::layout::{self, OwnedUITree, compute_scrollbar_geom};
use raxis::w_id;
use raxis::widgets::{Event, Widget};
use raxis::{
    current_dpi, dips_scale, dips_scale_for_dpi,
    gfx::RectDIP,
    widgets::{
        selectable_text::{SelectableText, SelectionMode},
        spinner::Spinner,
    },
};
use slotmap::DefaultKey;
use std::{ffi::c_void, sync::OnceLock};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_LEADING,
};
use windows::Win32::System::Com::{
    CoUninitialize, DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM, TYMED_HGLOBAL,
};
use windows::Win32::System::Ole::ReleaseStgMedium;
use windows::Win32::UI::WindowsAndMessaging::WM_KEYUP;
use windows::{
    Win32::{
        Foundation::{
            D2DERR_RECREATE_TARGET, GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, POINT,
            POINTL, RECT, S_OK, WPARAM,
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
            DataExchange::{
                CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
                OpenClipboard, SetClipboardData,
            },
            LibraryLoader::GetModuleHandleW,
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
            Ole::{
                CF_UNICODETEXT, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_MOVE, IDropTarget,
                IDropTarget_Impl, OleInitialize, OleUninitialize, RegisterDragDrop, RevokeDragDrop,
            },
            SystemServices::{MK_CONTROL, MODIFIERKEYS_FLAGS},
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
                    GetDoubleClickTime, GetKeyState, ReleaseCapture, SetCapture, SetFocus, VK_A,
                    VK_BACK, VK_C, VK_CONTROL, VK_DELETE, VK_END, VK_HOME, VK_LEFT, VK_RIGHT,
                    VK_SHIFT, VK_V, VK_X,
                },
            },
            WindowsAndMessaging::{
                self as WAM, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW,
                DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetClientRect, GetCursorPos,
                GetMessageTime, GetMessageW, GetSystemMetrics, HCURSOR, HTCLIENT, IDC_ARROW,
                IDC_IBEAM, LoadCursorW, MSG, RegisterClassW, SM_CXDOUBLECLK, SM_CXDRAG,
                SM_CYDOUBLECLK, SM_CYDRAG, SW_SHOW, SWP_NOACTIVATE, SWP_NOZORDER, SetCursor,
                SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, WINDOW_EX_STYLE,
                WM_CHAR, WM_COPY, WM_CREATE, WM_CUT, WM_DESTROY, WM_DISPLAYCHANGE,
                WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_STARTCOMPOSITION, WM_KEYDOWN,
                WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_PASTE, WM_SETCURSOR,
                WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
            },
        },
    },
    core::{PCWSTR, Result, implement, w},
};
use windows_numerics::Vector2;

pub struct SafeCursor(pub HCURSOR);
unsafe impl Send for SafeCursor {}
unsafe impl Sync for SafeCursor {}

static IBEAM_CURSOR: OnceLock<Option<SafeCursor>> = OnceLock::new();

const TEXT: &str = "Hello, בְּרֵאשִׁ֖ית בָּרָ֣א אֱלֹהִ֑ים אֵ֥ת הַשָּׁמַ֖יִם וְאֵ֥ת הָאָֽרֶץ.  DirectWrite! こんにちは 😍";

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

// ===== OLE Drop Target implementation =====
#[implement(IDropTarget)]
struct DropTarget {
    hwnd: HWND,
}

impl DropTarget {
    fn new(hwnd: HWND) -> Self {
        Self { hwnd }
    }

    fn choose_effect(&self, keys: MODIFIERKEYS_FLAGS) -> DROPEFFECT {
        if (keys.0 as u32 & MK_CONTROL.0 as u32) != 0 {
            DROPEFFECT_COPY
        } else {
            DROPEFFECT_MOVE
        }
    }

    fn update_preview_from_point(&self, pt: &POINTL) {
        unsafe {
            if let Some(state) = state_mut_from_hwnd(self.hwnd) {
                let mut p = POINT { x: pt.x, y: pt.y };
                let _ = ScreenToClient(self.hwnd, &mut p);
                let to_dip = dips_scale(self.hwnd);
                let x = (p.x as f32) * to_dip;
                let y = (p.y as f32) * to_dip;
                // TODO
                // if let Ok(idx16) = state.text_widget.hit_test_index(x, y) {
                //     state.text_widget.set_ole_drop_preview(Some(idx16));
                // }
                let _ = InvalidateRect(Some(self.hwnd), None, false);
            }
        }
    }

    fn insert_from_dataobject(
        &self,
        data: &IDataObject,
        pt: &POINTL,
        effect: DROPEFFECT,
    ) -> windows::core::Result<()> {
        unsafe {
            // Compute drop index from point
            if let Some(state) = state_mut_from_hwnd(self.hwnd) {
                let mut p = POINT { x: pt.x, y: pt.y };
                let _ = ScreenToClient(self.hwnd, &mut p);
                let to_dip = dips_scale(self.hwnd);
                let x = (p.x as f32) * to_dip;
                let y = (p.y as f32) * to_dip;
                // TODO
                // if let Ok(idx16) = state.text_widget.hit_test_index(x, y) {
                //     state.text_widget.set_ole_drop_preview(Some(idx16));

                //     // Request CF_UNICODETEXT via HGLOBAL
                //     let fmt = FORMATETC {
                //         cfFormat: CF_UNICODETEXT.0,
                //         ptd: std::ptr::null_mut(),
                //         dwAspect: DVASPECT_CONTENT.0 as u32,
                //         lindex: -1,
                //         tymed: TYMED_HGLOBAL.0 as u32,
                //     };
                //     if let Ok(mut medium) = data.GetData(&fmt) {
                //         let h = medium.u.hGlobal;
                //         let ptr = GlobalLock(h) as *const u16;
                //         if !ptr.is_null() {
                //             // Read until NUL
                //             let mut out: Vec<u16> = Vec::new();
                //             let mut i = 0isize;
                //             loop {
                //                 let v = *ptr.offset(i);
                //                 if v == 0 {
                //                     break;
                //                 }
                //                 out.push(v);
                //                 i += 1;
                //             }
                //             let _ = GlobalUnlock(h);
                //             let mut s = String::from_utf16_lossy(&out);

                //             // If we dropped from our own drag, remove the selection
                //             let internal_move = state.text_widget.can_drag_drop()
                //                 && (effect.0 & DROPEFFECT_MOVE.0) != 0;

                //             // Normalize CRLF to LF for internal text
                //             s = s.replace("\r\n", "\n");

                //             state.text_widget.finish_ole_drop(&s, internal_move)?;
                //         }
                //         let _ = ReleaseStgMedium(&mut medium as *mut STGMEDIUM);
                //     }
                //     state.text_widget.set_ole_drop_preview(None);
                //     let _ = InvalidateRect(Some(self.hwnd), None, false);
                // }
            }
            Ok(())
        }
    }
}

#[allow(non_snake_case)]
impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(
        &self,
        pDataObj: windows_core::Ref<'_, IDataObject>,
        grfKeyState: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdwEffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        unsafe {
            let mut accepts = false;
            if let Some(dobj) = pDataObj.as_ref() {
                let fmt = FORMATETC {
                    cfFormat: CF_UNICODETEXT.0,
                    ptd: std::ptr::null_mut(),
                    dwAspect: DVASPECT_CONTENT.0 as u32,
                    lindex: -1,
                    tymed: TYMED_HGLOBAL.0 as u32,
                };
                let hr = dobj.QueryGetData(&fmt);
                accepts = hr == S_OK;
            }
            let eff = if accepts {
                self.choose_effect(grfKeyState)
            } else {
                DROPEFFECT(0)
            };
            if !pdwEffect.is_null() {
                *pdwEffect = eff;
            }
            if accepts {
                self.update_preview_from_point(pt);
            }
        }
        Ok(())
    }

    fn DragOver(
        &self,
        grfKeyState: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdwEffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        unsafe {
            if !pdwEffect.is_null() {
                *pdwEffect = self.choose_effect(grfKeyState);
            }
            self.update_preview_from_point(pt);
        }
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        unsafe {
            if let Some(state) = state_mut_from_hwnd(self.hwnd) {
                // TODO
                // state.text_widget.set_ole_drop_preview(None);
                let _ = InvalidateRect(Some(self.hwnd), None, false);
            }
        }
        Ok(())
    }

    fn Drop(
        &self,
        pDataObj: windows_core::Ref<'_, IDataObject>,
        grfKeyState: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdwEffect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let effect = self.choose_effect(grfKeyState);
        unsafe {
            if !pdwEffect.is_null() {
                *pdwEffect = effect;
            }
        }
        if let Some(data) = pDataObj.as_ref() {
            let _ = self.insert_from_dataobject(data, pt, effect);
        }
        Ok(())
    }
}

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
    _dwrite_factory: IDWriteFactory,
    _text_format: IDWriteTextFormat,
    render_target: Option<ID2D1HwndRenderTarget>,
    black_brush: Option<ID2D1SolidColorBrush>,

    clock: f64,
    timing_info: DWM_TIMING_INFO,
    spinner: Spinner,

    ui_tree: OwnedUITree,
    scroll_state_manager: ScrollStateManager,

    // Selectable text widget encapsulating layout, selection, and bounds
    // text_widget: SelectableText,
    text_widget_ui_key: DefaultKey,

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
                72.0,
                PCWSTR(w!("en-us").as_ptr()),
            )?;
            text_format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
            text_format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;

            let spinner = Spinner::new(6.0, 200.0, 1.6);

            // Build selectable text widget using shared DWrite factory/format
            let text_widget = SelectableText::new(
                dwrite_factory.clone(),
                text_format.clone(),
                TEXT.to_string(),
            );

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

                background_color: Some(0xFFFF00FF),

                scroll: Some(ScrollConfig {
                    // horizontal: Some(true),
                    vertical: Some(true),

                    sticky_right: Some(true),
                    sticky_bottom: Some(true),

                    ..Default::default()
                }),

                horizontal_alignment: HorizontalAlignment::Center,

                width: Sizing::grow(),
                height: Sizing::grow(),

                ..Default::default()
            });
            ui_tree[root].children.push(child);

            let child2 = ui_tree.insert(UIElement {
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
                content: Some(ElementContent::Widget(Box::new(text_widget))),

                color: Some(0x6030F0FF),

                ..Default::default()
            });
            ui_tree[child].children.push(child2);

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

            Ok(Self {
                d2d_factory,
                _dwrite_factory: dwrite_factory,
                _text_format: text_format,
                render_target: None,
                black_brush: None,
                clock: 0.0,
                timing_info: DWM_TIMING_INFO::default(),
                spinner,
                // text_widget,
                ui_tree,
                scroll_state_manager: ScrollStateManager::default(),
                drop_target: None,
                pending_high_surrogate: None,
                last_click_time: 0,
                last_click_pos: POINT { x: 0, y: 0 },
                click_count: 0,
                scroll_drag: None,
                text_widget_ui_key: child2,
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

                // let _ = self.text_widget.update_bounds(rc_dip);
                // let _ = self.text_widget.draw(rt, brush, dt);

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

                let root = self.ui_tree.keys().next().unwrap();
                self.ui_tree[root].width = Sizing::fixed(rc_dip.width_dip);
                self.ui_tree[root].height = Sizing::fixed(rc_dip.height_dip);

                layout::layout(&mut self.ui_tree, root, &mut self.scroll_state_manager);
                layout::paint(
                    &rt,
                    brush,
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

        let root = match self.ui_tree.keys().next() {
            Some(k) => k,
            None => return None,
        };
        let mut result = None;
        dfs(self, root, x, y, &mut result);
        result
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            // WM_IME_STARTCOMPOSITION => {
            //     if let Some(state) = state_mut_from_hwnd(hwnd) {
            //         state.text_widget.ime_begin();
            //         // Position IME window at current caret
            //         let to_dip = dips_scale(hwnd);
            //         if let Ok((x_dip, y_dip, h)) = state
            //             .text_widget
            //             .caret_pos_dip(state.text_widget.caret_active16())
            //         {
            //             let x_px = (x_dip / to_dip).round() as i32;
            //             let y_px = ((y_dip + h) / to_dip).round() as i32;
            //             let himc = ImmGetContext(hwnd);
            //             if !himc.is_invalid() {
            //                 let cf = CANDIDATEFORM {
            //                     dwStyle: CFS_POINT,
            //                     ptCurrentPos: POINT { x: x_px, y: y_px },
            //                     rcArea: RECT::default(),
            //                     dwIndex: 0,
            //                 };
            //                 let _ = ImmSetCandidateWindow(himc, &cf);

            //                 let _ = ImmReleaseContext(hwnd, himc);
            //             }
            //         }
            //         let _ = InvalidateRect(Some(hwnd), None, false);
            //     }
            //     LRESULT(0)
            // }
            // WM_IME_COMPOSITION => {
            //     if let Some(state) = state_mut_from_hwnd(hwnd) {
            //         let himc = ImmGetContext(hwnd);
            //         if !himc.is_invalid() {
            //             let flags = lparam.0 as u32;

            //             // Handle result string (committed text)
            //             if flags & GCS_RESULTSTR.0 != 0 {
            //                 let bytes = ImmGetCompositionStringW(himc, GCS_RESULTSTR, None, 0);
            //                 if bytes > 0 {
            //                     let mut buf: Vec<u16> = vec![0; (bytes as usize) / 2];
            //                     let _ = ImmGetCompositionStringW(
            //                         himc,
            //                         GCS_RESULTSTR,
            //                         Some(buf.as_mut_ptr() as *mut _),
            //                         bytes as u32,
            //                     );
            //                     let s = String::from_utf16_lossy(&buf);
            //                     let _ = state.text_widget.ime_commit(s);
            //                 }
            //             }

            //             // Handle ongoing composition string
            //             if flags & GCS_COMPSTR.0 != 0 {
            //                 let bytes = ImmGetCompositionStringW(himc, GCS_COMPSTR, None, 0);
            //                 let mut comp = String::new();
            //                 if bytes > 0 {
            //                     let mut buf: Vec<u16> = vec![0; (bytes as usize) / 2];
            //                     let _ = ImmGetCompositionStringW(
            //                         himc,
            //                         GCS_COMPSTR,
            //                         Some(buf.as_mut_ptr() as *mut _),
            //                         bytes as u32,
            //                     );
            //                     comp = String::from_utf16_lossy(&buf);
            //                 }
            //                 // Caret within comp string (UTF-16 units)
            //                 let caret_units = {
            //                     let v = ImmGetCompositionStringW(himc, GCS_CURSORPOS, None, 0);
            //                     if v < 0 { 0 } else { v as u32 }
            //                 };
            //                 state.text_widget.ime_update(comp, caret_units);

            //                 // Reposition IME window at composition caret
            //                 let to_dip = dips_scale(hwnd);
            //                 if let Ok((x_dip, y_dip, h)) = state.text_widget.ime_caret_pos_dip() {
            //                     let x_px = (x_dip / to_dip).round() as i32;
            //                     let y_px = ((y_dip + h) / to_dip).round() as i32;
            //                     let cf = CANDIDATEFORM {
            //                         dwStyle: CFS_FORCE_POSITION,
            //                         ptCurrentPos: POINT { x: x_px, y: y_px },
            //                         rcArea: RECT::default(),
            //                         dwIndex: 0,
            //                     };
            //                     let _ = ImmSetCandidateWindow(himc, &cf);
            //                 }

            //                 let _ = InvalidateRect(Some(hwnd), None, false);
            //             }

            //             let _ = ImmReleaseContext(hwnd, himc);
            //         }
            //     }
            //     LRESULT(0)
            // }
            // WM_IME_ENDCOMPOSITION => {
            //     if let Some(state) = state_mut_from_hwnd(hwnd) {
            //         state.text_widget.ime_end();
            //         let _ = InvalidateRect(Some(hwnd), None, false);
            //     }
            //     LRESULT(0)
            // }
            WM_LBUTTONDOWN => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
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

                    let bounds = state.ui_tree[state.text_widget_ui_key].bounds();

                    let widget = state.ui_tree[state.text_widget_ui_key]
                        .content
                        .as_mut()
                        .unwrap()
                        .unwrap_widget();
                    widget.update(
                        hwnd,
                        Event::MouseButtonDown {
                            x,
                            y,
                            click_count: state.click_count,
                        },
                        bounds,
                    );

                    state.last_click_time = now;
                    state.last_click_pos = POINT { x: xi, y: yi };

                    // Ensure we receive keyboard input
                    let _ = SetFocus(Some(hwnd));
                    let _ = SetCapture(hwnd);
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

                    // TODO
                    // if state.text_widget.can_drag_drop() {
                    //     // If we've exceeded the system drag threshold,
                    //     // escalate to OLE DoDragDrop with CF_UNICODETEXT.
                    //     let dx = (xi - state.last_click_pos.x).unsigned_abs();
                    //     let dy = (yi - state.last_click_pos.y).unsigned_abs();
                    //     let w = GetSystemMetrics(SM_CXDRAG) as u32 / 2;
                    //     let h = GetSystemMetrics(SM_CYDRAG) as u32 / 2;
                    //     if dx > w || dy > h {
                    //         if let Some(s) = state.text_widget.selected_text() {
                    //             // Hand control to OLE DnD
                    //             let _ = ReleaseCapture();
                    //             state.text_widget.cancel_drag();
                    //             let effect = start_text_drag(&s, true).unwrap_or_default();
                    //             if (effect.0 & DROPEFFECT_MOVE.0) != 0 {
                    //                 // Delete original selection on successful MOVE drop
                    //                 let _ = state.text_widget.insert_str("");
                    //             }
                    //             state.text_widget.set_can_drag_drop(false);
                    //             let _ = InvalidateRect(Some(hwnd), None, false);
                    //             return LRESULT(0);
                    //         }
                    //     }
                    // }

                    // Continue manual drag (selection or preview drop position)
                    let x_px = xi as f32;
                    let y_px = yi as f32;
                    let to_dip = dips_scale(hwnd);
                    let x = x_px * to_dip;
                    let y = y_px * to_dip;
                    // if state.text_widget.update_drag(x, y) {
                    //     let _ = InvalidateRect(Some(hwnd), None, false);
                    // }

                    let bounds = state.ui_tree[state.text_widget_ui_key].bounds();
                    let widget = state.ui_tree[state.text_widget_ui_key]
                        .content
                        .as_mut()
                        .unwrap()
                        .unwrap_widget();
                    // state
                    //     .text_widget
                    widget.update(hwnd, Event::MouseMove { x, y }, bounds);
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
                    // if let Ok(idx) = state.text_widget.hit_test_index(x, y) {
                    //     state.text_widget.end_drag(idx);
                    // } else {
                    //     // Even on failure, ensure drag ends
                    //     state.text_widget.end_drag(0);
                    // }

                    let bounds = state.ui_tree[state.text_widget_ui_key].bounds();
                    let widget = state.ui_tree[state.text_widget_ui_key]
                        .content
                        .as_mut()
                        .unwrap()
                        .unwrap_widget();
                    widget.update(
                        hwnd,
                        Event::MouseButtonUp {
                            x,
                            y,
                            click_count: state.click_count,
                        },
                        bounds,
                    );

                    let _ = ReleaseCapture();
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            // WM_COPY => {
            //     if let Some(state) = state_mut_from_hwnd(hwnd) {
            //         if let Some(s) = state.text_widget.selected_text() {
            //             let _ = set_clipboard_text(hwnd, &s);
            //         }
            //     }
            //     LRESULT(0)
            // }
            // WM_CUT => {
            //     if let Some(state) = state_mut_from_hwnd(hwnd) {
            //         if let Some(s) = state.text_widget.selected_text() {
            //             let _ = set_clipboard_text(hwnd, &s);
            //             let _ = state.text_widget.insert_str("");
            //             let _ = InvalidateRect(Some(hwnd), None, false);
            //         }
            //     }
            //     LRESULT(0)
            // }
            // WM_PASTE => {
            //     if let Some(state) = state_mut_from_hwnd(hwnd) {
            //         if !state.text_widget.is_composing() {
            //             if let Some(s) = get_clipboard_text(hwnd) {
            //                 let _ = state.text_widget.insert_str(&s);
            //                 let _ = InvalidateRect(Some(hwnd), None, false);
            //             }
            //         }
            //     }
            //     LRESULT(0)
            // }
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
                        // let _ = state.text_widget.insert_str(&to_insert);
                        let bounds = state.ui_tree[state.text_widget_ui_key].bounds();
                        let widget = state.ui_tree[state.text_widget_ui_key]
                            .content
                            .as_mut()
                            .unwrap()
                            .unwrap_widget();
                        widget.update(
                            hwnd,
                            Event::Char {
                                text: to_insert.into(),
                            },
                            bounds,
                        );

                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
                LRESULT(0)
            }
            WM_KEYDOWN => {
                if let Some(state) = state_mut_from_hwnd(hwnd) {
                    let vk = wparam.0 as u32;

                    let bounds = state.ui_tree[state.text_widget_ui_key].bounds();
                    let widget = state.ui_tree[state.text_widget_ui_key]
                        .content
                        .as_mut()
                        .unwrap()
                        .unwrap_widget();
                    widget.update(hwnd, Event::KeyDown { key: vk }, bounds);

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

                    let bounds = state.ui_tree[state.text_widget_ui_key].bounds();
                    let widget = state.ui_tree[state.text_widget_ui_key]
                        .content
                        .as_mut()
                        .unwrap()
                        .unwrap_widget();
                    widget.update(hwnd, Event::KeyUp { key: vk }, bounds);

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
                        let dt: IDropTarget = DropTarget::new(hwnd).into();
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

                        let widget = &state.ui_tree[state.text_widget_ui_key];

                        let RectDIP {
                            x_dip: left,
                            y_dip: top,
                            width_dip: width,
                            height_dip: height,
                        } = widget.bounds(); //state.text_widget.metric_bounds();
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
