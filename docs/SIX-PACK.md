# Six-pack source checkpoint

Checkpoint: **v0.2.0-rc.8**, merged into `main` from `feature/complete-six-pack`.
Based on `v0.2.0-rc.7`, this carries forward
the latest Airspeed layout work and adds Magnetic Heading and Vertical Speed.
[Heading preview](images/heading-480.png) | [VSI preview](images/vsi-480.png)

All six hardware faces now use the shared renderer, touch menu, blue press
feedback and short active-control chirps. The user reported all tests working
on November 4, 2026. This source checkpoint has no new prebuilt release assets.

## Select and exercise a gauge

Hold the center for **3 seconds**, release, then tap a face. The time remains
configurable with `touch menu hold 3000`. Selection preserves the data source;
it does not start a demonstration or change aircraft controls. Local simulation
and the tray app's T2 stream support all six choices. T1 supports Altimeter
only; IMU bench mode supports Attitude only.

With the tray app disconnected, open UART at **921600 / 8N1 / no flow control**,
terminal local echo off. For a standalone demonstration:

```text
source sim
sim preset level
display on
sim heading 60
```

Use `sim vsi 60`, `sim airspeed 60`, `sim attitude 60`, `sim turn 60` or
`sim takeoff 60` for the other faces. One demo runs at a time. Durations are
5–600 seconds. Each supports `stop` and `status`; stopping holds the current
simulated value. Manual changes to that demo's inputs stop it. Changing source,
pausing, faulting an owned input or resetting the preset also stops it.
These commands cannot replace external or IMU readings.

| Face | Manual CLI examples | Meaning |
| --- | --- | --- |
| Heading | `instrument heading`, `sim set heading_deg 90` | Magnetic degrees clockwise from north; 0 is valid, 360 is rejected by the CLI |
| Vertical Speed | `instrument vsi`, `sim set vsi_fpm 1000` | Positive climb, negative descent, directly in ft/min |
| Airspeed | `instrument airspeed`, `sim set ias_kt 85` | Indicated knots; [limits and scale policy](AIRSPEED.md) |

Heading matches the phone prototype's rotating compass card and fixed aircraft.
The index points to the magnetic heading. East comes under the index at 90°;
north then sits to the left. The heading demo crosses north in both directions
without a false full-circle jump. No true heading, track or unreferenced IMU
fallback replaces missing magnetic heading.

VSI has a conventional zero-left **±2,000 ft/min** face. The numerals are in
hundreds of ft/min: 5 = 500, 10 = 1,000. Climb moves above zero, descent below.
The white pointer has a thin orange outline. Valid readings beyond ±2,000 show
**OVER RANGE** and their actual signed ft/min, with no misleading pegged pointer.
The model has a validated scale-profile boundary for future configuration;
host configuration and persistence remain TBD. The gauge uses direct simulator
vertical speed, without differentiating altitude or applying the barometer offset.

## Correctness and diagnostics

Each quantity retains its own unit, validity and actual age. External Heading
and VSI retain their existing **500 ms** expiry. Local Heading retains 250 ms;
local VSI retains 1,000 ms. Missing, failed or expired inputs remove that face's
numeric indication and display its warning. Other quantities remain independent.
The existing Attitude/Turn short-interruption policy and all watchdogs remain.

`display sample` reports `shown_heading_deg`, `shown_vsi_fpm`, `shown_ias_kt`,
quality and actual presentation ages; `display status` names the current
renderer. `display stats reset` then `display stats stop` measures completed
presentations, rather than drawing calls. `memory status` reports live heap and
stack headroom. [Measured results](../verification/SIX-PACK.md).

## Live MSFS check

The **existing rc.7 Windows tray connector** already transports all-six data.
No new installer, DLL, configuration flag or enrollment is required for these
faces. Its older status text may still describe only the three released faces;
the updated device menu determines availability. The new firmware is required
on the board. Use the existing
[Windows host installation guide](WINDOWS-DRIVER-SETUP.md).

1. Connect the board's UART USB to the simulator PC. Close PuTTY and other
   serial clients, then start the installed tray application and MSFS 2020.
2. Load a complete cockpit flight. Let the connector synchronize; select each
   face with the center menu. Keep each menu open for five seconds before
   selecting, and check that blue feedback and chirps work normally.
3. Compare Airspeed against the cockpit's **indicated** knots, Heading against
   **magnetic** heading, and VSI against vertical speed in **ft/min**. Check
   level flight, climb, descent, turns and a north crossing. A jet above 200 KIAS
   or ±2,000 ft/min will deliberately show the relevant overrange indication.
4. Watch for flicker, tearing, leftover pointers, red X during fresh running
   data, resets or an unresponsive menu. Check all six, especially rapid-motion
   Attitude and Turn. Do not evaluate against paused/loading or missing inputs.
5. On Altimeter only, test barometer Apply and Cancel. Switch faces and aircraft;
   confirm automatic recovery and that selecting a face never changes the plane.
6. If a problem occurs, export diagnostics from the tray app and record the
   selected face, aircraft, cockpit value and approximate time. Include a photo
   or video when possible. The existing release remains the rollback baseline.

Heading, VSI, Airspeed, Attitude and Turn are read-only. Only an explicit Apply
on the displayed Altimeter can request a simulator barometer change. Hidden
gauges continue receiving readings without issuing controls.

An inherited stress-test limit remains: sub-second direct local CLI switching
can briefly trip the 100 ms Attitude/Turn display guard. It is documented in
[verification](../verification/SIX-PACK.md#open-inherited-limit-aggressive-local-cli-switching);
the MSFS-style motion/recovery tests passed. The user has accepted the tested
behavior; this automated stress-test limit remains open.
