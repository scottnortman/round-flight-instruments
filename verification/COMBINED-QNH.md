# QNH gray outline - November 8, 2026

QNH button border now shares OUTLINE rgb(57,65,70) with the adjacent lower gauge
circles. Geometry/text/editor behavior retained. Five existing Combined renderer
regressions pass; production preview inspected. Built/flashed and application
readback verified. UUID and saved IMU coefficients preserved. Board left Combined,
local SIM level, DEMO off and IMU off. Host test.13 remains compatible.
ELF SHA256 2D1C9339E1B2E01575D3B2E2E176A8096FDD15E9D17B842D0DC97A59940DAFAF.
Exact ELF/application/previews/evidence: artifacts/qnh-gray-20261108/.

# QNH layout refinement - November 8, 2026

Button reduced from 208x30 to 188x26, moved from (136,414) to (146,422).
Text reduced from 15 to 14 and centered from the shared rectangle at (240,435).
Six clear pixel rows separate it from the lower dial outlines; corners remain
inside the round bezel. Touch target uses the same rectangle as the drawing.
Production normal/pressed/editor previews inspected. 113 UI tests and strict
UI all-target Clippy pass. Built/flashed/readback verified on COM5; 32.19 FPS
Combined at rest, no failures. UUID and full saved IMU coefficients unchanged.
ELF SHA256 94B26CD825B188191F38815E9B5E3F2767AB7114545BD71D24AAB7B8B6BA2FA6.
Exact firmware/previews/evidence: artifacts/qnh-layout-20261108/.
Board left in local SIM level, Combined, DEMO off, IMU off. Physical touch check
remains with user. Existing test.13 host compatible. Previous assets preserved.

# Combined QNH button — November 8, 2026

The Combined footer has a one-pixel white outline around QNH and accepts a short
tap/release. The shared pressure editor is reused without changing its controls:
minus/plus 0.01, standard 29.92, Cancel and Apply. Opening does not commit a value.
The opening gesture has the same contact freshness, drag, multitouch and display
checks as the standalone entry point. A screen change cancels the draft.

External control uses the existing session-bound request, claim and telemetry
confirmation path. Combined requires T2, valid altitude/QNH and enabled MSFS 2020
controls. Read-only, DEMO and calibration block editing. Other unavailable panel
components do not suppress the editor when altitude and QNH are valid.

## Validation

- 297 workspace/all-target tests pass with the connector MSFS feature; strict
  Clippy passes. New tests cover the opening release, presented editor guard,
  draft/Apply, rejected gestures, screen changes, read-only and confirmed external
  control dispatch. Renderer tests compare popup pixels with the standalone
  editor and full/incremental rendering across all three buffers, including
  unavailable airspeed with valid pressure data.
- Production renderer previews inspected: normal footer, blue pressed footer,
  and the shared editor. `output/consolidated/{level,qnh-pressed,qnh-editor}.png`.
- ESP32-S3 build/flash passed. ELF SHA256:
  `51D2B6EE039A37E4EC5447E0F596B5EE38D5E08223E671D7F28C277E97DF21EA`.
  Application 1,293,104 / 4,194,304 bytes. Full application flash readback passed.
- Eight-screen board regression passed at 32.11–32.50 FPS, zero deadline blanks;
  retained DEMO/IMU ownership and restored streamed values passed.
- UUID and saved IMU calibration generation 4, including all coefficients,
  verified unchanged. Final state: local SIM level, Combined, DEMO off, IMU off.
- Exact firmware, previews and evidence preserved under
  `artifacts/combined-qnh-20261108/`. Previous test.13 release is unchanged.

No physical touchscreen acceptance or new MSFS flight was performed by the agent.
The existing test.13 host is protocol compatible; no host reinstall is required.

## Physical and MSFS acceptance

1. With DEMO off, tap and release the lower QNH box on Combined. Confirm blue
   press feedback and the same BARO IN HG popup as on Altimeter.
2. Tap plus once, then Cancel. Confirm QNH is unchanged. Reopen, tap plus once
   and Apply. In local SIM, confirm QNH and indicated altitude update together.
3. In MSFS 2020, connect the test.13 host with aircraft controls enabled. Repeat
   Apply and confirm simulator QNH and both board pressure readouts agree after
   telemetry readback. There must be one requested change, without repeats.
4. Reconnect read-only, then test DEMO: the QNH box must not open an editable
   popup in either mode. Return DEMO off before resuming live operation.
5. Confirm changing gauges, a stale source or opening calibration cancels any
   draft. Dragging off the QNH box must not open the popup.
