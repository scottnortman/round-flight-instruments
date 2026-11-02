# First MSFS 2020 hardware test

**New two-way candidate:** use [TWO-WAY-TEST.md](TWO-WAY-TEST.md) in the test
package, or [the two-way guide](MSFS-TWO-WAY-TEST.md) in this repository. It requires
updated firmware. The instructions below describe the earlier read-only test.

This is a **read-only development build**, prepared for the first live test.
It has passed local tests; cockpit agreement has not yet been verified.
It mirrors the simulator's primary altimeter. Custom aircraft may need a
separate profile. Hardware Kollsman edits stay disabled while MSFS owns the gauge.

## Retest after the first connection failure

The `resolved-user-object-v2` package fixes a host decoder bug: MSFS returned
resolved aircraft object ID 1 for our USER (0) request, and the old bridge rejected
it with `0x8000FFFF`. The new bridge correlates the request before binding the
resolved aircraft ID. Freshness and read-only checks remain enabled.

On the simulator PC:

1. Stop the old bridge and close serial terminals. Extract this **whole ZIP into
   a new folder**, and open PowerShell there. Do not mix old and new executables.
   The package manifest should say `resolved-user-object-v2`.
2. Keep the existing firmware, Python, pyserial and SimConnect runtime setup.
   **No board reflash or SDK reinstall is needed.** Connect UART USB (COM5 on the
   previous test) and load an unpaused flight in MSFS 2020.
3. Run this command block. Each attempt gets new filenames:

   ```powershell
   $run = Get-Date -Format 'yyyyMMdd-HHmmss'
   New-Item -ItemType Directory -Force logs | Out-Null
   python scripts/sim_bridge.py --msfs --port COM5 --diagnostics --log "logs/$run-frames.jsonl" --report "logs/$run-summary.json" 2>&1 | Tee-Object -FilePath "logs/$run-console.txt"
   ```

4. Expect `MSFS 2020`, valid altitude and QNH matching the cockpit. Test a cockpit
   barometer change, climb/descent and pause/resume using the acceptance table
   below. Stop with Ctrl+C. Save the console and frame logs even if no summary
   appears. Record the aircraft/variant and cockpit versus board readings.

Offline replay of the original captured packet now passes. This is still a
**live-test candidate**, not a claim that cockpit accuracy has been verified.

## On the development PC

The SDK and tools are installed here. Build/test code and flash firmware here.
The Waveshare currently has the required integration-branch firmware.

The prepared test package includes our two Windows x64 executables, Python
relay/client, example replay and these instructions. A local bundle built with
`-IncludeSdkRuntime` also has `SimConnect.dll` from this PC's MSFS 2020 SDK.
If absent, copy `C:\MSFS SDK\SimConnect SDK\lib\SimConnect.dll` into the extracted
package's top folder. It must be the 2020 SDK DLL.

Copy the **whole extracted folder** or ZIP to your son's PC. Move the board and
its USB cable there when ready. No MSFS installation is needed on the dev PC.

For developers rebuilding this package:

```powershell
cargo test --workspace --features sim-bridge/msfs --locked
.\scripts\Build-SimBridge.ps1 -IncludeSdkRuntime
```

## On your son's PC: one-time setup

1. Extract the ZIP to a writable folder, for example
   `C:\Users\YOURNAME\Downloads\round-instruments-test`. Do not run inside the ZIP.
2. Use Python 3.11 or newer. The existing Python installation on the simulator PC
   is sufficient; no reinstall is needed.
   Open PowerShell in the extracted folder and run:

   ```powershell
   python --version
   python -m pip install -r requirements.txt
   ```

   If `python` is unavailable but the Windows launcher is installed, substitute
   `py -3` for `python` in these commands.

3. The native bridge and SimConnect require Microsoft's x64 Visual C++ runtime.
   If a VCRUNTIME/MSVCP missing-DLL message appears, install the
   [Microsoft x64 v14 Redistributable](https://aka.ms/vc14/vc_redist.x64.exe).
   [Microsoft runtime documentation](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist).
4. Connect the Waveshare **UART USB** port. In Device Manager → Ports (COM & LPT),
   find its CH343 COM number. Install the board's CH343 driver if Windows does not
   expose a COM port. Substitute that number for `COM5` below.
5. Close PuTTY and any other serial terminal. One program owns the port at a time.

No Rust, Git, Visual Studio or full SDK installation is needed on this PC when
the package includes `SimConnect.dll`. The bridge runs on the same PC as MSFS;
no connection to the development PC or remote SimConnect configuration is used.

## Check the cable and board before MSFS

In PowerShell, from the extracted package folder:

```powershell
python scripts/sim_bridge.py --synthetic 60 --port COM5 --report logs/usb-test.json
```

Watch the board during the run: `TEST INPUT`, QNH 30.12, altitude climbing from
0 to 10,000 ft. At the end, missing/stale indication is intentional. This confirms
USB and firmware, not an MSFS connection. Use `--synthetic 300` for a longer look.

## Connect to MSFS 2020

1. Start **MSFS 2020** and load a flight in a stock aircraft with an easy-to-read
   altimeter. Begin on the ground in stable weather. Have the cockpit visible.
2. From the same PowerShell window, run:

   ```powershell
   python scripts/sim_bridge.py --msfs --port COM5 --diagnostics --log logs/first-flight.jsonl --report logs/first-flight-summary.json
   ```

3. The board should show **MSFS 2020**, the cockpit's primary indicated altitude,
   and its barometer setting. Terminal diagnostics should show `state=running`,
   `altitude_quality=valid`, and `shown_quality=valid`. The popup should not open
   when holding the Kollsman window in this read-only mode.
4. Stop with **Ctrl+C**. The gauge will expire its last sample; that is intentional.
   A summary is written on a normal Disconnected end; the JSONL log remains
   available even when interrupted. Use a new log filename for each run—the tool
   refuses to overwrite an existing capture.

A `SimConnect setup stage 3 ... 0x80004005` error means the local simulator
connection did not open: ensure MSFS is running and a flight is loaded, then
retry. This error occurs before the relay opens the board's COM port.
`ERR syntax` on source selection usually means older firmware; bring the board
back to the development PC for the integration build. `ERR not_ready` during
streaming means a timing/freshness check failed; preserve the log and restart the
bridge after checking PC load and the connection. Do not disable those checks.

## First acceptance run

| Check | Expected result |
| --- | --- |
| Stationary at QNH 29.92 | Board and cockpit primary indication agree within their displayed resolution; record any difference |
| Change cockpit QNH to 30.12, then 29.92 | Board follows QNH and indicated altitude; no second correction |
| Climb and descend | Correct direction and tracking, no jumps or graphics artifacts |
| Full pause and Active Pause | Numeric altitude invalidated; no old reading presented as live; fresh readings resume after unpause |
| Load another aircraft/flight or reposition via menu | Session ends; load the desired flight, then restart the bridge with a new log filename |
| Exit MSFS | Source disconnects/expires; no automatic demo fallback |
| Stop bridge or unplug UART | Old data expires instead of freezing as valid |
| Hold hardware Kollsman window | Editing remains disabled |

Record aircraft/variant, MSFS version, QNH and cockpit/board altitude at each
comparison. A custom avionics failure model may not use the standard SimVars;
do not treat successful data reception as proof of cockpit agreement.

To measure display performance, stop the bridge, use `python scripts/cli.py --port
COM5` and issue `display stats reset`, then `exit`. Run the MSFS bridge without
`--diagnostics` for the workload, stop it, reconnect the CLI and issue
`display stats stop`. Record the measurement interval, including any idle time.
Rendering FPS and incoming measurement rate are separate: the earlier synthetic
workload rendered about 32 FPS with about 20 new samples/s. No interpolation is
used to invent extra source samples.

## Return to standalone demonstration

After closing the bridge:

```powershell
python scripts/cli.py --port COM5
```

At the `rfi>` prompt:

```text
source sim
sim preset level
instrument altimeter
baro 29.92
exit
```

This explicitly returns ownership to the board's manual/demo source and restores
touch barometer editing. Settings are volatile. Leave the board connected to your
son's PC while testing; move it back only when a firmware update is needed.
