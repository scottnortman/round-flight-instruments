# Second reset: legacy I2C recovery interrupt storm

The user reported an unprompted reset during the mailbox-fix validation.
`touch-mailbox-standard-capture-2.json` records the failure and preceding
touch errors. The capture's UART output identified an **interrupt watchdog**
on CPU0, distinct from the earlier task watchdog in the standard message queue.
Failing ELF SHA256:
`67080184FA8D96DCCFFF02F525195412495881372FF9ECC8D2E8E952E488B6EB`.
The exact binary is retained locally in `artifacts/touch-rejected-legacy-i2c`.

Captured trace excerpt:

```text
Guru Meditation Error: Core 0 panic'ed (Interrupt wdt timeout on CPU0).
Core 0 was running in ISR context.
Backtrace: 0x4037743e 0x40377d15 0x40378333 0x4037c46b
0x4206aa66 0x4206aa90 0x4206bbe0 0x4201b8e5 0x4201b846
0x4200a22d 0x42008680 0x42009f2b 0x42007725 ...
ELF file SHA256: 67080184f
Rebooting...
```

Decoding against that exact ELF located `i2c_isr_handler_default` above
`i2c_hw_disable -> i2c_hw_fsm_reset -> i2c_master_cmd_begin -> GT911::poll`.
CPU1 was idle. The legacy recovery code disables the peripheral before masking
its interrupt, leaving an interrupt-reentry hazard during recovery. The trace
and four preceding GT911 `Bus(263)` errors are consistent with that hazard;
they do not establish the electrical cause of the first timeout.

The IMU failed earlier with the same ESP_ERR_TIMEOUT (263), while the gauge and
owner remained healthy. Evidence: `touch-mailbox-imu.json` (failed acceptance)
and `touch-imu-failure-capture.json` (explicit error before disabling IMU).
The IMU never became a gauge source. Do not discard these failed runs.

## Board transport correction

`board_bus.rs` replaces legacy `I2cDriver` with synchronous ESP-IDF 5.5.5
`i2c_master` calls. The current driver's recovery resets the peripheral and masks
interrupts before clearing the bus. Its source was inspected in the installed,
unmodified IDF checkout. The bus retains exclusive peripheral/pin tokens, private
device handles, one worker, 400 kHz clock, 10 ms transaction wait arguments,
ACK checks, the documented 2000 us stretch allowance and seven-cycle glitch
filter. No asynchronous callbacks can outlive Rust buffers. Failed reads never
publish touch/IMU data; touch freshness and watchdog limits are unchanged.

The first migration build hit the IDF legacy conflict constructor before main
(`touch-master-first-boot.log`). Symbol inspection showed the new master API
present and the legacy install/configure/transaction/ISR functions absent.
Rust/IDF whole-archive linkage retained the constructor alone. The IDF
`CONFIG_I2C_SKIP_LEGACY_CONFLICT_CHECK` option removes that false positive;
`Build-Console.ps1` now rejects any linked legacy install/configure/transaction
or ISR symbol and requires the new master symbol and generated configuration.
This does not permit using both drivers. No watchdog setting was relaxed.

References: [IDF master driver](https://github.com/espressif/esp-idf/blob/v5.5.5/components/esp_driver_i2c/i2c_master.c),
[legacy implementation](https://github.com/espressif/esp-idf/blob/v5.5.5/components/driver/i2c/i2c.c).

Hardware results are recorded in [touch verification](TOUCH.md): IMU transport,
UART/echo, display fault handling, a 32.32 FPS climb with the IMU running and a
1200-request traffic test passed. The user confirmed the reset button and additional
ad hoc interactions worked without reboot. This is bench evidence, not proof that
all electrical bus faults are recoverable; physical bus fault injection is outstanding.
