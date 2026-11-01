# Animated altitude test and rendering performance — November 1, 2026

`sim takeoff [SECONDS]` provides a portable, monotonic-clock-driven 0-to-10000 ft
bench sweep. It holds zero for one second, climbs for 5..600 whole seconds
(default 60), then holds the endpoint. `sim takeoff status` reports progress;
`sim takeoff stop` holds the current altitude with fresh samples. The start
selects altimeter, barometer 29.92 and running simulation. IMU and other inputs
are unchanged. This is an altitude-only animation, not a flight-dynamics model.

The state owner handles the bounded simulation. Rendering remains on its worker.
A start resets board presentation statistics; completion/stop freezes them.
`display stats` reports actual completed presentations per second and maximum
draw/transfer time, not panel refresh rate or uncapped theoretical raster speed.
Manual `display stats reset|stop` is also available. No asynchronous text output
was added to the request/response protocol.

## Safety and tests

- Start rejects invalid duration, active altitude fault or latched timing fault
  before changing settings. No fault is silently cleared.
- Manual altitude changes, clear, level preset or pause cancel the sweep.
  Pause still ages samples to stale; resume does not restart the cancelled ramp.
- Altitude or timing fault aborts the sweep and preserves invalid-data behavior.
- 49 Rust tests and Clippy passed. New tests cover bounds, monotonic motion,
  zero hold, exact endpoint, stop, manual takeover, pause, fault preservation,
  strict parsing and cached/full renderer equality at 240/480/720 pixels.
- [Desktop sweep](takeoff-desktop.json) and [board sweep/controls](takeoff-fast-board.json)
  passed, including frozen final frame statistics and the visible endpoint's
  software state. [UART regression](uart-takeoff.json): 56 checks passed.
- [Display fault checks](takeoff-display-safety.json) and
  [concurrent IMU integration](takeoff-imu-integration.json) passed after optimization.
  No safety deadline or priority was relaxed; forced per-frame restart remains off.

## Measured improvement

| Board run | Completed presentations/sec | Maximum draw time |
| --- | ---: | ---: |
| Initial 5-second climb | 5.98 | 108 ms |
| Initial 60-second climb | 6.13 | 105 ms |
| Cached static backdrop, original extra pause | 10.32 | 36 ms |
| Cached backdrop, paced by completion wait | 15.64 | 36 ms |
| Final 60-second climb with cache/pacing | 15.47 | 35 ms |

Evidence: [initial short test](takeoff-board.json),
[initial full climb](takeoff-60s.json), [cache-only test](takeoff-cached-board.json),
[cache and pacing test](takeoff-fast-board.json).

The user confirmed the initial climb moved correctly without blanking or shifting,
but requested smoother motion. The static-face cache removes repeated rasterization
of ticks, labels and barometer window. Each frame restores the entire cached face
before drawing hands/readout/faults, preventing stale pointers. The caller owns
the optional cache in the no_std UI crate. The board allocates another 460800 bytes
of PSRAM; it never submits this cache to DMA. Geometry is unchanged.

Removing the redundant post-frame 20 ms pause further improved cadence. The
two-completion buffer handoff still waits and yields at least one 10 ms RTOS tick.
Rendering has not reached the earlier 30 fps target. These are observed bench
rates under normal supervision, not worst-case timing guarantees.

Final faster 60-second observation: see [full sweep report](takeoff-smooth-60s.json).
The user confirmed the faster sweep was **smoother and clean**, with no blanking
or shifted edges. It completed 944 presentations in 61019 ms, maximum transfer
47 ms, owner gap 30 ms, no timing fault and no new guard blanking during the run.

Final state: 10000 ft held, barometer 29.92, simulation running, display on,
IMU off, echo on; the sweep and its statistics are complete/stopped. COM5 is free.
Archive: `artifacts/takeoff-render-cache-20261101`, including source and evidence,
ELF, matching bootloader, SDK config, partition CSV, lockfile and SHA-256 manifest.
Application image 898224 / 1048576 bytes (85.66%). ELF SHA256:
eb2857778ca0fb45d3bf3a4639f3ad8a9f7f04be0701ebb3ddca1c0d78a54ede.
