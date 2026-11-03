# Touch gauge selection

Hold one finger near the dial center for **3 seconds**, then release. The center
turns blue with a MENU label and chirps once when a valid press starts. A menu
shows all six instruments; tap an enabled instrument and release to select it.
The current selection has an amber border. Active buttons turn blue and chirp
once per press, using the existing sound setting and cooldown. Disabled buttons,
titles and empty areas are silent. Selection closes the menu; Cancel preserves
the existing gauge. Inactivity closes it after 30 seconds.

The initial target is a 100-pixel-diameter circle centered at (240,240) on the
480-pixel face. The Kollsman window owns the overlapping portion of that circle
on the altimeter, even when pressure editing is unavailable. The small blue
center badge provides feedback without covering the pressure window. Target size
is a shared layout constant in the portable navigation policy, ready for tuning.

## Availability and source ownership

- Local simulation: altimeter and attitude are selectable.
- Current external T1 stream (including MSFS): altimeter only; attitude says
  NO SIM DATA because pitch/roll are not transported yet.
- IMU bench source: attitude only; altimeter says NO IMU DATA.
- Airspeed, heading, turn coordinator and vertical speed are visible but disabled
  with NOT BUILT labels until their renderers/data paths are implemented.

Selection calls the existing `Command::Select` only. It never changes the source,
barometer, source timestamps, validity or simulation parameters. A missing-data
face can still open the menu when the display is healthy. Navigation and pressure
editing are mutually exclusive. Choosing a gauge does not start a demo.

The simulator measurement abstraction supports expansion, but the current native
connector and T1 wire profile only drive the altimeter and select it on initial
connection/recovery. Supporting other live gauges requires their measurements,
units/validity rules and transport profile/capability handling end to end. This
feature does not claim MSFS attitude or six-pack support.

## CLI and lifetime

```text
touch menu hold 3000
touch status
touch cancel
touch sound off
touch sound on
```

`touch menu hold MS` accepts 250–5000 ms and cancels an open menu/hold. It is
independent of `touch hold MS`, which configures the barometer editor. Both are
volatile and reset on reboot. `touch cancel` closes either popup. `touch status`
adds menu state, hold duration, center press state and requested/presented menu
generation; its existing `overlay` field continues to mean the barometer editor.
`snapshot_misses` counts temporary contention when reading the touch mailbox.
Such contention retains the prior sample with its original timestamp; it cannot
extend the 120 ms input freshness limit. Explicit sensor invalidation clears it.

Fresh release is required before arming and after opening. Buttons act on release
inside the same accepted press, with no hold repeat. Dragging off, excessive
motion during the center hold, changed contact ID, multiple contacts, stale touch,
owner timing gaps, source/session/view changes or unavailable display cancel
gestures. A menu must have a matching confirmed presentation generation before
buttons can arm; an old framebuffer cannot authorize a new popup's controls.

The policy is allocation-free and hardware-independent. No touch injection CLI is
provided. Normal numeric freshness/watchdog rules continue while a menu is open.
The shared renderer tracks overlay damage per framebuffer, including restoration
against the attitude's moving horizon. A separate 450 KiB CPU-only PSRAM bitmap
caches the attitude menu lettering and button state; it is never submitted to DMA.
Initial preparation happens before the first visible frame; later cold preparation
is spread over eight frames before the hold completes. Press
transitions redraw only the old/new button, rather than the entire menu.
The three scanout buffers and their ownership rules are unchanged. Other display
sizes can use the portable uncached path.
The board retains separate prepared gauge backdrops so switching does not rebuild
the other face or exhaust the attitude's 100 ms freshness budget.

## Verification

Portable tests cover hold boundaries, release-to-arm, capability gates, disabled
buttons, cancellation, sound events, stale input, configuration bounds and old
presentation generations. Pixel comparisons exercise opening, pressed buttons,
closing and center feedback across all three rotating buffers on both real faces
and a placeholder. Generate the actual Rust-rendered previews with:

```powershell
cargo run -p instrument-ui --example gauge_menu --release --locked
```

On hardware, first verify motion budgets, then hold center/release, select
Attitude, reopen and return to Altimeter. Check blue feedback/chirps, disabled
buttons, dragging off, Cancel, inactivity and the existing pressure editor.
`scripts/check-gauge-switch.py --port COM5 --report output/gauge-switch.json`
automates simulated gauge switching and requires at least 30 FPS with no new
blanking or presentation expiry; it does not synthesize touchscreen input.
