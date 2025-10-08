pub mod clipboard;
pub mod context_menu;
pub mod dragdrop;
pub mod focus;
pub mod font_manager;
pub mod scroll;
pub mod smooth_scroll;
pub mod syscommand;
pub mod task;
pub mod tray;
pub mod vkey;

use crate::gfx::PointDIP;
use crate::gfx::command_executor::CommandExecutor;
use crate::gfx::draw_commands::DrawCommandList;
use crate::layout::model::{Axis, Direction, Element, Sizing, create_tree};
use crate::layout::{
    self, OwnedUITree, ScrollDirection, can_scroll_further, compute_scrollbar_geom,
};
use crate::runtime::context_menu::{ContextMenu, ContextMenuRequest, WM_SHOW_CONTEXT_MENU};
use crate::runtime::dragdrop::start_text_drag;
use crate::runtime::focus::FocusManager;
use crate::runtime::scroll::{ScrollPosition, ScrollStateManager};
use crate::runtime::smooth_scroll::SmoothScrollManager;
use crate::runtime::syscommand::{SystemCommand, SystemCommandResponse};
use crate::runtime::task::{
    Action, ClipboardAction, ContextMenuAction, Task, WindowAction, WindowMode, into_stream,
};
use crate::runtime::tray::{TrayEvent, TrayIcon, TrayIconConfig, WM_TRAYICON};
use crate::runtime::vkey::VKey;
use crate::widgets::drop_target::DropTarget;
use crate::widgets::renderer::{Renderer, ShadowCache};
use crate::widgets::{DragData, DragEvent, Event, Modifiers};
use crate::{
    DeferredControl, EventMapperFn, HookManager, RedrawRequest, Shell, UpdateFn, ViewFn, w_id,
};
use crate::{current_dpi, dips_scale, gfx::RectDIP};
use log::{error, warn};
use slotmap::DefaultKey;
use std::cell::RefCell;
use std::mem::ManuallyDrop;
use std::ops::DerefMut;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::sync::{MutexGuard, mpsc};
use std::thread;
use std::time::Instant;
use thiserror::Error;
use windows::Win32::Foundation::{COLORREF, HMODULE};
use windows::Win32::Graphics::Direct2D::Common::D2D1_ALPHA_MODE_PREMULTIPLIED;
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1,
    D2D1_DEVICE_CONTEXT_OPTIONS_NONE, ID2D1Bitmap1, ID2D1Device6, ID2D1DeviceContext6,
    ID2D1Factory7,
};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_9_1, D3D_FEATURE_LEVEL_9_2, D3D_FEATURE_LEVEL_9_3,
    D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
    ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice2, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::Graphics::Dwm::{
    DWM_BB_ENABLE, DWM_BLURBEHIND, DWM_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW, DWMSBT_NONE,
    DWMSBT_TABBEDWINDOW, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DwmDefWindowProc, DwmEnableBlurBehindWindow,
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
    DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIAdapter, IDXGIDevice4,
    IDXGIFactory7, IDXGISurface, IDXGISwapChain1,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, ClientToScreen, CreateSolidBrush, DeleteObject, EndPaint, FillRect, HDC,
    PAINTSTRUCT,
};
use windows::Win32::System::Com::CoUninitialize;
use windows::Win32::System::SystemServices::MK_SHIFT;
use windows::Win32::UI::Controls::{
    CS_ACTIVE, CloseThemeData, GetThemePartSize, MARGINS, OpenThemeData, TS_TRUE, WP_CAPTION,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::Ime::{
    CANDIDATEFORM, CFS_POINT, CPS_COMPLETE, ImmNotifyIME, ImmSetCandidateWindow, NI_COMPOSITIONSTR,
};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_MENU;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, DestroyWindow, GetForegroundWindow, GetWindowRect, HTBOTTOM, HTBOTTOMLEFT,
    HTBOTTOMRIGHT, HTCAPTION, HTLEFT, HTNOWHERE, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, MINMAXINFO,
    NCCALCSIZE_PARAMS, PostMessageW, SM_CXFRAME, SM_CXPADDEDBORDER, SM_CYFRAME,
    SPI_GETWHEELSCROLLLINES, SWP_FRAMECHANGED, SWP_NOMOVE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SetForegroundWindow, SystemParametersInfoW, WM_ACTIVATE, WM_CLOSE, WM_DPICHANGED,
    WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYUP, WM_MOUSEWHEEL, WM_NCCALCSIZE, WM_NCHITTEST,
    WM_SYSCOMMAND, WM_TIMER, WM_USER, WS_CAPTION, WS_EX_NOREDIRECTIONBITMAP,
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
            DirectWrite::{DWRITE_FACTORY_TYPE_SHARED, DWriteCreateFactory, IDWriteFactory6},
            Dwm::DWM_TIMING_INFO,
            Dxgi::Common::DXGI_FORMAT_UNKNOWN,
            Gdi::{InvalidateRect, ScreenToClient, UpdateWindow},
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
                self as WAM, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW,
                DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetClientRect, GetCursorPos,
                GetMessageTime, GetMessageW, GetSystemMetrics, HTCLIENT, IDC_ARROW, LoadCursorW,
                MSG, RegisterClassW, SM_CXDOUBLECLK, SM_CYDOUBLECLK, SW_HIDE, SW_SHOW,
                SWP_NOACTIVATE, SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos, ShowWindow,
                TranslateMessage, WINDOW_EX_STYLE, WM_CHAR, WM_DESTROY, WM_DISPLAYCHANGE,
                WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_STARTCOMPOSITION, WM_KEYDOWN,
                WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SETCURSOR, WM_SIZE,
                WNDCLASSW, WS_OVERLAPPEDWINDOW,
            },
        },
    },
    core::{PCWSTR, w},
};
use windows_core::Error as WinError;
use windows_core::{BOOL, IUnknown, Interface};

pub const LINE_HEIGHT: u32 = 32;

// Custom message for async task results
const WM_ASYNC_MESSAGE: u32 = WM_USER + 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UncheckedHWND(pub HWND);
unsafe impl Send for UncheckedHWND {}
unsafe impl Sync for UncheckedHWND {}

#[derive(Clone, Copy, Debug)]
struct ScrollDragState {
    element_id: u64,
    axis: Axis,
    // Offset within the thumb (along the drag axis) where the pointer grabbed, in DIPs
    grab_offset: f32,
}

type WinUserData<State, Message> = Mutex<ApplicationHandle<State, Message>>;

// Small helpers to reduce duplication and centralize Win32/DPI logic.
fn state_mut_from_hwnd<State, Message>(
    hwnd: HWND,
) -> Option<MutexGuard<'static, ApplicationHandle<State, Message>>> {
    unsafe {
        let ptr = WAM::GetWindowLongPtrW(hwnd, GWLP_USERDATA);

        if ptr != 0 {
            let mutex = &*(ptr as *const WinUserData<State, Message>);
            if mutex.try_lock().is_err() {
                warn!("event-loop mutex was locked, skipping event");
                return None;
            }

            Some(mutex.lock().unwrap())
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
        let client_rc = client_rect(hwnd)?;
        let mut offset = POINT { x: 0, y: 0 };
        ClientToScreen(hwnd, &mut offset).ok()?;

        Ok(RECT {
            left: offset.x,
            top: offset.y,
            right: offset.x + client_rc.right,
            bottom: offset.y + client_rc.bottom,
        })
    }
}

struct ApplicationHandle<State, Message> {
    device_resources: Rc<RefCell<DeviceResources>>, // TODO: This shouldn't really be necessary

    clock: f64,
    timing_info: DWM_TIMING_INFO,
    view_fn: ViewFn<State, Message>,
    update_fn: UpdateFn<State, Message>,
    _event_mapper_fn: EventMapperFn<Message>,
    user_state: State,

    ui_tree: OwnedUITree<Message>,

    shell: Shell<Message>,

    smooth_scroll_manager: SmoothScrollManager,

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

    // Async task executor
    task_sender: mpsc::Sender<Task<Message>>,
    message_receiver: mpsc::Receiver<Message>,

    // System tray icon
    tray_icon: Option<TrayIcon>,
    tray_event_handler: Option<Box<dyn Fn(&State, TrayEvent) -> Option<Task<Message>>>>,

    // System command handler
    syscommand_handler:
        Option<Box<dyn Fn(&State, SystemCommand) -> SystemCommandResponse<Message>>>,
}

pub struct DeviceResources {
    pub solid_brush: Option<ID2D1SolidColorBrush>,
    pub d2d_target_bitmap: Option<ID2D1Bitmap1>,
    pub back_buffer: Option<ID3D11Texture2D>,
    pub dxgi_swapchain: Option<IDXGISwapChain1>,

    pub dwrite_factory: IDWriteFactory6,
    pub dxgi_factory: IDXGIFactory7,
    pub dxgi_adapter: IDXGIAdapter,
    pub dxgi_device: IDXGIDevice4,
    pub d2d_device_context: ID2D1DeviceContext6,
    pub d2d_device: ID2D1Device6,
    pub d2d_factory: ID2D1Factory7,
    pub d3d_context: ID3D11DeviceContext,
    pub d3d_device: ID3D11Device,

    // DirectComposition objects
    pub dcomp_device: IDCompositionDevice,
    pub dcomp_target: IDCompositionTarget,
    pub dcomp_visual: Option<IDCompositionVisual>,

    pub shadow_cache: RefCell<ShadowCache>,
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Windows API error: {0}")]
    WindowsApi(#[from] WinError),

    #[error("Failed to create D3D11 device")]
    D3D11DeviceCreationFailed(WinError),

    #[error("Failed to create D2D factory")]
    D2DFactoryCreationFailed(WinError),

    #[error("Failed to create DWrite factory")]
    DWriteFactoryCreationFailed(WinError),

    #[error("Failed to initialize global font manager")]
    FontManagerInitializationFailed(WinError),

    #[error("Failed to create D2D device")]
    D2DDeviceCreationFailed(WinError),

    #[error("Failed to create D2D device context")]
    D2DDeviceContextCreationFailed(WinError),

    #[error("Failed to get DXGI adapter")]
    DxgiAdapterCreationFailed(WinError),

    #[error("Failed to create DirectComposition device")]
    DcompDeviceCreationFailed(WinError),

    #[error("Failed to create DirectComposition target")]
    DcompTargetCreationFailed(WinError),

    #[error("Failed to create DirectComposition visual")]
    DcompVisualCreationFailed(WinError),

    #[error("Failed to create DXGI swapchain")]
    DxgiSwapchainCreationFailed(WinError),

    #[error("Failed to resize swap chain buffers")]
    SwapChainResizeFailed(WinError),

    #[error("Failed to create window")]
    WindowCreationFailed(WinError),

    #[error("Task channel send failed")]
    TaskChannelSendFailed,
}

pub type Result<T> = std::result::Result<T, RuntimeError>;

impl DeviceResources {
    fn create_device_resources(&mut self, hwnd: HWND, width: u32, height: u32) -> Result<()> {
        unsafe {
            let dxgi_swapchain = match self.dxgi_swapchain {
                Some(ref dxgi_swapchain) => dxgi_swapchain,
                None => {
                    // println!("Creating DXGI swapchain");
                    let swapchain_desc = DXGI_SWAP_CHAIN_DESC1 {
                        Width: width,
                        Height: height,
                        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1, // Don't use multi-sampling
                            Quality: 0,
                        },
                        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                        BufferCount: 2,
                        Scaling: DXGI_SCALING_STRETCH,
                        SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                        AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,

                        ..Default::default()
                    };

                    let dxgi_swapchain: IDXGISwapChain1 = self
                        .dxgi_factory
                        .CreateSwapChainForComposition(
                            &self.d3d_device.cast::<IUnknown>().unwrap(),
                            &swapchain_desc,
                            None,
                        )
                        .map_err(RuntimeError::DxgiSwapchainCreationFailed)?;

                    // Create DirectComp visual
                    // TODO: split this out?
                    let dcomp_visual = self
                        .dcomp_device
                        .CreateVisual()
                        .map_err(RuntimeError::DcompVisualCreationFailed)?;
                    dcomp_visual
                        .SetContent(&dxgi_swapchain)
                        .map_err(RuntimeError::DcompVisualCreationFailed)?;
                    self.dcomp_target
                        .SetRoot(&dcomp_visual)
                        .map_err(RuntimeError::DcompTargetCreationFailed)?;
                    self.dcomp_visual = Some(dcomp_visual);

                    self.dxgi_swapchain = Some(dxgi_swapchain);
                    self.dxgi_swapchain
                        .as_ref()
                        .expect("Failed to create DXGI swapchain")
                }
            };

            let back_buffer = match self.back_buffer {
                Some(ref back_buffer) => back_buffer,
                None => {
                    // println!("Fetching back buffer");
                    let back_buffer: ID3D11Texture2D = dxgi_swapchain
                        .GetBuffer(0)
                        .map_err(RuntimeError::DxgiSwapchainCreationFailed)?;
                    self.back_buffer = Some(back_buffer);
                    self.back_buffer
                        .as_ref()
                        .expect("Failed to create back buffer")
                }
            };

            if self.d2d_target_bitmap.is_none() {
                // println!("Creating D2D target bitmap");
                let dpi = current_dpi(hwnd); // TODO: Get X / Y

                let bitmap_properties = D2D1_BITMAP_PROPERTIES1 {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: dpi,
                    dpiY: dpi,
                    bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                    colorContext: ManuallyDrop::new(None),
                };

                self.d2d_device_context.SetDpi(dpi, dpi);

                let d2d_target_bitmap = self
                    .d2d_device_context
                    .CreateBitmapFromDxgiSurface(
                        &back_buffer.cast::<IDXGISurface>().unwrap(),
                        Some(&bitmap_properties),
                    )
                    .map_err(RuntimeError::DxgiSwapchainCreationFailed)?;

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
                let brush = rt
                    .CreateSolidColorBrush(&black, None)
                    .map_err(RuntimeError::DxgiSwapchainCreationFailed)?;
                self.solid_brush = Some(brush);
            }

            // Initialize DirectComposition objects if not already created
            // if self.dcomp_device.is_none() {
            //     // Step 3: Create DirectComposition device object
            //     let dcomp_device =.unwrap();
            //     self.dcomp_device = Some(dcomp_device);
            // }

            // if self.dcomp_target.is_none() && self.dcomp_device.is_some() {
            //     // Step 4: Create composition target object
            //     let dcomp_device = self.dcomp_device.as_ref().unwrap();
            //     let dcomp_target = dcomp_device.CreateTargetForHwnd(hwnd, true).unwrap();
            //     self.dcomp_target = Some(dcomp_target);
            // }

            // if self.dcomp_visual.is_none() && self.dcomp_device.is_some() {
            //     // Step 5: Create visual object
            //     let dcomp_device = self.dcomp_device.as_ref().unwrap();
            //     let dcomp_visual = dcomp_device.CreateVisual().unwrap();
            //     self.dcomp_visual = Some(dcomp_visual);

            //     // Set the visual as the root visual of the target
            //     if let Some(ref target) = self.dcomp_target {
            //         target.SetRoot(&dcomp_visual).unwrap();
            //     }

            //     // Commit the composition to make it visible
            //     dcomp_device.Commit().unwrap();
            // }

            Ok(())
        }
    }

    fn discard_device_resources(&mut self) {
        self.solid_brush = None;
        self.back_buffer = None;
        self.d2d_target_bitmap = None;
        // Clean up DirectComposition objects
        // self.dcomp_visual = None;
        // self.dcomp_target = None;
        // self.dcomp_device = None;
        self.shadow_cache.borrow_mut().clear();

        unsafe {
            self.d2d_device_context.SetTarget(None);
            self.d3d_context.ClearState();

            // if let Some(ref mut swap_chain) = self.dxgi_swapchain {
            //     swap_chain
            //         .ResizeBuffers(
            //             0,
            //             0,
            //             0,
            //             DXGI_FORMAT_UNKNOWN,
            //             DXGI_SWAP_CHAIN_FLAG::default(),
            //         )
            //         .unwrap();
            // }
        }
    }
}

static PENDING_MESSAGE_PROCESSING: AtomicBool = AtomicBool::new(false);

impl<State: 'static, Message: 'static + Send + Clone> ApplicationHandle<State, Message> {
    fn new(
        view_fn: ViewFn<State, Message>,
        update_fn: UpdateFn<State, Message>,
        event_mapper_fn: EventMapperFn<Message>,
        boot_fn: impl Fn(&State) -> Option<Task<Message>>,
        user_state: State,
        hwnd: HWND,
        tray_config: Option<TrayIconConfig>,
        tray_event_handler: Option<Box<dyn Fn(&State, TrayEvent) -> Option<Task<Message>>>>,
        syscommand_handler: Option<
            Box<dyn Fn(&State, SystemCommand) -> SystemCommandResponse<Message>>,
        >,
    ) -> Result<Self> {
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
            )
            .map_err(RuntimeError::D3D11DeviceCreationFailed)?;
            let d3d_device = d3d_device.expect("Failed to create D3D device");
            let d3d_context = d3d_context.expect("Failed to create D3D context");

            let dxgi_device: IDXGIDevice4 = Interface::cast(&d3d_device).unwrap();

            // Ensure that DXGI doesn't queue more than one frame at a time.
            dxgi_device
                .SetMaximumFrameLatency(1)
                .map_err(RuntimeError::WindowsApi)?;

            let options = D2D1_FACTORY_OPTIONS {
                debugLevel: D2D1_DEBUG_LEVEL_NONE,
            };
            let d2d_factory: ID2D1Factory7 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, Some(&options))
                    .map_err(RuntimeError::D2DFactoryCreationFailed)?;

            let dwrite_factory: IDWriteFactory6 =
                DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)
                    .map_err(RuntimeError::DWriteFactoryCreationFailed)?;

            // Initialize global font manager
            font_manager::GlobalFontManager::initialize(dwrite_factory.clone())
                .map_err(RuntimeError::FontManagerInitializationFailed)?;

            let d2d_device = d2d_factory
                .CreateDevice(&dxgi_device)
                .map_err(RuntimeError::D2DDeviceCreationFailed)?;

            let d2d_device_context = d2d_device
                .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)
                .map_err(RuntimeError::D2DDeviceContextCreationFailed)?;

            let dxgi_adapter = dxgi_device
                .GetAdapter()
                .map_err(RuntimeError::DxgiAdapterCreationFailed)?;
            let dxgi_factory = dxgi_adapter
                .GetParent::<IDXGIFactory7>()
                .map_err(RuntimeError::DxgiAdapterCreationFailed)?;

            // Direct Composition
            let dcomp_device: IDCompositionDevice = DCompositionCreateDevice2(&d2d_device)
                .map_err(RuntimeError::DcompDeviceCreationFailed)?;
            let dcomp_target: IDCompositionTarget = dcomp_device
                .CreateTargetForHwnd(hwnd, true)
                .map_err(RuntimeError::DcompTargetCreationFailed)?;

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
                // DirectComposition objects - initially None, will be created in create_device_resources
                dcomp_device,
                dcomp_target,
                dcomp_visual: None,
                shadow_cache: RefCell::new(ShadowCache::default()),
            };

            // Call boot before we touch the tree
            let boot_task = boot_fn(&user_state);

            let mut ui_tree = OwnedUITree::<Message>::default();
            let mut scroll_state_manager = ScrollStateManager::default();
            let mut focus_manager = FocusManager::default();

            create_tree_root(
                &user_state,
                view_fn,
                &device_resources,
                &mut HookManager {
                    ui_tree: &mut ui_tree,
                    scroll_state_manager: &mut scroll_state_manager,
                    focus_manager: &mut focus_manager,
                    layout_invalidated: false,
                    requested_animation: false,
                    window_active: GetForegroundWindow() == hwnd,
                },
            );

            // Create channels for async task execution
            let (task_sender, task_receiver) = mpsc::channel::<Task<Message>>();
            let (message_sender, message_receiver) = mpsc::channel::<Message>();
            if let Some(boot_task) = boot_task {
                task_sender
                    .send(boot_task)
                    .map_err(|_| RuntimeError::TaskChannelSendFailed)?;
            }

            let shell = Shell::new(
                message_sender.clone(),
                task_sender.clone(),
                scroll_state_manager,
                focus_manager,
                event_mapper_fn,
            );

            // Spawn executor thread with selected async runtime
            {
                let message_sender = message_sender.clone();
                let hwnd = UncheckedHWND(hwnd);
                thread::spawn(move || {
                    async fn process_task_stream<Message: Send + Clone + 'static>(
                        stream: impl futures::Stream<Item = Action<Message>> + Send + Unpin + 'static,
                        message_sender: mpsc::Sender<Message>,
                        hwnd: UncheckedHWND,
                    ) {
                        use futures::StreamExt;
                        let mut stream = stream;
                        while let Some(action) = stream.next().await {
                            // if let Action::Output(message) = action {
                            match action {
                                Action::Output(message) => {
                                    // Send message to channel for UI thread processing
                                    let _ = message_sender.send(message);

                                    // If the UI thread is not processing messages, notify it
                                    if !PENDING_MESSAGE_PROCESSING.swap(true, Ordering::SeqCst) {
                                        unsafe {
                                            PostMessageW(
                                                Some(hwnd.0),
                                                WM_ASYNC_MESSAGE,
                                                WPARAM(0),
                                                LPARAM(0),
                                            )
                                            .ok();
                                        }
                                    }
                                }
                                Action::Clipboard(action) => match action {
                                    ClipboardAction::Set(text) => {
                                        let _ = clipboard::set_clipboard_text(hwnd.0, &text);
                                    }
                                    ClipboardAction::Get(sender) => {
                                        let _ = sender.send(clipboard::get_clipboard_text(hwnd.0));
                                    }
                                },
                                Action::Window(action) => match action {
                                    WindowAction::Activate => unsafe {
                                        SetForegroundWindow(hwnd.0).ok().ok();
                                    },
                                    WindowAction::SetMode(mode) => unsafe {
                                        let show_cmd = match mode {
                                            WindowMode::Windowed => SW_SHOW,
                                            WindowMode::Hidden => SW_HIDE,
                                        };
                                        ShowWindow(hwnd.0, show_cmd).ok().ok();
                                    },
                                },
                                Action::ContextMenu(ContextMenuAction::Show {
                                    items,
                                    position,
                                    sender,
                                }) => {
                                    // Build the context menu from items
                                    let menu = ContextMenu::new(items);

                                    // Request the menu to be shown on the UI thread and await the result
                                    let result = menu.show_async(hwnd, position).await;

                                    // Send the result back through the channel
                                    let _ = sender.send(result);
                                }
                                Action::Exit => unsafe {
                                    PostMessageW(Some(hwnd.0), WM_CLOSE, WPARAM(0), LPARAM(0)).ok();
                                },
                            }
                        }
                    }

                    async fn run_task_loop<Message: Send + Clone + 'static>(
                        task_receiver: mpsc::Receiver<Task<Message>>,
                        message_sender: mpsc::Sender<Message>,
                        hwnd: UncheckedHWND,
                    ) {
                        while let Ok(task) = task_receiver.recv() {
                            if let Some(stream) = into_stream(task) {
                                let message_sender = message_sender.clone();

                                #[cfg(all(feature = "smol-runtime", not(feature = "tokio")))]
                                smol::spawn(process_task_stream(stream, message_sender, hwnd))
                                    .detach();

                                #[cfg(feature = "tokio")]
                                tokio::spawn(process_task_stream(stream, message_sender, hwnd));
                            }
                        }
                    }

                    #[cfg(all(feature = "smol-runtime", not(feature = "tokio")))]
                    smol::block_on(run_task_loop(task_receiver, message_sender, hwnd));

                    #[cfg(feature = "tokio")]
                    {
                        let rt = tokio::runtime::Builder::new_multi_thread()
                            .enable_all()
                            .build()
                            .expect("Failed to create tokio runtime");
                        rt.block_on(run_task_loop(task_receiver, message_sender, hwnd));
                    }
                });
            }

            // Create and add tray icon if configured
            let mut tray_icon = tray_config.map(|config| TrayIcon::new(hwnd, config));
            if let Some(ref mut tray) = tray_icon {
                let _ = tray.add(); // Ignore errors during initialization
            }

            Ok(Self {
                device_resources: Rc::new(RefCell::new(device_resources)),
                clock: 0.0,
                timing_info: DWM_TIMING_INFO::default(),
                ui_tree,
                view_fn,
                update_fn,
                _event_mapper_fn: event_mapper_fn,
                user_state,
                shell,
                smooth_scroll_manager: SmoothScrollManager::new(),
                drop_target: None,
                pending_high_surrogate: None,
                last_click_time: 0,
                last_click_pos: POINT { x: 0, y: 0 },
                click_count: 0,
                scroll_drag: None,
                task_sender,
                message_receiver,
                tray_icon,
                tray_event_handler,
                syscommand_handler,
            })
        }
    }

    fn on_paint(&mut self, hwnd: HWND) -> Result<DrawCommandList> {
        let dt = self.timing_info.rateCompose.uiDenominator as f64
            / self.timing_info.rateCompose.uiNumerator as f64;

        let window_active = unsafe { GetForegroundWindow() == hwnd };

        // Update smooth scroll animations and apply positions to scroll state manager
        self.update_smooth_scroll_animations();

        // Allow at most 5 layout passes, otherwise assume infinite loop
        for _ in 0..5 {
            let mut hook = HookManager {
                ui_tree: &mut self.ui_tree,
                scroll_state_manager: &mut self.shell.scroll_state_manager,
                focus_manager: &mut self.shell.focus_manager,
                layout_invalidated: false,
                requested_animation: false,
                window_active,
            };

            create_tree_root(
                &self.user_state,
                self.view_fn,
                &self.device_resources.borrow(),
                &mut hook,
            );
            let invalidated = hook.layout_invalidated;
            if hook.requested_animation {
                self.shell.request_redraw(hwnd, RedrawRequest::Immediate);
            }

            let root = self.ui_tree.root;

            let rc = client_rect(hwnd).unwrap();
            let rc_dip = RectDIP::from(hwnd, rc);
            self.ui_tree.slots[root].width = Sizing::fixed(rc_dip.width);
            self.ui_tree.slots[root].height = Sizing::fixed(rc_dip.height);

            let dip_scale = dips_scale(hwnd);

            layout::layout(
                &mut self.ui_tree,
                root,
                &mut self.shell.scroll_state_manager,
                dip_scale,
            );

            if !invalidated {
                break;
            }
        }

        let root = self.ui_tree.root;
        let commands = layout::paint(&mut self.shell, &mut self.ui_tree, root);

        self.clock += dt;

        Ok(commands)
    }

    fn on_resize(&mut self, width: u32, height: u32) -> Result<()> {
        // let rt = &self.device_resources.d2d_device_context;

        // unsafe {
        //     rt.Resize(&D2D_SIZE_U { width, height }).unwrap();
        // }

        unsafe {
            let mut device_resources = self.device_resources.borrow_mut();
            device_resources.d2d_device_context.SetTarget(None);
            device_resources.d3d_context.ClearState();

            device_resources.d2d_target_bitmap = None;
            device_resources.back_buffer = None;

            if let Some(ref mut swap_chain) = device_resources.dxgi_swapchain {
                swap_chain
                    .ResizeBuffers(
                        0,
                        width,
                        height,
                        DXGI_FORMAT_UNKNOWN,
                        DXGI_SWAP_CHAIN_FLAG::default(),
                    )
                    .map_err(RuntimeError::SwapChainResizeFailed)?;
            }
        }

        Ok(())
    }

    // Hit test scrollbar thumbs following the same visibility logic as event dispatch.
    // This prevents scrolling through obscuring elements by checking:
    // 1. If the point is within an innermost element
    // 2. If the point is visible within all scroll ancestors
    // 3. If any element in the ancestry chain has a scrollbar at the hit point
    fn hit_test_scrollbar_thumb(
        &mut self,
        x: f32,
        y: f32,
        only_thumb: bool,
    ) -> Option<ScrollDragState> {
        let point = PointDIP { x, y };

        // First, find the innermost element at this position (respecting scroll clipping)
        let innermost_key = Shell::find_innermost_element_at(&mut self.ui_tree, x, y)?;

        // Collect the ancestry from innermost to root
        let ancestry = Shell::collect_ancestry(&mut self.ui_tree, innermost_key);

        // Check scrollbars from innermost to outermost (matching event dispatch order)
        // This ensures that overlapping elements block scrollbars behind them
        for &key in &ancestry {
            let element = &self.ui_tree.slots[key];

            if element.id.is_none() {
                continue;
            }

            // Check Y-axis scrollbar
            if let Some(geom) = compute_scrollbar_geom(&mut self.shell, element, Axis::Y) {
                let tr = if only_thumb {
                    geom.thumb_rect
                } else {
                    geom.track_rect
                };
                let thumb = geom.thumb_rect;

                // Check if point is within the track/thumb rect
                if point.within(tr) {
                    let grab_offset = y - thumb.y;
                    return Some(ScrollDragState {
                        element_id: element.id.unwrap(),
                        axis: Axis::Y,
                        grab_offset,
                    });
                }
            }

            // Check X-axis scrollbar
            if let Some(geom) = compute_scrollbar_geom(&mut self.shell, element, Axis::X) {
                let tr = if only_thumb {
                    geom.thumb_rect
                } else {
                    geom.track_rect
                };
                let thumb = geom.thumb_rect;

                // Check if point is within the track/thumb rect
                if point.within(tr) {
                    let grab_offset = x - thumb.x;
                    return Some(ScrollDragState {
                        element_id: element.id.unwrap(),
                        axis: Axis::X,
                        grab_offset,
                    });
                }
            }
        }

        None
    }

    fn update_smooth_scroll_animations(&mut self) {
        // Update all smooth scroll animations and apply current positions
        self.smooth_scroll_manager.update_animations();

        // Apply current animated positions to the scroll state manager
        for (&element_id, animation) in self.smooth_scroll_manager.get_active_animations() {
            let current_pos = animation.current_position(std::time::Instant::now());
            self.shell
                .scroll_state_manager
                .set_scroll_position(element_id, current_pos);
        }
    }

    // Process async messages from executor thread
    fn process_async_messages(&mut self, hwnd: HWND) {
        let mut cap = 100;
        while let Ok(message) = self.message_receiver.try_recv() {
            if let Some(task) = (self.update_fn)(&mut self.user_state, message) {
                self.spawn_task(task);
            }

            // Limit processing to 100 messages per frame
            cap -= 1;
            if cap == 0 {
                // Post a message to the window to continue processing later
                unsafe { PostMessageW(Some(hwnd), WM_ASYNC_MESSAGE, WPARAM(0), LPARAM(0)).ok() };
                break;
            }
        }

        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }

        PENDING_MESSAGE_PROCESSING.store(false, Ordering::SeqCst);
    }

    // Spawn a task on the executor thread
    fn spawn_task(&self, task: Task<Message>) {
        let _ = self.task_sender.send(task);
    }
}

fn create_tree_root<State: 'static, Message>(
    state: &State,
    view_fn: ViewFn<State, Message>,
    device_resources: &DeviceResources,
    hook_manager: &mut HookManager<Message>,
) {
    let children = view_fn(state, hook_manager);
    create_tree(
        device_resources,
        hook_manager.ui_tree,
        Element {
            id: Some(w_id!()),
            direction: Direction::ZStack,
            children: vec![Element {
                id: Some(w_id!()),
                direction: Direction::ZStack,
                children: vec![children],
                width: Sizing::grow(),
                height: Sizing::grow(),

                ..Default::default()
            }],

            ..Default::default()
        },
    )
}

type WndProc = dyn Fn(HWND, u32, WPARAM, LPARAM) -> LRESULT + Send + Sync;
static WNDPROC_IMPL: OnceLock<Box<WndProc>> = OnceLock::new();

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    WNDPROC_IMPL.get().unwrap()(hwnd, msg, wparam, lparam)
}

static REPLACE_TITLEBAR: AtomicBool = AtomicBool::new(false);
fn wndproc_impl<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let result = unsafe {
        if REPLACE_TITLEBAR.load(Ordering::Relaxed) {
            let mut l_ret = LRESULT(0);
            let mut skip_normal_handlers =
                DwmDefWindowProc(hwnd, msg, wparam, lparam, &mut l_ret).as_bool();

            match msg {
                WM_NCCALCSIZE if wparam.0 == 1 => {
                    let pncsp = lparam.0 as *mut NCCALCSIZE_PARAMS;

                    (*pncsp).rgrc[0].left +=
                        GetSystemMetrics(SM_CXFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
                    (*pncsp).rgrc[0].right -=
                        GetSystemMetrics(SM_CXFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
                    (*pncsp).rgrc[0].bottom -=
                        GetSystemMetrics(SM_CYFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);

                    return LRESULT(0);
                }
                WM_NCHITTEST if l_ret.0 == 0 => {
                    l_ret = LRESULT(hit_test_nca(hwnd, wparam, lparam) as isize);

                    if l_ret.0 != HTNOWHERE as isize {
                        skip_normal_handlers = true;
                    }
                }
                WM_ERASEBKGND => {
                    let hdc = HDC(wparam.0 as _);
                    let mut rc = RECT::default();
                    GetClientRect(hwnd, &mut rc).unwrap();
                    let brush = CreateSolidBrush(COLORREF(0x000000));
                    FillRect(hdc, &rc, brush);
                    let _ = DeleteObject(brush.into());
                    return LRESULT(1);
                }
                _ => {}
            }

            if skip_normal_handlers {
                return l_ret;
            }
        }

        // println!("msg: {}", msg);

        match msg {
            WM_ACTIVATE => {
                let _ = InvalidateRect(Some(hwnd), None, true);
                LRESULT(0)
            }
            WM_SYSCOMMAND => {
                // Handle system commands (minimize, maximize, close, etc.)
                let command = SystemCommand::from_wparam(wparam.0);

                if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
                    let state = state.deref_mut();
                    if let Some(ref handler) = state.syscommand_handler {
                        match handler(&state.user_state, command) {
                            SystemCommandResponse::Allow => {
                                // Fall through to default handling
                            }
                            SystemCommandResponse::AllowWith(message) => {
                                state.shell.message_sender.send(message);
                                state.shell.pending_messages = true;
                            }
                            SystemCommandResponse::Prevent => {
                                // Application handled the command, prevent default behavior
                                return LRESULT(0);
                            }
                            SystemCommandResponse::PreventWith(message) => {
                                state.shell.message_sender.send(message);
                                state.shell.pending_messages = true;
                                return LRESULT(0);
                            }
                        }
                    }
                }

                // Default handling
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_TRAYICON => {
                // Handle tray icon events
                if let Some(event) = TrayIcon::parse_message(lparam) {
                    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
                        let state = state.deref_mut();
                        if let Some(ref handler) = state.tray_event_handler {
                            if let Some(task) = handler(&state.user_state, event) {
                                state.task_sender.send(task);
                            }
                        }
                    }
                }
                LRESULT(0)
            }
            WM_SHOW_CONTEXT_MENU => {
                // Handle context menu request from async thread
                let request_ptr = wparam.0 as *mut ContextMenuRequest;
                if !request_ptr.is_null() {
                    let request = Box::from_raw(request_ptr);

                    // Show the menu on the UI thread
                    let result =
                        ContextMenu::show_sync_on_ui_thread(&request.items, hwnd, request.position);

                    // Send the result back through the channel
                    let _ = request.sender.send(result);
                }
                LRESULT(0)
            }
            WM_ASYNC_MESSAGE => {
                // Handle async messages from executor thread
                if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
                    let state = state.deref_mut();
                    state.process_async_messages(hwnd);
                }
                LRESULT(0)
            }
            WM_IME_STARTCOMPOSITION => {
                // println!("WM_IME_STARTCOMPOSITION");
                if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
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
                if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
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
                if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
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
                if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
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

                if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
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
                        if let Some(drag) = state.hit_test_scrollbar_thumb(x, y, true) {
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
                if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
                    let state = state.deref_mut();
                    // Current mouse in pixels
                    let xi = (lparam.0 & 0xFFFF) as i16 as i32;
                    let yi = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                    let to_dip = dips_scale(hwnd);
                    let x = (xi as f32) * to_dip;
                    let y = (yi as f32) * to_dip;

                    // Handle scrollbar dragging if active
                    if let Some(drag) = state.scroll_drag {
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
                            let axis = drag.axis;
                            if let Some(geom) = compute_scrollbar_geom(&mut state.shell, el, axis) {
                                let pos_along = match drag.axis {
                                    Axis::Y => y,
                                    Axis::X => x,
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
                                    .shell
                                    .scroll_state_manager
                                    .get_scroll_position(drag.element_id);
                                match drag.axis {
                                    Axis::Y => {
                                        state.shell.scroll_state_manager.set_scroll_position(
                                            drag.element_id,
                                            ScrollPosition {
                                                x: cur.x,
                                                y: new_scroll,
                                            },
                                        );
                                    }
                                    Axis::X => {
                                        state.shell.scroll_state_manager.set_scroll_position(
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
                    } else if let Some(drag) = state.hit_test_scrollbar_thumb(x, y, false) {
                        state
                            .shell
                            .scroll_state_manager
                            .set_active(drag.element_id, drag.axis);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    } else if state.shell.scroll_state_manager.set_inactive() {
                        let _ = InvalidateRect(Some(hwnd), None, false);
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

                if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
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

                    if state.shell.capture_event(0) {
                        // Use the same visibility logic as event dispatch:
                        // 1. Find innermost element at position
                        // 2. Walk up ancestry to find first scrollable element
                        // This prevents scrolling through obscuring elements
                        if let Some(innermost_key) =
                            Shell::find_innermost_element_at(&mut state.ui_tree, x_dip, y_dip)
                        {
                            let ancestry =
                                Shell::collect_ancestry(&mut state.ui_tree, innermost_key);

                            // Walk up the ancestry from innermost to outermost
                            for &key in &ancestry {
                                let element = &state.ui_tree.slots[key];

                                if element.scroll.is_some()
                                    && let Some(element_id) = element.id
                                {
                                    // Check if this element can scroll in the requested direction
                                    if can_scroll_further(
                                        element,
                                        axis,
                                        if wheel_delta > 0.0 {
                                            ScrollDirection::Positive
                                        } else {
                                            ScrollDirection::Negative
                                        },
                                        &state.shell.scroll_state_manager,
                                    ) {
                                        let mut scroll_lines = 3;
                                        let _ = SystemParametersInfoW(
                                            SPI_GETWHEELSCROLLLINES,
                                            0,
                                            Some(&mut scroll_lines as *mut i32 as *mut _),
                                            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
                                        );

                                        let wheel_delta =
                                            wheel_delta * LINE_HEIGHT as f32 * scroll_lines as f32;

                                        let (delta_x, delta_y) = if axis == Axis::Y {
                                            (0.0, wheel_delta)
                                        } else {
                                            (wheel_delta, 0.0)
                                        };

                                        // Get current scroll position (either from active animation or actual position)
                                        let current_pos = state
                                            .shell
                                            .scroll_state_manager
                                            .get_scroll_position(element_id);
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

                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                // println!("WM_LBUTTONUP");
                if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
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
                if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
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
                if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
                    let state = state.deref_mut();
                    let vk = wparam.0 as i32;

                    let modifiers = get_modifiers();
                    if let Ok(vk) = VKey::try_from(vk) {
                        state.shell.dispatch_event(
                            hwnd,
                            &mut state.ui_tree,
                            Event::KeyDown { key: vk, modifiers },
                        );
                    }

                    // if handled {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                    // }
                }

                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_KEYUP => {
                // println!("WM_KEYUP");
                if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
                    let state = state.deref_mut();
                    let vk = wparam.0 as i32;

                    let modifiers = get_modifiers();
                    if let Ok(vk) = VKey::try_from(vk) {
                        state.shell.dispatch_event(
                            hwnd,
                            &mut state.ui_tree,
                            Event::KeyUp { key: vk, modifiers },
                        );
                    }

                    // if handled {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                    // }
                }

                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_SIZE => {
                // println!("WM_SIZE");
                if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
                    let state = state.deref_mut();
                    let width = (lparam.0 & 0xFFFF) as u32;
                    let height = ((lparam.0 >> 16) & 0xFFFF) as u32;
                    if let Err(e) = state.on_resize(width, height) {
                        error!("Failed to resize: {e}");
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
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                )
                .ok();

                if REPLACE_TITLEBAR.load(Ordering::Relaxed) {
                    let margins = MARGINS {
                        cxLeftWidth: -1,
                        cxRightWidth: -1,
                        cyTopHeight: -1,
                        cyBottomHeight: -1,
                        // cxLeftWidth: 0,
                        // cxRightWidth: 0,
                        // cyBottomHeight: 0,
                        // cyTopHeight: compute_standard_caption_height_for_window(hwnd).unwrap(),
                    };
                    DwmExtendFrameIntoClientArea(hwnd, &margins).ok();
                }

                if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
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
                    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
                        let state = state.deref_mut();
                        // Get mouse in client pixels and convert to DIPs
                        let mut pt = POINT { x: 0, y: 0 };
                        let _ = GetCursorPos(&mut pt);
                        let _ = ScreenToClient(hwnd, &mut pt);
                        let to_dip = dips_scale(hwnd);
                        let x_dip = (pt.x as f32) * to_dip;
                        let y_dip = (pt.y as f32) * to_dip;
                        let point = PointDIP { x: x_dip, y: y_dip };

                        let mut cursor = None;

                        if let Some(target_key) =
                            Shell::find_innermost_element_at(&mut state.ui_tree, x_dip, y_dip)
                        {
                            let ancestry = Shell::collect_ancestry(&mut state.ui_tree, target_key);

                            for element in ancestry {
                                let bounds = state.ui_tree.slots[element].bounds();

                                if let Some(id) = state.ui_tree.slots[element].id {
                                    if point.within(bounds.border_box)
                                        && Shell::is_point_visible_in_scroll_ancestors(
                                            &mut state.ui_tree,
                                            element,
                                            point,
                                        )
                                    {
                                        if let Some(instance) = state.ui_tree.widget_state.get(&id)
                                        {
                                            if let Some(ref widget) =
                                                state.ui_tree.slots[element].content
                                            {
                                                cursor = widget.cursor(
                                                    &state.ui_tree.arenas,
                                                    instance,
                                                    point,
                                                    bounds,
                                                );
                                            }
                                        }
                                    }
                                }

                                if cursor.is_some() {
                                    break;
                                }
                            }

                            if let Some(cursor) = cursor {
                                cursor.set();
                                return LRESULT(1);
                            }
                        }
                    }
                }

                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_GETMINMAXINFO => {
                if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
                    let state = state.deref_mut();

                    let min_max_info = lparam.0 as *mut MINMAXINFO;

                    // Calculate the min size from the layout
                    let root_node = state.ui_tree.slots[state.ui_tree.root].children[0];
                    let sentinel_node = &state.ui_tree.slots[root_node];

                    let dpi_scale = dips_scale(hwnd);

                    // Set the minimum size of the window
                    (*min_max_info).ptMinTrackSize.x = (sentinel_node.min_width / dpi_scale).floor()
                        as i32
                        + (GetSystemMetrics(SM_CXFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER)) * 2;
                    (*min_max_info).ptMinTrackSize.y = (sentinel_node.min_height / dpi_scale)
                        .floor() as i32
                        + GetSystemMetrics(SM_CYFRAME)
                        + GetSystemMetrics(SM_CXPADDEDBORDER);
                }
                LRESULT(0)
            }
            WM_PAINT => {
                // println!("WM_PAINT");
                let commands = if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd)
                {
                    let state = state.deref_mut();
                    state.shell.replace_redraw_request(RedrawRequest::Wait);

                    let now = Instant::now();
                    state
                        .shell
                        .dispatch_event(hwnd, &mut state.ui_tree, Event::Redraw { now });

                    match state.on_paint(hwnd) {
                        Ok(commands) => {
                            let device_resources = state.device_resources.clone();
                            let redraw_request = state.shell.redraw_request;

                            Some((commands, device_resources, redraw_request))
                        }
                        Err(e) => {
                            error!("Failed to paint: {e}");
                            None
                        }
                    }
                } else {
                    None
                };

                if let Some((commands, device_resources, redraw_request)) = commands {
                    let mut ps = PAINTSTRUCT::default();
                    let _ = BeginPaint(hwnd, &mut ps);
                    // let device_width = ps.rcPaint.right.try_into().unwrap();
                    // let device_height = ps.rcPaint.bottom.try_into().unwrap();
                    // println!(
                    //     "device_width: {}, device_height: {}",
                    //     device_width, device_height
                    // );
                    let rc = client_rect(hwnd).unwrap();
                    let device_width = rc.right.try_into().unwrap();
                    let device_height = rc.bottom.try_into().unwrap();

                    let mut device_resources = device_resources.borrow_mut();
                    device_resources
                        .create_device_resources(hwnd, device_width, device_height)
                        .expect("Failed to create device resources");

                    if let (rt, Some(brush)) = (
                        &device_resources.d2d_device_context,
                        &device_resources.solid_brush,
                    ) {
                        rt.BeginDraw();
                        let white = D2D1_COLOR_F {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        };
                        rt.Clear(Some(&white));

                        let bounds = RectDIP::from(hwnd, rc);

                        let renderer = Renderer::new(
                            &device_resources.d2d_factory,
                            rt,
                            brush,
                            &device_resources.shadow_cache,
                        );

                        // Start frame for cache management
                        renderer.start_frame();

                        CommandExecutor::execute_commands_with_bounds(
                            &renderer,
                            &commands,
                            Some(bounds),
                        )
                        .ok();

                        // Evict unused cache entries before ending the frame
                        renderer.evict_unused_cache_entries();

                        let end = rt.EndDraw(None, None);
                        if let Err(e) = end {
                            if e.code() == D2DERR_RECREATE_TARGET {
                                warn!("Recreating D2D target");
                                device_resources.discard_device_resources();
                                device_resources
                                    .create_device_resources(hwnd, device_width, device_height)
                                    .expect("Failed to recreate device resources");
                            }
                        }
                    }

                    // // Present using DirectComposition if available, otherwise fallback to direct swap chain present
                    // if let (dcomp_device, Some(ref dcomp_visual), Some(ref swap_chain)) = (
                    //     &device_resources.dcomp_device,
                    //     &device_resources.dcomp_visual,
                    //     &device_resources.dxgi_swapchain,
                    // ) {
                    //     // Create composition surface from the swap chain for DirectComposition
                    //     if let Ok(comp_surface) =
                    //         dcomp_device.CreateSurfaceFromSwapChain(swap_chain)
                    //     {
                    //         // Set the composition surface as the visual's content
                    //         let _ = dcomp_visual.SetContent(&comp_surface);
                    //         // Commit the composition to make changes visible
                    //         let _ = dcomp_device.Commit();
                    //     }
                    if let Some(ref swap_chain) = device_resources.dxgi_swapchain {
                        // Fallback to traditional swap chain present if DirectComposition not available
                        let _ = swap_chain.Present(0, DXGI_PRESENT::default());

                        // device_resources
                        //     .dcomp_visual
                        //     .as_mut()
                        //     .unwrap()
                        //     .SetContent(swap_chain);
                        device_resources
                            .dcomp_device
                            .Commit()
                            .expect("Failed to commit DirectComposition");
                    }

                    let _ = EndPaint(hwnd, &ps);

                    if matches!(redraw_request, RedrawRequest::Immediate) {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
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
                    drop(Box::from_raw(ptr as *mut WinUserData<State, Message>));
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                WAM::PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    };

    let (deferred_controls, has_pending_messages) =
        if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
            let state = state.deref_mut();
            state.shell.dispatch_operations(&mut state.ui_tree);

            // TODO: Maybe move this into a deferred control
            // Schedule next frame if we have active animations
            if state.smooth_scroll_manager.has_any_active_animations() {
                state.shell.request_redraw(hwnd, RedrawRequest::Immediate);
            }

            if state.shell.redraw_request == RedrawRequest::Immediate {
                let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            }

            let pending_messages = state.shell.pending_messages;
            state.shell.pending_messages = false;

            (state.shell.drain_deferred_controls(), pending_messages)
        } else {
            (None, false)
        };

    if let Some(deferred_controls) = deferred_controls {
        for control in deferred_controls {
            match control {
                DeferredControl::StartDrag { data, src_id } => {
                    let DragData::Text(text) = data;

                    if let Ok(effect) = start_text_drag(&text, true) {
                        if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
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
                                x: (position.x / to_dip).round() as i32,
                                y: (position.y / to_dip).round() as i32,
                            },
                            rcArea: RECT::default(),
                            dwIndex: 0,
                        };
                        let _ = ImmSetCandidateWindow(himc, &cf);

                        let _ = ImmReleaseContext(hwnd, himc);
                    }
                },

                DeferredControl::SetClipboardText(text) => {
                    let _ = clipboard::set_clipboard_text(hwnd, &text);
                }
            }
        }
    }

    if has_pending_messages {
        // If the UI thread is not processing messages, notify it
        if !PENDING_MESSAGE_PROCESSING.swap(true, Ordering::SeqCst) {
            unsafe {
                PostMessageW(Some(hwnd), WM_ASYNC_MESSAGE, WPARAM(0), LPARAM(0)).ok();
            }
        }
    }

    result
}

// // Hit test the frame for resizing and moving.
// LRESULT HitTestNCA(HWND hWnd, WPARAM wParam, LPARAM lParam)
// {
//     // Get the point coordinates for the hit test.
//     POINT ptMouse = { GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam)};

//     // Get the window rectangle.
//     RECT rcWindow;
//     GetWindowRect(hWnd, &rcWindow);

//     // Get the frame rectangle, adjusted for the style without a caption.
//     RECT rcFrame = { 0 };
//     AdjustWindowRectEx(&rcFrame, WS_OVERLAPPEDWINDOW & ~WS_CAPTION, FALSE, NULL);

//     // Determine if the hit test is for resizing. Default middle (1,1).
//     USHORT uRow = 1;
//     USHORT uCol = 1;
//     bool fOnResizeBorder = false;

//     // Determine if the point is at the top or bottom of the window.
//     if (ptMouse.y >= rcWindow.top && ptMouse.y < rcWindow.top + TOPEXTENDWIDTH)
//     {
//         fOnResizeBorder = (ptMouse.y < (rcWindow.top - rcFrame.top));
//         uRow = 0;
//     }
//     else if (ptMouse.y < rcWindow.bottom && ptMouse.y >= rcWindow.bottom - BOTTOMEXTENDWIDTH)
//     {
//         uRow = 2;
//     }

//     // Determine if the point is at the left or right of the window.
//     if (ptMouse.x >= rcWindow.left && ptMouse.x < rcWindow.left + LEFTEXTENDWIDTH)
//     {
//         uCol = 0; // left side
//     }
//     else if (ptMouse.x < rcWindow.right && ptMouse.x >= rcWindow.right - RIGHTEXTENDWIDTH)
//     {
//         uCol = 2; // right side
//     }

//     // Hit test (HTTOPLEFT, ... HTBOTTOMRIGHT)
//     LRESULT hitTests[3][3] =
//     {
//         { HTTOPLEFT,    fOnResizeBorder ? HTTOP : HTCAPTION,    HTTOPRIGHT },
//         { HTLEFT,       HTNOWHERE,     HTRIGHT },
//         { HTBOTTOMLEFT, HTBOTTOM, HTBOTTOMRIGHT },
//     };

//     return hitTests[uRow][uCol];
// }

fn hit_test_nca(hwnd: HWND, _wparam: WPARAM, lparam: LPARAM) -> u32 {
    // let pt_mouse = POINT {
    //     x: GET_X_LPARAM(lparam),
    //     y: GET_Y_LPARAM(lparam),
    // };
    let x_px = (lparam.0 & 0xFFFF) as i16 as i32;
    let y_px = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

    let mut rc_window = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rc_window).unwrap() };

    let mut rc_frame = RECT::default();
    unsafe {
        AdjustWindowRectEx(
            &mut rc_frame,
            WS_OVERLAPPEDWINDOW & !WS_CAPTION,
            false,
            WINDOW_EX_STYLE::default() | WS_EX_NOREDIRECTIONBITMAP,
        )
        .unwrap()
    };

    let mut u_row = 1;
    let mut u_col = 1;
    let mut f_on_resize_border = false;

    let dpi_scale = dips_scale(hwnd);

    let topextendwidth: i32 =
        compute_standard_caption_height_for_window(hwnd).unwrap_or((28.0 / dpi_scale) as i32);
    let bottomextendwidth: i32 = (10.0 / dpi_scale) as i32;
    let leftextendwidth: i32 = (10.0 / dpi_scale) as i32;
    let rightextendwidth: i32 = (10.0 / dpi_scale) as i32;

    if y_px >= rc_window.top && y_px < rc_window.top + topextendwidth {
        f_on_resize_border = y_px < (rc_window.top - rc_frame.top);
        u_row = 0;
    } else if y_px < rc_window.bottom && y_px >= rc_window.bottom - bottomextendwidth {
        u_row = 2;
    }

    if x_px >= rc_window.left && x_px < rc_window.left + leftextendwidth {
        u_col = 0;
    } else if x_px < rc_window.right && x_px >= rc_window.right - rightextendwidth {
        u_col = 2;
    }

    let hit_tests = [
        [
            HTTOPLEFT,
            if f_on_resize_border { HTTOP } else { HTCAPTION },
            HTTOPRIGHT,
        ],
        [HTLEFT, HTNOWHERE, HTRIGHT],
        [HTBOTTOMLEFT, HTBOTTOM, HTBOTTOMRIGHT],
    ];

    hit_tests[u_row][u_col]
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
fn compute_standard_caption_height_for_window(window_handle: HWND) -> Result<i32> {
    let accounting_for_borders = -1;
    let theme = unsafe { OpenThemeData(Some(window_handle), w!("WINDOW")) };
    let dpi = unsafe { GetDpiForWindow(window_handle) };
    let caption_size =
        unsafe { GetThemePartSize(theme, None, WP_CAPTION.0, CS_ACTIVE.0, None, TS_TRUE)? };
    unsafe { CloseThemeData(theme)? };

    let height = (caption_size.cy as f32 * dpi as f32) / 96.0;
    Ok((height as i32) + accounting_for_borders)
}

#[derive(Debug, Default)]
pub enum Backdrop {
    None,
    #[default]
    Mica,
    MicaAlt,
    Acrylic,
}

pub struct Application<
    B: Fn(&State) -> Option<crate::runtime::task::Task<Message>> + 'static,
    State: 'static,
    Message: 'static + Send,
> {
    view_fn: ViewFn<State, Message>,
    update_fn: UpdateFn<State, Message>,
    event_mapper_fn: EventMapperFn<Message>,
    boot_fn: B,
    state: State,

    title: String,
    width: u32,
    height: u32,

    backdrop: Backdrop,
    replace_titlebar: bool,

    tray_config: Option<TrayIconConfig>,
    tray_event_handler: Option<Box<dyn Fn(&State, TrayEvent) -> Option<Task<Message>>>>,

    syscommand_handler:
        Option<Box<dyn Fn(&State, SystemCommand) -> SystemCommandResponse<Message>>>,
}

impl<
    B: Fn(&State) -> Option<crate::runtime::task::Task<Message>> + 'static,
    State: 'static,
    Message: 'static + Send + Clone,
> Application<B, State, Message>
{
    pub fn new(
        state: State,
        view_fn: ViewFn<State, Message>,
        update_fn: UpdateFn<State, Message>,
        boot_fn: B,
    ) -> Self {
        Self {
            view_fn,
            update_fn,
            event_mapper_fn: |_, _| None,
            boot_fn,
            state,

            title: "Raxis".to_string(),
            width: 800,
            height: 600,

            backdrop: Backdrop::default(),
            replace_titlebar: false,

            tray_config: None,
            tray_event_handler: None,

            syscommand_handler: None,
        }
    }

    pub fn with_title(self, title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..self
        }
    }

    pub fn with_window_size(self, width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            ..self
        }
    }

    pub fn with_backdrop(self, backdrop: Backdrop) -> Self {
        Self { backdrop, ..self }
    }

    pub fn with_event_mapper(self, event_mapper_fn: EventMapperFn<Message>) -> Self {
        Self {
            event_mapper_fn,
            ..self
        }
    }

    pub fn replace_titlebar(self) -> Self {
        Self {
            replace_titlebar: true,
            ..self
        }
    }

    pub fn with_tray_icon(self, config: TrayIconConfig) -> Self {
        Self {
            tray_config: Some(config),
            ..self
        }
    }

    pub fn with_tray_event_handler(
        self,
        handler: impl Fn(&State, TrayEvent) -> Option<Task<Message>> + 'static,
    ) -> Self {
        Self {
            tray_event_handler: Some(Box::new(handler)),
            ..self
        }
    }

    pub fn with_syscommand_handler(
        self,
        handler: impl Fn(&State, SystemCommand) -> SystemCommandResponse<Message> + 'static,
    ) -> Self {
        Self {
            syscommand_handler: Some(Box::new(handler)),
            ..self
        }
    }

    pub fn run(self) -> Result<()> {
        let Application {
            view_fn,
            update_fn,
            event_mapper_fn,
            boot_fn,
            state,

            title,
            width,
            height,

            backdrop,
            replace_titlebar,

            tray_config,
            tray_event_handler,

            syscommand_handler,
        } = self;

        WNDPROC_IMPL
            .set(Box::new(wndproc_impl::<State, Message>))
            .map_err(|_| "WNDPROC_IMPL already initialized")
            .unwrap();

        unsafe {
            // Opt-in to Per-Monitor V2 DPI awareness for crisp rendering on high-DPI displays
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

            let _ = CoInitialize(None);

            // Initialize OLE for drag-and-drop
            let _ = OleInitialize(None);

            let hinstance = GetModuleHandleW(None).unwrap();
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

            // Create window first without user data
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default() | WS_EX_NOREDIRECTIONBITMAP,
                class_name,
                PCWSTR(
                    title
                        .encode_utf16()
                        .chain(std::iter::once(0))
                        .collect::<Vec<u16>>()
                        .as_ptr(),
                ),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                width as i32,  // Will be adjusted after DPI calculation
                height as i32, // Will be adjusted after DPI calculation
                None,
                None,
                Some(hinstance.into()),
                None, // No user data yet
            )
            .map_err(RuntimeError::WindowCreationFailed)?;

            // Dark mode
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &BOOL(1) as *const _ as _,
                size_of::<BOOL>() as _,
            )
            .ok();

            // DwmSetWindowAttribute(
            //     hwnd,
            //     DWMWA_CAPTION_COLOR,
            //     &COLORREF(0x000000) as *const _ as _,
            //     size_of::<COLORREF>() as _,
            // )
            // .ok();

            // For Mica effect (Windows 11 only)
            let backdrop_result = DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &match backdrop {
                    Backdrop::None => DWMSBT_NONE,
                    Backdrop::Mica => DWMSBT_MAINWINDOW,
                    Backdrop::MicaAlt => DWMSBT_TABBEDWINDOW,
                    Backdrop::Acrylic => DWMSBT_TRANSIENTWINDOW,
                } as *const _ as _,
                size_of::<DWM_SYSTEMBACKDROP_TYPE>() as _,
            );

            // Check if backdrop setting succeeded
            let backdrop_supported = backdrop_result.is_ok();

            if backdrop_supported && !matches!(backdrop, Backdrop::None) {
                let bb = DWM_BLURBEHIND {
                    dwFlags: DWM_BB_ENABLE,
                    fEnable: true.into(),
                    ..Default::default()
                };
                DwmEnableBlurBehindWindow(hwnd, &bb).ok();
            }

            if replace_titlebar {
                REPLACE_TITLEBAR.store(true, Ordering::Relaxed);

                let margins = MARGINS {
                    cxLeftWidth: -1,
                    cxRightWidth: -1,
                    cyTopHeight: -1,
                    cyBottomHeight: -1,
                    // cxLeftWidth: 0,
                    // cxRightWidth: 0,
                    // cyBottomHeight: 0,
                    // cyTopHeight: compute_standard_caption_height_for_window(hwnd)?,
                };
                DwmExtendFrameIntoClientArea(hwnd, &margins).ok();
            }

            // Now create the app handle with the hwnd
            let mut app = ApplicationHandle::new(
                view_fn,
                update_fn,
                event_mapper_fn,
                boot_fn,
                state,
                hwnd,
                tray_config,
                tray_event_handler,
                syscommand_handler,
            )?;

            let dips = dips_scale(hwnd);

            // Register OLE drop target
            let dt: IDropTarget = DropTarget::new(hwnd, |hwnd, event| {
                // Dispatch drag/drop events to the Shell
                if let Some(mut app_state) = state_mut_from_hwnd::<State, Message>(hwnd) {
                    let app_state = app_state.deref_mut();
                    if let Some(result) = app_state.shell.dispatch_drag_event(
                        &mut app_state.ui_tree,
                        &event,
                        match &event {
                            DragEvent::DragEnter { drag_info }
                            | DragEvent::DragOver { drag_info }
                            | DragEvent::Drop { drag_info } => drag_info.position,
                            DragEvent::DragLeave => PointDIP { x: 0.0, y: 0.0 }, // Position not needed for DragLeave
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
            app.drop_target = Some(dt);

            let app = Mutex::new(app);
            let boxed = Box::new(app);
            let ptr = Box::into_raw(boxed) as isize;

            // Set the window's user data to point to our Application
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr);

            // Resize window based on DPI
            SetWindowPos(
                hwnd,
                None,
                0,
                0,
                (self.width as f32 / dips) as i32,
                (self.height as f32 / dips) as i32,
                SWP_NOZORDER | SWP_NOMOVE | SWP_NOACTIVATE,
            )
            .ok();

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
}
