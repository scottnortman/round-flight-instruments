# Install Round Instruments v0.2.0-rc.7

This prerelease is for **Waveshare ESP32-S3-Touch-LCD-2.8C, SKU 29086**,
480 x 480, 16 MiB flash / 8 MiB PSRAM, and a Windows 10/11 x64 MSFS 2020 PC.
Altimeter, Attitude and Turn Coordinator are implemented. The other three faces
are disabled. The user reported a successful live MSFS 2020 test on November 3,
2026; this does not establish support for every aircraft or MSFS 2024.

The firmware is unchanged from the tested rc.6 image. Existing users with that
firmware and the working preview 6 tray app need no update or uninstall.

## 1. Download and check

From [the prerelease](https://github.com/scottnortman/round-flight-instruments/releases/tag/v0.2.0-rc.7), download:

- `round-instruments-v0.2.0-rc.7-waveshare-29086.zip` for the board.
- `round-instruments-0.2.0-rc.7-setup.exe` for the MSFS PC.
- `SHA256SUMS.txt` to verify both downloads.

In PowerShell, compare each `Get-FileHash <filename> -Algorithm SHA256` result
with its entry in `SHA256SUMS.txt`, then extract the firmware ZIP into a new folder.
The installer is unsigned. It includes the unmodified Microsoft SimConnect DLL
with its Microsoft license and notices; the maintainer confirmed permission to
include it on November 3, 2026.

## 2. Prepare and back up the board (development PC)

Connect the board's **UART USB** connector using a data cable. Check Device
Manager for the CH343 COM port; replace COM5 below with your port. Exit the
Round Instruments tray app, PuTTY and all other serial clients before flashing.
Install the CH343 driver from [Waveshare](https://www.waveshare.com/wiki/ESP32-S3-Touch-LCD-2.8C)
if Windows does not recognize the adapter.

Open PowerShell in the extracted firmware folder. Install Python 3.11 or newer,
then create a local flashing environment:

```powershell
py -3 -m venv .venv
.\.venv\Scripts\python.exe -m pip install -r requirements.txt
.\.venv\Scripts\python.exe -m esptool --chip esp32s3 --port COM5 flash_id
```

Confirm 16 MB flash and the physical board model. Before the first write, save
two full-device reads in a new backup folder; never overwrite an earlier backup:

```powershell
.\.venv\Scripts\python.exe -m esptool --chip esp32s3 --port COM5 --baud 460800 read_flash 0 ALL factory-backup.bin
.\.venv\Scripts\python.exe -m esptool --chip esp32s3 --port COM5 --baud 460800 read_flash 0 ALL factory-backup-check.bin
Get-Item factory-backup*.bin | Select-Object Name,Length
Get-FileHash factory-backup*.bin -Algorithm SHA256
```

Both reads must be 16,777,216 bytes and have identical hashes. Save a second copy
of your backup elsewhere. Backups belong to the individual board; do not upload
them or flash another board's backup.

## 3. Flash and check (development PC)

```powershell
.\.venv\Scripts\python.exe scripts\flash_test_firmware.py --port COM5
```

The helper verifies manifest hashes, image bounds and the 4 MiB application
layout before opening the port. It writes three separate segments at 0x0,
0x8000 and 0x10000, preserving NVS/PHY. Do not combine them into a padded image
or erase the whole flash. Wait for successful verification and restart.

For a standalone check, open a serial terminal at **921600 / 8N1 / no flow
control**, with terminal local echo off, and enter:

```text
sim preset level
instrument altimeter
display on
sim takeoff 60
```

Hold the dial center for **3 seconds**, release, then choose Altimeter, Attitude
or Turn. Unfinished choices stay disabled. The pressure window opens after a
**0.25-second** hold; release, edit, then Apply or Cancel. Blue feedback and a
short chirp indicate active controls. `touch sound off` mutes chirps. Try
`sim attitude 60` or `sim turn 60` for the other motion demos.

Early boot logs use 115200 and may appear garbled in a 921600 terminal. A red X
before supplying simulated or simulator data is expected. Close the terminal
before connecting the tray app. Settings and gauge selection are volatile.

## 4. Install the connector (MSFS PC)

1. Exit any earlier Round Instruments tray app. Run
   `round-instruments-0.2.0-rc.7-setup.exe` for your Windows user; leave startup
   enabled. This upgrades in place and retains settings; no uninstall is needed.
2. The installer **includes SimConnect.dll** and selects that bundled copy
   automatically. No SDK, Python, Rust or Visual Studio is needed on the MSFS PC.
3. Launch Round Instruments from Start. **Choose SimConnect runtime...** is an
   optional override for a different local runtime. The default bundled copy
   needs no manual selection or download.
4. Connect the board's UART USB to the MSFS PC. Select **Use COM5 (...)** in the
   tray menu, using your actual detected port. Leave **Connect automatically**
   enabled. Close other apps that hold this port.
5. Load an unpaused MSFS 2020 flight. Status reports the connected simulator's
   name/build, selected gauge and connection state. Select any implemented gauge
   through its center menu; background instruments remain read-only.
6. Compare each face against the cockpit. Leave **Read-only** unchecked to test
   pressure edits: hold the Kollsman window, release, change the value and Apply.
   The gauge follows simulator confirmation. Simply changing faces never changes
   aircraft controls. In read-only mode, pressure editing is disabled.

The development PC is not needed during normal use. The tray app starts at
sign-in and reconnects automatically. See [the full host guide](WINDOWS-DRIVER-SETUP.md)
for aircraft switching, diagnostics, removal and current simulator limits.

## Diagnostics and recovery

The tray menu offers **Status**, **Open diagnostics folder** and **Export
diagnostics...**. Return the diagnostic ZIP plus aircraft/version and observed
behavior when reporting a problem. Brief Attitude/Turn source delays can retain
the prior pose; sustained stale, missing or failed data displays an invalid-data
warning. Source ages remain real and no demo is substituted for lost MSFS data.

To recover a board, flash and verify its own saved factory backup using esptool
at 0x0. Keep that backup outside the extracted package. Do not change security
settings, eFuses or use `--force` to bypass a mismatch.

This is bench/simulator software, not certified flight equipment.
