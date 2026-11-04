# Six-pack checkpoint verification

Follow-up: `v0.2.0-rc.9` adds the user-approved
three-miss policy. See [new verification](DISPLAY-HOLD.md). The records and
open-limit section below describe the rc.8 baseline, without rewriting its
failed stress-test results.

November 4, 2026. Branch `feature/complete-six-pack`, based on `v0.2.0-rc.7`.
Airspeed work is carried forward; Heading and VSI complete the six faces.
Accepted source checkpoint: **v0.2.0-rc.8**, merged into `main`. The user
reported "All tested and working" on November 4, 2026. No aircraft/version
matrix was supplied with that acceptance. New prebuilt release assets have not
been published; automated evidence and its limits remain recorded below.

## Installed image and memory

Waveshare SKU 29086, UART COM5. ELF SHA256:
`A3C765A7047F8E76CF8AAD081D6BFA09E7C3E2B2CFA0DC33779D29883CA6F3DF`.

The application is **1,084,992 B / 4,194,304 B (25.87%)**. The verified table,
NVS/PHY offsets, CPU/PSRAM/pixel clocks, 24-line bounce buffers, watchdogs and
freshness limits are unchanged. Exact ELF and separate flash segments are saved
locally in `artifacts/six-pack-20261103/firmware-final/manifest.json`.

There are six retained backgrounds, three shared scanout buffers and one shared
menu bitmap: 4,608,000 B of image PSRAM. Measured PSRAM free is **3,730,156 B**,
largest block 3,670,016 B. Internal heap free is **91,611 B**, minimum 59,580 B,
largest block 31,744 B. The renderer stack is 24 KiB, with 5,680 B minimum unused
in steady motion and 5,328 B in the rapid-switch stress run. No new per-face
internal glyph cache or renderer task is allocated.

## Automated results

`cargo test --workspace --all-targets --locked`: **208 passed**.
`cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
The firmware build verifies generated SDK safety settings and partition bounds;
flashing verifies the expected USB identity and factory backup checksum.

Coverage includes circular heading direction/north wrap, VSI sign and zero,
overrange without a pegged pointer, nonfinite/failed/missing/expired inputs,
quantity-specific ages, strict CLI bounds, mutually exclusive demos, rejected
commands leaving state intact, source ownership, and all-six menu availability.
Three-buffer pixel comparisons exercise moving new faces, faults, center press
feedback, opaque menus and source changes. Batched regular/bold text is compared
against the original pixel algorithm at multiple scales and sizes.

The final ten-second steady-motion windows measured completed presentations:

| Face | FPS |
| --- | ---: |
| Altimeter | 32.36 |
| Attitude | 32.36 |
| Turn Coordinator | 32.33 |
| Airspeed | 32.41 |
| Heading | 32.36 |
| Vertical Speed | 32.33 |

All six windows had stable free heap, unchanged expiry/blank counters and no
display failure. Evidence: `artifacts/six-pack-20261103/qualified-memory.json`.
These counters alone cannot establish absence of visible tearing. The user
subsequently reported all tests working on November 4, 2026.

A continuous T2 UART stream on the final image also passed all six faces:
Altimeter 32.26, Attitude 31.28, Turn 32.32, Airspeed 32.33, Heading 30.80 and
VSI 32.33 completed FPS, with no additional expiry/blanking during each timed
window. Evidence: `final-t2-six-faces.json`. This saturates the text telemetry
path; it is separate from the native connector timing tests below.

Heading initially missed the performance requirement. The retained static
aircraft/captions, precomputed tick directions, contiguous dirty-tile restores,
batched text and 28-pixel rotating lettering meet it without another RAM cache.
The rejected slower builds and their reports are retained locally.

## Actual UART and native connector

`check-heading-vsi.py` passes on the final image. It checks local cardinal
headings, both VSI directions and overrange, rejected values, missing/faulted
inputs and expiry. T2 tests deliberately supply different magnetic/true headings
and omit magnetic heading to prove there is no fallback. VSI stays independent
of changing altitude and barometer inputs. Source age is retained through
presentation. Evidence: `final-heading-vsi.json` in the same artifact directory.

`check-display.py` passes on the final image: zero/negative/high altitude,
missing/failed/stale data, barometric correction, display off/on, deliberate
render and pre-submission transfer stalls, independent backlight blanking and
recovery. Owner ticks continue during the stalls. These intentional negative
tests must blank; their guard counters are not steady-motion failures.

The **released rc.7 native worker/connector** was exercised through a test-only
SimConnect DLL, then actual UART and LCD. Replies are delayed 10–40 ms. CPU-load
cases constrain the connector and two competitors to two logical CPUs. The
fixture records any attempted aircraft write, including when `--writeback` is
enabled: none occurred without an explicit active-gauge action.

The broad run passed normal and loaded cases for Altimeter, Attitude, Turn,
Airspeed and VSI (31.62–32.39 FPS), plus VSI aircraft-switch and failure recovery.
It rejected the earlier 32-pixel Heading font at 28.75/29.26 FPS. After the
Heading layout change, the final image passed **32.15 FPS normal / 32.25 loaded**,
two automatic aircraft reconnects, and explicit Heading failure/recovery. No
fresh-source expiry or unintended write occurred in those passing cases.
See `native-host/report.json` for the broad run, including its rejected Heading
cases, and **`native-heading-final/report.json`** for the passing final Heading
repeat. The broad report is intentionally not relabeled as an entirely passing run.

These are local fixture results, not a live aircraft qualification. See the
[test checklist](../docs/SIX-PACK.md) for the user's next MSFS run.

## Open inherited limit: aggressive local CLI switching

The strict 50-switch local test still catches occasional brief numeric expiry
and guard blanking when directly switching away from local Attitude/Turn. This
is the [previously reproduced baseline limitation](MEMORY-LAYOUT.md#known-rapid-switch-limitation),
under their unchanged 100 ms local deadline. The six-face run's failing trace
is `qualified-switch.json`; subsequent diagnostic traces are also retained.
It is **not** claimed as a passing no-blanking stress test.

Experimental transition/guard changes did not establish a reliable fix and
were removed. The final image keeps the established ownership/fence/guard
logic and real sample timestamps. The touch menu is an opaque non-numeric view;
its open/select/close pixel tests pass for all six faces. The user subsequently
reported all tests working. External MSFS-style steady motion and reconnect
tests above passed; the inherited automated stress-test limit remains open.

## Reproduce

Use the IDF Python environment with pyserial and close other COM clients:

```powershell
python scripts/check-heading-vsi.py --port COM5 --report artifacts/heading-vsi.json
python scripts/check-memory.py --port COM5 --seconds 10 --report artifacts/six-pack-memory.json
python scripts/check-display.py --port COM5 --report artifacts/display-guard.json
```

`check-msfs-host.py` requires the MSFS SDK headers, MSVC and a compatible native
connector. Its test DLL stays separate from Microsoft's runtime. The existing
Windows installer already transports all six quantities; no new installer is
required for this firmware. Browser reference graphics remain separate from
the Rust hardware faces; the VSI browser face is still pending.
