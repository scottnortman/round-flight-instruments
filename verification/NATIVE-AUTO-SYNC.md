# Automatic synchronization correction - 2026-11-01

The user rejected requiring a restart/flag after aircraft changes. This correction
removes the forced-read-only recovery policy and its prompts. Default operation
automatically receives MSFS altitude/QNH and replaces the physical gauge values;
`--writeback` is never needed for that direction. If optional gauge-to-MSFS control
was enabled at launch, recovery now preserves it without user intervention.

Only configuration is preserved. The old SDK connection, session ID, measurements,
pending writes and confirmations are discarded. Fresh readings from the new
aircraft are required before a new edit can be accepted. No recovered gauge setting
is sent to the simulator automatically, and simulator readback remains authoritative.

## Verification

- The changed two-way recovery regression failed against the previous implementation
  at the forced disable, then passed after preserving the configured mode.
- 122 Rust tests and native-enabled all-target Clippy with warnings denied pass.
  Tests against the actual instrument core verify automatic altitude/QNH replacement
  without writeback, retention of enabled two-way mode, missing data before fresh
  telemetry, rejection of old frames/results, no replay of the old edit and correct
  handling of a new post-recovery edit with simulator confirmation.
- Six native executable acceptance tests pass. The switch fixture now changes QNH
  as well as altitude, and asserts replacement on every running frame without a
  writeback flag or intervening command. Formatting and diff whitespace checks pass.
- Native UART hardware test used the existing board firmware and explicit synthetic
  TEST INPUT. It exercised automatic recovery, one temporary connection refusal,
  and three sources with different values:

| Session | Frames | Last valid altitude | QNH read back from board |
| --- | --- | --- | --- |
| 1793585996342060 | 50 | 10,000 ft | 29.92 inHg |
| 1793585996342061 | 51 | 15,000 ft | 30.01 inHg |
| 1793585996342062 | 49 | 20,000 ft | 29.80 inHg |

All 150 frame CRCs, sequences and ACKs passed. Board source-status diagnostics
confirmed each new QNH with writeback=false; each session's display sample was
valid and its display status reported no failure. No control requests were sent.
Each session ended with a measurement-empty Disconnected frame. The connector
released COM5 at completion. Local evidence:
`output/native-auto-sync-board/session-1793585996342060/`.

Diagnostic queries were enabled for this short readback test, so its timing is
not a normal-operation benchmark. No firmware or rendering change was made.
Live MSFS plane-switch acceptance still requires the simulator PC. The hardware
test is synthetic and does not establish arbitrary aircraft control compatibility.

## Subsequent live report: recovery not yet accepted

The user reports that the first aircraft worked but the next did not. The console
capture for session 1793590154447557 shows a session-change recovery to ...558,
then another session-change recovery starting toward ...559. The user confirms
that Ctrl+C was pressed; returning to PowerShell is not evidence of a crash.
The capture has no telemetry or timing sufficient to determine whether fresh
readings arrived after the first reconnect, or how far the second attempt got.
The original session's logs and time spent waiting after loading/unpausing are
needed before identifying a cause. Do not mark live recovery as passed.
The user subsequently confirmed waiting about one minute after the new cockpit
loaded, with the red X throughout, before pressing Ctrl+C. This establishes that
the reported failure was not merely an immediate interruption of the retry wait;
the logs are still needed to locate where progress stopped.

The six native executable acceptance tests were rerun and passed (18.933 s).
These exercise synthetic source switches, not this live SimConnect event sequence;
they do not reproduce or disprove the reported failure. No recovery behavior was
changed on the basis of this console capture.

This supersedes the forced-read-only policy in the historical
[first recovery verification](NATIVE-RECOVERY.md). The latest
[setup guide](../docs/NATIVE-CONNECTOR.md) describes the corrected behavior.
