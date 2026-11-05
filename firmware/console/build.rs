fn main() {
    embuild::espidf::sysenv::output();
    // Git is build metadata only. Source archives build with an explicit fallback.
    println!("cargo:rerun-if-env-changed=RFI_BUILD_REV");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs");
    let describe = std::env::var("RFI_BUILD_REV").unwrap_or_else(|_| {
        std::process::Command::new("git")
            .args([
                "describe",
                "--tags",
                "--long",
                "--always",
                "--dirty",
                "--abbrev=8",
            ])
            .output()
            .ok()
            .filter(|r| r.status.success())
            .and_then(|r| String::from_utf8(r.stdout).ok())
            .unwrap_or_else(|| "source-archive".into())
    });
    println!("cargo:rustc-env=RFI_FIRMWARE_REV={}", describe.trim());
}
