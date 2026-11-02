# Simulator-link offline milestone — 2026-11-01

Branch `feature/msfs-integration`, based on v0.1.1. Development firmware, not a
release. This section records the initial offline milestone. For subsequent
native MSFS and two-way acceptance, see [MSFS-NATIVE.md](MSFS-NATIVE.md).
DCS remains unimplemented.

## Correctness checks

- 85 Rust workspace tests pass; workspace/all-target Clippy passes with warnings
  denied. Firmware builds with the existing generated-SDK and I2C link checks.
- Three Python protocol/transport tests pass on desktop and COM5, including a
  genuinely delayed partial UART line, CRC rejection, duplicate sequences,
  session isolation, status traffic not refreshing samples, pause/resume,
  terminal disconnect and read-only ownership. Legacy desktop CLI acceptance
  also passes (56 requests).
- Raster comparison checks source/lifecycle labels, missing barometer values
  and restoration across all three framebuffers against complete redraws.
- Generic measurement tests cover channel separation, explicit units, independent
  ages, capacity limits, invalid values and missing fields. Projection never
  applies the local barometric correction to simulator-indicated altitude.
- Abruptly terminated the diagnostic relay without a Disconnected frame. At the
  next probe the board retained `state=running` but correctly reported both
  quantities stale (age 1,400 ms), with `timing_fault=false`. No demo fallback.
  The display guard recorded a blank at expiry before resuming invalid-data
  rendering; a stale numeric frame is not kept visible to avoid that transition.

## Measured performance

Waveshare SKU 29086, 480x480, 18 MHz pixel clock, UART 921600/8N1, IMU off.
One outstanding text request; no interpolation or prediction.

| Measurement | Result |
| --- | --- |
| Initial 60 s bridge run | 16.98 accepted samples/s; 32.14 completed display FPS |
| After queuing ACK in the same owner tick, 60 s run | 19.92 accepted samples/s; max ACK 47 ms |
| Focused final 20 s run | 403 frames accepted in 20.109 s = 20.04 samples/s |
| Display across that run plus setup/teardown | 663 presentations in 20.770 s = 31.92 FPS |
| Maximum observed render / transfer duration | 118 / 130 ms, including source/layout transitions |
| Maximum sample gap in focused run | 94 ms |
| Maximum owner gap across subsequent diagnostics | 32 ms; no latched timing fault |

Average rendering meets the 30 Hz preference for this workload. It is not a
hard per-frame deadline guarantee, and incoming measurements are about 20 Hz,
not 30 Hz. More throughput work remains before claiming a 30 Hz data path or
support for attitude's tighter freshness budget. The all-instrument model does
not imply that other hardware renderers or wire profiles are implemented.

The initial user report of static `ALT DATA STALE` occurred around short test
runs. A longer diagnostic stream was then observed: the user confirmed
`TEST INPUT` and moving altitude. Live source and presentation metadata both
reported valid increasing altitude. The longer visual confirmation resolves
the apparent static-state concern; it does not substitute for a full artifact
or read-only popup acceptance check.

## Final board and artifacts

- Board restored explicitly to manual simulation: pressure altitude 10,000 ft,
  QNH 30.00, indicated altitude approximately 10,073.91 ft; display valid.
- Port closed and device character echo restored. Hardware Kollsman editing
  is available again in manual mode.
- Flashed ELF SHA-256:
  `7011F7D65121B3CE620328A22905C97773EE059EB23E198DA46A44DE49E3E3AD`.
- Raw local evidence is under ignored `output/`: `msfs-tests.txt`,
  `bridge-board-measured.json`, `bridge-board-measured-render.json`,
  `bridge-live-diagnostic.jsonl`, and `bridge-board-loss.json`.

Next acceptance: native read-only MSFS adapter on the son's MSFS 2020 PC, correct
instrument-channel/QNH mapping, cockpit comparisons, pause/load/reconnect and
live performance. DCS requires its own aircraft-specific adapter/profile.
