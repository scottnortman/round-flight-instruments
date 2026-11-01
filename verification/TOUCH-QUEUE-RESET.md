# Watchdog reset found during touch validation

**The mailbox correction alone did not resolve all resets.** A later unprompted
reset had a different trace in legacy I2C interrupt recovery; see
[the second reset investigation](TOUCH-I2C-RESET.md).

During the standard-pressure-button check, the user reported that a two-second
hold appeared to reset the board. The active `capture-touch.py` run captured an
actual task-watchdog abort, not just presentation blanking:

```text
E (36262) task_wdt: Task watchdog got triggered.
 - main (CPU 0)
 - IDLE0 (CPU 0)
CPU 0: main
CPU 1: pthread
Backtrace: 0x4200cd4e 0x4200cd31 0x4200dbb7 0x4200847a 0x42007b85 ...
ELF file SHA256: 5c420d164
Rebooting...
```

The failed read-only capture is `touch-standard-capture.json`. The capture
command was `python scripts/capture-touch.py --seconds 35 --report
verification/touch-standard-capture.json`. It ended with a protocol-bound
exception while the device rebooted. The original ELF (SHA256
`5C420D1646230DCDAE9AC9139D5D9BE8441022A561DBB65A5889F54E19AE880F`)
is retained locally under ignored `artifacts/touch-rejected-mpsc`.

## Evidence and diagnosis

Espressif addr2line decoded the addresses against that exact ELF:

- `0x4200cd4e`: `u32::pow`, inside the backoff helper
- `0x4200cd31`: `std::sync::mpmc::utils::Backoff::spin_light`
- `0x4200dbb7`: `Channel<console_runtime::Input>::start_recv / try_recv`
- `0x4200847a`: standard receiver `try_recv`
- `0x42007b85`: `console_runtime::run_with_owner_input`, input receive

The installed ESP Rust standard library's bounded array queue has a reservation
and publication gap: the sender advances tail before publishing the slot stamp.
Its nominal `try_recv` spins if tail says a message exists but the stamp has not
been published. A higher-priority receiver can therefore wait for a preempted
lower-priority producer. The symmetric send path also has internal retry loops.

This is a scheduling hazard in the previous console/IMU queue architecture,
exposed during the feature's validation, not evidence of a reset-pin touch fault.
The captured failure is definitively in that spin path. The exact preemption /
CPU-affinity interleaving was not instrumented. ESP-IDF can pin float-using tasks
to their current CPU, so a CLI float parse can change scheduling conditions;
see the [IDF FreeRTOS FPU documentation](https://docs.espressif.com/projects/esp-idf/en/v5.5.5/esp32s3/api-reference/system/freertos_idf.html).
Two 1200-request read-only replays on the old build passed, including one
after parsing `baro 30.01`; both had maximum observed response time of 79 ms.
These did not reproduce the reset. A short successful stress run alone is
insufficient to disprove this rare race. Transcripts: `touch-std-queue-stress.json`
and `touch-std-queue-float-stress.json`.

## Correction and regression seam

`bounded-mailbox` replaces production standard bounded channels in the console
and IMU services. Owner-side operations make exactly one `Mutex::try_lock`
attempt; contention returns Empty/Full and the next owner tick retries. No
partially published slot is visible. Capacity is allocated at startup; values
move under the short lock. Only I/O-worker `send/recv` wrappers retry, sleeping
10 ms between attempts. No condition-variable notification can introduce a wait
on the owner path. Disconnection, capacity, FIFO ordering and response retention
remain explicit. No unsafe code, watchdog relaxation or priority change is used.

The standard library's internal reservation cannot be paused through its public
API, so a deterministic host regression for the exact old instruction interleave
is unavailable. The replacement's test deliberately holds the queue lock while
another thread invokes both owner-side try operations; they must finish while
the peer remains paused. Other tests cover FIFO/capacity/disconnection, blocking
worker backpressure and the live owner-input hook under stalled UART output.
Hardware validation replays the original physical gesture with diagnostic
traffic and exercises sustained CLI/IMU/display workloads.

Prevention: a method named `try_recv` does not establish a bounded execution time
under unequal RTOS priorities. Review the implementation or choose a primitive
whose deferred-contention contract can be tested at the scheduler boundary.
