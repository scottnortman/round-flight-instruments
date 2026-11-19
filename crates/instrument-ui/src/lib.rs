#![no_std]
#![forbid(unsafe_code)]

pub mod altimeter;
pub mod consolidated;
pub mod debug;
pub mod calibration;
pub mod visibility;
pub mod airspeed;
pub mod attitude;
pub mod continuity;
pub mod deadline_hold;
pub mod damage;
pub mod feedback;
pub mod interaction;
pub mod navigation;
pub mod pipeline;
pub mod raster;
pub mod render;
pub mod turn;
pub mod heading;
pub mod vsi;
mod dial;
#[cfg(test)]
mod demo_tests;

use instrument_core::{App, Field, Instrument, Quality, Reading};

/// Immutable view input. Source age is retained through transport and rendering.
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub device: instrument_core::device::Info,
    pub imu_bench: bool,
    pub imu_last_good: Option<instrument_core::imu_attitude::Retained>,
    pub external: bool,
    /// T2 multi-instrument source capability (legacy field name).
    pub external_attitude: bool,
    pub source_running: bool,
    pub source_session: u64,
    pub source_sequence: u64,
    /// Receipt time in the console owner clock, not the platform display clock.
    pub source_received_ms: u64,
    pub baro_age_ms: Option<u64>,
    pub baro_quality: Quality,
    pub source_label: &'static str,
    pub kollsman_pressed: bool,
    pub altitude_hidden: bool,
    pub editor: Option<interaction::EditorView>,
    pub menu: Option<navigation::View>,
    pub calibration: Option<calibration::View>,
    pub gauge_profile: Option<instrument_core::gauge_profile::Profile>,
    pub center_pressed: bool,
    pub selected: Instrument,
    pub altitude: Reading,
    pub pressure_altitude: Reading,
    pub ias: Reading,
    pub heading: Reading,
    pub vsi: Reading,
    pub pitch: Reading,
    pub roll: Reading,
    pub turn_rate: Reading,
    pub ball: Reading,
    pub baro_inhg: f32,
}
impl Snapshot {
    /// Only the pressure/altitude inputs govern this editor, even if another
    /// component of the Combined panel is unavailable.
    pub fn baro_editor_visible(self)->bool {
        self.editor.is_some() && self.menu.is_none() && self.calibration.is_none()
            && matches!(self.selected,Instrument::Altimeter|Instrument::Consolidated)
            && self.model().indicated_ft.is_some()
    }
    fn consolidated_quality(self) -> Quality {
        let qualities = [self.model().quality, self.airspeed_model().quality,
            self.attitude_model().quality, self.heading_model().quality,
            self.turn_model().quality(), self.vsi_model().quality];
        [Quality::TimingFault, Quality::Failed, Quality::Stale, Quality::Missing]
            .into_iter().find(|q| qualities.contains(q)).unwrap_or(Quality::Valid)
    }
    pub fn heading_model(self)->heading::Heading {heading::model(self.heading,self.heading_stale_ms())}
    pub fn vsi_model(self)->vsi::Vsi {vsi::model_with_profile(self.vsi,self.vsi_stale_ms(),self.vsi_profile())}
    pub fn vsi_profile(self)->vsi::Profile {self.gauge_profile.map_or(vsi::DEFAULT_PROFILE,|p|vsi::Profile::new(p.vsi_max as f32,165.).unwrap())}
    fn heading_stale_ms(self)->u64 {
        if self.external {instrument_core::simulator_link::measurements::Quantity::MagneticHeadingDeg.stale_after_ms() as u64}else{Field::HeadingDeg.stale_after_ms()}
    }
    fn vsi_stale_ms(self)->u64 {
        if self.external {instrument_core::simulator_link::measurements::Quantity::VerticalSpeedFpm.stale_after_ms() as u64}else{Field::VsiFpm.stale_after_ms()}
    }
    pub fn render_profile(self)->Option<instrument_core::gauge_profile::Profile> {
        if matches!(self.selected,Instrument::Airspeed|Instrument::Vsi|Instrument::Consolidated){self.gauge_profile}else{None}
    }
    pub fn airspeed_model(self) -> airspeed::Airspeed {
        if let Some(p)=self.gauge_profile {let mut points=[(0.,0.);11];
            airspeed::model_with_profile(self.ias,self.airspeed_stale_ms(),airspeed::Profile::configured(p,&mut points))
        }else{airspeed::model(self.ias, self.airspeed_stale_ms())}
    }
    pub fn airspeed_demo_limits(self) -> bool {
        self.gauge_profile.is_none() && self.selected == Instrument::Airspeed && !self.external && !self.imu_bench
    }
    fn airspeed_stale_ms(self) -> u64 {
        if self.external {
            instrument_core::simulator_link::measurements::Quantity::IndicatedAirspeedKt.stale_after_ms() as u64
        } else { Field::IasKt.stale_after_ms() }
    }
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
    pub fn turn_model(self) -> turn::Turn {
        turn::model(self.turn_rate, self.ball, self.turn_stale_ms())
    }
    fn turn_stale_ms(self) -> u64 {
        if self.external {
            instrument_core::simulator_link::measurements::Quantity::TurnRateDps.stale_after_ms()
                as u64
        } else {
            turn::LOCAL_STALE_MS
        }
    }
    pub fn imu_holding(self) -> bool {
        self.imu_bench
            && self.attitude_model().angles.is_some()
            && self.attitude_model().quality != Quality::Valid
    }
    pub fn displayed_quality(self) -> Option<Quality> {
        if self.menu.is_some() || self.calibration.is_some() { return None; }
        match self.selected {
            Instrument::Debug => Some(crate::debug::quality(self)),
            Instrument::Consolidated => Some(self.consolidated_quality()),
            Instrument::Heading=>Some(self.heading_model().quality),
            Instrument::Vsi=>Some(self.vsi_model().quality),
            Instrument::Airspeed => Some(self.airspeed_model().quality),
            Instrument::Altimeter => Some(self.model().quality),
            Instrument::Attitude => Some(self.attitude_model().quality),
            Instrument::Turn => Some(self.turn_model().quality()),
        }
    }
    pub fn displayed_age_ms(self) -> Option<u64> {
        match self.selected {
            Instrument::Debug => crate::debug::rows(self).into_iter().filter_map(|(_, r)| r.age_ms).max(),
            Instrument::Consolidated => [self.altitude, self.ias, self.heading, self.vsi, self.pitch, self.roll, self.turn_rate, self.ball].into_iter().filter_map(|r| r.age_ms).max(),
            Instrument::Heading=>self.heading.age_ms,
            Instrument::Vsi=>self.vsi.age_ms,
            Instrument::Airspeed => self.ias.age_ms,
            Instrument::Altimeter => self.altitude.age_ms,
            Instrument::Attitude if self.imu_holding() => self.imu_last_good.map(|h| h.age_ms),
            Instrument::Attitude => Some(self.pitch.age_ms?.max(self.roll.age_ms?)),
            Instrument::Turn => self
                .turn_rate
                .age_ms
                .into_iter()
                .chain(self.ball.age_ms)
                .max(),
        }
    }
    pub fn from_app(app: &App) -> Self {
        let mut snapshot = Self {
            device: instrument_core::device::Info::default(),
            imu_bench: app.imu_bench(),
            imu_last_good: app.imu_last_good(),
            external: app.external().is_some(),
            external_attitude: app.external().is_some_and(|e| e.multi_profile),
            source_running: app.external().is_some_and(|e| e.state() == instrument_core::simulator_link::Lifecycle::Running),
            source_session: app.external().map_or(0, |e| e.session),
            source_sequence: app.external().map_or(0, |e| e.sequence()),
            source_received_ms: app.external().and_then(|e| e.received_ms()).unwrap_or(0),
            baro_age_ms: app.external_baro(app.now_ms()).and_then(|b| b.age_ms),
            baro_quality: app.external_baro(app.now_ms()).map_or(
                if app.imu_bench() { Quality::Missing } else { Quality::Valid }, |b| b.quality),
            source_label: app.source_label(),
            kollsman_pressed: false,
            altitude_hidden: false,
            menu: None,
            calibration: None,
            gauge_profile: app.gauge_profile(),
            center_pressed: false,
            editor: None,
            selected: app.selected(),
            altitude: app.altimeter_reading(app.now_ms()),
            pressure_altitude: app.reading(Field::PressureAltitudeFt, app.now_ms()),
            ias: app.reading(Field::IasKt, app.now_ms()),
            heading: app.reading(Field::HeadingDeg,app.now_ms()),
            vsi: app.reading(Field::VsiFpm,app.now_ms()),
            pitch: app.reading(Field::PitchDeg, app.now_ms()),
            roll: app.reading(Field::RollDeg, app.now_ms()),
            turn_rate: app.reading(Field::TurnDps, app.now_ms()),
            ball: app.reading(Field::Ball, app.now_ms()),
            baro_inhg: app
                .external_baro(app.now_ms())
                .map_or_else(|| app.baro_inhg(), |r| r.value.unwrap_or(f32::NAN)),
        };
        if let Some(frame) = app.demo_frame() {
            snapshot.imu_bench = false;
            snapshot.imu_last_good = None;
            snapshot.external = false;
            snapshot.external_attitude = false;
            snapshot.source_running = true;
            snapshot.source_session = 0;
            snapshot.source_sequence = app.tick_count();
            snapshot.source_received_ms = app.now_ms();
            snapshot.baro_age_ms = None;
            snapshot.baro_quality = Quality::Valid;
            snapshot.source_label = if frame.using_imu { "DEMO IMU" } else { "DEMO AUTO" };
            snapshot.altitude = frame.readings[Field::PressureAltitudeFt as usize];
            snapshot.pressure_altitude = snapshot.altitude;
            snapshot.ias = frame.readings[Field::IasKt as usize];
            snapshot.heading = frame.readings[Field::HeadingDeg as usize];
            snapshot.vsi = frame.readings[Field::VsiFpm as usize];
            snapshot.pitch = frame.readings[Field::PitchDeg as usize];
            snapshot.roll = frame.readings[Field::RollDeg as usize];
            snapshot.turn_rate = frame.readings[Field::TurnDps as usize];
            snapshot.ball = frame.readings[Field::Ball as usize];
            snapshot.baro_inhg = 29.92;
        }
        snapshot
    }
    pub fn aged(mut self, elapsed_ms: u64) -> Self {
        self.pressure_altitude.age_ms = self.pressure_altitude.age_ms.map(|a| a.saturating_add(elapsed_ms));
        if self.pressure_altitude.quality == Quality::Valid
            && self.pressure_altitude.age_ms.is_none_or(|a| a >= 1000) {
            self.pressure_altitude.value = None;
            self.pressure_altitude.quality = Quality::Stale;
        }
        let heading_ttl=self.heading_stale_ms();let vsi_ttl=self.vsi_stale_ms();
        for (r,ttl)in [(&mut self.heading,heading_ttl),(&mut self.vsi,vsi_ttl)] {
            r.age_ms=r.age_ms.map(|a|a.saturating_add(elapsed_ms));
            if r.quality==Quality::Valid && r.age_ms.is_none_or(|age|age>=ttl) {r.value=None;r.quality=Quality::Stale;}
        }
        self.ias.age_ms = self.ias.age_ms.map(|age| age.saturating_add(elapsed_ms));
        if self.ias.quality == Quality::Valid
            && self.ias.age_ms.is_none_or(|age| age >= self.airspeed_stale_ms()) {
            self.ias.value = None;
            self.ias.quality = Quality::Stale;
        }
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
        let turn_stale_ms = self.turn_stale_ms();
        for reading in [&mut self.turn_rate, &mut self.ball] {
            reading.age_ms = reading.age_ms.map(|a| a.saturating_add(elapsed_ms));
            if reading.quality == Quality::Valid
                && reading.age_ms.is_none_or(|a| a >= turn_stale_ms)
            {
                reading.value = None;
                reading.quality = Quality::Stale;
            }
        }
        if self.external {
            self.baro_age_ms = self.baro_age_ms.map(|a| a.saturating_add(elapsed_ms));
            if self.baro_age_ms.is_none_or(|a| a >= 1000) {
                self.baro_inhg = f32::NAN;
                if self.baro_quality == Quality::Valid { self.baro_quality = Quality::Stale; }
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
        // Navigation replaces the whole face with an opaque, non-numeric view.
        // Flight samples keep aging in the background; none are shown here.
        if self.menu.is_some() || self.calibration.is_some() { return None; }
        if self.selected == Instrument::Debug { return crate::debug::lifetime_ms(self); }
        if self.selected == Instrument::Consolidated {
            return [Instrument::Altimeter, Instrument::Airspeed, Instrument::Attitude,
                Instrument::Heading, Instrument::Turn, Instrument::Vsi]
                .into_iter().filter_map(|selected| Self { selected, ..self }.numeric_lifetime_ms()).min();
        }
        if self.selected==Instrument::Heading {
            return self.heading_model().magnetic_deg.map(|_|self.heading_stale_ms().saturating_sub(self.heading.age_ms.unwrap_or(self.heading_stale_ms())));
        }
        if self.selected==Instrument::Vsi {
            return self.vsi_model().fpm.map(|_|self.vsi_stale_ms().saturating_sub(self.vsi.age_ms.unwrap_or(self.vsi_stale_ms())));
        }
        if self.selected == Instrument::Airspeed {
            return self.airspeed_model().ias_kt.map(|_| self.airspeed_stale_ms()
                .saturating_sub(self.ias.age_ms.unwrap_or(self.airspeed_stale_ms())));
        }
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
        if self.selected == Instrument::Turn {
            let m = self.turn_model();
            return [
                (m.aircraft_deg.is_some(), self.turn_rate.age_ms),
                (m.ball.is_some(), self.ball.age_ms),
            ]
            .into_iter()
            .filter(|(valid, _)| *valid)
            .map(|(_, age)| {
                self.turn_stale_ms()
                    .saturating_sub(age.unwrap_or(self.turn_stale_ms()))
            })
            .min();
        }
        if self.selected != Instrument::Altimeter {
            return None;
        }
        let altitude = self.model().indicated_ft.is_some()
            .then(|| 1000u64.saturating_sub(self.altitude.age_ms.unwrap_or(1000)));
        let baro = (self.external && self.baro_inhg.is_finite() && (28.1..=31.).contains(&self.baro_inhg))
            .then(|| 1000u64.saturating_sub(self.baro_age_ms.unwrap_or(1000)));
        match (altitude, baro) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}

#[cfg(test)]
mod six_pack_tests {
    use super::*;
    #[test]
    fn scalar_expiry_uses_selected_quantity_and_preserves_age() {
        let mut app=App::new();app.apply(instrument_core::Command::LevelPreset).unwrap();app.tick(0);
        for (instrument,local_ttl) in [(Instrument::Heading,250),(Instrument::Vsi,1000)] {
            let mut s=Snapshot::from_app(&app);s.selected=instrument;
            for external in [false,true] {
                s.external=external;let ttl=if external{500}else{local_ttl};
                assert_eq!(s.numeric_lifetime_ms(),Some(ttl));
                assert_eq!(s.aged(ttl-1).displayed_quality(),Some(Quality::Valid));
                let expired=s.aged(ttl);
                assert_eq!(expired.displayed_age_ms(),Some(ttl));
                assert_eq!(expired.displayed_quality(),Some(Quality::Stale));
                assert_eq!(expired.numeric_lifetime_ms(),None);
            }
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
    #[test]
    fn ias_uses_its_existing_source_freshness_budget_without_extending_sample_age() {
        let mut app=instrument_core::App::new();
        app.apply(instrument_core::Command::LevelPreset).unwrap();app.tick(0);
        app.apply(instrument_core::Command::Select(instrument_core::Instrument::Airspeed)).unwrap();
        let mut s=super::Snapshot::from_app(&app);
        assert_eq!(s.numeric_lifetime_ms(),Some(1000));
        s.external=true;s.external_attitude=true;s.ias.age_ms=Some(450);
        assert_eq!(s.numeric_lifetime_ms(),Some(50));
        assert_eq!(s.aged(49).displayed_age_ms(),Some(499));
        assert_eq!(s.aged(50).displayed_quality(),Some(instrument_core::Quality::Stale));
        assert_eq!(s.aged(50).numeric_lifetime_ms(),None);
        assert!(!s.airspeed_demo_limits());
    }
    #[test]
    fn turn_display_lifetimes_preserve_independent_axes_and_source_policy() {
        use super::*;
        use instrument_core::Command;
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.tick(0);
        app.apply(Command::Select(Instrument::Turn)).unwrap();
        let mut s = Snapshot::from_app(&app);
        s.turn_rate.age_ms = Some(70);
        s.ball.age_ms = Some(90);
        assert_eq!(s.numeric_lifetime_ms(), Some(10));
        s.external = true;
        s.external_attitude = true;
        assert_eq!(s.numeric_lifetime_ms(), Some(110));
        let a = s.aged(109);
        assert_eq!(a.ball.age_ms, Some(199));
        assert_eq!(a.numeric_lifetime_ms(), Some(1));
        let a = s.aged(110);
        assert!(a.turn_model().aircraft_deg.is_some() && a.turn_model().ball.is_none());
        assert_eq!(a.numeric_lifetime_ms(), Some(20));
        assert_eq!(a.ball.age_ms, Some(200));
        assert_eq!(s.aged(130).numeric_lifetime_ms(), None);
        assert!(s.aged(130).turn_model().aircraft_deg.is_none());
    }
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
                device: instrument_core::device::Info::default(),
            heading: instrument_core::Reading { value: Some(0.), quality: instrument_core::Quality::Valid, age_ms: Some(0) },
            vsi: instrument_core::Reading { value: Some(0.), quality: instrument_core::Quality::Valid, age_ms: Some(0) },
            ias: instrument_core::Reading { value: Some(120.), quality: instrument_core::Quality::Valid, age_ms: Some(0) },
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
    fn external_attitude_expires_at_200ms_without_changing_local_policy_or_ages() {
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
        assert_eq!(snapshot.numeric_lifetime_ms(), Some(110));
        let live = snapshot.aged(109);
        assert_eq!(live.displayed_age_ms(), Some(199));
        assert_eq!(live.numeric_lifetime_ms(), Some(1));
        assert_eq!(live.attitude_model().quality, Quality::Valid);
        let stale = snapshot.aged(110);
        assert_eq!(stale.displayed_age_ms(), Some(200));
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
                device: instrument_core::device::Info::default(),
            heading: instrument_core::Reading { value: Some(0.), quality: instrument_core::Quality::Valid, age_ms: Some(0) },
            vsi: instrument_core::Reading { value: Some(0.), quality: instrument_core::Quality::Valid, age_ms: Some(0) },
            ias: instrument_core::Reading { value: Some(120.), quality: instrument_core::Quality::Valid, age_ms: Some(0) },
            turn_rate: Reading {
                value: Some(0.),
                quality: Quality::Valid,
                age_ms: Some(0),
            },
            ball: Reading {
                value: Some(0.),
                quality: Quality::Valid,
                age_ms: Some(0),
            },
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
            source_running: false,
            source_session: 0,
            source_sequence: 0,
            source_received_ms: 0,
            pressure_altitude: instrument_core::Reading {value: Some(4520.), quality: Quality::Valid, age_ms: Some(0)},
            baro_age_ms: None,
            calibration: None,
            gauge_profile: None,
            baro_quality: Quality::Valid,
            source_label: "SIM INPUT",
            kollsman_pressed: false,
            altitude_hidden: false,
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
