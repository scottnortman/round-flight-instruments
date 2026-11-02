# MSFS 2020 captured decoder regression

These hex files are unmodified SDK packets from the user-supplied
`rfi-evidence-20261101T203834Z-5d4c5c.zip`, SHA-256
`78cf1eef7ec347d847e3a1a79a83846484522f973840976ab84c6beab5483f4d`.
Source: `sdk-packets.jsonl`, SDK packet ordinals 9 (data) and 7 (Sim event).
The larger report and scripts are evidence only; no attached code is executed.

- Data: version 5, 72 bytes, request 9001, definition 9000, object 1,
  flags 0, entry 1/1, four FLOAT64 values at byte offset 40.
- Event: version 5, 24 bytes, group 0xffffffff, event 1100, value 1.

The fixture seam selects definition 9000 without altering any captured bytes.
Production always selects definition 1; the data layouts are identical.
Mutation tests explicitly change copies to check rejection and object passthrough.
Do not change the original hex to make tests pass or treat object 1 as a constant.

Before the fix, `cargo test -p simconnect-native --features sdk
captured_msfs2020_resolved_user_object --locked` failed in the actual native
decoder with 0x8000FFFF, matching the report. It passes after removing the
incorrect comparison of the reply object ID to the request selector USER (0).
The provider separately checks request correlation, age and session identity.
