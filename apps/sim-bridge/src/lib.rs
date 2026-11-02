//! Host-side provider boundary. Simulator SDKs stay behind this interface.
#![forbid(unsafe_code)]
pub mod msfs;
use simulator_link::measurements::{Key, Measurement, Measurements, Quantity, Value};
use simulator_link::{Datum, Frame, Lifecycle, Provider};
use std::{io, io::BufRead};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    SetBarometer,
}
#[derive(Clone, Copy, Debug)]
pub struct Capabilities<'a> {
    pub readings: &'a [Key],
    pub controls: &'a [(Control, u16)],
}
#[derive(Clone, Copy, Debug)]
pub struct Observation {
    pub at_ms: u64,
    pub state: Lifecycle,
    pub measurements: Measurements,
}
impl Observation {
    pub fn altimeter(
        at_ms: u64,
        state: Lifecycle,
        indicated: Datum,
        pressure: Datum,
        baro: Datum,
    ) -> Result<Self, simulator_link::Error> {
        let mut measurements = Measurements::new();
        for (quantity, datum) in [
            (Quantity::IndicatedAltitudeFt, indicated),
            (Quantity::PressureAltitudeFt, pressure),
            (Quantity::BarometerInHg, baro),
        ] {
            let value = match datum {
                Datum::Value(v) => Value::Valid(v as f64),
                Datum::Missing => Value::Missing,
                Datum::Failed => Value::Failed,
            };
            measurements.push(Measurement {
                key: Key::primary(quantity),
                value,
                age_ms: 0,
            })?;
        }
        Ok(Self {
            at_ms,
            state,
            measurements,
        })
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum ControlError {
    Unsupported,
    Invalid,
}
/// A provider has ended its old session and permits a completely new read-only
/// connection. Protocol/schema/value errors deliberately do not grant this hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryReason {
    SessionChanged,
    ConnectionLost,
}
/// Polling must return only a newly acquired observation. None never refreshes
/// a previous sample. Aircraft changes require a new bridge/board session.
pub trait SimulatorProvider {
    fn kind(&self) -> Provider;
    fn capabilities(&self) -> Capabilities<'_>;
    fn poll(&mut self, now_ms: u64) -> Option<Observation>;
    fn recovery_reason(&self) -> Option<RecoveryReason> {
        None
    }
    fn enable_controls(&mut self, _session: u64) -> Result<(), ControlError> {
        Err(ControlError::Unsupported)
    }
    fn control(
        &mut self,
        _request: simulator_link::control::Request,
        _now: u64,
    ) -> Result<simulator_link::control::Reply, ControlError> {
        Err(ControlError::Unsupported)
    }
    fn control_status(&self) -> Option<simulator_link::control::Reply> {
        None
    }
}
const READ_ONLY: Capabilities<'static> = Capabilities {
    readings: &[
        Key::primary(Quantity::IndicatedAltitudeFt),
        Key::primary(Quantity::PressureAltitudeFt),
        Key::primary(Quantity::BarometerInHg),
    ],
    controls: &[],
};
pub struct Synthetic {
    duration_ms: u64,
    last: Option<u64>,
    finished: bool,
    endpoint_sent: bool,
}
impl Synthetic {
    pub fn new(duration_ms: u64) -> io::Result<Self> {
        if !(1000..=600000).contains(&duration_ms) {
            return Err(io::Error::other("duration must be 1000..600000 ms"));
        }
        Ok(Self {
            duration_ms,
            last: None,
            finished: false,
            endpoint_sent: false,
        })
    }
}
impl SimulatorProvider for Synthetic {
    fn kind(&self) -> Provider {
        Provider::Synthetic
    }
    fn capabilities(&self) -> Capabilities<'_> {
        READ_ONLY
    }
    fn poll(&mut self, now: u64) -> Option<Observation> {
        if self.finished || self.last.is_some_and(|last| now.saturating_sub(last) < 33) {
            return None;
        }
        self.last = Some(now);
        if self.endpoint_sent {
            self.finished = true;
            return Some(Observation {
                at_ms: now,
                state: Lifecycle::Disconnected,
                measurements: Measurements::new(),
            });
        }
        self.endpoint_sent = now >= self.duration_ms;
        let position = now.min(self.duration_ms) as f32 / self.duration_ms as f32;
        // Deliberately distinct pressure/indicated values detect accidental
        // reapplication of the firmware's local barometric correction.
        Some(
            Observation::altimeter(
                now,
                Lifecycle::Running,
                Datum::Value(10000. * position),
                Datum::Value(10000. * position - 200.),
                Datum::Value(30.12),
            )
            .expect("bounded synthetic values"),
        )
    }
}
pub struct Replay {
    records: Vec<Observation>,
    next: usize,
    finished: bool,
}
impl Replay {
    /// Bounded file: max 100k rows, 256 bytes/row, strictly increasing ms.
    /// at_ms state indicated_ft pressure_ft baro_inhg; '-' missing, '!' failed.
    pub fn read(mut reader: impl BufRead) -> io::Result<Self> {
        let mut records = Vec::new();
        let mut bytes = Vec::new();
        loop {
            bytes.clear();
            // Read one byte at a time so a malformed line cannot grow unbounded.
            loop {
                let buf = reader.fill_buf()?;
                if buf.is_empty() {
                    break;
                }
                let b = buf[0];
                reader.consume(1);
                if b == b'\n' {
                    break;
                }
                if bytes.len() == 256 {
                    return Err(io::Error::other("replay line too long"));
                }
                bytes.push(b);
            }
            if bytes.is_empty() && reader.fill_buf()?.is_empty() {
                break;
            }
            let line = std::str::from_utf8(&bytes)
                .map_err(io::Error::other)?
                .trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let w: Vec<_> = line.split_ascii_whitespace().collect();
            if w.len() != 5 || records.len() >= 100000 {
                return Err(io::Error::other("invalid replay row/count"));
            }
            let at_ms = w[0].parse::<u64>().map_err(io::Error::other)?;
            if records
                .last()
                .is_some_and(|r: &Observation| r.at_ms >= at_ms)
            {
                return Err(io::Error::other("replay time must increase"));
            }
            let state = Lifecycle::parse(w[1]).ok_or_else(|| io::Error::other("invalid state"))?;
            let read = |s| Datum::parse(s).map_err(|_| io::Error::other("invalid datum"));
            let o = Observation::altimeter(at_ms, state, read(w[2])?, read(w[3])?, read(w[4])?)
                .map_err(|_| io::Error::other("invalid replay measurement"))?;
            to_frame(o, 1, 1, at_ms)
                .map_err(|_| io::Error::other("replay value outside altimeter profile"))?;
            if state == Lifecycle::Disconnected && !reader.fill_buf()?.is_empty() {
                return Err(io::Error::other("disconnected must be final replay row"));
            }
            records.push(o);
        }
        if records.is_empty() {
            return Err(io::Error::other("empty replay"));
        }
        Ok(Self {
            records,
            next: 0,
            finished: false,
        })
    }
}
impl SimulatorProvider for Replay {
    fn kind(&self) -> Provider {
        Provider::Replay
    }
    fn capabilities(&self) -> Capabilities<'_> {
        READ_ONLY
    }
    fn poll(&mut self, now: u64) -> Option<Observation> {
        if self.finished {
            return None;
        }
        if self.next == self.records.len() {
            self.finished = true;
            return Some(Observation {
                at_ms: now,
                state: Lifecycle::Disconnected,
                measurements: Measurements::new(),
            });
        }
        let mut latest = None;
        while self.next < self.records.len() && self.records[self.next].at_ms <= now {
            latest = Some(self.records[self.next]);
            self.next += 1;
        }
        if latest.is_some_and(|r| r.state == Lifecycle::Disconnected) {
            self.finished = true;
        }
        latest
    }
}
/// Project selected channel 0 into the current board's compact altimeter profile.
/// The provider boundary remains independent of this device/profile. Out-of-range
/// values are rejected, never clamped to the current gauge scale.
pub fn to_frame(
    o: Observation,
    session: u64,
    sequence: u64,
    now: u64,
) -> Result<Frame, simulator_link::Error> {
    let field = |quantity| o.measurements.get(Key::primary(quantity));
    let indicated = field(Quantity::IndicatedAltitudeFt);
    let pressure = field(Quantity::PressureAltitudeFt);
    let baro = field(Quantity::BarometerInHg);
    let oldest = [indicated, pressure, baro]
        .iter()
        .flatten()
        .filter(|m| matches!(m.value, Value::Valid(_)))
        .map(|m| m.age_ms)
        .max()
        .unwrap_or(0);
    Frame {
        session,
        sequence,
        receiver_ms: 0,
        age_ms: (now
            .checked_sub(o.at_ms)
            .unwrap_or(u32::MAX as u64)
            .min(u32::MAX as u64) as u32)
            .saturating_add(oldest),
        state: o.state,
        indicated_ft: indicated.map_or(Datum::Missing, |m| m.datum()),
        pressure_ft: pressure.map_or(Datum::Missing, |m| m.datum()),
        baro_inhg: baro.map_or(Datum::Missing, |m| m.datum()),
    }
    .validate()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn instrument_projection_preserves_age_channels_and_missing_fields() {
        let mut o = Observation::altimeter(
            100,
            Lifecycle::Running,
            Datum::Value(8500.),
            Datum::Missing,
            Datum::Value(30.12),
        )
        .unwrap();
        o.measurements
            .push(Measurement {
                key: Key::primary(Quantity::PitchDeg),
                value: Value::Valid(5.),
                age_ms: 0,
            })
            .unwrap();
        o.measurements
            .push(Measurement {
                key: Key {
                    quantity: Quantity::IndicatedAltitudeFt,
                    channel: 1,
                },
                value: Value::Valid(9000.),
                age_ms: 900,
            })
            .unwrap();
        let f = to_frame(o, 1, 1, 150).unwrap();
        assert_eq!(f.indicated_ft, Datum::Value(8500.));
        assert_eq!(f.age_ms, 50); // Other channels do not contaminate channel 0.
        let mut m = Measurements::new();
        m.push(Measurement {
            key: Key::primary(Quantity::BarometerInHg),
            value: Value::Valid(30.12),
            age_ms: 800,
        })
        .unwrap();
        m.push(Measurement {
            key: Key::primary(Quantity::PitchDeg),
            value: Value::Valid(5.),
            age_ms: 0,
        })
        .unwrap();
        let f = to_frame(
            Observation {
                measurements: m,
                ..o
            },
            1,
            2,
            150,
        )
        .unwrap();
        assert_eq!(f.indicated_ft, Datum::Missing); // New pitch does not carry old altitude.
        assert_eq!(f.age_ms, 850); // New pitch does not freshen older QNH.
        assert_eq!(to_frame(o, 1, 3, 99).unwrap().age_ms, u32::MAX);
    }
    #[test]
    fn replay_drops_backlog_preserves_age_and_does_not_refresh_holds() {
        let mut r=Replay::read(&b"0 running 0 - 29.92\n100 running 100 0 30.12\n200 paused - - -\n1000 running 1000 800 30.12\n"[..]).unwrap();
        let a = r.poll(150).unwrap();
        assert_eq!(a.at_ms, 100);
        assert_eq!(to_frame(a, 1, 1, 150).unwrap().age_ms, 50);
        assert!(r.poll(160).is_none());
        assert_eq!(r.poll(300).unwrap().state, Lifecycle::Paused);
        assert_eq!(r.poll(1200).unwrap().at_ms, 1000);
        assert_eq!(r.poll(1233).unwrap().state, Lifecycle::Disconnected);
        assert!(r.poll(1300).is_none());
        assert_eq!(
            r.control(
                simulator_link::control::Request {
                    id: 1,
                    session: 1,
                    channel: 0,
                    cents: 2992,
                    age_ms: 0,
                },
                0
            ),
            Err(ControlError::Unsupported)
        );
    }
    #[test]
    fn replay_rejects_bad_rows_and_synthetic_has_no_writable_controls() {
        for bytes in [
            "0 running NaN 0 29.92\n",
            "1 running 0 0 29.92\n0 running 1 0 29.92\n",
            "0 running 0 0 32\n",
            "",
        ] {
            assert!(Replay::read(bytes.as_bytes()).is_err());
        }
        let mut p = Synthetic::new(1000).unwrap();
        assert!(p.capabilities().controls.is_empty());
        assert_eq!(
            to_frame(p.poll(0).unwrap(), 1, 1, 0).unwrap().indicated_ft,
            Datum::Value(0.)
        );
        assert!(p.poll(20).is_none());
        assert_eq!(
            to_frame(p.poll(1000).unwrap(), 1, 2, 1000)
                .unwrap()
                .indicated_ft,
            Datum::Value(10000.)
        );
        assert_eq!(p.poll(1050).unwrap().state, Lifecycle::Disconnected);
    }
}
