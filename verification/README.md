# Console baseline verified — 2026-10-31 (America/New_York)

Repository/documentation milestone: [initial Git baseline](GIT-BASELINE.md).
Use the [canonical project README](../README.md) for current status; the
console-only observations below are historical.

Latest installed visual change: [larger black-on-white altitude readout](ALTIMETER-READOUT.md).
The 60-second sweep measured 32.33 FPS; the user accepted the readout and motion,
then requested a tighter lower panel margin.

Latest installed architecture: [30 Hz rendering pipeline](RENDERING-30HZ.md).
The 60-second sweep measured 32.32 FPS with the IMU running and was visually accepted.

Previous performance baseline: [animated altitude sweep and faster rendering](TAKEOFF.md).
The user confirmed smoother, clean movement; the full climb measured 15.47 fps.

Current verified fix: [physical scanout corruption](DISPLAY-SCANOUT.md).
The user confirmed the original replay is clean after disabling forced per-frame
restart; earlier software health results did not detect the photographed artifacts.

Historical console-only baseline. The current [first-gauge verification](DISPLAY.md)
records the installed display firmware. [IMU diagnostics](IMU.md) and
[estimator hooks](ESTIMATOR.md) retain sensor evidence and unresolved findings.
The original console artifacts/backups below remain preserved.
The latest installed firmware adds [character echo](CLI-ECHO.md) to
[921600-baud UART operation](UART-921600.md).

Normal console firmware is installed on Waveshare SKU 29086, ESP32-S3 silicon
revision 0.2, MAC a0:85:e3:e1:4a:74, UART COM5. The watchdog fault-test build was
removed and normal firmware restored. The user confirmed the physical display
is dark. The last acceptance run left simulation **paused**, attitude selected,
barometric setting 30.12 inHg, all sample values stale, and no timing fault.
Reboot returns to default settings with all quantities missing.

| Check | Result |
| --- | --- |
| Rust unit tests | 15 passed: 7 core, 6 protocol, 2 runtime |
| Desktop Clippy | Passed, all workspace targets, warnings denied |
| Formatting | Root workspace and separate firmware checked |
| ESP32-S3 build/link | Passed using locked dependencies and ESP-IDF 5.5.5 |
| Generated SDK settings | Verified 16 KiB main stack, 115200 UART, 16 MiB flash, initialized one-second task watchdog with panic |
| Desktop acceptance | 57 requests passed, including 30-second line timeout; max response 62 ms, max owner gap 25 ms |
| Final normal-firmware UART acceptance | 57 requests passed, including line timeout; max response 109 ms, max owner gap 31 ms |
| Hardware watchdog injection | Three-second owner stall produced `task_wdt` failure naming `main`, panic, reboot, then `READY`; normal build restored |
| Factory recovery image | Primary and secondary copies still match the recorded SHA-256; factory restore not tested |

Evidence: [desktop transcript](desktop-cli.json), [final UART transcript](uart-cli.json),
[watchdog capture](watchdog-uart.log). The initial UART run before the watchdog
test also passed (110 ms max response, 30 ms max owner gap). Times are observed
values under these tests, not worst-case guarantees. The owner aims at 20 ms;
the RTOS/host scheduler quantizes and delays actual wakeups. These tests remained
below the 100 ms timing-fault threshold.

The watchdog test exercises the opt-in `watchdog-self-test` Cargo feature,
which blocks the owner health callback at tick 100. The normal build has that
feature disabled. It proves recovery from this injected owner stall, not every
possible driver/interrupt/display failure. If the test script is interrupted
before cleanup, run `scripts/Flash-Console.ps1` to restore the normal build.

## Retained artifacts

The exact normal firmware ELF, matching ESP-IDF bootloader, partition CSV,
generated SDK config, dependency lockfile, and SHA-256 manifest are retained in
`artifacts/console-baseline-20261031/` (excluded from version control).

- Normal ELF: 14,816,768 bytes, SHA-256
  `2932e860d64b6c6e80877848c216a132d2ac1ad424bd47cb61663f104082ca1b`.
- Flash application: 680,256 bytes in the 1 MiB factory application partition.
- Factory image: 16,777,216 bytes, SHA-256
  `3cbea10793eb9cea69e0a2b9bb91df25d1fb6b8b4622f49cd8dae071a77ff2ba`.
- Primary factory image: `backups/factory-20261031-213635/`.
- Secondary factory image:
  `C:/Users/scott/Documents/RoundFlightInstruments/Backups/factory-20261031-213635/`.

Flashing used the generated ESP-IDF bootloader and its matching single-app
partition CSV, with espflash's verification enabled. No eFuses were changed.
The original backup README contains full-image recovery instructions.

## Scope still outstanding

No gauges, touch controls, sensor acquisition, PSRAM/display stress measurements,
settings persistence, or faster transport are implemented in this stage.
Pressure altitude is not barometrically corrected. The text interface is for
bench simulation and control, with no checksum/session/sequence guarantee.
The next implementation stage is altimeter/attitude math and rendering with
independent presentation health, followed by touch selection. Full stack/heap,
bus-load, rendering stalls, and visual fidelity checks belong to that stage.
