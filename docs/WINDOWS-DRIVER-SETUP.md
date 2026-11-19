# Round Instruments Windows setup (v0.2.0)

**The user confirmed the latest firmware and Windows app work as expected on
November 18, 2026.** v0.2.0 includes six standard faces, Combined and DEBUG,
manual live aircraft profiles, touch QNH editing and reviewed IMU calibration.
See [release downloads and installation](releases/v0.2.0.md) and
[release verification](../verification/RELEASE-v0.2.0.md). Firmware installation
is separate from the Windows installer.

The release preserves the tested runtime-inclusive
`round-instruments-0.2.0-test.13-setup.exe` (Windows version **0.2.0.13**) and
November 9 firmware without recompilation. The test.13 app label is expected.
Earlier installations
can be upgraded in place: exit the tray app, then run the installer; uninstalling
first is unnecessary. Device identity/settings are retained. Leave **Read-only**
unchecked to test explicit pressure-window Apply actions.

Install this on the **PC running MSFS 2020**. The development PC is only needed
to build/update firmware. The simulator PC needs neither Python nor Rust to use
the installed connector. This is a native tray application using SimConnect
and the board's existing CH343 serial driver.

## Before installation

* Windows 10/11 x64, MSFS 2020, Waveshare SKU 29086, connected through **UART USB**.
* Updated firmware supporting `protocol capabilities`: it must advertise **T2**,
  `active_only=true` and `control_claim=true`. Earlier released firmware needs an
  update first using the release firmware bundle. Live profiles also require
  its G1 capability. The installer does not flash the board.
* The supplied **runtime-inclusive installer** contains the unmodified
  Microsoft SimConnect client and its license/notice. The app selects that copy
  automatically. The maintainer confirmed redistribution permission on November 3,
  2026. No SDK, Python or Rust is needed on the simulator PC. Source builds made
  without `-IncludeSdkRuntime` can use **Choose SimConnect runtime...** to select
  a locally installed runtime. Avoid DLL download sites.
* Exit the previous Round Instruments tray app; close PuTTY, bridge scripts and
  earlier connector processes before upgrading.

## Install once

1. Run `round-instruments-0.2.0-test.13-setup.exe`. Install for your Windows user.
   Leave **Start Round Instruments when I sign in** checked.
2. Launch **Round Instruments** from Start. Click its icon in the notification
   area near the clock (it may be under the hidden-icons arrow).
3. The bundled runtime is selected automatically. **Choose SimConnect runtime...**
   is an explicit override if you need to select a different local runtime.
4. Click **Use COM5 (...)**, substituting the detected Waveshare UART port.
   Selection saves the USB serial identity, so COM-number changes are handled.
   If changing an already connected board, uncheck **Connect automatically**,
   wait a few seconds, select the board, then check it again.
5. Leave **Connect automatically** checked. Load an aircraft into the cockpit.
   The app waits/retries when MSFS or the device is unavailable.
6. Open **Status** to see connection state, displayed gauge, and **Last connected
   simulator** with application build and SimConnect API version. This comes from
   the actual server handshake, not the SDK version or the DLL filename.

## Everyday use

Start Windows, connect the board and load MSFS. No terminal command or bridge
script is needed. The app reconnects after aircraft switches or USB loss.
Hold the dial center for **1 second**, release, and select a screen. The six
standard faces, **COMBINED** and **DEBUG** are included. The menu temporarily
covers the entire gauge so it can
stay responsive without displaying stale numbers. Data continues updating in
the background. Turn DEMO off before comparing readings with the cockpit.

Open **Aircraft profiles**, select the intended aircraft/variant and click
**Use on connected board**. Wait for its name/revision and **confirmed by board**
in Status. Selection is manual; preview samples never become live readings.
**Use defaults** clears the selection. See [profiles](PROFILE-EDITOR.md).

## Simulator versions

The supported development target is **MSFS 2020 on Windows x64**, including
Steam/Microsoft Store installations; no simulator installation path is needed
for the local SimConnect connection. Installation does not guess the running
simulator from an SDK path. Once connected, Status reports the actual server
application name/build and API version. If multiple versions are installed,
the connected server determines which data is used.

Microsoft documents legacy 2020 SimConnect-client compatibility in MSFS 2024,
but **this connector has not been validated against MSFS 2024**. It is not an
all-versions guarantee, nor an FSX/Prepar3D/DCS interface. Aircraft can expose
nonstandard or missing instrument variables; compare each implemented face
with the cockpit before accepting that aircraft.

Source builders may use `scripts/Build-WindowsDriver.ps1 -IncludeSdkRuntime`
for a runtime-inclusive package. The Microsoft runtime is outside this project's
Apache-2.0 license; keep its accompanying Microsoft license and notice intact.

MSFS values are authoritative. Merely changing gauge, connecting, or loading an
aircraft sends **no aircraft control**. Hidden gauges are read-only. On Altimeter,
hold the pressure window for **0.25 seconds**, adjust, and tap **Apply** to request
a pressure change. On Combined, tap/release the outlined QNH box for the same
editor. The displayed value follows simulator confirmation. Cancel,
timeout and unsent edits during gauge/aircraft changes never write to MSFS.
A dispatched edit finishes or times out before the gauge can be changed.
DEMO and calibration block barometer editing. The retained app may still say
Altimeter-only in Status; the accepted firmware also supports Combined QNH.

Check **Read-only** in the tray menu to disable all gauge-to-aircraft changes.
Changing this setting reconnects the session; old edits are not replayed.
Unavailable/stale aircraft data shows an invalid-data indication; it never
falls back to a demonstration. Individual aircraft may not expose standard
instrument variables or standard pressure controls.

## Test on the simulator PC

Compare all six instruments with the cockpit, including indicated knots,
magnetic heading, signed VSI, altitude, QNH, pitch and bank. Verify direction during
small pitch/roll changes. Change QNH using Apply and confirm the cockpit changes
once. Select Attitude, change cockpit QNH, then return to Altimeter: it should
show the new cockpit value without changing it. Switch aircraft twice and
check recovery, then unplug/reconnect UART USB. Check read-only mode too.

The user reported the latest live MSFS 2020 test as "all working." This is
acceptance of the tested configuration, not all aircraft or simulator versions;
the report did not supply a new aircraft list or detailed test matrix.

## Troubleshooting and removal

Use **Status**, **Open diagnostics folder**, or **Export diagnostics...**.
Export creates a ZIP under `%LOCALAPPDATA%\RoundFlightInstruments` with recent
text logs, configuration, USB identity and local paths; it contains no executable.
An old firmware capability error requires a board update, not re-enrollment.
If COM is busy, close other serial apps. If no UART device appears, check the
cable/port and the official Waveshare CH343 installation instructions.

Exit from the tray menu before flashing firmware or using PuTTY. Uninstall via
Windows Settings > Apps. Startup registration is removed; settings and logs
are retained in `%LOCALAPPDATA%\RoundFlightInstruments` for diagnosis/reinstall.
The installer is unsigned. This is for bench/simulation use, not a certified flight instrument.
