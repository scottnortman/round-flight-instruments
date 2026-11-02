use std::{env, path::PathBuf};
fn main() {
    println!("cargo:rerun-if-env-changed=MSFS_SDK");
    println!("cargo:rerun-if-changed=native/shim.cpp");
    if env::var_os("CARGO_FEATURE_SDK").is_none() {
        return;
    }
    assert_eq!(
        env::var("CARGO_CFG_TARGET_OS").unwrap(),
        "windows",
        "MSFS adapter requires Windows"
    );
    assert_eq!(
        env::var("CARGO_CFG_TARGET_ARCH").unwrap(),
        "x86_64",
        "MSFS adapter requires x64"
    );
    let root = PathBuf::from(env::var_os("MSFS_SDK").unwrap_or_else(|| r"C:\MSFS SDK".into()));
    let include = root.join("SimConnect SDK/include");
    assert!(
        include.join("SimConnect.h").is_file(),
        "Install MSFS 2020 Core SDK or set MSFS_SDK"
    );
    println!(
        "cargo:rerun-if-changed={}",
        include.join("SimConnect.h").display()
    );
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .include(include)
        .file("native/shim.cpp")
        .warnings(true)
        .warnings_into_errors(true)
        .define("WIN32_LEAN_AND_MEAN", None)
        .define("NOMINMAX", None)
        .static_crt(true)
        .compile("rfi_simconnect");
    println!("cargo:rustc-link-lib=kernel32");
}
