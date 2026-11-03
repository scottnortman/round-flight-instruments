# Windows driver verification — 2026-11-03

Working branch: `feature/windows-instrument-driver`; base `96f693a` / `v0.2.0-rc.4`.
Unreleased development work. **Acceptance is incomplete.**

## Passed

- Rust tests with native MSFS feature: 181 passed (180 workspace tests plus the
  new tray restart regression, with all three tray tests rerun). Coverage includes
  T2 parsing/CRC/ranges/per-field ages, independent field faults, hidden-gauge
  write rejection, control-claim races, cancellation and recovery without replay.
- Workspace/all-target Clippy with MSFS enabled: no warnings.
- Native fixture tests exercise worker crash/reopen recovery, malformed SDK
  packets and all ten six-pack measurements with pitch/bank sign conversion.
- Per-user installer trial: install/uninstall returned zero, default startup
  entry created and removed correctly; text diagnostic export succeeded.
- Installed native Status dialog opened/read/closed through Windows UI automation;
  uninstall stopped the running tray and left no binary/startup entry. A supervisor
  regression verifies child-exit retry/backoff and that pausing prevents restart.
- T2 UART reliability: 8,446 frames over five minutes without syntax failure
  after lowering the receive FIFO threshold to 32 bytes. This was a transport
  result, not a clean display acceptance result.
- Latest buffered-RX/owner-loop board tests: Altimeter approximately 32 FPS
  without deadline expiry. Attitude also approximately 32 FPS.
- Approved 150 ms external attitude policy: core and UI boundary tests preserve
  actual ages, reject stale data at exactly 150 ms and retain local 100 ms policy.
- T2 hardware regression after that update: Altimeter 32.30 FPS, Attitude 32.30 FPS,
  then Altimeter 32.33 FPS, each for 20 seconds. Zero expiry ticks and zero blanking
  throughout, including gauge switches. Report: `artifacts/sixpack-board-150ms.json`.
- Five-minute native stream: 15,624 accepted frames, 52.08 updates/s, maximum ACK
  and sample gap 59 ms, no transport error. User confirmed both gauges and touch
  switching clean, with the earlier flicker gone. Logs:
  `artifacts/driver-150-native/session-1793714246244675`.

The long run is not recorded as a zero-expiry test: its final counters include
eight numeric-expiry ticks (five with a valid source, three invalid), and the last
blank timestamp maps to approximately 110 ms after intentional Disconnected.
No per-event timestamp was captured for the earlier ticks. Visual acceptance
does not disable or bypass the guard; delayed/stopped data still expires normally.

## Superseded failure and outstanding acceptance

Follow-up quick PC-to-board T2 demo on November 3: three 30-second stages passed
at **32.33 FPS Altimeter, 32.30 FPS Attitude, 32.35 FPS Altimeter**. No new expiry
ticks or blanking occurred across the stages; the user confirmed both gauges
smooth and clean. Report: `artifacts/sixpack-quick-demo-20261103.json`.
The board was returned to explicit local Altimeter simulation and COM5 released.

External attitude's original 100 ms deadline was repeatedly exceeded despite
meeting frame rate. Reported presentation ages often reach 77–98 ms, and the
next frame arrives roughly 31 ms later. The display guard therefore blanks.
Increasing receive buffering and scheduling frequency helped transport but did
not eliminate this failure. ACK padding did not help and was removed. The approved
150 ms external policy resolved it in the short regression above.

Visual bench acceptance passed. Live MSFS all-six mapping, Attitude cockpit
comparison, installed-app aircraft/USB recovery, hidden-control isolation and
startup-on-sign-in still need the simulator PC. External attitude uses 150 ms;
local/IMU policies and owner/render/scanout health limits are unchanged.

## Reproduction

Build native programs with `instrument-connector/msfs`; run workspace tests and
`scripts/test_msfs_worker.py`. Build/flash the console to the verified board.
With no other COM5 client, use the IDF Python environment:

```powershell
python scripts/check-sixpack.py --port COM5 --seconds 30 --report artifacts/sixpack-board.json
```

That regression intentionally fails on any numeric expiry/blanking or frame rate
below 30 FPS; it does not weaken validity rules to obtain a pass. It restores an
explicit local Altimeter preset on exit. Ignored reports are under `artifacts/`,
including `driver-tests-latest.txt` and `sixpack-board-timer.json`.
