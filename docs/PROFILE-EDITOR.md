# Live delivery in test.13

Use **Use on connected board** to save and manually activate a profile. The tray
app reconnects, delivers a single validated session-bound configuration, and
reports the acknowledged name/revision in Status. Save changes to an active
profile to deliver its new revision. **Use defaults** removes the selection.
Read-only still permits display configuration; neither action writes simulator
controls. Preview samples stay local. Match aircraft manually; there is no
automatic aircraft matching. See [current test instructions](MSFS-TEST-13.md).

The historical milestone notes below describe test.12's preview-only behavior.

# Aircraft profile editor

Feature branch: `feature/aircraft-profile-editor`, based on `v0.2.0-rc.10`.
This first milestone creates, validates, previews, saves, duplicates, imports
and exports profiles locally. Live device delivery and automatic aircraft
matching are later milestones. Firmware and the installed connector are unchanged.

## Open the editor

For the local preview build, open **`instrument-profile-editor.exe`** in
`artifacts/profile-editor-ui-20261105/preview-final/`. It is a standalone native Windows
application and starts no serial connection or simulator process. No Python,
MSFS installation or SimConnect DLL is needed for editing.
The packaged standalone build statically links the C runtime so it also avoids
a separate Visual C++ runtime installation.

The test.12 tray application has **Aircraft profiles (preview only)...** in its
menu and includes the standalone editor. The published rc.7 installer lacks this
menu. Profile delivery to the live board remains a later milestone.

Normal profiles are stored under:

```text
%LOCALAPPDATA%\RoundFlightInstruments\profiles
```

For an isolated developer/test directory:

```powershell
instrument-profile-editor.exe --profile-dir D:\some\profile-folder
```

## Create and preview a profile

1. Choose **New** and enter a profile name.
2. Choose the simulator, then enter the exact aircraft package/add-on and
   variant identifiers. These are manually supplied matching keys in this
   milestone; the editor does not discover or verify the installed aircraft.
   MSFS 2024 and DCS entries support profile authoring only, not a new live adapter.
3. Set the **Airspeed full scale**, from 40 to 1,000 KIAS in steps of 10.
   **Prototype layout** scales the phone-tested nonlinear dial; **Linear layout**
   distributes the selected range uniformly across the same 342-degree sweep.
4. Leave colored arcs disabled until you have the limits for that exact variant.
   To enable them, check **Use verified aircraft airspeed limits**, supply Vs0,
   Vs1, Vfe, Vno and Vne in **KIAS**, and enter the manual/page/variant reference.
   The editor validates ordering and scale coverage; it does not independently
   verify the accuracy of the source. No generic aircraft limits are pre-filled.
5. Set the **VSI full scale**, from ±500 to ±10,000 ft/min in steps of 100.
6. Choose any of the eight screens under **Preview gauge**, including Combined and DEBUG.
   Combined controls pitch/roll with fixed samples for the other readings. Edit the sample values
   and choose **Update preview**. Changed settings/sample values mark the preview
   as needing an update. Successful Save also refreshes a valid preview.
7. Choose **Save profile**. The saved revision increases, and the profile appears
   in the list. Reopening restores its saved fields. An asterisk in the title
   indicates unsaved profile edits; changing only preview samples does not alter
   the stored profile.

Example sample labels describe the actual semantics: positive pitch is nose up,
positive roll is right bank, positive VSI is climb, and slip is independent of
turn rate. Altimeter preview uses pressure altitude plus the existing local
barometric correction model. It does not replace live simulator indicated altitude.

All six instruments have typed settings in the profile. The editable instrument
settings in this milestone are airspeed scale/limits and VSI range. The other
modes remain the ones implemented by the existing faces:

| Instrument | Current mode |
| --- | --- |
| Airspeed | Indicated knots (KIAS) |
| Altimeter | Feet and inHg |
| Attitude | Pitch/roll degrees |
| Heading | Magnetic degrees |
| Turn coordinator | 3°/second standard rate and normalized slip ball |
| VSI | Feet/minute, with configurable range |

Unknown units or unsupported references are rejected on import. New modes need
matching data and renderer support before becoming selectable. Overrange sample
values retain the existing explicit overrange behavior, without a pegged needle.

## Sharing and keeping edits

- **Duplicate** creates an unsaved copy with its own profile UUID.
- **Export…** writes the current valid form as JSON. Choose a destination outside
  the managed profiles folder; export cannot bypass saving/revision checks by
  overwriting a managed record. The file dialog confirms ordinary file replacement.
- **Import…** validates the JSON and opens an unsaved copy with a new profile UUID.
  Review it and Save. Existing profiles are not overwritten by matching import IDs.
- New, Import, switching profiles and Close offer Save / Discard / Cancel when
  profile edits are pending. Invalid edits stay in the form when saving fails.
- If a file changes outside the editor, Save refuses to overwrite it. Close and
  reopen to load the external change, or Duplicate to keep your edited version.

A **profile UUID identifies an aircraft configuration**. The board's UUID
identifies physical hardware. Neither is the other, and importing a profile does
not change a board's identity or selected gauge.

## Storage and architecture

`aircraft-profiles` owns the host schema, validation, deterministic previews and
bounded Windows storage. `profile_window` is the native UI/FFI boundary. It has
no connection to the tray Supervisor, USB transport or simulator controls. The
standalone executable excludes those supervisor modules entirely. The tray's
connector continues in its independently supervised process while editing.

Each JSON file has schema version 1, a profile UUID, a monotonically increasing
revision, exact simulator/package/variant identity and all six settings. Reads
are limited to 64 KiB and the catalog to 256 JSON files. Unknown fields/versions,
malformed data, nonfinite values, invalid UUIDs and unsupported units are rejected.
UTF-8 BOM files are accepted and UUID spelling is normalized. Corrupt catalog
files are reported and retained rather than erased.

Writes use exclusive Windows file sharing, compare the complete previously
loaded record, stage a complete file, sync it and atomically replace the saved
record. A stale lock file cannot strand the editor: ownership belongs to the
open handle, which Windows releases on process exit. Links/reparse points are
rejected. The schema and preview are portable Rust; the storage lock and GUI
currently target Windows.

Preview uses the portable instrument models and original needle/face artwork.
Default airspeed/VSI previews are pixel-checked against existing rendering, and
custom ranges actually change scale labels and needle mapping. This adds a
separate preview API; it does not change the firmware's cached rendering path.
The desktop image is labeled **PREVIEW** and uses halftone scaling to retain
thin ticks and lettering when reduced to fit the window.

Next: bind profiles to authoritative aircraft identities, add validated atomic
delivery associated with the current simulator session, then UUID-aware device
assignments and multi-device management. Saving a profile must remain separate
from any action that changes an aircraft control.

See [verification](../verification/PROFILE-EDITOR.md), [device settings](DEVICE-SETTINGS.md)
and [Windows host setup](WINDOWS-DRIVER-SETUP.md).

## Build a review package

With the Windows Rust toolchain used by the existing host applications:

```powershell
.\scripts\Build-ProfileEditor.ps1
```

This builds only the standalone editor and packages it with the project/font
notices, unchanged dependency source archives and an exact source snapshot.
It requires no ESP-IDF or MSFS SDK and creates no installer or startup entry.
The supplied manifest records the source checkpoint and file hashes.
The explicit Windows target and `+crt-static` flag follow the [Rust runtime
linkage guidance](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes).
