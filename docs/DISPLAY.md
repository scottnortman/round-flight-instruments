# First gauge: simulated altimeter

The first rendered gauge is a three-pointer altimeter. It retains the handoff's
480-unit face geometry, original MS33558C font, numeral-one substitution,
pointer periods, barometer window and SIM INPUT marking. It adds an on-face
numeric bench reference and explicit missing/stale/failed/timing-fault states.
Other selected instruments show NOT IMPLEMENTED and cannot leave the previous
altimeter visible. Touch and the second gauge are subsequent stages.

The Rust face's graphics cleanup moves the numeral centers inward from radius
179 to 169 and raises the Kollsman window from y=217 to y=203 in the logical
480-unit viewport. This adds tick clearance, especially at 9 and 0, and keeps
the window clear of 3 while leaving space below 2. Window contents move with
its frame; font size, tick positions and instrument math are unchanged. These
offsets scale with the viewport. The browser remains the original visual
reference and does not yet incorporate this Rust-face adjustment.

## Boundaries

| Component | Responsibility |
| --- | --- |
| instrument-core | Validated source state, per-quantity quality/age, selected instrument, barometer setting |
| instrument-ui::altimeter | Pure simulated indicated-altitude calculation and pointer angles; no display or hardware dependencies |
| instrument-ui::Snapshot | Immutable readings and retained source age; transport/render delay is added, never reset |
| instrument-ui::raster | Pixel sink trait, bounded framebuffer adapter, viewport scaling, shapes and font drawing |
| instrument-ui::render | Gauge face, pointers, explicit fault/unsupported states, color/orientation pattern |
| firmware/console/src/display.rs | SKU-specific panel initialization, ESP-IDF RGB buffers, presentation worker and independent backlight supervisor |
| console-runtime/text-cli | Typed commands and nonblocking snapshot publication; no drawing in the state owner |

The UI crate is no_std and forbids unsafe code. Its pixel target supports both
individual pixels and batched horizontal spans. Framebuffer spans clip once and
fill contiguous storage. The board uses three 480x480 RGB565 framebuffers in
PSRAM (1,382,400 bytes), two internal 10-line bounce buffers (19,200 bytes), and
a separate 460800-byte CPU-only backdrop: 1,843,200 bytes of image PSRAM in total.
No per-frame application allocation occurs. Platform initialization and console
formatting have separate allocation behavior.

The static backdrop key includes view, barometer bits and test-pattern mode.
`FrameHistory` belongs to exactly one framebuffer and records every dynamic write
in a fixed 16x16 tile map. On reuse, all previously written tiles are restored
before the new overlay is drawn. First use or a background-key change restores
the entire backdrop. History must be reset if a caller changes pixels outside
this renderer. Missing/stale/fault overlays use the same tracking, so invalid
readings cannot retain old hands. This mechanism tracks actual writes rather
than hand-maintained per-gauge damage rectangles. Full-redraw fallback remains
available. Rotating three-buffer output is tested pixel-for-pixel against full
redraw across motion, failures, view/barometer/pattern changes and multiple sizes.

The gauge-independent `FramePipeline` keeps a front buffer, at most one pending
submission and one CPU drawing buffer. Drawing the next frame overlaps the
previous frame's completion wait. One bounded ISR-safe wake queue replaces
10 ms completion polling; the atomic completion counter remains authoritative.
Wake tokens may coalesce. The worker also yields for one 10 ms RTOS tick after
at least 100 ms between explicit yields, checked between renders, so continuous
CPU work cannot indefinitely starve idle tasks. Both idle-task watchdog checks
remain enabled. A first candidate without this yield failed its idle watchdog
and was rejected. Owner/guard/IMU priorities and safety deadlines are unchanged.

The preferred target for EVERY implemented gauge is at least 30 completed
presentations/second during normal animated operation, subject to measured
hardware capability. It is not a certified 33.33 ms worst-case deadline. Cold
background rebuilds, mode changes and safety responses may take longer. Future
gauges must pass their own full-motion and fault tests: a moving attitude
background may dirty most tiles and cannot assume the altimeter's savings.
See [rendering performance](../verification/RENDERING-30HZ.md).

The original font is unchanged; rendered glyph coverage is generated offline.
See [font attribution](../crates/instrument-ui/assets/NOTICE.md). The desktop
example renders the same Rust code at 240, 480 and 720 pixels:

```powershell
cargo run -p instrument-ui --example snapshots --locked
```

Files are written to output/gauge as PPM images. These are reproducible software
renders, not photographs or proof of the physical LCD's appearance.

## Data meaning

The CLI's pressure_altitude_ft remains an uncorrected simulated input. The gauge
applies the browser reference's barometric-offset formula exactly once, inside
the altimeter model. This is a retained **simulation model**, not a calibrated
air-data computation or full ISA inversion. At 29.92 inHg the input and displayed
altitude agree. Increasing the setting increases the displayed altitude.
The setting must remain within 28.10..31.00 inHg; source pressure altitude remains
within -2000..60000 ft. Raw input, display model and barometer setting are separate.

Pointer periods are 1000, 10000 and 100000 ft, clockwise from twelve o'clock.
Negative values wrap each hand correctly and retain their sign in the numeric
readout. Invalid readings produce no pointer angles or numeric altitude. Zero
is valid. An IMU cannot supply altitude, and the unresolved gyro offset remains
isolated from this simulated gauge.

## Safety and presentation

The owner publishes only through a short nonblocking snapshot lock. Drawing and
transfer waits occur on a separate worker. The renderer rechecks source age
before drawing and does not submit numeric frames that expire during rendering.
The guard alone owns GPIO6. It commands the backlight off unless all are true:

- A frame completed and the display has no latched initialization/transfer error.
- The application owner published within 100 ms and has no timing fault.
- The last completed presentation is younger than 250 ms.
- RGB VSYNC activity was observed within 100 ms.
- Any numeric source deadline has not expired (altitude's source lifetime is 1000 ms).
- Display output was explicitly enabled (enabled by default at boot).

The guard runs every 10 ms subject to RTOS scheduling. Thresholds are decision
budgets, not proven worst-case electrical deadlines: observation/scheduling adds
latency. Both state owner and guard subscribe to the one-second task watchdog.
A frozen renderer cannot draw its own fault flag, so it is independently blanked.
A stale source with a healthy renderer produces a visible fault face instead.
Task priorities are explicit: guard 11, state owner 10, IMU worker 6, and
rendering/other pthread workers 5. The board sets the IMU spawn priority and
restores the previous configuration before creating console threads. This lets
I2C completion preempt drawing within the unchanged 10 ms transaction timeout.
Raster code alone is optimized for speed; control/FFI stays on the
validated size profile. These choices were tested after the default main-task
priority allowed a long render to starve the owner and trigger its watchdog.

The driver starts scanning buffer 0, and the CPU starts drawing buffer 1;
buffer 2 allows the next draw to overlap scanout. After a successful submission,
the worker samples the completion counter and requires TWO later bounce-frame
completions before retiring the previous front. Sampling after the API return
also excludes a callback racing before submission. Only the spare buffer is
writable, and no mutable pixel slice survives submission. At most one submission
is pending; stale prepared frames are dropped rather than queued indefinitely.
Their source lifetime is checked again after both rendering and the pending-frame
wait. Metadata retains the original snapshot, timestamp and numeric expiry.

An incomplete pending transfer observed past 100 ms latches failure and blanks.
An already completed transfer observed late after an injected bench stall may
retire, but its numeric expiry is never renewed. Driver buffers and the wake
queue remain allocated until reset even if the worker fails, preventing callbacks
or DMA from touching freed memory. The wake queue stores no task handle or
stack reference. This follows the pinned IDF 5.5.5 bounce-buffer source and its
frame-copy completion semantics; API return alone is not presentation completion.

Scanout callbacks show peripheral progress, not optical panel correctness. Color,
orientation, tearing and actual darkness require physical observation. Shared
CPU, power and RTOS failures are not isolated by separate tasks; no independent
hardware fail-dark circuit or aircraft suitability is established. No runtime
flash writes/OTA are enabled while this bounce-buffer display runs.

The November 1 optical-corruption investigation demonstrated that these counters
can remain healthy while the physical LCD has shifted/duplicated edges. A healthy
`display status` is therefore not an optical acceptance test. Replaying a manual
altitude setting requires physical inspection as well as the software checks;
see [scanout investigation](../verification/DISPLAY-SCANOUT.md).
The verified configuration disables forced restart at every VSYNC and retains
ESP-IDF's conditional recovery for detected incomplete bounce-buffer delivery.
The build script checks the generated setting to prevent accidental re-enabling.

Bench diagnostic `display clock 12` or `display clock 18` requests a 12 or 18 MHz
pixel clock. Other values are rejected. `pclk_requested_hz` and `pclk_applied_hz`
in status show the request and the value after two VSYNC callbacks; these are
software configuration values, not a measured pin frequency. Default is 18 MHz.
The display worker owns the change; the state owner never waits on the hardware.

## Commands

```text
display status
display pattern
display gauge
instrument altimeter
sim preset level
sim resume
baro 29.92
sim set pressure_altitude_ft 12340
fault pressure_altitude_ft on
fault pressure_altitude_ft off
sim pause
sim resume
display off
display on
```

display status reports readiness, latched failure, commanded backlight, frame
count/age, owner age, maximum render/transfer times, worker stack headroom, PSRAM
size and available internal heap. shown_quality/altitude describe the completed
software frame; they must be considered with backlight and age, not used as
operational telemetry. Ordinary status reports display=managed on the board;
selection ACK does not claim optical completion. Desktop console remains
display=off; raster snapshots use the separate shared-renderer example.

Bench fault commands display stall and display transfer-stall pause the worker
before rendering or before submission. display recover releases these injected
stalls. It does not clear real hardware errors or owner timing faults; reboot
is required for those. Acquisition and CLI must continue during these stalls.

```powershell
& D:\rfi-e\python_env\idf5.5_py3.11_env\Scripts\python.exe scripts/check-display.py --report verification/display-local.json
```

This check temporarily blanks the display and changes simulated values, checks
invalid data and independent blanking/recovery, and leaves a 4520 ft simulated
altimeter visible on success. It requests output off on failure. It does not
induce electrical DMA failure; transfer-stall tests the blocked submission path.

## Board provenance

Pins, 18 MHz RGB timing/porches and ST7701 register values are from the
[Waveshare SKU 29086 demo](https://files.waveshare.com/wiki/ESP32-S3-Touch-LCD-2.8C/ESP32-S3-Touch-LCD-2.8C-Demo.zip),
whose ZIP SHA256 is f93eff03664f7dded6f91f9c362fda0d7c870991b4fa99ccbcfdd3e57ce39cf5.
The extracted source is in tmp/imu/demo. Only register data is generated into
panel_init.rs; driver policy, bounded I/O and ownership are implemented here.

The panel command link uses GPIO1/2 and nine-bit words. TCA9554 at 0x20 supplies
LCD reset on bit 0 and command CS on bit 2. The buzzer bit 7 is held low. Other
expander bits are preserved. This initialization borrows I2C before handing that
same driver to the IMU worker, so it does not create a competing I2C controller.
Future touch support still needs coordinated ownership of that bus.

RGB pins: HSYNC38, VSYNC39, DE40, PCLK41, data low-to-high
[5,45,48,47,21,14,13,12,11,10,9,46,3,8,18,17]. PSRAM uses octal mode/80 MHz,
64-byte cache lines, CPU 240 MHz. Build-Console verifies generated settings.
See the pinned [ESP-IDF 5.5.5 RGB driver reference](https://docs.espressif.com/projects/esp-idf/en/v5.5.5/esp32s3/api-reference/peripherals/lcd/rgb_lcd.html)
and local components/esp_lcd/rgb/esp_lcd_panel_rgb.c for buffering and callback
semantics. Actual board revision remains unknown.
