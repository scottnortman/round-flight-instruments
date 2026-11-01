# Getting started

**Have Waveshare SKU 29086 and want to run the release?** Start with the
[prebuilt firmware installation guide](INSTALL-WAVESHARE.md); no Rust compiler is needed.

This guide accompanies the [canonical README](../README.md). Start with a PC
workflow; no board or ESP toolchain is needed to explore the browser gauges or
run Rust core tests. Windows PowerShell is the tested environment. The browser
build scripts are portable Node.js; the Python desktop launcher currently
expects `target/debug/desktop-cli.exe` on Windows.

## 1. Prerequisites and checkout

Install only the tools required for your chosen path:

| Path | Requirements |
| --- | --- |
| Browser gauges | Node.js/npm and a modern browser; Python for the local HTTP server |
| Rust desktop | Desktop Rust/Cargo through rustup; on Windows, Microsoft C++ build tools; Python for the interactive client |
| UART control only | Python 3.11 or compatible, PySerial, a data USB cable and the correct serial driver/port |
| Firmware development | All desktop requirements plus the ESP Rust compiler, espflash, ldproxy, base Python, CMake/Ninja and ESP-IDF |

Official installation references: [Rust](https://rust-lang.org/tools/install/),
[Node.js](https://nodejs.org/en/download), [Python](https://www.python.org/downloads/windows/),
[espup](https://github.com/esp-rs/espup), [espflash](https://github.com/esp-rs/espflash).
The recorded workstation versions are desktop Rust 1.85.0, Node 24.14.1 and
Python 3.11.7. These are observed versions, not a promise of a tested minimum for
all components. Firmware uses a separate compiler, described below.

Clone your fork, or open an existing checkout. Replace `YOUR-ACCOUNT` first:

```powershell
git clone https://github.com/YOUR-ACCOUNT/round-flight-instruments.git
Set-Location round-flight-instruments
git status --short
```

All subsequent commands run from this root unless stated otherwise. `python`
means your actual Python interpreter; if Windows uses the `py` launcher, use
`py -3.11` or an explicit executable path instead. Do not install Python packages
into an unrelated application's environment.

For UART control and optional image conversion, create a local environment:

```powershell
py -3.11 -m venv .venv
.\.venv\Scripts\python.exe -m pip install pyserial Pillow
```

Use that executable in place of `python` below when those packages are needed.
Virtual environments and generated output are ignored by Git. Browser serving
and the desktop-only CLI need no third-party Python packages. The IMU plotting
script additionally needs `matplotlib`, `numpy` and, on Windows, `tzdata`.

## 2. Run the interactive PC gauges

```powershell
Push-Location apps/browser-sim
npm run check
npm run build
Pop-Location
python -m http.server 8000 --bind 127.0.0.1 --directory apps/browser-sim/dist
```

Leave that terminal open and visit <http://127.0.0.1:8000>. The server binds only
to your PC. Press Ctrl+C when finished. There are no npm dependencies, so
`npm install` is unnecessary.

Expected checks: five suites covering airspeed, device policy, heading, phone
attitude and turn coordinator, followed by a successful build message. The build
assembles a self-contained `dist/index.html` with the font and scripts embedded.

Try this sequence:

1. Confirm Heading opens, then run its demo to observe card rotation and north
   crossings. Use manual reference controls to inspect specific headings.
2. Select Airspeed and try the preset speeds/sliders. The range markings use a
   C172S example profile; the overspeed demonstration does not clamp to VNE.
3. Select Turn coordinator and vary yaw/roll rate and ball position. The small
   airplane is a rate cue, not an attitude indicator.
4. Select Attitude, Altimeter, or Attitude + altitude. Use manual inputs, the
   demonstration sequence and the barometer editor where available.
5. Use the Data selector to introduce independent failures and observe which
   gauge elements disappear or show a fault indication.

The browser has five individual gauges and one combined view; VSI is pending.
Changing browser controls does not send commands to the Rust CLI or hardware.
Its models and limits are detailed in
[apps/browser-sim/REFERENCE.md](../apps/browser-sim/REFERENCE.md).

### Optional phone experiments

Desktop browsers deliberately disable phone input, even on touch laptops.
To experiment on a supported phone, host the generated page over HTTPS and open
it directly on that phone. The PC's `127.0.0.1` address is not a phone-accessible
server. Tap Connect phone, grant supported permissions, wait for readings, hold
the screen upright facing you, and tap Set level. Screen rotation/page hiding
can require setting level again. Permission denial never produces fake samples.

Phone readings stay in that browser; there is no relay to the board or PC.
Heading is relative, and altitude/IAS stay simulated. The optional phone link
in `src/site-settings.json` defaults to `./`; use your own HTTPS URL for a
separate deployment and rebuild. No hosted service is required for desktop demos.

## 3. Run the Rust desktop simulation

```powershell
cargo build -p desktop-cli --locked
python scripts/cli.py
```

Expect `rfi>` from the client. Enter commands separately, waiting for each reply:

```text
sim preset level
instrument altimeter
baro 29.92
status
sim set pressure_altitude_ft 8500
status
sim takeoff 10
sim takeoff status
```

The preset stages 4520 ft and 120 kt IAS with other simulated inputs zero.
Set/preset values apply on the next running sample, so an immediate status
response can precede publication. The sweep holds zero for a second, then takes
10 seconds to climb. Query status again after it completes: altitude is 10000 ft
and state is complete. This process has no graphical window.

```text
sim takeoff stop
fault pressure_altitude_ft on
status
fault pressure_altitude_ft off
sim pause
```

After more than one second paused, `status` reports stale altitude without a
usable numeric value. Use `sim resume` to publish new samples. `sim clear
pressure_altitude_ft` makes altitude missing immediately. `exit` is a Python
client command, not part of the board wire protocol. Each new desktop process
starts a new session; it does not share or persist the board's settings.

For raw stdin/stdout use `cargo run -p desktop-cli --locked`. It emits `READY`
and accepts the same protocol, without the Python prompt. Physical display/IMU
commands are unsupported on desktop. See [CLI.md](CLI.md) for exact errors,
accepted ranges and state transitions.

## 4. View the Rust gauge renderer on the PC

```powershell
cargo run -p instrument-ui --example snapshots --locked
```

Expect 30 binary PPM files in `output/gauge/`. Use a PPM-capable viewer, or convert
one to PNG using the optional Pillow package:

```powershell
.\.venv\Scripts\python.exe -c "from PIL import Image; Image.open('output/gauge/valid-480.ppm').save('output/gauge/valid-480.png')"
Start-Process output/gauge/valid-480.png
```

Compare `manual-8500-480.ppm`, `zero-480.ppm`, `negative-480.ppm`, `baro-480.ppm`,
`missing-480.ppm`, `stale-480.ppm` and `failed-480.ppm`. Invalid frames should not
show usable pointers. This example renders fixed fixtures and does not consume
the running CLI process's state. A live Rust desktop GUI remains future work.

The required OTF, raster glyph data and panel register sequence are checked in;
ordinary builds do not run `scripts/prepare-gauge-assets.py`. That historical
regeneration script requires the original handoff ZIP at its recorded path and
the downloaded Waveshare demo under ignored `tmp/`. Likewise,
`render-schematic.py` expects a separately downloaded PDF and PyMuPDF. These are
maintenance aids, not prerequisites for a fresh clone's desktop workflows.

## 5. Run desktop acceptance checks

```powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
cargo build -p desktop-cli --locked
New-Item -ItemType Directory -Force output | Out-Null
python scripts/cli.py --test --slow --report output/desktop-acceptance.json
python scripts/check-takeoff.py --seconds 5 --report output/desktop-takeoff.json
```

The `--slow` acceptance check includes a deliberate 30-second incomplete-line
wait. It ends with simulation paused. The takeoff check exercises cancellation,
manual override, faults and completion. Reports should contain `passed: true`.
These test separate desktop processes and do not open the UART.

74 Rust unit tests currently cover state/quality, framing, backpressure, sensor
register behavior, touch gestures, estimator contracts, gauge math and cached/full raster
agreement. They do not measure optical LCD health or certify hardware timing.

## 6. Control the installed board

Connect the SKU 29086 board's **UART** USB socket with a data cable. The native
USB socket is available for ESP32-S3 USB capabilities, but this firmware's CLI
uses UART0 through CH343. Find the port in Device Manager or PowerShell:

```powershell
Get-PnpDevice -PresentOnly -Class Ports |
    Select-Object FriendlyName, InstanceId, Status
```

COM5 is the verified port on the development workstation; Windows may assign a
different number elsewhere. Match the board adapter before substituting a port.
COM4 on the development workstation is unrelated. Do not rely on port number
alone. Close other terminal/monitor programs before connecting:

```powershell
.\.venv\Scripts\python.exe scripts/cli.py --port COM5
```

For another terminal: **921600 baud, 8 data bits, no parity, 1 stop bit, no flow
control, local echo off**, ASCII with CR/LF/CRLF. Early boot logs use 115200; wait
for readable application READY before commands. Only one program can own the
port. Client exit releases it without requesting reset.

```text
status
display status
sim preset level
instrument altimeter
display gauge
display on
sim takeoff
```

Watch the climb, then query `display stats` and `sim takeoff status`. A final
10000 ft and frozen measurement are expected. The preferred target is 30 FPS; the current pipelined renderer exceeds it on
the altimeter. See verification/RENDERING-30HZ.md for the measured workload and
limits. Exact results vary with load. Check
edges, needle movement and absence of unintended blanking directly; counters
alone cannot establish that the physical image is clean.

First, try the touch barometer control while the altimeter has valid data. Hold
the Kollsman window for half a second and release. Tap + once, then Apply: the
barometer increases by 0.01 inHg and the displayed altitude changes accordingly.
Tap 29.92 to reset the draft to standard pressure, then Apply to commit.
Reopen, adjust and Cancel to verify that the committed value stays unchanged.
Use `touch status` for coordinates/errors and `touch hold 250` for the hold
threshold. See [TOUCH.md](TOUCH.md) for limits, timeout and shared-bus behaviour.

For optional IMU diagnostics:

```text
imu on
imu status
imu read
imu estimator
imu off
```

The Python client also accepts `watch imu` for a ten-second live readout after
`imu on`. Sensor diagnostics do not change the simulated gauge source. The
unresolved stationary Y-gyro offset prevents treating this as calibrated
attitude. See [IMU.md](IMU.md) before self-test, capture or motion experiments.

## Hardware development

### Validated environment and machine-specific assumptions

The checked-in firmware is a separate Cargo workspace. Its config selects
`xtensa-esp32s3-espidf`, ESP-IDF **v5.5.5**, and toolchain **esp**. The recorded ESP
compiler is Espressif 1.97.0.0 (`rustc 1.97.0-nightly`). `espup` 0.19.0, `espflash`
4.7.0 and `ldproxy` 0.3.5 were used. Lockfiles pin dependencies; `channel = "esp"`
does not itself pin the installed compiler version. Record/check your compiler
before comparing builds. See [tools/README.md](../tools/README.md).

The current scripts are validated on one Windows workstation, not a universal
installer. Read these assumptions before building on a fresh PC:

| Item | Current location / behavior | Adaptation on another machine |
| --- | --- | --- |
| Base Python | `%USERPROFILE%/.platformio/python3/python.exe` | `Enter-DevShell.ps1` requires this base interpreter; point its base-Python variable at your own Python 3.11 installation if different |
| CMake/Ninja environment | `%LOCALAPPDATA%/round-flight-tools/python-env` | Create from base Python; this is distinct from the root `.venv` |
| ESP environment | `%USERPROFILE%/export-esp.ps1` | Generated by espup; loaded by the dev-shell script |
| SDK/tools | `D:/rfi-e` | Set in both firmware and toolchain-check `.cargo/config.toml`; flash script also names the matching partition CSV |
| Firmware target | `D:/rfi-c` | Build/flash scripts use its artifact manifest; retain a short writable path |
| Probe target | `D:/rfi-t` | Used by `Test-Toolchain.ps1` |
| Flash identity | Exact CH343 device instance | Deliberately restricted to the verified original board |
| Recovery image | Local `backups/factory-20261031-213635` with expected hash | Not in Git; another board must use its own verified backup and identity |

If you have no D: drive, update **all** corresponding paths together, including
`Test-BoardWatchdog.ps1` before using it. Keep ESP-IDF output paths short to avoid
Windows path-length failures. Do not substitute another build directory's
bootloader or generated SDK configuration.

### Provisioning outline for a new workstation

Install desktop Rust and the Windows build prerequisites first. Install the
recorded releases of [espup](https://github.com/esp-rs/espup/releases) and
[espflash](https://github.com/esp-rs/espflash/releases) from their official
releases, or build them with Cargo if the host compiler meets their requirements.
For the recorded setup, espup installed target `esp32s3` with `--std`. Consult
`espup install --help` for selecting the recorded toolchain version; do not
assume a newly installed default matches this baseline.

```powershell
espup install --targets esp32s3 --std
cargo install ldproxy --version 0.3.5 --locked
```

The first command installs the default available ESP toolchain unless you select
a version using your espup release's supported options. Verify `rustc +esp
--version`; a newer compiler is an environment change requiring revalidation.
Keep the desktop toolchain as the root default.

Create the CMake/Ninja environment using your **base** Python, not another venv:

```powershell
$rfiToolsEnv = Join-Path $env:LOCALAPPDATA 'round-flight-tools\python-env'
py -3.11 -m venv $rfiToolsEnv
& (Join-Path $rfiToolsEnv 'Scripts\python.exe') -m pip install cmake==3.31.10 ninja==1.13.0
```

Adapt the base interpreter path in `Enter-DevShell.ps1` if needed. The script
puts base Python before this environment in PATH so ESP-IDF can create its own
Python environment. SDK dependencies are provisioned by `esp-idf-sys`; first
builds need downloads and can take several minutes. This outline documents the
existing setup; it has not been tested as a clean-machine automated installer.

### Compile without touching the board

```powershell
. .\scripts\Enter-DevShell.ps1
.\scripts\Test-Toolchain.ps1
.\scripts\Build-Console.ps1
```

The probe and firmware build do not open the serial port or flash. The firmware
build records exact Cargo-reported ELF, SDK config and bootloader paths plus an
ELF hash in `D:/rfi-c/console-build-artifacts.json`. It checks generated SDK safety
settings, including disabled forced per-VSYNC restart. Preserve the current
profile: raster code is optimized separately from control/FFI. An arbitrary
optimization change can alter behavior and requires revalidation.

For firmware formatting (separate from root workspace):

```powershell
Push-Location firmware/console
cargo fmt --all -- --check
Pop-Location
```

### Backup and flash

Before first programming a different board, identify its exact SKU/revision,
flash size and adapter, read its original flash using the flash tool's documented
read-back procedure, and keep two checksum-matched copies with recovery notes.
Do not replace the known-board hash with an arbitrary file just to pass a check.
The original development board has a verified 16 MiB backup; factory restoration
has not been exercised. Private recovery files are intentionally absent from Git.

`Flash-Console.ps1` is currently a guarded tool for the original board. On that
workstation, with its backup present and the terminal disconnected:

```powershell
.\scripts\Flash-Console.ps1 -Port COM5
```

It builds, checks adapter identity, verifies the recovery-image checksum and
ELF hash, then writes the matching bootloader, partition table and application
at flash-tool baud 460800. This replaces installed firmware and resets the
board. It does not modify eFuses. Wait for application startup before reconnecting
at 921600. For a different board, deliberately provision its own identity and
recovery checks in the script before flashing; a fresh clone is not sufficient.

`Test-BoardWatchdog.ps1` intentionally flashes a repeatedly stalling/resetting
build, then restores normal firmware during cleanup. It is an explicit fault
experiment, not a setup step. Interrupted cleanup requires restoring the normal
build. Review [display safety](DISPLAY.md) and the dated verification reports
before using any stall, transfer-stall, clock or watchdog diagnostic.

## Troubleshooting

| Symptom | Check / next step |
| --- | --- |
| Browser page missing | Run `npm run build` in `apps/browser-sim`; serve its `dist` directory, not repository root |
| Browser changes do not appear | Rebuild after editing source/settings, then reload |
| Phone input unavailable on PC | Expected device policy; use manual/demo controls |
| No Rust gauge window | Expected: CLI is text-only; run the snapshots example or browser app |
| Python cannot launch desktop | Build `desktop-cli`; client expects the root `target/debug/desktop-cli.exe`, not a custom Cargo target directory |
| `No module named serial` | Run with the venv that has PySerial installed |
| Serial access denied | Disconnect the other terminal/monitor; only one owner is allowed |
| Garbled reset text | Boot is 115200, application is 921600; wait for readable READY |
| Duplicate typed characters | Disable terminal local echo; firmware echo defaults on |
| Backspace prints an escape | Firmware has strict framing, not editing; enter a new complete line or use the Python client |
| Red X / missing altitude | Startup has no samples; use `sim preset level`, `sim resume`, and inspect explicit faults in `status` |
| Sweep rejected | Check duration 5..600 and altitude/timing faults; no silent fault clearing |
| Display unexpectedly dark | Inspect `display status`, owner timing, source and latched transfer errors; do not bypass the guard |
| Shifted dial/duplicated arc | Check generated per-VSYNC restart setting and review the scanout report; physical observation is necessary |
| ESP build fails on long paths | Check the short external target/SDK paths and base-Python setup |
| Flash rejects another adapter/backup | Expected guard; identify/provision that board explicitly, rather than removing checks |

If a transport request times out, close/reopen the client and inspect `status`
before further changes; the text protocol cannot correlate delayed replies.
Do not use queued status as operational telemetry. Record reproduction steps,
firmware/toolchain versions and whether a failure is software-reported or
physically observed when investigating a fault.
