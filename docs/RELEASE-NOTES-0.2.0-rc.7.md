# Round Flight Instruments v0.2.0-rc.7

Prerelease for Waveshare **ESP32-S3-Touch-LCD-2.8C, SKU 29086** and Windows x64
MSFS 2020. The user reported the live MSFS test as **"all working" on November 3,
2026**. This records the tested configuration; no additional aircraft list or
detailed live-test logs were supplied with that report.

## Included

- Altimeter, Attitude and Turn Coordinator, with a touch menu to switch faces.
  All six instruments have transport support; Airspeed, Heading and VSI faces
  are still disabled.
- Native Windows tray connector, startup at sign-in, automatic reconnection,
  simulator identity in Status, and explicit active-gauge barometer writeback.
  Hidden gauges remain read-only.
- Responsive retained menus, blue pressed feedback, short chirps, configurable
  3-second center hold and 0.25-second pressure-window hold.
- Corrected T2 data-age accounting, bounded Attitude/Turn frame reuse during
  brief source interruptions and 24-line RGB buffering. Real data ages, explicit
  failure handling and independent display/owner guards remain in place.
- A 4 MiB application slot and one shared menu cache, leaving room for future
  gauge faces. The exact tested firmware occupies 1,051,744 bytes and retains
  5,131,000 bytes of free PSRAM in the measured configuration.

## Downloads and installation

1. `round-instruments-v0.2.0-rc.7-waveshare-29086.zip`: firmware, validated
   flashing helper and instructions. **Back up your own board before flashing.**
2. `round-instruments-0.2.0-rc.7-setup.exe`: unsigned per-user Windows tray
   installer, including unmodified **SimConnect.dll**, its Microsoft license and
   runtime notice. The maintainer confirmed permission to include it. No separate
   SDK, Python or Rust is required on the MSFS PC.
3. `SHA256SUMS.txt`: checksums for both files.

Follow the [step-by-step guide](https://github.com/scottnortman/round-flight-instruments/blob/v0.2.0-rc.7/docs/PRERELEASE-0.2.md).
The installer upgrades in place; exit the tray app first, with no uninstall.
The Windows installer does not flash the board. Existing tested rc.6 firmware
and private preview 6 installations are compatible and need no update.

The firmware is unchanged from the tested rc.6 image:

- ELF SHA256: `EDBDDEEA394D85DF810FB1B84D3DB2D2E214D81A6DEA99CDB71C15656047C5B5`
- Application SHA256: `E8FDD9AC953C0B8E9379FBB44A3489C9C63A94E7134E27FEEB27783B70C89C0F`

## Verification and limits

The preceding firmware qualification passed 203 Rust tests, strict Clippy,
13 native transport/recovery stress cases, 50 rapid switches, heap stability,
display fault guards and the final physical touch/barometer check. Completed
presentation rates were 31–32 FPS in the measured workloads. The final user
check reported normal controls without freezing, sustained beep or reset.
See the [verification record](https://github.com/scottnortman/round-flight-instruments/blob/v0.2.0-rc.7/verification/CONTINUITY.md).

MSFS 2024, DCS and every aircraft variant are not qualified. Brief source gaps
may pause a pose; sustained missing/stale/failed data shows invalid-data warnings
and never falls back to a demo. The onboard IMU remains an explicit bench mode.
Settings are volatile. This is simulator/bench software, not certified flight
equipment. Main has not been merged as part of this prerelease.
