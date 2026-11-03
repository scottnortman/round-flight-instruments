# Round Instruments Windows setup (development preview)

**Development preview, not a release.** Both gauges pass over 32 FPS on the
development board with the approved 150 ms external-attitude freshness budget.
The previous 100 ms budget caused blanking; it is superseded for external attitude
only. Local simulation and IMU policies are unchanged. The user accepted the
five-minute native stream and touch switching; live MSFS validation is still required.

Install this on the **PC running MSFS 2020**. The development PC is only needed
to build/update firmware. The simulator PC needs neither Python nor Rust to use
the installed connector. This is a native tray application using SimConnect
and the board's existing CH343 serial driver.

## Before installation

* Windows 10/11 x64, MSFS 2020, Waveshare SKU 29086, connected through **UART USB**.
* Updated firmware supporting `protocol capabilities`: it must advertise **T2**,
  `active_only=true` and `control_claim=true`. Earlier released firmware needs an
  update on the development PC first. The installer does not flash the board.
* Keep the **SimConnect.dll from your working MSFS test package** in a permanent
  folder. Alternatively use your MSFS 2020 SDK runtime. The installer does not
  include Microsoft's runtime. Do not download DLLs from third-party DLL sites.
* Close PuTTY, previous bridge scripts and earlier connector processes.

## Install once

1. Run `round-instruments-0.2.0-preview5-setup.exe`. Install for your Windows user.
   Leave **Start Round Instruments when I sign in** checked.
2. Launch **Round Instruments** from Start. Click its icon in the notification
   area near the clock (it may be under the hidden-icons arrow).
3. Choose **Choose SimConnect runtime...** and select your `SimConnect.dll`.
   An existing SDK at `C:\MSFS SDK` is found automatically.
4. Click **Use COM5 (...)**, substituting the detected Waveshare UART port.
   Selection saves the USB serial identity, so COM-number changes are handled.
   If changing an already connected board, uncheck **Connect automatically**,
   wait a few seconds, select the board, then check it again.
5. Leave **Connect automatically** checked. Load an aircraft into the cockpit.
   The app waits/retries when MSFS or the device is unavailable.
6. Open **Status** to see connection state and the currently displayed gauge.

## Everyday use

Start Windows, connect the board and load MSFS. No terminal command or bridge
script is needed. The app reconnects after aircraft switches or USB loss.
Hold the dial center for **3 seconds**, release, and select **Altimeter** or
**Attitude**. Both receive live data without restarting the connector.
The other four instruments' data is subscribed in the background; their display
faces remain disabled until implemented.

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

Live MSFS acceptance for this expanded six-instrument profile is pending;
previous altimeter-only recovery tests do not validate the new attitude mapping.

## Troubleshooting and removal

Use **Status**, **Open diagnostics folder**, or **Export diagnostics...**.
Export creates a ZIP under `%LOCALAPPDATA%\RoundFlightInstruments` with recent
text logs, configuration, USB identity and local paths; it contains no executable.
An old firmware capability error requires a board update, not re-enrollment.
If COM is busy, close other serial apps. If no UART device appears, check the
cable/port and the official Waveshare CH343 installation instructions.

Exit from the tray menu before flashing firmware or using PuTTY. Uninstall via
Windows Settings → Apps. Startup registration is removed; settings and logs
are retained in `%LOCALAPPDATA%\RoundFlightInstruments` for diagnosis/reinstall.
This preview is unsigned and is not a certified flight instrument.
