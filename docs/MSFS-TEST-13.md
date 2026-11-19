# Round Instruments 0.2.0-test.13 - live profiles and Combined VSI

Install round-instruments-0.2.0-test.13-setup.exe on the MSFS 2020 PC, over
test.12. Exit the old tray app and serial terminals first. Preserve enrollment
and settings. The host includes SimConnect; no Rust, SDK or Python is needed
for normal use. MSFS 2024 remains unqualified.

## Board firmware

This feature needs the accompanying G1-capable firmware. The connected development
board has already been updated. Another board must be flashed using the package's
FLASH-INSTRUCTIONS.md. The Windows installer does not flash firmware. Saved board
UUID, gauge selection and IMU calibration are preserved by the firmware update.
Do not erase NVS or the complete flash.

## Apply the aircraft profile

1. Start Round Instruments, connect the UART USB board, enable Connect
   automatically and initially select Read-only. Fully load the aircraft in MSFS.
2. Open Aircraft profiles from the tray. Select the profile for that exact
   aircraft/variant and check the scale, KIAS limits and source reference.
3. Click Use on connected board. This saves the current form and selects the
   profile for live delivery. The tray app reconnects automatically.
4. Open tray Status. Wait for the profile name and revision followed by
   "confirmed by board". A saved preview alone is not delivery confirmation.
5. Turn DEMO off on the board for cockpit comparison. Select Airspeed: dial
   scale, white/green/yellow arcs and red line must match the saved preview; the
   needle uses live simulator IAS, not the preview's sample value.
6. Change and Save the active profile: a reconnect and a new confirmed revision
   should follow. Changing preview samples alone must not affect the aircraft
   or board readings. Reconnect USB and verify the active profile is delivered
   again. Restart the host and verify the selection is retained.
7. Use defaults in the editor removes the manual selection and reconnects with
   the default gauges. The real airspeed face then shows LIMITS NOT SET.

Selection is manual and remains active until changed. Choose the matching profile
when switching aircraft; simulator package/variant strings are not automatically
matched in this release. Board configuration is session-scoped in RAM and is
reapplied by the host on connection; it does not overwrite IMU calibration.
Profiles can be applied with Read-only enabled because they change display
configuration, not simulator controls.

## Combined VSI

Compare Combined and standalone VSI: zero points left; positive climb moves
above zero and descent below. Both use the profile's VSI full scale and shared
needle mapping. Values beyond the configured scale show OVER rather than a
pegged needle. Compare against cockpit feet/minute, not bank angle or altitude.

## Regression checks

Compare all eight screens and DEBUG values with the cockpit. Test the 1-second
menu, altitude-number hide/show and transparent hands, DEMO retention across
screen changes, then return to live MSFS with DEMO off. Pause/resume, reload or
change aircraft, and unplug/reconnect USB. Missing aircraft variables should
show missing data. Optional: disable Read-only for one +0.01 inHg Apply/Cancel
test, verify simulator readback, then restore the original pressure and setting.

Export diagnostics shortly after failures. Include host version, aircraft and
variant, selected profile/revision and its confirmation status, DEMO state,
expected/observed values and cockpit/board pictures. Earlier IMU fast-rotation
limitations remain separate from live MSFS attitude and are not fixed here.
