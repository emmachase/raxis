use windows::Win32::{
    Foundation::{HWND, LPARAM, POINT},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Shell::{
            NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
            Shell_NotifyIconW,
        },
        WindowsAndMessaging::{
            GetCursorPos, HICON, IMAGE_ICON, LR_DEFAULTSIZE, LoadImageW, WM_LBUTTONDBLCLK,
            WM_LBUTTONUP, WM_RBUTTONUP,
        },
    },
};
use windows_core::PCWSTR;

pub const WM_TRAYICON: u32 = windows::Win32::UI::WindowsAndMessaging::WM_USER + 100;

/// Events that can occur on a tray icon
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// Left mouse button clicked
    LeftClick,
    /// Left mouse button double-clicked
    LeftDoubleClick,
    /// Right mouse button clicked
    RightClick,
}

/// Configuration for a system tray icon
#[derive(Clone)]
pub struct TrayIconConfig {
    /// Tooltip text that appears when hovering over the icon
    pub tooltip: Option<String>,
    /// Icon resource ID from the executable
    pub icon_resource: Option<u16>,
}

impl Default for TrayIconConfig {
    fn default() -> Self {
        Self {
            tooltip: None,
            icon_resource: None,
        }
    }
}

impl TrayIconConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn with_icon_resource(mut self, resource_id: u16) -> Self {
        self.icon_resource = Some(resource_id);
        self
    }
}

/// Manages a system tray icon
pub struct TrayIcon {
    hwnd: HWND,
    config: TrayIconConfig,
    added: bool,
}

impl TrayIcon {
    /// Create a new tray icon (doesn't add it to the system tray yet)
    pub fn new(hwnd: HWND, config: TrayIconConfig) -> Self {
        Self {
            hwnd,
            config,
            added: false,
        }
    }

    /// Add the tray icon to the system tray
    pub fn add(&mut self) -> windows_core::Result<()> {
        if self.added {
            return Ok(());
        }

        unsafe {
            let mut nid = self.create_notify_icon_data()?;
            Shell_NotifyIconW(NIM_ADD, &mut nid).ok()?;
            self.added = true;
            Ok(())
        }
    }

    /// Update the tray icon (tooltip, icon, etc.)
    pub fn update(&mut self, config: TrayIconConfig) -> windows_core::Result<()> {
        self.config = config;
        if !self.added {
            return Ok(());
        }

        unsafe {
            let mut nid = self.create_notify_icon_data()?;
            Shell_NotifyIconW(NIM_MODIFY, &mut nid).ok()?;
            Ok(())
        }
    }

    /// Remove the tray icon from the system tray
    pub fn remove(&mut self) -> windows_core::Result<()> {
        if !self.added {
            return Ok(());
        }

        unsafe {
            let mut nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: 1,
                ..Default::default()
            };
            Shell_NotifyIconW(NIM_DELETE, &mut nid).ok()?;
            self.added = false;
            Ok(())
        }
    }

    /// Parse a tray icon message into a TrayEvent
    pub fn parse_message(lparam: LPARAM) -> Option<TrayEvent> {
        let msg = lparam.0 as u32;

        match msg {
            WM_LBUTTONUP => Some(TrayEvent::LeftClick),
            WM_LBUTTONDBLCLK => Some(TrayEvent::LeftDoubleClick),
            WM_RBUTTONUP => Some(TrayEvent::RightClick),
            _ => None,
        }
    }

    unsafe fn create_notify_icon_data(&self) -> windows_core::Result<NOTIFYICONDATAW> {
        let mut flags = NIF_MESSAGE;
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            uCallbackMessage: WM_TRAYICON,
            ..Default::default()
        };

        // Load icon
        if let Some(resource_id) = self.config.icon_resource {
            if let Ok(icon) = load_icon_from_resource(resource_id) {
                nid.hIcon = icon;
                flags |= NIF_ICON;
            }
        }

        // Set tooltip
        if let Some(ref tooltip) = self.config.tooltip {
            let wide: Vec<u16> = tooltip
                .encode_utf16()
                .chain(std::iter::once(0))
                .take(128) // szTip is 128 wide chars
                .collect();
            nid.szTip[..wide.len().min(128)].copy_from_slice(&wide[..wide.len().min(128)]);
            flags |= NIF_TIP;
        }

        nid.uFlags = flags;
        Ok(nid)
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}

/// Load an icon from a resource ID
fn load_icon_from_resource(resource_id: u16) -> windows_core::Result<HICON> {
    unsafe {
        let hinstance = GetModuleHandleW(None)?;
        let hicon = LoadImageW(
            Some(hinstance.into()),
            PCWSTR(resource_id as usize as *const u16),
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE,
        )?;
        Ok(HICON(hicon.0))
    }
}
