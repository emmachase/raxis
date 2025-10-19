use crate::dips_scale;
use crate::gfx::draw_commands::DrawCommandList;
use crate::gfx::{RectDIP, command_recorder::CommandRecorder};
use crate::layout::model::{
    Color, Direction, Element, ScrollbarStyle, Sizing, StrokeLineCap, create_tree,
};
use crate::layout::{self, OwnedUITree};
use crate::runtime::device::DeviceResources;
use crate::runtime::focus::FocusManager;
use crate::runtime::font_manager;
use crate::runtime::input::{MiddleMouseScrollState, MouseState, ScrollbarDragState};
use crate::runtime::scroll::{ScrollPosition, ScrollStateManager};
use crate::runtime::smooth_scroll::SmoothScrollManager;
use crate::runtime::syscommand::{SystemCommand, SystemCommandResponse};
use crate::runtime::task::Task;
use crate::runtime::tray::{TrayEvent, TrayIcon, TrayIconConfig};
use crate::widgets::Event;
use crate::{HookManager, RedrawRequest, Shell, UpdateFn, ViewFn, w_id};
use log::error;
use raxis_core::{self as raxis, svg};
use raxis_proc_macro::svg_path;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Instant;
use thiserror::Error;
use windows::Win32::Foundation::{HMODULE, HWND, POINT};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DEBUG_LEVEL_NONE, D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_FACTORY_OPTIONS,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1CreateFactory, ID2D1Factory7, ID2D1PathGeometry,
};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_9_1, D3D_FEATURE_LEVEL_9_2, D3D_FEATURE_LEVEL_9_3,
    D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::DirectComposition::{DCompositionCreateDevice2, IDCompositionDevice};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWriteCreateFactory, IDWriteFactory6,
};
use windows::Win32::Graphics::Dxgi::{IDXGIDevice4, IDXGIFactory7};
use windows::Win32::System::Ole::IDropTarget;
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows_core::{Error as WinError, Interface};

pub static PENDING_MESSAGE_PROCESSING: AtomicBool = AtomicBool::new(false);

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

    #[error("Device error: {0}")]
    DeviceError(#[from] crate::runtime::device::DeviceError),
}

pub type Result<T> = std::result::Result<T, RuntimeError>;

/// Main application state container managing the UI tree, rendering, input, and async tasks
pub struct ApplicationHandle<State, Message> {
    pub(crate) device_resources: Rc<RefCell<DeviceResources>>,

    pub(crate) clock: f64,
    pub(crate) last_frame_time: Instant,
    pub(crate) view_fn: ViewFn<State, Message>,
    pub(crate) update_fn: UpdateFn<State, Message>,
    pub(crate) _event_mapper_fn: crate::EventMapperFn<Message>,
    pub(crate) user_state: State,

    pub(crate) ui_tree: OwnedUITree<Message>,
    pub(crate) shell: Shell<Message>,
    pub(crate) smooth_scroll_manager: SmoothScrollManager,

    // Keep the window's OLE drop target alive for the lifetime of the window
    pub(crate) drop_target: Option<IDropTarget>,

    // For combining UTF-16 surrogate pairs from WM_CHAR
    pub(crate) pending_high_surrogate: Option<u16>,

    // Mouse state (click tracking, etc.)
    pub(crate) mouse_state: MouseState,

    // Active scrollbar dragging state (if any)
    pub(crate) scroll_drag: Option<ScrollbarDragState>,

    // Middle mouse scroll state (if any)
    pub(crate) middle_mouse_scroll: Option<MiddleMouseScrollState>,

    // Cached geometries for middle mouse scroll indicators
    pub(crate) scroll_icon_all: Option<ID2D1PathGeometry>,
    pub(crate) scroll_icon_horizontal: Option<ID2D1PathGeometry>,
    pub(crate) scroll_icon_vertical: Option<ID2D1PathGeometry>,

    // Async task executor
    pub(crate) task_sender: mpsc::Sender<Task<Message>>,
    pub(crate) message_receiver: mpsc::Receiver<Message>,

    // System tray icon
    pub(crate) _tray_icon: Option<TrayIcon>,
    pub(crate) tray_event_handler: Option<Box<dyn Fn(&State, TrayEvent) -> Option<Task<Message>>>>,

    // System command handler
    pub(crate) syscommand_handler:
        Option<Box<dyn Fn(&State, SystemCommand) -> SystemCommandResponse<Message>>>,
}

impl<State: 'static, Message: 'static + Send + Clone> ApplicationHandle<State, Message> {
    pub fn new(
        view_fn: ViewFn<State, Message>,
        update_fn: UpdateFn<State, Message>,
        event_mapper_fn: crate::EventMapperFn<Message>,
        boot_fn: impl Fn(&State) -> Option<Task<Message>>,
        user_state: State,
        hwnd: HWND,
        tray_config: Option<TrayIconConfig>,
        tray_event_handler: Option<Box<dyn Fn(&State, TrayEvent) -> Option<Task<Message>>>>,
        syscommand_handler: Option<
            Box<dyn Fn(&State, SystemCommand) -> SystemCommandResponse<Message>>,
        >,
        scrollbar_style: ScrollbarStyle,
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
            let d3d_device: ID3D11Device = d3d_device.expect("Failed to create D3D device");
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
            let dxgi_factory: IDXGIFactory7 = dxgi_adapter
                .GetParent()
                .map_err(RuntimeError::DxgiAdapterCreationFailed)?;

            // Direct Composition
            let dcomp_device: IDCompositionDevice = DCompositionCreateDevice2(&d2d_device)
                .map_err(RuntimeError::DcompDeviceCreationFailed)?;
            let dcomp_target = dcomp_device
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
                dcomp_device,
                dcomp_target,
                dcomp_visual: None,
                shadow_cache: RefCell::new(crate::widgets::renderer::ShadowCache::default()),
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
                scrollbar_style,
            );

            // Spawn executor thread with selected async runtime
            {
                let message_sender = message_sender.clone();
                let hwnd = crate::runtime::UncheckedHWND(hwnd);
                std::thread::spawn(move || {
                    crate::runtime::task::run_task_executor(task_receiver, message_sender, hwnd);
                });
            }

            // Create and add tray icon if configured
            let mut tray_icon = tray_config.map(|config| TrayIcon::new(hwnd, config));
            if let Some(ref mut tray) = tray_icon {
                let _ = tray.add(); // Ignore errors during initialization
            }

            // Create cached geometries for middle mouse scroll icons
            let scroll_icon_all = svg![
                svg_path!("M12 2v20"),
                svg_path!("m15 19-3 3-3-3"),
                svg_path!("m19 9 3 3-3 3"),
                svg_path!("M2 12h20"),
                svg_path!("m5 9-3 3 3 3"),
                svg_path!("m9 5 3-3 3 3"),
            ]
            .create_geometry(&device_resources.d2d_factory)
            .ok();

            let scroll_icon_horizontal = svg![
                svg_path!("m18 8 4 4-4 4"),
                svg_path!("M2 12h20"),
                svg_path!("m6 8-4 4 4 4"),
            ]
            .create_geometry(&device_resources.d2d_factory)
            .ok();

            let scroll_icon_vertical = svg![
                svg_path!("M12 2v20"),
                svg_path!("m8 18 4 4 4-4"),
                svg_path!("m8 6 4-4 4 4"),
            ]
            .create_geometry(&device_resources.d2d_factory)
            .ok();

            Ok(Self {
                device_resources: Rc::new(RefCell::new(device_resources)),
                clock: 0.0,
                last_frame_time: Instant::now(),
                ui_tree,
                view_fn,
                update_fn,
                _event_mapper_fn: event_mapper_fn,
                user_state,
                shell,
                smooth_scroll_manager: SmoothScrollManager::new(),
                drop_target: None,
                pending_high_surrogate: None,
                mouse_state: MouseState::default(),
                scroll_drag: None,
                middle_mouse_scroll: None,
                scroll_icon_all,
                scroll_icon_horizontal,
                scroll_icon_vertical,
                task_sender,
                message_receiver,
                _tray_icon: tray_icon,
                tray_event_handler,
                syscommand_handler,
            })
        }
    }

    pub fn on_paint(&mut self, hwnd: HWND) -> Result<DrawCommandList> {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame_time).as_secs_f64();
        self.last_frame_time = now;

        let window_active = unsafe { GetForegroundWindow() == hwnd };

        // Update middle mouse scrolling first
        let middle_mouse_scrolling = self.update_middle_mouse_scroll(dt);

        // Update smooth scroll animations and apply positions to scroll state manager
        let smooth_scrolling = self.update_smooth_scroll_animations();

        // If any scrolling happened, emit a mouse move event to update hover states
        if middle_mouse_scrolling || smooth_scrolling {
            // Get current mouse position in screen coordinates
            let mut pt = POINT { x: 0, y: 0 };
            unsafe { GetCursorPos(&mut pt).ok() };
            
            // Convert to client coordinates
            unsafe { ScreenToClient(hwnd, &mut pt).ok() };
            
            // Convert to DIPs
            let to_dip = dips_scale(hwnd);
            let x_dip = (pt.x as f32) * to_dip;
            let y_dip = (pt.y as f32) * to_dip;
            
            // Dispatch mouse move event
            self.shell.dispatch_event(
                hwnd,
                &mut self.ui_tree,
                Event::MouseMove { x: x_dip, y: y_dip },
            );
        }

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

            let rc = crate::runtime::client_rect(hwnd).unwrap();
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
        let mut commands = layout::paint(&mut self.shell, &mut self.ui_tree, root);

        // Draw middle mouse scroll indicator if active
        if let Some(middle_scroll) = self.middle_mouse_scroll {
            let mut recorder = CommandRecorder::new();

            // Create a 4-directional arrow icon (cross with arrows pointing in all directions)
            const ICON_SIZE: f32 = 32.0;
            let icon_rect = RectDIP {
                x: middle_scroll.origin_x - ICON_SIZE / 2.0,
                y: middle_scroll.origin_y - ICON_SIZE / 2.0,
                width: ICON_SIZE,
                height: ICON_SIZE,
            };

            // Pick icon based on scroll capabilities on x and y axis
            let scroll_config = if let Some(element_key) =
                Shell::find_key_by_id(&mut self.ui_tree, middle_scroll.element_id)
                && let ref element = self.ui_tree.slots[element_key]
                && let Some(scroll_config) = element.scroll.as_ref()
            {
                (
                    scroll_config.horizontal.is_some()
                        && element.computed_content_width > element.computed_width,
                    scroll_config.vertical.is_some()
                        && element.computed_content_height > element.computed_height,
                )
            } else {
                (false, false)
            };

            let geometry = match scroll_config {
                (true, true) => self.scroll_icon_all.as_ref(),
                (true, false) => self.scroll_icon_horizontal.as_ref(),
                (false, true) => self.scroll_icon_vertical.as_ref(),
                _ => None,
            };

            // Create geometry and draw
            // ViewBox is 24x24, icon is 32x32, so scale accordingly
            const VIEWBOX_SIZE: f32 = 24.0;
            let scale = ICON_SIZE / VIEWBOX_SIZE;

            if let Some(geometry) = geometry {
                recorder.stroke_path_geometry(
                    &icon_rect,
                    geometry,
                    Color::BLACK,
                    2.5,
                    scale,
                    scale,
                    Some(StrokeLineCap::Round),
                    None,
                );
                recorder.stroke_path_geometry(
                    &icon_rect,
                    geometry,
                    Color::WHITE,
                    1.5,
                    scale,
                    scale,
                    Some(StrokeLineCap::Round),
                    None,
                );
            }

            // Append the icon commands to the main command list
            let icon_commands = recorder.take_commands();
            commands.commands.extend(icon_commands.commands);
        }

        self.clock += dt;

        // Request continuous redraws if middle mouse scrolling is active
        if self.middle_mouse_scroll.is_some() {
            self.shell
                .request_redraw(hwnd, crate::RedrawRequest::Immediate);
        }

        Ok(commands)
    }

    pub fn on_resize(&mut self, width: u32, height: u32) -> Result<()> {
        self.device_resources
            .borrow_mut()
            .resize_swap_chain(width, height)?;
        Ok(())
    }

    pub fn hit_test_scrollbar_thumb(
        &mut self,
        x: f32,
        y: f32,
        only_thumb: bool,
    ) -> Option<ScrollbarDragState> {
        crate::runtime::input::scrollbar::hit_test_scrollbar_thumb(
            &mut self.ui_tree,
            &mut self.shell,
            x,
            y,
            only_thumb,
        )
    }

    fn update_middle_mouse_scroll(&mut self, dt: f64) -> bool {
        if let Some(middle_scroll) = self.middle_mouse_scroll {
            let (velocity_x, velocity_y) = middle_scroll.calculate_velocity(dt);

            // Check if there's actual scrolling happening (non-zero velocity)
            const EPSILON: f32 = 0.01;
            let is_scrolling = velocity_x.abs() > EPSILON || velocity_y.abs() > EPSILON;

            if is_scrolling {
                // Get current scroll position and apply velocity
                let current_pos = self
                    .shell
                    .scroll_state_manager
                    .get_scroll_position(middle_scroll.element_id);

                let new_pos = ScrollPosition {
                    x: current_pos.x + velocity_x,
                    y: current_pos.y + velocity_y,
                };

                // Update scroll position directly
                self.shell
                    .scroll_state_manager
                    .set_scroll_position(middle_scroll.element_id, new_pos);

                // Stop any smooth scroll animations for this element
                self.smooth_scroll_manager
                    .stop_animation(middle_scroll.element_id);
            }

            is_scrolling
        } else {
            false
        }
    }

    fn update_smooth_scroll_animations(&mut self) -> bool {
        self.smooth_scroll_manager.update_animations();

        let mut any_scrolling = false;

        // Apply current animated positions to the scroll state manager
        for (&element_id, animation) in self.smooth_scroll_manager.get_active_animations() {
            let current_pos = animation.current_position(std::time::Instant::now());
            let prev_pos = self.shell.scroll_state_manager.get_scroll_position(element_id);
            
            // Check if position actually changed (with small epsilon to avoid floating point issues)
            const EPSILON: f32 = 0.01;
            if (current_pos.x - prev_pos.x).abs() > EPSILON 
                || (current_pos.y - prev_pos.y).abs() > EPSILON {
                any_scrolling = true;
            }
            
            self.shell
                .scroll_state_manager
                .set_scroll_position(element_id, current_pos);
        }

        any_scrolling
    }

    // Process async messages from executor thread
    pub fn process_async_messages(&mut self, hwnd: HWND) {
        let mut cap = 100;
        while let Ok(message) = self.message_receiver.try_recv() {
            if let Some(task) = (self.update_fn)(&mut self.user_state, message) {
                self.spawn_task(task);
            }

            // Limit processing to 100 messages per frame
            cap -= 1;
            if cap == 0 {
                // Post a message to the window to continue processing later
                unsafe {
                    use windows::Win32::Foundation::{LPARAM, WPARAM};
                    use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
                    PostMessageW(
                        Some(hwnd),
                        crate::runtime::WM_ASYNC_MESSAGE,
                        WPARAM(0),
                        LPARAM(0),
                    )
                    .ok();
                }
                break;
            }
        }

        unsafe {
            use windows::Win32::Graphics::Gdi::InvalidateRect;
            let _ = InvalidateRect(Some(hwnd), None, false);
        }

        PENDING_MESSAGE_PROCESSING.store(false, Ordering::SeqCst);
    }

    // Spawn a task on the executor thread
    pub fn spawn_task(&self, task: Task<Message>) {
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
