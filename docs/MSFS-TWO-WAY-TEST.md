# MSFS 2020 barometer control test

This candidate adds **optional** board-to-simulator barometer control for the
standard primary altimeter (MSFS index 1). The previous read-only build passed
the reported ground/barometer/pause bench test. The two-way path subsequently
passed a live Just Flight Piper Warrior bench test: four requests were confirmed
by simulator readback, and the user reported working control. Full flight accuracy,
all failure paths and other aircraft remain unverified. Record the aircraft/variant;
custom aircraft may need a different control profile.

## Which PC does what

- Development PC: build/test sources; produce the host package and firmware.
- Simulator PC: connect the board's UART USB port, update firmware once, then
  run the bridge alongside MSFS. The board can remain on this PC.
- No Rust, Visual Studio or full SDK is needed on the simulator PC. Existing
  Python, pyserial and SimConnect runtime remain usable.

## Update the board once

1. Extract the entire candidate ZIP into a **new folder**, then open PowerShell
   in that folder. Confirm `manifest.json` says `barometer-two-way-v2`.
2. Stop the bridge and close PuTTY. Connect the Waveshare **SKU 29086,
   ESP32-S3-Touch-LCD-2.8C** through UART USB. Check its port in Device Manager;
   these commands use COM5. Do not use this image for other board models.
3. Install the flashing utility, if needed, and write the included verified image:

   ```powershell
   python -m pip install "esptool>=5,<6"
   python scripts/flash_test_firmware.py --port COM5
   ```

   The script verifies the image against the package manifest, then uses
   esptool's ESP32-S3 chip check and normal flash verification. Wait for successful
   completion and restart. This replaces the firmware; manual/demo settings are
   volatile. If the package has no `firmware` folder, obtain the firmware-inclusive
   package or update on the development PC with `scripts/Flash-Console.ps1`.

## First confirm read-only behavior

Load an unpaused flight in MSFS 2020. Run:

```powershell
python scripts/start_msfs.py --port COM5 --frame-timing
```

The gauge should mirror the cockpit and still lock touch adjustment. Full/menu
pause and Active Pause should hide numeric altitude and show PAUSED while the
connection is healthy. Resume requires fresh readings. Stop with Ctrl+C.

## Enable two-way barometer adjustment

```powershell
python scripts/start_msfs.py --port COM5 --writeback --frame-timing
```

Each launch creates a new `logs/<timestamp>-<random>/` folder containing console,
frame/control logs and a summary. No Tee-Object or manually reused filenames.
On interrupted/error exit, the summary describes the last observation, not a
post-stop measurement. Keep the existing read-only logs for comparison.

1. Confirm the board and cockpit QNH agree; record both readings and the aircraft.
2. Hold the Kollsman window for **0.25 seconds**, release, press + once, then Apply.
3. The board briefly shows **BARO PENDING**. Only the simulator's readback changes
   the active setting and altitude; there is no extra local altitude correction.
4. Confirm the cockpit setting changes by about 0.01 inHg and the board follows.
   MSFS's event uses integer hPa/16 increments, so internal values can differ
   slightly from the entered decimal; compare their displayed hundredths.
5. Repeat with −, then the 29.92 button and Apply. Cancel must send no write.
6. Change the cockpit setting directly and check the board follows without an
   echo/write loop. A cockpit setting change while editing cancels the draft.
7. Pause or stop the bridge during an edit. No pending request should replay on
   resume/restart; a new session disables controls until explicitly enabled.

BARO REJECTED or BARO TIMEOUT means the requested state was not confirmed under
the validity/deadline rules. Inspect the actual cockpit setting and preserve logs;
do not assume that timeout means the simulator never received the event. There
are no automatic write retries. Restart read-only if this aircraft ignores the
standard KOHLSMAN_SET event.

## Timing and remaining acceptance

Use rapid cockpit QNH changes, then a real climb/descent with changing pressure
altitude. The previous run's pressure altitude was constant, so it did not test
flight tracking. Repeat with and without diagnostics to measure their overhead.

The new `display sample` CLI response reports one coherent completed-frame record:
source session/sequence, receipt time, completion time, source age at
completion/current query, and displayed altitude. `received_owner_ms` uses the
console owner epoch (the same clock as source-status `now_ms`);
`now_display_ms` and `presented_display_ms` use the display/ESP timer epoch.
**Do not subtract timestamps from these different epochs.** Use the reported
relative source ages for cross-stage timing. These are renderer-completion
measurements, **not calibrated sensor-to-photon latency**.
Host diagnostics include query-end elapsed time to expose query skew.

Record `display stats reset` / `display stats stop` over a bounded run to measure
completed frames rather than cumulative maxima. Pause, flight/aircraft replacement,
reposition, simulator exit and cable loss must still invalidate old data. The
bridge still requires explicit restart after a flight/aircraft/session change.

## Development validation

```powershell
cargo test --workspace --features sim-bridge/msfs --locked
cargo clippy --workspace --all-targets --features sim-bridge/msfs --locked -- -D warnings
cargo build -p sim-bridge -p desktop-cli --features sim-bridge/msfs --locked
python -m unittest discover -s scripts -p test_sim_bridge.py -v
.\scripts\Build-SimBridge.ps1 -IncludeSdkRuntime -IncludeFirmware
```

The package includes source fingerprints, toolchain versions and a source snapshot
because this integration branch is not yet committed/released. The Microsoft
runtime remains subject to its own terms; it is not part of the project's license.

References: [MSFS instrumentation events](https://docs.flightsimulator.com/html/Programming_Tools/Event_IDs/Aircraft_Instrumentation_Events.htm),
[multi-parameter transmission](https://docs.flightsimulator.com/html/Programming_Tools/SimConnect/API_Reference/Events_And_Data/SimConnect_TransmitClientEvent_EX1.htm),
[Espressif flashing instructions](https://docs.espressif.com/projects/esptool/en/latest/esp32s3/esptool/basic-commands.html).
