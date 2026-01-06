use std::sync::OnceLock;

use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;

static IS_WINDOWS_11: OnceLock<bool> = OnceLock::new();

pub fn is_windows_11() -> bool {
    *IS_WINDOWS_11.get_or_init(|| {
        let mut version_info = OSVERSIONINFOW {
            dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
            ..Default::default()
        };
        let _ = unsafe { RtlGetVersion(&mut version_info) };
        version_info.dwBuildNumber >= 22000 && version_info.dwMajorVersion == 10
    })
}
