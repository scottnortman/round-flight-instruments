# Install v0.1 on Waveshare SKU 29086

This guide installs the prebuilt Round Flight Instruments firmware on the
**ESP32-S3-Touch-LCD-2.8C**, Waveshare **SKU 29086**, with 16 MiB flash and 8 MiB
PSRAM. It is a bench prototype, not flight equipment. Other round displays
require different firmware even if they use an ESP32-S3.

## 1. Download and connect

Download `round-instruments-v0.1-waveshare-29086.zip` and `SHA256SUMS.txt` from
[the v0.1 release](https://github.com/scottnortman/round-flight-instruments/releases/tag/v0.1).
Compare `Get-FileHash .\round-instruments-v0.1-waveshare-29086.zip -Algorithm SHA256`
with the ZIP entry in `SHA256SUMS.txt`. Stop if they differ. Extract the ZIP and
open PowerShell in its `round-instruments-v0.1-waveshare-29086` folder.

Connect a data USB cable to the board's **UART** connector. Windows Device
Manager should show a **CH343** serial port. Install the vendor's USB driver
from the [Waveshare board page](https://www.waveshare.com/wiki/ESP32-S3-Touch-LCD-2.8C)
if needed. Replace **COM5** in every command below with your board's port.
Close PuTTY and other serial clients before backup or programming.

## 2. Install the flashing tools

Install Python 3.11 from [python.org](https://www.python.org/downloads/windows/).
The following creates a local environment; it needs no Rust or ESP toolchain.

```powershell
py -3.11 -m venv .venv
.\.venv\Scripts\python.exe -m pip install esptool==4.12.0 pyserial==3.5
.\.venv\Scripts\python.exe -m esptool --chip esp32s3 --port COM5 flash_id
```

Confirm the board identity physically and that the command reports **16 MB flash**.
These examples intentionally use the underscore command names of esptool 4.12.0.
Stop on any error; do not add `--force` or change eFuse/security settings.

## 3. Save your original firmware

Read all 16 MiB twice **before the first write**. Do not overwrite an existing
factory backup; use a new folder for each board.

```powershell
.\.venv\Scripts\python.exe -m esptool --chip esp32s3 --port COM5 --baud 460800 read_flash 0 ALL factory-backup.bin
.\.venv\Scripts\python.exe -m esptool --chip esp32s3 --port COM5 --baud 460800 read_flash 0 ALL factory-backup-check.bin
Get-Item factory-backup*.bin | Select-Object Name,Length
Get-FileHash factory-backup*.bin -Algorithm SHA256
```

Both files must be **16,777,216 bytes** with identical hashes. Keep the backup,
hash, board identity and date in a second location. Backups are device-specific;
do not upload them to GitHub or use another board's factory image.

## 4. Flash and verify

The supplied combined image contains the bootloader, partition table and app,
starting at address **0x0**. It replaces the factory app and the covered settings
area. It is not a whole-device backup and does not contain another board's
factory data. The original recovery image remains your way back.

```powershell
.\.venv\Scripts\python.exe -m esptool --chip esp32s3 --port COM5 --baud 460800 write_flash 0x0 round-instruments-v0.1-waveshare-29086.bin
.\.venv\Scripts\python.exe -m esptool --chip esp32s3 --port COM5 --baud 460800 verify_flash 0x0 round-instruments-v0.1-waveshare-29086.bin
```

Wait for successful verification. Press RESET if the application does not start.
If connection fails, hold BOOT, press/release RESET, release BOOT, then retry.
For unreliable transfers try 115200 baud and a different data cable. The
application console speed remains 921600 regardless of flashing speed.

## 5. Start the simulated gauge

Connect a terminal at **921600 / 8N1 / no flow control**, local echo off, or use
this bundled client (only one client can use the port):

```powershell
.\.venv\Scripts\python.exe cli.py --port COM5
```

Then enter:

```text
help
sim preset level
instrument altimeter
baro 29.92
sim takeoff 60
```

The initial red X means altitude is missing, not that installation failed.
`sim preset level` supplies valid simulated values. The climb holds zero for one
second, rises to 10,000 ft over 60 seconds, and then holds. `display stats` shows
completed-frame performance. `sim set pressure_altitude_ft 8500` stops the climb
and holds a manual value. Type `exit` in the Python client to release the port.

Hold the Kollsman window for **0.25 seconds**, then lift. Blue indicates an active
opening press. Use −/+ or **29.92**, followed by **Apply**; Cancel or 30 seconds
of inactivity discards the draft. Input requires an observed finger release;
after startup, lift and retry if the first hold is ignored. Settings are volatile.

Popup buttons turn blue while pressed. Screen presses chirp once; mute with
`touch sound off`, re-enable with `touch sound on`, or inspect `touch sound status`.

Boot diagnostics use 115200 baud and can appear garbled in a 921600 terminal.
Some terminals toggle reset-control lines on connection; wait for startup and
re-enter simulated values. Persistent garbling suggests the wrong port/baud.

## Recovery and development

To restore your own verified backup, use the same esptool `write_flash 0x0`
command with `factory-backup.bin`, then `verify_flash`. Do not use a backup from
a different board. Factory recovery has not been exercised on the development
unit; the original image has two checksum-verified copies.

For source builds, PC simulations, all commands and known limitations, see the
[project README](https://github.com/scottnortman/round-flight-instruments/tree/v0.1)
and [development guide](https://github.com/scottnortman/round-flight-instruments/blob/v0.1/docs/GETTING-STARTED.md).
`scripts/Flash-Console.ps1` in the source tree deliberately guards the original
development board and its backup; it is not the installer for a new owner's board.
