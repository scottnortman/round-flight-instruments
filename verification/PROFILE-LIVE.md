# Live aircraft profiles and Combined VSI - test.13

The Combined VSI now uses the standalone VSI's needle model: zero left, climb
above zero and descent below, with the configured range and explicit overrange.
The small gauge also labels its left zero index.

Test.12's profile editor only saved previews, explaining the photographed
LIMITS NOT SET face. Test.13 adds **Use on connected board**, explicit manual
profile selection, and **Use defaults**. Existing profile JSON files remain
compatible. The host watches active-profile revisions, reconnects, and sends a
single bounded CRC-protected G1 configuration associated with the new T2 session.
The device validates and applies the entire configuration before accepting live
telemetry. Wrong sessions, corrupt packets and changes after telemetry starts
are rejected. A new source/session clears the previous aircraft configuration.

The host checks the acknowledged UUID, revision, session and settings, then
reports the confirmed profile name/revision in tray Status. Both standalone
airspeed and VSI use the same configured rendering as previews. Cache keys and
per-buffer damage histories include the profile, so revisions and removed arcs
cannot leave old artwork. Combined uses the same configured VSI model. Local
preview sample values remain independent of live telemetry.

Profiles configure displays and remain usable in Read-only mode; they issue no
simulator control writes. Manual selection is retained on the host and reapplied
at connection/recovery. There is no automatic aircraft-name matching. Firmware
holds this aircraft configuration in session RAM; device UUID and IMU calibration
remain separate NVS records. MSFS 2020 live delivery is supported; MSFS 2024 and
DCS authoring remain preview-only and cannot be activated for live delivery.

## Verification

- Workspace/all-target tests with MSFS feature: **293 passed**; strict Clippy passed.
- Tests cover G1 corruption/size/session validation, application after handshake
  and rejection during telemetry, stale acknowledgement rejection, active-file
  revision watching/default/error behavior, and pixel equality between saved
  profile previews and decoded board configuration across three buffers/revisions.
- Production previews show Combined VSI zero left. Native editor UI inspected
  with the Computer Use skill: delivery controls visible, Apply saves and selects
  a profile in isolated storage, and confirmation wording does not claim delivery
  before the connector's acknowledgement.
- Packaged native connector delivered and acknowledged fixture revisions 1 and 2
  through the actual COM5 UART board with synthetic flight data. No aircraft
  controls were queued. This is not a live MSFS flight qualification.
- Hardware custom-profile checks: airspeed full scale 300, linear layout, color
  bands and VSI range 4000; wrong-session/CRC rejection, band removal and source
  reset passed. Airspeed, VSI and Combined ran above 32 FPS with zero blanks.
- The full eight-screen DEMO/ten-field stream regression passed on the initial
  candidate. A final bound-check-only firmware revision is qualified separately
  in the final release results; the IMU calibration was preserved throughout.

### Final qualified build

Firmware ELF **CAE729832B0312D29C2A84F9EEB0F87A92199C14216787D4452953AC07A351CA**,
application **1,293,296 / 4,194,304 bytes (30.83%)**, is flashed on COM5. The
final native test.13 connector delivered revision 2 and streamed 340 synthetic
frames, with no error and a maximum acknowledgement time of 38 ms. An oversized
G1 packet with a valid CRC was rejected without partial application.

The final eight-screen regression passed at **32.09-32.53 FPS**, with zero
display blanks and retained DEMO/IMU ownership and ten-field stream restoration.
Custom 300-KIAS linear and 4000-ft/min configurations ran at **32.19-32.50 FPS**
on Airspeed, VSI and Combined. Renderer minimum stack reserve was **5,700 B**;
free internal heap at final custom checks was **68,259 B**, free PSRAM
**1,522,940 B**. Nonzero expiry counters remain covered by the bounded-hold
policy; the regression recorded 29 expiry ticks, not zero late frames.
Existing saved IMU calibration generation **4** and its complete coefficients
were preserved, as was the device UUID. Test profiles used isolated host
storage; no user profile selection was overwritten.

Final evidence: `artifacts/profile-live-final-native.log`,
`profile-live-final-boot.json`, `profile-live-final-screen-regression.json`,
`profile-live-final-custom-board.json` and `profile-live-flash-readback.log`.
Host installer: `artifacts/releases/0.2.0-test.13/`.

Evidence: `artifacts/profile-live-tests.log`, `profile-live-clippy.log`,
`profile-live-native-uart.log`, `profile-live-native-revision2.log`,
`profile-live-custom-board.json`, `profile-live-screen-regression.json` and
the final release package results. The known constrained-CPU strict zero-expiry
limit from test.12 remains; zero blanks does not mean zero late-frame counters.

An Xtensa optimizer failure in the formerly desktop-only profile-label path was
resolved by keeping label font sizes constant on that shared path. Previews and
firmware still use identical scale/arcs/needle logic. An existing IMU transaction
test now retries the documented transient snapshot-busy response rather than
occasionally failing under concurrent test scheduling; production behavior is
unchanged by that harness correction.

User steps: [test.13 installation and test instructions](../docs/MSFS-TEST-13.md).
