# Software architecture and baseline: proposal for discussion

Historical design discussion. Use the [root README](README.md) for the current
implementation and [getting-started guide](docs/GETTING-STARTED.md) for setup.

Update: the user authorized the recommended direction and a text CLI on both
desktop and board UART. See [the implemented baseline](docs/BASELINE.md) and
[CLI contract](docs/CLI.md). The discussion below is retained as design context;
its statements about unimplemented execution choices predate the CLI baseline.

Status: scope choices below are confirmed; architecture and detailed feature
proposals remain for discussion. No instrument firmware or display bring-up
is implemented by this document. The user asked to discuss architecture and
features after the factory backup, before proceeding with display work.

## Confirmed requirements

- Safety and correctness are top priorities. Execution-model selection must be
  justified by timing, failure behavior, and verification, not only convenience.
- Rust; portability across display sizes, MCUs, and display controllers.
- First milestone: one physical display with selectable simulated instruments.
- Confirmed topology: one board with one display that switches instruments.
- Confirmed progression: altimeter + attitude first, then all six incrementally.
- Confirmed input approach: touch first, with an encoder possible later.
- Preserve the browser prototype's accepted instrument appearance, font asset
  and attribution, conventions, and separate data-validity behavior.
- Initial board: Waveshare ESP32-S3-Touch-LCD-2.8C, SKU 29086.
- A Rust/ESP-IDF v5.5.5 toolchain has compiled and linked successfully. That
  establishes a viable build baseline, not a validated display configuration.

## Proposed architecture

Use one firmware application, with instruments as independent selectable
modules. Only the selected instrument renders; simulation, data validity, and
settings persist across switches. Share instrument logic and rendering with a desktop simulator.
Keep hardware integration at the edges. Logical boundaries matter more than
creating a separate crate for every small interface.

| Boundary | Owns | Must not own |
| --- | --- | --- |
| Instrument core | Units, input validation, timestamps, freshness, calibration inputs, instrument behavior, aircraft profiles | GPIOs, ESP-IDF, framebuffers, touch drivers |
| Layout | Logical geometry, viewport transforms, round/rectangular clipping, typography and stroke policy | Display-controller commands |
| Renderer | Draw an instrument from a validated snapshot and layout | Sensor reads, filtering, flash writes |
| Application | Selected instrument, menus, user commands, source selection, configuration lifecycle | Sensor fusion hidden inside UI callbacks |
| Data adapters | Simulation first; later simulator bridge and physical sensors | Direct modification of graphics |
| Display backend | Pixel format, buffers, frame presentation, DMA/synchronization, update strategy | Instrument meanings and limits |
| Board support | Pins, clocks, buses, memory, touch/backlight, monotonic clock, storage implementation | Gauge geometry |

Possible initial crate grouping: core; instrument UI (layout and rendering in
separate modules); application; desktop runner; Waveshare runner. Data adapters
can be separate modules/crates according to their platform dependencies. The
Waveshare runner contains separate board-support and display-backend modules.
Shared core should remain no_std-compatible where practical.

Data path: adapter -> validated samples -> instrument state -> layout/renderer
-> display backend. Touch or future encoder inputs become application commands,
such as SelectInstrument, SetBaro, AdjustHeadingReference, and SetBrightness.

## Data and execution rules

- Each quantity has explicit units, source identity, acquisition/receipt timing,
  and validity. Pitch, bank, heading, turn rate, ball, altitude, IAS, and vertical
  speed can fail independently.
- Zero is a valid measurement. Missing, nonfinite, stale, and out-of-range inputs
  remain distinct; they never become a plausible zero reading.
- Evaluate age against a local monotonic clock. A future remote source has its
  own timestamps/sequence numbers; do not assume its clock matches the board.
  Rendering an old sample does not refresh its acquisition timestamp.
- Source meaning is explicit: pressure altitude versus indicated altitude;
  indicated airspeed versus ground speed; relative versus magnetic/true heading.
  Apply barometric corrections at one defined boundary, never twice.
- The first source is deterministic simulation. Gauge selection changes the
  view, not the source lifecycle or calibration state.
- Read inputs independently of rendering. Present a coherent immutable snapshot
  each frame, preserving per-quantity ages. Use bounded buffers; telemetry can
  replace an older unread sample, while user commands require explicit handling.
- Keep instrument calculations synchronous and deterministic. Select the task/
  executor arrangement explicitly before firmware implementation; do not assume
  one combined data/render loop can meet acquisition and fault-detection needs.
  Avoid separate tasks for every gauge.
- Store versioned settings through a storage interface. Coalesce changes before
  writing flash. Begin with defaults and recover cleanly from invalid settings.
- Buffering belongs to the backend. A full framebuffer on this board must not
  become a requirement for every future target.

## Async and Embassy: execution-model discussion

The user explicitly raised Rust async and Embassy in light of safety/correctness.
No executor choice has been accepted or implemented.

Rust async/await describes cooperative tasks (futures); an executor polls them.
Embassy provides an embedded executor and supporting time/synchronization crates.
These are distinct choices from MCU HAL, graphics library, and ESP-IDF versus
bare-metal firmware. Async does not by itself establish deadlines or correctness.

The installed toolchain-check template enables embassy-time-driver and embassy-sync
integration through esp-idf-svc, and directly depends on embassy-time. Its entry
point is synchronous and it does not depend on the embassy-executor crate. Those
template features do not constitute adoption or validation of an Embassy runtime.

Recommended direction for discussion:

1. Keep the portable instrument core as pure/bounded synchronous operations with
   an injected clock. Test math, validity, reference changes, and state transitions
   independently of any scheduler.
2. Keep ESP-IDF as the initial board backend because its build chain is verified
   and it has an RGB display path to evaluate. A move to esp-hal/Embassy should
   compare the exact display, PSRAM, DMA, and touch support, not only language style.
3. Separate acquisition/state publication and health monitoring from rendering,
   settings writes, and logging. Initially use a small explicit set of ESP-IDF
   tasks where blocking driver calls require it. Priorities and bus/CPU budgets
   must be measured; RTOS tasks are not fault-isolated processes.
4. Use async for supported I/O waits when it simplifies concurrent adapters.
   Embassy is a candidate executor, including possible integration above ESP-IDF;
   integration and version compatibility must be tested. It is not mandatory
   merely to use async, and its executor is not a replacement for every driver.
5. Do not run blocking C/driver calls or long rendering work on the same cooperative
   executor as acquisition/health monitoring. An .await does not guarantee a yield:
   a ready future may continue immediately. Executor fairness applies between
   polls, not inside an arbitrarily long poll. Static tasks help bound task memory
   but do not bound every driver allocation or operation duration.

Required engineering checks regardless of executor:

- Define sample/update deadlines, source-age limits, maximum acceptable displayed
  age, and worst-case operation budgets before calling the execution model settled.
  The proposed 30 fps UI target is not a sensor or safety deadline.
- Give peripherals a clear owner, keep inter-task buffers bounded, and define
  overflow behavior. Avoid holding shared locks across waits, drawing, or storage.
- Audit timeout/cancellation paths: dropping a future must not leave DMA using
  released memory or leave a peripheral in an undocumented state.
- Use source and presentation progress independently for health monitoring.
  A live producer does not prove the LCD is updating. A frozen renderer cannot
  be relied upon to draw its own failure flag; define and test watchdog/recovery
  and any independent blanking behavior. Do not blindly feed a watchdog from a
  healthy timer while useful work is stuck.
- Bound steady-state allocation in application hot paths, measure stack/heap
  headroom, and keep unsafe/FFI code at reviewed hardware boundaries. Rust memory
  safety does not prove sensor accuracy, scheduling deadlines, or display behavior.
- Test with a virtual clock and replayable fixtures, then inject delayed/stuck
  I/O, stale/malformed samples, full queues, renderer stalls, and reset conditions.
  Measure under maximum rendering and shared-bus/PSRAM load. A timeout on the
  same stalled executor is not an independent watchdog.

Before selecting the production execution model, compare a minimal ESP-IDF task
arrangement with an async arrangement against the same deadlines and injected
faults. Keep this a scoped verification exercise, not a rewrite of the prototype.

References checked: [Embassy execution model](https://embassy.dev/book/),
[ESP-IDF Rust async service support](https://github.com/esp-rs/esp-idf/blob/master/esp-idf-svc/README.md),
[ESP-IDF 5.5.5 watchdog behavior](https://docs.espressif.com/projects/esp-idf/en/v5.5.5/esp32s3/api-reference/system/wdts.html).

## Proposed first milestone

Deliver the altimeter and attitude indicator, then demonstrate switching between
them on the physical display. This meets the already-confirmed selectable-
instrument milestone without requiring every gauge before the first result.

Include simulated input labeling, explicit failure indications, a simple
instrument picker, barometric adjustment, brightness adjustment, a deterministic
demo/pause/reset path, and a diagnostic view or serial output showing data age,
frame timing, and memory use. Touch is confirmed for the first baseline; physical
encoder hardware is deferred, with an input-command boundary to accommodate it.

Proposed performance goals for initial measurement: 30 rendered frames/second
for moving instruments and visible response to a control within 100 ms. These
are discussion targets, not measured capabilities or accepted requirements.

## Proposed baseline release after the first milestone

| Feature | Proposed scope |
| --- | --- |
| Instrument set | All six: altimeter, attitude, turn coordinator, IAS, heading, VSI; delivered incrementally |
| Display mode | One selected instrument filling the round display |
| Inputs | Deterministic simulated values and fault injection |
| Controls | Instrument selection, barometric setting, heading reference, brightness, demo pause/reset |
| Saved settings | Last instrument, brightness, barometric setting, aircraft profile; versioned defaults/reset |
| Appearance | Existing browser reference; readable typography at other display sizes |
| Failures | Per-quantity missing/stale/invalid indicators and recovery |
| Desktop target | Same core/layout/rendering exercised at 240, 480, and 720 pixel viewports |
| Diagnostics | Firmware/config version, active source, sample age, frame timing, memory |

VSI is new work; it is missing from the supplied browser prototype. The combined
attitude/altitude view is an optional follow-on unless the user selects it for the
baseline. Example C172S airspeed arcs remain an aircraft profile, not universal
instrument constants.

Physical-sensor fusion, absolute-heading sources, air-data sensors, MSFS transport,
multi-device synchronization, OTA updates, and aircraft installation are proposed
later stages. Define their interface boundaries now without implementing those
features in the first simulated baseline.

## Confirmed scope decisions and remaining discussion

The user selected one board/one selectable display, all six instruments delivered
incrementally after altimeter + attitude, and touch first with an encoder later.
The earlier recommendation for multiple independent boards is superseded by the
user's single-board choice. No multi-device discovery, synchronization, or shared
panel service is needed for this baseline.

Detailed features (picker interaction, simulation controls, persisted settings,
diagnostic presentation, combined view, performance targets) remain proposals.
The next discussion should settle those behaviors and the source-data contract,
then define acceptance checks and interfaces before display implementation.
No display work has been started.
