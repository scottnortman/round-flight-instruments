# Touch Kollsman editor - November 1, 2026

Branch: `feature/touch-kollsman`, based on `v0.1.3-altimeter-contrast`.

The portable UI controller implements a half-second hold on the Kollsman window,
release-before-button activation, 0.01 inHg steps, a 29.92 draft-reset button,
Apply, Cancel and a 30-second inactivity timeout. Only Apply changes the
validated application barometer setting; pressure-altitude samples and freshness
remain unchanged. GT911 register handling is a separate `no_std` crate; the
board's one bus worker publishes timestamped input. See [user guide](../docs/TOUCH.md).

## Accepted interaction evidence

- The user confirmed opening the editor, incrementing and applying, with an
  increased indicated altitude. `touch-first-capture.json` records physical
  contact and draft/committed-state transitions.
- The user confirmed both Cancel and 30-second inactivity discard the draft
  and leave 29.92 unchanged. `touch-cancel-timeout-capture.json` records these
  transitions; no sensor bus errors were reported in that run.
- Standard-reset button validation exposed a task watchdog reset. A subsequent
  build exposed a distinct I2C interrupt watchdog reset without a touch gesture.
  These failures are retained in [queue investigation](TOUCH-QUEUE-RESET.md)
  and [I2C investigation](TOUCH-I2C-RESET.md). Earlier successful interactions
  do not constitute acceptance of either rejected build.

## Software checks

- 63 workspace Rust tests passed, including bounded queue contention, input
  processing under UART backpressure, GT911 report/ACK validation, gesture
  cancellation, draft-only reset, application offset behavior, and raster
  equivalence with the overlay across three reused framebuffers and viewport sizes.
- Workspace Clippy with warnings denied and formatting passed. Firmware Clippy
  passes with the existing display `field_reassign_with_default` style lint allowed;
  its first strict run flagged two pre-existing C-struct initializers.
- 27 Rust snapshots generated; 480-pixel overlay preview inspected and included
  in the user guide. Browser reference was not changed by this feature.
- Desktop protocol acceptance passed: 56 requests, maximum response 47 ms,
  maximum owner gap 21 ms (`touch-desktop-cli.json`).

## Final hardware validation

New master-transport ELF SHA256:
`EC3019F9F9FB447D96EE9781125F81DEA33C6C34B4B25911939103C7263BBD3A`.
Application: 891440 / 1048576 bytes (85.01%). Exact ELF, bootloader, SDK config,
partitions, lockfile and hashes retained under ignored `artifacts/touch-master-20261101`.

- IMU transport/control: 103 captured readings over 10 seconds, enable/disable
  and restart passed, zero sensor errors (`touch-master-imu.json`). Existing
  stationary Y-gyro bias remains about +30 degrees/second.
- UART acceptance: 56 requests passed, maximum response 79 ms, maximum owner gap
  31 ms (`touch-master-uart.json`). Character echo, CRLF, escaped controls and
  overflow rejection passed (`touch-master-echo.json`).
- Touch CLI bounds, polling and setting preservation passed with zero touch
  errors (`touch-master-cli.json`).
- Display invalid/stale/failed input, renderer/transfer stall blanking and
  recovery passed (`touch-master-display.json`). These deliberately injected
  blanking and warning states; they are not spontaneous display failures.
- 60-second climb with IMU enabled and touch polling: 1972 presentations in
  61019 ms, **32.32 FPS**, maximum measured drawing 27 ms and transfer observation
  45 ms. No owner/display failure (`touch-master-takeoff.json`).

- 1200-request traffic test passed, maximum response 94 ms (`touch-master-stress.json`).
  Subsequent snapshots showed 5086 IMU samples and 7418 touch polls with zero
  errors (`touch-master-shared-bus.json`).
- The user confirmed 29.92 reset plus Apply returned the gauge to 4520 ft without
  reboot, then reported successful additional ad hoc testing. The capture records
  draft changes while the committed value stayed 30.01, followed by Apply to 29.92.
  `touch-master-standard-capture.json` is marked failed because its original test
  required IMU quality `valid`; handling the board produced an explicitly flagged
  `saturated` sample, with zero sensor errors and no reset. This was a too-strict
  transport-test criterion, not a successful full 180-second run. The script now
  records saturation separately while still rejecting stale/failed sensor data.
- The user then requested a 500 ms default instead of 2000 ms. All 15 UI tests
  and UI Clippy pass after this change, including no opening at 499 ms and
  opening at 500 ms. Release-before-action and draft/Apply safeguards remain.
  Final ELF SHA256: `ABAA50C5703B296CCFE969AE5643841B146AED380C412CE8F4877923B3902FA8`.
  Application size is unchanged. Final build artifacts are archived under
  `artifacts/touch-500ms-20261101`.
- Final 500 ms firmware booted with `hold_ms=500` before any setting command.
  Touch CLI checks passed (`touch-500ms-cli.json`). A 60-second capture with IMU
  enabled passed (`touch-500ms-capture.json`), including observed physical opening,
  adjustment, Apply, reset-draft and discard interactions. No reset, timing/display
  failure or bus error was observed. IMU was then disabled and COM5 released with
  echo restored (`touch-500ms-final.json`); the user's barometer setting was preserved.

Measurements are observations on this board, not worst-case execution guarantees.
The IMU remains diagnostic-only with unresolved gyro bias. Physical bus-disconnect
fault injection and aircraft validation have not been performed.


## Press feedback follow-on

The Kollsman interior turns deep blue (`#005A9C`, quantized to RGB565) during
an accepted opening hold, with regular white digits antialiased against blue.
Release/abort restores black, and opening the editor ends the highlight.
`Controller::window_pressed` derives the visual state from the existing hold;
there is no independent gesture timer or new I2C read. The snapshot carries one
boolean and the renderer draws only the window dynamically beneath the hands.
This state is deliberately absent from the static backdrop key. Existing dirty
regions restore the normal window separately in each reused framebuffer.

- All 65 workspace Rust tests pass, including 17 UI tests. New checks exercise
  release/abort cleanup, exact localization of changed pixels and restoration
  across all three framebuffers. Invalid-data warnings suppress the highlight.
  Existing incremental/full raster comparisons now alternate pressed state.
- UI Clippy with warnings denied passes; firmware build and generated safety
  settings / new-I2C symbol checks pass. Thirty PPM snapshots generated; the
  480-pixel pressed preview was inspected and added to the touch guide.
- The user confirmed prompt blue feedback, restoration to black on early release,
  and normal popup opening/cancellation without artifacts.
- Installed ELF SHA256:
  `12EFE1F0A3C4B410D42B9D8B275FADC4CAC2AA067D61BC65B0C61CD7F4E4FB14`.
  Application 892048 / 1048576 bytes (85.07%). Exact firmware artifacts retained
  under ignored `artifacts/touch-press-feedback-20261101`.

- The first 60-second performance run overlapped physical popup/setting tests.
  It measured 1783 presentations / 61010 ms = **29.22 FPS**, maximum drawing
  125 ms, transfer observation 137 ms. The test's endpoint assertion failed
  because the barometer had changed to 30.01, making indicated altitude
  10083.138 ft instead of the script's assumed 10000 ft at standard pressure.
  No display fault or blanking was reported. This is not an isolated measurement
  of the blue highlight: it includes editor rendering and barometer-driven
  backdrop rebuilds. Sustained 30 Hz during intensive editing is not established.
  Evidence: `touch-press-feedback-takeoff.json` (retained as failed).

- Controlled 20-second repeat at 29.92 with IMU running passed: 679 presentations
  / 20999 ms = **32.33 FPS**, maximum drawing 27 ms, transfer observation 43 ms,
  owner gap 30 ms, no reported display/timing fault or blanking. This matches
  the prior normal-gauge 32.32 FPS baseline. Evidence:
  `touch-press-feedback-baseline.json`. The user's 29.73 setting was restored,
  IMU disabled, echo restored and COM5 released (`touch-press-feedback-final.json`).
