# Initial Git baseline verification - November 1, 2026

This milestone imports the existing browser visual reference, places the current
Rust/hardware work under Git, and establishes the root README as the canonical
status reference with a detailed getting-started guide. Original project code
is Apache-2.0; third-party font and upstream scaffold attribution are retained.
The README and NOTICE disclose development with ChatGPT and Codex assistance.

## Checks completed

| Check | Result |
| --- | --- |
| Rust workspace tests | 49 passed |
| Clippy | All workspace targets, warnings denied, passed |
| Formatting | Root workspace and separate firmware workspace passed |
| Browser | All five test suites and build passed |
| Rust raster example | 24 PPM snapshots generated at 240/480/720 pixels |
| Desktop CLI acceptance | 57 requests passed, including the 30-second incomplete-line timeout; max response 47 ms, max owner gap 21 ms |
| Desktop takeoff | Stop/manual override/fault rejection and five-second climb passed; final altitude 10000 ft |
| Staged-source export | Exported only indexed files to a separate directory; Rust tests/build/snapshots/desktop protocol checks and browser checks/build passed without ignored source artifacts |
| Documentation | Relative Markdown file targets checked against tracked files; no missing or ignored targets |
| Public-file review | No recovery images, firmware archives, build outputs, local checkpoints or Python caches staged; common private-key/token patterns found no matches |

Evidence: [desktop protocol report](git-baseline-desktop.json),
[takeoff report](git-baseline-takeoff.json). The clean source export reused the
installed host toolchain and dependency cache; it is not a clean-machine firmware
provisioning test. Desktop protocol checks in that export omitted the slow
30-second case, already exercised in the primary checkout.

No hardware was flashed or tested during this repository/documentation milestone.
The installed firmware and prior physical acceptance remain represented by
[TAKEOFF.md](TAKEOFF.md) and [DISPLAY-SCANOUT.md](DISPLAY-SCANOUT.md).
The 15.47 FPS result is that earlier measured board run, not a new measurement.

The initial tracked content is approximately 2.5 MB, including source, required
font/raster assets, documentation and selected verification evidence. Historical
UART log bytes are preserved through Git attributes, including their original
line endings. Local checkpoints and recovery archives remain on disk outside
version control. The required font bytes were preserved unchanged.

GitHub publication is left to the project owner; repository initialization and
this baseline do not imply a remote repository exists or has been pushed.
