# Device settings and INFO verification

Date: November 4, 2026. Branch: `feature/device-identity-settings`, based on
`v0.2.0-rc.9` / `0cb87b28`, prepared as source checkpoint **v0.2.0-rc.10**.
The verified image is installed on the original SKU 29086 board. Source tagging
does not replace that tested image; no new installer or GitHub release is published.

Final ELF SHA256:
`BF4A20C11BF5BD2B28E6C7B3963D6884E73B93F9CE1EEAF57EF458D586DB17A9`.
INFO/CLI revision: `v0.2.0-rc.9-0-g0cb87b28-dirty`. INFO renders letters in
uppercase because the instrument font has uppercase glyphs. UUID spelling is
case-insensitive; the CLI uses lowercase canonical text.

## Results

- **225 workspace/all-target Rust tests pass**, plus strict all-target Clippy.
  Coverage includes UUID validation/canonical formatting, all-six record
  round trips, single-byte corruption and truncation, unsupported schemas,
  debounce/backoff and stale-save completion, startup without old live data,
  connected-source UUID reassignment rejection, acknowledged release actions,
  drag cancellation and inactive INFO areas. Cached and uncached INFO/menu
  pixels match across all six faces, page changes, UUID changes and pressed states.
- Hardware final-build persistence test: **eight restarts pass**, restoring all
  six choices and a temporarily reassigned UUID; the original UUID is restored.
  Reissuing an unchanged choice and Save causes no writes. An unchanged flush
  cannot force a future choice, and rapid changes coalesce to one final save.
  No old numeric readings or source sessions are restored.
- Normal firmware reflashing preserves the saved Heading choice and UUID.
  `pre-reflash.json` / `post-reflash.json` capture the comparison.
- Storage stress: **350 committed saves** on the first candidate, then **60 on
  the final build**, cycling all six faces. No reported storage failure, display
  blank or owner timing fault. Completed FPS: **30.46 / 30.31**, respectively.
  Final maximum measured NVS operation was 59 ms. The first candidate measured
  67 ms after extended writes; it preceded the build-revision and flush-edge
  refinements and used the same flash/cache protection and NVS adapter.
- Final-build **50 rapid all-six CLI switches pass at 31.65 completed FPS**,
  with no additional raw deadline ticks, held slots or blanking.
- Released rc.7 Windows connector, real UART/LCD and synthetic all-six telemetry
  with 35 ms acquisition age: Attitude **32.26 FPS**, Turn **32.29 FPS**. No
  added blank timestamp. This checks transport compatibility; it is not a new
  live-MSFS aircraft qualification.
- User physically confirmed that INFO, Back, pressed feedback/chirps and gauge
  switching work correctly. The board was left on the user's selected
  **Airspeed**, saved with `dirty=false`, rather than resetting their choice.

Final-build normal local motion (five-second completed-presentation windows):

| Gauge | FPS |
| --- | ---: |
| Altimeter | 32.45 |
| Attitude | 32.26 |
| Turn Coordinator | 32.32 |
| Airspeed | 32.34 |
| Heading | 32.32 |
| Vertical Speed | 32.25 |

## Resource and storage checks

App image: **1,130,400 / 4,194,304 bytes (26.95%)**; 3,063,904 bytes remain in
the app slot. Partition addresses and size are unchanged; table SHA256 remains
`FD8026BFF850CA0DEE41C41305160317FFFE604DDA30A9BD5A701AC82D96FA17`.

PSRAM XIP reserves approximately 1.06 MB for instructions/read-only data.
Physical PSRAM remains 8 MiB; allocator-reported PSRAM total is 7,331,272 B,
free 2,672,004 B, largest block 2,555,904 B. Internal free is 77,799 B, minimum
45,596 B and largest block 31,744 B. Minimum unused display stack is 4,620 B.
The low-priority settings worker has an 8 KiB stack. INFO and gauge selection
share two compact page regions within the existing menu bitmap; no new full-size
bitmap, renderer or scanout buffer is allocated.

Build checks verify `CONFIG_SPIRAM_XIP_FROM_PSRAM`,
`CONFIG_SPIRAM_FETCH_INSTRUCTIONS` and `CONFIG_SPIRAM_RODATA`. This prevents
ordinary NVS writes disabling the cache needed by RGB bounce scanout. Existing
clock, freshness, hold and watchdog policies are unchanged.

Pre-feature 24 KiB NVS backup SHA256:
`1DF8949B2E345AB8C00CB81FB6B83686E20A4080F969E5CD8B8D520A07CDABA2`.
Stored locally with the evidence; the original full factory backup is retained.

## Evidence and limits

Ignored local directory: `artifacts/device-settings-20261104/`.

- `tests.log`, `clippy.log`, `firmware/`: final tests and exact build artifacts.
- `persistence.json`: 350-write trial; `persistence-final.json`: final-build
  restarts, debounce and 60-write trial. The earlier `persistence-first.json`
  records a test-client stop on the deliberately nonblocking `settings_busy`
  reply; the client now retries it. It was not a flash or board failure.
- `motion.json`, `native.json`, `switches.json`: final performance checks.
- `visual-confirmed.json`: final installed image, saved choice and memory.
- `nvs-before.bin`: pre-feature storage backup; no device-specific records are
  committed to the public repository.

Reproduce persistence checks with the IDF Python environment:

```powershell
python scripts/check-device-settings.py --port COM5 --stress-writes 350 --report artifacts/settings-test.json
```

This changes selection, performs hardware restarts and briefly reassigns then
restores UUID. It finishes on saved Altimeter. It does not test cutting power
during an active write; record rejection is tested on the host and interrupted
flash recovery relies on ESP-IDF NVS. The helper also checks monotonic uptime
and exact save counts during stress.

Aircraft-profile editing/delivery and UUID-aware multi-device enrollment remain
future work. The existing installer continues to enroll one board by USB serial.
See [behavior and architecture](../docs/DEVICE-SETTINGS.md).
