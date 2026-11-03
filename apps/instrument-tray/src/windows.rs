//! Small Win32 shell. Unsafe calls are confined here; telemetry stays in Rust's
//! separately supervised connector process and never runs on the window thread.
use crate::supervisor::Supervisor;
use std::{
    cell::RefCell,
    io,
    mem::size_of,
    path::PathBuf,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::*, Registry::*, Threading::*},
    UI::{Controls::Dialogs::*, Shell::*, WindowsAndMessaging::*},
};
const CALLBACK: u32 = WM_APP + 1;
const SHOW_STATUS: u32 = WM_APP + 2;
thread_local! {static UI:RefCell<Option<Supervisor>>=const {RefCell::new(None)};}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn message(_hwnd: HWND, text: &str) {
    // SAFETY: both UTF-16 strings remain alive until the modal call returns.
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(text).as_ptr(),
            wide("Round Instruments").as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}
unsafe fn notify(hwnd: HWND, action: u32) {
    let mut n: NOTIFYICONDATAW = std::mem::zeroed();
    n.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    n.hWnd = hwnd;
    n.uID = 1;
    n.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    n.uCallbackMessage = CALLBACK;
    n.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);
    let tip = wide("Round Instruments - click for status and settings");
    n.szTip[..tip.len()].copy_from_slice(&tip);
    Shell_NotifyIconW(action, &n);
    if action == NIM_ADD {
        n.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &n);
    }
}
fn startup_enabled() -> bool {
    let mut key = null_mut();
    // SAFETY: predefined HKCU root and NUL-terminated constant names; returned
    // handle is closed on every success path. No machine-wide registry writes.
    unsafe {
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide(r"Software\Microsoft\Windows\CurrentVersion\Run").as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        ) != ERROR_SUCCESS
        {
            return false;
        }
        let r = RegQueryValueExW(
            key,
            wide("RoundFlightInstruments").as_ptr(),
            null(),
            null_mut(),
            null_mut(),
            null_mut(),
        );
        RegCloseKey(key);
        r == ERROR_SUCCESS
    }
}
fn set_startup(enabled: bool) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    let value = wide(&format!("\"{}\"", exe.display()));
    let mut key = null_mut();
    unsafe {
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(r"Software\Microsoft\Windows\CurrentVersion\Run").as_ptr(),
            0,
            null(),
            0,
            KEY_SET_VALUE,
            null(),
            &mut key,
            null_mut(),
        ) != ERROR_SUCCESS
        {
            return Err(io::Error::other("Cannot open startup settings"));
        }
        let result = if enabled {
            RegSetValueExW(
                key,
                wide("RoundFlightInstruments").as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            )
        } else {
            RegDeleteValueW(key, wide("RoundFlightInstruments").as_ptr())
        };
        RegCloseKey(key);
        if result != ERROR_SUCCESS && result != ERROR_FILE_NOT_FOUND {
            return Err(io::Error::from_raw_os_error(result as i32));
        }
    }
    Ok(())
}
unsafe fn choose_dll(hwnd: HWND) -> Option<PathBuf> {
    let mut buffer = vec![0u16; 32768];
    let filter = wide("MSFS SimConnect runtime\0SimConnect.dll\0\0");
    let title = wide("Select SimConnect.dll from your working test package or MSFS SDK");
    let mut dialog: OPENFILENAMEW = std::mem::zeroed();
    dialog.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner = hwnd;
    dialog.lpstrFilter = filter.as_ptr();
    dialog.lpstrFile = buffer.as_mut_ptr();
    dialog.nMaxFile = buffer.len() as u32;
    dialog.lpstrTitle = title.as_ptr();
    dialog.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
    if GetOpenFileNameW(&mut dialog) == 0 {
        None
    } else {
        let n = buffer.iter().position(|c| *c == 0)?;
        Some(PathBuf::from(String::from_utf16_lossy(&buffer[..n])))
    }
}
fn with_supervisor<T>(action: impl FnOnce(&mut Supervisor) -> T) -> T {
    UI.with(|cell| action(cell.borrow_mut().as_mut().expect("UI initialized")))
}
unsafe fn menu(hwnd: HWND) {
    // Never retain a RefCell borrow across a modal Win32 call: its nested
    // message loop must keep servicing the supervisor's recovery timer.
    let (connect, writeback) = with_supervisor(|s| (s.settings.connect, s.settings.writeback));
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    for (id, label, checked) in [
        (1, "Status", false),
        (2, "Connect automatically", connect),
        (3, "Read-only (disable all aircraft controls)", !writeback),
        (4, "Start with Windows", startup_enabled()),
        (5, "Choose SimConnect runtime...", false),
        (6, "Open diagnostics folder", false),
        (7, "Export diagnostics...", false),
    ] {
        AppendMenuW(
            menu,
            MF_STRING | if checked { MF_CHECKED } else { 0 },
            id,
            wide(label).as_ptr(),
        );
    }
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    let choices = Supervisor::devices().unwrap_or_default();
    if choices.is_empty() {
        AppendMenuW(
            menu,
            MF_STRING | MF_GRAYED,
            99,
            wide("No Waveshare UART device detected").as_ptr(),
        );
    }
    for (index, (port, serial)) in choices.iter().take(32).enumerate() {
        AppendMenuW(
            menu,
            MF_STRING,
            100 + index,
            wide(&format!("Use {port} ({serial})")).as_ptr(),
        );
    }
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(menu, MF_STRING, 9, wide("Exit").as_ptr());
    let mut point: POINT = std::mem::zeroed();
    GetCursorPos(&mut point);
    SetForegroundWindow(hwnd);
    let selected = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
        point.x,
        point.y,
        0,
        hwnd,
        null(),
    );
    DestroyMenu(menu);
    PostMessageW(hwnd, WM_NULL, 0, 0);
    let result: io::Result<()> = match selected {
        1 => {
            let status = with_supervisor(|s| s.status());
            message(hwnd, &status);
            Ok(())
        }
        2 => with_supervisor(|s| {
            s.settings.connect = !s.settings.connect;
            s.restart();
            s.save()
        }),
        3 => with_supervisor(|s| {
            s.settings.writeback = !s.settings.writeback;
            s.restart();
            s.save()
        }),
        4 => set_startup(!startup_enabled()),
        5 => {
            if let Some(path) = choose_dll(hwnd) {
                with_supervisor(|s| s.choose_dll(path))
            } else {
                Ok(())
            }
        }
        6 => {
            let root = with_supervisor(|s| s.root.clone());
            ShellExecuteW(
                hwnd,
                wide("open").as_ptr(),
                wide(&root.to_string_lossy()).as_ptr(),
                null(),
                null(),
                SW_SHOWNORMAL,
            );
            Ok(())
        }
        7 => match with_supervisor(|s| s.export()) {
            Ok(path) => {
                message(hwnd, &format!("Saved diagnostics:\r\n{}", path.display()));
                Ok(())
            }
            Err(e) => Err(e),
        },
        9 => {
            DestroyWindow(hwnd);
            Ok(())
        }
        n if (100..132).contains(&n) => {
            if let Some((port, _)) = choices.get((n - 100) as usize) {
                with_supervisor(|s| s.enroll(port))
            } else {
                Ok(())
            }
        }
        _ => Ok(()),
    };
    if let Err(e) = result {
        message(hwnd, &e.to_string());
    }
}
unsafe extern "system" fn window(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // The callback runs only on this UI thread. Reentrant modal-window messages
    // use try_borrow_mut rather than creating aliased mutable references.
    if msg == WM_DESTROY {
        notify(hwnd, NIM_DELETE);
        PostQuitMessage(0);
        return 0;
    }
    if msg == WM_CLOSE {
        DestroyWindow(hwnd);
        return 0;
    }
    let taskbar = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
    if msg == taskbar {
        notify(hwnd, NIM_ADD);
        return 0;
    }
    if msg == SHOW_STATUS {
        let status = with_supervisor(|s| s.status());
        message(hwnd, &status);
        return 0;
    }
    if msg == CALLBACK {
        let event = (l as u32) & 0xffff;
        if event == WM_CONTEXTMENU
            || event == WM_LBUTTONUP
            || event == WM_RBUTTONUP
            || event == NIN_SELECT
            || event == (NIN_SELECT | 1)
        {
            menu(hwnd);
        }
        return 0;
    }
    UI.with(|cell| {
        let Ok(mut value) = cell.try_borrow_mut() else {
            return DefWindowProcW(hwnd, msg, w, l);
        };
        let Some(s) = value.as_mut() else {
            return DefWindowProcW(hwnd, msg, w, l);
        };
        if msg == WM_TIMER {
            s.tick();
            return 0;
        }
        DefWindowProcW(hwnd, msg, w, l)
    })
}
pub fn run() {
    // All pointers passed into Win32 reference live local buffers or the static
    // callback. Handles are removed/closed after the window loop exits.
    unsafe {
        let class = wide("RoundInstrumentsTrayWindow");
        let existing = FindWindowW(class.as_ptr(), null());
        if std::env::args().any(|arg| arg == "--quit") {
            if !existing.is_null() {
                PostMessageW(existing, WM_CLOSE, 0, 0);
            }
            return;
        }
        let mutex = CreateMutexW(
            null(),
            0,
            wide(r"Local\RoundFlightInstrumentsTray").as_ptr(),
        );
        if mutex.is_null() {
            message(null_mut(), "Cannot establish application ownership.");
            return;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if !existing.is_null() {
                PostMessageW(existing, SHOW_STATUS, 0, 0);
            }
            CloseHandle(mutex);
            return;
        }
        let supervisor = match Supervisor::open() {
            Ok(s) => s,
            Err(e) => {
                message(null_mut(), &e.to_string());
                CloseHandle(mutex);
                return;
            }
        };
        let first = !supervisor.root.join("settings.json").exists();
        if let Err(e) = supervisor.save() {
            message(null_mut(), &e.to_string());
            CloseHandle(mutex);
            return;
        }
        UI.with(|s| *s.borrow_mut() = Some(supervisor));
        let instance = GetModuleHandleW(null());
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.hInstance = instance;
        wc.lpszClassName = class.as_ptr();
        wc.lpfnWndProc = Some(window);
        if RegisterClassW(&wc) == 0 {
            message(null_mut(), "Cannot register the tray window.");
            CloseHandle(mutex);
            return;
        }
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            wide("Round Instruments").as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            message(null_mut(), "Cannot create the tray window.");
            CloseHandle(mutex);
            return;
        }
        // Consume a launcher's STARTF_USESHOWWINDOW setting on our intentionally
        // hidden owner, so the first requested status dialog is actually visible.
        ShowWindow(hwnd, SW_HIDE);
        notify(hwnd, NIM_ADD);
        SetTimer(hwnd, 1, 500, None);
        if first {
            message(hwnd, "Round Instruments is running in the notification area near the clock.\r\n\r\nClick its icon (possibly under hidden icons), choose your SimConnect runtime and select Use COM... for your Waveshare UART device.\r\n\r\nThe setup guide is in Start > Round Instruments. Startup and connection settings are in the tray menu.");
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        UI.with(|s| {
            s.borrow_mut().take();
        });
        UnregisterClassW(class.as_ptr(), instance);
        CloseHandle(mutex);
    }
}
