# Aircraft-switch trace analysis, 2026-11-02

## Firmware installed and hardware regression passed, November 2 at 22:02 EST

User explicitly requested flashing and self-tests after reconnecting COM5.
`scripts/Flash-Console.ps1 -Port COM5` rebuilt and verified SDK safety settings,
verified the known CH343 USB identity and original factory backup hash, and
successfully flashed the board. ELF SHA256:
`7EEA53147339A326A77D47D2F4B721E8B26C96E1A67237E0238BDD56A23310A0`.
Application SHA256 `7C631A504152451A6717E913A86B237E46D4E9765357989426BDA90A1106C104`;
size 993,952 / 1,048,576 bytes. Source is the uncommitted recovery branch.

Hardware reports are retained in `output/debug/firmware-update-20261102/`:

- `external-timeout.json`: initial no-frame handshake becomes Stale with sequence
  zero and no fabricated value/age. Valid 8,500-ft telemetry recovers; duplicate
  sequence is rejected; received data later expires and recovers. A new session
  clears old data. Fresh Loading heartbeats remain Missing beyond the initial
  deadline, then transition to valid Running. No local demo fallback.
- `display-safety.json`: numeric/range/fault/expiry handling and independent
  render/submission-stall blanking/recovery pass. Host-observed blanking times
  407 ms and 313 ms are within this test's 600-ms polling budget.
- `imu-selftest.json`: QMI8658 built-in electrostatic self-test passed. Later
  diagnostics recorded zero IMU bus errors. This is not a calibration certificate.
- `attitude.json`: 30-second motion, 970 presentations / 29,986 ms = **32.35 FPS**;
  maximum draw 23 ms, transfer 32 ms. No blanking or expiry-counter increase in
  the timed motion window. Validity and fault recovery tests also pass.
- `altimeter.json`: 30-second climb plus ground hold, **32.32 FPS**, monotonic
  climb and correct 10,000-ft endpoint. Manual override, stop and fault checks pass.
- `integration-final.json` and `connector/`: packaged native connector with
  `--switch-demo` over real COM5, three sessions / three reconnect attempts
  (including one intentionally refused open), 180 frames. CRCs, sequences and
  ACKs verify; all three display probes show valid altimeter data. Exit is clean
  at synthetic source EOF. No data/demo fallback after disconnect. This does not
  verify the separate PowerShell Ctrl+C capture issue or live MSFS on new firmware.

Tests intentionally inject stale data and renderer stalls, so lifetime blanking
and expiry counters are nonzero. Do not describe them as unexpected motion failures.
Final board state: simulated altimeter 4,520 ft / 29.92, display enabled, owner
timing healthy, touch ready (250-ms hold, no bus errors), IMU off. Echo restored
and COM5 released. No new human visual acceptance was requested for this nonvisual
firmware change; results above are instrumented hardware checks.

## Live acceptance: two-aircraft test passed, 21:50–21:54

User reports physical test success with two planes. `recovery-capture.zip` SHA256:
`ed768122de911b84b27e63797a0f6a9dd771f1f4742dd5eb218c3fd705e1baed`.
Manifest and launch record match the 211401 package executable SHA256
`03343FB0AD4B32ACAA5B59D281636B1E65A94B8E55301B90CADB14E0BEA9B35E`.

- All **4,402 frames** pass CRC, consecutive per-session sequence and matching ACK
  checks. Four sessions (`1793674230036377` through `...6380`) include three
  automatic reconnects due to SessionChanged; all receive numeric readings.
  Session events do not establish aircraft identities or four different planes.
- First recovered session: 1,739 numeric frames over 58.857 s. A transitional
  session has one numeric frame before another change. Final recovered session:
  **2,153 numeric frames over 75.534 s**, around 31,000 ft. No SDK crash or failed
  open is recorded. Paused/loading frames contain no numeric readings.
- Three delayed probes show valid altimeter data, backlight on, and no recorded
  blanking/expiry. The transitional session ends before its probe is due.
  Display counter deltas imply **32.33 completed FPS averaged over 142.5 s**;
  this is not a minimum per-frame guarantee. Numeric transport averages about
  28.5–29.5 Hz in sustained segments, distinct from rendering speed.
- Writeback persists across sessions. Final-session edits to **29.95**, then
  **29.92 inHg**, both receive `sent` followed by `confirmed`. Telemetry reports
  29.951 and 29.919, consistent with existing simulator quantization/tolerance.

**Accepted:** aircraft-change recovery and post-recovery barometer writeback for
this two-aircraft test, supported by user observation. Aircraft identities were
not supplied; this is not an all-aircraft claim. The firmware-only handshake
timeout change was not installed for this test.

**Separate outstanding check:** clean shutdown. Transcript ends with `The pipeline
has been stopped`; exit code is null, no summary exists, and final telemetry is
Running. This establishes neither a native crash nor a clean stop, and does not
identify who stopped the pipeline. Verify Ctrl+C/capture separately. Evidence and
parsed metrics are saved under `output/debug/recovery-success-20261102/` (ignored).

## Current fix and verification (supersedes earlier next-step notes)

Priorities implemented on the recovery branch:

1. **Contain native access violations.** The connector's USB-owning process no
   longer calls SimConnect. Each SDK connection runs in a hidden child worker;
   worker loss invalidates telemetry and retries with a new worker/session.
   Drop kills/reaps a hung worker instead of waiting for SDK teardown. Queues and
   message sizes are bounded; polling never waits for a worker response. Request
   and heartbeat deadlines still apply. Pending barometer writes are discarded,
   never retried. Invalid SDK packets/protocol errors remain terminal.
2. **Expire abandoned handshakes.** Instrument core records session-start time.
   No first telemetry frame after 1,000 ms becomes Stale / LINK STALE, with no
   invented measurement or measurement age. Fresh telemetry can recover normally.
   This firmware change is built, not flashed; the host recovery test does not
   require it on the already-tested firmware.
3. **Select the correct face.** Session open/recovery explicitly selects the
   altimeter, so a board left showing attitude cannot hide valid altitude input.
4. **Preserve useful console evidence.** The PowerShell launcher converts stderr
   records to plain text while retaining stdout, stderr and exact exit status.
   Connection-opening and one delayed source/display probe per session remain.

Regression evidence:

- Before the core fix, `cargo test -p instrument-core abandoned_handshake` failed
  with LOADING instead of LINK STALE. It passes after the fix, including late
  session creation, exact timeout boundary, no-value/no-age and telemetry recovery.
- Before the API loss fix, `cargo test -p sim-bridge worker_connection_loss`
  failed with no recovery reason. It now distinguishes recoverable broken pipes/
  connection resets from fatal invalid data.
- `scripts/test_msfs_worker.py` compiles a temporary development-only DLL against
  the installed SDK headers. Its first connection raises a real unhandled Windows
  access violation after 19 numeric samples; its second crashes during open; its
  third supplies different altitude/QNH. The actual connector, native shim,
  child processes, supervisor, wire protocol and desktop receiver all participate.
  The previous 183132 package fails this test with **0xC0000005 and no summary**.
  The fixed connector survives, retries twice, uses two device sessions, accepts
  the new values and stops cleanly. A malformed-packet case proves no retry and
  no numeric publication on decoder failure. The fake DLL is never packaged.
- Worker unit tests cover abrupt exit, nonblocking polls, hung-worker cleanup,
  malformed/oversized messages and bit-exact nonfinite data for validation.
- Launcher fixture tested exit 0, 37 and -1073741819: exact codes and both output
  streams are retained, without NativeCommandError decorations.
- Workspace native-feature tests and targeted Clippy pass. Firmware build and
  generated SDK safety configuration verification pass.

Limits: the exact native faulting instruction/module from the son's PC is still
unknown. Isolation fixes crash propagation and DLL lifetime reuse; it is not a
claim to have repaired Microsoft SDK internals. The later two-aircraft acceptance
is recorded above; clean shutdown remains a separate unverified part of that
capture. Use `docs/MSFS-RECOVERY-TEST.md` for regression runs.

## Earlier evidence and investigation history

Branch: bugfix/msfs-aircraft-recovery, based on v0.2.0-rc.3.
Input: user-supplied aircraft-switch-logs.zip, SHA256 `cc4619f169264facac75891716f15f09877a92ad5712df71417e210ddfa427bf`.
Contains only session-1793590154447557/events-00.jsonl (1,578 records).
There is no summary, display/source probe, package identity or SDK event trace.
This session ID matches the earlier console transcript; the file is not evidence
of a new live test of the current attitude firmware.

## Evidence

| Event | Elapsed time from connector run |
|---|---:|
| Initial numeric telemetry | 0.063 s |
| First session paused | 39.832 s |
| First session disconnected / SessionChanged | 51.690 s |
| New session established | 52.852 s |
| Recovered numeric telemetry starts | 53.379 s |
| Last recovered numeric telemetry | 67.867 s |
| Recovered session paused, then loading | 67.897 / 68.090 s |
| Second SessionChanged / disconnected | 68.239 s |
| Last record: next-session recovery wait | 68.342 s |

The first session contains 1,173 acknowledged frames, including 1,147 numeric
frames. The recovered session contains 397 acknowledged frames, including 392
numeric frames over 14.488 seconds: indicated altitude 3.664-3.702 ft, QNH 29.921.
Incoming numeric data did resume, 1.689 seconds after the first disconnect.
A barometer request in the initial session received a confirmed response.

All 1,570 T1 frames were replayed with their captured receiver acknowledgement
timestamps through the current Frame decoder, App, external receiver and UI
altimeter model. Every one of 1,539 numeric frames produced valid indicated
altitude, assuming the altimeter face was selected. Maximum effective receiver
age was 297 ms, below the 1,000 ms altitude deadline. Missing lifecycle frames
remained missing. This replay does not execute old firmware, RGB hardware or
MSFS, and cannot prove what appeared on the physical screen.

Reproduction command (local throwaway harness retained under ignored output):

```powershell
cargo run --quiet --manifest-path output/debug/aircraft-switch-20261102/model-replay/Cargo.toml -- output/debug/aircraft-switch-20261102/wire.tsv
```

Result: PASS, 1,570 captured frames / 1,539 numeric / 2 sessions.
The reported physical red X has not been reproduced from this trace alone.
The file ending during recovery cannot establish why the final opening did not
complete, whether another log exists, or when Ctrl+C was pressed.

## Confirmed separate defect and fix

The altimeter-v1 host session did not select its display instrument. A current
board left in attitude/IMU mode accepts altitude/QNH but has no pitch/roll in T1;
it can therefore show an attitude missing-data X. The new regression failed with
`left: Attitude, right: Altimeter` before the fix. Session setup now selects and
verifies the altimeter on initial connection and recovery. It waits for fresh
values as before. This defect cannot be attributed to the older trace, which
predates the attitude feature.

## Diagnostic improvement

Provider-open begin/end records bracket SDK connection opening; a separate
record precedes recovery's device handshake. First numeric telemetry records a
telemetry_ready event and schedules one receiver/display probe after 250 ms,
while ordinary telemetry continues. There is no continuous diagnostic polling.
Each probe preserves source status, selected renderer, quality, displayed value
and sample provenance supplied by firmware. Diagnostics schema is logged as 2.
This adds evidence; it does not impose a timeout on a blocking SDK open.

Executable regression now checks all three recovered synthetic sessions have
numeric-ready and receiver-probe records and all four opening attempts have
begin/end records (one attempt is deliberately refused). It failed before the
logging changes and passes afterward. The profile-selection core regression
passes for initial open and recovery. Live MSFS and physical display acceptance
remain pending; the Waveshare UART device is not connected to the development PC.

Next test: docs/MSFS-RECOVERY-TEST.md, using the newly built package on the
simulator PC. Keep main unmerged until live recovery acceptance is established.

## Follow-up and package

User confirmed the red X was visible during the numeric-data interval and plans
to upload an image. Review that image before prescribing the next live test.

Prepared test package:
`artifacts/native-connector-20261102-181917-579.zip`
SHA256: `3FBE3A8EADB89E258ED7076F89DE9B50805D968887D82EF42272C5ADDDC18887`.
Contains the optional local Microsoft runtime, binaries, source snapshot,
manifest, notices and MSFS-RECOVERY-TEST.md. ZIP CRC and all 81 manifest hashes
verified. All six executable tests pass against the packaged release binaries;
workspace/all-target MSFS tests, Clippy -D warnings, formatting and diff checks
pass. No new firmware was flashed, and no commit/tag/push/merge was requested.

## Latest user evidence: spontaneous exit in a different run

User supplied console transcript for old package
round-instruments-native-auto-sync-20261101, session 1793661319997203.
Two enrollment attempts failed because device.json already exists, then run
successfully connected using that existing configuration. It reported heartbeat
timeout / ConnectionLost, recovered to 1793661319997204, then reported
SessionChanged and began recovery to 1793661319997205. PowerShell returned
without a visible shutdown summary or error. User explicitly confirmed it
returned by itself (not Ctrl+C). This differs from the earlier 1793590154447557
run; do not conflate them or infer a native crash without exit evidence.

Next evidence: $LASTEXITCODE on the simulator PC before another native program
runs, plus the complete logs/session-1793661319997203 directory. Request a new
ZIP for this session rather than recompressing the old session. Image is still
pending. A controlled failure vs abnormal process termination must be separated
before changing reconnect behavior. Enrollment should not be repeated; retain
and reuse device.json. New diagnostic package remains ready but untested live.

## Latest archive analyzed

User supplied aircraft-switch-latest-logs.zip, SHA256
`49f21bdd6928294bf76406a988d4e2f3ae089269cab6731bff5511185fefd9c2`.
Only session-1793661319997203/events-00.jsonl is present: 1,125 complete records,
no summary or exit reason. The PowerShell window was subsequently closed, so
the exit code cannot be recovered from that shell.

- Initial session: 470 acknowledged frames, 448 numeric; numeric interval
  0.046-15.034 seconds. A gauge barometer edit to 29.96 was confirmed.
- Paused from 15.067 s; ConnectionLost at 26.246 s (console said heartbeat timeout).
- New session established at 27.387 s, fresh numeric values at 30.322 s.
- Recovered session: 647 acknowledged frames, 639 numeric, last numeric 52.735 s.
  Indicated altitude near 2.6 ft; QNH 29.921. Numeric interval 22.413 seconds.
- Then paused (52.758), loading (52.901), disconnected/SessionChanged (53.001).
  Last record: recovery_wait at 53.105 s for session 1793661319997205.

The latest 1,117 frames also replay successfully through current core/UI:
1,087 numeric frames valid, maximum receiver age 273 ms. This is not a physical
screen reproduction. Both archives stop at the same observable recovery phase,
but neither can distinguish an opening-call wait/crash from external termination.
User says this latest process returned by itself; its cause is still unproven.

Added Run-MSFS-Recovery-Test.ps1 to the package. It runs the existing connector,
records a transcript, saves the executable hash and process exit code in decimal
and hex immediately after return, and preserves the code for the parent shell.
It cannot capture after PowerShell itself is forcibly terminated before return.
Windows PowerShell 5.1 tests with a native fixture passed exits 0, 37 and
-1073741819 (0xC0000005), preserving stdout and stderr. This is simulated status
capture testing, not a reproduced native crash. The initial transcript-only
attempt missed native output; piping it to Out-Host fixed that and tests passed.
Harness retained under ignored output/debug/recovery-launcher-test.

## manifest.zip: access violation confirmed (20:39 / 20:40 runs)

Input SHA256 55b58242338c8d38448d5106d486e28b42732e7e6fdbb4ebe96fa07b67c8d3ec.
Verified supplied manifest/executable hash matches the diagnostic package:
CD99D984C2923A1F3DF8586E7850233EBC5A0270F593332093AEACFAFE7ACEDB.
Extracted privately to output/debug/msfs-manifest-20261102.

First run 1793669964081944: saved exit -1073741819 / 0xC0000005, no launcher
error. This is a confirmed native access violation, not a normal reconnect exit.
Attempt2 provider_open_end succeeded (3 ms), then recovery_device_begin was
recorded, but no new start record followed. Thus the crash occurred after source
opening returned and before the recovered session completed; async SDK threads
can also crash the process during this interval. A stack/module is still needed.

Second run 1793670028781357: five sessions, final session1793670028781361 sent
4367 numeric frames from98.713s through244.683s (~146s). Four session probes
report renderer=altimeter, shown_quality=valid, backlight=true, no expiry or
blanking. One intermediate session ended46ms after its first numeric frame,
before the scheduled250ms probe. All shown probes have matching session IDs.
The second launcher exit code is null; no summary was saved. Its finally block
ran without recording a native exit code, so it does not prove another native
crash. User was asked whether the physical gauge recovered in this second run;
answer pending. The PowerShell NativeCommandError formatting in the transcript
is the rendering of native stderr, not evidence of a terminating script error.

Next evidence needed: Windows Application Error record for instrument-connector.exe,
especially faulting module/version/offset for the20:39 crash. May already exist;
no need to reproduce again just to retrieve it. Added read-only collector
scripts/Collect-Connector-Crash.ps1 (PS5.1 execution tested; no local matching
crashes). It filters this executable's Application events1000/1001/1002 from
last2days, saves bounded JSON, and reports missing/query errors explicitly.
No native lifetime change has been made without a crash stack/module. Potential
DLL unload/reopen lifetime issue remains a hypothesis, not a confirmed cause.
No new firmware or package installed; uncommitted work remains on recovery branch.

## User clarification: second run still showed physical red X

User explicitly answered that the gauge still showed a red X during the second
run despite recovered sessions and valid firmware display probes. Do not mark
physical recovery accepted or dismiss the observation based on metadata.
Two separate unresolved findings now exist: native access violation on first
run, and disagreement between display presentation metadata and observed screen
on second run. Need the promised photo with the complete red-X fault text and,
if available, approximate observation time/session to correlate with lifecycle.
Windows Application Error module/offset is separately pending for the crash.
Do not weaken source validity checks or change rendering based on this mismatch
without reproducing/correlating it. Main remains unmerged.

## Photo received: ALT DATA MISSING / LOADING

Photo clearly shows the altimeter face with ALT DATA, MISSING, LOADING and
missing barometer digits. It is not ATT DATA or LINK STALE. There is no obvious
render corruption in this still image; this does not prove scanout is updating.

Reproduced the exact logical status using current App + Snapshot: establish a
new external Msfs2020 session, send no telemetry, advance normal owner ticks
for10s -> source_label=LOADING, model quality=Missing, altitude=None,
source_age=None, sequence=0. Harness:
cargo run --quiet --manifest-path output/debug/aircraft-switch-20261102/model-replay/Cargo.toml --bin handshake_only
This reproduces the status, not the native access violation.

A crash after source-external acknowledgement but before first telemetry is
consistent with both the photo and first run's recovery_device_begin/crash.
No per-command ACK evidence or photo timestamp proves this exact ordering.
The same label can appear while fresh explicit Loading frames arrive. Earlier
valid presentation probes may refer to other moments/sessions; do not dismiss
user observation or conclude firmware metadata is corrupt solely from the photo.
Current initial no-sample state has no age/deadline, so it can remain LOADING
indefinitely if the host dies during handshake. Keep readings missing; do not
extend freshness or hide the fault as a recovery fix. Root native crash module/
offset remains pending from Windows Application events. No firmware changed.
