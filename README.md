# Round Flight Instruments

Latest tagged checkpoint: **v0.2.0-rc.4**. Touch gauge selection, Attitude rendering
and IMU bench motion are implemented. Menu switching and idle display passed
visual checks; both gauges render above 30 FPS in bench motion tests. The baseline
also includes native SDK crash isolation, two-plane MSFS recovery and pressure
writeback verified on November 2. This source checkpoint has no new binary release.

A Rust instrument-display prototype for the **Waveshare ESP32-S3-Touch-LCD-2.8C
(SKU 29086, 480 x 480, 16 MiB flash)**. This source checkpoint provides a simulated
three-pointer altimeter, touch barometer adjustment, a text CLI and a smooth
0–10,000 ft demonstration, a standalone attitude renderer and touch gauge selection.
The remaining four six-pack gauges are planned. MSFS 2020 input and optional two-way barometer
control are bench-tested on the Just Flight Piper Warrior. See the
[MSFS setup guide](docs/MSFS-TWO-WAY-TEST.md).

**Bench/simulation use only. Not validated or certified for flight.** The onboard
IMU is available for diagnostics and explicit accelerometer-only attitude bench testing.

![Rust altimeter preview](docs/images/altimeter-480.png)

*Software-rendered preview. Instrument lettering: Leonardo Di Lena / MS33558C.*

## Get it running on your Waveshare

You need the exact board above, a data-capable USB cable, and a Windows PC with
Python 3.11 or later. **The prebuilt firmware does not require Rust.**
The published v0.1.1 ZIP below runs the standalone demonstration; it does not
include MSFS integration. For MSFS, use the firmware-inclusive integration test
package and [its installation guide](docs/MSFS-TWO-WAY-TEST.md). This checkpoint
does not publish a new binary release.

1. Download and extract **[round-instruments-v0.1.1-waveshare-29086.zip](https://github.com/scottnortman/round-flight-instruments/releases/download/v0.1.1/round-instruments-v0.1.1-waveshare-29086.zip)** from the [v0.1.1 release](https://github.com/scottnortman/round-flight-instruments/releases/tag/v0.1.1).
2. Connect the board's **UART USB** port. Identify its CH343 COM port in Device
   Manager. The other USB connector does not provide this firmware's CLI.
3. Follow the **[board installation guide](docs/INSTALL-WAVESHARE.md)** (also
   included as `INSTALL.md` in the ZIP): install the Python tools, verify the
   download, back up your original firmware, and flash the supplied image.
   Replace the example `COM5` with your own port; close other serial terminals.
4. Open PuTTY or another serial terminal at **921600 baud, 8 data bits, no parity,
   1 stop bit, no flow control**. Disable terminal local echo: the board echoes
   characters. Initial boot logs use 115200 and may look garbled briefly.
5. Enter these commands, waiting for each `OK` response:

```text
sim preset level
instrument altimeter
baro 29.92
sim takeoff 60
```

The gauge holds zero for one second, climbs to 10,000 ft over 60 seconds, then
holds there. On a fresh boot, missing altitude and a red X are expected until
you supply simulated data. Settings and simulated values reset on reboot.

## Attitude gauge development

In this source checkpoint, hold the dial center for **3 seconds**, release,
then tap Altimeter or Attitude to select a supported face. All six instruments
are listed; unfinished or source-incompatible choices are disabled. Active
buttons turn blue and chirp. Set the volatile hold with `touch menu hold 3000`.
See [touch gauge menu and current MSFS limits](docs/GAUGE-MENU.md).

`feature/attitude-gauge` (checkpoint **v0.2.0-rc.3**) adds the standalone phone-reference attitude face,
shared pitch/roll CLI inputs and `sim attitude 60`. The board passes a 60-second
motion test at **32.33 completed FPS**, with no observed deadline misses or
unexpected blanking. The hardware motion was visually accepted: smooth, with no
tearing or artifacts. This source tag has no prebuilt firmware release package.
After building/flashing this branch, enter:

```text
source sim
sim preset level
instrument attitude
display on
sim attitude 60
```

Use `sim set pitch_deg 10` or `sim set roll_deg 30` for manual poses and
`instrument altimeter` to return. See the [attitude guide](docs/ATTITUDE.md)
and [verification record](verification/ATTITUDE.md). For handheld debugging,
`source imu` selects the onboard accelerometer tilt with an **IMU BENCH** label;
hold the screen upright facing you. A 15-sample average smooths the motion; rejected
readings retain the last usable pose without a hold annotation, red X, or
source-age blanking. See [IMU bench controls and limitations](docs/IMU-ATTITUDE.md).
The 15-sample IMU motion and adjusted 30/60 bank labels were visually accepted.
See the [IMU checkpoint and verification](verification/IMU-ATTITUDE.md).
Current MSFS telemetry does not yet drive the attitude face.

## Control the gauge

Hold the Kollsman pressure window for **0.25 seconds**; it turns blue while pressed.
Release, then use **−/+** for 0.01 inHg steps or **29.92** to reset the draft.
**Apply** changes the barometric offset; **Cancel** or 30 seconds of inactivity
leaves the active value unchanged. [Touch details](docs/TOUCH.md).

Popup buttons also turn blue while pressed. A short chirp acknowledges each new
active-control press; use `touch sound off` to mute or `touch sound on` to re-enable it.

| CLI command | Result |
| --- | --- |
| `help` / `status` | Commands and current data/validity |
| `sim set pressure_altitude_ft 8500` | Hold a manual simulated altitude |
| `sim takeoff 60` / `sim takeoff stop` | Start/stop the climb demonstration |
| `baro 30.01` | Change the indicated-altitude offset |
| `touch status` / `touch hold 250` | Touch diagnostics / hold time in milliseconds |
| `touch sound on` / `touch sound off` / `touch sound status` | Active-control chirp controls and diagnostics |
| `display stats` | Last rendering measurement; the climb records this automatically |
| `imu on`, `imu read`, `imu off` | Sensor acquisition/diagnostics; does not change the selected source |
| `source imu` / `source sim` | Select upright IMU attitude bench mode / return to simulation |

The bundled Python client provides line editing: `python cli.py --port COM5`.
In a source checkout use `python scripts/cli.py --port COM5`. Type `exit` to
release the port. See the [complete CLI reference](docs/CLI.md).

## Run the PC simulations

Clone this repository and run these commands from its root:

```powershell
git clone https://github.com/scottnortman/round-flight-instruments.git
cd round-flight-instruments
git checkout v0.2.0-rc.2
```

**Interactive browser gauges** (Node.js and Python):

```powershell
Push-Location apps/browser-sim
npm run check
npm run build
Pop-Location
python -m http.server 8000 --bind 127.0.0.1 --directory apps/browser-sim/dist
```

Open <http://127.0.0.1:8000>. This is a separate JavaScript visual reference with
multiple gauges and manual/demo controls; it does not connect to the board.

**Shared Rust simulation** (Rust/Cargo and Python):

```powershell
cargo build -p desktop-cli --locked
python scripts/cli.py
cargo run -p instrument-ui --example snapshots --locked
```

The desktop CLI uses the board's validation and command logic. It is text-only;
the snapshot example writes 30 still PPM images to `output/gauge/` using the
actual Rust renderer. [Detailed PC setup](docs/GETTING-STARTED.md).

## Development, verification and limits

**MSFS integration:** a simulator-independent measurement contract, synthetic/replay
PC bridge and native MSFS 2020 adapter support the altimeter. Read-only is the
default; `--writeback` enables barometer edits confirmed by simulator readback.
The Just Flight Piper Warrior live test confirmed four writes. Full flight/accuracy
acceptance and other aircraft remain unverified; the DCS adapter is planned.
The bridge currently exits when aircraft, flight or position changes. Load the
new flight, unpause, then restart:

```powershell
python scripts/start_msfs.py --port COM5 --writeback --frame-timing
```

Run this from the Python bridge package folder. For automatic aircraft-change
recovery and operation without Python, use the native connector below.

The [native connector](docs/NATIVE-CONNECTOR.md) replaces Python with a Rust
executable and USB-identity enrollment. A hidden
SimConnect worker contains native crashes and is replaced automatically. Desktop
tests verify fresh altitude/QNH after recovery and preserve optional two-way mode.
A two-aircraft live test also passed recovery and pressure writeback; see the
[acceptance record](verification/MSFS-AIRCRAFT-SWITCH-20261102.md). Receiving simulator values never
requires `--writeback`; that flag enables gauge-to-simulator edits only. Use the
[recovery test guide](docs/MSFS-RECOVERY-TEST.md) and return its captured logs.
Tray UI remains pending. Leave optional diagnostic queries off for normal use
to avoid their visible timing impact.
[Two-way setup and test guide](docs/MSFS-TWO-WAY-TEST.md) ·
[First MSFS test: which PC does what](docs/MSFS-FIRST-TEST.md) ·
[Run the bridge](docs/TELEMETRY.md) ·
[Architecture, six-pack semantics and rendering budgets](docs/MSFS-INTEGRATION.md).

Portable Rust crates separate instrument state, protocol, touch gestures,
rendering and sensor drivers from ESP-IDF board support. One state owner applies
validated commands; input freshness, display supervision and watchdogs remain
independent of rendering. See [architecture](docs/DISPLAY.md) and
[control contracts](docs/BASELINE.md).

- **159 Rust tests**, native-enabled Clippy and eight executable connector
  acceptance tests pass for this baseline. Updated firmware passes hardware
  timeout/recovery and safety checks: altimeter **32.32 FPS**, attitude **32.35 FPS**.
  Five browser regression suites passed for the standalone baseline.
  The rapid-QNH hardware benchmark averaged **31.72 completed FPS**. Normal hardware
  animation measured **32.33 FPS** with IMU polling; repeated popup/setting
  activity measured **29.22 FPS**. These are measured workloads, not minimum-rate
  guarantees. [Verification and reset investigations](verification/TOUCH.md).
- Other hardware gauges, an encoder, persistent
  settings and faster communications remain future work.
- The IMU has an unresolved stationary gyro offset; its readings and bench tilt
  are not calibrated attitude. [IMU limitations](docs/IMU.md).
- Building firmware uses a separate Cargo workspace and ESP-IDF 5.5.5. The
  development scripts contain workstation paths and guard the original board;
  new owners should use the prebuilt install route or follow
  [hardware development setup](docs/GETTING-STARTED.md#hardware-development).

```powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

This README is the canonical release overview. Detailed setup, contracts and
dated evidence live in `docs/` and `verification/`. Older handoff notes are
historical. [Checkpoint notes](docs/releases/v0.2.0-rc.1.md) and
[standalone release notes](docs/releases/v0.1.1.md).

## License and AI assistance

Original code and documentation: **[Apache-2.0](LICENSE)**. Third-party assets
retain their own terms, including the instrument font; see [NOTICE](NOTICE) and
[third-party notices](THIRD-PARTY-NOTICES.md).

Developed with **OpenAI ChatGPT and Codex**, under Scott Nortman's direction,
with automated checks and physical bench testing. AI assistance is not a
correctness or flight-safety guarantee.
