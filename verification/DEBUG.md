# Test.12 telemetry debug and Windows title checks

The eighth persisted gauge is `debug` (wire preference ID 7; existing IDs stay
unchanged). Navigation lists eight gauges with a separate DEMO/INFO/CANCEL row.
DEBUG remains selectable when other gauges lack usable source data.

The debug face displays all ten current MSFS T2 flight quantities, including
distinct indicated and pressure altitudes. Values retain their acquisition age,
quality and source session/sequence. Invalid rows print placeholders. The debug
numeric lease covers every visible non-static reading, including pressure
altitude and QNH. Local static QNH has no acquisition deadline. Existing owner,
render, transfer and display freshness guards remain active. No new polling or
simulator writes were introduced. A dedicated static face cache uses 460,800
bytes of PSRAM, with no per-frame allocation.

Portable regression covers the 999/1,000 ms center-hold boundary on every gauge,
all eight reachable/non-overlapping menu choices, independent raw altitudes,
per-row age/quality and expiry, pressure-setting age in the displayed age metric,
and complete/incremental pixel equality across three rotating buffers at 240 and
480 pixels. Full workspace MSFS-feature tests and strict Clippy are recorded in
`artifacts/debug-screen-tests.log` and `artifacts/debug-screen-clippy.log`.

`scripts/check-screen-demo.py` checks eight-board-gauge rendering, retained DEMO
and IMU ownership, unchanged device UUID, and the actual confirmed presentation
of ten distinct T2 fixture values after exiting DEMO. It also requires the active
board to report `menu_hold_ms=1000`. It does not synthesize physical touch.
The native worker/connector-to-UART timing fixture in `scripts/check-msfs-host.py`
tests normal input, constrained CPU load, failures and aircraft reconnects.
Fixture data is test-only, not live MSFS qualification. Preserve strict expiry
failures in reports even when the existing bounded hold prevents blanking.

The Windows editor uses ASCII titles in both clean and dirty states. A native
UI smoke test checked both titles and selected the eighth DEBUG preview. Its
temporary unsaved draft was discarded; no profile was saved or changed.

Manual acceptance remains: a real one-second finger hold/menu selection on the
board, legibility of ten rows at the intended viewing distance, and cockpit
comparison with MSFS. Use `docs/MSFS-TEST-12.md` with DEMO off and read-only first.
