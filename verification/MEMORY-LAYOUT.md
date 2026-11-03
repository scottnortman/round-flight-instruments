# Memory layout verification - 2026-11-03

Scope: `feature/memory-budget`, based on `v0.2.0-rc.5` (`168542e`).
Hardware: Waveshare SKU 29086, ESP32-S3 revision 0.2, CH343 UART COM5,
16 MiB flash / 8 MiB PSRAM. CLI 921600/8N1. This is bench validation.

The change expands the factory app slot to 4 MiB, preserves NVS/PHY/app offsets,
shares one CPU-owned menu cache across all faces, and adds query-only
`memory status`. It does not change CPU/PSRAM/LCD clocks, DMA ownership,
watchdog subscriptions, instrument math or numeric freshness limits.

## Layout and update integrity

- Canonical CSV: `firmware/console/partitions.csv`; entries are NVS
  `0x9000/0x6000`, PHY `0xf000/0x1000`, factory `0x10000/0x400000`.
- Generated table MD5 and entries independently verified against the CSV.
- Installed table read back at 0x8000 matches the 3072-byte generated binary.
  SHA256: `FD8026BFF850CA0DEE41C41305160317FFFE604DDA30A9BD5A701AC82D96FA17`.
- NVS/PHY comparison covers 0x9000 through 0xffff, byte-for-byte unchanged.
  SHA256: `1A18623767DA32C6945D41D1EE5C0535776239517EE7E6AA14A313E06BC7A4BB`.
- Pre-migration 32 KiB table/data backup SHA256:
  `464F3957B8284A1370A7BCE21126792C5410729E9E82B049AED1DBCADF1004A1`.
- Factory 16 MiB backup remains local and verified by the guarded flash script.
- Build, guarded flashing and firmware-inclusive packaging verify the same
  table artifacts. Packages write three separate segments, preserving the
  intervening NVS/PHY sectors. No eFuse changes or OTA implementation.

## Software checks

192 Rust workspace tests passed, including the MSFS feature; all-target Clippy
passed with warnings denied. The shared-cache raster regression compares every
pixel across three retained renderers and three rotating framebuffers through
cold menu opening, gauge/session/source changes, pressed states and restoration.
Existing tests cover invalid/stale values, read-only hidden gauges and controls.

Five offline migration/package tests passed: canonical layout, preserved-data
segment boundaries, obsolete app size / moved data, corrupt/truncated tables,
missing MD5, changed CSV/SDK paths, tampered manifests and oversized/bad images.
Three native Windows worker/DLL recovery tests passed without a live simulator.

## Measured resource use and steady motion

Normal ELF SHA256:
`0101153769FB03C9120F6BFB039A1B66108E4209EBA09513298A837A59F8E120`.
Application image: **1,042,112 / 4,194,304 B (24.85%)**, leaving **3,152,192 B**.
`memory status` confirms the running app at 65,536 with size 4,194,304 B.

| Runtime resource | Total | Free | Minimum free | Largest block |
| --- | ---: | ---: | ---: | ---: |
| Internal 8-bit heap | 411,011 B | 111,291 B | 79,180 B | 38,912 B |
| PSRAM | 8,388,608 B | 5,131,000 B | 5,131,000 B | 5,111,808 B |
| Internal DMA (overlaps internal heap) | - | 103,503 B | - | 38,912 B |

Display-task minimum unused stack: **4,128 B** of 20 KiB. Static internal code/data
remains approximately 82.1 KiB outside the registered heap pool. See the budget
for low-water accounting cautions and future allocation guidance.

Thirty-second local motion segments, without diagnostic queries during motion:

| Face | Completed FPS | Maximum draw | Maximum transfer |
| --- | ---: | ---: | ---: |
| Altimeter | 32.32 | 19 ms | 31 ms |
| Attitude | 32.34 | 24 ms | 32 ms |
| Turn Coordinator | 32.31 | 7 ms | 31 ms |

Internal free heap stayed at 111,291 B throughout these segments; PSRAM free and
minimum free stayed at 5,131,000 B across all faces. Shared menu storage recovers
exactly 460,800 B of image allocations. Three further 450 KiB backdrops would
leave approximately **3.57 MiB of PSRAM free**, before smaller future allocations.
The six-face final footprint is a projection, not completed implementation.

Altimeter validation passed missing/failed/stale data, range checks, pressure
correction, rendering/transfer stalls, independent owner progress and recovery.
Attitude validation passed the same data/stall cases plus 30-second motion with
IMU diagnostics running: no added expiry/blanking during its timed motion.
The supervisor still blanks stale numeric frames rather than extending their age.

## Additional board checks

- Two-minute steady Attitude idle: 3,891 completed frames in 120.344 seconds,
  no added blanking or numeric expiry; touch diagnostics/CLI bounds passed.
- Explicit owner watchdog fault build: captured task-watchdog panic, reboot and
  READY. Normal firmware restored and verified afterward.
- Firmware-inclusive package built successfully and its supplied Python helper
  flashed the three separate segments on the board. esptool verified each image
  hash; final flash readback again matched the table and unchanged NVS/PHY.
- Factory backup and pre-migration data backups remain available locally;
  factory-firmware restoration itself was not exercised.

The committed `scripts/check-memory.py` also passed a fresh run from the
packaged installation: Altimeter 32.32, Attitude 32.31, Turn 32.31 completed FPS.
Each 30-second motion window had unchanged free heap, unchanged expiry/blanking
counters and a healthy display. This script is the repeatable memory regression;
view-entry transitions are tested separately rather than hidden in its results.

Physical acceptance: the user confirmed all three touch menu switches, five-second
menu dwell, pressure +/Cancel/29.92/Apply, blue feedback and chirps were correct,
with no visual artifacts or resets. The 120-second capture covered all three
renderers and recorded no added numeric expiry or blanking (counters 9 -> 9 and
blank timestamp 141163 -> 141163); touch bus errors remained zero. The finite
capture ended during the user's menu exercise; the confirmation covers the
completed exercise. Source ages were not reset or extended by navigation.

## External synthetic transport stress finding

The final Python T2 stream test passed Altimeter at 32.31 FPS, then failed the
strict no-expiry assertion for Attitude: 32.23 FPS with two numeric-expiry ticks
and a changed blank timestamp during its 15-second window. Maximum host-observed
telemetry ACK was 51.04 ms. The test stopped on that failure, so Turn was not
reached in this final run. Earlier timed segments passed all three faces, but
that does not supersede the failing trace. Receiver sample ages and the approved
150 ms external budget were preserved. Root cause of this intermittent
external-source expiry is not established by this memory test; investigate
host transport/sample timing and presentation latency before live MSFS acceptance.
See `sixpack-final.json` in the local evidence folder. This is an open finding,
not a zero-flicker external-stream acceptance result.

## Known rapid-switch limitation

The expanded stress test (`check-gauge-switch.py --count 48 --gauges turn
altimeter attitude`) detected brief numeric expiry/guard blanking during
sub-second CLI switching. The unchanged tagged baseline ELF reproduces this
failure too. Its SHA256 is
`5037804BD2ED685983B577C79ADA2B67CD6097FBF6F225F6490217B6140D56D5`.
The baseline used the same bootloader/table/hardware for comparison.

The shared cache initially added cold menu preparation cost. The final design
warms its static panel once and composites selection outlines separately, so a
gauge/session-only change reuses cached lettering. Pixel-reference tests pass.
The inherited rapid-switch deadline failure is still open; this report does
**not** claim that the 48-switch stress test passed. Investigate full-face
restoration / scanout latency for transitions between short-lifetime gauges.
Do not relax freshness limits or renew old samples to hide it.

Raw traces, failed candidates and baseline comparison are retained locally in
`artifacts/memory-layout-20261103/`. Live MSFS acceptance of the all-six profile
remains a separate simulator-PC test. The three remaining display faces are not
implemented by this update.
