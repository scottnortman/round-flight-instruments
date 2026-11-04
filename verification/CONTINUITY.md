# Bounded external presentation continuity — 2026-11-03

Source checkpoint `v0.2.0-rc.6` on branch `bugfix/live-gauge-timing`. Implements the
user's request to make brief interruptions invisible by retaining completed
pixels. This succeeds the failing [host stress baseline](MSFS-HOST-STRESS.md).
Live MSFS-PC acceptance remains separate from local bench testing.

The original candidate below passed automated timing tests but failed the
physical touch check. The subsequent menu/scanout investigation and normal
build are recorded at the end of this report; the old image is historical.

## Policy and invariants

The 200 ms external Pitch/Roll/Turn/Ball validity limits are unchanged. The UI
adds 300 ms of visual grace; a displayed sample cannot exceed 500 ms of real
age. A repeated frame preserves its original metadata and absolute deadline.
Repeating stops with at least 100 ms reserved to compose and present the
non-numeric invalid-data view, adjusted upward with a slower pixel clock.
Fresh data resumes naturally.

Only external, running, fully known Attitude/Turn poses are eligible. No reuse
across a different session, gauge, non-running lifecycle, menu, explicit failure
or missing quantity. Local simulation, the IMU policy and Altimeter are unchanged.
No old sample is fed into the application state or aircraft-control path.

The CPU copies the immutable, completed front into the disjoint spare using the
existing bounded three-buffer pipeline. It invalidates the spare's damage history
before later drawing resumes. ISR completion counters still determine ownership;
the original visibility deadline is passed through every repeat without renewal.
Owner/render/scanout guards remain active. No new bitmap allocation or scheduler,
LCD, CPU or PSRAM clock adjustment is needed.

CLI telemetry exposes actual age, original session/sequence, `quality=held` and
`holding=true`. No visual hold label is added. Counters distinguish exact pixel
repeats, other grace presentations, and actual invalid-data warning frames.

## Regression seams

The policy regression first failed with 100 ms remaining lifetime where the new
extended presentation lifetime was required. After implementation it verifies that
data becomes Stale at its original deadline and repeated pixels cannot extend
their hold by being resubmitted. Context/failure tests forbid reuse on source,
gauge, lifecycle or field failure changes and while navigation is open.

A rotating three-buffer pixel test copies completed frames, resumes with a new
pose in the same buffer, and compares the entire image against full reference
rendering for Attitude and Turn. No leftover graphics are permitted.

201 workspace Rust tests with the MSFS feature and strict all-target Clippy
passed. The firmware build checks the partition, watchdog, I2C and clock settings.
Physical appearance and live simulator mapping still require their own acceptance.

## Original hold candidate (superseded)

ELF SHA256 `6D1EF6310A8DA5DF6B2EB8C815859509B7102F70BF4F6A7634D2EA990D1113E2`.
Application 1,050,704 / 4,194,304 bytes (25.05%). The generated partition hash
remains `FD8026BFF850CA0DEE41C41305160317FFFE604DDA30A9BD5A701AC82D96FA17`.
Artifacts and exact images are retained under ignored
`artifacts/continuity-20261103/`.

The existing preview 6 host executable and Microsoft runtime are unchanged and
remain compatible with this firmware. No new installer, public release, merge
or source commit is claimed by this candidate.

## Original hold-candidate hardware matrix

The final matrix uses the exact preview 6 MSFS-provider executable, test-only
SDK DLL, isolated worker, native decoder, real UART and this firmware. Source
reply jitter and CPU-affinity contention match the failed baseline. Positive
cases require at least 30 completed FPS, zero added numeric expiry, no backlight
blanking and zero physical invalid-data warning frames. Short-gap cases must
also exercise actual completed-frame copies. Negative cases test long data gaps,
explicit instrument failures and two aircraft changes, with confirmed recovery
and no unsolicited simulator writes. Final results:

| Final candidate case | Completed FPS | Repeated frames | Grace frames | Warning frames | Dark flashes |
| --- | ---: | ---: | ---: | ---: | ---: |
| Attitude, normal jitter, 20 s | 31.87 | 0 | 0 | 0 | 0 |
| Turn, normal jitter, 20 s | 32.34 | 0 | 0 | 0 | 0 |
| Attitude, two-CPU contention, 20 s | 32.06 | 1 | 190 | 0 | 0 |
| Turn, two-CPU contention, 20 s | 32.22 | 2 | 73 | 0 | 0 |
| Attitude, 250 ms reply gap, 15 s | 31.77 | 5 | 7 | 0 | 0 |
| Turn, 250 ms reply gap, 15 s | 32.28 | 6 | 7 | 0 | 0 |
| Attitude, 800 ms reply gap, 15 s | 31.91 | 7 | 10 | 17 (expected) | 0 |
| Turn, 800 ms reply gap, 15 s | 32.24 | 7 | 8 | 17 (expected) | 0 |
| Attitude, two aircraft changes, 15 s | 31.90 | 0 | 2 | 0 | 0 |
| Attitude, explicit instrument failure, 15 s | 31.90 | 0 | 0 | 32 (expected) | 0 |
| Turn, explicit instrument failure, 15 s | 32.32 | 0 | 0 | 33 (expected) | 0 |

All cases passed with this exact installed image, the existing preview 6
connector, normal process priority and unchanged 10–40 ms SDK reply jitter.
The short-gap cases require actual pixel repeats, no warning frames and no
backlight change. Long gaps/failures require real invalidation and confirmed
valid presentation afterward. Aircraft changes recovered through three distinct
sessions. No fixture simulator-write function was called; no resets, owner
timing fault, unexpected expiry or display-worker failure was observed.

The source can still deliver fewer than 30 new observations/second under CPU
contention. The table counts completed presentations, including holds; it does
not claim source latency disappeared or that held readings remain valid data.

Local memory/motion regressions on the final image: Altimeter **32.30 FPS**,
Attitude **32.45 FPS**, Turn **32.34 FPS**, with unchanged heap usage and no
added expiry/blanking. Final UART tests passed missing/stale/failed values,
rendering stall and pre-submission transfer stall, recovery, local animation
with IMU diagnostics, and freezing all stopped timing/hold/warning totals.
Rendering/transfer stalls intentionally exercised the independent backlight
guard. The owner watchdog configuration was verified by the build; a new
owner-watchdog panic/reboot injection was not performed in this run.

Measured internal heap total/free/minimum: 411,011 / 111,275 / 79,212 bytes;
largest block 38,912 bytes. PSRAM free/minimum 5,131,000 bytes; largest block
5,111,808 bytes. Display stack minimum free 3,200 bytes. No new full-size bitmap
or per-frame allocation is introduced. Firmware/partition hashes were checked
against the running board and saved images.

Intermediate trials remain in the ignored evidence folder. A 450 ms visual
deadline with a 100 ms handoff reserve exposed one premature Turn warning; a
smaller 64 ms reserve removed that warning but allowed a one-tick dark flash
during sustained Attitude loss. The final 500 ms deadline and at least 100 ms
reserve pass both short-gap invisibility and sustained-loss handoff tests.

Raw final reports: `artifacts/continuity-20261103/half-second-check/report.json`,
`final-stress/report.json`, `local-memory-regression.json`, `display-guards.json`
and `attitude-guards.json`. Physical visual acceptance of this new hold build
and a live MSFS-PC retest remained pending at this stage. Subsequent findings:

## Physical menu follow-up

The initial human check found Attitude tearing during the center hold and an
unresponsive menu. Early probes showed that the controller retained its open
menu, while input gaps discarded selection gestures. During an open menu,
touch polling fell to roughly 13–26 polls/s, compared with over 100 while the
gauge was visible. Successful I2C transactions reported no bus errors.

Two independent changes address these measured causes:

- Increase the two RGB bounce buffers from 16 to 24 lines. A temporary counter
  in the SDK's existing incomplete-frame recovery branch recorded five recovery
  events in the 60-second 16-line probe. Both 24-line visual probes recorded zero.
  This changes buffer capacity, not clock speed or recovery policy, and costs
  15 KiB of internal DMA RAM.
- Retain the opaque menu separately in each framebuffer. A static menu now
  performs no pixel writes when new simulator observations arrive; changed
  highlights redraw the panel, and closing restores the whole selected face.
  Previously, every menu frame filled the full screen and recopied its panel.
  The repeat physical capture sustained about 160 touch polls/s while the menu
  was open, with zero stale-input gesture cancellations and ten menu openings.

Navigation also preserves its open view during temporary input/pipeline gaps,
discarding the gesture and requiring a fresh release. It still closes on context
change, explicit Cancel/selection or inactivity. Pressure editing remains
separate and fail-closed. Pixel tests cover static menus through source updates,
all three rotating buffers, highlight changes, closing and gauge restoration.

The 205-second retained-menu capture completed at **32.05 FPS**, with no RGB
recoveries, expiry or recorded backlight blanking. It included periodic 250 ms
SDK reply gaps. One invalid-data warning frame was counted; this capture does
not establish zero warnings for every possible menu/source transition. The user
confirmed that menu presses and selections were working. They also reported a
freeze, sustained beep and reset; the capture did not record a reboot, UART break
or backward board clock, so that report remains unconfirmed and is not dismissed.
The pressure editor was intentionally disabled in this read-only external test.

Evidence: `artifacts/continuity-20261103/touch-diagnostic/`, including failed
traces, probe provenance, the retained-menu trace and its exact diagnostic ELF.
The shared SDK file was restored byte-for-byte to SHA256
`31f0b6053c21a416f546f2fe84a287fe8c46d0baba149f99cf987586a9985a14`.
The temporary getter/counter was removed and the SDK LCD component rebuilt.

## Normal build after the menu fix

ELF SHA256 `EDBDDEEA394D85DF810FB1B84D3DB2D2E214D81A6DEA99CDB71C15656047C5B5`.
Application **1,051,744 / 4,194,304 bytes (25.08%)**. The partition hash is unchanged.
Saved images and hashes: `artifacts/continuity-20261103/normal-firmware/`.
The ELF contains no temporary RGB probe symbol. All **203 workspace Rust tests**
and strict all-target Clippy pass.

The normal build passed **50 rapid switches** across Altimeter, Attitude and
Turn at **31.36 completed FPS**, with zero added expiry or blanking. Five-second
steady local runs measured **32.27 / 32.32 / 32.42 FPS**, respectively, with stable
internal/PSRAM free heaps. Measured internal total/free/minimum/largest block:
411,011 / 95,811 / 63,700 / 31,744 bytes; PSRAM free 5,131,000 bytes.
Additional normal-build stress and physical checks follow.

The complete normal-build transport matrix passed all 13 cases. This uses the
actual native MSFS provider/worker/decoder with a test-only SDK DLL, real UART
and the installed normal firmware. It does not replace live MSFS-PC acceptance.

| Normal-build case | Completed FPS | Warning frames |
| --- | ---: | ---: |
| normal-altimeter | 32.34 | 0 |
| normal-attitude | 31.96 | 0 |
| normal-turn | 32.34 | 0 |
| loaded-altimeter | 32.31 | 0 |
| loaded-attitude | 32.03 | 0 |
| loaded-turn | 32.30 | 0 |
| attitude-switch | 31.94 | 0 |
| attitude-short_gap | 31.92 | 0 |
| turn-short_gap | 32.31 | 0 |
| attitude-gap | 31.94 | 17 |
| turn-gap | 32.28 | 16 |
| attitude-failed | 31.90 | 33 |
| turn-failed | 32.34 | 33 |

All normal, CPU-contended and short-gap cases had zero warning frames,
valid-source expiry or blanking. Both short-gap cases exercised completed-frame
reuse. The 800 ms gaps and explicit failures produced warnings as required,
with confirmed valid presentation after recovery. Two simulated aircraft changes
recovered through three sessions. No fixture simulator-write API was called.
Raw report: `artifacts/continuity-20261103/normal-stress/report.json`.


Final normal-build UART guard regressions passed missing/stale/failed readings,
rendering and pre-submission transfer stalls, recovery, and Attitude motion with
IMU diagnostics enabled. Only the deliberate stalls blanked the backlight.
Reports: `normal-display-guards.json` and `normal-attitude-guards.json`.

A final **100-second monitored local touch check** passed at **31.38 completed
FPS**, with five menu openings, no added expiry or blanking, no owner timing
fault, and zero touch/buzzer errors. The buzzer was inactive at completion;
board tick counts remained monotonic. The user explicitly confirmed that the
pressure popup, +/Cancel, menu selection, blue feedback and short chirps worked
normally, without a freeze, sustained beep or reset. This confirms the current
normal build's behavior; the earlier reported reset was not reproduced or
assigned an unsupported cause. Trace: `normal-touch/observations.jsonl`.

Final display stack minimum free: **3,040 bytes**. Heap usage stayed unchanged
through touch testing. The board is left on local, level Attitude with IMU
acquisition off, QNH 29.92 and COM5 released. This source checkpoint remains on
`bugfix/live-gauge-timing`; live MSFS-PC acceptance is still required.
