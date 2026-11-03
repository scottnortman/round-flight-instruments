#![no_std]
#![forbid(unsafe_code)]

pub mod altimeter;
pub mod attitude;
pub mod damage;
pub mod feedback;
pub mod interaction;
pub mod navigation;
pub mod pipeline;
pub mod raster;
pub mod render;

use instrument_core::{App, Field, Instrument, Quality, Reading};

/// Immutable view input. Source age is retained through transport and rendering.
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub imu_bench: bool,
    pub imu_last_good: Option<instrument_core::imu_attitude::Retained>,
    pub external: bool,
    pub external_attitude: bool,
    pub source_session: u64,
    pub source_sequence: u64,
    /// Receipt time in the console owner clock, not the platform display clock.
    pub source_received_ms: u64,
    pub baro_age_ms: Option<u64>,
    pub source_label: &'static str,
    pub kollsman_pressed: bool,
    pub editor: Option<interaction::EditorView>,
    pub menu: Option<navigation::View>,
    pub center_pressed: bool,
    pub selected: Instrument,
    pub altitude: Reading,
    pub pitch: Reading,
    pub roll: Reading,
    pub baro_inhg: f32,
}
impl Snapshot {
    pub fn model(self) -> altimeter::Altimeter {
        altimeter::model_with_semantics(self.altitude, self.baro_inhg, self.external)
    }
    pub fn attitude_model(self) -> attitude::Attitude {
        let mut model = attitude::model_with_limit(self.pitch, self.roll, self.attitude_stale_ms());
        if self.imu_bench && model.angles.is_none() {
            if let Some(held) = self.imu_last_good.filter(|h| {
                Field::PitchDeg.accepts(h.pitch_roll_deg[0])
                    && Field::RollDeg.accepts(h.pitch_roll_deg[1])
            }) {
                model.angles = Some(attitude::Angles {
                    pitch_deg: held.pitch_roll_deg[0],
                    roll_deg: held.pitch_roll_deg[1],
                });
            }
        }
        model
    }
    fn attitude_stale_ms(self) -> u64 {
        if self.external {
            instrument_core::simulator_link::measurements::Quantity::PitchDeg.stale_after_ms()
                as u64
        } else {
            attitude::STALE_MS
        }
    }
    pub fn imu_holding(self) -> bool {
        self.imu_bench
            && self.attitude_model().angles.is_some()
            && self.attitude_model().quality != Quality::Valid
    }
    pub fn displayed_quality(self) -> Option<Quality> {
        match self.selected {
            Instrument::Altimeter => Some(self.model().quality),
            Instrument::Attitude => Some(self.attitude_model().quality),
            _ => None,
        }
    }
    pub fn displayed_age_ms(self) -> Option<u64> {
        match self.selected {
            Instrument::Altimeter => self.altitude.age_ms,
            Instrument::Attitude if self.imu_holding() => self.imu_last_good.map(|h| h.age_ms),
            Instrument::Attitude => Some(self.pitch.age_ms?.max(self.roll.age_ms?)),
            _ => None,
        }
    }
    pub fn from_app(app: &App) -> Self {
        Self {
            imu_bench: app.imu_bench(),
            imu_last_good: app.imu_last_good(),
            external: app.external().is_some(),
            external_attitude: app.external().is_some_and(|e| e.multi_profile),
            source_session: app.external().map_or(0, |e| e.session),
            source_sequence: app.external().map_or(0, |e| e.sequence()),
            source_received_ms: app.external().and_then(|e| e.received_ms()).unwrap_or(0),
            baro_age_ms: app.external_baro(app.now_ms()).and_then(|b| b.age_ms),
            source_label: app.source_label(),
            kollsman_pressed: false,
            menu: None,
            center_pressed: false,
            editor: None,
            selected: app.selected(),
            altitude: app.altimeter_reading(app.now_ms()),
            pitch: app.reading(Field::PitchDeg, app.now_ms()),
            roll: app.reading(Field::RollDeg, app.now_ms()),
            baro_inhg: app
                .external_baro(app.now_ms())
                .map_or_else(|| app.baro_inhg(), |r| r.value.unwrap_or(f32::NAN)),
        }
    }
    pub fn aged(mut self, elapsed_ms: u64) -> Self {
        let attitude_stale_ms = self.attitude_stale_ms();
        if let Some(held) = &mut self.imu_last_good {
            held.age_ms = held.age_ms.saturating_add(elapsed_ms);
        }
        for reading in [&mut self.pitch, &mut self.roll] {
            reading.age_ms = reading.age_ms.map(|age| age.saturating_add(elapsed_ms));
            if reading.quality == Quality::Valid
                && reading.age_ms.is_none_or(|age| age >= attitude_stale_ms)
            {
                reading.value = None;
                reading.quality = Quality::Stale;
            }
        }
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
        if self.selected == Instrument::Attitude {
            // Explicit bench hold policy: numerical age stays observable, but
            // does not blank a held frame. Owner/render/scanout guards still apply.
            if self.imu_bench {
                return None;
            }
            return self.attitude_model().angles.map(|_| {
                self.attitude_stale_ms()
                    .saturating_sub(self.displayed_age_ms().unwrap_or(self.attitude_stale_ms()))
            });
        }
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
    fn imu_hold_preserves_quality_and_age_and_does_not_change_other_sources() {
        use instrument_core::{imu_attitude::Input, Command};
        let mut app = App::new();
        app.apply(Command::SourceImu).unwrap();
        let waiting = Snapshot::from_app(&app);
        assert!(waiting.attitude_model().angles.is_none());
        app.accept_imu(Input {
            received_us: 1,
            age_ms: 3,
            pitch_roll_deg: Some([10., 20.]),
            quality: Quality::Valid,
        });
        let live = Snapshot::from_app(&app);
        assert!(!live.imu_holding());
        assert_eq!(live.numeric_lifetime_ms(), None);
        let held = live.aged(2000);
        assert!(held.imu_holding());
        assert_eq!(held.displayed_quality(), Some(Quality::Stale));
        assert_eq!(held.imu_last_good.unwrap().age_ms, 2003);
        assert_eq!(held.attitude_model().angles.unwrap().pitch_deg, 10.);
        assert_eq!(held.source_label, live.source_label);
        assert_eq!(held.numeric_lifetime_ms(), None);
        let strict = Snapshot {
            imu_bench: false,
            ..held
        };
        assert!(strict.attitude_model().angles.is_none());
        app.apply(Command::SourceDemo).unwrap();
        assert!(Snapshot::from_app(&app).imu_last_good.is_none());
    }
    #[test]
    fn attitude_expires_at_100ms_using_the_older_axis_and_never_uses_demo_for_external() {
        use instrument_core::Command;
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.apply(Command::Select(Instrument::Attitude)).unwrap();
        app.tick(0);
        let mut s = Snapshot::from_app(&app);
        s.pitch.age_ms = Some(70);
        s.roll.age_ms = Some(90);
        assert_eq!(s.numeric_lifetime_ms(), Some(10));
        assert!(s.aged(9).attitude_model().angles.is_some());
        let expired = s.aged(10);
        assert!(expired.attitude_model().angles.is_none());
        assert_eq!(expired.attitude_model().quality, Quality::Stale);
        assert_eq!(expired.numeric_lifetime_ms(), None);
        app.apply(Command::SourceExternal(
            instrument_core::simulator_link::Provider::Msfs2020,
            1,
        ))
        .unwrap();
        let external = Snapshot::from_app(&app);
        assert!(external.attitude_model().angles.is_none());
        assert_eq!(external.pitch.value, None);
        assert_eq!(external.roll.value, None);
    }
    #[test]
    fn external_attitude_expires_at_150ms_without_changing_local_policy_or_ages() {
        use instrument_core::Command;
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.apply(Command::Select(Instrument::Attitude)).unwrap();
        app.tick(0);
        let mut snapshot = Snapshot::from_app(&app);
        snapshot.pitch.age_ms = Some(70);
        snapshot.roll.age_ms = Some(90);
        assert_eq!(snapshot.numeric_lifetime_ms(), Some(10));
        snapshot.external = true;
        snapshot.external_attitude = true;
        assert_eq!(snapshot.numeric_lifetime_ms(), Some(60));
        let live = snapshot.aged(59);
        assert_eq!(live.displayed_age_ms(), Some(149));
        assert_eq!(live.numeric_lifetime_ms(), Some(1));
        assert_eq!(live.attitude_model().quality, Quality::Valid);
        let stale = snapshot.aged(60);
        assert_eq!(stale.displayed_age_ms(), Some(150));
        assert_eq!(stale.attitude_model().quality, Quality::Stale);
        assert!(stale.attitude_model().angles.is_none());
        assert_eq!(stale.numeric_lifetime_ms(), None);
        snapshot.roll.quality = Quality::Failed;
        assert_eq!(snapshot.attitude_model().quality, Quality::Failed);
        assert!(snapshot.attitude_model().angles.is_none());
    }
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
            pitch: Reading {
                value: Some(0.),
                quality: Quality::Valid,
                age_ms: Some(0),
            },
            roll: Reading {
                value: Some(0.),
                quality: Quality::Valid,
                age_ms: Some(0),
            },
            imu_bench: false,
            imu_last_good: None,
            external: false,
            external_attitude: false,
            source_session: 0,
            source_sequence: 0,
            source_received_ms: 0,
            baro_age_ms: None,
            source_label: "SIM INPUT",
            kollsman_pressed: false,
            menu: None,
            center_pressed: false,
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
