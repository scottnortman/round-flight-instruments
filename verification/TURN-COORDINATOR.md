# Turn Coordinator verification — 2026-11-03

Source checkpoint **v0.2.0-rc.5**, developed on `feature/turn-coordinator`
with the Windows driver/T2 foundation. No new binary release.

Portable checks pass: standard-rate geometry/sign/saturation, independent rate
and ball failures, exact 100 ms local / 150 ms external expiry, demo bounds and
cancellation, strict CLI parsing, and pixel equivalence through three rotating
buffers. The existing touch overlay test now includes Turn. The phone prototype's
own Node tests pass. Workspace Clippy passes with warnings denied.

The initial local board demo reached 32.33 FPS but recorded 15 numeric-expiry ticks.
That candidate is superseded: the final candidate captures continuing Turn frames
closer to their transfer slot. It retains the same numeric deadlines, bypasses
capture delay on a view change and keeps all pipeline watchdogs enabled.

The user accepted initial motion. The final layout moves TURN COORDINATOR above
the airplane motion envelope and centers NO PITCH / INFORMATION on two footer
lines. The final 61 portable UI tests pass, including the subsequent menu fixes.

The corrected 60-second local demo passes at **32.31 FPS**, maximum raster time
6 ms, with zero expiry ticks or blanking. T2 external regression with 20 seconds
per stage passes: **Turn32.31, Altimeter32.31, Attitude32.32, Turn32.33 FPS**.
Zero expiry ticks or blanking throughout, including the intervening switches.
Reports: `artifacts/turn-local-final-20261103.json` and
`artifacts/turn-t2-regression-20261103.json`. The final caption/title layout also
passes a fresh T2 Turn run at **32.33 FPS**, zero expiry/blanking, in
`artifacts/turn-layout-final-20261103.json`.

189 Rust tests pass in the final workspace suite; final
workspace/all-target Clippy is clean. Coverage includes refusal of hidden
Altimeter writes while Turn is active, independent source gates and expiry,
bounded/cancellable demos, and full-frame versus damaged-tile equality. The three
native fake-SDK crash/recovery tests pass. The user confirmed the three-gauge
menu and layout worked, then requested both top lines 12 pixels lower. The final
title/source baselines are y=103/136; the two-line footer remains centered.
The lowered-title firmware passed a fresh 20-second local run at **32.35 FPS**,
zero expiry ticks and zero blanking (`artifacts/turn-lowered-layout-20261103.json`).
ELF SHA256: D32F94EC3119E8215ABE761AD65014A7B4A587FB8F89880A17D0BB19F669761F.

Current application fits the unchanged 1 MiB app partition at approximately 99.1%.
Future gauge additions will likely require a planned app partition expansion.
No partition-table change was made for this feature. Live MSFS mapping and cockpit
agreement still need the simulator PC; the existing all-six data stream supplies
both Turn fields without new simulator writes.

Follow-up menu issue was diagnosed and fixed: see [Turn menu fix](TURN-MENU-FIX.md).
Final hardware dwell/selection and Turn streaming both reached32.37FPS with no new
expiry or blanking during the measured intervals. The board is left on local
centered Turn, menu closed, COM5 released. The checkpoint retains the hardware acceptance evidence.
