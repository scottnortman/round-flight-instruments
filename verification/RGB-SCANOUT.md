# RGB scanout investigation — 2026-11-02

User reported intermittent lines/shift/tearing during Attitude center holds,
artifacts on switching, and a brief dark image while idle. The menu feature is
now visually accepted after the fixes below. Normal framebuffer comparisons and backlight counters
cannot detect every physical scanout fault.

## Reproduction

The installed ESP-IDF v5.5.5 RGB driver automatically restarts DMA when
`bb_eof_count < expect_eof_count` at VSYNC. This can occur without the application
guard switching the backlight off. Espressif describes the related bandwidth and
flicker limitations in its [RGB display documentation](https://docs.espressif.com/projects/esp-idf/en/v5.3.6/esp32s3/api-reference/peripherals/lcd/rgb_lcd.html).

A temporary, tagged `[DEBUG-rgb]` atomic counter was added exactly inside that
short-EOF branch. The CLI diagnostic build exposes `rgb_short_eofs`. There is no
ISR logging. Original SDK source SHA256:
`31F0B6053C21A416F546F2FE84A287FE8C46D0BABA149F99CF987586A9985A14`.
Its exact original bytes are backed up in ignored
`output/gauge-menu/rgb-probe/esp_lcd_panel_rgb.c.original`.

The red-capable hardware command is:

```powershell
python scripts/check-gauge-switch.py --port COM5 --count 50 --check-rgb --report output/gauge-menu/rgb-probe/switches.json
```

With the original configuration it failed after nine switches:
`AssertionError: RGB refill deadline missed; DMA recovery occurred`.
The RGB counter increased 2 to 3 while `blanked_at_ms` and `expired_ticks` stayed
zero. This confirms a driver recovery event invisible to the earlier tests;
the user's subsequent physical check confirmed clean touch, switching and idle.

## Controlled measurements

All candidates below keep CPU/PSRAM/pixel clocks at 240/80/18 MHz and leave the
watchdog, source age, buffer ownership and 100 ms attitude freshness policy intact.

| Bounce lines | Data cache | Result |
| --- | --- | --- |
| 10 | 32 KiB | RGB refill failure after 9 switches; no guard expiry |
| 20 | 32 KiB | No RGB counter increase before stop; numeric expiry at switch 22 |
| 16 | 32 KiB | No RGB counter increase before stop; numeric expiry at switch 44 |
| 16 | 64 KiB | No RGB counter increase; numeric expiry at switch 34; cache experiment rejected |

The remaining expiry cases occur leaving Attitude. Its late-capture scheduler
waited before reading the next snapshot even after another gauge was selected.
A portable regression reproduced the unnecessary 7 ms wait. The candidate now
skips late capture on view changes and checks for a change during the bounded
wait. Original 32 KiB cache is restored. Two 50-switch runs with the scheduling
fix and 16-line buffers passed at 31.01 and 31.23 FPS, with zero RGB recovery,
expiry or blanking events. The 30-second attitude regression passed at 32.35 FPS,
including fault handling and concurrent IMU sampling, with zero RGB recoveries.
The 120-second idle probe also passed. The user then confirmed: "Yes—touch,
switching, and idle are clean." The concurrent 120-second physical-touch capture
passed with ten presented menu generations and 23 chirps, no sound/touch errors,
and no new RGB recovery, blanking or expiry. Its cumulative counters stayed at
RGB 1, blanked-at 52275 ms and expiry 25 from earlier tests; these are unchanged
baselines, not zero lifetime counts.

Reports are in `output/gauge-menu/rgb-probe/`. The 10-line idle probe passed
60 seconds; the separate 180-second idle guard capture also passed. These short
idle runs do not disprove the user's intermittent idle symptom.

## Delivery

The temporary SDK probe was removed, exact original SDK bytes restored (SHA above),
and the SDK static archive rebuilt. SDK Git status is clean. The tagged Rust
getter/status field was removed and normal firmware rebuilt and flashed on COM5.
No SDK changes are needed to reproduce the delivered build. Optional `--check-rgb`
test flags require the diagnostic probe and must be omitted on normal firmware.
See [final validation](GAUGE-MENU.md) for the installed hash and normal-build tests.
