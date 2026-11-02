use crate::{ApplyError, Quality, Reading};
use simulator_link::{Datum, Frame, Lifecycle, Provider, STALE_MS};
#[derive(Clone, Copy, Debug)]
pub struct External {
    pub provider: Provider,
    pub session: u64,
    sequence: u64,
    sample: Option<(Frame, u64)>,
}
impl External {
    pub const fn new(provider: Provider, session: u64) -> Self {
        Self {
            provider,
            session,
            sequence: 0,
            sample: None,
        }
    }
    pub fn sequence(self) -> u64 {
        self.sequence
    }
    pub fn received_ms(self) -> Option<u64> {
        self.sample.map(|(_, at)| at)
    }
    pub fn clear_altitude(&mut self) {
        if let Some((f, _)) = &mut self.sample {
            f.indicated_ft = Datum::Missing;
            f.pressure_ft = Datum::Missing;
        }
    }
    pub fn state(self) -> Lifecycle {
        self.sample.map_or(Lifecycle::Loading, |(f, _)| f.state)
    }
    pub fn age(self, now: u64) -> Option<u64> {
        let (f, at) = self.sample?;
        Some(now.checked_sub(at)?.saturating_add(f.age_ms as u64))
    }
    pub fn accept(&mut self, mut frame: Frame, now: u64) -> Result<(), ApplyError> {
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
    pub fn read(
        self,
        now: u64,
        fault: Option<Quality>,
        field: impl FnOnce(Frame) -> Datum,
    ) -> Reading {
        let age = self.age(now);
        let datum = self.sample.map_or(Datum::Missing, |(f, _)| field(f));
        let quality = if self.sample.is_some() && age.is_none() {
            Quality::TimingFault
        } else if let Some(fault) = fault {
            fault
        } else if age.is_some_and(|a| a >= STALE_MS) {
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
