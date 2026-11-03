# MSFS aircraft-switch recovery test

This package isolates SimConnect in a hidden worker process. If native SDK code
crashes, the connector retains the USB connection, invalidates the old readings,
and automatically starts a new worker/session. It also selects the altimeter
face at startup and recovery. Offline tests reproduce an access violation and
verify automatic recovery. A two-aircraft live test on November 2 also passed
recovery and post-recovery pressure writeback. Use this guide for regression
runs; clean shutdown capture still needs verification.

**Use this new connector first; no firmware update is required** for the
previously tested two-way firmware or v0.2.0-rc.3. A separate firmware change
makes a handshake with no first frame time out to LINK STALE after one second,
instead of staying LOADING forever. That change is on the current recovery branch
and has passed hardware tests; it is not included in this host-only ZIP. It does
not restore simulator data. The development board was updated after the successful
two-aircraft MSFS test.

## Simulator PC only (your son's PC)

1. Extract this entire new ZIP into a new folder. Close the old connector, Python
   bridge and any serial terminal. Connect the board's **UART USB** port.
2. Copy the previously working `device.json` into this new folder. It must refer
   to this board. If it is unavailable, list and enroll the correct port:

   ```powershell
   .\instrument-connector.exe devices
   .\instrument-connector.exe enroll --port COM5 --config device.json
   ```

   Substitute the port shown for this board. Do not overwrite an existing config.
3. Load the Just Flight Warrior cockpit in MSFS 2020 and unpause. In PowerShell,
   opened in the extracted folder, run the supplied capture launcher:

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File .\Run-MSFS-Recovery-Test.ps1 -Writeback
   ```

   The launcher uses this package's executable and automatically saves console
   output and the process exit code under `test-results/run-<timestamp>/`.
   Normal native diagnostics appear as plain text, without PowerShell's
   misleading `NativeCommandError` decoration. Actual exit codes are preserved.
   No `--diagnostics` or `--frame-timing` flag is needed: this build captures one
   readback per fresh session. `-Writeback` enables touch edits sent to MSFS;
   omit it for read-only synchronization. The connector selects the altimeter.
4. Confirm board altitude and pressure match the cockpit. Change pressure once
   on the gauge and confirm cockpit readback. Note the observed values.
5. Leave the connector running. Switch to the F/A-18, load/unpause the cockpit,
   and wait **at least 60 seconds after the cockpit is usable**. Note whether the
   board shows ALT, a red X, or a number, and the exact text below any X. If
   practical, change altitude briefly so a stationary ground value is not
   mistaken for frozen data. Compare against the cockpit's primary altimeter.
6. Switch to a third aircraft, load/unpause, and repeat the same observation.
   If a failure occurs, note which switch, the wall-clock time and exact screen
   message. Keep the connector running for the full observation interval.
   A brief red X during loading is expected. A persistent red X after the cockpit
   is running is a failure; collect logs before restarting the connector.
7. Press Ctrl+C once to stop if the connector has not already returned. Wait for
   the launcher to print `Saved exit status and console output`. If the connector
   exits by itself, the launcher saves its exit code immediately. Closing the
   PowerShell window before the child process returns can still prevent capture.
   If it hangs, note the last console line and how long you waited; do not
   describe a forced close as a clean exit.
8. Return **the complete new `logs` and `test-results` folders**, plus
   `manifest.json` and your observations. In the package folder you can create:

   ```powershell
   Compress-Archive -Path .\logs,.\test-results,.\manifest.json -DestinationPath .\recovery-capture.zip
   ```

   Use a new ZIP name for each attempt. Include `summary.json` when present.
   There is no need to include executables or SimConnect.dll in the returned ZIP.

The development PC is only needed to build packages and analyze the returned
logs. The live test requires MSFS and the board on the simulator PC.
