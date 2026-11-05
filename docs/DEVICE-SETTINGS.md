# Device identity and saved gauge selection

Source checkpoint: **v0.2.0-rc.10**, developed on `feature/device-identity-settings`
from `v0.2.0-rc.9`. This feature does not yet change the Windows installer or add
multi-device enrollment.

The candidate is installed and user-checked. See [verification and resource
cost](../verification/DEVICE-SETTINGS.md) for exact image hashes, persistence,
touch and 30+ FPS results.

## Using the board

Select a gauge through the center-hold menu or `instrument NAME`. After the
choice remains unchanged for one second, the board saves it automatically.
After a restart or normal firmware update, that gauge is selected again.
Power removal before the save completes can restore the earlier selection.

Hold the center for three seconds, release, and tap **INFO**. The information
page shows the firmware's Git revision and the board's full UUID, on two lines.
**BACK** returns to gauge selection. INFO and BACK have the same blue pressed
feedback, release-to-activate behavior and single chirp as the other buttons.
The usual inactivity timeout still applies. `-dirty` in the revision identifies
an image built with uncommitted source changes; it is not a released binary.

UART remains 921600 baud, 8N1, no flow control:

```text
device info
device uuid
settings status
instrument attitude
settings save
settings status
```

`settings save` queues an immediate save of a changed selection. Wait until
`settings status` reports `dirty=false` and the expected `saved=...` before
unplugging. A queued response does not claim that flash has already been written.
Unchanged selections do not cause writes. A brief `ERR settings_busy` means the
bounded metadata lock was occupied; retry the command after a short delay.
No flash operation blocks the state owner. Settings commands are platform
operations; the desktop CLI reports `device_unsupported_target` rather than
inventing a board identity or persistent storage.

## UUID provisioning

The first boot without an identity creates a random RFC-variant UUID version 4
and commits/read-verifies it before starting the display. It identifies the
**physical board**, independently of which gauge is displayed. It is a serial
identifier, not an authentication credential. It does not change the CH343 USB
serial number or Windows COM-port assignment. Existing driver enrollment still
uses the USB serial identity; UUID-aware multi-device enrollment is future work.

Explicit reassignment is available for provisioning:

```text
source sim
device uuid set 11223344-5566-4788-9aab-ccddeeff0011
settings status
device info
```

Use a newly generated UUID for each real board; the UUID above is an example.
Accepts canonical 36-character RFC-variant UUIDs with versions 1 through 8;
rejects nil IDs, invalid variants and malformed strings. The reply reports
`identity_persisted=false` while queued. Wait for `uuid_pending=false`,
`error=0`, and the new UUID in `device info`. The old identity remains published
until the new record is committed and read-verified. Reassignment is rejected
while an external simulator or IMU source is active. Future host enrollment
must detect duplicate UUIDs and require re-enrollment after reassignment.

## Storage and display behavior

The existing 24 KiB NVS partition and application partition addresses are
unchanged. Namespace `rfi_device` contains independent `uuid` and `selection`
records, each with a magic, schema version and CRC32. Preference wire IDs are
explicit and do not depend on Rust enum ordering. NVS replaces values directly;
the adapter never erases the old key before setting a replacement.

A missing preference uses Altimeter. A corrupt preference uses that default
and can be replaced by a subsequent save. A newer unsupported preference schema
is preserved read-only. A corrupt identity is reported as unavailable and is
**not** silently regenerated; use explicit provisioning after investigation.
Initialization errors, full partitions and newer NVS formats never cause an
automatic partition erase. Other namespaces, including factory data, are
preserved. Erasing all flash or the NVS partition removes these settings; an
ordinary application flash preserves them. Do not clone a provisioned full-flash
image onto multiple boards without explicitly assigning unique identities.

A low-priority worker owns all flash operations. The application owner only
observes changes and queues commands using bounded state; it does not wait for
flash. Changes coalesce for one second, failed preference writes retry at most
once per five seconds, and unchanged values are skipped. `settings status`
reports pending state, write/failure counts and maximum write duration.

The RGB display uses PSRAM bounce buffers. ESP-IDF explicitly states that this
mode cannot operate while the external cache is disabled by ordinary NVS
writes. This build enables `CONFIG_SPIRAM_XIP_FROM_PSRAM` (instruction fetch and
read-only data in PSRAM), keeping the external cache active during flash writes.
Build checks verify all three generated options. See [Espressif's RGB LCD
documentation](https://docs.espressif.com/projects/esp-idf/en/v5.5.2/esp32s3/api-reference/peripherals/lcd/rgb_lcd.html#bounce-buffer-with-single-psram-frame-buffer).
NVS provides wear leveling and interrupted-write recovery, with the newest
in-flight value potentially lost on power removal. See [Espressif's NVS
documentation](https://docs.espressif.com/projects/esp-idf/en/v5.5.2/esp32s3/api-reference/storage/nvs_flash.html).

Only identity and selected gauge are persisted in this feature. Live readings,
simulator sessions, sample ages, IMU estimates, barometer values, pending
commands and fault-injection controls are never restored from flash. Existing
stale-data limits, display holds and watchdogs remain in force.

## Recommended aircraft configuration architecture (next feature)

Extend the existing tray application with a **Configure** window; a separate
service or replacement application is unnecessary. Keep three distinct models:

| Model | Authoritative location | Contents |
| --- | --- | --- |
| Physical device | Board NVS; host enrollment references UUID | UUID, selected/startup gauge; later name and display preferences |
| Aircraft profile | Host PC | Profile UUID, schema/revision, simulator/provider, exact aircraft package and variant, verified limits and units for all six instruments |
| Active configuration | Validated host-to-board transaction | Resolved profile and selected instrument parameters for the current aircraft/session |

Use one aircraft profile containing all gauge settings, rather than six
independent aircraft definitions. Configure airspeed scale and aircraft-specific
arcs/limits, VSI range, heading reference and applicable display units through
typed validated parameters. Preserve the existing canonical measurement units;
formatting choices must not silently change the telemetry contract. Existing
portable Airspeed and VSI profile types provide validation boundaries, but
host profile editing and profile transport are **not implemented yet**.

The configuration window should list boards by UUID and friendly panel name,
provide an Identify action and gauge assignment, and edit aircraft profiles.
Match exact simulator/package/variant identity automatically after initial
assignment, with a visible manual override when needed. Do not infer certified
limits from a broad aircraft name or silently reuse the previous aircraft's
profile. Unknown aircraft should retain numeric instrument operation where its
scale is applicable, hide unverified colored arcs, and report missing limits.

Apply a complete validated profile atomically, with schema/revision, CRC/hash,
session association and acknowledgement. Partial or incompatible updates must
not leave a mixture of old and new scales. Profile delivery is infrequent and
separate from the 30+ FPS data stream. A cached profile may aid standalone use
later, but it cannot establish current aircraft identity or make old readings
live. Only explicit actions on a displayed gauge may affect the aircraft;
profile selection and reconnect must never write barometer or other controls.
