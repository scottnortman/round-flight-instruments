# Rendering architecture and 30 Hz target - November 1, 2026

The preferred requirement is at least **30 completed presentations per second**
for every implemented gauge during normal animated operation, where the hardware
can sustain it without compromising validity, supervision or memory ownership.
This is a performance target, not certification or a proven worst-case deadline.

The current altimeter meets that target: **1972 presentations in 61009 ms =
32.32 FPS**, during a 60-second altitude climb plus its one-second ground hold.
The onboard IMU diagnostic worker was running concurrently. Maximum draw time
was 16 ms, submission-to-observed-completion latency 36 ms, owner gap 31 ms.
The user confirmed the sweep was "smooth and clean", with no leftover hands,
tearing, shifted edges or unexpected blanking. Source remains simulated.

## Why the old loop was slower

The previous two-buffer loop serialized drawing and completion waiting. It also
copied the entire 460800-byte backdrop on every frame. The previous accepted
60-second measurement was 15.47 FPS. The new design overlaps these stages and
reduces memory traffic without changing gauge geometry or numeric calculations.

| Change | Purpose |
| --- | --- |
| Three scanout buffers | Draw into a spare while the previous submission completes; never draw into front/pending storage |
| Portable `FramePipeline<T>` | Explicit ownership transitions, at most one pending submission, two post-submit completions before retirement, wrapping counter/timeout tests |
| Bounded ISR wake queue | Wake immediately on completion rather than waiting for a 10 ms polling tick; counter remains authoritative if wake tokens coalesce |
| Per-buffer `FrameHistory` | Restore all previously written 16x16 tiles from the static backdrop; every overlay write is tracked automatically |
| Horizontal spans | Clip and fill contiguous rows once instead of dispatching every filled pixel individually |
| Explicit idle budget | Yield one RTOS tick after at least 100 ms between explicit yields, checked between renders; keep both idle watchdog subscriptions |

The original full-render path remains available for equivalence checks. A new
history or changed background key forces a full restore. The three framebuffers
have independent histories: otherwise a hand last drawn two frames ago could
survive in a reused buffer. Faults, glyphs, clears and overlays all use the same
tracked target. There are no guessed needle bounding boxes.

Memory: three RGB565 frames plus a separate backdrop total 1,843,200 bytes of
PSRAM, versus 1,382,400 previously. The extra frame costs 460800 bytes. Internal
bounce buffers remain 19,200 bytes; tile maps add fixed small storage on the
renderer stack. Observed stack headroom was 4112 bytes. No per-frame application
allocation was introduced. The queue and driver buffers survive worker failure
until reset, so callbacks cannot target a destroyed task/stack allocation.

## Safety properties retained

Owner priority 10, guard 11 and IMU 6 remain above renderer 5. Pixel clock remains
18 MHz, with forced per-VSYNC restart disabled. Watchdog settings, owner 100 ms,
presentation 250 ms, scanout 100 ms and altitude 1000 ms validity thresholds were
not relaxed. No compiler optimization profile was broadened.

The completion counter is sampled AFTER a successful submission; two subsequent
frame-copy callbacks are required. Neither the submitted nor old front buffer is
writable until retirement. The next frame is checked for numeric expiry after
both drawing and any pending-frame wait. Expired prepared frames are discarded.
Retirement records the original snapshot deadline, never a fresh source time.

A pending transfer still incomplete when checked past 100 ms fails closed. A
completed transfer observed late after a deliberate stall can retire, but its
original numeric expiry still applies. Thus `transfer_max_ms` now measures
submission-to-observation latency, not time spent blocked; injected stalls can
make it much larger than 100 ms. Independent presentation expiry still blanks.
This is documented in the CLI contract.

Driver assumptions were checked against the installed ESP-IDF v5.5.5
`esp_lcd_panel_rgb.c`: it supports up to three frames, selects the bounce source
at a frame boundary before its completion callback, and uses the supplied
framebuffer directly. See the [versioned RGB LCD reference](https://docs.espressif.com/projects/esp-idf/en/v5.5.5/esp32s3/api-reference/peripherals/lcd/rgb_lcd.html).
Healthy counters remain insufficient to prove optical correctness.

## Experiments and verification

| Run | Result |
| --- | --- |
| Pipeline without explicit idle yield | Rejected: IDLE0 watchdog fired while renderer occupied CPU0; watchdog was retained and the scheduler budget fixed |
| Pipeline plus idle budget, full backdrop copy | 25.25 FPS, maximum draw 38 ms in a short sweep |
| Pipeline plus tile restoration and spans | Short measurement 32.23 FPS, draw maximum 13 ms; endpoint observation race in test harness identified |
| Final 60-second sweep with IMU running | 32.32 FPS, draw maximum 16 ms, transfer observation maximum 36 ms; passed and user visually accepted |
| Rust workspace | 53 tests passed; Clippy all targets with warnings denied; formatting passed |
| Raster compatibility | All 24 existing PPM fixtures byte-identical before/after span optimization |
| Incremental correctness | 300 rotating-buffer frames compared to full redraw at 240/480/720, odd dimensions and wide viewport; fault, pattern, view and barometer transitions included |
| Tile boundaries | Clipped runs, a one-pixel target, partial edge tiles and full 1024-pixel width restore exactly |
| Display fault checks | Missing/zero/negative/failure/stale, render/transfer stalls, recovery, placeholders, barometer and on/off checks passed |
| IMU integration | Driver/control checks passed; post-sweep sensor status valid with zero recorded bus errors; known stationary gyro offset remains unresolved |
| UART regression | 56 requests passed at 921600; maximum response 78 ms and owner gap 31 ms |
| Desktop sweep | Updated endpoint verification passed |

The test harness previously read `status`, then `sim takeoff status` in separate
requests. The source could complete between them, leaving the first response at
9940 ft and the second complete. It now reads and verifies a fresh valid 10000 ft
endpoint AFTER completion, preserving the exact endpoint requirement. This was
a test observation race, not evidence of a failed source endpoint.

Evidence:

- [Rejected no-yield run](pipeline-rejected-no-yield.json)
- [Budgeted pipeline short sweep](pipeline-budget-takeoff-short.json)
- [Endpoint observation race](pipeline-endpoint-observation-race.json)
- [Final full sweep](pipeline-takeoff-60s.json)
- [Display safety checks](pipeline-display-safety.json)
- [IMU integration](pipeline-imu-integration.json) and [post-sweep status](pipeline-imu-after-sweep.json)
- [UART regression](pipeline-uart.json)
- [Desktop sweep](pipeline-desktop-takeoff.json)
- [Final board state](pipeline-final-state.json): 10000 ft, 29.92 inHg, IMU off,
  simulation running, display healthy; port released with original echo restored

The final sweep's blanking counter stayed at 65791 ms, the timestamp left by
prior explicit fault/on-off tests; it is not a blanking event within the sweep.
The 61-second measurement includes the ground hold and boundary frames. These
numbers measure completed presentations, not actual pin clock frequency, the
screen's optical refresh rate or a worst-case frame interval.

## Applying this to future gauges

The pipeline, batched spans and automatic damage tracking are shared mechanisms.
Each new renderer must still supply correct static-key invalidation and current
source validity. A moving attitude background may touch nearly every tile and
need additional raster optimization; the altimeter result does not promise that
an unimplemented gauge will sustain the same rate.

For each new gauge, compare optimized output with full redraw across its range
and faults, measure a sustained animated run with concurrent sensor/CLI activity,
and check physical appearance. Prefer a 33.33 ms frame budget with rendering
headroom; do not weaken health deadlines or reuse DMA-owned memory to hit it.
Cold initialization, background rebuilds, gauge switches and deliberate failure
responses are outside the normal steady-animation rate claim.

Installed ELF SHA-256:
`34a0913c4433a266fd317cc266febfe80a6f43579b6413fb6e01c4fb8f1312fc`.
Application size is 910880 / 1048576 bytes (86.87%). Partition capacity still
needs attention before substantially expanding the firmware. The normal image,
matching bootloader/config and checksums are retained in the ignored local
`artifacts/rendering-30hz-20261101` directory.
