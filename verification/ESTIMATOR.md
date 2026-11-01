# Estimator extension verification - 2026-10-31 America/New_York

Implemented and flashed the replaceable attitude-estimator boundary, initial
accelerometer-only bench tilt adapter, `imu estimator`, and `imu audit`.
This adds hooks for a later Kalman implementation; no Kalman fusion or gyro
bias correction is enabled. See [contract and usage](../docs/ATTITUDE-ESTIMATOR.md).

## Checks and evidence

| Check | Result |
| --- | --- |
| `cargo test --workspace --locked` | 34 tests passed: timing, source/calibration/frame resets, stale/invalid/saturated input, invalid estimator output, covariance constraints, independent byte reads, and existing baseline behavior |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| ESP32-S3 build | Locked build passed; generated stack, UART, flash, and watchdog settings verified |
| Flash | Known COM5 adapter and factory backup verified; espflash completed successfully with verification enabled |
| Board estimator lifecycle | Off hides angles; on yields bench tilt; off/re-enable works; raw gyro, yaw absence, unknown covariance/bias, and unchanged simulation source/settings verified |
| Read audit | Three matching configuration/byte comparisons, no driver errors; Y counts 3829/3830/3832 independently matched the corresponding burst |
| Owner timing | Maximum observed gap 32 ms, no timing fault; sensor gap reached 60 ms while audit consumed its separate diagnostic sample |
| CLI regression | 56 requests passed on desktop and 56 on COM5; maximum responses 62/94 ms respectively; unchanged 30-second partial-line timeout was already tested in the baseline |
| Physical self-test | All six axes passed; this is check-alive, not bias/gain calibration |

Evidence: [board estimator/audit transcript](estimator-hooks.json),
[desktop regression](desktop-estimator-hooks.json),
[UART regression](uart-estimator-hooks.json),
[physical self-test](selftest-estimator-hooks.json).

## Offset investigation remains open

The independently confirmed stationary capture remains a failing reproduction:

```powershell
& D:\rfi-e\python_env\idf5.5_py3.11_env\Scripts\python.exe scripts/check-gyro-offset.py verification/imu-initial.json
```

Exit 1: gyro mean degrees/second approximately [-1.60518, +30.19298, -0.05772],
against the datasheet's +/-10 degrees/second initial zero-rate tolerance.
This script assumes independent stationarity confirmation; small acceleration
variation alone does not establish it. The confirmed earlier capture is retained.

Three hypotheses guided the new diagnostic:

1. Read-format/burst indexing error: compare individually addressed bytes and
   the locked burst. All eleven compared bytes agreed in three hardware audits.
2. Configuration mismatch: reread CTRL1/2/3/5/7. Hardware returned exactly
   [0x40, 0x16, 0x46, 0x00, 0x83], including little-endian format and 256 dps range.
3. A sensor-side rate offset: the raw Y counts remain high through both read
   paths with the expected configuration. The new 20-observation mean was
   [-1.71055, +29.91524, -0.08164] degrees/second. No new independent human
   stationarity confirmation was obtained for this capture.

The results make a simple burst-format or configured-scale mismatch less likely.
They do not establish a specific physical defect or calibration cause. No offset
is subtracted, no numerical fix is claimed, and IMU estimates cannot drive gauges.
Further controlled stationary/temperature/rotation measurements are needed
before validating any bias correction or integrating gyro rates.

## Reproducible firmware snapshot

Archive: `artifacts/estimator-hooks-20261031/`, with ELF, matching bootloader,
partition CSV, SDK configuration, Cargo lock, and hash manifest. Previous firmware
and both verified factory-backup copies remain intact.

- Application size: 738,640 of 1,048,576 partition bytes (70.44%).
- ELF SHA-256: `659f1e4c84eae9349636e0af488dd1fb1eec22fe52028df76004700bc0bb08c0`.
- Target: ESP32-S3, MAC `a0:85:e3:e1:4a:74`, UART COM5.
- Final state after checks: IMU off, simulation paused, attitude selected,
  barometer setting 30.12 inHg, display/backlight off.
