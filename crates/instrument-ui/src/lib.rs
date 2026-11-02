#![no_std]
#![forbid(unsafe_code)]

pub mod altimeter;
pub mod damage;
pub mod feedback;
pub mod interaction;
pub mod pipeline;
pub mod raster;
pub mod render;

use instrument_core::{App, Instrument, Quality, Reading};

/// Immutable view input. Source age is retained through transport and rendering.
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub external: bool,
    pub source_session: u64,
    pub source_sequence: u64,
    /// Receipt time in the console owner clock, not the platform display clock.
    pub source_received_ms: u64,
    pub baro_age_ms: Option<u64>,
    pub source_label: &'static str,
    pub kollsman_pressed: bool,
    pub editor: Option<interaction::EditorView>,
    pub selected: Instrument,
    pub altitude: Reading,
    pub baro_inhg: f32,
}
impl Snapshot {
    pub fn model(self) -> altimeter::Altimeter {
        altimeter::model_with_semantics(self.altitude, self.baro_inhg, self.external)
    }
    pub fn from_app(app: &App) -> Self {
        Self {
            external: app.external().is_some(),
            source_session: app.external().map_or(0, |e| e.session),
            source_sequence: app.external().map_or(0, |e| e.sequence()),
            source_received_ms: app.external().and_then(|e| e.received_ms()).unwrap_or(0),
            baro_age_ms: app.external_baro(app.now_ms()).and_then(|b| b.age_ms),
            source_label: app.source_label(),
            kollsman_pressed: false,
            editor: None,
            selected: app.selected(),
            altitude: app.altimeter_reading(app.now_ms()),
            baro_inhg: app
                .external_baro(app.now_ms())
                .map_or_else(|| app.baro_inhg(), |r| r.value.unwrap_or(f32::NAN)),
        }
    }
    pub fn aged(mut self, elapsed_ms: u64) -> Self {
        if self.external {
            self.baro_age_ms = self.baro_age_ms.map(|a| a.saturating_add(elapsed_ms));
            if self.baro_age_ms.is_none_or(|a| a >= 1000) {
                self.baro_inhg = f32::NAN;
            }
        }
        if let Some(age) = self.altitude.age_ms {
            let age = age.saturating_add(elapsed_ms);
            self.altitude.age_ms = Some(age);
            if self.altitude.quality == Quality::Valid && age >= 1000 {
                self.altitude.value = None;
                self.altitude.quality = Quality::Stale;
                if self.external {
                    self.source_label = "LINK STALE";
                    self.baro_inhg = f32::NAN;
                }
            }
        }
        self
    }
    /// Remaining lifetime of displayed numeric data. Invalid-data/placeholder
    /// frames have no numeric expiry, but still require owner/render health.
    pub fn numeric_lifetime_ms(self) -> Option<u64> {
        if self.selected != Instrument::Altimeter {
            return None;
        }
        let altitude = (self.altitude.quality == Quality::Valid)
            .then(|| 1000u64.saturating_sub(self.altitude.age_ms.unwrap_or(1000)));
        let baro = (self.external && self.baro_inhg.is_finite())
            .then(|| 1000u64.saturating_sub(self.baro_age_ms.unwrap_or(1000)));
        match (altitude, baro) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
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
    fn external_barometer_expires_even_without_altitude() {
        let mut app = App::new();
        app.tick(0);
        use instrument_core::{
            simulator_link::{Datum, Frame, Lifecycle, Provider},
            Command,
        };
        app.apply(Command::SourceExternal(Provider::Replay, 1))
            .unwrap();
        app.apply(Command::Telemetry(Frame {
            session: 1,
            sequence: 1,
            receiver_ms: 0,
            age_ms: 990,
            state: Lifecycle::Running,
            indicated_ft: Datum::Missing,
            pressure_ft: Datum::Missing,
            baro_inhg: Datum::Value(29.92),
        }))
        .unwrap();
        let s = Snapshot::from_app(&app);
        assert_eq!(s.numeric_lifetime_ms(), Some(10));
        assert!(s.aged(9).baro_inhg.is_finite());
        assert!(!s.aged(10).baro_inhg.is_finite());
    }
    #[test]
    fn independent_health_and_age_boundaries() {
        assert!(presentation_healthy(99, 0, 0, 100, 0));
        assert!(!presentation_healthy(100, 0, 90, 1000, 90));
        assert!(!presentation_healthy(250, 240, 0, 1000, 240));
        assert!(!presentation_healthy(100, 90, 90, 100, 90));
        assert!(!presentation_healthy(10, 11, 9, 100, 9));
        let s = Snapshot {
            external: false,
            source_session: 0,
            source_sequence: 0,
            source_received_ms: 0,
            baro_age_ms: None,
            source_label: "SIM INPUT",
            kollsman_pressed: false,
            selected: Instrument::Altimeter,
            altitude: Reading {
                value: Some(0.),
                quality: Quality::Valid,
                age_ms: Some(990),
            },
            editor: None,
            baro_inhg: 29.92,
        };
        assert_eq!(s.aged(9).altitude.value, Some(0.));
        assert_eq!(s.aged(10).altitude.value, None);
    }
}
