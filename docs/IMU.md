# QMI8658C driver and CLI

The board now reads its physical IMU through a Rust driver. Acquisition is
explicitly enabled and separate from the simulated gauge source. Fusion,
mounting alignment, bias calibration, and gauge-source selection remain future
work. Raw gyro errors cannot silently become gauge indications.

## Control

```powershell
& D:\rfi-e\python_env\idf5.5_py3.11_env\Scripts\python.exe scripts/cli.py --port COM5
```

| Command | Effect |
| --- | --- |
| `imu on` | Queue reset, identity/configuration checks, then acquisition |
| `imu off` | Queue disable and clear the published sample |
| `imu status` | Identity, state, quality, counters, age, errors, self-test results |
| `imu read` | Latest fresh raw counts, g, degrees/second, die temperature, bench tilt |
| `imu selftest` | Queue electrostatic check-alive; finishes acquisition off |
| `watch imu` | Python client command: poll readings for 10 seconds; enable first |

On/off/self-test ACKs contain `request=N`. Wait for status `completed=N` before
treating the operation as complete. IDs are boot-local and prevent confusing an
old self-test result with a new one. Initialization resets diagnostic counters
and self-test results. `imu on` is idempotent while running; to recover stale
data, complete off then on. Errors require explicit retry. There is no automatic
simulation fallback. The desktop runner returns `ERR imu_unsupported_target`.
At boot no samples are acquired; an unprobed off state does not prove sensor
power-down across an MCU-only reset.

## Data meanings

All axes use the **sensor-native frame**, without aircraft/screen alignment or
bias correction. Acceleration uses 8192 counts/g at +/-4 g; gyro uses 128 counts
per degree/second at +/-256 dps. Temperature is signed raw / 256 degrees C and
measures the die, not ambient air.

The 24-bit device `counter` wraps. `sample` counts accepted samples; `skipped`
counts device samples not collected by polling. This is latest-sample diagnostics,
not lossless acquisition. Duplicates never refresh age; backwards jumps fail
acquisition. `age_ms` starts when a fresh burst is received and excludes unknown
sensor conversion/filter latency. Age >=100 ms returns `ERR imu_stale` without
numeric values. Off/missing/testing/failed/saturated reads also return errors.

`quality=valid` means fresh and non-saturated, **not calibrated accuracy or flight
suitability**. Calibration is explicitly `none`. Tilt is accelerometer-only:
pitch = atan2(-ax, sqrt(ay²+az²)); roll = atan2(ay,az), in degrees. It is withheld
outside 0.8–1.2 g or at the roll singularity. Near-1 g does not prove stationarity;
acceleration can still produce plausible wrong tilt.

Self-test applies the chip's internal stimulus and publishes no normal samples.
QST thresholds are absolute response >0.2 g on each accelerometer axis and >300
dps on each gyro axis. A pass does not establish zero-rate bias, scale accuracy,
mounting alignment, or calibration.

## Implementation

The worker also owns a replaceable [attitude estimator](ATTITUDE-ESTIMATOR.md).
`imu estimator` inspects bench tilt and uncertainty metadata. `imu audit` checks
configuration readback and independent byte/burst agreement; poll `imu status`
for completion and evidence. Neither command calibrates the gyro or feeds gauges.

`crates/qmi8658` is dependency-free, `no_std`, and forbids unsafe code. Its bus
adapter must bound transactions. The board uses I2C0, SDA15/SCL7, 400 kHz, with a
10 ms transaction timeout subject to RTOS granularity. Future touch/RTC support
must share this bus owner. Identity probes only 0x6B/0x6A, requires ID 0x05,
checks reset completion, and reads back configuration.

Configuration is little endian, auto-increment, +/-4 g, +/-256 dps, 112.1 Hz
6DOF ODR, no extra LPF, with 250 ms startup settling. QST Rev A's locked-read
sequence disables AHB clock gating via acknowledged CTRL9, enables SyncSample,
locks through STATUSINT, waits/checks the lock, then bursts 0x30–0x40 through
GZ_H to release. Failed transfers are discarded and release/disable is attempted.
All waits have finite bounds; a broken bus cannot confirm physical shutdown.

`crates/imu-service` owns the driver in a worker with a two-slot command queue.
The state owner only uses nonblocking queue/lock operations. Register I/O runs
outside the snapshot lock. Contention returns `ERR imu_busy`. A blocked-bus
test verifies the console/state owner continues.

Worker sleep is 20 ms, with approximately 30 ms observed intervals. The first
5 ms delay triggered the idle watchdog: ESP-IDF `usleep` busy-waits below its
10 ms RTOS tick. A decoded backtrace identified that delay; the yielding interval
fixed it and board operation was retested.

## Reproduce checks

```powershell
cargo test --workspace --locked
& D:\rfi-e\python_env\idf5.5_py3.11_env\Scripts\python.exe scripts/imu-selftest.py --report verification/imu-selftest-local.json
& D:\rfi-e\python_env\idf5.5_py3.11_env\Scripts\python.exe scripts/imu-check.py --seconds 10 --report verification/imu-local.json
& D:\rfi-e\python_env\idf5.5_py3.11_env\Scripts\python.exe scripts/imu-check.py --seconds 15 --motion --report verification/imu-motion-local.json
```

Gently tilt/rotate during the motion capture. Tests leave acquisition off.
Transport/control passes do not imply calibrated accuracy; the gyro-offset
check is conditional on stationarity. Review [measured results and unresolved
findings](../verification/IMU.md) before gauge integration.

To record a plot-ready session that preserves rejected readings as events:

```powershell
& D:\rfi-e\python_env\idf5.5_py3.11_env\Scripts\python.exe scripts/capture-imu.py --seconds 15 --report verification/imu-motion-capture.json
& "$env:LOCALAPPDATA\round-flight-tools\python-env\Scripts\python.exe" scripts/plot-imu.py --capture verification/imu-motion-capture.json --stationary verification/imu-initial.json --stationarity-confirmed --prior-saturation --output output/imu/imu-motion-20261031
```

The plot command writes PNG, SVG, PDF, CSV, and summary JSON. Confirmation and
prior-saturation flags describe this recorded test; omit them for future runs
unless those facts are established. Matplotlib is installed in the isolated
support Python environment. The firmware was unchanged for this capture.

## References

Written from the register specification, with demo pins/scales cross-checked.
No proprietary fusion library was imported. Downloads remain in ignored `tmp/imu/`.

- [Waveshare demo](https://files.waveshare.com/wiki/ESP32-S3-Touch-LCD-2.8C/ESP32-S3-Touch-LCD-2.8C-Demo.zip),
  SHA-256 `f93eff03664f7dded6f91f9c362fda0d7c870991b4fa99ccbcfdd3e57ce39cf5`.
  Under `ESP-IDF/ESP32-S3-Touch-LCD-2.8C-Test/main/`, the relevant files are
  `QMI8658/QMI8658.c`, `I2C_Driver/I2C_Driver.h`, and `LVGL_UI/LVGL_Example.c`.
  The latter labels a widget “Board angle” but formats acceleration X/Y/Z in g.
  This describes published source, not proof of the exact factory binary.
- [QST Rev A datasheet](https://www.qstcorp.com/upload/pdf/202210/13-52-27%20QMI8658C%20Datasheet%20Rev%20A%20%281%29.pdf),
  SHA-256 `9887d8827b1769dff31ad5d5c0c4672458d593af5309aa0ed183d38440f7defd`.
  Sections 5/13/15 specify registers, locking, and self-test. This driver avoids
  the demo's unbounded CTRL9 wait and reserved-bit writes.
- [Analog Devices AN-1057](https://www.analog.com/en/resources/app-notes/an-1057.html)
  explains acceleration limitations of gravity-based tilt.
