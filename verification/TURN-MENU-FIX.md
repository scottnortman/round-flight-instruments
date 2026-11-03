# Turn menu cancellation fix — 2026-11-03

The user reported a momentary gauge menu that could not accept a selection.
On Turn, the captured menu lasted 0.260 and 0.127 seconds before cancelling;
numeric-expiry ticks increased, and the backlight briefly went off. Touch-bus
errors stayed zero. This was reproduced on the installed firmware.

Root cause: Turn rendered menu lettering in full every frame instead of using
the existing optional menu cache. Its renderer also skipped incremental cache
preparation because that path was restricted to Attitude. Drawing rose from
roughly 6 ms to 49 ms; numeric expiry made the display unhealthy, correctly
cancelling touch navigation. The navigation controller was not spontaneously
changing gauges or treating the held finger as a selection.

The renderer now prepares supplied menu artwork for every face and uses the
cached navigation layer after either static-face or attitude drawing. Turn has
its own CPU-only 450 KiB PSRAM menu bitmap, like Attitude. It is never submitted
to DMA. Numeric lifetimes, watchdogs, presentation acknowledgement, touch limits
and cancellation on genuinely unhealthy displays remain intact.

The first cache-only check made selection usable, but caught one brief numeric
expiry that still closed an open menu. The final policy separates numeric-data
visibility from owner/render/scanout health: a brief dark interval preserves the
open menu, clears its pressed state and disables input. A fresh visible release
is required before another press. Source/view changes, stale touch, actual display
unavailability and inactivity still cancel. The backlight guard itself is unchanged.

Before the fix, `turn_prepares_navigation_cache_before_the_hold_completes` failed
with 0 prepared stages versus 8 required. After the fix, all 61 UI tests pass,
including existing full-frame/damage-buffer equivalence and menu pressed states.
Workspace/all-target MSFS Clippy passes with warnings denied. A new visibility
regression verifies that a hidden gesture cannot select, recovery requires a fresh
release, and genuine display unavailability still cancels. The user confirmed
holding the final menu open and making selections works after installation.

The cache-only external regression reached Turn32.38 and Attitude32.20 FPS, but
recorded one Attitude numeric-expiry tick, so it is not a zero-expiry pass. The
existing 150 ms external-data guard remains active; this menu fix does not claim
to eliminate every source-jitter expiry or alter that policy.

The hardware trace assertion also reproduces the original defect:

```powershell
python artifacts/assert-menu-trace.py artifacts/turn-menu-before-fix.json
```

The Windows structured capture uses the existing touch/display CLI without
injecting touch or bypassing safety checks. Trace files and temporary capture
tools live in the ignored `artifacts/` debug area. No production debug logs or
new touch-injection commands were added.

Prevention: pixel equivalence alone cannot detect missed rendering deadlines.
The new regression checks that a supplied Turn menu cache is prepared before
the hold can complete; the board check requires a five-second menu dwell and
selection, with actual expiry/backlight counters recorded.

Final hardware capture passed: menu open12.724s, stable presented-menu interval
12.418s at32.37FPS, successful change to another gauge, zero new expiry ticks,
zero blanking and zero sampled dark frames. User confirmed the final menu stays
open and works. Evidence: artifacts/turn-menu-final.json and
artifacts/turn-menu-final-result.json. This supersedes the cache-only candidate.
Final firmware app1,039,792/1,048,576 bytes; ELF SHA256:
5037804BD2ED685983B577C79ADA2B67CD6097FBF6F225F6490217B6140D56D5.
