# Attitude estimator extension boundary

The board runs a replaceable estimator on the IMU worker. `BenchTilt` provides
unfiltered accelerometer tilt; the board now selects `AveragedTilt`, a 15-sample
acceleration average, for explicit IMU attitude bench mode. There is **no Kalman
filter, gyro integration, bias subtraction or absolute heading**. The confirmed
large stationary Y gyro offset remains unresolved and is not integrated.

## Architecture and extension point

`qmi8658::Sample` retains raw counts. `imu-service` converts a newly accepted
sample into an `attitude_estimator::Observation`, then calls `Pipeline<E>`.
The dependency-free `attitude-estimator` crate is `no_std` and forbids unsafe
code. It can be tested on desktop and used with other sensor/board adapters.
The core application remains independent of estimator implementation and RTOS.

Implement `AttitudeEstimator::{name, reset, update}` and supply the implementation
through `Service::start_with_estimator(bus, estimator)` to replace bench tilt.
No driver, UART command definition, or future gauge renderer needs to know
the filter's internal state dimensions. A later Kalman implementation owns its
state vector, full covariance P, process model/noise Q, measurement model/noise R,
innovation gating, and bias estimation. The public 3x3 covariance fields describe
specific measurement/output quantities; they do not constrain internal P.

`update` and `reset` must be bounded synchronous math without I/O, blocking,
allocation, or scheduling. They run on the sensor worker, outside the shared
snapshot lock. The current worker has an 8192-byte stack: size and measure any
new filter's stack/state and timing before enabling it on hardware. An async
adapter can later own the same pure estimator interface.

## Data contract

| Field | Meaning |
| --- | --- |
| `received_us` | Monotonic worker receipt time in microseconds, fixed origin for the worker lifetime; CLI reads never refresh it |
| `acquired_us` | Optional acquisition time in the **same clock domain**; currently unknown, never synthesized from receipt time |
| `sensor_counter` | Optional device sample counter; QMI adapter checks duplicates, regression and 24-bit wrap before invoking the pipeline |
| `source_epoch` | Changes on source reinitialization; adapters must also advance it when calibration parameters or alignment change, including verified-to-verified changes |
| `frame` | Native sensor axes or body forward/right/down; the board supplies the verified upright mounting to the estimator; raw CLI data stays native |
| `quality`, `calibration` | Input freshness/fault/saturation and independently verified calibration status; fresh does not mean accurate |
| `accel_g`, `gyro_dps`, `temperature_c` | Measured values with explicit units, unchanged by the estimator; SI conversion helpers are available |
| Measurement covariance | Optional full symmetric 3x3 matrices in g squared or (degrees/second) squared; `None` means unknown, never zero confidence error |
| `roll_pitch_deg` | Optional **[roll, pitch]**, degrees, respective bounds +/-180 and +/-90 |
| `yaw_deg` | Optional yaw, +/-180 degrees; unaided six-axis data must not claim absolute heading |
| `quaternion_wxyz` | Optional unit quaternion, order w,x,y,z; the filter must document its reference frame and rotation convention |
| Attitude covariance | Optional 3x3 small-angle error covariance in radians squared; not Euler-degree variance |
| Gyro bias and covariance | Optional bias in radians/second and covariance in (radians/second) squared |

Covariance construction checks finite, symmetric, positive semidefinite matrices
using principal minors in f64. It rejects negative minors conservatively rather
than tolerating small negative values through an absolute epsilon. Nearly
singular matrices affected by rounding may be rejected; a filter must handle
that as invalid/unknown uncertainty, not unwrap blindly.

The pipeline passes actual **receipt interval** seconds to `update`. Zero marks
initialization after reset. Current acquisition is latest-sample polling, not
lossless: skipped device counts are exposed. Receipt intervals and nominal ODR
are not a substitute for verified sample timing. A high-rate fusion stage must
establish acquisition timing, missed-sample behavior, anti-aliasing, and noise
parameters before relying on integrated rates.

## Invalidity and lifecycle

The pipeline resets for source/frame/calibration changes, invalid inputs, invalid
outputs, and receipt intervals <=0 or >=100 ms. Known acquisition times must be
fresh, no later than receipt, and advance within an epoch. Duplicate counters
are rejected; adapter-specific wrap/regression checks remain the adapter's job.
Gaps reject the triggering sample; the next valid sample initializes with dt=0.
Saturation and nonfinite data do not enter the estimator. Returned angles and
quaternions are checked for finite/range/unit-norm validity and frame consistency.
Unavailable output contains no numeric estimate, covariance, or bias.

The service resets on off/reinitialization/self-test/driver error. Only accepted
fresh samples advance the estimator. CLI output suppresses old estimates at
age >=100 ms and in off, missing, testing, failed, or saturated states.
Unverified calibration or native coordinates restrict output to `BenchOnly`.
`Estimated` is an algorithm status, not certification or permission to feed
gauges. Operational `gauge_eligible` remains false. Explicit `source imu` permits
fresh mounted output only for the labeled [IMU BENCH display](IMU-ATTITUDE.md);
`bench_display_eligible` reports that separate qualification.

## CLI and verification

```text
imu on
imu status
imu estimator
imu read
imu audit
imu status
imu off
imu estimator
```

Wait for on/audit/off operation IDs to appear as `completed` in `imu status`.
`imu estimator` is a snapshot, not a filter-selection command. It reports
`estimator=accel_mean15` on the current board, `quality=bench_only` when tilt geometry is usable,
`attitude_covariance=unknown`, `gyro_bias_rad_s=None`, and `yaw_deg=None`.
Off/invalid input reports `quality=unavailable` with absent angles.
An accelerometer norm near 1 g does not prove stationarity or trustworthy tilt
under acceleration. The existing `imu read` raw/scaled contract is preserved.
Desktop CLI accepts the grammar but reports unsupported physical IMU target;
the pure pipeline and service behavior are exercised by host Rust tests.

`imu audit` requires acquisition already running. It reads CTRL1/2/3/5/7, then
compares eleven individually addressed accel/gyro bytes (0x35..0x3F) with the
same locked burst. GZ_H at 0x40 is read only at the end of the burst to release
the lock, so it is not part of the independent comparison. Each audit consumes
a diagnostic sample that is not fed into the estimator. Configuration/data
mismatch stops acquisition, clears estimates, and reports `AuditMismatch`.
Bus failures likewise fail closed; waits are bounded. An audit requested while
off completes with `audit=not_run` and does not turn on the sensor.

```powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
& D:\rfi-e\python_env\idf5.5_py3.11_env\Scripts\python.exe scripts/check-estimator.py --report verification/estimator-hooks-local.json
```

The board check verifies audit agreement, estimator metadata, off/re-enable,
unchanged simulation source/settings, and owner timing. It leaves IMU acquisition
off. See [hardware evidence and unresolved offset](../verification/ESTIMATOR.md).
