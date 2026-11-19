# Screen demonstrations

Every screen's center-hold menu includes **DEMO**. Hold the center for one
second, release, then tap DEMO to start showcasing the selected screen. Its
button has an outline while active; presses use the usual blue feedback and
chirp. Open the menu and tap DEMO again to return to normal data. DEMO stays
enabled when selecting another gauge, and its animation continues without a
restart. Cancel just closes the menu.

The screen always identifies demonstration data as **DEMO AUTO** or **DEMO IMU**.
Demo mode is temporary and starts off after reboot; the selected gauge still
uses the normal saved-selection preference.

## Automatic and IMU motion

All eight faces support a repeating one-minute animation. It exercises altitude
from 0 to 10,000 ft, airspeed from 0 to 170 kt, a full magnetic heading rotation,
climb/descent to +/-1,600 fpm, pitch/bank, and turn/slip in both directions.
The sequence demonstrates movement and is not an aircraft flight-dynamics model.

For **Attitude**, **Turn Coordinator** and **COMBINED**, fresh usable onboard
IMU tilt takes priority:

- Attitude and the combined horizon follow the existing smoothed pitch/roll
  bench estimate. Hold the display upright facing you and move it gently.
- Turn Coordinator responds to tilt as a demonstration control: roll divided
  by 10 drives the rate input (limited to +/-6 degrees/second), and roll divided
  by 45 drives slip (limited to +/-1). These are demo mappings, not measured
  aircraft turn rate or slip.
- If an estimate is missing, rejected, stale or the sensor is unavailable,
  animation takes over with the **DEMO AUTO** caption. Fresh usable estimates
  restore **DEMO IMU**. Old IMU receipts cannot renew the input's age.

The current bench estimator uses accelerometer tilt. The gyro's previously
measured bias remains unresolved, so DEMO does not integrate its raw output
into heading or aircraft attitude. The board has no magnetic heading input;
altitude, airspeed and vertical speed also cannot be measured by this IMU.
Those components animate even in the combined IMU demonstration.
See [IMU bench behavior](IMU-ATTITUDE.md) for mounting and estimator details.

Demo automatically requests acquisition when needed and retains it across
screen changes. If it started a previously off IMU, it requests Off when DEMO
is turned off. Acquisition that was already running is
preserved. Explicit self-tests are not interrupted; automatic motion remains
available while they run. An explicit `imu off` during demo also causes automatic
motion until fresh estimates become available again.

## Live source and controls

Demo is a presentation overlay. Local simulator samples, external telemetry,
session/sequence numbers and the live barometer setting keep their original
ownership and ages. The connector can continue receiving live data throughout.
Turning demo off displays the latest available real readings; it does not replay
the readings from demo entry. Explicit source/session changes also end demo.

All gauges are selectable while DEMO is active, including when the live source
lacks a particular reading. After turning DEMO off, unavailable live readings
show the normal missing-data indication.

Barometer editing and aircraft-control dispatch are blocked during demo. An
unsent edit is discarded on entry. A write already claimed by the connector must
finish its bounded confirmation before demo can start. The altitude hide/show
window remains a local display control.

The legacy duration-limited `sim takeoff|attitude|turn|airspeed|heading|vsi`
commands are still available for existing bench procedures. Starting one exits
the menu's screen-demo mode.

## CLI and verification

```text
instrument consolidated
demo on
demo status
demo off
```

`demo status` reports active state, selected screen, `input=imu|automatic|none`,
the underlying live source and `writeback=false`. It does not restart the
sequence. `display status` reports `source=demo` while demo pixels are shown;
`source status` and `status` continue reporting the underlying source.

Generate production-renderer previews with:
`cargo run -p instrument-ui --example consolidated --locked`.
`output/consolidated/menu-demo.ppm` shows the active DEMO button.

The explicit hardware regression is
`python scripts/check-screen-demo.py --port COM5 --seconds 5 --report artifacts/demo-board.json`
using the ESP-IDF Python environment with pyserial. It changes bench source and
selection, exercises sensor acquisition and leaves Combined in DEMO for review.
See [verification](../verification/DEMO.md) for measured results and physical
acceptance still needed.
