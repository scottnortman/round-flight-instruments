//! Portable device identity and versioned preference records. No live data is saved.
use crate::Instrument;
use core::fmt;

pub const SAVE_DELAY_MS: u64 = 1_000;
pub const RETRY_DELAY_MS: u64 = 5_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Uuid([u8; 16]);
impl Uuid {
    /// RFC variant UUIDs, versions 1 through 8; nil and nonstandard IDs rejected.
    pub fn from_bytes(bytes: [u8; 16]) -> Option<Self> {
        ((1..=8).contains(&(bytes[6] >> 4)) && bytes[8] & 0xc0 == 0x80).then_some(Self(bytes))
    }
    pub fn v4(mut entropy: [u8; 16]) -> Self {
        entropy[6] = (entropy[6] & 0x0f) | 0x40;
        entropy[8] = (entropy[8] & 0x3f) | 0x80;
        Self(entropy)
    }
    pub fn parse(s: &str) -> Option<Self> {
        let bytes = s.as_bytes();
        if bytes.len() != 36 {
            return None;
        }
        let mut raw = [0; 16];
        let mut n = 0;
        let mut hi = None;
        for (i, &b) in bytes.iter().enumerate() {
            if [8, 13, 18, 23].contains(&i) {
                if b != b'-' {
                    return None;
                }
                continue;
            }
            let digit = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return None,
            };
            if let Some(h) = hi.take() {
                raw[n] = h * 16 + digit;
                n += 1;
            } else {
                hi = Some(digit);
            }
        }
        Self::from_bytes(raw)
    }
}
impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, b) in self.0.iter().enumerate() {
            if [4, 6, 8, 10].contains(&i) {
                f.write_str("-")?;
            }
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Info {
    pub firmware: &'static str,
    pub uuid: Option<Uuid>,
}
impl Default for Info {
    fn default() -> Self {
        Self {
            firmware: "development",
            uuid: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Info,
    SetUuid(Uuid),
    SettingsStatus,
    Save,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordError {
    Corrupt,
    Unsupported,
}

fn crc(bytes: &[u8]) -> u32 {
    let mut value = !0u32;
    for &b in bytes {
        value ^= b as u32;
        for _ in 0..8 {
            value = (value >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(value & 1)));
        }
    }
    !value
}
fn check(bytes: &[u8], magic: &[u8; 4], size: usize) -> Result<(), RecordError> {
    if bytes.len() != size || &bytes[..4] != magic {
        return Err(RecordError::Corrupt);
    }
    if crc(&bytes[..size - 4]).to_le_bytes() != bytes[size - 4..] {
        return Err(RecordError::Corrupt);
    }
    if bytes[4] != 1 {
        return Err(RecordError::Unsupported);
    }
    Ok(())
}
pub fn encode_uuid(id: Uuid) -> [u8; 28] {
    let mut b = [0; 28];
    b[..4].copy_from_slice(b"RFIU");
    b[4] = 1;
    b[8..24].copy_from_slice(&id.0);
    let checksum = crc(&b[..24]);
    b[24..].copy_from_slice(&checksum.to_le_bytes());
    b
}
pub fn decode_uuid(b: &[u8]) -> Result<Uuid, RecordError> {
    check(b, b"RFIU", 28)?;
    if b[5..8] != [0; 3] {
        return Err(RecordError::Corrupt);
    }
    Uuid::from_bytes(b[8..24].try_into().unwrap()).ok_or(RecordError::Corrupt)
}
// Explicit wire IDs: enum declaration order is not a storage format.
pub fn encode_selection(gauge: Instrument) -> [u8; 12] {
    let mut b = [0; 12];
    b[..4].copy_from_slice(b"RFIP");
    b[4] = 1;
    b[5] = match gauge {
        Instrument::Altimeter => 0,
        Instrument::Attitude => 1,
        Instrument::Airspeed => 2,
        Instrument::Heading => 3,
        Instrument::Turn => 4,
        Instrument::Vsi => 5,
    };
    let checksum = crc(&b[..8]);
    b[8..].copy_from_slice(&checksum.to_le_bytes());
    b
}
pub fn decode_selection(b: &[u8]) -> Result<Instrument, RecordError> {
    check(b, b"RFIP", 12)?;
    if b[6..8] != [0; 2] {
        return Err(RecordError::Corrupt);
    }
    match b[5] {
        0 => Ok(Instrument::Altimeter),
        1 => Ok(Instrument::Attitude),
        2 => Ok(Instrument::Airspeed),
        3 => Ok(Instrument::Heading),
        4 => Ok(Instrument::Turn),
        5 => Ok(Instrument::Vsi),
        _ => Err(RecordError::Corrupt),
    }
}

/// Coalesces selection changes. A completed old save cannot mark a new choice saved.
pub struct SelectionSave {
    pub selected: Instrument,
    pub saved: Option<Instrument>,
    changed_at: u64,
    retry_at: u64,
    force: bool,
}
impl SelectionSave {
    pub fn new(saved: Option<Instrument>) -> Self {
        Self {
            selected: saved.unwrap_or(Instrument::Altimeter),
            saved,
            changed_at: 0,
            retry_at: 0,
            force: false,
        }
    }
    pub fn observe(&mut self, selected: Instrument, now: u64) {
        if self.selected != selected {
            self.selected = selected;
            self.changed_at = now;
            self.force = false;
        }
    }
    pub fn flush(&mut self) {
        self.force = self.dirty();
    }
    pub fn dirty(&self) -> bool {
        self.saved != Some(self.selected)
    }
    pub fn due(&self, now: u64) -> Option<Instrument> {
        (self.dirty()
            && now >= self.retry_at
            && (self.force || now.saturating_sub(self.changed_at) >= SAVE_DELAY_MS))
            .then_some(self.selected)
    }
    pub fn completed(&mut self, gauge: Instrument, success: bool, now: u64) {
        if self.selected == gauge {
            self.force = false;
        }
        if success {
            self.saved = Some(gauge);
            self.retry_at = 0;
        } else {
            self.retry_at = now.saturating_add(RETRY_DELAY_MS);
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::string::ToString;
    #[test]
    fn uuid_validation_and_canonical_format() {
        let text = "11223344-5566-4788-9aab-ccddeeff0011";
        let id = Uuid::parse(text).unwrap();
        assert_eq!(id.to_string(), text);
        assert_eq!(Uuid::parse(&text.to_ascii_uppercase()), Some(id));
        for bad in [
            "",
            "11223344556647889aabccddeeff0011",
            "11223344-5566-0788-9aab-ccddeeff0011",
            "11223344-5566-4788-7aab-ccddeeff0011",
            "00000000-0000-0000-0000-000000000000",
        ] {
            assert!(Uuid::parse(bad).is_none());
        }
        let id = Uuid::v4([0xff; 16]);
        assert_eq!(id.0[6], 0x4f);
        assert_eq!(id.0[8], 0xbf);
    }
    #[test]
    fn records_reject_torn_corrupt_and_future_data() {
        let id = Uuid::v4([21; 16]);
        let bytes = encode_uuid(id);
        assert_eq!(decode_uuid(&bytes), Ok(id));
        for i in 0..bytes.len() {
            assert!(decode_uuid(&bytes[..i]).is_err());
            let mut bad = bytes;
            bad[i] ^= 1;
            assert!(decode_uuid(&bad).is_err());
        }
        for gauge in Instrument::ALL {
            let bytes = encode_selection(gauge);
            assert_eq!(decode_selection(&bytes), Ok(gauge));
            for i in 0..bytes.len() {
                let mut bad = bytes;
                bad[i] ^= 1;
                assert!(decode_selection(&bad).is_err());
            }
        }
        let mut future = encode_selection(Instrument::Turn);
        future[4] = 2;
        let sum = crc(&future[..8]);
        future[8..].copy_from_slice(&sum.to_le_bytes());
        assert_eq!(decode_selection(&future), Err(RecordError::Unsupported));
    }
    #[test]
    fn saves_coalesce_skip_unchanged_and_preserve_newer_edits_on_completion() {
        let mut p = SelectionSave::new(Some(Instrument::Altimeter));
        assert_eq!(p.due(5000), None);
        p.observe(Instrument::Turn, 5000);
        p.observe(Instrument::Attitude, 5500);
        assert_eq!(p.due(6499), None);
        assert_eq!(p.due(6500), Some(Instrument::Attitude));
        p.observe(Instrument::Vsi, 6510);
        p.completed(Instrument::Attitude, true, 6520);
        assert!(p.dirty());
        assert_eq!(p.due(7509), None);
        assert_eq!(p.due(7510), Some(Instrument::Vsi));
        p.completed(Instrument::Vsi, false, 7520);
        p.flush();
        assert_eq!(p.due(12519), None);
        assert_eq!(p.due(12520), Some(Instrument::Vsi));
        p.completed(Instrument::Vsi, true, 12521);
        assert!(!p.dirty());
        assert_eq!(p.due(u64::MAX), None);
    }
    #[test]
    fn unchanged_flush_does_not_force_a_future_choice_or_cancel_a_newer_flush() {
        let mut p = SelectionSave::new(Some(Instrument::Altimeter));
        p.flush();
        p.observe(Instrument::Turn, 10);
        assert_eq!(p.due(20), None);
        p.flush();
        assert_eq!(p.due(20), Some(Instrument::Turn));
        // A new choice gets a new debounce even if the previous one was forced.
        p.observe(Instrument::Attitude, 30);
        assert_eq!(p.due(31), None);
        // An explicit new flush during an older write must remain pending.
        p.flush();
        p.completed(Instrument::Turn, true, 32);
        assert_eq!(p.due(33), Some(Instrument::Attitude));
    }
}
