# Native Windows connector: automatic simulator synchronization

This development build replaces the Python relay with a native Rust executable.
It uses the existing MSFS SimConnect adapter, altimeter firmware and confirmed
barometer protocol. **Python is not needed on the simulator PC.** This is still
a command-line application. Aircraft/flight changes trigger automatic reconnect attempts
with a fresh session. SimConnect runs in a hidden worker process; a native crash
triggers automatic replacement while the connector retains the USB connection.
Injected native-crash recovery and a two-aircraft live recovery/writeback test
pass. **MSFS is the source of truth: its altitude and pressure
setting automatically replace the physical gauge's values after every switch.**
No `--writeback` flag, restart or confirmation is needed for this default behavior.
Tray UI and automatic startup remain future work.

Earlier native builds passed UART synthetic-climb tests on the Waveshare. This
worker-isolation build passes offline native-crash and desktop tests, plus live
two-aircraft recovery and post-recovery barometer edits. Clean shutdown was not
captured in that live run and remains a separate check. For the current
build, follow [MSFS-RECOVERY-TEST.md](MSFS-RECOVERY-TEST.md). Keep the working
`barometer-two-way-v2` package available for comparison. No reflash is needed for
a board already running that firmware. Only Waveshare SKU 29086 is supported. The `altimeter-v1` connector explicitly
selects the altimeter face at startup and recovery, including when the board was
previously left in attitude/IMU mode. It does not yet transmit attitude data.

## On the development PC

```powershell
$env:MSFS_SDK = 'C:\MSFS SDK'
cargo build -p instrument-connector -p desktop-cli --features instrument-connector/msfs --locked
.\target\debug\instrument-connector.exe run --desktop target/debug/desktop-cli.exe --synthetic 5 --diagnostics
.\target\debug\instrument-connector.exe run --desktop target/debug/desktop-cli.exe --replay fixtures/altimeter-lifecycle.txt
```

Build a local test ZIP with the installed SDK runtime:

```powershell
.\scripts\Build-NativeConnector.ps1 -IncludeSdkRuntime
```

The script writes a unique package under `artifacts/`, including executables,
instructions, source snapshot, dependency notices/source archives and hashes.
The optional Microsoft DLL is for this local test distribution and retains its
own terms; the project license does not relicense it. No firmware is flashed or
included. Native binaries are built for Windows x64; use a current Microsoft
x64 Visual C++ runtime if Windows reports a missing runtime DLL.

## On the simulator PC

1. Extract the whole native connector ZIP to a new folder. Close the Python bridge
   and PuTTY. Connect the board's **UART USB** port. Only one application can own it.
2. Open PowerShell in the extracted folder and list candidates:

   ```powershell
   .\instrument-connector.exe devices
   ```

   This lists CH343 USB identities without opening ports. Verify the correct
   Waveshare board in Device Manager. Other CH343 products can share its VID/PID;
   this list alone is not proof of the board model.
3. Enroll that board once, substituting its current port:

   ```powershell
   .\instrument-connector.exe enroll --port COM5 --config device.json
   ```

   Enrollment saves its USB serial identity, not COM5. Later COM-number changes
   are resolved automatically at launch. It refuses missing or ambiguous identities
   and never overwrites an existing configuration. For a replacement board, use
   a new configuration filename. Enrollment sends no device commands.
4. Load an unpaused flight in MSFS 2020. Start automatic MSFS-to-gauge synchronization:

   ```powershell
   .\instrument-connector.exe run --config device.json --msfs --dll .\SimConnect.dll
   ```

   If the test ZIP does not contain the DLL, supply the full path to the same
   MSFS 2020 SimConnect.dll used by the working bridge. The full SDK is not needed
   on the simulator PC. A simulator connection failure occurs before COM access.
5. Confirm altitude and pressure agree with the cockpit. Touch barometer editing
   should be disabled. Press **Ctrl+C** and wait for the summary/PowerShell prompt.
6. Optional: if you also want gauge touch edits sent back to MSFS, add `--writeback`
   when launching. This setting is preserved automatically across aircraft changes:

   ```powershell
   .\instrument-connector.exe run --config device.json --msfs --dll .\SimConnect.dll --writeback
   ```

   Hold the Kollsman window for 0.25 s, release, increase by 0.01 and Apply. Check
   the cockpit setting and board readback, then try 29.92 and Cancel. The setting
   changes only through simulator readback; a pending request is not applied locally.
7. Pause/resume, then stop with Ctrl+C. Verify paused/stale readings are hidden
   and the device does not fall back to simulated data. Clean stop sends
   Disconnected; an abrupt failure relies on the firmware's freshness timeout.

## Changing aircraft without restarting the connector

Leave the connector running while you change aircraft, flight, or position in MSFS.
It sends Disconnected, discards pending controls, closes the old SimConnect
connection and establishes a **new session**. It keeps the USB port open, so no
second serial-open/reset sequence is requested. Load and unpause the new flight;
numeric readings resume only from fresh, validated simulator responses.

Each connection uses a new worker process. The main connector never loads the
SDK DLL. A worker crash or liveness timeout invalidates the session and starts
recovery; a blocked SDK shutdown cannot block the parent's reconnect loop.
Worker messages and queues are bounded, polling is nonblocking, and uncertain
barometer writes are never replayed. Protocol/packet errors remain fatal.

The screen may show disconnected, stale or missing data during loading. That is
expected; the old aircraft's readings are not held as valid. If the simulator is
temporarily unavailable, reconnection attempts wait 1, 2, 4, then at most 8 seconds
between attempts. Known simulator-liveness timeouts and simulator quit/restart
also use this recovery path after the first successful connection. Ctrl+C stops
the wait, and `--duration` applies across all sessions and reconnect waits.

**Recovery is automatic and preserves the selected communication mode.** In default
mode, each new aircraft's altitude and QNH overwrite the physical gauge without
any command or flag. If two-way operation was enabled at launch, it remains
enabled after recovery; no stop/restart or reauthorization is required.

`--writeback` means **gauge-to-MSFS edits only**. It is never required to receive
MSFS data or update the display. Even in two-way mode, MSFS readback determines
the displayed values. Recovery does not send the old gauge setting to the new
aircraft: old pending writes/confirmations are discarded, edits remain blocked
until fresh readings arrive, and only a new user edit can request a simulator change.

Malformed SDK data, schema/protocol errors, a busy/missing/unplugged USB device,
and serial transaction faults stop the connector instead of triggering blind
retries. Correct the cause and restart. Initial connection still requires MSFS
to be running; automatic recovery applies after a connection has been established.

To test the new build on the simulator PC: start on the Warrior, switch to the
F/A-18, then a third aircraft while leaving the connector open. Verify that each
new cockpit's altitude/QNH returns on the board after loading/unpausing, that the
console reports a new session without requiring interaction, the new aircraft's
pressure setting replaces the old gauge value, and no old barometer edit applies.
Retain the entire run's log directory. Other aircraft's standard altimeter
mapping still requires comparison against their cockpit instruments.

An offline recovery fixture is available:

```powershell
.\instrument-connector.exe run --desktop .\desktop-cli.exe --switch-demo
```

Use `--config device.json` instead of `--desktop .\desktop-cli.exe` to run it on
the board. It is explicitly synthetic/TEST INPUT: three short altitude streams
with QNH 29.92, 30.01 and 29.80 inHg,
two session changes and one simulated connection refusal. It tests host/device
recovery, not actual MSFS aircraft events. No simulator writes are enabled.

## Logs and useful options

- Each launch creates `logs/session-<id>/events-*.jsonl` and writes `summary.json`
  on an ordinary return. Abrupt termination can leave no summary. The supplied
  `Run-MSFS-Recovery-Test.ps1` saves console output and process exit status under
  `test-results/` when the child process returns; see the recovery test guide.
  Use `--log-dir PATH` to choose a different root. Existing sessions are not reused.
- One run's directory contains all its recovered sessions. `start`, `session_end`,
  `recovery_wait` and `reconnect_failed` records mark transitions. Frame sequence
  numbers restart at 1 only with a strictly newer session ID. Summaries count
  sessions/attempts and total frames. Maximum sample gap is measured within
  sessions; intentional reconnect waits are logged separately.
- `provider_open_begin` / `provider_open_end` bracket each source connection
  attempt (their clock is labeled `execute_elapsed_ms`); a begin without an end
  localizes a wait inside the source-opening call. `recovery_device_begin`
  precedes the device session handshake. These do not add an SDK timeout.
- `telemetry_ready` records the first numeric altitude/QNH pair per session.
  About 250 ms later, `session_probe` captures source status and, on hardware,
  display status/sample once. Telemetry continues during that scheduling delay;
  the short queries themselves are sequential. This records what was actually
  selected/presented without continuous extra serial polling. A session that ends
  before its probe deadline has no probe. A busy display sample is logged as busy.
- `run_info` identifies diagnostics schema 2; retain the package manifest for the
  exact binary identity. Wire logs contain frames/ACKs and control requests/results.
  Per-session logs rotate through eight 4 MiB files; old parts in that session are
  replaced. For long runs, sort records by session/sequence/time, not filename.
  Previous session directories are preserved; remove old runs manually when needed.
- A bounded asynchronous queue separates disk writes from telemetry. Disk failure
  or queue overflow stops the session visibly. The summary is saved separately.
- `--duration 60` stops cleanly after one minute. `--diagnostics` adds source/display
  queries once per second. `--frame-timing` adds coherent display provenance and
  requires the v2 firmware. Use these only for a short diagnostic capture: the
  extra serial queries caused small visible hand hitches in the hardware test.
  Normal operation leaves both off; wire/control logs and the one-time session
  probe are still recorded.
- Serial is 921600 baud, 8N1, no flow control. DTR/RTS are held low; no reset pulse
  is requested. Echo is negotiated and restored on a healthy exit. After an
  uncertain transaction, cleanup sends no commands; `echo on` can be entered
  manually after reopening a terminal if needed.

For a bounded diagnostic capture, append
`--diagnostics --frame-timing --duration 60` to the MSFS command. It stops the
connection after one minute. The diagnostic run averaged 31.59 displayed FPS,
but its extra serial traffic reduced telemetry to 23.09 Hz and produced 53 gaps
over 80 ms in 60 seconds. Every such gap coincided with diagnostic queries.
The user confirmed smoother hands with diagnostics disabled. This is not a
guarantee of a minimum frame rate or measurement-update rate.

The normal-mode follow-up accepted 1,535 frames in 60.06 s (25.56 Hz), with a
maximum recorded frame gap of 69 ms, compared with 175 ms in diagnostic mode.
All frame CRCs, sequences and ACK correlations passed in both runs.

The summary's `accepted_hz` is telemetry throughput, **not rendered FPS**.
Use firmware frame counters and the separate source-age diagnostics to assess
rendering and latency. Other hardware gauges and DCS remain unsupported by this
executable; their shared quantity and provider boundaries are retained.

## Development verification

```powershell
cargo test --workspace --all-targets --features instrument-connector/msfs --locked
cargo clippy --workspace --all-targets --features instrument-connector/msfs --locked -- -D warnings
python -m unittest discover -s scripts -p test_native_connector.py
```

The Python command is a developer acceptance harness only. It checks the actual
native executable against the desktop console, including CRC/ACK correlation,
pause/missing/failed lifecycle replay and clean duration stop. The Rust runtime
tests use the actual instrument core for readback/control validation.

Implementation references: [SimConnect application guidance](https://docs.flightsimulator.com/html/Programming_Tools/SimConnect/SimConnect_SDK.htm),
[serial configuration](https://docs.rs/serialport/4.10.1/serialport/struct.SerialPortBuilder.html),
[Ctrl+C handling](https://docs.rs/ctrlc/3.5.2/ctrlc/).
