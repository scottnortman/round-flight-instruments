# Live gauge timing and installer candidate — 2026-11-03

Branch: `bugfix/live-gauge-timing`, based on memory-layout commit
`96111b458c108cb16897020ba4f9838d8ea8a776`. This is a private development
candidate, not a published release. Main remains at `v0.2.0-rc.5`.

**Later qualification finding:** the additional
[MSFS-path host stress matrix](MSFS-HOST-STRESS.md) found an intermittent
Attitude freshness expiry with jittered SDK replies and repeated Attitude/Turn
expiry under CPU contention. The passing steady synthetic tests below remain
valid for their tested conditions; they do not qualify this broader envelope.
The installer/firmware candidate has not been changed or promoted by those tests.
Subsequent firmware work implements the user's request for a bounded last-frame
hold; see [continuity verification](CONTINUITY.md) for the current candidate.

## Report and reproduction

The user reported working altitude but motion-dependent Attitude flicker,
unresponsive navigation, and intermittent Turn stale indications with the
previous tray installer on MSFS 2020. The supplied video clips show menu
brightness changes, Turn stale indications and dark frames.

A real UART/board reproduction used continuously valid T2 observations aged
35 ms, animated pitch/bank, and no diagnostic queries during motion. It produced
59 numeric-expiry ticks in approximately five seconds despite 31.86 completed
FPS. Removing the motion reduced the failure. Average FPS alone was therefore
an inadequate acceptance check. Timing probes observed frames retiring at up
to 143 ms with only 8 ms of visible lifetime left before the next scanout.

## Causes and changes

* The multi-instrument connector pinned a receiver-clock reference to the last
  ACK while computing observation ages at the current host time. This charged
  elapsed time twice. A new regression first failed with a 35 ms observation
  becoming 76 ms old. The corrected reference advances conservatively between
  ACKs; original observation timestamps still age normally. Receiver bounds
  and future-data rejection remain enforced. Legacy T1 is unchanged.
* Changing the Attitude source caption rebuilt its entire backdrop/foreground
  cache, taking over 100 ms. A bounded caption-only update preserves geometry
  and cached runs; other dimensions or longer labels use the existing path.
* Navigation previously retained live flight indications behind its overlay,
  tying menu visibility and center-hold progress to short numeric leases. It is
  now an opaque non-numeric view. Source data continues aging in the background.
  Unseen buttons cannot select; fresh release and acknowledged-generation rules
  remain. Owner, drawing and scanout health guards still apply.
* The user explicitly approved a **200 ms external-only trial** for pitch,
  bank, turn rate and slip ball. Measured sample ages are retained. Local
  simulation remains at 100 ms; IMU hold policies are unchanged. True stale,
  missing, disconnected and failed data still invalidate the flight view.

Rejected rendering experiments (a different dirty mask and full horizon redraw)
were reverted. CPU, PSRAM and LCD clocks, RGB buffer ownership and SDK driver
configuration are unchanged. No runtime debug logging was added to the scanout
path. Bounded CLI counters expose capture/submit/retire ages and minimum
remaining visible lifetime; stopped statistics freeze their timing totals.

## Results

| Native connector, 35 ms-old observations | Completed FPS, 20 s | Added valid-source expiry |
| --- | ---: | ---: |
| Altimeter | 32.31 | 0 |
| Attitude | 32.13 | 0 |
| Turn | 32.29 | 0 |

Attitude's final 20-second run had a 32 ms maximum draw time, 47 ms maximum
transfer time, and 50 ms minimum visible margin. A repeat using the committed
`check-live-link.py` script for 10 seconds per gauge measured
32.36 / 32.15 / 32.29 FPS, with zero added valid-source expiry.

A separate native Attitude stream accepted 29,129 frames over 600.02 seconds
(48.55 Hz), with maximum ACK 32 ms and maximum sample gap 62 ms. Including the
subsequent idle period, the display completed 27,740 presentations over
860.405 seconds (32.24 FPS), with zero numeric expiry or backlight blanking,
maximum draw/transfer 32/45 ms, and minimum visible margin 34 ms. The 860-second
display measurement is not an isolated 600-second FPS measurement.

The user subsequently confirmed: **all three gauges and the menu are steady and
responsive** during the delayed native TEST INPUT exercise. This is physical
bench acceptance, not live MSFS cockpit acceptance.

Software validation: 198 workspace Rust tests with the MSFS feature; strict
all-target Clippy; five partition/package tests; and three native DLL/worker
recovery tests passed. New regressions cover conservative clock advancement,
numeric-blackout navigation, opaque menu pixels/leases, delayed synthetic
observations, validated server version metadata, and old tray settings migration.
Existing pixel-reference tests exercise cached rendering and multiple buffers.
The final flashed candidate also passed UART display and Attitude regressions
for missing/failed/stale quantities, rendering stall, pre-submission transfer
stall, recovery, gauge changes and local motion with IMU diagnostics running.
Attitude stall/transfer-stall blanking was observed at 141/172 ms by the CLI
polling check; this deliberate fault blanking is distinct from the flicker fix.

## Firmware and memory

Installed ELF SHA256:
`BF19759764425F2A084B75D19D34388D6B7E55C710D7ABCB82B110C4A9D5B837`.
The `memory status` command reports the running image identity directly.

Application image: 1,045,616 / 4,194,304 bytes (24.93%). Partition layout is
unchanged from the memory update. Measured internal data heap total/free/minimum:
411,011 / 111,275 / 79,164 bytes; largest block 38,912 bytes. PSRAM free/minimum:
5,131,000 bytes; largest block 5,111,808 bytes. Drawing stack minimum free:
4,144 bytes. DMA free overlaps internal heap and must not be added to it.

## Installer and simulator identification

Private Windows preview 6 can include the unmodified Microsoft SimConnect client
from installed SDK 0.24.5, with its Microsoft license and notice. This runtime
is selected automatically, including when upgrading the earlier preview; an
explicit user-selected DLL remains possible. The SDK/client version does not
identify the running simulator. Status now records the actual server application
name/build and API version from validated `SIMCONNECT_RECV_OPEN` metadata.
Worker recovery tests cover this metadata after reconnects.

The candidate installer was built and its file installation/removal exercised in
an isolated folder with a unique application ID. The QA wrapper excluded actual
startup, Start menu, tray launch and tray-stop actions, so this is not a new
installed-startup acceptance result. The installer is unsigned. Microsoft
runtime redistribution for a public release remains subject to licensing review.

Target: MSFS 2020 Windows x64. Microsoft documents
[legacy 2020 client compatibility in MSFS 2024](https://docs.flightsimulator.com/msfs2024/retail/programming-apis/simconnect/simconnect-sdk/),
but this connector has not been validated against 2024 or every aircraft/version.
See the [host setup and retest guide](../docs/WINDOWS-DRIVER-SETUP.md).

## Repeatable bench check and remaining acceptance

Close the tray/terminal, use a Python environment with pyserial, and enroll this
board with the native connector. Then run against the newly built connector:

```powershell
python scripts/check-live-link.py --config device.json --seconds 20 --report artifacts/live-link.json
```

The script injects 35 ms of actual observation age through the native transport,
requires at least 30 completed FPS and zero added valid-source expiry on all
three gauges, retains timing counters, and restores explicit local level
Altimeter at 29.92. It never enables simulator writeback. A stopped stream is
intentionally disconnected; an invalid-data indication afterward is expected.

Live simulator-PC acceptance remains open: cockpit signs/units and accuracy,
motion-dependent flicker, center hold/menu dwell and switching under load,
aircraft changes, USB reconnection, active-only pressure Apply/readback and
read-only mode. The installer does not flash firmware; this development board
has already been updated. No public release, merge or push is claimed here.

Raw traces, rejected trials, video extracts and package artifacts are retained
only in the ignored `artifacts/live-gauge-timing/` area. User media is not part
of the source snapshot or installer.
