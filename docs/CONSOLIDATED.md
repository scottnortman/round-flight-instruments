# Consolidated flight panel

The development firmware adds **COMBINED** as the seventh touch-menu option.
Hold the screen center for one second, release, then tap COMBINED. The same
screen is available over UART with `instrument consolidated`; `instrument list`
and `protocol capabilities` advertise it. The selected screen is remembered by
the existing debounced device settings service.

The 480-pixel round panel contains:

- A central pitch/roll horizon, pitch ladder, bank scale and aircraft reference.
- Scrolling indicated-airspeed (knots) and indicated-altitude (feet) tapes.
- Turn rate/slip, magnetic heading and vertical speed (feet/minute) below.
- Source identification and the current QNH in inches of mercury.

The Combined bank scale uses larger, bold 30/60-degree labels, positioned inward
to clear the tick marks. Labels remain clipped to the central attitude disk and
its viewport so they cannot overlap the adjacent tapes or lower instruments.

The arrangement follows the supplied reference, using the measurements already
available in the six-instrument stream. Distance, ETE, clock and Wi-Fi indicators
are omitted because there are no corresponding inputs. No aircraft speed limits
are inferred or added to the airspeed tape. The VSI uses the standalone gauge's
left-hand zero and range (default +/-2,000 fpm). Values beyond that range show
OVER instead of a pegged needle; the number retains the actual valid value.

Each component uses the same validated model, units, sign convention and source
age as its standalone face. Failed or expired components show a cross or dashes;
other components remain available. A combined frame's numeric deadline is the
earliest deadline of any measurement still visible, including external QNH.
Navigation stays available through the existing independent display supervision.
COMBINED is disabled for legacy altitude-only T1 sources and IMU bench mode;
local simulation and the existing T2 connector supply its measurements.

Tap and release the outlined **QNH XXX.XX INHG** button at the bottom to open the
same pressure editor as the standalone Altimeter. Use +/-0.01 or 29.92 to change
the draft, then Apply or Cancel. The opening tap never changes the setting.
The button turns blue while pressed and retains its thin gray border, matching the adjacent gauge circles.

In local simulation, Apply changes the shared simulated setting. With MSFS 2020,
the existing test.13 host must have aircraft controls enabled; Apply sends the
same request and waits for simulator confirmation/readback. Read-only sessions,
DEMO, missing/stale altitude or QNH, calibration and unavailable displays block
editing. Other missing Combined instruments do not block a valid pressure edit.
Screen changes cancel the draft. See [touch editor details](TOUCH.md).

Saved aircraft profiles must be explicitly selected with **Use on connected
board** in the host editor. See [profile delivery](PROFILE-EDITOR.md).

## Altitude readout visibility

On the standalone Altimeter, tap and release the numeric altitude window to
hide its digits and fill, revealing the hands underneath. Tap the same outlined area again to show them. Its
one-pixel white border stays visible in both states, including invalid-data
views. The analog hands, altitude measurements and QNH remain active.

A continuous press toggles once on release. Sliding outside, multiple fingers,
missing/stale touch input or an unavailable display cancels the gesture. The
readout cannot respond through an open menu or pressure editor. `touch cancel`
also cancels a pending readout tap. Accepted presses use the existing chirp.
`touch status` reports `altitude_hidden=true|false`.

Visibility lasts through screen changes and menus, and resets to visible on
reboot. It is a local display preference and never sends an aircraft command.

## Desktop review

`cargo run -p instrument-ui --example consolidated --locked` writes production
renderer PPM previews under `output/consolidated/`: level, climb/bank, partial
failure, visible/hidden altitude, pressed QNH button, shared pressure editor and
the gauge menu. See the dated
[verification record](../verification/CONSOLIDATED.md) for hardware evidence and
remaining physical acceptance checks.
