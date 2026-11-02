# Standalone attitude development — November 2, 2026

Tag: `v0.2.0-rc.2`; branch: `feature/attitude-gauge`, based on local MSFS checkpoint `653b573`.
This is development firmware, not a published release. MSFS aircraft-switch
recovery remains a separate unresolved live test.

## Reference and correctness

The implementation follows the phone-tested standalone `AttitudeModule(false)`
in `apps/browser-sim/src/instrument-fragment.html`: original lettering, pitch
scale, fixed aircraft symbol, bank scale and rotation conventions. RGB565 uses
discrete sky/earth gradient bands and opaque caption/scale backing.

Both pitch and roll must be valid and younger than 100 ms. Missing, stale, failed
or timing-fault inputs remove the moving horizon and show the explicit reason.
The source timestamp is retained through drawing and presentation. The current
external MSFS profile has no attitude fields; it cannot supply this gauge yet.
The IMU remains diagnostic and is not silently used as an attitude source.

Forty UI tests pass, including independent projection/sign checks, full redraw
versus rotating incremental buffers across motion/extremes/faults/view changes,
cache fallbacks, source-label transitions and concurrent presentation-fence
payload transfer. The complete Rust workspace including native MSFS tests passes;
Clippy with warnings denied and formatting checks pass.

## Flicker diagnosis and changes

The first candidates exceeded the 100 ms numeric lifetime while rendering and
waiting for presentation. The independent supervisor therefore blanked the
backlight. An average above 30 FPS alone did not resolve those misses.

The final candidate retains full 480 x 480 resolution, 18 MHz pixel clock,
240 MHz CPU, 80 MHz PSRAM and two post-submission completion events. It uses:

- A separate core for rasterization and RGB interrupts.
- Per-buffer horizon history and 16 x 8 damage tiles, exact fixed-point band
  boundaries, row writes and grouped rotations of the original glyphs.
- A bounded 96 KiB internal foreground/glyph cache and 20 KiB renderer stack.
- A 1 ms scheduler tick, blocking idle opportunities and later input capture.
- An atomic completion fence publishing the original deadline from the ISR;
  the guard reads asynchronous timestamps before its evaluation clock.
- The existing owner, idle-task and display-supervisor watchdog protections.

Rejected candidates included a stack overflow after enlarging the damage map;
the stack protection reset the board. The final stack allocation was increased
and its remaining headroom measured. No protection was disabled to pass tests.

## Hardware measurements

Waveshare SKU 29086, verified CH343 UART identity, COM5 at 921600/8N1. IMU
diagnostics run concurrently; no diagnostic UART queries occur during animation.
All figures count completed presentations, not render-loop iterations.

| Exercise | Completed FPS | Maximum draw / transfer observation | Observed numeric expiry ticks |
| --- | --- | --- | --- |
| Accelerated 10-second dark sweep | 32.15 | 32 / 32 ms | 0 |
| Normal 60-second dark sweep | 32.33 | 29 / 32 ms | 0 |
| Normal 60-second visible acceptance sweep | 32.33 | 29 / 32 ms | 0 additional |
| Altimeter 60-second climb plus 1-second hold | 32.33 | 22 / 31 ms | 0 additional |

The normal sweep completed 1,940 frames in 60,010 ms. Renderer stack headroom
was 7,652 bytes, free internal heap 153,239 bytes, owner maximum gap 18 ms,
and IMU bus errors zero. Cold initialization and deliberately injected faults
are outside the normal-animation FPS measurement.

Installed ELF SHA256:
`49672ED0497EE32749896C2F20A44C2B58C8378E752B85360446D274AC5883FC`

The visible acceptance script passed missing/failed/stale checks on both axes,
view switching, independent render/transfer-stall blanking and the full motion
exercise. Both intentional stalls were observed dark within 156 ms using host
polling. The motion interval completed 1,940 frames in 60,013 ms; its blanking
timestamp and expiry counter remained unchanged. Stack headroom after fault
checks was 7,600 bytes. Raw local report: `verification/attitude-board.json`.

The altimeter regression script passed missing/stale/failed handling, both
stalls, placeholder selection, barometer correction and display on/off recovery.
Raw local report: `verification/attitude-altimeter-regression.json`.

The altimeter climb completed 1,972 presentations in 61,005 ms, with no change
to its blanking timestamp or expiry counter and no owner timing fault. Final
stack headroom was 7,408 bytes; heap remained 153,239 bytes. Raw local report:
`verification/attitude-altimeter-performance.json`.

On November 2, 2026, the user watched a repeated 60-second motion exercise and
confirmed: "it looks great; smooth motion, not tearing, no artifacts."
This records physical visual acceptance separately from the software counters.
It does not establish a worst-case real-time bound for every possible attitude
trajectory or validate the instrument for flight.

Reproduce with `scripts/check-attitude.py --port COM5 --seconds 60 --report
verification/attitude-board.json`. See [the attitude guide](../docs/ATTITUDE.md)
for commands and [the display architecture](../docs/DISPLAY.md) for ownership
and supervision boundaries.
