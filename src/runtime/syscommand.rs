use windows::Win32::UI::WindowsAndMessaging::{
    SC_CLOSE, SC_MAXIMIZE, SC_MINIMIZE, SC_RESTORE,
};

/// System commands that can be intercepted from window caption buttons
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemCommand {
    /// Window minimize button clicked
    Minimize,
    /// Window maximize button clicked
    Maximize,
    /// Window restore button clicked (from maximized state)
    Restore,
    /// Window close button clicked
    Close,
    /// Other system command
    Other(u32),
}

impl SystemCommand {
    /// Parse a system command from wparam
    pub fn from_wparam(wparam: usize) -> Self {
        // Mask off the lower 4 bits which may contain additional info
        let cmd = (wparam & 0xFFF0) as u32;
        match cmd {
            SC_MINIMIZE => SystemCommand::Minimize,
            SC_MAXIMIZE => SystemCommand::Maximize,
            SC_RESTORE => SystemCommand::Restore,
            SC_CLOSE => SystemCommand::Close,
            _ => SystemCommand::Other(cmd),
        }
    }
}

/// Response from a system command handler
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemCommandResponse {
    /// Allow the default behavior to proceed
    Allow,
    /// Prevent the default behavior (command is handled by application)
    Prevent,
}
