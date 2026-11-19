# Screen DEMO verification — November 7, 2026

## Follow-up: retained DEMO and one-second menu

The current refinement build keeps DEMO and its animation phase active across
gauge changes. Its owned IMU keeps running across those changes, avoiding sensor
stop/start cycles. All seven gauges are selectable during DEMO, including with
an altitude-only or IMU-only live source. The default center hold is now
**1,000 ms**. The earlier descriptions of gauge selection ending DEMO and a
three-second menu hold describe the superseded build below.

Installed refinement ELF:
`EAE215310A816D1DFC5159865D6AA02FE5CE5387998D048057AD5022991F6ADB`.
Application **1,199,216 B / 4 MiB (28.59%)**. Build/flash layout and SDK safety
checks pass. **253 workspace/all-target tests and strict Clippy pass**; tests
check retained activation/phase across all seven screens, one-second hold
boundaries, and transparent altitude-window pixels through three rotating buffers.

Board DEMO intervals measured **32.16–32.50 completed FPS**. The extended
`check-screen-demo.py` regression verifies retained DEMO on all seven screen
changes with the same IMU request ID and uninterrupted acquisition; normal exit
still restores sensor ownership and latest live T2 data. It also verifies the
board reports `menu_hold_ms=1000`. Physical press timing remains a manual check.

Evidence: `artifacts/minor-updates-tests.log`, `minor-updates-clippy.log`,
`minor-updates-board-final.json`, `minor-updates-flash.log`. Exact artifacts:
`artifacts/display-refinements-20261107/firmware/`. Board left in Combined DEMO,
COM5 released. No Git publication or installed driver change.

## Original DEMO implementation

The shared menu's DEMO toggle is implemented for all seven screens and installed
on the verified Waveshare SKU 29086 on COM5. The accepted profile-editor work
and preceding altitude visibility/combined-view changes remain uncommitted on
`feature/aircraft-profile-editor`. Existing editor windows and profile folders
were preserved. No Git landing, installer, public release, installed connector
update or live MSFS operation was performed.

## Automated checks

**253 workspace/all-target Rust tests and strict all-target Clippy pass.**
Python syntax checks and `git diff --check` also pass.

New tests cover the complete repeating sequence and field limits; idempotent
activation; unchanged live readings/settings; latest-stream restoration with
original ages/session; IMU age preservation and automatic fallback; owner timing
faults; cancelled/dragged/stale/multitouch/unseen DEMO gestures; active DEMO outline
and blue feedback through the menu cache; and pixel equality of full and cached
renders across all seven faces, including IMU/automatic transitions and exit.
Control tests prove that demo cancels unsent barometer requests, rejects edits,
does not replay them on exit, and cannot interrupt a claimed T2 write.

Logs: `artifacts/demo-tests.log`, `demo-clippy.log`. Production software previews
under `output/consolidated/` include `menu-demo.png` and `demo-auto.png`; these
were visually inspected. Software previews do not establish physical appearance.

## Installed firmware and board results

Final ELF SHA256:
`CE419FFF83D0611D0A88F15DE9B25D0A66523C5C31AC6281FCEEB5C451B28061`.
Application **1,201,056 B / 4,194,304 B (28.64%)**. Build/flash safety checks pass
with the existing partition layout, PSRAM XIP, task watchdog and I2C settings.
Exact ELF, SDK configuration, bootloader and partition artifacts are retained
under `artifacts/screen-demo-20261107/firmware/`.

The board showcase regression measured these three-second steady windows after
one second of warm-up, without UART queries inside each timing window:

| Screen | Input reported | Completed FPS |
| --- | --- | ---: |
| Altimeter | Automatic | 32.45 |
| Airspeed | Automatic | 32.20 |
| Magnetic heading | Automatic | 32.19 |
| Vertical speed | Automatic | 32.53 |
| Attitude | Actual onboard IMU | 32.46 |
| Turn Coordinator | Actual IMU tilt mapped as demo controls | 32.19 |
| Combined | Actual onboard IMU plus automatic scalar readings | 32.51 |

All seven windows retained valid displayed data with **zero additional expiry
ticks or deadline blanks**. Barometer edits were rejected and control polling
reported no queued writes. The IMU returned to Off after demos that started it;
acquisition already running before demo remained running afterward.

The same test continued receiving T2 synthetic telemetry behind Combined DEMO,
then exited to the latest altitude 8,123 ft, IAS 142 kt, heading 220 degrees,
VSI +750 fpm and pitch/roll +5/-15 degrees, preserving the session. This exercises
the existing connector protocol without sending real aircraft controls.
Evidence: `artifacts/demo-board-final.json` and `demo-board-final.log`.

Afterward, normal T2 rendering across all seven screens measured **32.02–32.34
completed FPS**, with no additional expiry/blanking in the steady intervals.
Evidence: `artifacts/demo-normal-t2-final.json`. This is synthetic protocol
qualification, not a new live-aircraft/MSFS qualification.

Final queried memory: PSRAM free **2,137,616 B**, internal free **77,639 B**,
minimum internal free **45,476 B**, largest internal block **31,744 B**, and
minimum drawing-stack reserve **3,884 B**. No extra bitmap, task or per-frame
allocation was added for DEMO. Cumulative expiry counters include source/mode
transitions outside the measured windows; final deadline blanks and blanked time
remain zero. Do not describe all transitions as zero-expiry.

## Handoff and remaining acceptance

Board left on **COMBINED, DEMO IMU**, underlying local simulator, selected screen
saved and `dirty=false`, sensor running with valid data, menu/editor closed,
touch controller ready, no active display hold and no blanking. UUID unchanged.
COM5 is released and echo restored. Latest state is in
`artifacts/demo-ready-final.json`. Demo itself is volatile and turns off on reboot.

**Physical menu taps and handheld motion still need user acceptance.** Hold
center, release, tap DEMO; hold the display upright facing you and tilt it for
Attitude/Turn/Combined. Verify the active outline, blue/chirp feedback, automatic
motion on the other faces, and return to normal via DEMO or a gauge selection.
No physical touch reports or IMU measurements were fabricated during automation.
Fresh onboard IMU estimates were observed, but the board was not physically moved
by the test. The estimator remains the existing accelerometer tilt bench path;
raw biased gyro output is not integrated into heading or flight attitude.
