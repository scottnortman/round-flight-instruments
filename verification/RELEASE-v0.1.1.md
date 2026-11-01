# v0.1.1 active-control sound fix — 2026-11-01

## Cause and correction

v0.1's buzzer worker independently detected raw screen contacts, so it chirped
outside interactive controls. The UI interaction controller now emits a one-tick
event only when it accepts a new Kollsman opening press or popup-button press.
The state owner forwards that event through one atomic slot; the worker consumes
it once, rejects events 120 ms old, and retains the 150 ms rate limit. It no
longer reads or interprets touch coordinates. Mute and quiesce discard requests.

The existing UI validity, display readiness, release-to-arm and popup-presented
checks therefore govern both visual and audible feedback. No extra render work
or I²C traffic is added for inactive touches. Sound still indicates an accepted
press, not a committed barometer change.

## Validation

- 74 workspace Rust tests pass, including 24 instrument-ui tests.
- Workspace Clippy, all targets with warnings denied: passed.
- Normal firmware build and SDK safety checks: passed.
- Regression tests cover inactive dial areas, hidden buttons, popup value text,
  all five active buttons, opening hold, held/dragged contacts, invalid/unavailable
  controls, request expiration, rate limiting and the wrapping hardware clock.
- Read-only review of the patch confirmed a single event producer in the UI
  owner and no remaining raw-contact path into the buzzer worker.

Hardware results and final binary provenance follow below.
The original v0.1 evidence remains historical; v0.1.1 fixes its sound hit testing.
The existing I²C shutoff limitations in [TOUCH.md](../docs/TOUCH.md) still apply.

## Binary provenance

- ELF SHA256: `999B40420F73E8EAFC9A8D9C187C979EFA4FCBB98E2E3C36756556FFB452F789`.
- Application: 895,472 / 1,048,576 bytes (85.40%).
- Combined image: 961,008 bytes, written at 0x0, DIO/40 MHz/16 MiB.
- esptool 4.12.0 write and readback digest verification: passed.
- Original factory backup checksum and development board identity verified before writing.

## Physical acceptance

The user confirmed that only active controls chirp. The 60-second capture
`release-v0.1.1-feedback.json` passed with no touch/sound errors, display failure
or owner timing fault. The opening hold remains 250 ms. `release-v0.1.1-cli.json`
checks mute/re-enable and idle shutoff. Rendering results are in
`release-v0.1.1-takeoff.json`.

The 20-second climb passed: 663 completed presentations / 20,999 ms = 31.57 FPS,
maximum draw 60 ms and transfer 76 ms; no display failure or timing fault. This
is a measured workload, not a guaranteed minimum frame rate.
