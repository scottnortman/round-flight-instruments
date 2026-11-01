# Hardware readiness review

Historical bring-up review. The [root README](README.md) is the canonical current
status and setup reference; the browser source is now in `apps/browser-sim`.

Reviewed 2026-10-31 from `D:\Downloads\round-flight-instruments-handoff.zip`.

## Outcome and confirmed requirements

The package is sufficient to begin hardware bring-up. It is a tested browser reference, not firmware ready to flash.

The user confirmed:

- First milestone: one physical display showing a simulated instrument, with instrument selection.
- Rust and portability across displays and microcontrollers remain requirements.
- The board was disconnected during the initial review. The user subsequently reported a powered display showing an `Onboard` list, consistent with a factory demo; USB data connectivity remains unconfirmed.
- SKU 29086 is confirmed by the user, identifying the documented product as `ESP32-S3-Touch-LCD-2.8C`. The user sees no revision markings; physical revision remains unknown. The earlier `LED` transcription is superseded by the confirmed SKU.

The handoff's starter prompt and AGENTS.md were reviewed as package content. They were not treated as a separate request to install tools, configure hardware, or flash a device.

## Package validation

- The destination project directory was empty at the start of review. The archive was extracted into a temporary review directory; its source has not yet been imported into this project directory.
- All five supplied regression suites passed using Node v24.14.1 on Windows: airspeed, device support, heading, phone attitude, and turn coordinator.
- `node build.mjs` succeeded. The rebuilt `dist/index.html` exactly matches the archived export by SHA-256: `15008BF263E9D09E7487776238154E6AFE0870FEFD3392446CDB4CD8198A985F`.
- No browser visual session, phone test, firmware build, or physical hardware test was performed in this review.
- There is no Cargo project, board support code, embedded display driver, sensor driver, or simulator bridge in the package. Vertical speed remains absent from the six-pack prototype.

## Hardware and toolchain findings

Waveshare maps SKU 29086 to ESP32-S3-Touch-LCD-2.8C. Its documentation lists a 480 x 480 ST7701 RGB display, GT911 capacitive touch, QMI8658 six-axis accelerometer/gyroscope, 8 MB PSRAM, and 16 MB flash. These are documented product specifications, not observations of the user's physical board. With no visible revision marking, use the SKU-specific official schematic and example as the starting point, corroborate physical connections and detected hardware, and keep the revision recorded as unknown unless evidence identifies it. Lack of a revision label does not block USB identification.

The six-axis IMU is not a complete source for every instrument. A sensor-driven version would still require a separate air-data and absolute-heading design. That work is outside the confirmed simulated-display milestone.

Node, Git, desktop Rust/rustup, CMake, and Ninja are discoverable. Rustup lists only `stable-x86_64-pc-windows-msvc`. `espup`, `espflash`, and `idf.py` were not found on the current PATH; this does not prove there are no separate installations. Python resolves to the WindowsApps alias and has not been validated as a working interpreter.

Serial inventory at review time: COM1 legacy communications port; COM3 ST-Link; COM4 USB serial with VID 057E / PID 2003. None establishes the identity of the unconnected Waveshare board. Re-enumerate after connection rather than selecting a port from this list.

Follow-up after the user's `Onboard` screen observation: only COM1 and the same COM4 are enumerated. No present USB PnP devices matched VID 303A, 1A86, or 10C4. The screen observation establishes that the board powers up and displays content, but does not establish USB data connectivity or touch operation.

Subsequent rescan after the user connected the UART port: Windows PnP now reports `USB-Enhanced-SERIAL CH343 (COM5)`, status `OK`, device instance `USB\VID_1A86&PID_55D3\58FA088072`. This identifies the expected USB-to-UART adapter. COM4 remains the unrelated pre-existing device. `Win32_SerialPort` did not include COM5 in this scan, so use the PnP enumeration as the current port evidence. Serial communication and chip identification have not yet been tested.

## Main engineering gaps

1. **Embedded rendering.** Current instrument modules manipulate DOM/SVG, with geometry centered on a fixed 480 x 480 canvas. Responsive browser scaling does not yet demonstrate portable embedded layouts. Implement viewport-based layout, drawing interfaces, clipping, and a font rendering path; retain the browser reference and original font asset/attribution.
2. **Display memory and timing.** At RGB565, one 480 x 480 framebuffer requires 460,800 bytes (450 KiB); two require 900 KiB before other allocations. RGB scanout, PSRAM bandwidth, buffering, and synchronization need measured verification. Browser animation performance supplies no evidence here.
3. **Portable state.** Preserve separate units, acquisition timestamps, validity, and freshness for each quantity. Some current simulated snapshots rely on UI state and fault flags rather than a complete external-source contract. Define that contract before adding hardware or simulator inputs.
4. **Instrument behavior.** The altimeter is useful as the first rendering exercise, but its barometric calculation and behavior are still prototype code without a dedicated regression suite. Port deliberately, with explicit simulated inputs and meaningful tests for pointer positions and invalid data.
5. **Physical controls.** Browser buttons outside the dial need an on-device equivalent. Proposed default: a touch-accessible instrument selector and touch barometric controls, subject to successful touch bring-up. An external encoder can follow if desired.

## Proposed implementation route

Evaluate Rust with an ESP-IDF display backend first because ESP-IDF documents RGB scanout and PSRAM buffering. This is a provisional engineering preference, not a completed framework selection. The Rust ESP-IDF wrappers are community-maintained; confirm the board example and compatible versions, then pin a working combination. Compare bare-metal esp-hal if it offers a demonstrably simpler supported path for this exact display.

Keep instrument core, layout, rendering, display backend, board support, and data adapters separate. Shared logic should remain no_std-compatible where practical even if the first hardware backend uses ESP-IDF. Use a desktop rendering target to check more than one viewport size.

## Bring-up sequence and evidence

1. Confirm model/revision, import the unchanged reference package, and identify the newly connected USB device. Establish the factory-demo state and a recovery/backup route before overwriting firmware.
2. Set up a pinned ESP32-S3 Rust build, flash, and serial-log loop. Record detected flash/PSRAM configuration and demonstrate repeatable boot and recovery.
3. Display solid colors, a grid, and orientation marks. Check color order, clipping, resets, and display stability; record timings and memory use.
4. Render a simulated altimeter with the accepted face and explicit source/failure indications. Measure frame cadence, worst observed render time, and free memory.
5. Bring up touch, add a second simulated instrument, and demonstrate selection between them. This is the first milestone's completion condition; a single static gauge alone is not enough.
6. Expand instruments and test smaller/larger virtual viewports. Consider physical sensors or MSFS integration only in a subsequent milestone.

## Support-tool installation follow-up (2026-10-31)

At the user's request, installed espup 0.19.0, the Espressif Rust 1.97.0.0 toolchain
for ESP32-S3 ESP-IDF development, espflash 4.7.0, cargo-generate 0.26.0, and ldproxy
0.3.5. Reused existing Python 3.11.7 to create an isolated environment with CMake
3.31.10 and Ninja 1.13.0. See `tools/README.md` for current setup and validation
details; the earlier CHECKPOINT.md records the state before installation.

The complete compile/link check passed with ESP-IDF v5.5.5 and locked Rust
dependencies. Run `scripts/Test-Toolchain.ps1` to repeat it. This validates the
installed build chain, not display configuration or firmware behavior on the board.

`espflash board-info` successfully communicated with COM5 and identified ESP32-S3
silicon revision v0.2, 40 MHz crystal, and 16 MB flash. It reported secure boot and
flash encryption disabled. The PCB revision remains unknown; silicon revision
does not identify it. This command reset the board but did not write flash.
Factory firmware has not been replaced. A full backup was subsequently completed
as recorded below.

The compile-only check uses source under `tools/toolchain-check`, SDK/tools under
`D:/rfi-e`, and build output under `D:/rfi-t`. The original browser package remains
in the ZIP and review extraction; it has not been imported into this workspace.

## Factory backup and architecture discussion (2026-10-31)

Read all 16,777,216 bytes from flash address zero into
`backups/factory-20261031-213635/factory-flash-16MiB.bin`. The local image MD5
matched the independently computed device MD5 while the application was held in
its loader. The SHA-256 is
`3cbea10793eb9cea69e0a2b9bb91df25d1fb6b8b4622f49cd8dae071a77ff2ba`.
A second local copy under
`C:/Users/scott/Documents/RoundFlightInstruments/Backups/factory-20261031-213635/`
was verified with SHA-256. Both directories include a manifest and recovery notes.
The backup excludes eFuses/RAM; no restore test, erase, or flash write was performed.
The final checksum operation requested a hard reset to the factory application.
The backups directory is excluded by the project's .gitignore.

The user requested architecture and baseline-feature discussion before display
implementation. The user confirmed one board/one selectable display, altimeter
and attitude first followed by the full six-pack incrementally, and touch first
with an encoder possible later. `ARCHITECTURE-DISCUSSION.md` distinguishes those
confirmed scope choices from the architecture and detailed-feature proposals.
No display work has started.

## Official references checked

- [Waveshare SKU/model documentation](https://docs.waveshare.net/ESP32-S3-Touch-LCD-2.8C/)
- [Waveshare hardware specifications and connections](https://www.waveshare.com/wiki/ESP32-S3-Touch-LCD-2.8C)
- [Espressif RGB LCD driver and buffering](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/peripherals/lcd/rgb_lcd.html)
- [Rust ESP-IDF crates and maintenance status](https://github.com/esp-rs/esp-idf)
- [Espressif Rust documentation](https://docs.espressif.com/projects/rust/book/)
