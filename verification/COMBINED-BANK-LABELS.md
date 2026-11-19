# Combined bank-label readability - November 9, 2026

Combined 30/60-degree bank labels enlarged 50% (logical 12 to 18), bold, radius
205 instead of 212, +/-25 and +/-55 degrees instead of +/-27 and +/-57. This
clears the rim/ticks while retaining the shared circular Disk and Combined
viewport clipping. Explicit compact rendering entry point; standalone attitude,
its cached foreground and captions unchanged.

113 UI tests and strict UI/all-target Clippy pass. Production level/banked/detail
previews inspected. Before/after pixel comparison: 214 changed pixels in each
valid view, all inside the attitude disk/viewport; partial-failure pixels exactly
unchanged. Three-buffer and multi-size regression included in existing UI suite.
Firmware built/flashed and application readback verified. Combined 32.34 FPS,
no failure/grace/warning frames. Device UUID and all saved IMU coefficients
unchanged. Board left local SIM level, Combined, DEMO off, IMU off for user review.
Physical readability acceptance remains with user; existing test.13 host works.
ELF SHA256 AFBC7782D819824BDED2770F927EA6DB34AA30D97A9AAEE88B3D1CD4F2598E53.
Application 1,292,784 / 4 MiB. Exact firmware/evidence/previews:
artifacts/combined-bank-labels-20261109/. Previous firmware/releases preserved.
