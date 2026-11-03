# Touch gauge menu - accepted bench validation, 2026-11-03

Source checkpoint: `v0.2.0-rc.4`, developed on `feature/touch-gauge-menu` from
main `6c36917`. See [usage and source restrictions](../docs/GAUGE-MENU.md).

The user confirmed: "Yes - touch, switching, and idle are clean." This supersedes
rejection of earlier candidates. Physical acceptance used the diagnostic build;
the normal build installed afterward removed the temporary counter. The later
graphics alignment changes and their separate validation are recorded below.

## Behavior and fixes

Configurable 3-second center hold; six visible gauge choices; capability-based
disabling; release-to-select; Cancel and 30-second inactivity; blue pressed
feedback and one chirp per active press. Pressure editing remains independent.
No simulator protocol or sensor data mapping changed.

The touch mailbox now distinguishes lock contention from explicitly invalid
input. Cached input retains its original timestamp and expires at 120 ms.
Menu buttons require a matching confirmed presentation generation before arming.
The attitude menu is cached, preparation is bounded, and press transitions redraw
only changed buttons. Separate prepared face caches avoid rebuilding on switches.

Hardware stress then isolated two remaining problems: missed RGB refill deadlines
and an unnecessary attitude capture delay when leaving that view. The final build
uses 16-line bounce buffers and skips that delay on view changes, checking selection
during 1 ms waits. CPU/PSRAM/pixel clocks remain 240/80/18 MHz, data cache 32 KiB.
Frame ownership, source freshness, 100 ms attitude deadline and watchdogs are
unchanged. See [reproduction and controlled measurements](RGB-SCANOUT.md).

## Evidence

- 169 workspace Rust tests with `instrument-connector/msfs` passed, process exit 0.
- Targeted UI/core/CLI Clippy passed with all targets and warnings denied.
- Pixel comparisons cover all three buffers through open/press/close, changing
  attitude poses, cached/uncached menus and retained-renderer switches.
- Input tests cover holds, mailbox contention, expiry, invalidation, disabled
  controls, source changes, presentation generations, dragging, configuration,
  cancellation and inactivity. A regression first reproduced the wasted view-change wait.
- Diagnostic build: two 50-switch runs at 31.01/31.23 FPS, 30-second attitude
  motion at 32.35 FPS, and 120-second idle passed with no new RGB recoveries.
- Physical touch capture: 120 seconds, ten presented menu generations, 23 chirps,
  no new recovery/blanking/expiry and no touch/sound errors. User confirmed clean visuals.
- Normal build: 50 switches at 31.17 FPS, no blanking or expiry; 30-second
  attitude motion with concurrent IMU sampling at 32.35 completed FPS, render
  maximum 24 ms and transfer maximum 32 ms, no new blanking or expiry.
  Missing/failed/stale data and deliberate render/transfer stall checks passed.
- Normal altimeter climb passed at 32.34 completed FPS (10-second climb plus
  initial second), render maximum 21 ms, transfer maximum 32 ms; guard counters
  remained unchanged from the preceding attitude fault-test baseline.

Reports are in ignored `output/gauge-menu/`: `rust-tests-final.txt`,
`final-normal-switches.json`, `final-normal-attitude.json`,
`final-normal-altimeter.json`, and `rgb-probe/*-16-lines-schedule*.json`.
Deliberate fault tests increment lifetime guard counters; normal motion checks
compare deltas. Finite bench tests do not establish absence of every intermittent fault.

## Installed normal build

Waveshare SKU 29086, verified CH343 UART COM5. ELF SHA256:
`955D7597B47AB90B8D013769705ECAAA563E44A06A08878AF9E943F160250396`.
Application 1,008,640 / 1,048,576 bytes (96.19%). The temporary SDK probe and
Rust getter were removed, original SDK source hash verified, SDK archive rebuilt,
and SDK Git status confirmed clean before this build was flashed.

MSFS attitude transport is still pending: external T1 streams enable only
Altimeter; simulation enables Altimeter and Attitude; IMU enables Attitude only.

## Graphic alignment follow-up, 2026-11-03

The altimeter failure cross and banner now share the dial center (240, 240).
Both gauges' failure lettering is lowered three pixels within its banner to
center the visible font ink. Attitude bank labels move inward from radius 216
to 212 so the left "30" is fully inside the clipped disk, preserving the angular
offset from major ticks. Actual-render previews confirmed the previously cut
top of the "3" is complete and clear of the tick. All 54 UI tests passed,
including full/cached/incremental equivalence and gauge switching.
Before/after previews are in ignored `output/graphics-alignment/`.

Updated-board regression passed: 12 switches at 31.54 FPS and 10-second attitude
motion at 32.32 FPS, no new blanking/expiry during either; deliberate fault
handling also passed. Board left on level Attitude for label inspection; COM5 free.
