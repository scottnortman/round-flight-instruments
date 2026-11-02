//! Versioned, bounded barometer controls. Values are integer hundredths inHg.
use crate::{crc32, Error, Line, MAX_FRAME};
use core::fmt::Write;
pub const REQUEST_MAX_AGE_MS: u32 = 500;
pub const CONFIRM_MS: u64 = 2000;
pub const READBACK_TOLERANCE: f32 = 0.002;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub session: u64,
    pub id: u64,
    pub channel: u16,
    pub cents: u16,
    pub age_ms: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Sent,
    Confirmed,
    Rejected,
    Timeout,
}
impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Self::Sent => "sent",
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
            Self::Timeout => "timeout",
        }
    }
    fn parse(s: &str) -> Result<Self, Error> {
        match s {
            "sent" => Ok(Self::Sent),
            "confirmed" => Ok(Self::Confirmed),
            "rejected" => Ok(Self::Rejected),
            "timeout" => Ok(Self::Timeout),
            _ => Err(Error::Syntax),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reply {
    pub session: u64,
    pub id: u64,
    pub status: Status,
}
fn body(s: &str) -> Result<&str, Error> {
    if s.len() > MAX_FRAME {
        return Err(Error::TooLong);
    }
    if !s.bytes().all(|b| b.is_ascii_graphic() || b == b' ') {
        return Err(Error::Syntax);
    }
    let (body, crc) = s.rsplit_once(" *").ok_or(Error::Syntax)?;
    if crc.len() != 8 || !crc.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Syntax);
    }
    if u32::from_str_radix(crc, 16).map_err(|_| Error::Syntax)? != crc32(body.as_bytes()) {
        return Err(Error::Checksum);
    }
    Ok(body)
}
fn finish(mut line: Line) -> Result<Line, Error> {
    let crc = crc32(line.as_str().as_bytes());
    write!(line, " *{crc:08X}").map_err(|_| Error::TooLong)?;
    Ok(line)
}
impl Request {
    pub fn validate(self) -> Result<Self, Error> {
        if self.session == 0
            || self.id == 0
            || self.channel != 0
            || !(2810..=3100).contains(&self.cents)
            || self.age_ms > REQUEST_MAX_AGE_MS
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
            "C1 {} {} {} {} {}",
            self.session, self.id, self.channel, self.cents, self.age_ms
        )
        .map_err(|_| Error::TooLong)?;
        finish(line)
    }
    pub fn decode(s: &str) -> Result<Self, Error> {
        let mut w = body(s)?.split(' ');
        if w.next() != Some("C1") {
            return Err(Error::Syntax);
        }
        let mut next = || w.next().ok_or(Error::Syntax);
        let r = Self {
            session: next()?.parse().map_err(|_| Error::Syntax)?,
            id: next()?.parse().map_err(|_| Error::Syntax)?,
            channel: next()?.parse().map_err(|_| Error::Syntax)?,
            cents: next()?.parse().map_err(|_| Error::Syntax)?,
            age_ms: next()?.parse().map_err(|_| Error::Syntax)?,
        };
        if w.next().is_some() {
            return Err(Error::Syntax);
        }
        r.validate()
    }
}
impl Reply {
    pub fn encode(self) -> Result<Line, Error> {
        if self.session == 0 || self.id == 0 {
            return Err(Error::Range);
        }
        let mut line = Line::new();
        write!(
            line,
            "R1 {} {} {}",
            self.session,
            self.id,
            self.status.name()
        )
        .map_err(|_| Error::TooLong)?;
        finish(line)
    }
    pub fn decode(s: &str) -> Result<Self, Error> {
        let mut w = body(s)?.split(' ');
        if w.next() != Some("R1") {
            return Err(Error::Syntax);
        }
        let mut next = || w.next().ok_or(Error::Syntax);
        let r = Self {
            session: next()?.parse().map_err(|_| Error::Syntax)?,
            id: next()?.parse().map_err(|_| Error::Syntax)?,
            status: Status::parse(next()?)?,
        };
        if w.next().is_some() || r.session == 0 || r.id == 0 {
            return Err(Error::Syntax);
        }
        Ok(r)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controls_are_bounded_correlated_and_crc_protected() {
        let r = Request {
            session: 4,
            id: 9,
            channel: 0,
            cents: 2992,
            age_ms: 500,
        };
        assert_eq!(Request::decode(r.encode().unwrap().as_str()), Ok(r));
        for bad in [
            Request { session: 0, ..r },
            Request { id: 0, ..r },
            Request { channel: 1, ..r },
            Request { cents: 3101, ..r },
            Request { age_ms: 501, ..r },
        ] {
            assert!(bad.encode().is_err());
        }
        assert!(Request::decode("C1 4 9 0 2993 500 *00000000").is_err());
        for status in [
            Status::Sent,
            Status::Confirmed,
            Status::Rejected,
            Status::Timeout,
        ] {
            let r = Reply {
                session: 4,
                id: 9,
                status,
            };
            assert_eq!(Reply::decode(r.encode().unwrap().as_str()), Ok(r));
        }
    }
}
