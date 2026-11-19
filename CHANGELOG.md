# Changelog

## [0.2.0] - 2026-11-18

### Added

- Save, import, edit and preview aircraft profiles in the native Windows editor, and explicitly apply the selected profile to the connected board with confirmation in tray Status.
- Use the Combined flight-panel screen alongside the six standard instrument faces and DEBUG screen.
- Run DEMO from every screen and return to the current live source when finished.
- Calibrate the IMU through guided LEVEL and FULL 24-pose workflows with review, explicit save and persistent coefficients; use calibrated six-axis attitude fusion for bench testing.

### Changed

- Read larger, bold 30/60-degree bank labels on Combined and use its compact outlined QNH button to open the shared pressure editor.
- Keep aircraft scale settings consistent between Windows previews and board rendering, including revised profiles and configurable VSI ranges.
- Distribute the accepted Windows test.13 build and November 9 firmware together as the v0.2.0 release, retaining their original binary identities.

### Fixed

- Match Combined VSI direction, zero position and scale to the standalone VSI.
- Preserve live-data freshness and source ownership while switching views, using DEMO or completing calibration.
- Guard configuration delivery with CRC, session, revision and acknowledgement checks before sending telemetry.

Earlier released milestones are documented in `docs/releases/` and Git tags.
