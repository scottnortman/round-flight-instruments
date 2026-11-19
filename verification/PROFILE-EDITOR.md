# Profile editor verification

Dates: November 4–5, 2026. Feature: `feature/aircraft-profile-editor`, based on
source checkpoint `v0.2.0-rc.10` / `188bfb9`. Profile editor work remains
uncommitted for review; no new installer or public release is published.

## Acceptance

User selected **editor and preview first; live delivery follows**, and confirmed
**“Layout and preview workflow look good.”**

The native Windows window was exercised using the computer-use skill in an
isolated profile folder. Verified:

- Saving incomplete aircraft identity displays an error and creates no profile.
- Create/save produces revision 1; editing airspeed range to 400 and saving
  produces revision 2. Closing/reopening restores the saved identity and range.
- Changing VSI to 4,000 ft/min marks the preview out of date; Save produces
  revision 3 and updates both scale labels and needle mapping.
- Native keyboard gauge selection changes sample labels and the displayed face.
- Export opened the standard file dialog; successful export was observed in
  the editor status. Core tests cover complete export/import round trips and
  import-copy isolation. The November 5 follow-up below completes the native
  Import-dialog walkthrough.
- User confirmed layout and preview workflow. Existing test-window profiles
  were kept. The connected board and installed driver were not accessed or
  updated during this host-editor task.

## Automated checks

**237 workspace/all-target Rust tests** and strict Clippy pass, including the
new profile tests and standalone target. Production host builds also pass.

- All-six schema JSON round trip; exact identity requirements; missing metadata,
  malformed UUID, unknown fields/schema/units, duplicate keys and oversized input.
- Airspeed limit ordering, finite numbers, required reference and scale coverage.
  Invalid VSI range/step and unsupported turn calibration are rejected.
- UTF-8 BOM compatibility and normalized UUID spelling.
- Revisioned saves; rejected invalid/conflicting saves preserve existing bytes.
- External edits are detected even without a revision bump.
- Corrupt files remain in place and are isolated from valid catalog entries.
- Exclusive lock rejects a second owner, then releases correctly on close.
- Export/import round trip; exporting cannot overwrite a managed record;
  importing as a new UUID preserves the original profile.
- All-six deterministic previews, meaningful custom-range pixel changes, invalid
  sample rejection and default airspeed/VSI pixel equality with existing gauges.
- OS-generated profile UUIDs are valid and distinct.

The standalone target shares the UI module, so its UUID boundary test is also
exercised by `--all-targets`. Tests use exclusively created temporary directories;
cleanup verifies their canonical parent and task prefix before recursive removal.

## Boundaries and evidence

The first milestone changes no telemetry protocol, active aircraft controls,
device UUIDs, saved gauge selection or installed firmware. Live profile delivery,
aircraft discovery/matching, additional unit modes and multiple-device assignment
are not claimed. No real aircraft limits are bundled or inferred.

Local ignored evidence is under `artifacts/profile-editor-20261104/`:
`tests.log`, `clippy.log`, isolated `ui-profiles/`, optimized builds, preview
package and checkpoint/manifest. Tool screenshots were inspected directly;
private desktop/window inventories are not copied into public verification files.

The same module is reachable from the updated tray's Configure menu. The
standalone `instrument-profile-editor.exe` provides local review without starting
the tray Supervisor or connector. Source archives retain license/font notices.
No source changes to any existing fast renderer path were needed; the default
render comparisons and existing renderer regressions remain green. Physical
30+ FPS qualification from rc.10 remains the installed-board baseline.

See [usage and design](../docs/PROFILE-EDITOR.md).

## November 5: all-six native UI workflow

The user requested completion of the development-PC test instructions. The
packaged static-runtime executable was exercised through the computer-use skill,
using only synthetic identity and a separate test directory under
`artifacts/profile-editor-ui-20261105/`. The user's existing editor window,
profiles, firmware and installed connector were preserved.

| Preview | UI samples inspected |
| --- | --- |
| Airspeed | 85 and 300 KIAS, configured 400-KIAS prototype scale |
| Attitude | Pitch +5°, roll +20°; pitch −10°, roll −30° |
| Altimeter | 4,520 ft / 29.92; pressure altitude 8,500 ft / 30.12, showing 8,684 ft |
| Heading | Magnetic 90° and 0°, E and N at the fixed index |
| Turn coordinator | +3°/s with centered ball; −3°/s with independent +0.7 ball |
| VSI | +1,000 and −2,500 ft/min on the configured ±4,000 scale |

- Incomplete identity, 35-KIAS full scale and a 4,001-ft/min VSI scale were
  rejected. Invalid form values stayed available for correction, and the saved
  profile bytes/revision were unchanged after failed saves.
- Saving and closing/reopening restored identity, UUID, revision and both scale
  settings. Changing only sample values did not mark the profile as edited.
- Duplicate created a new UUID; saving its independent 300-KIAS scale preserved
  the original 400-KIAS profile byte for byte.
- Native Export wrote JSON equal to the saved original, including all six gauge
  settings. Native Import loaded it as an unsaved copy with a fresh UUID/revision
  zero; saving the named import created a separate record.
- No aircraft limits were fabricated or enabled. No simulator, serial or hardware
  testing is claimed by this desktop-editor workflow.

The UI checks found two desktop preview defects: default Windows bitmap shrinking
dropped thin bright strokes, and the long source label overlapped a VSI numeral.
The editor now selects HALFTONE stretching and sets its brush origin as required
by [Microsoft's bitmap scaling API](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-setstretchbltmode),
and preview faces use the shorter **PREVIEW** label. These changes affect the
host preview only. Final package and test evidence are recorded in the local
November 5 checkpoint.
