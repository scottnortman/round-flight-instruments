# Bounded hold for late display frames

Source checkpoint: **v0.2.0-rc.9**, merged into `main` from
`bugfix/bounded-display-hold`, based on `v0.2.0-rc.8`.
The default is **three consecutive missed display-frame slots**. A short delay
keeps the last completed reading visible, without a flash or on-screen HOLD
annotation. The **fourth consecutive miss blanks the backlight**.

At the normal 18 MHz pixel clock, a display slot is approximately 32 ms (two RGB
completion intervals). Three permitted slots provide about **96 ms** after the
original numeric presentation deadline. The guard evaluates every 10 ms, so
physical blanking is observed at the fourth slot within that polling resolution.
It counts elapsed frame slots, not individual guard polls.

| Consecutive miss | Action |
| --- | --- |
| 0 | Show current data normally |
| 1, 2, 3 | Keep the last completed reading visible |
| 4 or more | Blank until a usable fresh presentation or nonnumeric warning/menu is available |

## CLI

UART remains **921600 / 8N1 / no flow control**. Close the PC connector before
opening a terminal.

```text
display hold status
display hold 3
display status
display sample
```

`display hold N` accepts **0–10**, is volatile, and defaults to 3 after boot.
Zero restores immediate numeric-expiry blanking for comparison. Changing the
setting does not reset an existing miss run or extend its original frame period.
An increased limit can permit its remaining budget; the counter continues.

`display hold status` reports the configured limit, current consecutive misses,
active hold, nominal frame period, permitted held slots, episode count, maximum
run, deadline-caused blanks, the miss count at the last blank and confirmed fresh
presentations. `display status` includes the main hold fields. `display sample`
reports held data as `quality=held`, with `holding=true` while visible, and its
original source age continues increasing. Raw `expired_ticks` remain available;
they are not erased merely because a brief miss was tolerated.

## Freshness and failure behavior

The count resets only when a fresh numeric presentation **actually reaches the
LCD**, confirmed by the existing two-event presentation fence. Drawing a frame,
receiving new telemetry, copying old pixels or re-submitting an expired reading
does not reset it. A frame must still be within its original measurement lifetime
at that physical confirmation. There is no last-reading fallback before any
usable numeric frame has been confirmed.

Quantity validity, original sample timestamps and existing source freshness
limits are preserved. The existing external Attitude/Turn presentation grace
remains separate. The added policy tolerates a bounded late presentation; it
does not mark an old reading as new or extrapolate instrument values.

Source-error pages continue to render normally. Menus, patterns and wholly
invalid warning views contain no retained flight numbers, so they can remain
visible without clearing the unresolved numeric miss run. A partial warning
view can qualify only for numeric components that are actually valid and shown;
failed components remain removed/flagged. Fresh IMU estimates can clear a run,
but its retained-pose fallback cannot; the existing IMU bench hold is unchanged.

The independent **owner (100 ms), renderer/presentation (250 ms) and scanout
(100 ms)** checks and task watchdogs retain their limits. A genuine pipeline
failure may therefore blank independently of the configured numeric miss budget.
No extra bitmap, rendering task, pixel clock change or per-frame allocation is
introduced. The LCD naturally retains its immutable front buffer during a miss.

## Verification

[Verification record](../verification/DISPLAY-HOLD.md). Automated hardware tests
deliberately stall drawing/transfer, change owner readings during the stall,
check that the old reading and real age remain, verify blanking at X+1, then
require a newly confirmed fresh LCD frame for recovery. The rapid-switch test
records raw deadline misses while requiring no blanking or budget exhaustion.

```powershell
python scripts/check-display-hold.py --port COM5 --report artifacts/display-hold.json
python scripts/check-gauge-switch.py --port COM5 --count 50 --gauges airspeed heading vsi turn attitude altimeter --report artifacts/switch-hold.json
```

The first test intentionally blanks the screen during sustained-failure checks.
Both clients restore safe bench settings; the hold test restores the default 3.
The existing Windows connector needs no update for this firmware policy.
