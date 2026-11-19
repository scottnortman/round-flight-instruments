# Round Instruments 0.2.0-test.11 - MSFS test instructions

This test release updates the Windows host for the current seven-screen firmware
and includes the aircraft-profile editor. Test on the PC running MSFS 2020,
preferably with the previously tested Just Flight Piper Warrior. MSFS 2024 and
other aircraft configurations are not qualified by this release.

Your development board already has the accepted firmware: DO NOT REFLASH IT.
Move that board to the MSFS PC using its UART USB connector. The installer only
updates Windows programs; device enrollment, app settings, profiles and the
board's UUID/settings are preserved. It does not flash the board.

## 1. Install on the MSFS PC

1. Exit the old Round Instruments tray app. Close all terminals/CLI clients using
   the board. Do not uninstall the old app.
2. Verify the setup executable's SHA256 against the supplied checksums:
   `Get-FileHash .\round-instruments-0.2.0-test.11-setup.exe -Algorithm SHA256`.
3. Run round-instruments-0.2.0-test.11-setup.exe over the current per-user
   installation. Keep your preferred Start with Windows setting. This installer
   is unsigned; it includes the unmodified Microsoft SimConnect runtime and its
   license. Rust, Python and the MSFS SDK are not needed for normal host use. The bundled Microsoft DLL still requires the
   Microsoft Visual C++ x64 runtime used by MSFS. If Windows reports missing
   MSVCP140.dll or VCRUNTIME140*.dll, install the official Microsoft Visual C++
   x64 Redistributable before retrying; do not copy individual DLLs.
4. Start Round Instruments once. Tray > Status should show
   0.2.0-test.11 MSFS test release.
5. Keep existing board enrollment. If no board is enrolled: uncheck Connect
   automatically, choose Use COMx for the Waveshare UART, then re-enable Connect
   automatically. The same CH343 serial driver as before is required. USB
   identity enrollment should recover even if Windows changes the COM number.
6. Initially check Read-only (disable all aircraft controls).
7. Start MSFS 2020 and fully load a familiar aircraft into its cockpit. Tray
   Status should show a running MSFS data stream and an increasing frame count.

## 2. Select live data

Hold the board center for 1 SECOND and release. If DEMO has an amber outline,
tap it to turn it off; otherwise close the menu with Cancel. A DEMO caption
means showcase data, even if the host's stream and frame counter are running.
DEMO remains enabled across screen changes, so disable it explicitly before
comparing with the cockpit. Host Status reports Combined flight panel when that
screen is selected. It does not poll the board for DEMO state during streaming.

## 3. Compare all seven screens

Start in a stable flight, then make gentle pitch, bank, speed, climb/descent
and rudder changes. Use matching units/references and allow normal display lag.
Record persistent discrepancies with the exact aircraft/variant.

| Screen | Check |
| --- | --- |
| Altimeter | Indicated feet and QNH in inHg agree. Tap the number window twice to hide/show. Its white outline remains and hands show through the hidden window. |
| Attitude | Nose up/down and right/left bank move in the same direction as the cockpit. |
| Airspeed | Knots indicated agree and move smoothly; compare against KIAS, not mph or true airspeed. |
| Heading | Magnetic heading agrees through the 359-to-0 transition; do not compare against GPS track. |
| Turn coordinator | Left/right rate and ball respond sensibly. Turn rate is not bank angle. |
| Vertical speed | Climb positive, descent negative, near zero in steady level flight. |
| Combined | Attitude, IAS, altitude, magnetic heading and VSI agree with their standalone views; QNH/turn/slip are visible. Distance/ETE have no inputs and are omitted. |

Switch through all seven screens and open/close the one-second menu repeatedly.
Look for smooth motion, complete menu lettering, responsive touch and no
persistent flicker, frozen readings or unexpected blanking. Unavailable aircraft
variables should show missing/failed indications rather than invented zeros.

## 4. DEMO while connected to MSFS

1. Keep the simulator stable, then open the board menu and tap DEMO.
2. Switch through all seven screens: DEMO stays enabled and does not restart.
3. Hold the board upright facing you and tilt gently. Attitude, Turn and the
   combined horizon use fresh IMU tilt (DEMO IMU); unavailable tilt falls back to
   DEMO AUTO. Other readings animate. Demo Turn uses tilt mappings, not measured
   aircraft turn/slip; the biased raw gyro is not integrated.
4. On Altimeter, a pressure-window hold during DEMO must not open the editor or
   change MSFS QNH. The simulator connection/frame count should keep running.
5. Tap the outlined DEMO button again. The latest available cockpit readings
   should return without restarting the host. Compare them again.

## 5. Optional two-way pressure test

After read-only comparisons pass, uncheck Read-only and wait for reconnection.
Select Altimeter and ensure DEMO is OFF.

1. Hold the pressure window 0.25 seconds and release. Change the draft +0.01 inHg
   and Apply: MSFS should change once, and the board should follow its readback.
2. Open again, change a draft, then Cancel: MSFS must not change.
3. Change QNH in the cockpit: the board should follow without an extra altitude
   correction.
4. Re-enable Read-only: a board edit must not change the aircraft. Other screens
   and DEMO must never issue aircraft-control writes.

Restore your original pressure and preferred read-only setting after testing.

## 6. Recovery and profile previews

- Pause/resume. Old readings must not remain valid indefinitely; warnings should
  appear and recover when fresh data returns.
- Change aircraft or reload the flight. The host should recover to the new data
  without replaying an old pressure edit.
- Unplug/reconnect UART USB. Recovery should use the same device identity. The
  board remembers its screen; DEMO resets OFF after a reboot.
- Close/reopen MSFS with the host running; check recovery after the new cockpit
  loads. Allow time for normal reconnect/backoff rather than launching a second
  copy of the host.
- Open Aircraft profiles (preview only)? from the tray. Preview all seven faces,
  including Combined; its controls change pitch/roll with fixed other sample
  values. Save/reopen a test profile and close the editor. Streaming should
  continue. Profiles are LOCAL PREVIEWS in this release: they do not yet configure
  the board or automatically match aircraft.
- Optionally sign out/in and verify Start with Windows. Only one tray app and
  its supervised connector should run.

## 7. Report results and diagnostics

Please include MSFS version; aircraft/add-on/variant; read-only/two-way setting;
which test passed/failed; expected versus observed; and whether DEMO or simulator
pause was active. Add a cockpit/board photo or short clip for persistent value or
direction differences. Export diagnostics from the tray shortly after a failure.

The diagnostic ZIP includes release identity, recent logs, USB enrollment and
local paths. Inspect it before sharing if desired. It includes no executable or
SimConnect DLL. Leave extra CLI/display polling off during normal comparisons;
that polling changes the timing workload.

## Rollback

Exit the test tray app and reinstall the previous working rc.7 Windows installer
over the same installation. Settings/profiles are preserved. Its T2 stream is
compatible with the current board; no firmware downgrade is required. Ensure no
second host/terminal owns the serial port. This release does not add live profile
delivery, multi-device assignment or new simulator adapters.
