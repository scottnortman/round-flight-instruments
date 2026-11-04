# Installed Windows connector

Prerelease: **v0.2.0-rc.7**. The user reported live MSFS 2020 success on
November 3, 2026. See [setup](WINDOWS-DRIVER-SETUP.md)
and [verification](../verification/WINDOWS-DRIVER.md).

Current branch: `bugfix/live-gauge-timing`, based on the memory-layout
update. Live MSFS 2020 tests exposed flicker and unusable menus despite 32 FPS.
The fix corrects T2 clock-reference aging, avoids full-face caption rebuilds,
and makes navigation an opaque non-numeric view. The user approved testing
**200 ms for external Attitude and Turn only**; earlier 150 ms bench results are
historical. Local and IMU rules are unchanged, as are true sample ages and the
owner/render/scanout guards. The subsequent live test was reported successful.
See [verification](../verification/LIVE-GAUGE-TIMING.md).

Accepted scope: per-user Windows tray application, starts at sign-in by default;
automatic simulator and USB recovery; no Python or terminal for ordinary use.
Data support for all six standard instruments; Altimeter, Attitude and Turn
faces implemented, remaining faces disabled. Native Rust runtime, isolated SimConnect worker,
existing CH343 USB serial driver. DCS remains a future provider.
The Turn Coordinator development branch also enables Turn using the existing
T2 data; the user reported a successful live MSFS 2020 test on November 3, 2026. Airspeed, Heading and VSI
remain disabled.

Background readings never write aircraft state. Simulator readings are authoritative
on startup, aircraft changes and gauge changes. Only an explicit action on the active
gauge may generate a control (currently barometer Apply). Two-way permission defaults
on for supported controls; offer a read-only setting. Selection itself never writes
simulator settings. Cancel unsent controls on gauge/session change; never replay an
uncertain write or restore an old aircraft setting. Already dispatched writes cannot
be undone: serialize control dispatch and gauge deactivation, track acknowledgment,
and discard late results without issuing compensating simulator writes.

Retain typed quantities, explicit units/channels and individual ages. Subscribe once
to required readings, distribute bounded latest snapshots; no growing telemetry FIFO.
The active face alone renders. Hidden readings stay available without hidden rendering.
Unsupported or failed aircraft variables stay unavailable, never synthetic fallback.
External pitch/roll/turn/ball use a 200 ms freshness budget; local attitude retains 100 ms.
The preferred 30 FPS rendering requirement is unchanged.

Implemented components:
1. Device control ownership and regression coverage.
2. Versioned T2 measurement transport and capability negotiation; preserve T1 support.
3. All-six MSFS readings with failure/sign/unit mapping and bounded acquisition.
4. Active-view control routing, startup/recovery without forcing Altimeter.
5. Windows tray configuration/status, single-instance device ownership, USB reconnect.
6. Per-user installer/uninstaller, startup registration, SDK runtime deployment review,
   diagnostic export, installation and live-test guide.

The tray owns settings and one supervised connector child. The connector owns
serial access and simulator sessions; an isolated worker owns native SDK calls.
Device enrollment stores the USB serial identity instead of a fixed COM number.
Status uses host-side acknowledgments rather than extra serial polling. Settings
and retained diagnostics live under `%LOCALAPPDATA%\RoundFlightInstruments`.
Uninstall removes startup registration and binaries, preserving diagnostics.

T2 is a bounded 512-byte ASCII complete snapshot with CRC32, session/sequence,
quantity IDs, channels, per-field ages and validity. Omitted quantities are missing,
never carried forward. Firmware advertises `T2`, `active_only=true` and
`control_claim=true`; the tray requires these capabilities without downgrading.
Legacy T1 remains available to existing tools. A 32-byte UART FIFO threshold and
buffered reads prevent overflow under longer frames; RGB ownership is unchanged.

| Instrument | Measurements | Canonical units |
| --- | --- | --- |
| Altimeter | Indicated altitude, pressure altitude, Kollsman setting | ft, ft, inHg |
| Attitude | Instrument pitch and bank | degrees; nose up/right bank positive |
| Airspeed | Indicated airspeed | knots |
| Heading | Magnetic heading | degrees, 0 through less than 360 |
| Vertical speed | Vertical speed | ft/min, climb positive |
| Turn coordinator | Turn indicator rate, coordinator ball | deg/s, normalized ball |

The MSFS definition also reads instrument failure flags. Invalid values fail their
own quantities without invalidating unrelated data. Aircraft-specific variable
support and cockpit agreement still require live verification.

For controls, the board issues a session-scoped request and the host claims it
immediately before dispatch. Selection cancels unclaimed requests. A successful
claim briefly blocks gauge changes until confirmation, failure or timeout, closing
the polling-to-dispatch race. Host and worker deadlines reject delayed commands.
Simulator readback remains authoritative; no local altitude offset is applied to
external indicated altitude. A read-only tray setting disables all writeback.

Build with `scripts/Build-WindowsDriver.ps1`, the MSFS 2020 SDK and Inno Setup 6.
The unsigned installer contains native executables, source/dependency notices and
hashes. It does not flash firmware. The rc.7 prerelease
also includes the unmodified Microsoft client runtime and its license/notice;
it selects that runtime automatically. Source builds omit the DLL unless
`-IncludeSdkRuntime` is requested. The maintainer confirmed permission to include
the DLL on November 3, 2026; the accompanying Microsoft terms remain applicable.

Validate hidden gauges produce zero simulator writes, explicit active action produces
one write with correlated readback, old requests cannot replay after transitions,
per-field staleness, invalid wire inputs, simulator/USB loss, worker crash, background
UI/log activity and >=30 FPS live transport on the development board. Aircraft-side
mapping/latency and reconnect acceptance require the MSFS PC.

References: Microsoft SimConnect SDK recommends out-of-process applications and one
owning thread per non-thread-safe SDK connection:
https://docs.flightsimulator.com/html/Programming_Tools/SimConnect/SimConnect_SDK.htm
