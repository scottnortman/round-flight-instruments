# Altimeter readout emphasis - November 1, 2026

Branch: `altimeter-readout-emphasis`, based on the tagged 30 Hz milestone.

The lower numeric altitude is now 32-unit bold black text on a white
232-by-40-unit panel. Previous text was 20 units. The `FT` suffix remains;
negative and five-digit values fit. The panel clears the Kollsman window and
SIM INPUT label. The existing font's coverage is expanded horizontally by one
source pixel and composited against white in one pass. No font asset or new
allocation is needed. This is the shared Rust renderer, not the browser reference.

Validation:

- All 53 existing Rust tests pass, including incremental/full redraw equivalence
  across three buffers, faults, settings and viewport sizes.
- Clippy all targets with warnings denied, formatting and firmware build/config
  checks pass. Software previews inspected at 480 pixels for 8500 and -100 ft;
  24 existing snapshots regenerated at 240, 480 and 720 pixels.
- Installed on COM5 and ran the takeoff acceptance script for a 60-second climb
  plus one-second ground hold. [Evidence](readout-takeoff-60s.json).
- 1973 completed presentations / 61019 ms = **32.33 FPS**; maximum render 20 ms,
  submission-to-observed-completion 39 ms, owner gap 31 ms. No reported display or
  timing failure; blanked_at_ms remained 0. IMU was off in this run, unlike the
  previous milestone's concurrent IMU measurement.
- Ends at valid 10000 ft, 29.92 inHg, simulation running, display healthy. Serial
  client closed and restored echo. The user accepted the appearance and motion,
  then requested less white space below the digits. The final panel removes six
  logical pixels from its bottom while leaving text position unchanged.

Initial 46-unit-panel firmware ELF SHA256 (60-second measurement above):
`ABE585963E60385C17CD66ACB695CEA6553664515A75DCC2369FF7403E24C539`.
Final 40-unit-panel firmware ELF SHA256:
`DED0336539AE3C301E4B337325A51F882B7FFAB7E5EE193AA52502148963E482`.
All 10 UI tests were rerun successfully after trimming the panel; snapshots and
the README preview were regenerated, and the firmware rebuilt and installed.
The final panel's [20-second climb check](readout-tight-panel-takeoff.json)
passed at **32.33 FPS** (679 presentations / 20999 ms including ground hold),
render maximum 20 ms, transfer observation maximum 40 ms, owner gap 31 ms,
no timing/display fault and no reported blanking. It ends at 10000 ft with COM5
released. The six-pixel margin adjustment has not yet received separate user
optical confirmation.
Application 911600/1048576 bytes (86.94%). Safety thresholds and buffer ownership
are unchanged. The numeric panel exists only for valid altitude; invalid states
retain red quality indicators and no numeric altitude.
