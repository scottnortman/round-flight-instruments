# Windows instrument connector development plan

Branch: `feature/msfs-driver`, based on `v0.2.0-rc.1` (`bd6bdbf`).
Status: first native connector milestone implemented; desktop/fault tests and a
native UART synthetic climb on the Waveshare pass. Live MSFS acceptance of this
new executable remains pending. Fresh-session recovery for aircraft changes and
known simulator-liveness loss is implemented and tested offline/on the board with
synthetic transitions. USB reconnection and the Windows tray interface remain future work.
See [native connector setup](NATIVE-CONNECTOR.md).

## User experience and scope

Provide an installable Windows application for the simulator PC. Users select
their instrument device once, see connection and data status, and can optionally
enable startup at Windows sign-in. Normal operation should require neither Python
nor a terminal. Retain a text CLI and diagnostic export for testing and support.

The first device remains one Waveshare board with one display; instrument
selection and additional gauges must fit the architecture. Multiple boards are
a future extension, not a prerequisite for the first application. Keep the current
firmware and proven bridge available throughout migration.

The host application uses SimConnect out of process. This follows Microsoft's
[recommended application architecture](https://docs.flightsimulator.com/html/Programming_Tools/SimConnect/SimConnect_SDK.htm).
Keep the existing narrow native SDK wrapper and Rust validation. Each SimConnect
connection has one owning thread; the SDK is not thread-safe. A Windows kernel
driver is not part of this work: the existing CH343 serial driver provides USB
access, while the new application connects instrument data to the simulator.

## Component boundaries

```text
MSFS adapter / future DCS adapter / synthetic or replay source
                         |
             typed observations and capabilities
                         |
          session supervisor and instrument subscriptions
                         |
              per-device protocol/profile adapter
                         |
                UART / future transports

Windows UI and diagnostic CLI -> application commands and status
```

- **Simulator providers:** retain `SimulatorProvider` and the generic quantity,
  unit, channel, validity and age definitions. Simulator variable names, aircraft
  mappings and SDK events stay in their adapters. Separate aircraft profiles from
  simulator connection management. Do not treat custom cockpit controls as
  universally compatible with standard SimConnect events.
- **Host runtime:** a reusable Rust library owns sessions, subscriptions,
  device routing and control authorization. Move reusable provider contracts out
  of the current executable package when needed; avoid parallel versions of the
  same validation logic. Windows UI and CLI use this same runtime.
  The first implementation is `crates/connector-runtime`; native transports,
  enrollment, logging and the executable are in `apps/instrument-connector`.
  Provider acquisition and one device's bounded I/O currently share an owner
  thread, matching the old relay. Separate acquisition/device workers before
  adding multiple devices or the tighter attitude freshness profile.
- **Instrument profiles:** declare required quantities, channels, freshness
  budgets and permitted controls. Start with the existing altimeter profile.
  Unsupported attitude, heading or engine profiles must fail explicitly until
  their acquisition, transport and firmware rendering are implemented.
- **Device transport:** expose bounded request/response operations and connection
  state independently of COM APIs. Initially preserve T1/C1/R1 and 921600/8N1.
  A later binary protocol requires explicit version/capability negotiation.
- **Device identity:** persist USB identity/serial rather than assuming COM5.
  Enumerate candidates without opening unrelated ports; enroll the intended board
  and validate the supported firmware protocol before selecting an external source.
  Ambiguous or missing identities require selection. Do not probe arbitrary COM
  devices with gauge commands or trigger resets through DTR/RTS.
- **Presentation:** a thin Windows status/tray interface shows simulator, aircraft
  profile, device, active instrument, data validity and writeback state. It must
  not own simulator timing or block telemetry. Logs and CLI remain available even
  if the graphical interface closes.

## Correctness and recovery

The existing aircraft-change termination is a safety boundary. Implement recovery
around it by creating a new provider/session, not by reusing the old provider's
object identity or simply removing its termination check.

1. Start disconnected; default to automatic simulator-to-gauge synchronization.
   Simulator readings replace gauge values without requiring writeback permission.
   Optional gauge-to-simulator edits are enabled by the user's launch configuration.
2. On simulator loss, aircraft/flight/position change, or device loss, invalidate
   observations, clear device queues and discard pending controls. Best-effort
   invalidation frames complement the firmware's independent freshness timeout.
3. Reconnect with bounded backoff and a fresh session identifier. Re-establish
   the device session and verify capabilities before publishing numeric data.
4. Require fresh valid readings from the newly bound aircraft. Old cached values
   and a healthy connection alone cannot authorize a valid gauge display.
5. Preserve the configured communication direction across recovery. No per-aircraft
   restart, flag change or reauthorization is required. Reacquire fresh readings
   before accepting edits. Simulator data remains authoritative in both modes;
   the new aircraft's QNH must never be overwritten by the previous gauge setting.
6. Never replay a write after reconnect or retry an uncertain simulator write.
   Confirmation requires correlated post-write simulator readback, as today.

Each future device has independent transport progress, sequence state and control
identity. Device-local request IDs must not collide at the shared provider. The
initial implementation supports one enrolled device; do not expose multi-device
writeback until routing and arbitration are tested.

## All instruments and performance

Preserve per-quantity acquisition age and failure status. A new altitude sample
cannot refresh attitude or heading. Maintain explicit units, reference frames,
signs and instrument channels; never silently substitute pressure altitude for
indicated altitude or magnetic heading for true heading.

Subscribe to the union of required quantities and distribute each observation to
interested instruments. Use bounded latest-observation delivery per device, not
an accumulating telemetry FIFO. Any coalesced sample retains its acquisition age;
session changes and control results have separate delivery rules. A slow device
must not stall simulator acquisition or other devices. Unsupported readings stay
missing; there is no automatic synthetic fallback.

Keep the preferred 30 Hz rendering target separate from data acquisition rate.
The current altimeter wire path measured about 25 Hz while rendering averaged
over 30 FPS. Future attitude data has a tighter 100 ms freshness budget. Measure
sampling, transport, source age at presentation and render timing independently
before claiming that profile is supported. UI refresh and log export run outside
the data path; logs must be bounded/rotated and logging failures surfaced.

## Implementation milestones

1. **Native connector:** reusable runtime, native serial transport, enrolled-device
   configuration and CLI. Support synthetic/replay sources and the current MSFS
   altimeter, read-only and opt-in confirmed writeback, without Python.
2. **Session recovery:** simulator absent at startup, flight/aircraft transitions,
   pause/resume and USB removal/reconnect. Use deterministic fault-injection tests
   and then repeat live checks on the simulator PC.
3. **Windows application:** connection/status UI, optional startup, single-instance
   ownership, bounded diagnostics and installation/uninstallation. Review runtime
   redistribution requirements before publishing installer assets.
4. **Expansion:** profile and capability negotiation for additional instruments,
   multiple devices and a DCS provider. Preserve one-board instrument switching.

Acceptance must include wrong/stale session rejection, duplicate control rejection,
no write replay, timeouts, invalid values, ambiguous device selection, slow devices,
malformed transport input and recovery after aircraft changes. Compare the native
connector with the existing bridge using replay and hardware timing evidence.
No MSFS installation exists on the development PC; live aircraft validation still
requires the simulator PC. Add a supported build/run guide with each milestone.
