# Bounded display-hold verification

November 4, 2026. Source checkpoint **v0.2.0-rc.9**, merged into `main` from
`bugfix/bounded-display-hold`, based on accepted
`v0.2.0-rc.8` (`3ade300`). User selected **three misses**, then blank on the fourth.
The user requested committing and merging this fix on November 4, 2026. It is
deployed to the local Waveshare board; no new prebuilt release assets have been
published. The automated/board evidence below does not claim additional
user visual or real-aircraft qualification.

## Policy and proof of freshness

The independent guard tolerates three expired presentation slots, approximately
32 ms each at 18 MHz, while leaving the immutable completed image visible.
The fourth miss blanks. The original numeric deadline anchors the run; repeated
10 ms guard checks do not consume multiple slots. An advanced deadline without
a real fresh presentation cannot rearm the budget.

The existing two-event LCD fence now carries the original measurement expiry
separately from presentation grace. Only physical confirmation before that
measurement expiry advances the fresh-presentation generation. Receiving data,
preparing/submitting a frame, expired copies and nonnumeric menus cannot reset
the counter. Partial views qualify only for valid numeric components actually
shown; malformed valid tags cannot fabricate freshness. Existing IMU retained
pose behavior remains separate, and only a real fresh estimate resets the run.

Source ages/qualities and quantity freshness limits are retained. A held numeric
sample reports `quality=held` in diagnostics, with its increasing real age.
The independent owner, renderer and scanout limits remain 100/250/100 ms; task
watchdogs, frame ownership, pixel/CPU/PSRAM clocks and partitions are unchanged.
No extra frame cache, rendering task or per-frame allocation is introduced.

## Final installed image

Waveshare SKU 29086, verified CH343 UART COM5. ELF SHA256:
`F0122542BD586C1FC43978C3D021FE52DA0DFF22DD1DBC4F4D4F235985FE3D7F`.

Application **1,087,856 B / 4 MiB (25.94%)**. PSRAM free remains 3,730,156 B;
internal heap free 91,563 B, minimum 59,452 B, largest block 31,744 B. Minimum
unused renderer stack is 5,328 B. Heaps stayed stable in each motion window.
Exact images and hashes are retained locally under
`artifacts/bounded-display-hold-20261104/firmware/manifest.json`.

## Checks passed

- **217 Rust workspace/all-target tests** and strict all-target Clippy.
- Policy tests cover slot boundaries, delayed polls, three holds/fourth blank,
  zero-limit mode, unchanged runs after configuration/clock-period changes,
  no fallback before a usable reading, generation wrap and hard liveness gates.
- Fence tests preserve original expiry through physical confirmation and refuse
  freshness renewal for repeated/nonnumeric frames. CLI accepts only 0–10 and
  reports unsupported hardware commands on the desktop target.

Actual-board `check-display-hold.py` changes owner pitch/roll from +10/+20 to
-10/-20 while deliberately preventing drawing or submission. The displayed
sample stays +10/+20 and its age exceeds 100 ms rather than being renewed.
The permitted slots remain lit and diagnostics identify the retained sample.

| Configured X | Injected delay | First blank at miss |
| --- | --- | ---: |
| 0 | Render stall | 1 |
| 1 | Render stall | 2 |
| 3 | Render stall | 4 |
| 3 | Pre-submission transfer stall | 4 |

Every case recovers only after a newly confirmed fresh numeric LCD frame and
resets its run to zero. A separate Altimeter stall with X=10 still trips the
independent 250 ms renderer guard; it is classified as a liveness failure, not
numeric budget exhaustion. Owner updates continue and no reset/timing fault occurs.
Evidence: `qualified-thresholds.json`.

The final **100 rapid all-six CLI switches** pass with no changed blank timestamp
and no budget exhaustion: **31.25 / 31.16 completed FPS** in two 50-switch runs.
The second run records two permitted hold slots and two raw expiry ticks, proving
the new path was exercised. Raw misses were not hidden by resetting counters.
Evidence: `qualified-switch-a.json`, `qualified-switch-b.json`.

All-six steady-motion regression, five seconds each:

| Face | Completed FPS |
| --- | ---: |
| Altimeter | 32.26 |
| Attitude | 32.47 |
| Turn Coordinator | 32.32 |
| Airspeed | 32.26 |
| Heading | 32.42 |
| Vertical Speed | 32.26 |

These windows had no added expiry or blanking and stable heaps.
Evidence: `qualified-memory.json`.

The released rc.7 native connector/isolated worker was tested with the test-only
SimConnect DLL through actual UART/LCD: normal and CPU-contended Attitude,
Turn and Heading; two automatic aircraft reconnects; an 800 ms data interruption;
and explicit Attitude/Turn failures. All **10 cases passed**, at 31.55–32.38 FPS.
Fresh-source performance cases had no expiry/warning interruption. Intentional
stale/failed data still produced warning views and recovered, and the fixture
detected no unintended simulator writes. Evidence: `native/report.json`.
These are local fixture results, not a new real-MSFS aircraft qualification.

The raw evidence directory is `artifacts/bounded-display-hold-20261104/` (ignored
by Git). Build/test logs have prefix `artifacts/bounded-hold-qualified-*`.
The fix addresses the rc.8 automated rapid-switch limit with the user-approved
bounded presentation policy; the old failing records remain historical evidence.
No new Windows installer is required. See [CLI and behavior](../docs/DISPLAY-HOLD.md).
