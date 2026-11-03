//! Versioned, bounded measurement snapshots. Every field retains its own age.
use crate::{
    crc32,
    measurements::{Key, Measurement, Measurements, Quantity, Value},
    Error, Lifecycle,
};
use core::fmt::{self, Write};
pub const MAX_FRAME: usize = 512;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub session: u64,
    pub sequence: u64,
    pub receiver_ms: u64,
    pub state: Lifecycle,
    pub measurements: Measurements,
}
pub struct Line {
    bytes: [u8; MAX_FRAME],
    len: usize,
}
impl Line {
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).expect("ASCII formatter")
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
impl Frame {
    pub fn encode(self) -> Result<Line, Error> {
        if self.session == 0 || self.sequence == 0 {
            return Err(Error::Range);
        }
        let mut line = Line {
            bytes: [0; MAX_FRAME],
            len: 0,
        };
        write!(
            line,
            "T2 {} {} {} {}",
            self.session,
            self.sequence,
            self.receiver_ms,
            self.state.name()
        )
        .map_err(|_| Error::TooLong)?;
        for m in self.measurements.iter() {
            m.validate()?;
            write!(
                line,
                " {}:{}:{}:",
                m.key.quantity.wire_id(),
                m.key.channel,
                m.age_ms
            )
            .map_err(|_| Error::TooLong)?;
            match m.value {
                Value::Valid(v) => write!(line, "{v:.5}"),
                Value::Missing => line.write_str("-"),
                Value::Failed => line.write_str("!"),
            }
            .map_err(|_| Error::TooLong)?;
        }
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
        if w.next() != Some("T2") {
            return Err(Error::Syntax);
        }
        let num = |v: Option<&str>| -> Result<u64, Error> {
            let v = v.ok_or(Error::Syntax)?;
            if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
                return Err(Error::Syntax);
            }
            v.parse().map_err(|_| Error::Range)
        };
        let session = num(w.next())?;
        let sequence = num(w.next())?;
        let receiver_ms = num(w.next())?;
        if session == 0 || sequence == 0 {
            return Err(Error::Range);
        }
        let state = Lifecycle::parse(w.next().ok_or(Error::Syntax)?).ok_or(Error::Syntax)?;
        let mut measurements = Measurements::new();
        for field in w {
            let mut f = field.split(':');
            let id = u8::try_from(num(f.next())?).map_err(|_| Error::Range)?;
            let quantity = Quantity::from_wire(id).ok_or(Error::Range)?;
            let channel = u16::try_from(num(f.next())?).map_err(|_| Error::Range)?;
            let age_ms = u32::try_from(num(f.next())?).map_err(|_| Error::Range)?;
            let value = match f.next().ok_or(Error::Syntax)? {
                "-" => Value::Missing,
                "!" => Value::Failed,
                v => Value::Valid(v.parse().map_err(|_| Error::Syntax)?),
            };
            if f.next().is_some() {
                return Err(Error::Syntax);
            }
            measurements.push(Measurement {
                key: Key { quantity, channel },
                value,
                age_ms,
            })?;
        }
        Ok(Self {
            session,
            sequence,
            receiver_ms,
            state,
            measurements,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_ages_channels_and_integrity_roundtrip() {
        let mut measurements = Measurements::new();
        for (quantity, channel, value, age_ms) in [
            (Quantity::PitchDeg, 0, Value::Valid(12.5), 90),
            (Quantity::RollDeg, 0, Value::Failed, 1),
            (Quantity::PitchDeg, 1, Value::Missing, 0),
        ] {
            measurements
                .push(Measurement {
                    key: Key { quantity, channel },
                    value,
                    age_ms,
                })
                .unwrap();
        }
        let f = Frame {
            session: 1,
            sequence: 2,
            receiver_ms: 99,
            state: Lifecycle::Running,
            measurements,
        };
        let mut line = f.encode().unwrap();
        assert_eq!(Frame::decode(line.as_str()), Ok(f));
        line.bytes[3] = b'2';
        assert_eq!(Frame::decode(line.as_str()), Err(Error::Checksum));
    }
    #[test]
    fn malformed_fields_never_partially_accept() {
        for body in [
            "T2 1 1 0 running 4:0:0:NaN",
            "T2 1 1 0 running 4:0:0:91",
            "T2 1 1 0 running 4:0:0:1 4:0:0:2",
            "T2 1 1 0 running 99:0:0:1",
            "T2 1 1 0 running 4:0:0:1:2",
            "T2 0 1 0 running",
            "T2 1 1 0 running ",
        ] {
            extern crate std;
            let s = std::format!("{body} *{:08X}", crc32(body.as_bytes()));
            assert!(Frame::decode(&s).is_err(), "{body}");
        }
    }
}
