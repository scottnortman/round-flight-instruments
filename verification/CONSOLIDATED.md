# Consolidated screen and altitude visibility — November 7, 2026

The later refinement makes the hidden altitude window transparent: its fill and
digits disappear while the analog hands remain visible through the white
outline. The previous blank-interior description below is historical. Pixel
tests verify the hand crosses the hidden window and that toggling restores all
three drawing buffers exactly; the software preview was visually inspected.
See the [current refinement build](DEMO.md#follow-up-retained-demo-and-one-second-menu)
for the installed hash and regression evidence. The default menu hold is now one
second, and DEMO remains enabled across gauge selections.

Implemented on the existing `feature/aircraft-profile-editor` checkout while
preserving the accepted, uncommitted editor work. Both features are built and
flashed to the verified Waveshare SKU 29086 on COM5. No commit, tag, push,
installer, public release or installed Windows connector update was performed.

## Automated checks

- **244 workspace/all-target Rust tests pass**, including touch release/drag/
  stale-input/multitouch cancellation, seven-button reachability and separation,
  CLI selection/capabilities, persistence-record round trips, and combined-view
  aircraft-control isolation.
- Strict workspace/all-target Clippy passes. `git diff --check` and modified
  Python syntax checks pass.
- Rotating-buffer renders match full redraws through motion, partial failures,
  stale data, view switches, menus and altitude hiding. Combined render checks
  cover 64-, 241- and 480-pixel sizes. The hidden altitude test checks every
  border pixel, blank interior and unchanged pixels outside the window.
- Numeric lifetime tests retain each component's existing age/expiry and use
  the earliest visible component deadline. Partial failures do not fabricate
  zero readings. Combined view cannot dispatch a barometer control.
- Production renderer previews were generated and visually inspected for level,
  climb/bank, partial failure, altitude visible/hidden and the expanded menu.

Logs: `artifacts/combined-tests.log`, `combined-clippy.log`,
`combined-ui-tests.log`; images: `output/consolidated/`.

## Physical board measurements

Final installed ELF SHA256:
`9B46533FCEA2186A752AA446D762B5FA1A7E9F0E0A67D72228976E444F2C30B1`.
Application **1,196,400 B / 4,194,304 B (28.52%)**. Build safety checks verify
the existing partition layout, PSRAM XIP configuration, watchdogs and I2C driver.
Original partition-table hash remains
`FD8026BFF850CA0DEE41C41305160317FFFE604DDA30A9BD5A701AC82D96FA17`.
Exact ELF, SDK settings, bootloader and partition artifacts are retained under
`artifacts/consolidated-20261107/firmware/`.

| Workload | Completed FPS | Result |
| --- | ---: | --- |
| Combined, first T2 motion interval | 32.34 | Pass |
| Altimeter / Attitude / Turn | 32.32 / 32.37 / 32.23 | Pass |
| Airspeed / Heading / VSI | 32.40 / 32.34 / 32.40 | Pass |
| Combined, second T2 bank range | 32.31 | Pass |
| Combined, local attitude motion (10 seconds) | 32.35 | Pass |

Every five-second steady T2 interval retained valid data with **zero additional
expiry ticks or deadline blanks**. Local component-fault trials cover altitude,
IAS, roll, heading and VSI: the corresponding displayed value becomes unavailable,
the combined quality reports failure, and normal samples recover after fault
removal. Seventy rapid local switches across all seven screens produced **no
blanking**, and recovered to valid Combined with no active deadline hold.

The rapid switching workload averages **29.57 FPS**, including a 111 ms maximum
render and 134 ms transfer interval. Its source/mode transitions and switches
produce permitted expiry/hold events; this is not a zero-expiry or 30-FPS rapid
switching qualification. Steady-motion results above are separate measurements.

Final free memory: **2,138,304 B PSRAM**, **77,771 B internal heap**, minimum
internal free 45,700 B, largest internal block 31,744 B; minimum renderer stack
reserve **2,940 B**. A retained CPU-only combined backdrop adds 460,800 B PSRAM;
there is no extra scanout buffer or per-frame allocation. The small horizon is
drawn directly into the existing drawing buffer. Row forwarding, fixed-artwork
caching and skipping restore of opaque horizon pixels keep steady rendering below
30 ms. Desktop profile-preview exports are excluded from ESP-IDF compilation,
avoiding an Xtensa compiler crash in unused desktop artwork.

Board evidence: `artifacts/combined-t2-final.json`,
`combined-local-final.json`, `combined-flash.log` and `consolidated-before.json`.
These use synthetic local/T2 measurements; they are not a new live-MSFS aircraft
qualification. The existing connector protocol needs no change.

## Handoff

Board left on **COMBINED**, local simulation, level pitch/roll, IAS 135 kt,
altitude 6,500 ft, magnetic heading 315 degrees, VSI +1,250 fpm, QNH 29.92.
Selection is saved (`dirty=false`); UUID is unchanged; no NVS failures. Touch
controller is ready, menus/editors closed, altitude visibility defaults visible,
and COM5 is released with its original echo setting restored.

**Physical touch and appearance still need user acceptance.** Automated tests
exercise the portable touch state machine; no physical taps were fabricated.
Check the standalone Altimeter window twice to hide/show, its outline, barometer
editing, and the center-hold menu's COMBINED / INFO / Cancel controls. Inspect
the new screen for legibility and motion. The two existing profile-editor windows
and their profile folders were left untouched.
