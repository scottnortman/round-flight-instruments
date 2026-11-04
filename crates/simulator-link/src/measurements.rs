//! Canonical provider measurements. Gauge scales do not define sensor limits.
//! Add new quantities explicitly; never reinterpret an existing quantity.
use crate::{Datum, Error};

pub const MAX_MEASUREMENTS: usize = 24;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Feet,
    InHg,
    Degrees,
    Knots,
    FeetPerMinute,
    DegreesPerSecond,
    Normalized,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quantity {
    IndicatedAltitudeFt,
    PressureAltitudeFt,
    BarometerInHg,
    PitchDeg,
    RollDeg,
    IndicatedAirspeedKt,
    MagneticHeadingDeg,
    VerticalSpeedFpm,
    TurnRateDps,
    SlipBall,
    TrueHeadingDeg,
    TrueAirspeedKt,
    HeightAboveGroundFt,
}
impl Quantity {
    /// Stable wire IDs. Append new quantities; never renumber existing ones.
    pub const ALL: [Self; 13] = [
        Self::IndicatedAltitudeFt,
        Self::PressureAltitudeFt,
        Self::BarometerInHg,
        Self::PitchDeg,
        Self::RollDeg,
        Self::IndicatedAirspeedKt,
        Self::MagneticHeadingDeg,
        Self::VerticalSpeedFpm,
        Self::TurnRateDps,
        Self::SlipBall,
        Self::TrueHeadingDeg,
        Self::TrueAirspeedKt,
        Self::HeightAboveGroundFt,
    ];
    pub fn wire_id(self) -> u8 {
        Self::ALL.iter().position(|q| *q == self).unwrap() as u8 + 1
    }
    pub fn from_wire(id: u8) -> Option<Self> {
        id.checked_sub(1)
            .and_then(|i| Self::ALL.get(i as usize))
            .copied()
    }
    pub const fn unit(self) -> Unit {
        use Quantity::*;
        match self {
            IndicatedAltitudeFt | PressureAltitudeFt | HeightAboveGroundFt => Unit::Feet,
            BarometerInHg => Unit::InHg,
            PitchDeg | RollDeg | MagneticHeadingDeg | TrueHeadingDeg => Unit::Degrees,
            IndicatedAirspeedKt | TrueAirspeedKt => Unit::Knots,
            VerticalSpeedFpm => Unit::FeetPerMinute,
            TurnRateDps => Unit::DegreesPerSecond,
            SlipBall => Unit::Normalized,
        }
    }
    pub const fn stale_after_ms(self) -> u32 {
        use Quantity::*;
        match self {
            // User-approved 200 ms external attitude/turn budget includes transport
            // and LCD scanout. Actual acquisition ages remain unchanged.
            // Local/IMU policies live in instrument-core and are unchanged.
            PitchDeg | RollDeg | TurnRateDps | SlipBall => 200,
            MagneticHeadingDeg | TrueHeadingDeg | IndicatedAirspeedKt | TrueAirspeedKt
            | VerticalSpeedFpm => 500,
            _ => 1000,
        }
    }
    pub fn accepts(self, v: f64) -> bool {
        use Quantity::*;
        if !v.is_finite() {
            return false;
        }
        match self {
            IndicatedAltitudeFt | PressureAltitudeFt => (-10000.0..=200000.0).contains(&v),
            HeightAboveGroundFt => (-1000.0..=200000.0).contains(&v),
            BarometerInHg => (20.0..=35.0).contains(&v),
            PitchDeg => (-90.0..=90.0).contains(&v),
            RollDeg => (-180.0..=180.0).contains(&v),
            MagneticHeadingDeg | TrueHeadingDeg => (0.0..360.0).contains(&v),
            IndicatedAirspeedKt | TrueAirspeedKt => (0.0..=10000.0).contains(&v),
            VerticalSpeedFpm => (-100000.0..=100000.0).contains(&v),
            TurnRateDps => (-1000.0..=1000.0).contains(&v),
            SlipBall => (-1.0..=1.0).contains(&v),
        }
    }
}
/// Channel 0 is the explicitly selected primary instrument, not an SDK index.
/// An adapter/profile must map pilot/copilot/engine channels consistently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub quantity: Quantity,
    pub channel: u16,
}
impl Key {
    pub const fn primary(quantity: Quantity) -> Self {
        Self {
            quantity,
            channel: 0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Valid(f64),
    Missing,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measurement {
    pub key: Key,
    pub value: Value,
    /// Age at the observation's host monotonic timestamp; unrelated samples
    /// retain their own ages when an adapter assembles an observation.
    pub age_ms: u32,
}
impl Measurement {
    pub fn validate(self) -> Result<Self, Error> {
        if matches!(self.value, Value::Valid(v) if !self.key.quantity.accepts(v)) {
            Err(Error::Range)
        } else {
            Ok(self)
        }
    }
    pub fn datum(self) -> Datum {
        match self.value {
            Value::Valid(v) => Datum::Value(v as f32),
            Value::Missing => Datum::Missing,
            Value::Failed => Datum::Failed,
        }
    }
}
/// A complete, bounded observation. Omitted keys are missing, not carried forward.
/// Private storage prevents bypassing duplicate and finite/range validation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measurements {
    items: [Option<Measurement>; MAX_MEASUREMENTS],
    len: usize,
}
impl Default for Measurements {
    fn default() -> Self {
        Self::new()
    }
}
impl Measurements {
    pub const fn new() -> Self {
        Self {
            items: [None; MAX_MEASUREMENTS],
            len: 0,
        }
    }
    pub fn push(&mut self, item: Measurement) -> Result<(), Error> {
        item.validate()?;
        if self.get(item.key).is_some() {
            return Err(Error::Syntax);
        }
        if self.len == MAX_MEASUREMENTS {
            return Err(Error::TooLong);
        }
        self.items[self.len] = Some(item);
        self.len += 1;
        Ok(())
    }
    pub fn iter(&self) -> impl Iterator<Item = &Measurement> {
        self.items[..self.len].iter().flatten()
    }
    pub fn get(&self, key: Key) -> Option<Measurement> {
        self.iter().find(|m| m.key == key).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channels_units_bounds_and_independent_age() {
        let mut batch = Measurements::new();
        let pitch = Measurement {
            key: Key::primary(Quantity::PitchDeg),
            value: Value::Valid(12.5),
            age_ms: 90,
        };
        batch.push(pitch).unwrap();
        assert_eq!(batch.push(pitch), Err(Error::Syntax));
        batch
            .push(Measurement {
                key: Key {
                    channel: 1,
                    ..pitch.key
                },
                age_ms: 3,
                ..pitch
            })
            .unwrap();
        assert_eq!(batch.get(pitch.key).unwrap().age_ms, 90);
        assert!(batch.get(Key::primary(Quantity::RollDeg)).is_none());
        assert!(!Quantity::MagneticHeadingDeg.accepts(360.0));
        assert!(!Quantity::PitchDeg.accepts(f64::NAN));
        assert!(Quantity::IndicatedAirspeedKt.accepts(500.0)); // Beyond current dial's scale.
        assert_eq!(Quantity::VerticalSpeedFpm.unit(), Unit::FeetPerMinute);
        assert!(
            Quantity::PitchDeg.stale_after_ms() < Quantity::IndicatedAltitudeFt.stale_after_ms()
        );
        for channel in 2..MAX_MEASUREMENTS as u16 {
            batch
                .push(Measurement {
                    key: Key {
                        channel,
                        ..pitch.key
                    },
                    ..pitch
                })
                .unwrap();
        }
        assert_eq!(
            batch.push(Measurement {
                key: Key {
                    channel: 99,
                    ..pitch.key
                },
                ..pitch
            }),
            Err(Error::TooLong)
        );
    }
}
