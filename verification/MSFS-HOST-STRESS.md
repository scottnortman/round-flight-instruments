# MSFS host-path stress qualification — 2026-11-03

Historical failure baseline. The subsequent user-requested
[bounded continuity candidate](CONTINUITY.md) separates raw 200 ms validity from
short visual holds. Retain the results below as the pre-fix evidence; they are
not results for the updated firmware.

**Result: qualification remains open.** These extra tests found freshness misses
that were absent from the earlier steady synthetic-stream tests. No production
code, firmware, clocks, age limits or installed software was changed for this
matrix. Preview 6 remains a private candidate, not a fully qualified flicker fix.

## Scope and method

Test executable: the exact `instrument-connector.exe` from private preview 6
(`artifacts/windows-driver-20261103-182748-801/package/`). Installed board ELF:
`BF19759764425F2A084B75D19D34388D6B7E55C710D7ABCB82B110C4A9D5B837`.

New repeatable harness: `scripts/check-msfs-host.py`, with test-only
`tests/fixtures/simconnect_timing.cpp`. It exercises the actual MSFS provider,
isolated worker process, native SDK-header decoder, T2 encoder, CH343 UART,
firmware model and three renderers. The DLL queues replies nonblockingly with
10–40 ms jitter. Animated values correspond to the request time, not the later
dispatch time; real ages are retained throughout. Pitch/bank, altitude, turn
rate and ball all move, alongside the six-instrument data profile.

This is not Microsoft simulator execution. The fixture requires an explicit
test-state environment variable, identifies itself as a test server, lives only
in ignored artifacts, and never replaces or ships as Microsoft's runtime.
The installed SDK and packaged Microsoft DLL remained byte-identical.

CPU cases constrain connector, worker and two normal-priority CPU-bound
competitors to the same two logical CPUs. Child affinity is checked by the
harness; the worker mask was also observed as `3`. The development host is a
Ryzen 7 5800X, 8 cores / 16 logical CPUs. The other host CPUs remain available.
This does not emulate MSFS GPU load, every scheduler condition, or the son's PC.

All cases enable the real connector's writeback capability but make no explicit
pressure edits. Fixture mapping/transmit functions record any write attempt;
none occurred. Timed steady cases use tray-equivalent status output and logging,
with no diagnostic UART queries during motion. Fault/recovery cases deliberately
enable sparse diagnostics to confirm a valid physical presentation afterward.

## Unmodified candidate results

| Case | Completed FPS | Added valid-source expiry ticks | Result |
| --- | ---: | ---: | --- |
| Altimeter, jitter, 20 s | 32.34 | 0 | Pass |
| Attitude, jitter, 20 s | 32.23 | 1 | Fail: intermittent freshness miss |
| Attitude repeat, jitter, 20 s | 32.21 | 0 | Pass in repeat; original failure retained |
| Turn, jitter, 20 s | 32.37 | 0 | Pass |
| Altimeter, CPU contention, 20 s | 32.30 | 0 | Pass |
| Attitude, CPU contention, 20 s | 30.03 | 575 | Fail: repeated freshness misses |
| Turn, CPU contention, 20 s | 32.33 | 362 | Fail: repeated freshness misses |

The counters are guard polling ticks, not counts of separate flicker episodes.
The normal Attitude failure occurred during the stream, not at disconnection.
Its maximum observed retirement age was 172 ms and minimum visible margin
28 ms; a nominal next presentation takes approximately 31 ms. These figures
show why completed FPS alone cannot qualify continuous freshness. They do not
establish the exact causal chain for the individual missed deadline.

Under CPU contention, maximum within-session sample gaps grew to 112–114 ms.
Attitude/Turn capture ages approached 197–199 ms, leaving insufficient room for
composition and confirmed LCD presentation inside the 200 ms policy. The guard
correctly invalidated expired flight indications. Further diagnosis is needed
to separate host scheduling, UART/owner handoff and frame queue contributions.

## Recovery and control isolation

The real MSFS-provider path recovered from two injected aircraft-change events:
three fresh sessions, fresh readings in the final session, and a diagnostic
confirming the physical Attitude view was valid after recovery. No executable
restart, user flag change, spontaneous simulator write, board fault or reset
was required. This does not qualify transition flicker; the intentional source
changes include invalid intervals.

Separate 15-second Attitude and Turn cases withheld one reply for 500 ms.
Both exercised invalid-data handling, retained the late reading's age of at
least 500 ms, then recovered to a diagnostically confirmed valid physical view.
The short invalid interval may fall between one-second probes, so the harness
accepts a recorded numeric guard expiry or an observed stale view as fault-path
evidence; it separately requires valid post-fault presentation. These deliberate
negative tests are not included in zero-expiry performance acceptance.

Three targeted connector regressions also passed: active-gauge-only writes,
switch-between-poll-and-claim cancellation, and discarded old writes after
session recovery. No physical touch/Apply test was newly performed in this run.

## Scheduling experiment, not shipped

A test-only comparison used Windows Above Normal priority for the connector and
worker, retaining two normal-priority competitors and the same CPU affinity.
Over 30 seconds each, loaded Attitude and Turn achieved 32.18 / 32.33 FPS with
zero valid-source expiry; maximum sample gaps fell to 62 / 59 ms. However,
normal-load Attitude still had one expiry tick in 30 seconds at 32.17 FPS.
Priority is therefore a promising mitigation for host contention, not a complete
fix. It was not added to production software or the supplied installer.

Windows documents the priority classes and scheduling behavior in
[SetPriorityClass](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setpriorityclass)
and [Scheduling Priorities](https://learn.microsoft.com/en-us/windows/win32/procthread/scheduling-priorities).
This experiment makes no hard real-time guarantee.

## Reproduction and next work

Close serial clients, select a Python environment with pyserial, and supply
the enrollment for this board and the packaged connector. Use a new output
directory for every run; requires Windows, MSVC and MSFS SDK headers:

```powershell
python scripts/check-msfs-host.py --config device.json --connector target/release/instrument-connector.exe --report artifacts/host-check/report.json
```

Optional `--cases normal-attitude loaded-attitude loaded-turn` selects cases;
`--above-normal` reproduces only the scheduling experiment. The test restores
explicit local level Altimeter at 4,520 ft / 29.92 and terminates its CPU workers.
It does not change installed drivers, system settings or the firmware.

Raw results are retained under ignored `artifacts/msfs-host-stress-20261103*`.
The initial harness incorrectly filtered T2 fields by their age component; it
was corrected before evaluating the matrix above. Sparse negative-test probes
were also corrected to use the guard counters as described above. Hardware
failure traces are retained, including the intermittent normal Attitude miss.

Next work is to reduce delivery/presentation delay while retaining the 200 ms
external limit, and rerun this matrix before qualifying the fix. Live MSFS on
the target PC is still required for aircraft variable accuracy, visual behavior,
GPU contention, installed tray/startup behavior and real aircraft/USB recovery.
No source commit, tag, merge, push or new installer release is claimed here.
