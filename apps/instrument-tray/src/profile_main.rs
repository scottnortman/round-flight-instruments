//! Standalone entry point for reviewing profiles without starting the connector.
#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod profile_window;
fn main() {
    #[cfg(windows)]
    {
        let args: Vec<_> = std::env::args_os().collect();
        let root = args
            .iter()
            .position(|a| a == "--profile-dir")
            .and_then(|n| args.get(n + 1))
            .map(std::path::PathBuf::from);
        if let Err(error) = profile_window::standalone(root) {
            // SAFETY: the temporary UTF-16 message/title outlive this modal call.
            unsafe {
                let text: Vec<u16> = error.to_string().encode_utf16().chain(Some(0)).collect();
                let title: Vec<u16> = "Aircraft profiles".encode_utf16().chain(Some(0)).collect();
                windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                    std::ptr::null_mut(),
                    text.as_ptr(),
                    title.as_ptr(),
                    windows_sys::Win32::UI::WindowsAndMessaging::MB_OK
                        | windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
                );
            }
        }
    }
    #[cfg(not(windows))]
    eprintln!("The aircraft profile editor requires Windows.");
}
