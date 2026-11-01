# Altimeter spacing cleanup - November 1, 2026

The dial numerals were too close to the tick marks, especially 9 and 0, and the
Kollsman window overlapped 3. In the Rust renderer's 480-unit coordinate system:

- Numeral-center radius changed from 179 to 169, moving every numeral inward
  by 10 units while retaining its size and angular position.
- Window top changed from 217 to 203, raising the frame and its contents by
  14 units. The preview was checked for clearance from both 2 and 3.

The shared Canvas scales the offsets for other display sizes. Instrument math,
ticks, needle geometry, source validity, presentation supervision and scanout
configuration were not changed. The browser reference retains its original
layout. The README's software-rendered image was regenerated from the shared
Rust renderer.

## Verification

- Six instrument-ui tests passed, including cached/full-frame equality,
  invalid-pointer removal and repeated-setting repaint checks at 240/480/720.
- Instrument-ui Clippy with warnings denied and workspace formatting passed.
- Firmware compiled and the build script verified generated SDK settings.
- The guarded flash script checked the known adapter and recovery backup,
  then installed the build on COM5.
- Altitude was set to 8500 ft twice; the displayed value was valid and the
  software reported no timing/display failure.
- The user physically confirmed: "Yes—spacing looks good" in response to
  checking tick clearance at 9/0 and window clearance from 2/3.
- A 20.08-second steady observation recorded 311 presentations, 15.49 FPS,
  no unexpected blanking, no timing fault and maximum owner gap 30 ms.

Evidence: [manual-setting replay](graphics-cleanup-replay.json) and
[steady observation](graphics-cleanup-steady.json). The steady report's 128 ms
render maximum is a lifetime counter, not a reset measurement of this window;
do not compare it directly with the earlier takeoff-window maximum. Physical
spacing acceptance is a user observation, separate from software health checks.

Installed ELF SHA-256:
`876f54853d73fae03a3434c70de0897fc01ebeff7277f5639599958fb93535ae`.
Application size remains 898224 / 1048576 bytes. Final checked state: 8500 ft,
29.92 inHg, simulation running, display enabled, IMU off. The port was released
and the Python client restored the original echo setting.
