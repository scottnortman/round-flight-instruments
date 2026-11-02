# Native MSFS 2020 adapter — local verification, 2026-11-01

Checkpoint: `v0.2.0-rc.1`. MSFS is not installed on the development PC; live
tests ran on the simulator PC. The dated sections below preserve the progression
from offline verification to live read-only and two-way testing. Earlier pending
statements are historical; the latest acceptance section records current evidence.

## Build and tests

- MSFS 2020 Core SDK **0.24.5**, installed at `C:\MSFS SDK`. Installer signature
  verified as Microsoft Corporation; elevated installation completed successfully.
- Native adapter compiled against the official installed `SimConnect.h`; loaded
  the installed SDK DLL successfully. `SimConnect_Open` returns expected
  `0x80004005` when no simulator is available. This occurs before board COM access.
- **94 Rust tests** pass with `cargo test --workspace --features sim-bridge/msfs
  --locked`; native-enabled workspace/all-target Clippy passes with warnings denied.
  Portable/default-feature tests also ran successfully before the native build.
- Native packet fixtures check the official SDK layout, copied numeric values,
  truncated/oversized packets, incorrect field count, resolved object-ID propagation, and event
  decoding. Rust provider tests cover coherent values, acquisition-age bounds,
  failure enums, unknown/out-of-range input, pause/Active Pause, cancelled replies,
  same-batch load events, request expiry, heartbeat loss, clock regression and
  event-queue limits. These fixtures are not live simulator validation.
- Three Python protocol tests pass. Extracted-package smoke tests independently
  verify synthetic and replay runs, all ten packaged file hashes, and the actual
  DLL/no-simulator path with a deliberately nonexistent COM port: native connection
  fails first, without attempting to open that port.

## Test package

Built with `scripts/Build-SimBridge.ps1 -IncludeSdkRuntime`:

`artifacts/msfs2020-test-20261101-160134-666.zip`

SHA-256:
`D1328AF6CD81A2D4CD91A1C73C79DF07C0EADEE21C9C26D83C90B432E01BC7CB`

This is a local Windows x64 test bundle, including the user's installed Microsoft
SimConnect runtime. Build outputs and the SDK runtime are outside tracked source.
The package manifest identifies the base commit, modified working tree, SDK
version and per-file SHA-256 hashes. Python/pyserial and the Microsoft x64 VC++
runtime are prerequisites on the simulator PC; Rust and a full SDK are not.

Local evidence: `output/msfs-native-tests.txt`, `output/msfs-no-simulator.txt`,
`output/msfs-package-smoke.json`, and the clean extracted smoke-test directory.

The board firmware was not changed for this adapter. It already accepts the
MSFS provider's T1 frames. Prior synthetic hardware evidence is recorded in
[SIMULATOR-LINK.md](SIMULATOR-LINK.md); it is not a measurement of live MSFS.

## First live failure and local correction

The simulator-PC report confirmed the synthetic USB path (27.48 accepted
samples/s) worked, but native MSFS ended before sending numeric values with
`0x8000FFFF`. This is not a measurement of rendering FPS.

The tested executable SHA-256 matches the original local package:
`732e577498c7622880a4069414070bfebd57eb76a59f42c786d37fc123b6505a`.
Direct SDK probes in the supplied evidence received valid values using the same
DLL. Their USER request used selector 0; the returned resolved object was 1.
Our decoder erroneously required the reply object to equal selector 0 and itself
returned E_UNEXPECTED. The earlier synthetic native fixture incorrectly treated
object 1 as invalid; it did not represent the real SDK response.

An unmodified 72-byte captured four-field reply now exercises the actual decoder
with the capture's definition ID 9000. The regression failed before the fix with
exactly 0x8000FFFF and passes after it. Production still requires definition 1.
The provider binds an object only after request, lifecycle, age and value checks;
it ignores cancelled/unmatched replies and terminates on a later object change.
No assumption that the resolved object must be 1 remains. Native rejection now
reports bounded packet metadata separately from raw API HRESULT failures.

Additional tests preserve count/flags/bounds/entry checks, accept the captured
UNKNOWN_GROUP event, reject stale/invalid binding and require a new session for
another identity. Fixture provenance is in
[the fixture README](../crates/simconnect-native/tests/fixtures/README.md).
The attached scripts were inspected as evidence, not executed.

Validation: 94 Rust tests, native-enabled Clippy with warnings denied, three
Python protocol tests. A fresh extraction verifies all ten manifest file hashes,
synthetic and replay desktop runs, and actual SDK DLL loading with the expected
no-simulator error before opening a deliberately nonexistent COM port.
Evidence: `output/msfs-object-fix-tests.txt` and
`output/msfs-object-fix-package-smoke.json`.

Replacement package revision: `resolved-user-object-v2`, also copied to
`D:\Downloads\round-instruments-msfs2020-object-fix-20261101.zip`.
No firmware, serial protocol, renderer or simulator write API changed.
**Retesting on the simulator PC is still required**; offline captured-packet
success does not establish live cockpit agreement or streaming performance.

## Remaining live acceptance

Confirm stock primary-altimeter mapping, nonstandard-QNH agreement, failure
indications, pause/resume, aircraft/flight transitions, connection loss, touch
read-only ownership, source-to-display delay and completed-frame FPS. The adapter
terminates sessions after load/reposition events rather than reusing prior state;
restart it after loading the desired flight. No simulator write API is exposed.
DCS and two-way barometer control remain future work.

## Primary specifications used

- [MSFS 2020 SimVars: indicated/pressure altitude, indexed QNH, altimeter failure](https://docs.flightsimulator.com/html/Programming_Tools/SimVars/Aircraft_SimVars/Aircraft_System_Variables.htm)
- [System lifecycle events, including Pause_EX1](https://docs.flightsimulator.com/html/Programming_Tools/SimConnect/API_Reference/Events_And_Data/SimConnect_SubscribeToSystemEvent.htm)
- [RequestDataOnSimObject](https://docs.flightsimulator.com/html/Programming_Tools/SimConnect/API_Reference/Events_And_Data/SimConnect_RequestDataOnSimObject.htm)
- [GetNextDispatch](https://docs.flightsimulator.com/html/Programming_Tools/SimConnect/API_Reference/General/SimConnect_GetNextDispatch.htm)

## Live read-only acceptance and next candidate

Results archive `msfs-object-fix-results-20261101-170806-897.zip` SHA256
`e4769d6bf1e683392114a632299c2c8189fc1a6c1d416388cf0b01b8b7c28789`
was independently checked: 28,534 frames in 1064.027 s, correct CRC/sequence/ACKs,
985 diagnostics with no display failures, 946 running valid samples and 39 paused
numeric-hidden samples. Manifest hashes match the delivered object-fix package.
Frame-counter increments averaged 31.71/s. Pressure altitude remained 12.069 ft;
this was QNH/ground testing, not climb/descent or precise accuracy acceptance.

The new barometer-two-way-v1 candidate preserves that startup fix and adds:
optional C1/R1 writes with confirmation, state-only paused heartbeats, dynamic
QNH rendering, coherent completed-frame provenance and unique capture launching.
The new rendering regression failed on the old barometer-dependent cache key,
then passed after moving the value into damage-tracked overlays. Existing
multi-buffer pixel comparisons also pass. Source/render latency on hardware is
not yet measured; the earlier apparent 424 ms lag and two host-wire gaps need
a new bounded run, with and without diagnostics.

Candidate validation: 102 native-enabled Rust tests; Clippy warnings denied;
six Python tests, including real desktop CLI control/readback, interruption
summary and evidence preservation. Firmware compiles with generated SDK safety
checks. This does not establish live two-way SDK behavior or physical touch
acceptance. See MSFS-TWO-WAY-TEST.md for the outstanding checks.

## Development-board checks after reconnecting UART

The verified CH343 board returned on COM5, hardware identity
USB VID_1A86/PID_55D3, serial 58FA088072. The firmware-inclusive candidate was
flashed successfully with the existing backup/identity/build checks. Installed
ELF SHA256: F67FDF3BBB7B4C565AD424CE3BCA226AF76E1BDC5030077F7F37CC21380897B2.
The SDK safety settings and 16 MB flash configuration were verified.

The actual board passed the local C1/R1 protocol test: read-only rejection,
pending adjustment without local value substitution, rejection before matching
readback, and confirmation after fresh matching telemetry. This test uses an
emulated host readback; it is not a SimConnect or live aircraft test.

A 25-second rapid-QNH/altitude bench run sent 648 frames (25.89 accepted/s) and
recorded 795 completed presentations over 25,060 ms: **31.72 rendered FPS**.
Reset-window maximum render time was 31 ms; transfer maximum was 50 ms; no display
failure was reported. Among once-per-second diagnostic samples, conservative
source age at the completion point had median 136 ms and maximum 175 ms. These
are board/transport measurements, not independently measured photon latency.
They do not explain the prior two MSFS-side wire gaps or establish a hard minimum
FPS. Evidence: output/two-way-board-baro-bench.json and
output/two-way-board-acceptance.json.

Aircraft clarified by the user: Just Flight Piper Warrior. Its actual cockpit
writeback still requires live acceptance on the simulator PC.

The follow-up `barometer-two-way-v2` changes diagnostic timestamp labels to
identify their epochs: received_owner_ms versus now_display_ms/presented_display_ms.
Relative source-age calculations are unchanged. The console owner and display
timer start at different times; never subtract their absolute timestamps.
V2 was built, flashed, and passed the physical-board protocol test again.
Installed ELF SHA256 E9D213AEB3B4E8E22BE1551B85E5624872B352245E0D3049DC5949CDAE06F723.

The local touch emulator was run, but no physical touch gestures were received;
visual/touch acceptance is still pending. A first harness run stopped on an
expected ERR display_sample_busy diagnostic reply; the harness was adjusted to
tolerate it, matching the production relay's existing handling. The second run
ended normally and restored the manual 4520ft/29.92 demonstration. No SimConnect
connection or live aircraft write was attempted on this development PC.

Firmware-inclusive replacement: round-instruments-msfs2020-two-way-v2-20261101.zip
SHA256 FD3C621829D5404CD6D9AE7C92AB455A28580F5951E390D8EA71C139AC286E96.
The previous non-v2 ZIP is superseded. All 17 replacement package entries were
checked against the manifest.


## Live two-way acceptance and landing checkpoint - 2026-11-01

User supplied `logs.zip`, SHA256
`29283528d6d9fe4dce1f708945ad6ac83f639da1f482d3b88a43c2b83ced1a21`,
and reported that two-way control worked on the Just Flight Piper Warrior.
The exact aircraft package version was not recorded. Archive integrity passed.

- Initial read-only run: 512 frames in 18.915 s; no control requests.
- Two-way run: 7,012 frames in 277.229 s, 25.29 accepted samples/s. All frame
  CRCs, contiguous sequence numbers and session/sequence ACK correlations passed
  in both runs. Telemetry throughput is not display FPS.
- Four requests (30.00, 29.92, 30.16, 29.92 inHg) each progressed from `sent`
  to `confirmed`, demonstrating correlated simulator readback. This agrees with
  the user's report; no independent cockpit video or precision comparison exists.
- Two-way lifecycle frames: 6,973 running, 38 paused, one disconnected. The
  maximum sample gap of 567.49 ms spans a run containing paused heartbeats;
  it is not evidence of a 567 ms gap in valid running measurements.
- After aircraft changes, the adapter intentionally ended with
  `aircraft/flight/position changed`, sent Disconnected and returned to PowerShell.
  A bridge restart is currently required. Healthy display diagnostics in the
  supplied excerpt show missing data rather than a stalled renderer.

This establishes a limited live two-way bench pass, not full flight accuracy,
all cancellation/failure paths, F/A-18 compatibility or automatic recovery.
The installed v2 firmware and delivered package remain unchanged by this landing.

Landing checks rerun: 102 native-enabled Rust tests, six Python integration tests,
all-target native-enabled Clippy with warnings denied, and Rust formatting pass.
No fresh hardware test or reflash was needed for documentation-only landing edits.
