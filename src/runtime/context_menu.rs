use futures::channel::oneshot;
use windows::Win32::{
    Foundation::{HWND, LPARAM, POINT, WPARAM},
    UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetSystemMetrics, MF_CHECKED,
        MF_DISABLED, MF_GRAYED, MF_SEPARATOR, MF_STRING, PostMessageW, SM_MENUDROPALIGNMENT,
        SetForegroundWindow, TPM_LEFTALIGN, TPM_LEFTBUTTON, TPM_RETURNCMD, TPM_RIGHTALIGN,
        TrackPopupMenuEx, WM_USER,
    },
};
use windows_core::PCWSTR;

use crate::runtime::UncheckedHWND;

/// Custom window message for showing context menus on the UI thread
pub const WM_SHOW_CONTEXT_MENU: u32 = WM_USER + 200;

/// A context menu that can be displayed to the user
#[derive(Debug, Clone)]
pub struct ContextMenu {
    pub items: Vec<ContextMenuItem>,
}

/// Data to pass to the UI thread for showing a context menu
pub struct ContextMenuRequest {
    pub items: Vec<ContextMenuItem>,
    pub position: Option<(i32, i32)>,
    pub sender: oneshot::Sender<Option<usize>>,
}

/// A single item in a context menu
#[derive(Debug, Clone)]
pub struct ContextMenuItem {
    /// The text to display for this menu item
    pub label: String,
    /// Whether this item is enabled
    pub enabled: bool,
    /// Whether this item is checked
    pub checked: bool,
    /// Whether this item is a separator (label and message are ignored)
    pub is_separator: bool,
}

impl ContextMenu {
    /// Create a new empty context menu
    pub fn new(items: Vec<ContextMenuItem>) -> Self {
        Self { items }
    }

    /// Posts a request to show the context menu on the UI thread and returns the selected index
    ///
    /// This function sends the menu request to the UI thread via a window message,
    /// then awaits the result asynchronously.
    pub async fn show_async(
        &self,
        hwnd: UncheckedHWND,
        position: Option<(i32, i32)>,
    ) -> Option<usize> {
        let (sender, receiver) = oneshot::channel();

        let request = Box::new(ContextMenuRequest {
            items: self.items.clone(),
            position,
            sender,
        });

        // Send the request pointer to the UI thread
        let request_ptr = Box::into_raw(request);

        unsafe {
            let _ = PostMessageW(
                Some(hwnd.0),
                WM_SHOW_CONTEXT_MENU,
                WPARAM(request_ptr as usize),
                LPARAM(0),
            );
        }

        // Await the result from the UI thread
        receiver.await.ok().flatten()
    }

    /// Show the context menu synchronously on the UI thread (called from window procedure)
    ///
    /// # Safety
    /// This function MUST be called from the UI thread that owns the window
    pub unsafe fn show_sync_on_ui_thread(
        items: &[ContextMenuItem],
        hwnd: HWND,
        position: Option<(i32, i32)>,
    ) -> Option<usize> {
        unsafe {
            // Create the popup menu
            let hmenu = CreatePopupMenu().ok()?;

            // Add all items to the menu
            for (index, item) in items.iter().enumerate() {
                if item.is_separator {
                    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
                } else {
                    let mut flags = MF_STRING;
                    if !item.enabled {
                        flags |= MF_DISABLED | MF_GRAYED;
                    }
                    if item.checked {
                        flags |= MF_CHECKED;
                    }

                    let wide_label: Vec<u16> = item
                        .label
                        .encode_utf16()
                        .chain(std::iter::once(0))
                        .collect();

                    // Use index + 1 as the command ID (0 means no selection)
                    let _ = AppendMenuW(hmenu, flags, index + 1, PCWSTR(wide_label.as_ptr()));
                }
            }

            // Determine position
            let (x, y) = if let Some((px, py)) = position {
                (px, py)
            } else {
                let mut cursor_pos = POINT::default();
                GetCursorPos(&mut cursor_pos).ok()?;
                (cursor_pos.x, cursor_pos.y)
            };

            // Set the window as foreground so the menu can be shown
            let _ = SetForegroundWindow(hwnd);

            // Determine the correct alignment based on system settings
            // SM_MENUDROPALIGNMENT returns non-zero if menus are right-aligned
            let is_right_aligned = GetSystemMetrics(SM_MENUDROPALIGNMENT) != 0;

            let horizontal_align = if is_right_aligned {
                TPM_RIGHTALIGN
            } else {
                TPM_LEFTALIGN
            };

            // Show the menu and get the selected command
            let selected_cmd = TrackPopupMenuEx(
                hmenu,
                (horizontal_align | TPM_LEFTBUTTON | TPM_RETURNCMD).0,
                x,
                y,
                hwnd,
                None,
            );

            // Clean up the menu
            let _ = DestroyMenu(hmenu);

            // If a command was selected (non-zero), return the index
            if selected_cmd.0 > 0 {
                let index = (selected_cmd.0 - 1) as usize;
                if index < items.len() {
                    Some(index)
                } else {
                    None
                }
            } else {
                None
            }
        }
    }
}

impl ContextMenuItem {
    /// Create a new context menu item
    pub fn new<T>(message: T, label: impl Into<String>) -> (Option<T>, Self) {
        (
            Some(message),
            Self {
                label: label.into(),
                enabled: true,
                checked: false,
                is_separator: false,
            },
        )
    }

    /// Create a separator item
    pub fn separator<T>() -> (Option<T>, Self) {
        (
            None,
            Self {
                label: String::new(),
                enabled: true,
                checked: false,
                is_separator: true,
            },
        )
    }

    /// Set whether this item is enabled
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set whether this item is checked
    pub fn with_checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Set whether this item is disabled
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Set whether this item is checked
    pub fn checked(mut self) -> Self {
        self.checked = true;
        self
    }
}

pub trait ContextMenuItemExt<T> {
    fn with_message(self, message: T) -> Self;
    fn with_enabled(self, enabled: bool) -> Self;
    fn with_checked(self, checked: bool) -> Self;
    fn disabled(self) -> Self;
    fn checked(self) -> Self;
}

impl<T> ContextMenuItemExt<T> for (Option<T>, ContextMenuItem) {
    fn with_message(self, message: T) -> Self {
        (Some(message), self.1)
    }
    fn with_enabled(self, enabled: bool) -> Self {
        (self.0, self.1.with_enabled(enabled))
    }
    fn with_checked(self, checked: bool) -> Self {
        (self.0, self.1.with_checked(checked))
    }
    fn disabled(self) -> Self {
        (self.0, self.1.disabled())
    }
    fn checked(self) -> Self {
        (self.0, self.1.checked())
    }
}
