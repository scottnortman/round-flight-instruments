# Flash and RAM budget

Waveshare ESP32-S3-Touch-LCD-2.8C, SKU 29086: **16 MiB flash / 8 MiB PSRAM**.
Measured on 2026-11-03 against `v0.2.0-rc.5` (`168542e`). The memory-layout
update is on `feature/memory-budget`; it includes three implemented faces.

## Flash layout

The project owns [`firmware/console/partitions.csv`](../firmware/console/partitions.csv).
It has one **4 MiB factory application**, preserving the existing data offsets:

| Region | Offset | Size |
| --- | ---: | ---: |
| Bootloader | 0x000000 | Must end before 0x008000 |
| Partition sector | 0x008000 | 4 KiB (3 KiB table plus padding) |
| NVS | 0x009000 | 24 KiB |
| PHY initialization | 0x00f000 | 4 KiB |
| Factory application | 0x010000 | 4 MiB, ends at 0x410000 |
| Unallocated flash | 0x410000 | 11.94 MiB |

This is a USB-flashed single-app layout; it provides no OTA slots. The old 1 MiB
slot was 99.16% full (1,039,792 B). Physical flash was never nearly full.
Current app size and measured RAM results are recorded in
[the verification report](../verification/MEMORY-LAYOUT.md).
The later [continuity candidate](../verification/CONTINUITY.md) uses
1,051,744 bytes (25.08% of the 4 MiB slot), retains 5,131,000 bytes free PSRAM,
and adds no full-size bitmap. The normal build's measured display stack reserve
is about 3 KiB; exact run results are in that report.

Use `scripts/Build-Console.ps1`: it resolves the CSV path for the SDK's generated
CMake project, verifies the generated table and its MD5 against the CSV, and
records matching artifact hashes. Flashing and firmware packaging consume those
verified artifacts. Packages carry bootloader/table/application as separate
segments so padding cannot overwrite NVS/PHY. The package helper checks all
hashes, layout and image bounds before opening the serial port.

The migration was checked by reading flash back: the installed table matches
the generated binary and NVS/PHY are byte-for-byte unchanged. Keep the original
16 MiB factory backup and pre-migration partition/data backup locally.

## RAM architecture and expansion budget

Each 480 x 480 RGB565 bitmap uses **460,800 B (450 KiB)**. Display image storage:

| Allocation | Count | Bytes |
| --- | ---: | ---: |
| Shared DMA scanout buffers | 3 | 1,382,400 |
| Retained face backdrops | 3 | 1,382,400 |
| Shared CPU-only menu cache | 1 | 460,800 |
| **Total image PSRAM** | **7** | **3,225,600** |

Sharing the menu recovers **450 KiB** versus the previous two per-face caches.
Further gauges reuse it and the existing scanout buffers/tasks. The cache retains
its panel/lettering across gauge changes; selection is a small output outline.
Capability changes prepare new button artwork incrementally; unprepared artwork
is never displayed. Pressed artwork is keyed and restored across sessions/views.

The Attitude foreground cache remains **96 KiB of internal RAM**. DMA bounce
buffers use another 45 KiB internally. Increasing them from 16 to 24 lines costs
15 KiB and removed the RGB refill recoveries measured during live/touch traffic.
Do not add another 96 KiB internal cache
per gauge: the measured largest free internal block is only 31 KiB. Put large
future artwork in PSRAM and reuse fonts, transfer buffers and render tasks.

Three more retained backdrops require **1,382,400 B (1.318 MiB)**. With approximately
4.9 MiB of PSRAM free now, that leaves roughly **3.6 MiB**, before future smaller
allocations. This is a projection; exact final six-gauge use and frame rate must
be measured as each remaining face is built. All-six data transport already exists.

## Recheck on the board

At 921600 / 8N1, no flow control:

```text
memory status
display status
sim attitude 30
display stats reset
```

After motion finishes, run `display stats stop`. For a repeatable all-three check,
run the IDF Python with pyserial:

```powershell
python scripts/check-memory.py --port COM5 --seconds 30 --report artifacts/memory-check.json
```

This changes to local simulation, checks heap stability, completed FPS and guard
counters, then restores the level altimeter. It separates steady motion from
gauge entry; the existing rapid-switch limitation is documented in the report.
Statistics count completed
presentations, not merely drawing calls. `memory status` reports the installed
app offset/size and each heap's total/free/minimum/largest block, plus minimum
unused display-task stack. Internal and DMA heaps overlap; do not add them.
Minimum-free readings sum per-region low watermarks, not a simultaneous global
minimum. Task stack space already consumes heap. ELF dummy/alias sections are
not additional RAM. Querying diagnostics does not enable simulation or change sources.

See [development setup](GETTING-STARTED.md) for toolchain/flash commands and
[display design](DISPLAY.md) for buffer ownership and unchanged freshness policy.

References: [Espressif partition tables](https://docs.espressif.com/projects/esp-idf/en/v5.5.3/esp32s3/api-guides/partition-tables.html),
[heap capabilities](https://docs.espressif.com/projects/esp-idf/en/v5.5.3/esp32s3/api-reference/system/mem_alloc.html).
