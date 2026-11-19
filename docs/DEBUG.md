# Telemetry debug gauge

Hold the center for one second, release, then tap **DEBUG**. This eighth screen
shows the latest values delivered by the host over the existing T2 stream. It
does not add simulator requests, serial polling, logging or aircraft controls.

The rows are indicated altitude (ALT FT), pressure altitude (PALT FT), indicated airspeed (kt), pitch and roll
(degrees), magnetic heading (degrees), turn rate (degrees/second), slip ball
(unitless -1 to +1), vertical speed (ft/min), and QNH (inHg). Positive pitch is
nose up, positive roll/turn is right, and positive vertical speed is climb.
These are the normalized instrument inputs, not every raw SimConnect variable.

Each row shows its acquisition age in milliseconds and state: OK, NONE (missing),
STALE, FAIL, or TIME (timing fault). Only valid values are printed. Invalid
values use `---`; ages above 9,999 ms use `>9999`. QNH is a local static setting
in bench mode and has no sample age. No invalid value is replaced with zero.
The display retains the normal source freshness and presentation watchdogs.

The source caption identifies MSFS 2020, paused/loading/disconnected input,
local bench input, or DEMO. SEQ is the source frame sequence and SESSION identifies
the current source session. A reconnect starts a new session. The display shows
the latest sample each frame, not a history of every received packet.

DEMO remains enabled when switching to DEBUG. **DEMO AUTO / DEMO DATA** identifies
the animated samples; turn DEMO off before comparing with MSFS. DEBUG remains
selectable with missing data, a legacy altitude-only stream, or IMU-only input
so you can inspect which quantities are available. Desktop profile preview uses
fixed local samples and never connects to the aircraft or board.

Test.12 also uses plain ASCII in the profile editor title, including its unsaved
state: `Aircraft profiles - Round Instruments` followed by ` *` while dirty.
