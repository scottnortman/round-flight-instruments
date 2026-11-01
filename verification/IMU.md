# IMU diagnostic verification — 2026-10-31 (America/New_York)

Rust IMU diagnostics are installed on the verified Waveshare SKU 29086, UART
COM5. The device responded at **0x6B**, WHO_AM_I **0x05**, revision **0x7C**.
The application remains a simulation gauge source with display/backlight off.
Final tests left IMU acquisition off and simulation paused.

## Results

Later update: [estimator hooks and read-format audit verification](ESTIMATOR.md).
The measurements below remain historical evidence; the offset is still unresolved.

| Check | Evidence/result |
| --- | --- |
| Driver/service/core/protocol tests | 26 passed, including signed scaling, identity/config rejection, bounded command timeout, locked-read failure, duplicate/stale data, counter wrap, tilt geometry, self-test thresholds, and blocked sensor I/O while the owner continues |
| Desktop Clippy | Workspace/all-targets passed with warnings denied |
| ESP32-S3 build/flash | Locked build and generated watchdog settings verified; espflash verification enabled |
| Physical check-alive | Passed all six axes, including an immediate repeat with distinct completed operation IDs |
| Acquisition | Final 10-second capture: 84 host observations, device/worker counters advancing, zero reported driver errors; enable/off/re-enable verified |
| Owner scheduling | Maximum observed update gap 31 ms during sensor checks; no owner timing fault |
| Existing CLI regression | 56 requests passed on each target; max response 63 ms desktop, 94 ms UART; the unchanged 30-second timeout was already tested in the prior baseline |
| Human motion check | Completed after user confirmed readiness: 15-second recording, 125 valid readings, zero rejections/errors in the completed recording; all gyro axes responded. Absolute scale and axis alignment remain unverified. |

Evidence files: [final capture](imu-final.json), [self-test](imu-selftest-final.json),
[repeated self-test](imu-selftest-repeat.json), [desktop regression](desktop-with-imu.json),
[UART regression](uart-with-imu.json). The earlier [initial capture](imu-initial.json)
is retained. Capture `passed` describes driver transport/control, not calibration.

Representative self-test responses:

| Axis | Acceleration response (g) | Gyro response (degrees/second) |
| --- | ---: | ---: |
| X | 0.8184 | 1093.19 |
| Y | 0.7075 | 908.44 |
| Z | 0.6533 | 1051.69 |

QST check-alive thresholds are magnitude >0.2 g and >300 dps respectively.
These are internally stimulated readings, not the board's actual motion.

## Unresolved finding: large Y gyro reading

The final capture had mean acceleration norm **0.9888 g**, with each acceleration
axis spanning less than 0.005 g. Mean raw gyro rates were approximately
**X -1.59, Y +30.12, Z -0.08 degrees/second**, at die temperature **29.79 C**.
The earlier capture also showed approximately +30.19 degrees/second on Y.

The user subsequently confirmed the earlier capture was taken while the board
was sitting still. Its +30.19 degrees/second Y result exceeds the Rev A
datasheet's stated initial offset tolerance (+/-10 dps).
Successful check-alive does not resolve bias or prove rate accuracy.
No offset was subtracted and these values are not fed to gauges. Before fusion:
repeat controlled stationary measurements, compare controlled rotation/axis signs, investigate
configuration/sensor calibration and temperature effects, and verify any proposed
correction against independent observations. Do not hide this finding with an
automatic zero operation.

The first board attempt exposed a 5 ms worker sleep busy-waiting under ESP-IDF's
10 ms tick and triggered its idle-task watchdog. The decoded backtrace pointed
to that sleep. A 20 ms yielding interval corrected it; subsequent sensor and CLI
runs completed without resets. This is recorded as a resolved integration issue.

## Artifacts and limits

### Motion capture and plots

The first motion attempt stopped on `ERR imu_saturated` and is retained in
[its report](imu-motion.json). A follow-up recorder retained timing and would
show rejected readings as gaps; it completed with 125 valid observations and
no rejected readings. No numerical correction or smoothing was applied.

The completed recording's gyro ranges (degrees/second) were X -107.79 to +94.48,
Y -22.90 to +82.41, and Z -52.50 to +58.58. Device-driver errors remained zero,
maximum sensor-worker gap was 30 ms, and maximum owner gap remained 31 ms.
Acquisition was disabled at completion. This verifies motion response, not
calibrated rate accuracy or independent axis/sign alignment.

- [Four-panel plot](assets/imu-motion-20261031.png)
- [PDF plot](assets/imu-motion-20261031.pdf)
- [CSV observations](assets/imu-motion-20261031.csv)
- [Raw recording and command transcript](imu-motion-capture.json)

Plot time is host receipt time, about eight observations/second; it is not the
112.1 Hz device ODR. The separate stationary baseline is `imu-initial.json`.
Roll is shown in its wrapped +/-180 degree representation, with line breaks at
wraps and markers retaining all valid observations. Tilt remains accelerometer-only.

The current application occupies 730,080 bytes of the 1 MiB app partition. Its
ELF/config/bootloader/lockfile are retained under
`artifacts/imu-diagnostics-20261031/` with a checksum manifest. Earlier console
firmware and both verified factory backups remain preserved.

The driver fixes full-scale/ODR settings, does not persist calibration, and does
not implement fusion, magnetometer heading, air data, or gauge-source switching.
Timestamp age is receipt age, not proven end-to-end sensor latency. No touch/
display/PSRAM contention or complete physical bus-fault test has been performed.
The desktop blocked-bus test verifies software isolation, not every electrical
failure mode. The hardware motion check remains reproducible with the command
in [the IMU guide](../docs/IMU.md).
