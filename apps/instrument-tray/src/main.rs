#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod supervisor;
#[cfg(windows)]
mod windows;
fn main() {
    #[cfg(windows)]
    windows::run();
    #[cfg(not(windows))]
    eprintln!("Round Instruments tray requires Windows.");
}
