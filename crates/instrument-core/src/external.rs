use crate::{ApplyError, Quality, Reading};
use simulator_link::{Datum, Frame, Lifecycle, Provider, STALE_MS};
#[derive(Clone, Copy, Debug)]
pub struct External {
    pub provider: Provider,
    pub session: u64,
    sequence: u64,
    sample: Option<(Frame, u64)>,
    opened_ms: u64,
    pub multi_profile: bool,
    multi: Option<(simulator_link::multi::Frame, u64)>,
}
impl External {
    pub const fn new(provider: Provider, session: u64) -> Self {
        Self::new_at(provider, session, 0)
    }
    pub const fn new_at(provider: Provider, session: u64, opened_ms: u64) -> Self {
        Self {
            provider,
            session,
            sequence: 0,
            sample: None,
            opened_ms,
            multi_profile: false,
            multi: None,
        }
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn received_ms(&self) -> Option<u64> {
        self.sample.map(|(_, at)| at)
    }
    pub fn clear_altitude(&mut self) {
        if let Some((f, _)) = &mut self.sample {
            f.indicated_ft = Datum::Missing;
            f.pressure_ft = Datum::Missing;
        }
        if let Some((f, _)) = &mut self.multi {
            use simulator_link::measurements::{Measurements, Quantity, Value};
            let mut cleared = Measurements::new();
            for mut m in f.measurements.iter().copied() {
                if matches!(
                    m.key.quantity,
                    Quantity::IndicatedAltitudeFt | Quantity::PressureAltitudeFt
                ) {
                    m.value = Value::Missing;
                }
                cleared
                    .push(m)
                    .expect("existing bounded validated measurements");
            }
            f.measurements = cleared;
        }
    }
    pub fn state(&self) -> Lifecycle {
        self.sample.map_or(Lifecycle::Loading, |(f, _)| f.state)
    }
    pub fn age(&self, now: u64) -> Option<u64> {
        let (f, at) = self.sample?;
        Some(now.checked_sub(at)?.saturating_add(f.age_ms as u64))
    }
    /// A session that never receives its first frame must also expire. Keep
    /// measurement age absent until an actual measurement frame arrives.
    pub fn stale(&self, now: u64) -> bool {
        self.age(now)
            .or_else(|| {
                if self.sample.is_none() {
                    now.checked_sub(self.opened_ms)
                } else {
                    None
                }
            })
            .is_some_and(|age| age >= STALE_MS)
    }
    pub fn accept(&mut self, frame: Frame, now: u64) -> Result<(), ApplyError> {
        if self.multi_profile {
            return Err(ApplyError::Unsupported);
        }
        self.accept_base(frame, now)
    }
    fn accept_base(&mut self, mut frame: Frame, now: u64) -> Result<(), ApplyError> {
        frame.validate().map_err(|_| ApplyError::OutOfRange)?;
        let elapsed = now
            .checked_sub(frame.receiver_ms)
            .ok_or(ApplyError::NotReady)?;
        if elapsed > simulator_link::RECEIVER_LEASE_MS {
            return Err(ApplyError::NotReady);
        }
        // Conservative transit age also covers OS/UART queues and partial lines.
        frame.age_ms = frame.age_ms.saturating_add(elapsed as u32);
        if frame.session != self.session || frame.sequence <= self.sequence {
            return Err(ApplyError::OutOfRange);
        }
        // Disconnected ends this session; a new explicit session is required.
        if self.state() == Lifecycle::Disconnected || self.sample.is_some_and(|(_, at)| now < at) {
            return Err(ApplyError::NotReady);
        }
        self.sequence = frame.sequence;
        self.sample = Some((frame, now));
        Ok(())
    }
    pub fn accept_multi(
        &mut self,
        frame: simulator_link::multi::Frame,
        now: u64,
    ) -> Result<(), ApplyError> {
        if !self.multi_profile {
            return Err(ApplyError::Unsupported);
        }
        let base = Frame {
            session: frame.session,
            sequence: frame.sequence,
            receiver_ms: frame.receiver_ms,
            age_ms: 0,
            state: frame.state,
            // Reuse only session/lifecycle/lease checks from T1. Its narrow
            // altimeter display ranges must not invalidate another instrument.
            indicated_ft: Datum::Missing,
            pressure_ft: Datum::Missing,
            baro_inhg: Datum::Missing,
        };
        // Validate the full snapshot before changing any state, including values
        // constructed directly by a provider instead of parsed from the wire.
        for m in frame.measurements.iter() {
            m.validate().map_err(|_| ApplyError::OutOfRange)?;
        }
        self.accept_base(base, now)?;
        self.multi = Some((frame, now));
        Ok(())
    }
    pub fn read_quantity(
        &self,
        now: u64,
        fault: Option<Quality>,
        quantity: simulator_link::measurements::Quantity,
    ) -> Reading {
        use simulator_link::measurements::{Key, Value};
        let item = self.multi.as_ref().and_then(|(f, at)| {
            f.measurements
                .get(Key::primary(quantity))
                .map(|m| (f, *at, m))
        });
        let age = item.and_then(|(f, at, m)| {
            now.checked_sub(at)
                .zip(at.checked_sub(f.receiver_ms))
                .map(|(local, transit)| {
                    local
                        .saturating_add(transit)
                        .saturating_add(u64::from(m.age_ms))
                })
        });
        let quality = if now < self.opened_ms || (item.is_some() && age.is_none()) {
            Quality::TimingFault
        } else if let Some(f) = fault {
            f
        } else if age.is_some_and(|a| a >= u64::from(quantity.stale_after_ms())) {
            Quality::Stale
        } else if self.state() != Lifecycle::Running {
            Quality::Missing
        } else {
            match item.map(|(_, _, m)| m.value) {
                Some(Value::Valid(_)) => Quality::Valid,
                Some(Value::Failed) => Quality::Failed,
                _ => Quality::Missing,
            }
        };
        Reading {
            quality,
            age_ms: age,
            value: if quality == Quality::Valid {
                item.and_then(|(_, _, m)| match m.value {
                    Value::Valid(v) => Some(v as f32),
                    _ => None,
                })
            } else {
                None
            },
        }
    }
    pub fn read(
        &self,
        now: u64,
        fault: Option<Quality>,
        field: impl FnOnce(Frame) -> Datum,
    ) -> Reading {
        let age = self.age(now);
        let datum = self.sample.map_or(Datum::Missing, |(f, _)| field(f));
        let quality = if now < self.opened_ms || (self.sample.is_some() && age.is_none()) {
            Quality::TimingFault
        } else if let Some(fault) = fault {
            fault
        } else if self.stale(now) {
            Quality::Stale
        } else if self.state() != Lifecycle::Running {
            Quality::Missing
        } else {
            match datum {
                Datum::Value(_) => Quality::Valid,
                Datum::Missing => Quality::Missing,
                Datum::Failed => Quality::Failed,
            }
        };
        Reading {
            quality,
            age_ms: age,
            value: match (quality, datum) {
                (Quality::Valid, Datum::Value(v)) => Some(v),
                _ => None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{App, Command, Field};
    #[test]
    fn multi_gauge_samples_keep_independent_age_and_clear_omitted_fields() {
        use simulator_link::{
            measurements::{Key, Measurement, Measurements, Quantity, Value},
            multi,
        };
        let mut app = App::new();
        app.tick(0);
        app.apply(Command::SourceExternal2(Provider::Msfs2020, 1))
            .unwrap();
        let mut measurements = Measurements::new();
        for (quantity, value, age_ms) in [
            (Quantity::IndicatedAltitudeFt, 4520., 0),
            (Quantity::PressureAltitudeFt, 4520., 0),
            (Quantity::BarometerInHg, 29.92, 0),
            (Quantity::PitchDeg, 10., 90),
            (Quantity::RollDeg, 20., 0),
        ] {
            measurements
                .push(Measurement {
                    key: Key::primary(quantity),
                    value: Value::Valid(value),
                    age_ms,
                })
                .unwrap();
        }
        let frame = multi::Frame {
            session: 1,
            sequence: 1,
            receiver_ms: 0,
            state: Lifecycle::Running,
            measurements,
        };
        app.apply(Command::Telemetry2(frame)).unwrap();
        assert_eq!(app.reading(Field::PitchDeg, 0).value, Some(10.));
        app.tick(10);
        assert_eq!(app.reading(Field::PitchDeg, 59).quality, Quality::Valid);
        assert_eq!(app.reading(Field::PitchDeg, 59).age_ms, Some(149));
        assert_eq!(app.reading(Field::PitchDeg, 109).quality, Quality::Valid);
        assert_eq!(app.reading(Field::PitchDeg, 109).age_ms, Some(199));
        assert_eq!(app.reading(Field::PitchDeg, 110).quality, Quality::Stale);
        assert_eq!(app.reading(Field::RollDeg, 10).quality, Quality::Valid);
        assert_eq!(app.altimeter_reading(10).quality, Quality::Valid);
        app.apply(Command::Telemetry2(multi::Frame {
            sequence: 2,
            receiver_ms: 10,
            measurements: Measurements::new(),
            ..frame
        }))
        .unwrap();
        assert_eq!(app.reading(Field::RollDeg, 10).quality, Quality::Missing);
        assert!(app.apply(Command::Telemetry2(frame)).is_err());
        assert_eq!(app.altimeter_reading(10).quality, Quality::Missing);
        assert!(app
            .apply(Command::Telemetry(super::tests::frame(3)))
            .is_err());
    }
    #[test]
    fn abandoned_handshake_expires_without_inventing_a_sample() {
        let mut a = App::new();
        for t in (0..=5000).step_by(10) {
            a.tick(t);
        }
        a.apply(Command::SourceExternal(Provider::Msfs2020, 10))
            .unwrap();
        assert_eq!(a.source_label(), "LOADING");
        for t in (5010..6000).step_by(10) {
            a.tick(t);
        }
        assert_eq!(a.source_label(), "LOADING");
        a.tick(6000);
        assert_eq!(a.source_label(), "LINK STALE");
        assert_eq!(a.altimeter_reading(6000).quality, Quality::Stale);
        assert_eq!(a.altimeter_reading(6000).value, None);
        assert_eq!(a.altimeter_reading(6000).age_ms, None);
        a.apply(Command::Telemetry(Frame {
            receiver_ms: 6000,
            ..frame(1)
        }))
        .unwrap();
        assert_eq!(a.source_label(), "MSFS 2020");
        assert_eq!(a.altimeter_reading(6000).value, Some(8500.));
    }
    fn frame(sequence: u64) -> Frame {
        Frame {
            session: 10,
            sequence,
            receiver_ms: 0,
            age_ms: 20,
            state: Lifecycle::Running,
            indicated_ft: Datum::Value(8500.),
            pressure_ft: Datum::Value(8300.),
            baro_inhg: Datum::Value(30.12),
        }
    }
    #[test]
    fn live_samples_age_without_republication_and_session_switch_clears_demo() {
        let mut a = App::new();
        a.apply(Command::LevelPreset).unwrap();
        a.tick(0);
        a.apply(Command::SourceExternal(Provider::Replay, 10))
            .unwrap();
        assert_eq!(a.altimeter_reading(0).quality, Quality::Missing);
        a.apply(Command::Telemetry(frame(1))).unwrap();
        for t in (20..=1000).step_by(20) {
            a.tick(t);
        }
        assert_eq!(a.altimeter_reading(979).value, Some(8500.));
        assert_eq!(a.altimeter_reading(980).quality, Quality::Stale);
        assert_eq!(a.reading(Field::PitchDeg, 20).value, None);
        assert!(a.apply(Command::Telemetry(frame(1))).is_err());
        assert_eq!(a.altimeter_reading(1000).age_ms, Some(1020));
        assert!(a.apply(Command::Baro(29.92)).is_err());
        assert!(a.apply(Command::LevelPreset).is_err());
        a.apply(Command::SourceDemo).unwrap();
        a.tick(1020);
        assert_eq!(a.altimeter_reading(1020).value, None);
        assert!(a
            .apply(Command::SourceExternal(Provider::Replay, 10))
            .is_err());
        a.apply(Command::SourceExternal(Provider::Replay, 11))
            .unwrap();
        assert!(a.apply(Command::Telemetry(frame(2))).is_err());
    }
    #[test]
    fn delayed_or_future_clock_credit_cannot_refresh_any_field() {
        let mut e = External::new(Provider::Replay, 10);
        e.accept(frame(1), 0).unwrap();
        assert!(e.accept(frame(2), 251).is_err());
        assert!(e
            .accept(
                Frame {
                    receiver_ms: 300,
                    ..frame(2)
                },
                250
            )
            .is_err());
        assert_eq!(e.sequence(), 1);
        assert_eq!(e.age(980), Some(1000));
        assert_eq!(
            e.read(980, None, |f| f.indicated_ft).quality,
            Quality::Stale
        );
        e.accept(
            Frame {
                receiver_ms: 980,
                ..frame(2)
            },
            1000,
        )
        .unwrap();
        assert_eq!(e.age(1000), Some(40));
        assert_eq!(
            e.read(1000, Some(Quality::Failed), |f| f.indicated_ft)
                .quality,
            Quality::Failed
        );
    }
    #[test]
    fn atomic_rejection_pause_missing_disconnect_and_recovery() {
        let mut e = External::new(Provider::Msfs2020, 10);
        e.accept(frame(1), 100).unwrap();
        assert!(e
            .accept(
                Frame {
                    baro_inhg: Datum::Value(f32::NAN),
                    ..frame(2)
                },
                120
            )
            .is_err());
        assert_eq!(e.sequence(), 1);
        e.accept(
            Frame {
                indicated_ft: Datum::Missing,
                ..frame(2)
            },
            120,
        )
        .unwrap();
        assert_eq!(e.read(120, None, |f| f.indicated_ft).value, None);
        e.accept(
            Frame {
                state: Lifecycle::Paused,
                ..frame(3)
            },
            140,
        )
        .unwrap();
        assert_eq!(e.read(140, None, |f| f.indicated_ft).value, None);
        e.accept(frame(4), 160).unwrap();
        assert_eq!(e.read(160, None, |f| f.indicated_ft).value, Some(8500.));
        e.accept(
            Frame {
                state: Lifecycle::Disconnected,
                ..frame(5)
            },
            180,
        )
        .unwrap();
        assert!(e.accept(frame(6), 200).is_err());
    }
}
