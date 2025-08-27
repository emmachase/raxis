pub mod clipboard;
pub mod dragdrop;
pub mod focus;
pub mod scroll;
pub mod smooth_scroll;

use crate::gfx::PointDIP;
use crate::gfx::command_executor::CommandExecutor;
use crate::gfx::draw_commands::DrawCommandList;
use crate::layout::model::{
    Axis, Direction, Element, ElementContent, ScrollConfig, Sizing, create_tree,
};
use crate::layout::visitors::VisitAction;
use crate::layout::{
    self, OwnedUITree, ScrollDirection, can_scroll_further, compute_scrollbar_geom, visitors,
};
use crate::runtime::dragdrop::start_text_drag;
use crate::runtime::scroll::{ScrollPosition, ScrollStateManager};
use crate::runtime::smooth_scroll::SmoothScrollManager;
use crate::widgets::drop_target::DropTarget;
use crate::widgets::{Cursor, DragData, DragEvent, Event, Modifiers, Renderer};
use crate::{DeferredControl, HookManager, RedrawRequest, Shell, ViewFn, w_id};
use crate::{current_dpi, dips_scale, dips_scale_for_dpi, gfx::RectDIP};
use slotmap::DefaultKey;
use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::ops::DerefMut;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Instant;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct2D::Common::D2D1_ALPHA_MODE_IGNORE;
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1,
    D2D1_DEVICE_CONTEXT_OPTIONS_NONE, ID2D1Bitmap1, ID2D1Device7, ID2D1DeviceContext7,
    ID2D1Factory8,
};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_9_1, D3D_FEATURE_LEVEL_9_2, D3D_FEATURE_LEVEL_9_3,
    D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
    ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    DXGI_PRESENT, DXGI_SCALING_NONE, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
    DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIAdapter, IDXGIDevice4,
    IDXGIFactory7, IDXGIOutput, IDXGISurface, IDXGISwapChain1,
};
use windows::Win32::System::Com::CoUninitialize;
use windows::Win32::System::SystemServices::MK_SHIFT;
use windows::Win32::UI::Input::Ime::{
    CANDIDATEFORM, CFS_POINT, CPS_COMPLETE, ImmNotifyIME, ImmSetCandidateWindow, NI_COMPOSITIONSTR,
};
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
                Common::{D2D1_COLOR_F, D2D1_PIXEL_FORMAT},
                D2D1_DEBUG_LEVEL_NONE, D2D1_FACTORY_OPTIONS, D2D1_FACTORY_TYPE_SINGLE_THREADED,
                D2D1CreateFactory, ID2D1SolidColorBrush,
            },
            DirectWrite::{DWRITE_FACTORY_TYPE_SHARED, DWriteCreateFactory, IDWriteFactory},
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
use windows_core::{IUnknown, Interface};

pub const LINE_HEIGHT: u32 = 16;

pub struct SafeCursor(pub HCURSOR);
unsafe impl Send for SafeCursor {}
unsafe impl Sync for SafeCursor {}

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

struct MaybeGuard {
    #[cfg(debug_assertions)]
    guard: std::sync::MutexGuard<'static, AppState>,

    #[cfg(not(debug_assertions))]
    guard: &'static mut AppState,
}

impl Deref for MaybeGuard {
    type Target = AppState;

    fn deref(&self) -> &Self::Target {
        #[cfg(debug_assertions)]
        return self.guard.deref();

        #[cfg(not(debug_assertions))]
        return self.guard;
    }
}

impl DerefMut for MaybeGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        #[cfg(debug_assertions)]
        return self.guard.deref_mut();

        #[cfg(not(debug_assertions))]
        return self.guard;
    }
}

type WinUserData = Mutex<AppState>;

// Small helpers to reduce duplication and centralize Win32/DPI logic.
fn state_mut_from_hwnd(hwnd: HWND) -> Option<MaybeGuard> {
    unsafe {
        let ptr = WAM::GetWindowLongPtrW(hwnd, GWLP_USERDATA);

        #[cfg(debug_assertions)]
        if ptr != 0 {
            let mutex = &*(ptr as *const WinUserData);
            if mutex.try_lock().is_err() {
                panic!("mutex is locked");
            }

            Some(MaybeGuard {
                guard: mutex.lock().unwrap(),
            })
        } else {
            None
        }

        #[cfg(not(debug_assertions))]
        if ptr != 0 {
            Some(MaybeGuard {
                guard: (&mut *(ptr as *mut WinUserData)).get_mut().unwrap(),
            })
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

struct AppState {
    // d2d_factory: ID2D1Factory,
    // _dwrite_factory: IDWriteFactory,
    // _text_format: IDWriteTextFormat,
    device_resources: Rc<RefCell<DeviceResources>>, // TODO: This shouldn't really be necessary
    // render_target: Option<ID2D1HwndRenderTarget>,
    // solid_brush: Option<ID2D1SolidColorBrush>,
    clock: f64,
    timing_info: DWM_TIMING_INFO,
    // spinner: Spinner,
    view_fn: Box<ViewFn>,
    ui_tree: OwnedUITree,

    shell: Shell,

    scroll_state_manager: ScrollStateManager,
    smooth_scroll_manager: SmoothScrollManager,

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

pub struct DeviceResources {
    pub solid_brush: Option<ID2D1SolidColorBrush>,
    pub d2d_target_bitmap: Option<ID2D1Bitmap1>,
    pub back_buffer: Option<ID3D11Texture2D>,
    pub dxgi_swapchain: Option<IDXGISwapChain1>,

    pub dwrite_factory: IDWriteFactory,
    pub dxgi_factory: IDXGIFactory7,
    pub dxgi_adapter: IDXGIAdapter,
    pub dxgi_device: IDXGIDevice4,
    pub d2d_device_context: ID2D1DeviceContext7,
    pub d2d_device: ID2D1Device7,
    pub d2d_factory: ID2D1Factory8,
    pub d3d_context: ID3D11DeviceContext,
    pub d3d_device: ID3D11Device,
}

impl DeviceResources {
    fn create_device_resources(&mut self, hwnd: HWND) -> Result<()> {
        unsafe {
            let dxgi_swapchain = match self.dxgi_swapchain {
                Some(ref dxgi_swapchain) => dxgi_swapchain,
                None => {
                    // println!("Creating DXGI swapchain");
                    let swapchain_desc = DXGI_SWAP_CHAIN_DESC1 {
                        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1, // Don't use multi-sampling
                            Quality: 0,
                        },
                        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                        BufferCount: 2,
                        Scaling: DXGI_SCALING_NONE,
                        SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                        AlphaMode: DXGI_ALPHA_MODE_IGNORE,

                        ..Default::default()
                    };

                    let dxgi_swapchain: IDXGISwapChain1 =
                        self.dxgi_factory.CreateSwapChainForHwnd(
                            &self.d3d_device.cast::<IUnknown>()?,
                            hwnd,
                            &swapchain_desc,
                            None,
                            None as Option<&IDXGIOutput>,
                        )?;

                    self.dxgi_swapchain = Some(dxgi_swapchain);
                    self.dxgi_swapchain.as_ref().unwrap()
                }
            };

            let back_buffer = match self.back_buffer {
                Some(ref back_buffer) => back_buffer,
                None => {
                    // println!("Fetching back buffer");
                    let back_buffer: ID3D11Texture2D = dxgi_swapchain.GetBuffer(0)?;
                    self.back_buffer = Some(back_buffer);
                    self.back_buffer.as_ref().unwrap()
                }
            };

            if self.d2d_target_bitmap.is_none() {
                // println!("Creating D2D target bitmap");
                let dpi = current_dpi(hwnd); // TODO: Get X / Y

                let bitmap_properties = D2D1_BITMAP_PROPERTIES1 {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_IGNORE,
                    },
                    dpiX: dpi,
                    dpiY: dpi,
                    bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                    colorContext: ManuallyDrop::new(None),
                };

                self.d2d_device_context.SetDpi(dpi, dpi);

                let d2d_target_bitmap = self.d2d_device_context.CreateBitmapFromDxgiSurface(
                    &back_buffer.cast::<IDXGISurface>()?,
                    Some(&bitmap_properties),
                )?;

                self.d2d_device_context.SetTarget(&d2d_target_bitmap);
                self.d2d_target_bitmap = Some(d2d_target_bitmap);
            }

            if self.solid_brush.is_none() {
                // println!("Creating solid brush");
                let rt = &self.d2d_device_context;

                let black = D2D1_COLOR_F {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                };
                let brush = rt.CreateSolidColorBrush(&black, None)?;
                self.solid_brush = Some(brush);
            }
            Ok(())
        }
    }

    fn discard_device_resources(&mut self) {
        self.solid_brush = None;
        self.back_buffer = None;
        self.d2d_target_bitmap = None;

        unsafe {
            self.d2d_device_context.SetTarget(None);
            self.d3d_context.ClearState();

            if let Some(ref mut swap_chain) = self.dxgi_swapchain {
                swap_chain
                    .ResizeBuffers(
                        0,
                        0,
                        0,
                        DXGI_FORMAT_UNKNOWN,
                        DXGI_SWAP_CHAIN_FLAG::default(),
                    )
                    .unwrap();
            }
        }
    }
}

impl AppState {
    fn new(view_fn: Box<ViewFn>) -> Result<Self> {
        unsafe {
            let mut d3d_device = None;
            let mut d3d_context = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[
                    D3D_FEATURE_LEVEL_11_1,
                    D3D_FEATURE_LEVEL_11_0,
                    D3D_FEATURE_LEVEL_10_1,
                    D3D_FEATURE_LEVEL_10_0,
                    D3D_FEATURE_LEVEL_9_3,
                    D3D_FEATURE_LEVEL_9_2,
                    D3D_FEATURE_LEVEL_9_1,
                ]),
                D3D11_SDK_VERSION,
                Some(&mut d3d_device),
                None,
                Some(&mut d3d_context),
            )?;
            let d3d_device = d3d_device.unwrap();
            let d3d_context = d3d_context.unwrap();

            let dxgi_device: IDXGIDevice4 = Interface::cast(&d3d_device)?;

            // Ensure that DXGI doesn't queue more than one frame at a time.
            dxgi_device.SetMaximumFrameLatency(1)?;

            let options = D2D1_FACTORY_OPTIONS {
                debugLevel: D2D1_DEBUG_LEVEL_NONE,
            };
            let d2d_factory: ID2D1Factory8 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, Some(&options))?;

            let dwrite_factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;

            let d2d_device = d2d_factory.CreateDevice(&dxgi_device)?;

            let d2d_device_context =
                d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;

            let dxgi_adapter = dxgi_device.GetAdapter()?;
            let dxgi_factory = dxgi_adapter.GetParent::<IDXGIFactory7>()?;

            let device_resources = DeviceResources {
                d3d_device,
                d3d_context,
                d2d_factory,
                d2d_device,
                d2d_device_context,
                dxgi_device,
                dxgi_adapter,
                dxgi_factory,
                dwrite_factory,
                solid_brush: None,
                back_buffer: None,
                d2d_target_bitmap: None,
                dxgi_swapchain: None,
            };

            let mut ui_tree = OwnedUITree::default();

            create_tree_root(&view_fn, &device_resources, &mut ui_tree);

            let shell = Shell::new();

            Ok(Self {
                device_resources: Rc::new(RefCell::new(device_resources)),
                clock: 0.0,
                timing_info: DWM_TIMING_INFO::default(),
                ui_tree,
                view_fn,
                shell,
                scroll_state_manager: ScrollStateManager::default(),
                smooth_scroll_manager: SmoothScrollManager::new(),
                drop_target: None,
                pending_high_surrogate: None,
                last_click_time: 0,
                last_click_pos: POINT { x: 0, y: 0 },
                click_count: 0,
                scroll_drag: None,
            })
        }
    }

    fn on_paint(&mut self, hwnd: HWND) -> Result<DrawCommandList> {
        let dt = self.timing_info.rateCompose.uiDenominator as f64
            / self.timing_info.rateCompose.uiNumerator as f64;

        // Update smooth scroll animations and apply positions to scroll state manager
        self.update_smooth_scroll_animations();

        create_tree_root(
            &self.view_fn,
            &self.device_resources.borrow(),
            &mut self.ui_tree,
        );
        let root = self.ui_tree.root;

        let rc = client_rect(hwnd)?;
        let rc_dip = RectDIP::from(hwnd, rc);
        self.ui_tree.slots[root].width = Sizing::fixed(rc_dip.width_dip);
        self.ui_tree.slots[root].height = Sizing::fixed(rc_dip.height_dip);

        layout::layout(&mut self.ui_tree, root, &mut self.scroll_state_manager);
        let commands = layout::paint(
            &self.shell,
            &mut self.ui_tree,
            root,
            &mut self.scroll_state_manager,
            0.0,
            0.0,
        );

        self.clock += dt;

        Ok(commands)
    }

    fn on_resize(&mut self, width: u32, height: u32) -> Result<()> {
        // let rt = &self.device_resources.d2d_device_context;

        // unsafe {
        //     rt.Resize(&D2D_SIZE_U { width, height })?;
        // }

        unsafe {
            let mut device_resources = self.device_resources.borrow_mut();
            device_resources.d2d_device_context.SetTarget(None);
            device_resources.d3d_context.ClearState();

            device_resources.d2d_target_bitmap = None;
            device_resources.back_buffer = None;

            if let Some(ref mut swap_chain) = device_resources.dxgi_swapchain {
                swap_chain.ResizeBuffers(
                    0,
                    width,
                    height,
                    DXGI_FORMAT_UNKNOWN,
                    DXGI_SWAP_CHAIN_FLAG::default(),
                )?;
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
            let element = &state.ui_tree.slots[key];
            // Recurse into children first
            for child in element.children.iter() {
                dfs(state, *child, x, y, out);
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

        let root = self.ui_tree.root;
        let mut result = None;
        dfs(self, root, x, y, &mut result);
        result
    }

    fn update_smooth_scroll_animations(&mut self) {
        // Update all smooth scroll animations and apply current positions
        self.smooth_scroll_manager.update_animations();

        // Apply current animated positions to the scroll state manager
        for (&element_id, animation) in self.smooth_scroll_manager.get_active_animations() {
            let current_pos = animation.current_position(std::time::Instant::now());
            self.scroll_state_manager
                .set_scroll_position(element_id, current_pos);
        }
    }
}

fn create_tree_root(
    view_fn: &ViewFn,
    device_resources: &DeviceResources,
    ui_tree: &mut OwnedUITree,
) {
    let children = view_fn(HookManager { ui_tree });
    create_tree(
        device_resources,
        ui_tree,
        Element {
            id: Some(w_id!()),
            background_color: Some(0xFFFFFFFF),
            direction: Direction::TopToBottom,
            scroll: Some(ScrollConfig {
                vertical: Some(true),
                ..Default::default()
            }),
            children: vec![children],

            ..Default::default()
        },
    )
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let result = unsafe {
        match msg {
            WM_IME_STARTCOMPOSITION => {
                // println!("WM_IME_STARTCOMPOSITION");
                if let Some(mut state) = state_mut_from_hwnd(hwnd) {
                    let state = state.deref_mut();

                    state.shell.dispatch_event(
                        hwnd,
                        &mut state.ui_tree,
                        Event::ImeStartComposition,
                    );
                }
                LRESULT(0)
            }
            WM_IME_COMPOSITION => {
                // println!("WM_IME_COMPOSITION");
                if let Some(mut state) = state_mut_from_hwnd(hwnd) {
                    let state = state.deref_mut();

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
                // println!("WM_IME_ENDCOMPOSITION");
                if let Some(mut state) = state_mut_from_hwnd(hwnd) {
                    let state = state.deref_mut();

                    state
                        .shell
                        .dispatch_event(hwnd, &mut state.ui_tree, Event::ImeEndComposition);

                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_TIMER => {
                // println!("WM_TIMER");
                let timer_id = wparam.0;
                if let Some(mut state) = state_mut_from_hwnd(hwnd) {
                    let state = state.deref_mut();
                    state.shell.kill_redraw_timer(hwnd, timer_id);
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                // println!("WM_LBUTTONDOWN");
                // Capture mouse & keyboard input
                let _ = SetFocus(Some(hwnd));
                let _ = SetCapture(hwnd);

                if let Some(mut state) = state_mut_from_hwnd(hwnd) {
                    let state = state.deref_mut();

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
                // println!("WM_MOUSEMOVE");
                if let Some(mut state) = state_mut_from_hwnd(hwnd) {
                    let state = state.deref_mut();
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
                        for k in state.ui_tree.slots.keys() {
                            if state.ui_tree.slots[k].id == Some(drag.element_id) {
                                found_key = Some(k);
                                break;
                            }
                        }

                        if let Some(k) = found_key {
                            let el = &state.ui_tree.slots[k];
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

                    // println!("MouseMove {} {}", x, y);
                    state
                        .shell
                        .dispatch_event(hwnd, &mut state.ui_tree, Event::MouseMove { x, y });
                }
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                // println!("WM_MOUSEWHEEL");
                let wheel_delta = (wparam.0 >> 16) as i16;
                let modifiers = (wparam.0 & 0xFFFF) as u16;
                let x = (lparam.0 & 0xFFFF) as i16 as i32 as f32;
                let y = (lparam.0 >> 16) as i16 as i32 as f32;

                let shift = (modifiers & MK_SHIFT.0 as u16) != 0;
                let axis = if shift { Axis::X } else { Axis::Y };

                if let Some(mut state) = state_mut_from_hwnd(hwnd) {
                    let state = state.deref_mut();
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

                        let root = state.ui_tree.root;
                        visitors::visit_reverse_bfs(&mut state.ui_tree, root, |ui_tree, key, _| {
                            let element = &mut ui_tree.slots[key];
                            let bounds = element.bounds();
                            if point.within(bounds.border_box)
                                && element.scroll.is_some()
                                && let Some(element_id) = element.id
                            {
                                // If the point is within the scrollable area, scroll
                                if can_scroll_further(
                                    element,
                                    axis,
                                    if wheel_delta > 0.0 {
                                        ScrollDirection::Positive
                                    } else {
                                        ScrollDirection::Negative
                                    },
                                    &state.scroll_state_manager,
                                ) {
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

                                    // Get current scroll position (either from active animation or actual position)
                                    let current_pos =
                                        state.scroll_state_manager.get_scroll_position(element_id);
                                    let current_animated_pos = state
                                        .smooth_scroll_manager
                                        .get_current_position(element_id, current_pos);

                                    // Use accumulate_scroll_delta for fast scrolling support
                                    let delta = ScrollPosition {
                                        x: delta_x,
                                        y: delta_y,
                                    };

                                    state.smooth_scroll_manager.accumulate_scroll_delta(
                                        element_id,
                                        current_animated_pos,
                                        delta,
                                    );

                                    state
                                        .shell
                                        .request_redraw(hwnd, crate::RedrawRequest::Immediate);

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
                // println!("WM_LBUTTONUP");
                if let Some(mut state) = state_mut_from_hwnd(hwnd) {
                    let state = state.deref_mut();

                    // If we take scroll_drag, ignore the event as we consume it
                    if state.scroll_drag.take().is_none() {
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
                    }

                    // println!("WM_LBUTTONUP dispatch finished");
                }

                // Release mouse capture
                let _ = ReleaseCapture();
                // println!("WM_LBUTTONUP release capture");
                let _ = InvalidateRect(Some(hwnd), None, false);
                // println!("WM_LBUTTONUP invalidate rect");

                // println!("WM_LBUTTONUP finished");
                LRESULT(0)
            }

            WM_CHAR => {
                // println!("WM_CHAR");
                if let Some(mut state) = state_mut_from_hwnd(hwnd) {
                    let state = state.deref_mut();
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
                // println!("WM_KEYDOWN");
                if let Some(mut state) = state_mut_from_hwnd(hwnd) {
                    let state = state.deref_mut();
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
                // println!("WM_KEYUP");
                if let Some(mut state) = state_mut_from_hwnd(hwnd) {
                    let state = state.deref_mut();
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
                // println!("WM_CREATE");
                let pcs = lparam.0 as *const CREATESTRUCTW;
                let ptr = (*pcs).lpCreateParams;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);

                if let Some(mut state) = state_mut_from_hwnd(hwnd) {
                    let state = state.deref_mut();
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
                        let dt: IDropTarget = DropTarget::new(hwnd, |hwnd, event| {
                            // Dispatch drag/drop events to the Shell
                            if let Some(mut app_state) = state_mut_from_hwnd(hwnd) {
                                let app_state = app_state.deref_mut();
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
                // println!("WM_SIZE");
                if let Some(mut state) = state_mut_from_hwnd(hwnd) {
                    let state = state.deref_mut();
                    let width = (lparam.0 & 0xFFFF) as u32;
                    let height = ((lparam.0 >> 16) & 0xFFFF) as u32;
                    if let Err(e) = state.on_resize(width, height) {
                        eprintln!("Failed to resize: {e}");
                    }
                    // let _ = InvalidateRect(Some(hwnd), None, true);
                }
                let _ = UpdateWindow(hwnd);
                LRESULT(0)
            }
            WM_DPICHANGED => {
                // println!("WM_DPICHANGED");

                let suggested = &*(lparam.0 as *const RECT);
                SetWindowPos(
                    hwnd,
                    None,
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )
                .unwrap();

                if let Some(mut state) = state_mut_from_hwnd(hwnd) {
                    let state = state.deref_mut();
                    state
                        .device_resources
                        .borrow_mut()
                        .discard_device_resources();

                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_SETCURSOR => {
                // println!("WM_SETCURSOR");
                // Set I-beam cursor when hovering over visible text bounds (in client area)
                let hit_test = (lparam.0 & 0xFFFF) as u32;
                if hit_test == HTCLIENT {
                    if let Some(mut state) = state_mut_from_hwnd(hwnd) {
                        let state = state.deref_mut();
                        // Get mouse in client pixels and convert to DIPs
                        let mut pt = POINT { x: 0, y: 0 };
                        let _ = GetCursorPos(&mut pt);
                        let _ = ScreenToClient(hwnd, &mut pt);
                        let to_dip = dips_scale(hwnd);
                        let x_dip = (pt.x as f32) * to_dip;
                        let y_dip = (pt.y as f32) * to_dip;
                        let point = PointDIP { x_dip, y_dip };

                        let mut cursor = None;
                        let root = state.ui_tree.root;
                        visitors::visit_reverse_bfs(
                            &mut state.ui_tree,
                            root,
                            |ui_tree, element, _| {
                                let bounds = ui_tree.slots[element].bounds();
                                if let Some(ElementContent::Widget(ref widget)) =
                                    ui_tree.slots[element].content
                                {
                                    if let Some(id) = ui_tree.slots[element].id {
                                        if let Some(instance) = ui_tree.widget_state.get(&id) {
                                            if point.within(bounds.border_box) {
                                                cursor = widget.cursor(instance, point, bounds);
                                            }
                                        }
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
                // println!("WM_PAINT");
                let commands = if let Some(mut state) = state_mut_from_hwnd(hwnd) {
                    let state = state.deref_mut();
                    state.shell.replace_redraw_request(RedrawRequest::Wait);

                    let now = Instant::now();
                    state
                        .shell
                        .dispatch_event(hwnd, &mut state.ui_tree, Event::Redraw { now });

                    

                    match state.on_paint(hwnd) {
                        Ok(commands) => Some((
                            state.device_resources.clone(),
                            commands,
                            state.shell.redraw_request,
                        )),
                        Err(e) => {
                            eprintln!("Failed to paint: {e}");
                            None
                        }
                    }
                } else {
                    None
                };

                if let Some((device_resources, commands, redraw_request)) = commands {
                    let mut device_resources = device_resources.borrow_mut();
                    device_resources.create_device_resources(hwnd).ok();
                    // Refresh target DPI in case it changed (e.g. monitor move)
                    // self.update_dpi(hwnd);
                    let mut ps = PAINTSTRUCT::default();
                    // println!("BeginPaint");
                    BeginPaint(hwnd, &mut ps);

                    if let (rt, Some(brush)) = (
                        &device_resources.d2d_device_context,
                        &device_resources.solid_brush,
                    ) {
                        rt.BeginDraw();
                        let white = D2D1_COLOR_F {
                            r: 1.0,
                            g: 1.0,
                            b: 1.0,
                            a: 1.0,
                        };
                        rt.Clear(Some(&white));

                        CommandExecutor::execute_commands(
                            &Renderer {
                                render_target: rt,
                                brush,
                                factory: &device_resources.d2d_factory,
                            },
                            &commands,
                        )
                        .ok();

                        // let root = self.root_key;

                        // Spinner drawn above uses the current brush color.

                        let end = rt.EndDraw(None, None);
                        if let Err(e) = end {
                            if e.code() == D2DERR_RECREATE_TARGET {
                                println!("Recreating D2D target");
                                device_resources.discard_device_resources();
                                device_resources.create_device_resources(hwnd).ok();
                            }
                        }
                    }

                    let _ = EndPaint(hwnd, &ps);

                    // TODO: Pass present dirty rects / scroll info
                    if let Some(ref swap_chain) = device_resources.dxgi_swapchain {
                        let _ = swap_chain.Present(1, DXGI_PRESENT::default());
                    }

                    if matches!(redraw_request, RedrawRequest::Immediate) {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    // println!("EndPaint");
                }

                LRESULT(0)
            }
            WM_DISPLAYCHANGE => {
                // println!("WM_DISPLAYCHANGE");
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_DESTROY => {
                // println!("WM_DESTROY");
                let _ = RevokeDragDrop(hwnd);

                let ptr = WAM::GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if ptr != 0 {
                    drop(Box::from_raw(ptr as *mut WinUserData));
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                WAM::PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    };

    let deferred_controls = if let Some(mut state) = state_mut_from_hwnd(hwnd) {
        let state = state.deref_mut();
        state.shell.dispatch_operations(&mut state.ui_tree);

        // TODO: Maybe move this into a deferred control
        // Schedule next frame if we have active animations
        if state.smooth_scroll_manager.has_any_active_animations() {
            state.shell.request_redraw(hwnd, RedrawRequest::Immediate);
        }

        state.shell.drain_deferred_controls()
    } else {
        None
    };

    if let Some(deferred_controls) = deferred_controls {
        for control in deferred_controls {
            match control {
                DeferredControl::StartDrag { data, src_id } => {
                    let DragData::Text(text) = data;

                    if let Ok(effect) = start_text_drag(&text, true) {
                        if let Some(mut state) = state_mut_from_hwnd(hwnd) {
                            let state = state.deref_mut();

                            let event = Event::DragFinish { effect };

                            state
                                .shell
                                .dispatch_event_to(hwnd, &mut state.ui_tree, event, src_id);
                        }
                    }
                }

                DeferredControl::DisableIME => unsafe {
                    let himc = ImmGetContext(hwnd);
                    if !himc.is_invalid() {
                        let _ = ImmNotifyIME(himc, NI_COMPOSITIONSTR, CPS_COMPLETE, 0);
                    }
                },

                DeferredControl::SetIMEPosition { position } => unsafe {
                    let himc = ImmGetContext(hwnd);
                    if !himc.is_invalid() {
                        let to_dip = dips_scale(hwnd);
                        let cf = CANDIDATEFORM {
                            dwStyle: CFS_POINT,
                            ptCurrentPos: POINT {
                                x: (position.x_dip / to_dip).round() as i32,
                                y: (position.y_dip / to_dip).round() as i32,
                            },
                            rcArea: RECT::default(),
                            dwIndex: 0,
                        };
                        let _ = ImmSetCandidateWindow(himc, &cf);

                        let _ = ImmReleaseContext(hwnd, himc);
                    }
                },
            }
        }
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

pub fn run_event_loop(view_fn: impl Fn(HookManager) -> Element + 'static) -> Result<()> {
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

        let app = AppState::new(Box::new(view_fn))?;
        let mut dpi_x = 0.0f32;
        let mut dpi_y = 0.0f32;
        app.device_resources
            .borrow()
            .d2d_factory
            .GetDesktopDpi(&mut dpi_x, &mut dpi_y);

        let app = Mutex::new(app);

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
