# Implemented control/state baseline

Current update: the [first simulated gauge and supervised display backend](DISPLAY.md)
are implemented. Earlier descriptions below of absent rendering and permanently
disabled backlight describe the console-only baseline. The display document
defines current presentation expiry, guard, buffers, and remaining limitations.

Update: [IMU driver and CLI diagnostics](IMU.md) are now implemented in a
separate sensor worker. The simulated gauge source remains unchanged. This
document's statements about absent sensor acquisition describe the earlier
console baseline; see the IMU document for the current diagnostic stage.

The user confirmed Rust, portability, one board/one selectable display, touch
first, and altimeter + attitude before completing the six-pack. Safety and
correctness take priority. The user authorized the recommended architecture
direction and requested a text CLI on both desktop and board UART before faster
communications. This document records that implemented stage; it supersedes
the earlier discussion's wording that all execution choices remain unimplemented.

## Boundaries

| Component | Responsibility |
| --- | --- |
| `instrument-core` | `no_std`, no dependencies or unsafe code; validated commands, private application state, injected monotonic clock, independent freshness/faults |
| `text-cli` | `no_std`, no unsafe code; bounded framing, strict parsing, bounded responses; maps text to typed core commands |
| `console-runtime` | Initial std/RTOS adapter: owner loop plus separate blocking RX and TX threads, bounded queues, progress metrics |
| `desktop-cli` | stdin/stdout runner using the same core/parser/runtime |
| `qmi8658` | Portable bounded sensor register driver, locked reads, self-test and read-format audit |
| `attitude-estimator` | Pure `no_std` filter interface, timing/quality checks, reset lifecycle, optional covariance and bias hooks |
| `imu-service` | Sensor worker, raw diagnostics, replaceable estimator, nonblocking console snapshots; bench tilt only |
| `firmware/console` | ESP-IDF 5.5.5 board adapter, UART0, backlight disabled, owner watchdog subscription/feed |
| `scripts/cli.py` | Interactive host control and repeatable acceptance checks on either target |

The synchronous core has one writer. UI/transport code cannot bypass validation
by writing application fields directly. Renderers will consume validated
readings or snapshots and will not own acquisition timestamps. There is no
renderer in this stage. This remains compatible with introducing an async
adapter/executor later without moving domain logic into asynchronous tasks.

Embassy is not selected as the executor. Initial firmware uses ESP-IDF tasks
because blocking console I/O is straightforward to isolate and test. HAL
dependencies may contain async/synchronization support; that is not evidence of
an Embassy executor. Compare scheduler choices under actual display/driver load
before settling the eventual execution model.

## Safety/correctness properties and limits

1. Missing is distinct from zero. Nonfinite/out-of-range inputs cannot replace
   valid staged inputs. Rejected text never partially applies a command.
2. Each quantity has its own acquisition timestamp and quality. Reading status
   cannot refresh it. Failed/stale/timing-fault readings contain no usable value.
3. The owner aims for 10 ms updates. A gap over 100 ms or a clock regression
   latches a timing fault. Resume cannot clear it; restart creates a new session.
4. Blocking RX/TX and output backpressure do not block the owner. Bounded channel
   allocations occur at startup; this does not prove all platform library calls
   are allocation-free. Worker failure ends normal operation. Desktop EOF drains
   output and exits; draining can wait for its writer.
5. On the board, ESP-IDF's task watchdog subscribes the owner and is configured
   for one second with panic/reset. Only healthy owner progress feeds it. Owner
   error/exit or a latched timing fault stops feeding. This monitors owner
   progress; it does not yet supervise renderer/presentation or detect every
   stalled worker. A blocked output worker is deliberately tolerated so sampling
   can continue.
6. GPIO6 holds backlight PWM low. The manufacturer schematic shows GPIO6 driving
   the low-side backlight MOSFET, so low commands the backlight off. That is a
   console-stage measure, not a verified independent fail-dark design through
   every reset or hardware failure.

The schematic was visually checked using the PDF workflow:
[Waveshare schematic](https://files.waveshare.com/wiki/ESP32-S3-Touch-LCD-2.8C/ESP32-S3-Touch-LCD-2.8C_schematic_diagram.pdf).
Watchdog behavior is based on the version-matched
[ESP-IDF 5.5.5 reference](https://docs.espressif.com/projects/esp-idf/en/v5.5.5/esp32s3/api-reference/system/wdts.html).

The core uses f32 simulation inputs with explicit field units. Typed operational
source adapters, calibration, barometric altitude correction, heading reference,
aircraft-specific envelopes, physical sensor validity, and independent presentation
health are future work. Stored barometric settings are not yet applied to pressure
altitude. All source identity is currently `sim`.

## Verification and next gates

Unit tests cover stale boundaries, zero/missing distinction, invalid input,
independent faults/recovery, clock faults, selection persistence, framing/timeout
recovery, all byte values, broken output, and continued owner ticks under a blocked
writer. The shared acceptance script checks the running desktop/board protocol
and records response times and owner gaps. These checks are evidence about this
baseline only, not certification or a worst-case scheduling proof.

Before display bring-up is accepted: add altimeter/attitude math/reference tests,
preserve the supplied visual conventions/font attribution, define maximum
presented age, measure render/transfer/stack/heap budgets, and inject stuck
render/transfer failures to demonstrate independent blanking/recovery. Then add
touch controls through the typed command boundary. Recheck sampling under full
display/PSRAM/shared-bus load. Persistent settings need versioning and corruption/
power-interruption tests before enabling flash writes.

Before faster transport: define framing, integrity checks, sequence/session rules,
rate limits, timebase mapping, timeout/reconnection behavior, and replay tests.
The current text CLI remains a diagnostic/test interface to the same application
commands. It does not yet provide operational telemetry guarantees.
