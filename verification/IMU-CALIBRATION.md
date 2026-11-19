# IMU calibration candidate - November 7, 2026

Minor UI follow-up: the installed image now includes fixed horizontal and
vertical center axes behind the calibration content. ELF SHA256 is
**D9E65AD33AC3BF4363FEC019B0CBCF942ED8F665D35845BE3C33E3F4B8A4B440**;
application size **1,280,864 B**. All **109 UI tests** pass, production previews
were inspected, and hardware confirms the modal is presented with the saved
LEVEL generation 2, gyro offsets and leveling quaternion preserved. Evidence:
`artifacts/imu-calibration-axes-board.json`; exact build/source snapshot:
`artifacts/imu-calibration-axes-20261107/`. The earlier test.1 ZIP is unchanged.
The qualification and physical follow-up below describe the preceding image.

Implemented on `feature/aircraft-profile-editor`, retaining the existing dirty
test.12 work. Base HEAD is `188bfb90f9cfee709a8fbc1b358edbf42eb513b4`; this is
not a new commit or published release. The connected SKU 29086 board is flashed
with ELF SHA256 **A130967BBEECA75DF69836A6A2D192F35555D9361A10C414E2DA49EEB12EFB42**.
Application size is **1,280,816 / 4,194,304 bytes (30.54%)**. Flash readback
verified all 1,280,816 application bytes against the packaged image.

## Behavior

Every gauge menu has CALIBRATE. LEVEL collects stationary three-axis gyro bias
and installed upright level. FULL 24 uses eighteen diverse stationary poses for
a robust accelerometer ellipsoid fit, six withheld poses for validation, then
a final upright level capture. Explicit review/SAVE is required; cancelling does
not replace the active calibration. Reviewed RESET uses the same durable-save
handshake. Versioned, mounted, CRC-protected coefficients are committed and read
back by the existing settings worker before activation.

Calibrated Madgwick quaternion fusion includes acceleration rejection,
stationary residual-bias tracking and counter/receipt timing guards. Uncalibrated
or temperature-ineligible input retains the accelerometer average fallback.
Raw sensor CLI values and MSFS telemetry stay unchanged. The modal blocks new
barometer edits, including local CLI edits, and prevents aircraft write actions.

## Automated verification

- `cargo test --workspace --all-targets --features instrument-connector/msfs`:
  **286 passed**. Includes ellipsoid recovery/withheld rejection, complete
  synthetic 24-pose and final-level workflow, corruption/version/thermal/motion
  cases, quaternion motion/linear-acceleration cases, stale jobs, delayed and
  failed storage, UI release/generation guards and three-buffer pixel restore.
- `cargo clippy --workspace --all-targets --features instrument-connector/msfs
  -- -D warnings`: passed.
- ESP-IDF build safety checks and partition validation: passed.
- Physical-board CLI: modal opens and is confirmed presented, hides underlying
  numeric values, blocks edits, cancels/restores the face, rejects stale SAVE,
  and cancels RESET without changing coefficients.
- FULL startup: all six hardware self-test axes passed, configuration and locked
  burst audit matched, then the worker reached Collect and correctly requested
  the target direction. Cancel restored previously-off acquisition.
- An empty RESET record was explicitly saved on a board with no calibration.
  Durable generation 1 was restored after reboot with `calibrated=false`;
  UUID and saved Combined selection were preserved. NVS save took 3 ms.
- Failure-path test verifies failed bus quiescence publishes a failed job,
  releases its reservation, and still permits local reset/review/cancel.
- Final eight-screen board regression: **32.15-32.51 FPS**, DEMO retained across
  every screen, acquisition ownership restored correctly, ten-field synthetic
  MSFS-format stream restored after DEMO, no aircraft controls and **zero display
  blanks**. Renderer minimum stack reserve was **7,296 B** after the complete
  regression; IMU reserve was **5,716 B** during the separate calibration test.
  Final free internal heap was 68,635 B and free PSRAM 1,591,228 B. The regression
  recorded 24 expiry ticks, handled by the existing bounded-hold policy.

Evidence: `artifacts/imu-calibration-tests.log`, `imu-calibration-clippy.log`,
`imu-calibration-hardware.json`, `imu-calibration-persistence.json`,
`imu-calibration-reboot.json`, `imu-calibration-flash-readback.log` and
`imu-calibration-screen-regression.json` (all paths under `artifacts/`).

## Resource corrections during qualification

An early candidate overflowed the IMU stack because large closure captures were
copied through nested pthread/unwind launch frames. Boxing the startup capture
removed those copies; the worker retains a 12 KiB internal stack. The new
calibration modal also enlarged retained frame history. Allocating the three
damage histories once on the heap restored renderer stack headroom without new
framebuffers or per-frame allocations. Render task stack remains 24 KiB and
display-init remains 12 KiB. Early failed images are not the qualified image.

## Physical accuracy acceptance still required

At the original test-bundle qualification, no physical LEVEL or FULL calibration
had been accepted or saved; the board held only an empty reset record. A person must secure it
on a known level support, perform the requested orientations and review results.
Actual final fit stack/timing under a completed physical FULL session and dynamic
reference accuracy remain to be accepted. The math workflow is covered by host
tests; this is not a claim of certified static/dynamic accuracy.

### Subsequent physical LEVEL acceptance

The user subsequently secured the board upright on a level, rigid support and
confirmed hands-off capture. LEVEL completed with **480 accepted samples** at
**32.42 C**. After reviewing the high-bias warning, the user explicitly authorized
SAVE and an at-rest check. Durable calibration **generation 2** is now stored:

- Mounted gyro offsets: **[-0.17233072, -30.234325, -1.6946615] degrees/second**.
- Per-axis measured gyro standard deviations:
  **[0.06457601, 0.09053408, 0.057288453] degrees/second**.
- Upright level quaternion: **[0.9995435, -0.024845013, -0.017193912, 0]**.
- `calibrated=true`, LEVEL only; accelerometer coefficients remain identity.

Madgwick fusion was verified active with valid bench output and the guarded
counter timebase. In the initial short rest check, roll ranged -0.071 to +0.016
degrees and pitch -0.015 to +0.037 degrees. Actual LCD samples were also valid
and near zero. A restart restored the identical saved offsets and quaternion;
the repeated rest check stayed within 0.12 degrees of zero, with no display
blanks. This measures repeatability relative to the accepted fixture zero,
not independent absolute or dynamic accuracy.

The first physical attempt was rejected during large measured motion; the
subsequent rigid-support attempt passed without changing acceptance thresholds.
Evidence: `artifacts/imu-level-rigid-capture-20261107-153955.json`,
`artifacts/imu-level-saved-verification-20261107-154059.json`,
`artifacts/imu-level-display-verification-20261107.json` and
`artifacts/imu-level-reboot-verification-20261107.json`.
The board is left on **Attitude / IMU BENCH**, acquisition on, DEMO off. FULL 24
and motion-reference validation remain outstanding. The original firmware test
ZIP is unchanged and does not contain device-specific NVS calibration data.

The sensor has a previously confirmed roughly +30 dps native-Y stationary
offset, exceeding its stated initial-offset tolerance; the UI warns before SAVE.
Six-axis sensing cannot establish absolute heading, distinguish all sustained
linear accelerations from gravity, or measure absolute gyro scale/cross-axis
alignment and temperature curves from static poses alone. See the
[procedure and method references](../docs/IMU-CALIBRATION.md).

This qualification retains the existing bounded three-miss display hold.
Transient expiry/hold counters are reported honestly; zero display blanks is
distinct from zero late frames. Earlier constrained-CPU MSFS strict zero-expiry
stress remains outside this feature's qualification; no new MSFS flight was run.
