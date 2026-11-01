# First gauge: simulated altimeter

The first rendered gauge is a three-pointer altimeter. It retains the handoff's
480-unit face geometry, original MS33558C font, numeral-one substitution,
pointer periods, barometer window and SIM INPUT marking. It adds an on-face
numeric bench reference and explicit missing/stale/failed/timing-fault states.
Other selected instruments show NOT IMPLEMENTED and cannot leave the previous
altimeter visible. Touch and the second gauge are subsequent stages.

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

The UI crate is no_std and forbids unsafe code. Its pixel target does not require
a framebuffer. The board backend uses two 480x480 RGB565 framebuffers in PSRAM
(921,600 bytes total), plus two internal 10-line bounce buffers (19,200 bytes).
No per-frame allocation occurs in the application renderer. Driver/platform
allocations occur during initialization; this is not a claim about every library
or console formatting path.

An additional 460800-byte CPU-only PSRAM backdrop caches the static dial face
and barometer window. Its key includes view, barometer bits and test-pattern
mode; each frame restores the complete backdrop before drawing pointers, numeric
altitude, fault indication and the SIM INPUT overlay. Invalid readings cannot
retain previous pointers. The cache is optional and caller-owned in the portable
UI layer; cached/full rendering is compared pixel-for-pixel at 240/480/720 pixels.
Only the original two scanout buffers are ever submitted to DMA.

Normal presentation is paced by the two-completion wait, which always yields
at least one 10 ms RTOS tick. There is no extra fixed sleep after presentation.
Guard deadlines, owner/IMU priorities and transfer timeouts are unchanged.

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

Two PSRAM buffers isolate drawing from scanout. The driver starts on buffer 0;
the CPU starts on buffer 1. After submitting, the worker waits for two complete
bounce-frame callbacks before reusing the former front buffer. The extra callback
avoids a completion racing just before submission. No mutable Rust slice survives
submission. Each wait is limited to 100 ms; timeout latches failure and the guard
blanks. Buffers/driver remain allocated until reset on failures, avoiding DMA
use-after-free. The backend follows the pinned IDF 5.5.5 source's buffer-complete
semantics, not merely the draw-call return value.

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
