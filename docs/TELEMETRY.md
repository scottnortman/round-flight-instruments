# External telemetry — development branch

This branch includes the offline foundation and a read-only native MSFS 2020
adapter awaiting live acceptance. DCS remains planned. It is not in the v0.1.1 download. The
provider contract covers future gauges; this first board wire profile carries
only the altimeter. [Architecture and measurement conventions](MSFS-INTEGRATION.md).

For live MSFS setup, use [First MSFS test](MSFS-FIRST-TEST.md), with separate
development-PC and simulator-PC instructions. Native source build:

```powershell
cargo build -p sim-bridge -p desktop-cli --features sim-bridge/msfs --locked
python scripts/sim_bridge.py --msfs --port COM5 --diagnostics --log output/first-flight.jsonl
```

This requires Windows x64 and the MSFS 2020 SDK for the build. The runtime needs
the SDK's `SimConnect.dll`, found through `--dll`, beside the packaged executables'
parent folder, or in the SDK installation. Load a flight in MSFS first. Other
providers still build without enabling the native feature.

## Run here without MSFS

From this branch's repository root, with Rust and Python 3.11 installed:

```powershell
cargo build -p sim-bridge -p desktop-cli --locked
python scripts/sim_bridge.py --synthetic 60
python scripts/sim_bridge.py --replay fixtures/altimeter-lifecycle.txt
```

Without `--port`, this drives the shared Rust desktop CLI; it does not open a
graphical PC window. The browser simulator remains a separate visual reference.
The synthetic provider produces a 0–10,000 ft climb with a deliberate distinction
between indicated altitude, pressure altitude (200 ft lower), and QNH (30.12).
This fixture tests routing; it is not an air-data physics model.

For the board, build/flash this branch using the existing
[source-development instructions](GETTING-STARTED.md). Close PuTTY, then run:

```powershell
python -m pip install pyserial
python scripts/sim_bridge.py --synthetic 60 --port COM5 --report output/bridge.json
python scripts/sim_bridge.py --replay fixtures/altimeter-lifecycle.txt --port COM5
```

Use your own UART COM port. The link is 921600 / 8N1, no flow control; the relay
turns echo off and normally restores it on exit. Only one program owns the port.
The display labels synthetic data `TEST INPUT` and recorded data `REPLAY`.
The Kollsman popup is disabled while external data owns the gauge.

At normal EOF the provider sends Disconnected. On a process/transport failure,
the last sample expires. Neither case resumes a demo automatically. To return
to manual testing after the relay closes:

```text
source sim
sim preset level
instrument altimeter
baro 29.92
```

`source status` reports provider, session, sequence, lifecycle, age and individual
altitude/barometer validity. The ordinary `status` barometer is the last received
setting; `source status` is authoritative for its current validity. `display stats`
measures completed rendering; the relay report's `accepted_hz` measures received
telemetry, not display FPS. `--log NEW.jsonl` writes a bounded-line wire transcript
for diagnosis (not a replay input file).

For a visual check that leaves enough time to watch, use `--synthetic 300`.
Add `--diagnostics` to print live source and display status once per second;
these extra queries reduce available telemetry throughput. `ALT DATA STALE`
after a run ends is expected. During a running synthetic stream, the source
should report `state=running altitude_quality=valid` and display status should
report `shown_quality=valid` with a changing `shown_alt_ft`.

## Replay file

ASCII, one row per newly acquired observation:

```text
# at_ms lifecycle indicated_ft pressure_ft baro_inhg
0 running 8500 8300 30.12
500 paused - - -
1000 running 8600 8400 30.12
1500 disconnected - - -
```

Times must strictly increase; first may be zero. `-` is Missing, `!` is Failed.
An unchanged value still needs a new observation to remain fresh. Gaps are
preserved; no fabricated samples fill them. Late polling selects the newest due
row and preserves its age. EOF invalidates the source; put an explicit later
Disconnected row if a final value needs to remain visible briefly. Disconnected
must be the final row. Maximum 100,000 rows, 256 bytes per row. Invalid values,
duplicate times and oversized input abort instead of being repaired silently.

## Current wire profile

One ASCII command/response in flight at a time, maximum 256 command bytes and
1,024 response bytes (excluding command terminator). Handshake:

```text
source external synthetic SESSION
source external replay SESSION
source status
```

`msfs2020` identifies the implemented native provider; `dcs` is reserved for the
planned adapter. Sessions are nonzero u64 values and
must increase within a board boot, including after returning to demo. The relay
uses host UTC microseconds as a unique increasing session convention; a regressed
host clock may require a corrected clock/new session. Session IDs do not compare
clock domains and do not measure freshness. Aircraft changes require new sessions.

On the recovery branch, a new external session that receives no first frame
expires after 1,000 ms: readings stay absent and the face reports LINK STALE
instead of remaining LOADING indefinitely. Measurement age stays absent until a
frame actually arrives. Fresh explicit Loading frames keep the link alive but
expose no numeric readings; valid Running frames restore the gauge normally.
Hardware regression: `python scripts/check-external-timeout.py --port COM5
--report output/external-timeout.json` (one command, simulated test input).

```text
T1 SESSION SEQUENCE RECEIVER_MS AGE_MS STATE INDICATED_FT PRESSURE_FT BARO_INHG *CRC32
```

- `T1` is the current altimeter profile. Exact single spaces; decimal unsigned
  IDs/times; sequence nonzero and strictly increasing within a session.
- `STATE`: `running`, `paused`, `loading`, `disconnected`. Only Running exposes
  values as valid. Disconnected terminates a session; it cannot resume.
- Numeric fields accept finite values or `-` / `!`. Ranges: indicated altitude
  −3,000..65,000 ft, pressure altitude −2,000..60,000 ft, QNH 28.1..31 inHg.
  These are current profile limits, narrower than the general provider model.
  Values outside the profile are rejected, not clipped. No pressure correction
  is applied to already indicated altitude. Decimal encoding uses three places;
  `f32` precision also applies on the board (about 0.004 ft at 65,000 ft).
- The full frame replaces all three values atomically. Missing clears a value;
  rejected frames change neither values, sequence nor receipt time.
- CRC-32/ISO-HDLC covers the bytes before ` *`, encoded as eight hex digits.
  The check vector `123456789` is `CBF43926`. CRC detects corruption; it does not
  authenticate a source. This is a trusted local UART protocol, not a LAN service.
- `AGE_MS` is elapsed acquisition age at sender serialization, not a timestamp
  from a foreign clock. The provider model retains each field's age; this small
  profile conservatively sends the oldest of its present numeric fields.
- `RECEIVER_MS` echoes the board's `now_ms` from the most recent source/telemetry
  response. The board rejects future values or credits older than 250 ms, then
  adds that elapsed time to AGE_MS. This conservatively includes transmission,
  blocked UART buffers, partial-line assembly and owner-queue delay without
  pretending that the PC and board monotonic clocks are synchronized.
- ACK: `OK telemetry session=... seq=... now_ms=...`. The host requires matching
  identity and a bounded response deadline. Idle `source status` requests renew
  clock credit, **never sample age**. A timeout closes the stream; there is no
  blind retransmission or response reuse. Data expires at age >=1,000 ms.

The relay polls up to 30 times/s and sends only new observations. All capacities
are bounded. The receiver-clock age is a conservative upper bound, not a precise
one-way latency measurement. A transport/profile for faster attitude data must
honor its 100 ms freshness deadline and measure adequate latency before release.

## Verification

```powershell
cargo test --workspace --locked
python -m unittest discover -s scripts -p test_sim_bridge.py -v
python scripts/cli.py --test
```

Tests exercise CRC errors, duplicate/wrong sessions, ordering, delayed/future
clock credit, delayed partial lines, pause, missing/failed fields, disconnect,
stale expiry under status traffic, read-only ownership and barometer expiry.
Firmware build and physical rendering checks are additional acceptance steps.
Setting `RFI_TEST_PORT=COM5` explicitly runs the Python acceptance checks against
hardware instead of the desktop; they change the selected source and leave it
external with missing data. Close terminals first and restore `source sim`
afterward. [Measured bench results](../verification/SIMULATOR-LINK.md).
For live MSFS acceptance, run a native adapter on the son's MSFS 2020 PC with the
board connected there; compare the selected cockpit instrument at standard and
nonstandard QNH. DCS needs its own aircraft profile and acceptance checks.

## Optional barometer control profile C1/R1

T1 telemetry is unchanged. Controls are enabled explicitly for one MSFS session
with `control enable SESSION`; source/session changes reset this capability.
The default bridge remains read-only. `--writeback` enables the primary profile.

```
C1 SESSION REQUEST CHANNEL CENTS_INHG AGE_MS *CRC32
R1 SESSION REQUEST sent|confirmed|rejected|timeout *CRC32
```

Both use the same ASCII CRC32 convention and 256-byte bound as T1. Only channel
0 (profile primary) is supported. Values are integer hundredths inHg, 2810..3100;
request age must be <=500 ms. Board Apply creates one pending request; the
telemetry ACK advertises `control=ID` and `control poll` returns `OK control C1 ...`.
Repeated polls do not cause repeated writes. The host accounts for control-poll
transit age, correlates session/ID, and sends at most once. Capability negotiation
fails explicitly on old firmware rather than enabling local-only adjustment.

Provider acceptance returns sent, never confirmed. A matching later simulator
read must arrive within 2 seconds and within 0.002 inHg (covering hPa/16 rounding).
The board displays telemetry only and checks its own fresh matching readback
before accepting confirmation. A fresh cockpit change always wins; no echo loop.
Stale data, pause, source/session change, disconnect or timeout cancels the edit;
no request survives restart. Simulator exceptions terminate the session.

Transport checksums detect corruption, not authentication. This remains a local
trusted USB/CLI transport; a future network transport needs its own trust model.
