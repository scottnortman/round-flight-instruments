# Windows ESP32-S3 support tools

This is the dated provisioning record. Use the
[current getting-started guide](../docs/GETTING-STARTED.md#hardware-development)
for setup and the [root README](../README.md) for project status. Absolute paths
and adapter identity below belong to the original development workstation.

This directory contains a compile-only toolchain check, generated from the official
esp-rs/esp-idf-template at commit b814c2b968f84949707ed0a6ab72d262e3fbfbc4.
It is not the instrument firmware and has not been flashed. The generated Cargo
flash runner was removed so the check does not provide an implicit flashing step.
Its generic SDK configuration does not configure the Waveshare display, touch,
PSRAM, or a board-specific partition table.

From PowerShell in the project root:

```powershell
. .\scripts\Enter-DevShell.ps1
.\scripts\Test-Toolchain.ps1
```

The first command loads the installed ESP Rust environment and places the base
Python interpreter and isolated CMake/Ninja tools on the current terminal's PATH. The
second compiles an ESP32-S3 executable with the locked Cargo dependencies. It
does not open a serial port or write firmware.

## Verification completed 2026-10-31

`scripts/Test-Toolchain.ps1` completed successfully with exit code 0, including
the ESP-IDF C libraries, bootloader build, Rust standard library, bindings, and
final executable link. The successful provisioning/build run took 6m 45s; earlier
downloads and setup are additional. No firmware was flashed.

Two Windows setup issues were resolved in the scripts: the ESP-IDF output-path
limit (using D:/rfi-t) and starting the SDK's Python-environment installer from
base Python. Cargo.lock is retained for repeatable dependency resolution.

## Installed components

| Component | Version / location |
| --- | --- |
| espup | 0.19.0; user .cargo/bin |
| ESP Rust compiler | rustc 1.97.0-nightly, Espressif release 1.97.0.0; rustup toolchain `esp` |
| espflash | 4.7.0; user .cargo/bin |
| cargo-generate | 0.26.0; user .cargo/bin |
| ldproxy | 0.3.5, installed with `cargo install ldproxy --locked`; user .cargo/bin |
| Python | Existing PlatformIO Python 3.11.7, reused as the base interpreter |
| Isolated Python environment | %LOCALAPPDATA%/round-flight-tools/python-env |
| CMake | 3.31.10 in the isolated environment |
| Ninja | Python package 1.13.0 in the isolated environment |
| ESP-IDF for the check | v5.5.5, provisioned by esp-idf-sys |
| SDK C compiler | xtensa-esp-elf esp-14.2.0_20260121 |
| SDK Clang | esp-19.1.2_20250312 |
| SDK-managed CMake / Ninja | 3.30.2 / 1.12.1 |
| SDK ULP compiler / ROM symbols | 2.38_20240113 / 20241011 |
| Rust bindings | esp-idf-sys 0.38.1, esp-idf-hal 0.47.0, esp-idf-svc 0.53.0; see Cargo.lock |

VS Code, rust-analyzer, Codex, Microsoft C++ tools, Git, Node, and desktop Rust
were already installed. Desktop Rust remains the default toolchain. The `esp`
toolchain is selected only by the check's rust-toolchain.toml. espup was installed
with `--targets esp32s3 --std`; the ESP-IDF build downloads its matching C toolchain.
This setup has not been validated for bare-metal no_std firmware.

The existing PlatformIO environment and system CMake/Ninja were not modified.
The project Python environment depends on the PlatformIO base Python remaining
installed. espup created ~/export-esp.ps1 and added its Clang variables to the
user environment; open a new terminal or use Enter-DevShell.ps1 to load them.
ESP-IDF creates a separate Python environment under D:/rfi-e/python_env. Its
installer must start from base Python rather than another virtual environment;
Enter-DevShell.ps1 puts the interpreter paths in the required order.

## Build locations

- Source: tools/toolchain-check
- ESP-IDF and its tools: D:/rfi-e
- Build output: D:/rfi-t
- Expected executable: D:/rfi-t/xtensa-esp32s3-espidf/debug/toolchain-check

Short external paths avoid Windows path-length problems. The absolute D: paths
are specific to this workstation and can be changed in the check configuration
and Test-Toolchain.ps1. The framework version used here validates the tools; final
display firmware dependencies must still be checked against the Waveshare example.

## Board identification

On 2026-10-31, `espflash board-info --port COM5 --non-interactive --skip-update-check`
successfully identified ESP32-S3 silicon revision v0.2, a 40 MHz crystal, and
16 MB flash. Secure boot and flash encryption were reported disabled. Silicon
revision is distinct from the unknown Waveshare PCB revision. PSRAM has not yet
been measured by running firmware.

COM5 is the CH343 adapter with device instance USB/VID_1A86&PID_55D3/58FA088072.
Reconfirm the device before using the port; Windows may reassign COM numbers.
The board-info check briefly entered the loader and requested a hard reset back
to the existing program. It did not erase or program flash.

Factory backup subsequently completed and verified on 2026-10-31. The full 16 MiB
image is in `backups/factory-20261031-213635/`, with a checksum-verified second
copy under `C:/Users/scott/Documents/RoundFlightInstruments/Backups/factory-20261031-213635/`.
The backup README documents recovery; manifest.json records checksums and device
identity. The backup step did not write firmware. The subsequent console baseline
was flashed and checked over UART; see [verification](../verification/README.md).
Factory restore has not been exercised.

## Download provenance

Release archives came directly from the maintainers' GitHub release pages and
were verified against SHA-256 digests returned by the GitHub releases API:

- espup v0.19.0 Windows x86_64 executable:
  c0ceeb4894064cfedb0e3c6e31173d260719d47578843823f81f84fe6fbacf96
- espflash v4.7.0 Windows x86_64 ZIP:
  2b10333e4f4f1a26024259b0cc1942ba17f27b3c530621c6111f4c10ac70213e
- cargo-generate v0.26.0 Windows x86_64 tar.gz:
  4ac7ebfcd6fdfbd1cb671af793b90c022b8da19d5fe49ed1c539e640c2eb881e

Downloads are retained in %LOCALAPPDATA%/round-flight-tools/downloads.
The template supports native Cargo-driven ESP-IDF installation, so a separate
system-wide ESP-IDF installer, WSL installation, or driver replacement was unnecessary.

References: [espup](https://github.com/esp-rs/espup),
[espflash](https://github.com/esp-rs/espflash),
[ESP-IDF Rust template](https://github.com/esp-rs/esp-idf-template),
[ESP-IDF Rust build options](https://github.com/esp-rs/esp-idf/blob/master/esp-idf-sys/BUILD-OPTIONS.md).
