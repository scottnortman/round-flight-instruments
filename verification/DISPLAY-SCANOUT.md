# Physical scanout corruption — November 1, 2026 (America/New_York)

The user supplied a photograph showing damaged left-side tick marks and a
duplicated arc below the dial after entering `sim set pressure_altitude_ft 8500`
twice. Software status reported a valid 8500 ft reading, progressing frames,
no timing fault, no latched display error and no backlight blanking. These
counters did not detect the optical corruption.

## Reproduction and probes

`scripts/check-display-optical.py --report REPORT.json` clears altitude, sends
the exact 8500 ft command twice with board echo on, captures status, and leaves
the result visible. It never marks optical success based on counters. The
human observation is recorded separately with `--observe clean|artifact|uncertain`.
This is a Windows/UART adaptation of the diagnosing-bugs skill's HITL loop.

| Probe | Physical observation |
| --- | --- |
| Original firmware, exact repeated command | User confirmed artifacts remain |
| Diagnostic firmware, 18 MHz, same replay | User confirmed artifacts remain |
| Change only clock to 12 MHz without reset, replay | User confirmed artifacts remain |
| Restore 18 MHz, disable forced restart every VSYNC, replay | User confirmed artifacts gone; bottom arc and left tick marks clean |

Evidence: [initial state](display-artifact-initial.json),
[original replay](display-optical-before.json),
[diagnostic 18 MHz replay](display-optical-18mhz.json),
[12 MHz transition](display-clock-12-transition.json),
[12 MHz replay](display-optical-12mhz.json),
[conditional-restart replay](display-optical-no-forced-restart.json).

The portable renderer produces identical full-frame pixels for the first and
second 8500 ft setting, including drawing over the missing face or poisoned
buffer contents. The 480-pixel software render has a clean left edge and bottom
arc. This eliminates that desktop rendering path as the reproduction; it does
not prove optical correctness or fully exclude a target-specific buffer issue.

## Confirmed change and validation boundary

The installed fix retains 18 MHz and the same framebuffers, geometry, handoff and
safety thresholds. It disables `CONFIG_LCD_RGB_RESTART_IN_VSYNC`, which had
forced a GDMA/FIFO restart every frame. ESP-IDF 5.5.5's conditional recovery
remains: its RGB driver checks incomplete bounce-buffer EOF counts at VSYNC
and can restart transmission when needed. The build script now verifies the
generated setting is disabled rather than trusting only sdkconfig.defaults.

The controlled comparison identifies forced per-frame restart as the cause of
the observed corruption in this configuration. The exact low-level FIFO/DMA
offset mechanism was not independently measured. This is not a claim that
forced restart is defective for every panel or ESP-IDF configuration.

Primary references: [Espressif RGB LCD documentation](https://docs.espressif.com/projects/esp-idf/en/v5.5.5/esp32s3/api-reference/peripherals/lcd/rgb_lcd.html),
and pinned local driver `D:/rfi-e/esp-idf/v5.5.5/components/esp_lcd/rgb/esp_lcd_panel_rgb.c`
(`lcd_rgb_panel_try_restart_transmission`), plus the component Kconfig.

The diagnostic clock command accepts only 12/18 MHz, changes at VSYNC, and has
a bounded completion wait. It is not a substitute for the optical acceptance.
New tests cover repeated manual-setting rendering and the restricted clock
command. The original rendering fault cannot be locked down by a host-only unit
test: physical replay remains required for changes to scanout configuration.

The user confirmed the original physical replay is now clean. All 44 Rust tests
passed (43 workspace tests plus the added clock-command test in the targeted
text-cli run); Clippy passed before that test-only addition. Formatting is clean.
Final display safety checks passed for missing/failed/stale input, invalid input
rejection, alternate selection, manual on/off, render stall, transfer stall and
recovery. Host-observed blanking was 469 ms for render stall and 203 ms for
transfer stall; these include command/response/polling and are not electrical
deadline measurements. The owner continued progressing without a timing fault.
The final UART suite passed 56 requests at 921600, with maximum response 63 ms
and owner gap 31 ms. See [safety replay](display-scanout-safety.json) and
[UART replay](uart-scanout-fix.json).
Concurrent IMU enable/restart, register/data audits and estimator checks also
passed while the 8500 ft gauge advanced without additional guard blanking.
The IMU was then turned off. [IMU integration](imu-scanout-fix.json).

Read-only steady observation passed: 121 presentations over 20.14 seconds,
no guard blanking, owner maximum gap 31 ms, render/transfer maxima 131/49 ms.
The board remains at simulated 8500 ft, 29.92 inHg, 18 MHz, IMU off, echo on.
COM5 is released. [Steady report](display-no-forced-restart-steady.json).

Confirmed archive: `artifacts/display-scanout-fix-20261101`.
The earlier candidate snapshot and prior firmware archives are retained.
Image size 894320 / 1048576 bytes (85.29%); ELF SHA256
91424c65d747f209a624513f3d2a0bf7a2ea1a1e05ff464cb5234987affe5655.

Preventing recurrence requires both the generated-SDK configuration check and
physical replay after display-driver changes. Host raster tests and callback
counts alone could not detect this bug. The optical script retains the explicit
human verdict instead of silently promoting counter checks to visual acceptance.
