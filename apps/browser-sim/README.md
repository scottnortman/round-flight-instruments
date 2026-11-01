# Browser gauge simulator

This is the JavaScript/SVG visual reference imported from the supplied ChatGPT
handoff. The current project status and hardware implementation are described in
the [canonical README](../../README.md); start with the
[getting-started guide](../../docs/GETTING-STARTED.md).

From this directory, with Node.js installed:

```powershell
npm run check
npm run build
python -m http.server 8000 --bind 127.0.0.1 --directory dist
```

Open <http://127.0.0.1:8000>. No npm dependencies or install step are needed.
Press Ctrl+C in the server terminal to stop it.

Select Heading, Airspeed, Turn coordinator, Attitude, Altimeter, or the combined
Attitude + altitude view. Use the demo, sliders, preset controls and data-loss
selector. Heading opens by default. VSI has not been implemented. This app has
its own simulation state: it is not connected to the Rust desktop process or
the board's UART. Browser tests do not verify the physical display.

On desktop, phone motion input is deliberately unavailable. On a supported
phone, open an HTTPS deployment directly, connect sensors and set level with the
screen upright facing you. Sensors stay local to that browser. Phone heading is
relative, and airspeed/altitude remain simulated. This has not been validated for
flight. The localhost server above is for the PC and is not reachable from a
phone.

`src/site-settings.json` controls the optional phone-version link. It defaults
to `./`, pointing to the same deployment; set it to your own HTTPS URL if hosting
a separate phone copy, then rebuild. The original personal deployment URL was
removed for this repository. `dist/` is generated and ignored by Git.

The [imported design reference](REFERENCE.md) preserves the handoff's detailed
gauge models and browser checks. Statements about versions describe that browser
prototype. The old handoff's agent instructions were not installed as current
project instructions. Font bytes and on-page attribution are preserved; see
[third-party notices](../../THIRD-PARTY-NOTICES.md).
