# Native connector milestone 1 - 2026-11-01

Historical milestone-1 evidence. The subsequent
[aircraft recovery verification](NATIVE-RECOVERY.md) supersedes the statements
below that automatic aircraft-change recovery is not implemented.

Branch `feature/msfs-driver`, from `bd6bdbf` / `v0.2.0-rc.1`. Native executable,
runtime and packaging are new; board firmware and the SimConnect adapter logic
are unchanged. This is a development milestone, not a public release.

## Automated verification

- 117 Rust tests pass, including runtime tests against the actual instrument
  core for session/ACK correlation, independent epochs, stale data, read-only
  ownership, confirmed post-write readback, duplicate suppression, expired controls,
  pause cancellation, disconnect, malformed replies and clock reversal.
- Native-enabled Clippy with warnings denied and Rust formatting pass.
- Four executable acceptance tests pass against the real desktop console:
  synthetic completion, lifecycle replay (paused/missing/failed), clean duration
  stop, and rejecting synthetic writeback before opening a device. They validate
  every frame CRC/sequence/ACK and run against an extracted release package too.
- Existing six Python bridge regression tests pass. Python is used for development
  tests/packaging only; the native executable does not launch Python or the old relay.
- The actual installed MSFS 2020 SDK DLL loads. With no simulator on this PC,
  setup fails at SimConnect stage 3 before any COM connection attempt, as expected.
- Device enumeration identified the intended CH343 UART board as COM5. Enrollment
  saved its USB serial identity in an ignored local configuration. Unit tests cover
  changing COM numbers, missing/ambiguous identities and unsupported profiles.

## Hardware runs

Waveshare SKU 29086, existing barometer-two-way-v2 firmware, 18 MHz pixel clock,
UART 921600/8N1. The source was explicitly synthetic/TEST INPUT, not live MSFS.

| Measurement | Extra diagnostic queries | Normal mode |
| --- | --- | --- |
| Duration | 60.167 s | 60.061 s |
| Accepted telemetry frames | 1,389 | 1,535 |
| Accepted telemetry rate | 23.09 Hz | 25.56 Hz |
| Maximum ACK round trip | 61 ms | 69 ms |
| Maximum recorded sample gap | 175 ms | 69 ms |
| Median / 95th percentile inter-frame gap from logs | 30 / 63 ms | 30 / 62 ms |
| Completed display FPS across diagnostic samples | 31.59 | Not sampled |

Both logs pass all CRC, contiguous sequence and ACK identity checks. In the first
run, all 53 inter-frame gaps over 80 ms coincided with diagnostic queries. All 53
sampled display reports were ready/not failed and all sampled display data valid.
The user saw clean motion with occasional slight hand jitter, then confirmed the
normal-mode run was smoother. Guides now omit diagnostic queries in normal use.
The optional queries remain available for bounded troubleshooting captures.

No interpolation, prediction or firmware rendering change was introduced. These
average rates are workload measurements, not hard deadlines or proof of support
for attitude's tighter freshness budget. The 117-test count includes a new startup
noise regression: only bounded non-ACK noise during initial echo negotiation may
be skipped; malformed ACKs and active-session noise remain fatal. An initial
hardware connection attempt rejected an invalid response before streaming;
its exact bytes were not captured. A later passive read and echo check was clean.
Do not claim that the original invalid response's precise cause was established.

Local evidence directories:

- `output/native-board/session-1793584459763785/`
- `output/native-board-normal/session-1793584536865378/`
- `output/native-board-final-status.json`

Both runs ended with an acknowledged Disconnected frame. The final independent
probe confirmed the normal-run session/sequence, disconnected/stale readings,
display ready=true/failed=false and a monotonic uptime; no reset was observed.
The port was released and echo restored. The screen may now show stale data,
which is expected after stopping an external source. No firmware was flashed.

## Remaining acceptance

Live MSFS 2020 input and two-way cockpit control through this new native executable
still require the simulator PC. The prior Python-relay Warrior acceptance is not
claimed as acceptance of the new host transport. USB reconnect and automatic
aircraft-change recovery are not implemented; the current session ends safely.
Windows tray UI/installer/startup, multi-device scheduling, new instrument wire
profiles and DCS remain future work. See [setup](../docs/NATIVE-CONNECTOR.md) and
[architecture](../docs/MSFS-DRIVER.md).
