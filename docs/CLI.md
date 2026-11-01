# Text CLI protocol 1

The board now has [supervised display output](DISPLAY.md). Use `display status`
for presentation health and `display pattern|gauge|on|off` for viewing/control.
`display stall|transfer-stall|recover` are explicit bench fault-injection commands.
`display clock 12|18` selects a restricted bench pixel-clock diagnostic; normal
operation uses 18 MHz. See DISPLAY.md for request/completion status semantics.
They are recognized but unsupported on the desktop console. The shared raster
example provides desktop render verification. Board status/selection ACKs now
say `display=managed`; this does not claim physical presentation completion.

The board also implements [optional IMU diagnostics](IMU.md):
`imu on`, `imu off`, `imu status`, `imu read`, `imu selftest`, `imu audit`, and
`imu estimator`. See the [estimator extension contract](ATTITUDE-ESTIMATOR.md)
for bench tilt, invalidity, and future Kalman hooks.
They do not select a gauge source or alter simulation settings.

The same commands run on desktop and UART. The application UART uses 921600/8N1 without flow
control. ASCII commands are case-sensitive. Fields include their units; decimal
and exponent notation are accepted. NaN, infinity, and values outside the
documented ranges are rejected, with no implicit clamping.

The Python client defaults to 921600; `--baud 115200` supports older firmware.
ROM/boot and early initialization logs remain at 115200. The firmware drains
those logs and switches UART0 before publishing READY. A terminal set to 921600
can therefore show garbled boot text before readable application responses.
Wait for the readable READY line before sending commands after a reset/flash.
If the Python client is opened during boot and rejects garbled framing, close it
and reopen after startup; malformed bytes never execute a partial command.
Flashing uses a separate 460800-baud connection and does not set the CLI speed.

## Character echo

The board starts with character echo **on**; turn your serial terminal's local
echo **off** to avoid seeing each character twice. `echo on`, `echo off`, and
`echo status` return `OK echo=on` or `OK echo=off`. The setting is volatile and
does not change instrument state. Desktop console echo defaults to off because
the host terminal normally echoes input itself.

Characters are echoed before Enter. CR, LF and CRLF produce one newline (CRLF
on the board). Unsupported control bytes, including Escape, Backspace and DEL,
are displayed as `\xHH` and still rejected by the command framer; this is not
line editing. After the 128-byte command limit, echo emits `[...]` once and
suppresses further characters until the newline; the full command is rejected.

Echo and responses share one bounded output queue and one writer. Echo may block
the input worker under output pressure, but never the instrument state owner.
Raw input and receipt times still reach the existing strict framer. Send one
complete command and wait for its reply before typing another.

The Python UART client queries the current echo setting, disables echo for its
session, then restores it on normal close if it was originally enabled. This
also applies to hardware test scripts using the shared Link. A forcibly killed
client may leave echo off; type `echo on` or reboot to restore it. Older firmware
that rejects the echo command continues to work. The direct terminal controls
are intended for manual use; automated clients should disable echo first.

Send **one command and wait for its response** before sending another. Each
nonblank line produces one `OK ...` or `ERR ...` line. Startup emits
`READY protocol=1 source=sim display=off commands=help,status` on desktop; the
board advertises `display=managed echo=on commands=help,status,display,echo`. ESP-IDF boot logs
may precede it. A reboot starts a new session with missing data/default settings.
Status describes the snapshot when processed, not its age when received by a
slow client. Do not treat queued text status as live flight telemetry.

## Commands

### Touch barometer editor

Hold the Kollsman window for half a second, release, then use −/+ and Apply/Cancel.
Only Apply changes the active barometer/indicated-altitude offset. See
[touch behaviour and safety](TOUCH.md) for cancellation and freshness rules.

`touch status` reports sensor/overlay state and last contact age. `touch hold MS`
sets a volatile threshold from 250 to 5000 milliseconds (default 250) and cancels
the current edit. `touch cancel` discards the draft. These board-only operations
return `ERR display_unsupported_target` on desktop. Commands do not inject touches.

### Animated altitude test

`sim takeoff` selects the altimeter, sets barometer 29.92 inHg, resumes the
simulator, holds zero altitude for one second, then climbs linearly to 10000 ft
over 60 seconds. `sim takeoff 30` uses a 30-second climb; whole seconds 5..600
are accepted. This is an accelerated altitude-only bench animation, not a flight
dynamics model; other fields and the IMU are unchanged.

- `sim takeoff status`: running/complete/stopped/faulted and elapsed/duration.
- `sim takeoff stop`: cancel the ramp and keep publishing the current altitude.
- `display stats`: completed presentations per second and maximum draw/transfer
  times. The measurement starts with the sweep and freezes at completion or stop.
- `display stats reset` / `display stats stop`: manually start/freeze a measurement.

Completion holds 10000 ft. A manual altitude set/clear, level preset, or pause
cancels the sweep; `sim resume` then holds the current value rather than restarting
the animation. Pause still stops source publication and lets data go stale.
An altitude fault or owner timing fault stops the sweep without clearing the
fault; a new sweep is rejected while either fault is present. Other injected
faults remain unchanged. A new valid takeoff command restarts from zero.

FPS measures completed presentations under the normal scheduling and safety
checks, not the LCD refresh rate or uncapped theoretical raster speed. The window
includes the ground hold and boundary frames; completion of the source ramp can
precede presentation of its final frame. Statistics are queried explicitly;
there are no unsolicited completion messages. Desktop supports the same sweep
but has no board display statistics.
With pipelined rendering, `transfer_max_ms` is submission-to-observed-completion
latency, including overlapped drawing and any deliberate diagnostic stall. It is
not time spent blocked on the hardware. It can exceed 100 ms after a bench stall
when completion already occurred; the 100 ms timeout applies to transfers still
incomplete when checked. Compare measurements with reset/frozen windows, not
lifetime maxima left over from fault tests.

Draw-time maxima include a backdrop rebuild if the view or barometer changes
during the measurement. `state=complete` means the simulation reached its endpoint,
not that a hardware self-test certified the display or sensors.

### General controls

| Command | Effect |
| --- | --- |
| `help` | Command and field names |
| `echo on` / `echo off` / `echo status` | Control/query console character echo |
| `status` | Selected view, source, settings, timing, each quantity's value/quality/age |
| `instrument list` | Six identifiers; altimeter renderer implemented, others show a placeholder |
| `instrument NAME` | Select altimeter, attitude, airspeed, heading, turn, or vsi |
| `sim preset level` | Stage 4520 ft pressure altitude, 120 kt IAS, all other inputs zero; injected faults unchanged |
| `sim set FIELD VALUE` | Stage one simulated input; applied at the next running sample |
| `sim clear FIELD` | Immediately remove staged input and current sample |
| `sim pause` / `sim resume` | Stop/restart source publication |
| `fault FIELD on` / `fault FIELD off` | Fail one quantity; recovery requires a new sample |
| `baro INHG` | Set reference, 28.10–31.00 inHg; pressure altitude remains unchanged |

Instrument selection preserves source state and barometric settings. Settings
are RAM-only and reset at reboot. ACK means the command was accepted, not that a
screen frame was presented. Set/preset commands while paused stage values until
resume. There are no commands for flash erasure or arbitrary memory access.
Physical IMU diagnostics are exposed separately as documented above.

| Field | Accepted bench range | Stale at age |
| --- | --- | --- |
| `pressure_altitude_ft` | -2000 through 60000 ft | 1000 ms |
| `pitch_deg` | -90 through 90 degrees | 100 ms |
| `roll_deg` | -180 through 180 degrees | 100 ms |
| `heading_deg` | 0 inclusive through 360 exclusive | 250 ms |
| `ias_kt` | 0 through 200 kt | 1000 ms |
| `turn_dps` | -30 through 30 degrees/second | 100 ms |
| `ball` | -1 through 1, normalized | 100 ms |
| `vsi_fpm` | -6000 through 6000 ft/min | 1000 ms |

These ranges and freshness limits are initial verification parameters, not
validated aircraft limits. Simulation is piecewise constant, with a nominal
20 ms sampling interval while running. Actual cadence depends on scheduling;
the final board checks observed owner gaps up to 31 ms. There is no motion
model or air-data computation yet.

Status quantity format is `field=value:quality:age`, for example
`ias_kt=0.000:valid:20ms` or `pitch_deg=-:stale:120ms`. Quality is `missing`,
`valid`, `stale`, `failed`, or `timing_fault`. Invalid readings expose no numeric
value; missing age is `none`. The output rounds numbers to three decimal places
(barometric settings to two); this is formatting, not measurement precision.

## Framing and failures

- Maximum command: 128 bytes excluding terminator. CR, LF, and CRLF accepted.
  Space/tab delimit tokens; blank lines have no response. No terminal escape or
  backspace editing is implemented in firmware; the Python client provides local
  line editing and sends completed lines.
- Incomplete lines expire 30 seconds after the framer receives the first byte.
  The timeout is checked on the next byte; no asynchronous timeout message is
  emitted. OS terminal buffering can delay delivery on desktop.
- An overlong, invalid-byte, or expired line is discarded through its terminator.
  Its suffix cannot execute as a separate command. EOF never executes a partial
  command. Errors are `syntax`, `unknown_field`, `unknown_instrument`, `number`,
  `range`, `line_too_long`, `invalid_byte`, or `line_timeout`.
- Replies are bounded to 1024 bytes including LF. A formatting overflow produces
  `ERR internal_response_overflow`. The UART VFS may translate LF to CRLF.
- Commands queue: 8; responses queue: 4 plus one pending owner response and one
  transmitting response. Full output queues stop command processing while source
  and health updates continue. UART RX buffering is 2048 bytes. Flooding violates
  the request/response contract and can overrun UART buffers; delivery is not
  guaranteed outside the contract.
- This bench protocol has no checksum, sequence numbers, authentication, or
  retransmission. It cannot detect every corruption that becomes another valid
  command. Future faster communications must add an explicit integrity/session/
  sequencing design before being used for operational data.

The Python client has a three-second response deadline. After any transport
timeout, close/reopen and inspect `status` before changing state; there are no
request IDs for matching late responses. Link loss does not silently change a
simulation into a live source. It also does not stop the deliberate onboard
simulation: issue `sim pause` when you want samples to age out.

## Touch sound

`touch sound on`, `touch sound off`, and `touch sound status` control the short
active-control chirp (enabled at boot). Status includes requested enable state,
hardware readiness, `active_or_unknown`, nominal `pulse_ms`, beep and error
counts. Mute takes effect on the bus worker; settings are volatile. See
[TOUCH.md](TOUCH.md) for rate limiting and bus-fault limitations.
