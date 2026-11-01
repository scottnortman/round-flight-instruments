#![no_std]
#![forbid(unsafe_code)]

pub mod altimeter;
pub mod raster;
pub mod render;

use instrument_core::{App, Field, Instrument, Quality, Reading};

/// Immutable view input. Source age is retained through transport and rendering.
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub selected: Instrument,
    pub altitude: Reading,
    pub baro_inhg: f32,
}
impl Snapshot {
    pub fn from_app(app: &App) -> Self {
        Self {
            selected: app.selected(),
            altitude: app.reading(Field::PressureAltitudeFt, app.now_ms()),
            baro_inhg: app.baro_inhg(),
        }
    }
    pub fn aged(mut self, elapsed_ms: u64) -> Self {
        if let Some(age) = self.altitude.age_ms {
            let age = age.saturating_add(elapsed_ms);
            self.altitude.age_ms = Some(age);
            if self.altitude.quality == Quality::Valid && age >= 1000 {
                self.altitude.value = None;
                self.altitude.quality = Quality::Stale;
            }
        }
        self
    }
    /// Remaining lifetime of displayed numeric data. Invalid-data/placeholder
    /// frames have no numeric expiry, but still require owner/render health.
    pub fn numeric_lifetime_ms(self) -> Option<u64> {
        if self.selected == Instrument::Altimeter && self.altitude.quality == Quality::Valid {
            Some(1000u64.saturating_sub(self.altitude.age_ms.unwrap_or(1000)))
        } else {
            None
        }
    }
}

/// Presentation supervision independent of the renderer. All arguments use one
/// monotonic clock domain. These are bench budgets, not aircraft guarantees.
pub fn presentation_healthy(
    now: u64,
    owner_at: u64,
    presented_at: u64,
    numeric_deadline: u64,
    scanout_at: u64,
) -> bool {
    now.checked_sub(owner_at).is_some_and(|a| a < 100)
        && now.checked_sub(presented_at).is_some_and(|a| a < 250)
        && now.checked_sub(scanout_at).is_some_and(|a| a < 100)
        && now < numeric_deadline
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_health_and_age_boundaries() {
        assert!(presentation_healthy(99, 0, 0, 100, 0));
        assert!(!presentation_healthy(100, 0, 90, 1000, 90));
        assert!(!presentation_healthy(250, 240, 0, 1000, 240));
        assert!(!presentation_healthy(100, 90, 90, 100, 90));
        assert!(!presentation_healthy(10, 11, 9, 100, 9));
        let s = Snapshot {
            selected: Instrument::Altimeter,
            altitude: Reading {
                value: Some(0.),
                quality: Quality::Valid,
                age_ms: Some(990),
            },
            baro_inhg: 29.92,
        };
        assert_eq!(s.aged(9).altitude.value, Some(0.));
        assert_eq!(s.aged(10).altitude.value, None);
    }
}
