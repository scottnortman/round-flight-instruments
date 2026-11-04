# Round Instruments Windows setup (v0.2.0-rc.7 prerelease)

**User-reported live MSFS 2020 test passed on November 3, 2026.** This prerelease
includes Altimeter, Attitude and Turn, touch gauge selection and pressure editing.
See [installation from release downloads](PRERELEASE-0.2.md) and the detailed
[continuity verification](../verification/CONTINUITY.md). The firmware is the
same tested image from rc.6; firmware installation is separate from this installer.

Source checkpoint **v0.2.0-rc.6** is compatible with the existing runtime-inclusive
`round-instruments-0.2.0-preview6-setup.exe` (SHA256
`D049D7B4E7384B804C95EBEEE38C4DD684907072285176D0E19D7C14B3507ACB`).
No new host installer is required for this firmware update. Earlier installations
can be upgraded in place: exit the tray app, then run that installer; uninstalling
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
  update on the development PC first. The installer does not flash the board.
* The supplied **runtime-inclusive installer** contains the unmodified
  Microsoft SimConnect client and its license/notice. The app selects that copy
  automatically. The maintainer confirmed redistribution permission on November 3,
  2026. No SDK, Python or Rust is needed on the simulator PC. Source builds made
  without `-IncludeSdkRuntime` can use **Choose SimConnect runtime...** to select
  a locally installed runtime. Avoid DLL download sites.
* Exit the previous Round Instruments tray app; close PuTTY, bridge scripts and
  earlier connector processes before upgrading.

## Install once

1. Run `round-instruments-0.2.0-rc.7-setup.exe`. Install for your Windows user.
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
Hold the dial center for **3 seconds**, release, and select **Altimeter**,
**Attitude** or **Turn**. The menu temporarily covers the entire gauge so it can
stay responsive without displaying stale numbers. Data continues updating in
the background. Airspeed, Heading and VSI display faces remain unfinished.

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
a pressure change. The displayed value follows simulator confirmation. Cancel,
timeout and unsent edits during gauge/aircraft changes never write to MSFS.
A dispatched edit finishes or times out before the gauge can be changed.

Check **Read-only** in the tray menu to disable all gauge-to-aircraft changes.
Changing this setting reconnects the session; old edits are not replayed.
Unavailable/stale aircraft data shows an invalid-data indication; it never
falls back to a demonstration. Individual aircraft may not expose standard
instrument variables or standard pressure controls.

## Test on the simulator PC

Compare altitude, QNH, pitch and bank with the cockpit. Verify direction during
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
This preview is unsigned and is not a certified flight instrument.
