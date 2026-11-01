# Altimeter contrast improvements - November 1, 2026

Branch: `feature/altimeter-contrast`, based on `v0.1.2-kollsman-readability`.

The lower altitude panel changes from white to light grey, RGB (190, 190, 190).
Black glyph antialiasing uses the same grey background. Size, position and font
weight are unchanged. The hundreds and thousands hands gain a thin bright-orange
RGB (255, 128, 0) outline, 2 logical units wide. Edge segments are drawn from
the same rotated vertices as their fills. The thin ten-thousands pointer and
its triangle remain unchanged. Orange is a static readability accent, not a
warning state. Invalid data still removes every hand, including these edges.

Validation:

- All 10 UI tests pass, including full/cached/incremental equivalence across
  motion, faults, settings, viewport sizes and three independently reused buffers.
- UI Clippy all targets with warnings denied, formatting and firmware build /
  generated safety settings pass.
- Regenerated 24 snapshots at 240, 480 and 720 pixels. Inspected the 480-pixel
  8500-ft preview; README preview updated from the 4520-ft snapshot.
- The user requested a slightly darker grey and 2-pixel edges after reviewing
  the initial RGB210 / 1.5-unit version. The final RGB190 version uses a square
  brush with a two-pixel footprint at 480x480, scaled at other viewport sizes;
  the existing circular brush would round an even width to three pixels.
- All 10 UI tests, Clippy and firmware build/config checks pass again after the
  revision. Updated software preview inspected; the user confirmed the final
  installed appearance "looks good" before requesting publication.
- [20-second climb plus ground hold](altimeter-contrast-takeoff.json) passed:
  680 completed presentations / 21020 ms = **32.35 FPS**, maximum draw 22 ms,
  transfer observation 40 ms during measurement, owner gap 31 ms. IMU off.
  No reported display/timing fault or blanking. Ends at 10000 ft and 29.92 inHg
  with simulation running, serial client closed and echo restored.

Initial RGB210 / 1.5-unit build ELF SHA256 (measurement above):
`B31BEADB070ABDF5194F97779A520888EAFCED30B12464393C807A5A260362D4`.
Final RGB190 / 2-unit build ELF SHA256:
`BC226267ADA896EE7F28532F3278386EDD3513F29415D9B1A6246F6832C1E5DC`.
The [final 20-second climb check](altimeter-contrast-adjusted-takeoff.json) passed
at **32.35 FPS** (680 presentations / 21019 ms including ground hold), maximum
draw 23 ms, transfer observation 40 ms and owner gap 31 ms. No reported timing
or display fault or blanking. Ends at 10000 ft, 29.92 inHg, IMU off; COM5 released.
Application 914720 / 1048576 bytes (87.23%). No changes to source validity,
instrument calculations, buffer ownership or health deadlines.
