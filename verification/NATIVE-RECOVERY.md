# Native aircraft-switch recovery - 2026-11-01

Historical first-recovery-build evidence. The later
[automatic synchronization correction](NATIVE-AUTO-SYNC.md) supersedes the
forced-read-only/restart policy below. Recovery now preserves the selected mode;
receiving MSFS data never requires `--writeback`.

Branch `feature/msfs-driver`, local working changes from `v0.2.0-rc.1`. This
extends the native connector; board firmware and the T1/C1/R1 wire formats are
unchanged. The latest native setup guide describes the new default behavior.

## Behavior and safety boundary

- MSFS aircraft/flight/position events and resolved user-object changes still
  terminate the old provider. Known liveness timeouts and simulator quit also
  expose an explicit recovery hint. Invalid measurements, SDK/schema exceptions,
  clock reversal and event overflow do not permit automatic recovery.
- Only an acknowledged Disconnected session with that hint can transfer the
  healthy device handle to a new provider. The old SDK handle is dropped before
  reconnecting. A strictly newer session starts with sequence 1 and read-only
  control authority. Pending writes/results and prior readings do not transfer.
- Reconnection waits 1/2/4/8 seconds, capped at 8 seconds between attempts; the
  normal SimConnect-open unavailable error is retryable after an established
  session. Initial absent-simulator startup still fails before COM access.
  DLL/configuration and serial/protocol errors remain terminal.
- `--duration` and Ctrl+C stop during streaming or reconnect waits. The same COM
  handle is retained during source recovery. USB unplug/re-enumeration recovery
  is not part of this change. No interpolation or rendering changes were added.
- Barometer writeback is never automatically carried into another aircraft.
  The user must check the new aircraft, stop, and explicitly restart with
  `--writeback` to reauthorize it. Aircraft-specific mapping remains future work.

## Verification

121 Rust tests pass; native-enabled all-target Clippy with warnings denied and
formatting pass. Added adapter tests distinguish recoverable lifecycle/liveness
endings from invalid data and SDK errors. Runtime tests use the real instrument
core to verify fresh IDs, invalidated readings, read-only recovery, rejection of
old frames/confirmations, no replay of pending writes, refusal to recover a
faulted/active session, ID exhaustion and bounded backoff.

Six native executable acceptance tests pass, including three synthetic sessions
with one temporary connection refusal and a duration stop during retry backoff.
Existing synthetic/replay and opt-in control acceptance remain covered. These
tests use the actual desktop console process; they do not emulate SimConnect's
live aircraft lifecycle on the simulator PC.

The real Waveshare board passed `--switch-demo` over native UART on COM5. The
fixture is explicitly synthetic/TEST INPUT and ends after three short streams:

| Session | Frames | Final valid indicated altitude |
| --- | --- | --- |
| 1793585494608451 | 52 | 10,000 ft |
| 1793585494608452 | 53 | 15,000 ft |
| 1793585494608453 | 54 | 20,000 ft |

All 159 frame CRCs, sequence numbers and ACK correlations passed. Every session
ended with a measurement-empty Disconnected frame. There were two successful
recoveries and one intentionally refused connection attempt; all three starts
were read-only. Total elapsed time 10.261 s includes four seconds of deliberate
backoff, so its aggregate 15.50 accepted Hz is not a steady-stream performance
measurement. Maximum within-session sample gap was 63 ms, maximum ACK 57 ms.

Final independent probe confirmed the third session/sequence 54, disconnected
and stale, writeback=false, display ready=true/failed=false and monotonically
increased uptime. The port was released; no firmware was flashed. Raw local
evidence is in `output/native-recovery-board/session-1793585494608451/` and
`output/native-recovery-final-status.json`.

## Remaining live check

Actual Warrior -> F/A-18 -> third-aircraft switching must still be tested with
MSFS 2020 on the simulator PC. Verify automatic return of fresh cockpit-matching
data after loading/unpausing, read-only recovery and no old barometer write.
This local acceptance does not establish cockpit mapping for arbitrary aircraft,
live MSFS restart timing, USB reconnect support or a hard 30 Hz deadline.
