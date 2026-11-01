> Historical browser design reference imported from the ChatGPT handoff.
> For current setup and project status, use the [project README](../../README.md).

# Modular round flight instruments — phone bench prototype

Native render coordinate system: 480 × 480. Each gauge is a separate render module in the instrument registry. Sensor adapters feed independent attitude, air-data and turn-data snapshots. All phone processing stays in the browser; there is no sensor-data recording or network transport.

## Desktop / phone input policy (v7)

- `src/device-support.js` owns browser-device classification and preflight capability checks. Instrument renderers and math do not decide device policy.
- Recognized desktop Windows/macOS/Linux/ChromeOS browsers have phone motion input disabled. Touch-screen laptops remain desktop. API constructor presence, a small viewport, or a coarse pointer alone do not identify a phone or prove an IMU exists.
- Handheld candidates include Android phones/tablets and iPhone/iPad, including iPad desktop-style Macintosh user agents with touch capability. Low-entropy UA Client Hints are used when present, with UA/platform fallbacks; this is a UI heuristic, not guaranteed hardware detection.
- Secure context, orientation API presence and advertised sensor policy are checked separately. Actual finite, fresh sensor samples still determine instrument validity after the user requests access. No sensor listeners or permission requests are initiated during device classification.
- Both browser sensor adapters also reject direct desktop connection attempts before permission requests/subscriptions. The UI guard runs before changing source mode, so the disabled option does not stop the demo or manufacture a phone-data failure.
- On desktop, the phone button reads Phone input unavailable, calibration instructions are hidden, and a Desktop demo mode message points to manual/demo controls. The page does not imply a remote connection from the PC to a separate phone. A supported phone opens the same URL and requests its own local sensors.

## Heading indicator (v6)

- Separate heading module and `headingData` snapshot. Conventional 360° compass card: 5° ticks, 30° labels with trailing zero omitted, N/E/S/W cardinals. Fixed aircraft symbol and top index; the card rotates opposite increasing heading. Existing embedded MS33558C-based lettering is retained. Layout references are FAA PHAK chapter 8 and Mid-Continent's six-pack instrument explanation.
- Phone mode is explicitly **relative heading**, referenced by the user. It does not assert magnetic or true north, use GPS, or request additional compass permissions. Phone drift/accuracy have not been validated for navigation.
- `src/heading.js` consumes complete orientation matrices from the existing upright-panel adapter. Its reference, circular heading filter, freshness, and data validity are independent of the pitch/bank display limits and turn-rate adapter. Right yaw increases heading; isolated pitch and roll do not change heading at an upright level reference pose. Heading is unavailable close to vertical forward direction.
- Set level captures a new phone pose while preserving the indicated heading. Set reference/±5°/cardinal presets adjust the heading at the current pose without zeroing attitude or turn data. A numeric reference field permits 0–360°; north reads 360° in the bench output and N on the card. The simulated slider covers 0–359°.
- Circular filtering handles 359°/0° smoothly. Card labels are oriented on the rotating card, so the selected label reads upright at the fixed top index. No heading bug, course pointer or HSI navigation function is implied.
- Heading data expires after 2.5 s. Missing alpha/full orientation, invalid timestamps, stale data, explicit heading loss and near-vertical heading geometry hide the compass card and aircraft and display HDG DATA. Setup/re-level is amber. Page hiding or a screen rotation clears the heading pose reference and requires Set level again, retaining the last indicated heading for re-referencing.
- The demo covers cardinal headings, north crossings in both directions, data loss and recovery. Heading is the default opening module. The other instrument renderers and phone adapters remain separately selectable.

## Airspeed indicator (v5)

- Separate `AirspeedModule` renderer, `airspeedData` snapshot, and `src/airspeed.js` aircraft profile/scale definitions. Input is **simulated indicated airspeed in knots (KIAS)**. It is not phone IMU integration, GPS ground speed, CAS or TAS.
- C172S POH example markings, figure 2-2: white 40–85 KIAS, green 48–129, yellow 129–163, red line 163. These are an example aircraft profile, not universal six-pack limits. The renderer can accept a validated alternative profile; face labeling follows the supplied profile.
- Classic single-engine conventions follow FAA PHAK figure 8-8. United UI8000-B445 page 2 (8025/B.477, 0–200 knots) is the dial-layout reference. The prototype's monotonic nonlinear scale is representative, not a measured reproduction of a particular serial-number instrument.
- Needle, ticks, numeric labels and all arc endpoints share one speed-to-angle mapping. White and green arcs occupy different radii to preserve their overlap. The needle continues beyond VNE in an overspeed demonstration; it is not capped at the red line.
- Zero IAS is valid. Invalid, nonfinite, negative, out-of-scale or explicitly lost data hides the pointer and shows IAS DATA. It does not masquerade as zero. Airspeed validity is independent of attitude, turn and altitude validity.
- Finite demo, 0–200 KIAS slider, and 60/85/120/150/163 knot preset buttons. The demo visits the speed ranges, the red line, an overspeed example and an independent airspeed-data loss/recovery. It is an illustration of the instrument, not an aircraft performance simulation.
- In the original v5 release, airspeed opened by default; heading is now the opening module. The phone-connect panel is replaced with an explicit simulated-IAS explanation on this gauge. Existing phone adapters continue to serve the motion instruments.

Airspeed profile reference (Cessna document, hosted by Camarillo Airport):
https://www.camarilloairport.com/downloads/N577ND_AFM-POH.pdf

Dial reference:
https://www.unitedinst.com/Documents/UI8000-B445%20(E).pdf?action=view

## Turn coordinator (v4)

- Layout references: FAA *Pilot’s Handbook of Aeronautical Knowledge*, chapter 8, figures 8-21/8-22; Mid-Continent 1394T100 installation manual 9020079 rev.A, figure 3.3 and section 4. These establish conventional cues, not an exact manufacturer face reproduction.
- Existing embedded MS33558C-based reproduction, with the author's straight numeral-one correction, supplies the face lettering. The original font data and attribution are preserved.
- The miniature airplane shows a rate cue, not bank angle. Zero-rate reference marks and standard-rate L/R marks are fixed. A steady upright ±3°/s yaw input aligns the airplane's wing direction with a standard-rate mark (two-minute turn).
- Prototype rate model: equivalent rate `r + p * tan(30°)`, where p is body roll rate and r is body yaw rate. Deflection is 20° per 3°/s, limited to ±45°. This chosen cant, scale and 0.22 s rate filter illustrate conventional roll response; they are not a validated dynamic model of a 1394T100 or a calibrated aircraft turn-rate estimator.
- Phone mounting: screen upright, facing the operator. Aircraft forward points into the screen, right follows screen-right, down follows screen-bottom. DeviceMotion angular rates use W3C alpha=z, beta=x, gamma=y in natural device axes; screen orientation is applied before the aircraft transform. Existing attitude mapping is retained.
- Preferred rate source: DeviceMotion gyroscope rates in degrees/s. Fallback: rotation-matrix differences from complete DeviceOrientation readings, avoiding Euler-angle derivatives. The UI names the active rate source. Fallback can be noisy at slow rates because of browser orientation quantization.
- Inclinometer ball: apparent-gravity direction from `accelerationIncludingGravity`, opposite measured specific force, transformed into screen axes. Its visual travel saturates at ±15° transverse apparent-gravity angle. Freefall/invalid acceleration does not produce a fake centered ball. A banked stationary phone makes the ball fall toward the low side; that does not demonstrate a coordinated aircraft turn. The ball does not depend on yaw or a fabricated bank-to-slip calculation.
- Rate and ball freshness/validity are independent. Samples expire after 1.5 s. Screen rotation/visibility changes clear cached samples and require the shared level reference again. Rate setup has an amber pending state; missing rate data produces OFF/failure cues. Missing acceleration hides the ball with BALL DATA.
- Connect phone requests orientation and motion permissions from the same user gesture where the browser requires it. Permission denial does not fabricate inputs. Set level establishes the attitude reference; it does not subtract a guessed gyro bias or change the inclinometer's gravity reference.
- The turn demo and sliders use explicitly simulated yaw rate, roll rate and ball displacement. They are separate from the phone adapter. Switching gauges preserves the connection. Altitude remains simulated.

This is a bench/UI prototype. Browser sensor accuracy, latency, gyro bias, temperature response, mounting and instrument dynamics have not been validated for flight use.

## Build and checks

```text
node test-phone-attitude.cjs
node test-turn-coordinator.cjs
node test-airspeed.cjs
node test-heading.cjs
node test-device-support.cjs
node build.mjs
```

## References

- https://www.faa.gov/sites/faa.gov/files/10_phak_ch8.pdf
- https://assets.mcico.com/assets/products/InstallationManuals/1394T100-3B_7B_14RB_im.pdf
- https://www.w3.org/TR/orientation-event/
- https://www.studiodilena.com/en/2573.html
