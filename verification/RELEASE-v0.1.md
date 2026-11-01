# v0.1 release verification — 2026-11-01

## Standards review

Reviewed staged changes against main (`f8eb717`) for correctness, safety,
bounded input handling and repository conventions. One P2 finding was resolved:
touch startup now remains unknown until an acknowledged controller report,
instead of fabricating a released contact after identity probing. A regression
test proves unknown/down startup cannot arm until an observed release. Re-review
found no outstanding findings.

## Spec review

Reviewed against the agreed touch contracts: 250 ms hold, blue press feedback,
release-to-activate, 0.01 inHg steps, draft-only standard-pressure reset,
Apply-only commit, Cancel and 30-second timeout. No outstanding findings.
Performance claims distinguish ordinary animation from intensive editing.

## Validation

- `cargo test --workspace --locked`: 72 tests passed (22 instrument-ui tests).
- Workspace Clippy, all targets with warnings denied: passed.
- Browser `npm run check`: five regression suites passed; build passed.
- Normal firmware build: passed; watchdog self-test disabled.
- ELF SHA256: `1FD4AAD250884D936754809EC9C2E32AE77F258FC43029C47DB52DE41FBC23FC`.
- Application size: 895,536 / 1,048,576 bytes (85.40%).
- Combined image: 961,072 bytes, bootloader at 0, partition table at 0x8000,
  application at 0x10000; DIO / 40 MHz / 16 MiB flash.
- Combined image flashed using esptool 4.12.0 and `verify_flash`: digest matched.
- Release UART evidence: `release-v0.1-touch.json`, `release-v0.1-takeoff.json`.

Earlier physical touch acceptance and rendering measurements remain documented
in [TOUCH.md](TOUCH.md). No claim of flight qualification or guaranteed frame
rate is made. A factory backup was verified before flashing; restoration of the
factory backup has not been physically tested.

## Final scope additions

All five popup buttons now show blue press feedback with release-only activation.
The active buzzer supplies a nominal 40 ms screen-press chirp, with mute/status
CLI controls and a 150 ms rate limit. A second review finding was fixed: automatic
IMU sampling-failure cleanup now quiesces sound before its command handshake,
just like queued commands. Regression tests cover cleanup ordering and rejected
commands when quiescing fails. Both reviewers reported no outstanding findings.

The user physically confirmed blue button feedback and short chirps without
continuous buzzing or artifacts. The subsequent opening threshold change from
500 to 250 ms is covered at 249/250 ms in the portable tests, with the final
board CLI confirming 250 ms after reboot. The popup still times out after 30 s.

`release-v0.1-feedback.json` records touch/display/sound monitoring during the
physical check; `release-v0.1-sound-cli.json` records mute/enable validation.
Those captures and the 32.33 FPS 20-second sweep were taken on the immediately
preceding build (ELF 7253F0F1CEECAD768076A1C71FF3D5A42656CFA5A6DE6389FF2D58B4480A475A),
whose only subsequent behavior change was the 250 ms hold threshold. Final-image
checks are recorded in `release-v0.1-final.json`, `release-v0.1-touch.json` and
`release-v0.1-final-takeoff.json`. Fault pulse duration is not guaranteed; see
[the buzzer fault contract](../docs/TOUCH.md).
