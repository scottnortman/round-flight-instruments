# Character echo — November 1, 2026 (America/New_York)

The installed board firmware defaults to character echo on at 921600/8N1,
without flow control. Set terminal local echo off. `echo on`, `echo off`, and
`echo status` are console settings; gauge source/state is unchanged.

Echo is produced by the RX worker and sent through the same bounded queue and
single TX writer as complete replies. Only RX can wait for echo space; the state
owner retains its nonblocking output path. The existing command framer receives
unmodified bytes and receipt times. CRLF is echoed once; unsupported controls are
escaped; echo truncates after 128 bytes while the framer rejects the full line.
This change does not add backspace editing.

The Python client queries/disables board echo while connected and restores it
on a normal synchronized close. An interrupted or unfinished request prevents
restoration, so a late reply cannot be mistaken for a new echo command. A raw
terminal can restore echo with `echo on` after terminating any partial input.

| Check | Result |
| --- | --- |
| Rust workspace | 42 tests passed; includes character-before-Enter behavior, CRLF suppression, bounded/escaped invalid input, echo switching, and owner progress with blocked echo output |
| Clippy and format | All workspace targets passed with warnings denied; root and firmware formatted |
| Firmware | Locked build/SDK validation and known-board flash passed; previous firmware and factory backups retained |
| Raw UART echo | Characters received before Enter; enable/disable/status, single CRLF, control escaping, overflow rejection/recovery passed |
| Python negotiation | Echo disabled during session and restored on clean close; unfinished request did not cause a new command on close |
| UART protocol | 57 checks passed, including 30-second timeout; maximum response 63 ms and owner gap 31 ms in this suite |
| Desktop protocol | 56 checks passed; maximum response 79 ms, owner gap 21 ms |
| Concurrent IMU/display | Sensor enable/restart, audits and estimator checks passed; display kept progressing without blanking; IMU ended off |

Tests first attempted immediately after flashing encountered the documented
115200-baud startup noise at a 921600-baud terminal and a rejected malformed
input. After initialization and the next terminating newline, the echo suite
passed. Wait for READY before sending commands after reset/flash.

Evidence: [raw echo](uart-echo.json), [UART protocol](uart-echo-protocol.json),
[desktop protocol](desktop-echo-protocol.json), [IMU integration](imu-with-echo.json),
[final display observation](display-with-echo.json).

Final intended/verified state: simulated 4520 ft, barometer 29.92 inHg, altimeter
on, IMU off, board echo on. See the final display report for measured timing.

Archive: `artifacts/cli-echo-20261101`, with source, firmware, matching bootloader,
SDK settings, partition CSV, lockfile and SHA-256 manifest.

- Application image: 893616 / 1048576 bytes (85.22%).
- ELF SHA256: 4c18d21afcbe5e6c47409dea4cf0866f5a9064ac921321be3d81f7bf2dcde506.
