# Onboard IMU calibration

The menu now includes **CALIBRATE** on every gauge. Hold the center for one
second, release, then tap CALIBRATE. It uses the real onboard QMI8658, even when
DEMO is enabled. The selected gauge and live MSFS stream continue in the
background. Calibration cannot send aircraft controls. The existing Windows
test.12 connector works without an update.

Thin horizontal and vertical background lines cross at the display center on
the calibration pages, providing fixed visual axes for alignment.

## Preparation

1. Power the board for at least a minute at the temperature where it will be used.
   Let the die temperature settle; its reading is not ambient temperature.
2. Use a rigid, stationary support with a spirit level or an independently known
   level reference. Keep the cable slack and remove vibration. A hand-held level
   capture is less reliable than a fixture.
3. The **level reference is the normal upright instrument mounting**: screen
   facing you, lettering upright, top edge up. Do not lay the screen flat for
   LEVEL. Screen-flat is a different calibration pose, not the instrument's zero.
4. For highest supported accuracy, do FULL 24 once, then LEVEL after changing
   the installed mounting. After a large temperature change, repeat FULL 24.

## Quick LEVEL: gyro zero and installed level

1. Tap LEVEL and leave the supported board completely still. The worker audits
   sensor configuration, settles for three seconds, then collects roughly five
   seconds of fresh stationary samples. Movement, saturation, sample gaps and
   excessive noise reject or restart collection. Acquisition is enabled if needed.
2. Review GX/GY/GZ zero-rate offsets and the measured noise. These use the
   upright board axes before the level trim: [sensor Z, -sensor Y, sensor X].
   The historical sensor-native +30 dps Y offset normally appears as about
   -30 dps on the mounted Y axis. The proposed result
   has **not** been applied or saved yet.
3. If **HIGH BIAS: OUT OF SPEC** appears, the offset exceeds the manufacturer's
   stated initial-offset tolerance. This board previously measured about +30 dps
   on sensor-native Y while stationary. Correcting a repeatable offset does not
   establish that the sensor is healthy or its angular scale is accurate. Save
   only if the board truly was stationary; otherwise Cancel and repeat. Retain
   the raw evidence and investigate the sensor if this persists.
4. Press SAVE. Wait for CALIBRATION SAVED; the settings worker commits and reads
   back the record before activating it. BACK returns to the current gauge.

LEVEL estimates all three gyro biases and an upright leveling rotation. It
preserves a previously accepted accelerometer calibration. It does not pretend
that one static pose measures three independent accelerometer biases or scales.

## FULL 24: multi-axis accelerometer calibration

1. Tap FULL 24. An internal six-axis check-alive test and configuration audit run
   first; a failed test prevents calibration. Wait for the pose instructions.
2. Follow the named directions on the screen. The first six poses cover screen
   up/down, right/left edge down, and top edge up/down. For two or three named
   directions, tilt between them: these are diagonal or corner poses. Exact
   reference angles are unnecessary. Keep the board stationary in each pose
   until the progress bar completes, then move to the next requested pose.
3. Each pose needs about two seconds of stable samples, with a short settling
   interval between poses. A chirp marks completion when touch sounds are enabled.
   For screen-down poses use a support that does not press or scratch the glass;
   you can turn it briefly to read the next instruction after the chirp.
4. Eighteen diverse poses fit the model; six additional poses validate it without
   being used in the fit. Repeated or poorly covered orientations, unstable
   temperature, excessive noise and bad validation residuals are rejected.
5. After pose 24, put the board back in its upright, level installed orientation
   and hold still for the final LEVEL capture. Review the offsets, noise and
   validation RMS, then SAVE. Plan on a few minutes for the complete procedure.

The fit estimates accelerometer bias, three-axis scale and symmetric cross-axis
correction. A robust weighted ellipsoid fit uses gravity magnitude, with positive
matrix, coefficient bounds and independent validation checks. The final level
capture resolves mounting tilt. Calibration rotation is applied to both sensor
vectors; accelerometer correction is not applied to angular rates.

## Persistence, cancellation and recovery

- Cancel before SAVE keeps the previous active calibration. RESET prepares a
  separate review screen; SAVE there removes calibration and restores the
  uncalibrated tilt estimator. RESET does not erase device identity or settings.
- Saving is non-cancellable while the flash commit/readback is in progress.
  A failed save does not activate the proposed coefficients. After a storage
  fault, inspect calibration status after reboot before assuming which record
  is durable. No unreviewed coefficients are written.
- Successful records survive power cycling. They contain version/mount tags,
  CRC, generation, coefficients, temperature span, noise, actual accepted sample
  count and validation residuals. Invalid or foreign records are not used;
  newer-version storage is preserved instead of being silently overwritten.
- Temporary acquisition is restored off when a calibration that started with the
  sensor off completes or is cancelled. Pre-existing acquisition stays on.
- A full calibration is accepted only across a temperature span of at most 3 C.
  Reusing its matrix during a later LEVEL capture preserves that temperature
  span; if it is too far from the new temperature, run FULL 24 again.

## What changes dynamically

A saved calibration enables six-axis **Madgwick quaternion fusion** for onboard
IMU attitude and IMU-driven DEMO motion. Before calibration, the existing
15-sample accelerometer tilt remains active. MSFS readings are not filtered or
recalibrated by this procedure.

The estimator uses calibrated gyro rates, adaptive accelerometer norm/innovation
rejection, measured counter intervals anchored to the receipt clock, and a
conservative residual-bias update after a stationary interval. It resets across
faults, source/calibration changes and timing gaps. It does not average Euler
angles across bank wrap. Detected linear acceleration suppresses accelerometer
feedback while gyro propagation continues; diagnostics expose that condition.
Outside a 3 C margin around the recorded temperature span, gyro fusion falls
back to tilt and the calibration page reports a temperature warning. Recalibrate
at the working temperature rather than inventing a temperature coefficient.

This is a six-axis IMU: absolute yaw/heading and sustained-acceleration ambiguity
are unresolved without external references. The procedure does not calibrate
absolute gyro scale, gyro cross-axis alignment, long-term temperature curves or
an aircraft attitude installation. Those require independently validated motion
or reference equipment. Posterior attitude/bias covariance remains unknown.
Madgwick is used for bounded quaternion fusion; an untuned Kalman filter would
not remove these reference limitations or create additional calibration data.

## Verification after saving

Use `source imu` to check the actual IMU attitude display, or select Attitude /
COMBINED with DEMO enabled. MSFS source selection continues to show MSFS data.
Check level, positive/negative pitch, clockwise/right bank, return-to-level and
slow/fast known rotations. Compare approximately +90 and -90 degrees about each
axis against an independent square/fixture. Test rest again at working
temperature. Static fitting residuals are not a dynamic accuracy certificate.

Optional UART CLI (921600/8N1):

```text
imu calibration status
imu calibration level
imu calibration full
imu calibration save JOB
imu calibration cancel JOB
imu calibration reset
imu estimator
imu read
display calibration
touch cancel
```

Use the exact job number from status for save/cancel. Reset is review-only until
SAVE. `imu read` deliberately retains original sensor-native raw diagnostics;
`imu estimator` and the calibration status describe corrected/fused data. Disable
extra serial polling during normal MSFS use.

## Method references

- [QST QMI8658C Rev A datasheet](https://www.qstcorp.com/upload/pdf/202210/13-52-27%20QMI8658C%20Datasheet%20Rev%20A%20%281%29.pdf): sensor scaling, check-alive and offset specifications.
- [Madgwick's original orientation-filter report](https://x-io.co.uk/downloads/madgwick_internal_report.pdf): six-axis quaternion gradient-descent formulation. The project adds its own calibration, acquisition guards and acceleration rejection.
- [Tedaldi, Pretto and Menegatti, ICRA 2014](https://albertopretto.altervista.org/papers/tpm_icra2014.pdf): multi-position IMU calibration principles. The implemented stationary ellipsoid/validation workflow does not implement their complete dynamic gyroscope scale calibration.
