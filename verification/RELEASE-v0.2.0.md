# v0.2.0 acceptance and release verification

Date: November 18, 2026.

The user explicitly reported: "the latest FW and windows app have been tested
and work as expected" and authorized commit, tag, push, merge to main, release
and checkpoint, followed by DCS integration. This supersedes the pending
functional acceptance in the November 9 checkpoint. It does not claim
independent IMU dynamic-accuracy qualification or qualification of other aircraft.

## Accepted artifacts

- Windows test.13 installer SHA256:
  `484D1BD620E823B3758AC138BCA1A4A4B74C89FDB7C57DB46CEB41DEDA37257D`.
- November 9 firmware ELF SHA256:
  `AFBC7782D819824BDED2770F927EA6DB34AA30D97A9AAEE88B3D1CD4F2598E53`.
- Firmware application: 1,292,784 bytes. The original test.13 bundle contains
  older firmware; v0.2.0 packaging replaces it with the accepted November 9 image.
- Accepted artifacts are promoted without rebuilding. The app's internal
  test.13/0.2.0.13 build identifiers remain unchanged.

## Fresh release checks

- `cargo test --workspace --all-targets --features instrument-connector/msfs --locked`:
  297 passed, zero failed, including 113 renderer tests.
- `cargo clippy --workspace --all-targets --features instrument-connector/msfs --locked -- -D warnings`:
  passed.
- `python -m unittest discover -s scripts -p 'test_*.py'` after building the
  current native executables: 20 passed. Includes synthetic/replay, process-crash
  recovery, malformed-packet rejection, six-pack recovery and partition guards.
- The three SimConnect worker recovery/malformed-packet/six-pack checks also
  passed against the exact packaged test.13 executables selected with
  `RFI_NATIVE_BIN`, without recompiling those release artifacts.
- Coverage audit and independent focused review found no blocking regression
  in profile validation/storage, session-bound delivery, QNH control guards,
  calibration persistence or source ownership. No exhaustive numeric coverage
  claim is made.
- Python native executable checks must run against freshly built binaries or
  the explicitly selected packaged binaries. `cargo test` alone does not rebuild
  `target/debug/instrument-connector.exe`; stale standalone binaries caused
  an initial integration mismatch and are not used as release evidence.

The release bundle's manifest, source provenance and SHA256 lists identify the
exact payload. Existing detailed hardware evidence remains in
`PROFILE-LIVE.md`, `COMBINED-QNH.md`, `COMBINED-BANK-LABELS.md`,
`IMU-CALIBRATION.md`, `DEMO.md` and `DEBUG.md` in this directory.
