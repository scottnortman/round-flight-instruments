# IMU-driven attitude bench mode

On `feature/attitude-gauge`, the onboard QMI8658 can explicitly drive the
standalone attitude face. This is accelerometer-only tilt for debugging on a
desk or in your hand. It is uncalibrated, does not use the known biased gyro,
and is not a flight attitude solution. A 15-sample moving average reduces
jitter before tilt calculation. Acceleration can produce misleading tilt
even when its magnitude is close to 1 g.

Hold the display upright facing you, with its lettering upright. In the UART
CLI (921600/8N1, no flow control), enter:

```text
source imu
display on
source status
imu estimator
```

`source imu` queues sensor acquisition, clears the previous source's data and
selects attitude. Wait for fresh readings; its acknowledgment does not claim
sensor initialization has completed. The caption is **IMU BENCH**. Clockwise
rotation of the face represents right bank; tipping the top edge away from you
represents nose up. Rotate gently, keeping the cable slack.

```text
imu off
imu on
```

Off retains the last usable attitude without changing its caption; on resumes
only with fresh usable estimates. Before the first usable sample, the face shows
**WAITING FOR IMU**, without an invented level attitude.
Self-test, sensor failures, implausible acceleration (outside 0.8–1.2 g), the
pitch-pole singularity, and data aged 100 ms or more invalidate live readings.
In this bench mode the display holds the last usable filtered pitch/roll pair
instead of blanking or drawing a red X. It can hold indefinitely; the held pose
is not a current attitude measurement. Changing sources clears the retained pair.
Placing the screen flat approaches the pitch pole, where bank is undefined.
There is no automatic simulation fallback. Turning the sensor on alone never
selects it as the gauge source.

Return to the simulated gauge with:

```text
source sim
sim preset level
instrument attitude
sim attitude 60
```

`source sim` leaves sensor acquisition unchanged; use `imu off` if no longer
needed. Simulation edit/demo commands are rejected while IMU is selected.
The IMU supplies only pitch and roll: selecting the altimeter in this source
shows missing altitude. Mode and mounting are not written to flash.

## Boundaries and timing

The portable estimator pipeline remains replaceable for later filtering/Kalman
work. A validated, right-handed mounting permutation maps sensor inputs to the
upright board frame: **[forward, right, down] = [sensor Z, −sensor Y, sensor X]**.
The upright and clockwise-bank directions were checked against physical samples.
This mounting is specific to the tested SKU 29086 orientation. It is not a bias
or scale calibration. Raw `imu read` output remains sensor-native.

The sensor worker applies mounting before the estimator. The board uses
`AveragedTilt`: average 15 acceleration vectors over less than 200 ms, then
calculate angles. This avoids averaging across the ±180° bank discontinuity.
Startup and recovery require 15 usable observations; raw invalidity immediately
clears history. Filter-history retention (200 ms) is separate from the 100 ms
new-output freshness limit: delivery gaps of 100 ms or stale input reset the
pipeline, and an unchanged output still expires at 100 ms. The extra history
margin tolerates occasional skipped conversions without inventing new samples.
The causal average adds roughly 60-65 ms of response lag at 100-112 Hz; reported
receipt age describes the newest observation, not a claim of zero filter lag.
Only fresh, usable
output in the body frame is eligible for this explicit bench display. Calibration
remains unverified, covariance/absolute heading remain unknown, and gyro samples
are neither corrected nor integrated. `imu estimator` reports
`bench_display_eligible` separately from operational `gauge_eligible=false`.

A typed, nonblocking snapshot crosses into the single state owner. Its receipt
identifier and age are preserved; duplicate reads cannot renew or revive data.
Both axes update atomically. Lock contention ages the previous sample; failures
invalidate it. The owner hands off IMU data every nominal 5 ms, and the sensor
worker targets a 5 ms start-to-start period on core 0, separate from the raster core,
and always blocks at least 1 ms after each iteration. This avoids adding another
full 5 ms delay after sensor/touch bus work, without catch-up spinning,
using the 1 ms board scheduler. Device ODR
remains 112.1 Hz. Duplicate device counts are ignored. Actual acquisition time
and sensor/filter latency remain unknown; reported ages use host receipt time.

Rendering uses the shared buffered renderer and independent supervisor. Live
readings still become stale at 100 ms, but IMU bench attitude has no numeric
display expiry: it retains the prior usable pose without changing its caption.
Owner, renderer, transfer and scanout watchdogs remain active and can still blank
the display on an actual pipeline failure. Simulated and external sources keep
their existing invalid-data behavior. No timestamps are renewed, and no poses
are extrapolated. `source status` reports `holding`, `held_pitch_roll_deg` and
`held_age_ms`; `display sample` reports the shown pair, its age and `holding`.

`imu status` includes `estimate_rejections` (all unavailable estimates, including
startup/window recovery) and `raw_tilt_rejections` (raw gravity/geometry rejected
by the bench tilt checks). A rising first count with an unchanged second count
can identify acquisition/window timing problems rather than implausible gravity.
These counts remain diagnostic even while the display holds a previous pose.

## Current limits and acceptance

There is no configured angular-rate cutoff for the accelerometer-only estimator.
The raw acceleration norm must remain between 0.8 and 1.2 g. Its roll geometry
also rejects a forward-axis pole when sqrt(right^2 + down^2) is below 0.001 g.
The sensor is configured for +/-4 g and +/-256 deg/s per axis; saturation of
ANY accelerometer or gyro channel invalidates a sample, even though gyro rates
are not integrated. These are programmed ranges, not calibrated performance
ratings. Fifteen usable samples are required after rejection (nominally about 125-140 ms).

High-rate hand movement can create non-gravitational acceleration, so widening
these checks would not establish accurate attitude. Calibrated gyro/accelerometer
fusion is needed to progress beyond this limited tilt demonstration.

Earlier strict-expiry builds had visible red X/blanking during rejected movement.
The 15-sample build changes the IMU display policy as described above. See
verification/IMU-ATTITUDE.md for measured results and visual acceptance. The diagnostic
`expired_source_valid` / `expired_source_invalid` counters classify the latest
owner snapshot when the guard observes expiry; they do not prove its cause.
