#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod supervisor;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod profile_window;
fn main() {
    #[cfg(windows)]
    {
        let args:Vec<_>=std::env::args_os().collect();
        if args.iter().any(|a|a=="--profile-editor") {
            let root=args.iter().position(|a|a=="--profile-dir").and_then(|n|args.get(n+1)).map(std::path::PathBuf::from);
            if let Err(e)=profile_window::standalone(root){windows::show_error(&e.to_string());}
        }else{windows::run();}
    }
    #[cfg(not(windows))]
    eprintln!("Round Instruments tray requires Windows.");
}
