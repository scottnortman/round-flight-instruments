# Simulator integration architecture — MSFS 2020 first

Checkpoint: `v0.2.0-rc.1`, integration developed from released v0.1.1.
Status: generic provider contract, synthetic/replay bridge, bounded text transport
and external altimeter implemented.
The native MSFS 2020 adapter uses SDK 0.24.5. Its read-only path passed the
reported ground/barometer/pause bench test; full accuracy/flight acceptance is
pending. Optional two-way primary-altimeter control passed a Just Flight Piper
Warrior live bench test with four confirmed writes. Updated integration firmware
is required. Aircraft changes currently terminate the session and require a bridge
restart. The DCS adapter is not implemented.
See [first MSFS test and deployment steps](MSFS-FIRST-TEST.md).
See [run the offline bridge and wire contract](TELEMETRY.md).

Confirmed: MSFS 2020 is the first target; other simulators must fit behind an
abstraction layer. Read-only integration comes first, followed by two-way
Kollsman control. MSFS is not installed on this development PC and is not
required here for portable-core, transport, synthetic-source or replay testing.
The user's son's PC has MSFS 2020 and is the intended live-test machine.
DCS World is the confirmed second simulator target.

## First milestone

Match the selected aircraft's altimeter and barometer setting on the existing
Waveshare board, using a read-only Windows bridge and the current UART link.
Verify disconnect and pause behavior before adding simulator writeback.
Retain the desktop CLI, deterministic onboard demonstration and portable Rust
renderer; source selection must be explicit.

```text
MSFS 2020 / DCS World / replay
                  |
          simulator adapter
                  |
       normalized instrument data
                  |
       shared bridge and diagnostics
                  |
       transport (UART first) -> live source -> gauge
```

## Simulator abstraction

### All instruments, including future additions

The shared `simulator-link::measurements` model covers the six-pack. Its quantity
registry also distinguishes true heading, true airspeed and height above ground.
It is independent of the current board's smaller altimeter transport profile.

| Quantity | Canonical convention | Bench freshness limit |
| --- | --- | --- |
| Indicated altitude / pressure altitude | Feet; separate quantities, no implicit conversion | 1,000 ms |
| Barometer setting | inHg, for the selected instrument channel | 1,000 ms |
| Pitch / roll | Degrees, nose up / right wing down positive | 100 ms |
| Indicated / true airspeed | Knots; separate quantities | 500 ms |
| Magnetic / true heading | Degrees clockwise from the named north reference, [0, 360) | 500 ms |
| Vertical speed | ft/min, climb positive | 500 ms |
| Turn rate | Heading turn rate in degrees/s, right turn positive; not body-axis gyro rate | 100 ms |
| Slip ball | Instrument ball position, [-1, 1], right positive | 100 ms |
| Height above ground | Feet above terrain, distinct from barometric altitude | 1,000 ms |

These are initial explicit bench deadlines, not certified accuracy or latency
claims. Confirm each adapter's signs, coordinate frame, reference, source
instrument and update cadence before enabling the corresponding gauge. A turn
coordinator's aircraft-specific indication must not be passed off as turn rate;
add a distinct quantity if the semantics differ. Deriving any reading requires a
separate documented model with its own validation, rather than a silent adapter
substitution. The current synthetic provider supplies only the three altimeter
quantities; other readings are unavailable.

Each measurement has a quantity key, channel, `f64` value or Missing/Failed state,
and its own acquisition age. A newer altitude must not refresh older attitude.
An observation is a complete bounded batch (24 entries, no per-frame heap
allocation); absent keys clear data. Duplicate keys, non-finite values and values
outside documented engineering envelopes are rejected. Those envelopes are
validation limits, not current dial scales. Display-scale overflow must be
annunciated or rejected, never silently clamped. The compact altimeter profile
has narrower supported ranges and uses `f32`; conversion is explicit and checked.

Capabilities list readable quantity/channel pairs and supported control/channel
pairs. Channel 0 means the profile's selected primary instrument; it is not a
SimConnect index. Aircraft/profile changes start a new session and invalidate
samples and pending controls. Providers are responsible for publishing only
advertised readings. Engine, navigation and other future instruments can add
explicit quantities and units without embedding SDK identifiers in the gauge.
Transport extensions require a version/profile change and acceptance tests;
unknown versions must fail explicitly rather than reinterpret existing fields.

### Rendering and accuracy

Acquisition, validation, state ownership and rendering run independently. The
provider API returns a newly acquired observation or None; a live adapter must
use bounded, nonblocking polling behind this boundary. Native SDK callbacks must
not execute inside the display/state-owner task. Coalesce to the newest complete
observation rather than drain a historical backlog. Keep coupled quantities
(such as pitch/roll from one attitude sample) coherent; unrelated quantities
retain independent ages. Validity/lifecycle changes are never interpolated.

The board retains its immutable snapshots, cached background, per-buffer damage
tracking and supervised scanout. The renderer consumes the latest accepted data
at its own cadence; no UART parsing, SDK calls, allocation or waiting for telemetry
belongs in a render pass. No prediction or interpolation has been added. A 30 Hz
render does not create 30 Hz measurements when a simulator supplies fewer updates.
Any future smoothing must preserve original acquisition age, handle heading wrap,
and have a bounded latency/error specification. Hooks for estimator selection do
not authorize filtering authoritative simulator indications by default.

Measure accepted sample rate, acquisition-to-presentation age, render duration,
completed-frame FPS, queue drops and owner/scanout health separately for every new
gauge. The 30 Hz preference means a 33.3 ms frame budget on the selected hardware;
it remains a measured target, not a promise for arbitrary future graphics. Protect
correctness on overload: discard expired data and show invalidity rather than
hide faults, lower validation, or refresh old readings to sustain animation.

The bridge has a simulator-provider interface and a separate transport interface.
MSFS names, indexes, event IDs, native structures and ABI calls stay inside the
MSFS adapter. The core, telemetry contract and board contain no SimConnect types.

The provider contract covers:

- Connection/session lifecycle, aircraft identity, running/loading/paused state,
  and structured errors. Distinguish source loss from a paused simulation.
- Normalized instrument samples with explicit units, semantics, per-field
  validity, source/session/sequence identity and timing metadata. Missing data
  is unavailable, never zero. Raw flight state and displayed instrument readings
  remain distinct; adapters must not silently substitute one for the other.
- Capabilities: available readings, supported control requests and instrument
  channels. Unsupported controls must be reported rather than assumed to work.
- Later, typed control requests such as setting an altimeter's pressure reference,
  plus request correlation, acceptance/rejection and observed-state confirmation.
  Native simulator events and aircraft-specific logic stay inside the adapter.

A deterministic synthetic provider and a recording/replay provider implement
the same contract first. They let us develop failure handling and run the board
without a simulator installed. An MSFS 2020 provider implements that contract via
SimConnect; DCS World is the second simulator target, behind its own adapter.
The abstraction is a Rust module/trait boundary initially, not a dynamic-plugin
system. Keep simulator SDK dependencies optional and platform-specific so shared
code and replay tests build without them.

The PC bridge owns Windows/SimConnect dependencies. Board firmware knows only
the versioned telemetry/control contract. A later network or binary transport
can replace UART without changing instrument behavior or the renderer.

### Native MSFS 2020 implementation

A data request uses the USER selector (0), while its reply can contain a resolved
object ID (1 in the captured MSFS 2020 run). The decoder validates the definition,
field count, flags, packet bounds and single-object entry metadata. The provider
binds the resolved ID only on a fresh, matching outstanding USER request with
valid values, then rejects an identity change for that session. Cancelled or
unmatched replies cannot bind identity or refresh data. No object ID is hardcoded.
Local decoder rejection is diagnosed separately from a raw SDK HRESULT.


The optional `msfs` Cargo feature adds `simconnect-native`: a narrow C++ ABI shim
compiled against the installed Microsoft SDK header, wrapped by a single-owner
Rust connection. The shim loads an explicitly selected DLL by absolute path,
using its directory and Windows system directories for dependencies. It exposes
open/close, read definitions, lifecycle subscriptions and read requests. The
optional write path lazily maps KOHLSMAN_SET and loads TransmitClientEvent_EX1;
read-only use never invokes or loads it. SDK packet size, definition, object, count and
flags are checked before copying data into Rust-owned values.

The initial profile is the standard primary altimeter only: `INDICATED ALTITUDE`
in feet, `PRESSURE ALTITUDE` explicitly requested in feet (its default is metres),
`KOHLSMAN SETTING HG:1` in inHg, and `PARTIAL PANEL ALTIMETER` as an enum. Failure
or blank flags invalidate the indicated altitude. There is no assumption that
these standard variables mirror a custom aircraft's proprietary avionics.

One data request is outstanding at a time, using `SIMCONNECT_PERIOD_ONCE` and a
unique request ID. All four values arrive in one definition. Age starts at the
host's request time, conservatively including SDK acquisition/queue delay; it
does not claim a precise simulator acquisition timestamp. Late or cancelled
responses cannot refresh readings. A 1-second data deadline, a separate 2-second
connection heartbeat and bounded dispatch processing detect loss/overload.

The provider observes `Sim`, `Pause_EX1`, aircraft/flight load, crash and position
events. It waits for initial lifecycle state; pause cancels pending data and
resume requires a fresh matching reply. Aircraft/flight/reposition changes after
data publication terminate the session. The user restarts the bridge after loading
the desired flight; there is no automatic reconnection with old session state.
SDK exceptions and out-of-profile data also terminate the session with diagnostics.

`msfs::Api` permits deterministic provider tests without Windows or MSFS. Native
decoder fixtures additionally compile against the official SDK to check layout
and malformed packets. Loading the actual DLL and testing the no-simulator
failure path is possible here; none of those tests replaces live cockpit acceptance.

## Deployment without MSFS on this development PC

Recommended first live setup: run the bridge on the Windows PC that runs MSFS
2020, and plug the Waveshare UART USB port into that PC. Development and replay
testing stay here; copy the bridge build to the simulator PC for acceptance.
Live MSFS validation still requires access to a running simulator somewhere.
Use the son's PC for this acceptance step; moving the board there for the first
USB test is the recommended arrangement, pending practical confirmation.

If the board must remain on this development PC, a later LAN relay can carry the
same normalized protocol from the simulator-side bridge to a local UART gateway.
That adds reconnect, queue-age and network configuration work and is not needed
for the first USB milestone. A relay must preserve original sample age rather
than making buffered samples appear fresh at each hop.

Direct remote SimConnect is also documented, but Microsoft's CFG/XML approach
is marked legacy and subject to change. Prefer a local simulator adapter with
our own simulator-independent transport boundary rather than making remote
SimConnect a dependency of the overall design.

## DCS World adapter boundary

DCS is a second provider, not a SimConnect compatibility layer. Evaluate the
community DCS-BIOS Export.lua interface for the selected aircraft: it exposes
cockpit state and controls through module definitions. A custom export adapter
remains an alternative if a required reading is not supported. Neither choice
is an implemented dependency yet.

Aircraft-specific export identifiers, encodings, scales, instrument channels and
control operations belong in the DCS adapter/profile. Convert them to the shared
typed instrument contract before sending anything to the board. Do not assume
that raw altitude, a normalized needle argument and a cockpit's indicated
altitude have interchangeable meanings. Confirm units and barometer semantics
against that module's cockpit. Report missing or disallowed exports as unavailable.

This motivates per-aircraft capabilities: support for altitude readback does not
imply support for barometer readback or writeback. The common bridge must reject
unsupported control requests and must not invent acknowledgements. Aircraft
changes invalidate the prior profile, samples and outstanding requests.

Installation must preserve existing export integrations. Inspect the target
installation/module references before choosing the DCS path; do not overwrite
an existing Export.lua or assume all aircraft expose the same controls.

## Required changes

1. **Live-source contract.** Add a distinct external-simulator source and atomic,
   validated sample updates. The current `sim set` path intentionally republishes
   held values every 10 ms; feeding MSFS into that path would hide a lost link.
   Only newly received source samples refresh live data. Keep telemetry freshness,
   bridge/link liveness and rendering health separate.
2. **Altimeter semantics.** Use MSFS indicated altitude as authoritative for the
   mirrored gauge, with the matching Kollsman setting. Keep pressure altitude as
   a separate diagnostic quantity. Do not apply our bench barometric correction
   to an already corrected indicated altitude. Verify the selected instrument
   index and agreement against the chosen aircraft, including nonstandard QNH.
3. **Windows Rust bridge.** Isolate the official SimConnect ABI behind a small
   adapter. Subscribe to user-aircraft data and simulation lifecycle events;
   publish the newest complete sample at up to 30 Hz when available. SimConnect
   frame callbacks are tied to simulator progress, not a promised fixed 30 Hz.
   Record received data and replay it without MSFS for repeatable tests.
4. **UART transport.** Start at the existing 921600 / 8N1. Define bounded text
   telemetry frames with version, session, sequence, validity/lifecycle flags,
   units and integrity check. Separate telemetry from CLI responses; no character
   echo for the machine stream. One bridge owns the port and provides diagnostics.
   Bound buffers and prefer newest data over queued old frames. Specify the host
   age/board receipt-clock relationship; do not compare unrelated monotonic clocks.
5. **Lifecycle and UI.** Explicitly identify the MSFS source and paused, loading,
   stale and disconnected states. Do not silently fall back to the onboard demo.
   Simulator pause is distinct from a transport timeout; frozen data must never
   appear to be advancing live data. New sessions/aircraft loads invalidate old
   samples and pending controls. Choose and test freshness deadlines against the
   first aircraft and simulator frame rate.

## Kollsman ownership

For the read-only milestone, MSFS owns the barometer setting. The board shows
it but disables the editing popup while that source is selected; local demo mode
retains editing. This prevents a hardware-only adjustment from disagreeing with
the cockpit while displaying MSFS indicated altitude.

In the optional two-way candidate, a touch Apply becomes a versioned request to the
bridge with request/session identity. The active setting changes only after
simulator readback confirms it. Timeout or aircraft/session change rejects the
pending edit. Do not replay requests after reconnection or echo received settings
back into the simulator. Use the selected aircraft's supported control interface;
custom avionics may require an aircraft-specific adapter.

The standard primary profile targets KOHLSMAN_SET parameter 1 = altimeter index 1.
Parameter 0 is integer hPa*16, rounded from hundredths inHg once. The provider
consumes monotonically increasing control IDs, rejects stale/wrong-session requests,
drains queued lifecycle events before writing, and never retries. It cancels any
pre-write data request; only a later read may confirm within 0.002 inHg and 2 s.
The board independently checks matching session/request, fresh readback and a
newer telemetry sequence. Timeout is not proof the simulator ignored the write.

Pause uses measurement-empty state frames every 500 ms while SDK liveness is
healthy. This preserves PAUSED without refreshing or displaying frozen numbers.
The 2-second SDK heartbeat timeout still terminates an unresponsive connection.

QNH digits are now dynamic damage-tracked content: pressure changes do not
invalidate/rebuild the full dial cache. New completed-frame provenance reports
source sequence and age separately from the frame-counter rate. Hardware timing
acceptance, including the prior 310/320 ms host-wire gaps, remains outstanding.

## Acceptance sequence

- Desktop replay: units, range/finite checks, atomic snapshots, malformed frames,
  loss/duplicates/reordering, session changes, paused/loading and stale data.
- Read-only live PC connection: log indicated altitude, pressure altitude and
  Kollsman setting; compare with the cockpit on the ground and during a climb.
- Hardware mirror: test level flight, climb/descent, zero/negative altitude where
  supported, QNH changes, pause/resume, aircraft reload, bridge termination and
  unplug/replug. Confirm failure annunciation and no obsolete samples on recovery.
- Measure completed-frame FPS and source-to-display latency under live traffic;
  aim for 30 Hz, report achieved rate and jitter independently of simulator FPS.
- Add and verify two-way barometer changes only after read-only acceptance.

Before live testing: confirm where the board and MSFS PC will be connected and
select the baseline aircraft for MSFS and, later, the first DCS module.
Neither aircraft choice blocks the shared contract and replay work.
Use the matching installed SDK; do not bundle Microsoft SDK binaries without
checking their redistribution terms. The project remains a simulation peripheral.

## Primary references

- [SimConnect API](https://docs.flightsimulator.com/html/Programming_Tools/SimConnect/SimConnect_API_Reference.htm)
- [Request aircraft data and periods](https://docs.flightsimulator.com/html/Programming_Tools/SimConnect/API_Reference/Events_And_Data/SimConnect_RequestDataOnSimObject.htm)
- [Indicated altitude, pressure altitude and Kollsman SimVars](https://docs.flightsimulator.com/html/Programming_Tools/SimVars/Aircraft_SimVars/Aircraft_System_Variables.htm)
- [Lifecycle events and pause states](https://docs.flightsimulator.com/html/Programming_Tools/SimConnect/API_Reference/Events_And_Data/SimConnect_SubscribeToSystemEvent.htm)
- [MSFS 2020 SimConnect SDK](https://docs.flightsimulator.com/html/Programming_Tools/SimConnect/SimConnect_SDK.htm)
- [MSFS 2020 remote server configuration and legacy caveat](https://docs.flightsimulator.com/html/Programming_Tools/SimConnect/SimConnect_XML_Definition.htm)
- [DCS-Skunkworks DCS-BIOS project](https://github.com/DCS-Skunkworks/dcs-bios)
- [DCS-BIOS developer guide and module definitions](https://github.com/DCS-Skunkworks/dcs-bios/blob/main/Scripts/DCS-BIOS/doc/developerguide.adoc)
