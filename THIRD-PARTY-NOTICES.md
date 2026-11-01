# Third-party material

Project licensing does not replace third-party terms. Cargo dependencies retain
their own licenses; the checked-in lockfiles identify the resolved packages.

## Instrument lettering

MS33558C 1.100 by Leonardo Di Lena (Flanker), copyright 2020.

- Author/source: <https://www.studiodilena.com/en/2573.html>
- License identified in the supplied handoff: CC BY-ND 4.0,
  <https://creativecommons.org/licenses/by-nd/4.0/>.
- Unchanged font: `crates/instrument-ui/assets/MS33558C.otf` and the embedded
  font data in `apps/browser-sim/src/instrument-fragment.html`.
- SHA-256 of the original OTF:
  `22c536f3c5f179b2f17384195aedce445dbcb741f5d44ca25bd11f2372b84cee`.

The font outlines and metadata have not been modified. The UI uses the existing
capital-I glyph for the documented numeral-one substitution. The Rust renderer
uses offline rasterized text coverage in `glyphs-4bit.bin`. Retain the font
attribution and license link with distributions; the original code license does
not relicense the font. See the [asset notice](crates/instrument-ui/assets/NOTICE.md).

## Browser reference

The browser source and tests were imported from the user-supplied
`round-flight-instruments-handoff.zip`. The original documentation is retained as
`apps/browser-sim/REFERENCE.md`. Public-repository integration changed the entry
documentation and replaced the personal deployment URL with a relative link;
the gauge rendering and sensor algorithms were retained.

## Hardware register data and toolchain check

`firmware/console/src/panel_init.rs` contains the SKU-specific register sequence
extracted from Waveshare's official demo. Source references and the board
integration boundary are recorded in [docs/DISPLAY.md](docs/DISPLAY.md).
Downloaded vendor demo trees, schematics and SDK installations are not vendored
in this repository.

`tools/toolchain-check` was generated from the esp-rs/esp-idf-template at commit
`b814c2b968f84949707ed0a6ab72d262e3fbfbc4`; see [tools/README.md](tools/README.md).
Copyright 2021-2024 Contributors to esp-idf-template. The upstream template's
[MIT license notice](tools/toolchain-check/LICENSE-MIT) is retained with the
derived scaffold; original project additions are covered by the root Apache-2.0
license. Dependencies are fetched separately and retain their upstream terms.
