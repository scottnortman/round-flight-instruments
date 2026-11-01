# Kollsman readability - November 1, 2026

Branch: `feature/kollsman-readability`, renamed from the empty next-feature
branch after the user specified the feature.

Removed the internal IN HG caption. Pressure digits grow from 20 to 32 logical
units, with regular weight selected after the user reviewed the bold version.
A static bold HG label grows
from the former caption's 12 units to 22 units and sits outside the box to the
right. The window widens toward the hub to 136 by 42 units at (270, 211).
Digits center at (338, 235); HG centers at (428, 235). Geometry scales uniformly.
Two decimal places, barometer limits, invalid placeholder and instrument math
are unchanged. These are shared Rust renderer changes; browser reference is
unchanged. Both items belong to the cached backdrop, whose key includes barometer.

Validation:

- All 10 existing instrument-ui tests pass, including cached/full/incremental
  equivalence across barometer changes, faults, view switches and three buffers.
- UI Clippy all targets with warnings denied, workspace formatting and firmware
  build/generated safety configuration checks pass.
- Regenerated the 24 snapshots at 240, 480 and 720 pixels; inspected the 480-pixel
  preview for clearance from 2, 3, ticks, hub and lower altitude panel.
- The user accepted the larger digits and external HG, then requested less black
  space below the numbers. The final window removes six units from the bottom
  (48 to 42 high), preserving the digits, label and three-unit border thickness.
  Final software preview inspected; all 10 UI tests and firmware build/config
  checks pass again. This margin adjustment awaits separate optical confirmation.
- [20-second climb plus ground hold](kollsman-takeoff.json) passed at **32.32 FPS**
  (679 presentations / 21009 ms). Maximum render 20 ms, transfer observation
  38 ms during measurement, owner gap 31 ms; no display/timing failure or reported
  blanking. IMU off. Ends at valid 10000 ft, 29.92 inHg with simulation running.
- [Barometer endpoint check](kollsman-baro-limits.json) exercised 28.10, 31.00 and
  29.92 through the board CLI; display remained healthy with valid altitude.
  Restored 29.92, closed the serial client and restored echo.

Initial 48-unit-high window ELF SHA256 (sweep and barometer checks above):
`24CC8DFECE393E1091F629D2E5162414B4BA3A995B010F7495019E3145D7681F`.
Intermediate 42-unit-high window with bold digits ELF SHA256:
`D1104C379EBAAD8AE084AD4D8DA29A110C30E3245BAC7C0ECC48DAB90616B7FD`.
Installed and [checked over UART](kollsman-tight-final-state.json): valid 4520 ft,
29.92 inHg, display healthy, no timing fault. Serial client closed and echo restored.
Application 911616 / 1048576 bytes (86.94%).

The user subsequently requested non-bold Kollsman numbers for readability.
Only their weight changes; size, position, window dimensions and external HG
label remain as above. All 10 UI tests, formatting and firmware build/config
checks pass; snapshots and README preview regenerated and inspected.
Regular-weight firmware ELF SHA256:
`8F4EB8F375354D7CE32D1572611B19AAA66BD3FB2CA3E3C24FA86F3E34143821`.
Installed and [checked over UART](kollsman-regular-final-state.json): valid
4520 ft, 29.92 inHg, display healthy and no timing fault. COM5 released.
