//! Windows SDK boundary with a narrow, optional primary-altimeter write path.
#![deny(unsafe_op_in_unsafe_fn)]
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Event {
    pub kind: u32,
    pub request: u32,
    pub code: u32,
    pub detail: u32,
    pub values: [f64; 4],
}
#[cfg(all(windows, feature = "sdk"))]
mod native {
    use super::Event;
    use std::{ffi::c_void, io, os::windows::ffi::OsStrExt, path::Path, ptr::NonNull};
    unsafe extern "C" {
        fn rfi_sc_open(path: *const u16, stage: *mut u32, error: *mut u32) -> *mut c_void;
        fn rfi_sc_close(client: *mut c_void);
        fn rfi_sc_request(client: *mut c_void, id: u32, heartbeat: u32) -> i32;
        fn rfi_sc_poll(client: *mut c_void, event: *mut Event) -> i32;
        fn rfi_sc_set_baro(client: *mut c_void, pressure16: u32) -> i32;
    }
    pub struct Connection {
        handle: NonNull<c_void>,
    }
    impl Connection {
        pub fn set_baro(&mut self, pressure16: u32) -> io::Result<()> {
            // SAFETY: exclusive live handle, bounded integer pressure; no borrowed SDK memory.
            let hr = unsafe { rfi_sc_set_baro(self.handle.as_ptr(), pressure16) };
            if hr < 0 {
                Err(io::Error::other(format!(
                    "SimConnect KOHLSMAN_SET failed: 0x{hr:08X}"
                )))
            } else {
                Ok(())
            }
        }
        pub fn open(path: &Path) -> io::Result<Self> {
            let path = path.canonicalize()?;
            let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
            if wide.contains(&0) {
                return Err(io::Error::other("NUL in DLL path"));
            }
            wide.push(0);
            let (mut stage, mut error) = (0, 0);
            // SAFETY: valid NUL-terminated UTF-16 path and exclusive out parameters.
            let handle = unsafe { rfi_sc_open(wide.as_ptr(), &mut stage, &mut error) };
            NonNull::new(handle).map(|handle| Self {handle}).ok_or_else(|| {
                let hint = match stage {
                    1 => "check the DLL path and install the Microsoft x64 Visual C++ runtime",
                    2 => "the DLL lacks a required API; use SimConnect.dll from the MSFS 2020 SDK",
                    3 => "start MSFS 2020 and load a flight before connecting",
                    _ => "the simulator rejected read-only setup; check the SDK/profile compatibility",
                };
                // Only the usual absent/unavailable simulator result is transient.
                // DLL, ABI and subscription failures must not be retried blindly.
                let kind = if stage == 3 && error == 0x80004005 {
                    io::ErrorKind::ConnectionRefused
                } else { io::ErrorKind::Other };
                io::Error::new(kind, format!("SimConnect setup stage {stage} failed (0x{error:08X}); {hint}"))
            })
        }
        pub fn request(&mut self, id: u32, heartbeat: bool) -> io::Result<()> {
            // SAFETY: this connection exclusively owns the live C handle.
            let result = unsafe { rfi_sc_request(self.handle.as_ptr(), id, u32::from(heartbeat)) };
            if result < 0 {
                Err(io::Error::other(format!(
                    "SimConnect request failed: 0x{result:08X}"
                )))
            } else {
                Ok(())
            }
        }
        pub fn poll(&mut self) -> io::Result<Option<Event>> {
            let mut event = Event::default();
            // SAFETY: handle is live; output has matching repr(C) layout (48 bytes).
            let result = unsafe { rfi_sc_poll(self.handle.as_ptr(), &mut event) };
            match result {
                0 => Ok(None),
                1 => Ok(Some(event)),
                2 => Err(io::Error::other(
                    "local SimConnect packet validation failed (see packet metadata above)",
                )),
                _ => Err(io::Error::other(format!(
                    "SimConnect dispatch failed: 0x{result:08X}"
                ))),
            }
        }
    }
    impl Drop for Connection {
        fn drop(&mut self) {
            // SAFETY: unique handle; close exactly once after all borrowed calls.
            unsafe {
                rfi_sc_close(self.handle.as_ptr());
            }
        }
    }
}
#[cfg(all(windows, feature = "sdk"))]
pub use native::Connection;

#[cfg(all(test, windows, feature = "sdk"))]
mod tests {
    unsafe extern "C" {
        fn rfi_sc_decoder_test() -> u32;
        fn rfi_sc_decode_fixture(
            input: *const u8,
            size: u32,
            definition: u32,
            out: *mut super::Event,
        ) -> i32;
    }
    #[test]
    fn captured_msfs2020_resolved_user_object() {
        let hex = include_str!("../tests/fixtures/captured-data.hex").trim();
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let mut out = super::Event::default();
        // SAFETY: valid byte slice and exclusive matching-layout output; C copies to aligned storage.
        let result =
            unsafe { rfi_sc_decode_fixture(bytes.as_ptr(), bytes.len() as u32, 9000, &mut out) };
        assert_eq!(
            result, 1,
            "captured valid USER request reply rejected: 0x{result:08X}"
        );
        assert_eq!(out.request, 9001);
        assert_eq!(out.code, 1); // Resolved aircraft object; not USER selector 0.
        assert!((out.values[0] - 12.068822).abs() < 0.000001);
        assert!((out.values[2] - 29.921343).abs() < 0.000001);
    }
    fn captured(hex: &str, definition: u32) -> (i32, super::Event) {
        let hex = hex.trim();
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        decode_bytes(&bytes, definition)
    }
    fn decode_bytes(bytes: &[u8], definition: u32) -> (i32, super::Event) {
        let mut out = super::Event::default();
        // SAFETY: bounded input slice and exclusive matching-layout output.
        let result = unsafe {
            rfi_sc_decode_fixture(bytes.as_ptr(), bytes.len() as u32, definition, &mut out)
        };
        (result, out)
    }
    #[test]
    fn captured_unknown_group_event_is_valid() {
        let (result, out) = captured(include_str!("../tests/fixtures/captured-event.hex"), 1);
        assert_eq!(result, 1);
        assert_eq!((out.kind, out.code, out.detail), (4, 1100, 1));
    }
    #[test]
    fn captured_data_schema_and_bounds_remain_enforced() {
        let hex = include_str!("../tests/fixtures/captured-data.hex").trim();
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        assert!(decode_bytes(&bytes, 1).0 < 0); // Original capture uses definition 9000.
        for length in 0..bytes.len() {
            assert!(decode_bytes(&bytes[..length], 9000).0 < 0);
        }
        // Explicit mutations of copies, never modifications to the raw fixture.
        for (offset, value) in [(0, 71u32), (20, 9002), (24, 1), (28, 0), (32, 2), (36, 3)] {
            let mut changed = bytes.clone();
            changed[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(decode_bytes(&changed, 9000).0 < 0, "offset={offset}");
        }
        for object in [0u32, 1, 7, 1024] {
            let mut changed = bytes.clone();
            changed[16..20].copy_from_slice(&object.to_le_bytes());
            let (result, out) = decode_bytes(&changed, 9000);
            assert_eq!(result, 1);
            assert_eq!(out.code, object); // Provider, not decoder, correlates identity.
        }
    }
    #[test]
    fn official_sdk_layout_and_malformed_packets() {
        assert_eq!(std::mem::size_of::<super::Event>(), 48);
        // SAFETY: no arguments, local C++ fixtures, no SDK connection or IO.
        assert_eq!(unsafe { rfi_sc_decoder_test() }, 0);
    }
}
