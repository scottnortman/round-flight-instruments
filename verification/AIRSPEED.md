# Airspeed verification history

Accepted with **v0.2.0-rc.8** on November 4, 2026, following the user's report
that all tests worked. See the [final six-pack verification](SIX-PACK.md) for
the accepted image and current results. The entries below describe earlier
uncommitted development images; their pending status is historical.

## Initial hardware candidate - 2026-11-03

Branch `feature/airspeed-gauge`, uncommitted development work from rc.7.
User-approved baseline: phone-reference 0–200 KIAS, local demo arcs only, with
separate validated profiles for future host configuration. Live limits remain
unconfigured. Physical visual acceptance is pending the user's test.

Initial baseline firmware ELF SHA256:
`B297CFCC843A7B81F2EF1F11BFEFD0A0DE17D794CBF72CC84255324CB94651AA`.
Application 1,062,784 / 4,194,304 bytes (25.34%). Partition hash remains
`FD8026BFF850CA0DEE41C41305160317FFFE604DDA30A9BD5A701AC82D96FA17`.
Board identity: Waveshare SKU 29086, verified CH343 UART on COM5.

## Checks completed

- 210 workspace Rust tests and strict all-target Clippy passed.
- Real UART values: 0, 20, 40, 85, 120, 163 and 200 KIAS; invalid local inputs;
  missing/faulted/paused data; demo endpoint and owner-health checks passed.
- Synthetic T2 packets confirmed IAS 120 with TAS 300 displays 120; omitted IAS
  with TAS present remains missing. IAS 350 shows explicit over-range with the
  actual value. Explicit failure, retained source age and external expiry passed.
- Ten-second airspeed demo: **32.33 completed FPS**, no added expiry/blanking
  during motion. Deliberate missing/stale-data checks are outside that window.
- **50 switches** among all four faces: **31.57 completed FPS**, no added
  expiry, blanking or display-worker failure.

| Steady local face, 5 seconds | Completed FPS |
| --- | ---: |
| Altimeter | 32.28 |
| Attitude | 32.26 |
| Turn | 32.41 |
| Airspeed | 32.28 |

The exact published rc.7 connector was exercised over real UART using the native
MSFS provider, isolated worker and test-only timing DLL:

| Airspeed external case, 15 seconds | Completed FPS | Warnings | Valid-source expiry | Added blanking |
| --- | ---: | ---: | ---: | ---: |
| 10–40 ms SDK reply jitter | 32.15 | 0 | 0 | 0 |
| Same jitter with two CPU competitors | 32.15 | 0 | 0 | 0 |

Both cases passed with no fixture simulator writes. This is a local transport
and rendering qualification, not a new live MSFS aircraft comparison.

Heaps stayed stable across each timed local run. Measured PSRAM free/minimum:
4,664,052 bytes; largest free PSRAM block 4,587,520 bytes. Internal heap
total/free/minimum/largest: 411,011 / 95,795 / 63,684 / 31,744 bytes.
Display-stack minimum free: 2,576 bytes through the four-face regression.
Image PSRAM now comprises eight 480x480 RGB565 bitmaps, 3,686,400 bytes total.
Two future retained face backdrops would consume another 921,600 bytes.

An initial immediate-after-flash CLI request encountered boot/baud-transition
data. A bounded newline/drain resynchronized the bench client after startup;
subsequent checks passed. No firmware reset/panic was found in that startup
capture. Application CLI remains 921600, early boot diagnostics 115200.

Raw evidence: `artifacts/airspeed-20261103/` (ignored), including final flash log,
UART semantic test, memory/motion, switching and native-host timing reports.
Generated previews: `output/gauge/airspeed*-480.png`. No tag, commit, merge,
release or installer replacement is claimed for this development candidate.


## Needle outline and numeral clearance update

The follow-up graphics candidate adds a 1-pixel bright-orange needle contour
and moves the numerals from radius 163 to 148. The title moves down 6 pixels
for clearance from the inset 180. The white needle fill, scale mapping and input
semantics are unchanged. The generated preview was inspected and refreshed.

Installed ELF at this stage:
`F33B8268CD546340451AF8BED370E9046DDAF8EEC6B716103D9EF52D705009A2`.
Application 1,063,152 bytes (25.35% of the 4 MiB slot), partition unchanged.
All 75 UI tests and strict UI Clippy passed, including rotating-buffer pixel
comparisons. The complete UART airspeed check passed again with **32.44 FPS**
in its five-second motion window and no added motion expiry/blanking. Heaps
remained unchanged. Evidence: `graphics-tests.log`, `graphics-clippy.log`,
`graphics-flash.log`, `graphics-board.json` and `graphics-firmware/` under the
ignored airspeed artifact folder. Board left at local 85 KIAS; visual acceptance
is pending. No commit/tag/push was requested for this adjustment.


## Title and lower-label spacing follow-up

The user requested moving AIRSPEED down slightly, and KNOTS, the source caption
and the aircraft-demo label up slightly. AIRSPEED moved from y157 to y165;
KNOTS from313 to305, source343 to335, profile367 to359. The updated raster
preview was inspected and copied into the guide. The installed image at that stage was
`FC96477B52B163A3FAF998406B7E99C9B40266D2E7D480245B0DD24AC08C5CA3`,
with the same 1,063,152-byte application size and partition layout. The board's
sample probe confirmed healthy 85 KIAS on Airspeed after programming. No data,
scale, needle or control behavior changed; no new semantic tests were required.
Evidence: `label-flash.log`, `label-board.json` and `label-firmware/`.


Further user spacing adjustment: AIRSPEED y165 to175; aircraft/profile label
y359 to355. Preview inspected and refreshed, programmed and confirmed healthy
85 KIAS on COM5. Current ELF SHA256 `A94D0AD397854BD4B4527651A4E27FCA87F8AEC71E4FF9D39B420B3C883FA3DC`.
Application size remains 1,063,152 bytes; source/control behavior unchanged.
Evidence: `spacing2-flash.log`, `spacing2-board.json`, `spacing2-firmware/`.
