# IMU attitude bench checkpoint: v0.2.0-rc.3

Branch: `feature/attitude-gauge`, based on `6a9f16d` / `v0.2.0-rc.2`.
Source checkpoint tag: `v0.2.0-rc.3`. No prebuilt release package.
User accepted smoother IMU motion without blackouts and the final bank-label
spacing. The current face has no visible hold annotation; hold quality and age
remain available through the CLI. Full workspace/all-target tests with the native
MSFS feature, Clippy with warnings denied, formatting and diff checks pass.

## Bank label clearance adjustment (2026-11-02)

Installed ELF SHA256: `03FC8A5CF3116FAD680396A4DFDA1228F5E645A3C77AA503D0A534A2A379AC28`.
Shifted the 30/60 labels three degrees toward level along the outer arc to clear
the major ticks. Preview inspected; all 45 UI tests pass. Board display healthy;
left IMU mode active, COM5 released. User confirmed the final spacing looks good.
The first formulation hit an Xtensa compiler selection error; using a separate
fixed label loop builds successfully with unchanged geometry.

## Earlier bank labels update (2026-11-02)

Installed ELF SHA256:
`F92DDFDCE4F3105CA7EA1EB3670BAA3ADC5EB157042A5FC7A8C28B5907182A2C`
App size 993,296 / 1,048,576 bytes. Added small upright 30/60 labels on both
sides of the fixed bank scale, included in the static cache. Preview inspected;
45 UI tests pass, including cache capacity and triple-buffer equivalence.
`attitude-bank-labels.json`: 324 frames / 10,014 ms = 32.35 FPS in IMU mode,
draw max13 ms / transfer max33 ms, no blanking or expiry. Left display on with
IMU active, COM5 released. Physical label readability awaits user feedback.

## Preceding unannotated hold build (2026-11-02)

Installed ELF SHA256:
`A48D89378B27F145F3F5B25B724AB58C0984C09362B2BC01C97FFC6611FB5E18`
App size 992,928 / 1,048,576 bytes.

User confirmed the preceding 15-sample build was smoother with no blackouts,
but requested removal of the visible hold annotation. The face now retains its
ordinary IMU BENCH caption during holds. CLI hold/quality/age diagnostics and
all other source/guard behavior are unchanged. All 45 UI tests pass, including
pixel equality of a live pose and its held equivalent through all three buffers.

Hardware report `imu-mean15-unannotated-hold.json` passes sensor-off and injected
fault holds, increasing held age, fresh recovery and 10-second presentation check:
**32.34 FPS**, no blanking/expiry/owner fault/I2C error. IMU mode remains active.

## Preceding annotated 15-sample hold build


Previous ELF SHA256:
`0A382462E5C4FAF61979D146958A07E79B77C4B08A6088B9BEFC0519B8420B81`
App size 994,000 / 1,048,576 bytes. Same clocks/cache/stack as below.

At the user's request, 15 usable acceleration vectors are averaged. Rejection,
staleness and sensor-off retain the last usable pitch/roll pair with IMU HOLD.
No prior pair means WAITING FOR IMU; source changes clear retention. Source
quality and real receipt age remain unchanged. Numeric expiry is disabled only
for IMU bench attitude; owner/render/scanout/transfer watchdogs remain enabled.

- Full workspace tests and all-target Clippy with warnings denied pass.
- 45 UI tests include exact pixel equivalence across all three buffers through
  wait/live/hold/recovery. Core checks cover source isolation, aging, duplicate
  receipts and faulted input not updating the retained pair.
- `imu-mean15-hold-rest.json`: sensor-off and injected roll-fault both held the
  exact prior pair for more than 1.2 seconds, with increasing age, backlight on,
  no expiry and fresh-data recovery. The subsequent 10-second observation had
  324 presentations / 10,002 ms = **32.39 FPS**, draw max26 ms / transfer max33 ms,
  no blanking/expiry/owner fault/I2C error. It included 24 raw tilt rejections and
  135 unavailable estimates; despite the filename, physical movement occurred.
- `imu-mean15-sim-regression.json`: SIM missing/stale/fault cases, deliberate
  render/transfer stalls and recovery pass. Ten seconds of animated motion:
  **32.34 FPS**, draw max24 ms / transfer max32 ms, no unexpected blanking/expiry.
- `imu-mean15-hold-before-fault-fix.json` records a caught and fixed defect:
  CLI-injected axis faults previously allowed the retained pair to update.

User reported smoother motion with no blackouts; requested removal of the hold
caption as implemented above. The following sections
preserve earlier ten-sample implementation results and failed candidates.

## Earlier ten-sample evidence

Previous ten-sample ELF SHA256:
`FF0A2BE5AAB839FBF65BEF319268D1F24ADD0172E995EEC5C09751393771C39C`
App size 987,360 / 1,048,576 bytes. COM5 CH343 UART, 921600/8N1/no flow.
CPU/PSRAM/LCD clocks: 240/80/18 MHz. Data cache 32 KiB; renderer stack 20 KiB.

## Scope

Explicit accelerometer-only bench tilt, upright [Z,-Y,X] mounting, ten usable
acceleration vectors averaged before angle calculation. No gyro integration,
calibration, extrapolation, hidden simulation fallback, or operational attitude
claim. Known native Y gyro bias remains unused. The estimator trait and typed
quality/age boundary remain replaceable for future filtering.

Owner handoff: nominal 5 ms. Single I2C worker: 5 ms start-to-start budget,
minimum 1 ms blocking per iteration, no catch-up spinning. This corrects the
prior additional 5 ms sleep after bus work. Numeric freshness is still 100 ms;
invalid raw gravity/geometry clears the filter immediately. History may span less
than 200 ms to tolerate delivery jitter; this does not change the 100 ms limit
on new-output age or the pipeline reset on a 100 ms inter-sample gap.

## Software checks

- Full workspace/all-target tests including native MSFS passed.
- Latest UI: 43 tests pass with development overflow checks and in release mode.
- Pixel equivalence: batched rotated pitch labels versus original individual
  glyph rendering, with/without cache, multiple angles and sizes.
- Three-buffer rendering matches independent full redraw through motion,
  invalidity, view changes, and dimensions including 1x1 and 1024x1024.
- Source isolation, duplicate receipt expiry, recovery, mounting determinant,
  ten-sample startup/recovery, raw rejection and wrap-around angle averaging tested.
- Clippy all targets/MSFS -D warnings, both format checks and git diff check pass.

The board UI package uses release-style integer arithmetic with opt-level 3.
Host development tests retain overflow traps. Pixel slice bounds checks, typed
input validation, explicit deadline arithmetic, watchdogs and two-completion
buffer ownership remain active. See docs/DISPLAY.md for the geometry bounds.
Touch rectangle addition is widened explicitly; pressure increments saturate.

## Hardware evidence

- `imu-attitude-wide-motion.json`: synthetic pitch +/-35 deg and bank +/-40 deg,
  20-second UART-driven exercise, 649 presentations / 20,067 ms = 32.34 FPS.
  Draw max33 ms; transfer max34 ms; no additional expired guard ticks.
  Sensor had only its nine startup warm-up rejections, zero raw rejections/I2C errors.
- `imu-sim-regression.json`: 60-second built-in attitude motion, 1,941 frames /
  60,010 ms = 32.34 FPS, draw max22 ms, transfer max32 ms. No unexpected blanking,
  extra expiry or owner fault. Missing/stale/failed axes and deliberate renderer/
  transfer stalls invalidate/blank as expected and recover.
- `imu-altimeter-regression.json`: existing altitude, barometer, invalid-data,
  placeholder, and intentional-stall checks pass.
- `imu-altimeter-performance.json`: 60-second climb plus one-second hold,
  1,972 frames / 61,004 ms = 32.33 FPS; draw max19 ms, transfer max31 ms.
  Final altitude 10,000 ft, no unexpected blanking, expiry or owner fault.
- Current renderer minimum observed free stack: 5,600 bytes; internal heap ~153 KiB.

Previous ten-sample physical test `imu-attitude-board.json` FAILED the no-expiry requirement:
970 frames / 30,004 ms = 32.33 FPS; draw max26 ms, transfer max37 ms. Three
additional expired guard ticks occurred while the latest owner source was valid.
There were 122 raw gravity/geometry rejections and 327 unavailable estimates
(including filter recovery), with no I2C error or owner timing fault. The user
reports improvement but red X at high rates. Smoothness improved; blanking was
NOT resolved on that build; it was not visually accepted.

`imu-window-margin-transitions.json`: 25 injected fault/recovery pairs at rest,
32.24 FPS, no expiry and no additional unavailable sensor estimates after the
nine startup samples, despite skipped sensor conversions. This supports separating
filter-history retention from new-output freshness.

The synthetic/altimeter regression figures above were captured on the preceding
8B1F25BB build with the same retained renderer. That ten-sample build added the history
margin and expiry-source diagnostic counters. No live MSFS attitude was tested.

## Failed candidates retained as evidence

Unfiltered, five-sample, and initial ten-sample runs met average 30 FPS but
missed numeric deadlines; users reported jitter/red X/blanking. Their JSON files
are retained with distinct filenames. A stationary ten-sample run had no misses,
helping separate movement-related drawing cost from persistent bus failure.

Stage probes identified horizon repainting as the largest cost. Larger-cache,
full-repaint and conditional-pixel-write candidates did not improve the workload
and were reverted. Temporary timing hooks were removed. Batched labels and
release-style UI arithmetic reduced rendering cost; correcting worker cadence
removed window losses in the final unattended exercise. These changes do not
turn acceleration-derived tilt into a dynamic-flight estimator.

Additional rejected experiments: full fault-frame cache, sparse fault cache and
pre-submit fault-priority redraw. They did not eliminate expiry and were removed;
no extra fault-cache allocation or fault-priority hook remains. Their reports are
retained for diagnosis. The retained renderer still uses its original triple-buffer transfer ownership.
The new IMU-only display policy above supersedes its numeric expiry behavior.
