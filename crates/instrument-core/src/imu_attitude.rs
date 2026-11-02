//! Explicit bench source. Receipt age is preserved; repeated reads cannot renew it.
use crate::{Field, Quality, Reading};

#[derive(Clone, Copy, Debug)]
pub struct Input {
    /// Monotonic worker receipt identifier, unchanged by snapshot reads.
    pub received_us: u64,
    pub age_ms: u64,
    pub pitch_roll_deg: Option<[f32; 2]>,
    /// Valid here means usable BENCH tilt, never calibrated aircraft attitude.
    pub quality: Quality,
}
impl Input {
    pub fn unavailable(quality: Quality) -> Self {
        Self {
            received_us: 0,
            age_ms: 0,
            pitch_roll_deg: None,
            quality,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Receiver {
    last_id: Option<u64>,
    input: Input,
    at_ms: u64,
    last_good: Option<(Input, u64)>,
}
impl Receiver {
    pub const fn new() -> Self {
        Self {
            last_id: None,
            input: Input {
                received_us: 0,
                age_ms: 0,
                pitch_roll_deg: None,
                quality: Quality::Missing,
            },
            at_ms: 0,
            last_good: None,
        }
    }
    pub fn accept(&mut self, mut input: Input, now: u64) {
        if input.quality == Quality::Valid
            && (input.received_us == 0
                || input
                    .pitch_roll_deg
                    .is_none_or(|[p, r]| !Field::PitchDeg.accepts(p) || !Field::RollDeg.accepts(r)))
        {
            input.quality = Quality::Failed;
        }
        if input.quality == Quality::Valid && input.age_ms >= 100 {
            input.quality = Quality::Stale;
        }
        if input.quality != Quality::Valid {
            input.pitch_roll_deg = None;
            // Keep the receipt watermark: an old sample cannot revive after a fault.
            self.last_id = self
                .last_id
                .into_iter()
                .chain((input.received_us != 0).then_some(input.received_us))
                .max();
            self.input = input;
            self.at_ms = now;
            return;
        }
        if let Some(last) = self.last_id {
            if input.received_us == last {
                return;
            }
            if input.received_us < last || now < self.at_ms {
                self.input = Input::unavailable(Quality::TimingFault);
                self.at_ms = now;
                return;
            }
        }
        self.last_id = Some(input.received_us);
        self.input = input;
        self.at_ms = now;
        self.last_good = Some((input, now));
    }
    /// Display-only retention. Never changes the live reading's quality or age.
    pub fn last_good(&self, now: u64) -> Option<Retained> {
        let (input, at) = self.last_good?;
        Some(Retained {
            pitch_roll_deg: input.pitch_roll_deg?,
            age_ms: input.age_ms.saturating_add(now.checked_sub(at)?),
        })
    }
    pub fn read(&self, field: Field, now: u64, fault: Option<Quality>) -> Reading {
        let index = match field {
            Field::PitchDeg => 0,
            Field::RollDeg => 1,
            _ => {
                return Reading {
                    value: None,
                    quality: fault.unwrap_or(Quality::Missing),
                    age_ms: None,
                }
            }
        };
        let age = (self.input.received_us != 0)
            .then(|| now.checked_sub(self.at_ms))
            .flatten()
            .map(|a| self.input.age_ms.saturating_add(a));
        let quality = fault.unwrap_or_else(|| {
            if self.input.quality != Quality::Valid {
                self.input.quality
            } else if age.is_none() {
                Quality::TimingFault
            } else if age.is_some_and(|a| a >= 100) {
                Quality::Stale
            } else {
                Quality::Valid
            }
        });
        Reading {
            value: self
                .input
                .pitch_roll_deg
                .filter(|_| quality == Quality::Valid)
                .map(|a| a[index]),
            quality,
            age_ms: age,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Retained {
    pub pitch_roll_deg: [f32; 2],
    pub age_ms: u64,
}
impl Default for Receiver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_pair_survives_rejection_without_renewing_live_data_or_age() {
        let mut r = Receiver::new();
        assert_eq!(r.last_good(0), None);
        let good = Input {
            received_us: 10,
            age_ms: 5,
            pitch_roll_deg: Some([12., -30.]),
            quality: Quality::Valid,
        };
        r.accept(good, 100);
        r.accept(Input::unavailable(Quality::Failed), 120);
        r.accept(good, 130); // same receipt cannot revive live input or renew hold
        assert_eq!(r.read(Field::PitchDeg, 150, None).value, None);
        assert_eq!(
            r.last_good(150),
            Some(Retained {
                pitch_roll_deg: [12., -30.],
                age_ms: 55
            })
        );
        assert_eq!(r.last_good(10_000).unwrap().age_ms, 9905);
        r.accept(
            Input {
                received_us: 11,
                pitch_roll_deg: Some([15., 20.]),
                ..good
            },
            10_001,
        );
        assert_eq!(r.last_good(10_001).unwrap().pitch_roll_deg, [15., 20.]);
        assert_eq!(
            r.read(Field::PitchDeg, 10_001, None).quality,
            Quality::Valid
        );
        r = Receiver::new(); // source selection resets retention
        assert_eq!(r.last_good(10_002), None);
    }
    #[test]
    fn repeated_reads_cannot_refresh_or_revive_and_axes_expire_together() {
        let mut r = Receiver::new();
        assert_eq!(
            r.read(Field::PitchDeg, 10000, None),
            Reading {
                value: None,
                quality: Quality::Missing,
                age_ms: None
            }
        );
        let input = Input {
            received_us: 123,
            age_ms: 20,
            pitch_roll_deg: Some([10., 30.]),
            quality: Quality::Valid,
        };
        r.accept(input, 100);
        r.accept(Input { age_ms: 0, ..input }, 150);
        assert_eq!(r.read(Field::PitchDeg, 179, None).age_ms, Some(99));
        for f in [Field::PitchDeg, Field::RollDeg] {
            assert_eq!(r.read(f, 180, None).quality, Quality::Stale);
            assert_eq!(r.read(f, 180, None).value, None);
        }
        r.accept(Input::unavailable(Quality::Failed), 181);
        r.accept(input, 182);
        assert_eq!(r.read(Field::PitchDeg, 182, None).quality, Quality::Failed);
        r.accept(
            Input {
                received_us: 124,
                ..input
            },
            183,
        );
        assert_eq!(r.read(Field::RollDeg, 183, None).value, Some(30.));
        assert_eq!(r.read(Field::PressureAltitudeFt, 183, None).value, None);
        r.accept(
            Input {
                received_us: 122,
                ..input
            },
            184,
        );
        assert_eq!(
            r.read(Field::PitchDeg, 184, None).quality,
            Quality::TimingFault
        );
    }
    #[test]
    fn invalid_angles_and_unknown_pairs_never_reach_the_gauge() {
        for pair in [
            None,
            Some([f32::NAN, 0.]),
            Some([91., 0.]),
            Some([0., 181.]),
        ] {
            let mut r = Receiver::new();
            r.accept(
                Input {
                    received_us: 1,
                    age_ms: 0,
                    pitch_roll_deg: pair,
                    quality: Quality::Valid,
                },
                0,
            );
            assert_eq!(r.read(Field::PitchDeg, 0, None).quality, Quality::Failed);
            assert_eq!(r.read(Field::RollDeg, 0, None).value, None);
        }
    }
}
