//! Simulator-neutral measurements and a bounded altimeter wire profile.
#![no_std]
#![forbid(unsafe_code)]
use core::fmt::{self, Write};
pub mod control;
pub mod measurements;
pub const MAX_FRAME: usize = 256;
pub const STALE_MS: u64 = 1000;
pub const RECEIVER_LEASE_MS: u64 = 250;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Synthetic,
    Replay,
    Msfs2020,
    Dcs,
}
impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Self::Synthetic => "synthetic",
            Self::Replay => "replay",
            Self::Msfs2020 => "msfs2020",
            Self::Dcs => "dcs",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        [Self::Synthetic, Self::Replay, Self::Msfs2020, Self::Dcs]
            .into_iter()
            .find(|p| p.name() == s)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    Running,
    Paused,
    Loading,
    Disconnected,
}
impl Lifecycle {
    pub fn name(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Loading => "loading",
            Self::Disconnected => "disconnected",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        [
            Self::Running,
            Self::Paused,
            Self::Loading,
            Self::Disconnected,
        ]
        .into_iter()
        .find(|p| p.name() == s)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Datum {
    Value(f32),
    Missing,
    Failed,
}
impl Datum {
    pub fn parse(s: &str) -> Result<Self, Error> {
        match s {
            "-" => Ok(Self::Missing),
            "!" => Ok(Self::Failed),
            _ => s.parse().map(Self::Value).map_err(|_| Error::Syntax),
        }
    }
    fn valid(self, low: f32, high: f32) -> bool {
        match self {
            Self::Value(v) => v.is_finite() && (low..=high).contains(&v),
            _ => true,
        }
    }
}
impl fmt::Display for Datum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Value(v) => write!(f, "{v:.3}"),
            Self::Missing => f.write_str("-"),
            Self::Failed => f.write_str("!"),
        }
    }
}
/// Age is time already elapsed before receipt, not a remote clock timestamp.
/// Each frame is one complete snapshot; Missing clears that field immediately.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub session: u64,
    pub sequence: u64,
    /// Receiver monotonic time from the latest acknowledgement. Never host time.
    pub receiver_ms: u64,
    pub age_ms: u32,
    pub state: Lifecycle,
    pub indicated_ft: Datum,
    pub pressure_ft: Datum,
    pub baro_inhg: Datum,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Syntax,
    Checksum,
    Range,
    TooLong,
}
impl Frame {
    pub fn validate(self) -> Result<Self, Error> {
        if self.session == 0
            || self.sequence == 0
            || !self.indicated_ft.valid(-3000., 65000.)
            || !self.pressure_ft.valid(-2000., 60000.)
            || !self.baro_inhg.valid(28.1, 31.)
        {
            Err(Error::Range)
        } else {
            Ok(self)
        }
    }
    pub fn encode(self) -> Result<Line, Error> {
        self.validate()?;
        let mut line = Line::new();
        write!(
            line,
            "T1 {} {} {} {} {} {} {} {}",
            self.session,
            self.sequence,
            self.receiver_ms,
            self.age_ms,
            self.state.name(),
            self.indicated_ft,
            self.pressure_ft,
            self.baro_inhg
        )
        .map_err(|_| Error::TooLong)?;
        let crc = crc32(line.as_str().as_bytes());
        write!(line, " *{crc:08X}").map_err(|_| Error::TooLong)?;
        Ok(line)
    }
    pub fn decode(s: &str) -> Result<Self, Error> {
        if s.len() > MAX_FRAME {
            return Err(Error::TooLong);
        }
        if !s.bytes().all(|b| b.is_ascii_graphic() || b == b' ') {
            return Err(Error::Syntax);
        }
        let (body, check) = s.rsplit_once(" *").ok_or(Error::Syntax)?;
        if check.len() != 8 || !check.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Syntax);
        }
        if u32::from_str_radix(check, 16).map_err(|_| Error::Syntax)? != crc32(body.as_bytes()) {
            return Err(Error::Checksum);
        }
        let mut w = body.split(' ');
        if w.next() != Some("T1") {
            return Err(Error::Syntax);
        }
        let mut next = || w.next().ok_or(Error::Syntax);
        let result = Self {
            session: next()?.parse().map_err(|_| Error::Syntax)?,
            sequence: next()?.parse().map_err(|_| Error::Syntax)?,
            receiver_ms: next()?.parse().map_err(|_| Error::Syntax)?,
            age_ms: next()?.parse().map_err(|_| Error::Syntax)?,
            state: Lifecycle::parse(next()?).ok_or(Error::Syntax)?,
            indicated_ft: Datum::parse(next()?)?,
            pressure_ft: Datum::parse(next()?)?,
            baro_inhg: Datum::parse(next()?)?,
        };
        if w.next().is_some() {
            return Err(Error::Syntax);
        }
        result.validate()
    }
}
/// CRC-32/ISO-HDLC, standard check vector 123456789 -> CBF43926.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for b in bytes {
        crc ^= *b as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB88320u32 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}
pub struct Line {
    bytes: [u8; MAX_FRAME],
    len: usize,
}
impl Default for Line {
    fn default() -> Self {
        Self::new()
    }
}
impl Line {
    pub const fn new() -> Self {
        Self {
            bytes: [0; MAX_FRAME],
            len: 0,
        }
    }
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).expect("only valid UTF8 written")
    }
}
impl Write for Line {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.len + s.len() > MAX_FRAME {
            return Err(fmt::Error);
        }
        self.bytes[self.len..self.len + s.len()].copy_from_slice(s.as_bytes());
        self.len += s.len();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_integrity_and_invalid_values() {
        assert_eq!(crc32(b"123456789"), 0xCBF43926);
        let f = Frame {
            session: u64::MAX,
            sequence: u64::MAX,
            receiver_ms: u64::MAX,
            age_ms: u32::MAX,
            state: Lifecycle::Running,
            indicated_ft: Datum::Value(0.),
            pressure_ft: Datum::Missing,
            baro_inhg: Datum::Failed,
        };
        let encoded = f.encode().unwrap();
        assert_eq!(Frame::decode(encoded.as_str()), Ok(f));
        let mut damaged = encoded;
        damaged.bytes[3] = b'0';
        assert_eq!(Frame::decode(damaged.as_str()), Err(Error::Checksum));
        for value in [f32::NAN, f32::INFINITY, 65001., -3001.] {
            assert_eq!(
                Frame {
                    indicated_ft: Datum::Value(value),
                    ..f
                }
                .encode()
                .err(),
                Some(Error::Range)
            );
        }
        assert!(Frame::decode("T2 1 1 0 running 0 0 29.92 *00000000").is_err());
    }
}
