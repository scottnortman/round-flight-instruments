#![no_std]
#![forbid(unsafe_code)]
pub mod attitude_demo;
mod baro_control;
pub mod external;
pub mod takeoff;
pub use simulator_link;

/// Bench verification limits, not validated aircraft operating limits.
// Generate bench-source samples at 100 Hz to budget acquisition age separately
// from rendering and confirmed LCD transfer. External timestamps are unchanged.
pub const SOURCE_PERIOD_MS: u64 = 10;
pub const MAX_UPDATE_GAP_MS: u64 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Instrument {
    Altimeter,
    Attitude,
    Airspeed,
    Heading,
    Turn,
    Vsi,
}
impl Instrument {
    pub const ALL: [Self; 6] = [
        Self::Altimeter,
        Self::Attitude,
        Self::Airspeed,
        Self::Heading,
        Self::Turn,
        Self::Vsi,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Altimeter => "altimeter",
            Self::Attitude => "attitude",
            Self::Airspeed => "airspeed",
            Self::Heading => "heading",
            Self::Turn => "turn",
            Self::Vsi => "vsi",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|i| i.name() == s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Field {
    PressureAltitudeFt,
    PitchDeg,
    RollDeg,
    HeadingDeg,
    IasKt,
    TurnDps,
    Ball,
    VsiFpm,
}
impl Field {
    pub const ALL: [Self; 8] = [
        Self::PressureAltitudeFt,
        Self::PitchDeg,
        Self::RollDeg,
        Self::HeadingDeg,
        Self::IasKt,
        Self::TurnDps,
        Self::Ball,
        Self::VsiFpm,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::PressureAltitudeFt => "pressure_altitude_ft",
            Self::PitchDeg => "pitch_deg",
            Self::RollDeg => "roll_deg",
            Self::HeadingDeg => "heading_deg",
            Self::IasKt => "ias_kt",
            Self::TurnDps => "turn_dps",
            Self::Ball => "ball",
            Self::VsiFpm => "vsi_fpm",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.name() == s)
    }
    pub fn accepts(self, value: f32) -> bool {
        if !value.is_finite() {
            return false;
        }
        match self {
            Self::PressureAltitudeFt => (-2000.0..=60000.0).contains(&value),
            Self::PitchDeg => (-90.0..=90.0).contains(&value),
            Self::RollDeg => (-180.0..=180.0).contains(&value),
            Self::HeadingDeg => (0.0..360.0).contains(&value),
            Self::IasKt => (0.0..=200.0).contains(&value),
            Self::TurnDps => (-30.0..=30.0).contains(&value),
            Self::Ball => (-1.0..=1.0).contains(&value),
            Self::VsiFpm => (-6000.0..=6000.0).contains(&value),
        }
    }
    pub fn stale_after_ms(self) -> u64 {
        match self {
            Self::PitchDeg | Self::RollDeg | Self::TurnDps | Self::Ball => 100,
            Self::HeadingDeg => 250,
            _ => 1000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    Missing,
    Valid,
    Stale,
    Failed,
    TimingFault,
}
impl Quality {
    pub fn name(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Valid => "valid",
            Self::Stale => "stale",
            Self::Failed => "failed",
            Self::TimingFault => "timing_fault",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub value: Option<f32>,
    pub quality: Quality,
    pub age_ms: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sample {
    value: f32,
    at_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImuCommand {
    Estimator,
    Audit,
    SelfTest,
    On,
    Off,
    Status,
    Read,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    SourceDemo,
    SourceExternal(simulator_link::Provider, u64),
    SourceStatus,
    Telemetry(simulator_link::Frame),
    AttitudeDemo(u32),
    AttitudeDemoStop,
    AttitudeDemoStatus,
    Takeoff(u32),
    TakeoffStop,
    TakeoffStatus,
    Echo(Option<bool>),
    Display(DisplayCommand),
    Imu(ImuCommand),
    Help,
    Status,
    Instruments,
    Select(Instrument),
    Set(Field, f32),
    Clear(Field),
    Pause,
    Resume,
    LevelPreset,
    Fault(Field, bool),
    Baro(f32),
    ControlEnable(u64),
    ControlPoll,
    ControlReply(simulator_link::control::Reply),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DisplayCommand {
    TouchSound(bool),
    TouchSoundStatus,
    TouchStatus,
    SampleStatus,
    TouchHold(u32),
    TouchCancel,
    Stats,
    StatsReset,
    StatsStop,
    PixelClock(u32),
    Status,
    On,
    Off,
    Pattern,
    Gauge,
    Stall,
    TransferStall,
    Recover,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyError {
    NotReady,
    OutOfRange,
    Unsupported,
}

/// Single-writer application state. A renderer receives readings, never raw stale values.
pub struct App {
    external: Option<external::External>,
    highest_session: u64,
    control: baro_control::BaroControl,
    takeoff: takeoff::Takeoff,
    attitude_demo: attitude_demo::Demo,
    selected: Instrument,
    running: bool,
    baro_inhg: f32,
    timing_fault: bool,
    max_update_gap_ms: u64,
    tick_count: u64,
    held: [Option<f32>; 8],
    samples: [Option<Sample>; 8],
    faults: [bool; 8],
    last_tick: Option<u64>,
    publish_phase_ms: Option<u64>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
impl App {
    pub fn attitude_demo(&self) -> attitude_demo::Demo {
        self.attitude_demo
    }
    pub fn external(&self) -> Option<external::External> {
        self.external
    }
    pub fn source_name(&self) -> &'static str {
        self.external.map_or("sim", |e| e.provider.name())
    }
    pub fn source_label(&self) -> &'static str {
        if self.control_data_ready() {
            match self.control_status() {
                Some(simulator_link::control::Status::Sent) => return "BARO PENDING",
                Some(simulator_link::control::Status::Timeout) => return "BARO TIMEOUT",
                Some(simulator_link::control::Status::Rejected) => return "BARO REJECTED",
                _ => (),
            }
        }
        self.external.map_or("SIM INPUT", |e| {
            if e.age(self.now_ms())
                .is_some_and(|a| a >= simulator_link::STALE_MS)
            {
                return "LINK STALE";
            }
            match e.state() {
                simulator_link::Lifecycle::Loading => "LOADING",
                simulator_link::Lifecycle::Paused => "PAUSED",
                simulator_link::Lifecycle::Disconnected => "NO LINK",
                simulator_link::Lifecycle::Running => match e.provider {
                    simulator_link::Provider::Synthetic => "TEST INPUT",
                    simulator_link::Provider::Replay => "REPLAY",
                    simulator_link::Provider::Msfs2020 => "MSFS 2020",
                    simulator_link::Provider::Dcs => "DCS WORLD",
                },
            }
        })
    }
    fn external_fault(&self, field: Option<Field>) -> Option<Quality> {
        if self.timing_fault {
            Some(Quality::TimingFault)
        } else if field.is_some_and(|f| self.faults[f as usize]) {
            Some(Quality::Failed)
        } else {
            None
        }
    }
    pub fn altimeter_reading(&self, now: u64) -> Reading {
        if let Some(e) = self.external {
            e.read(
                now,
                self.external_fault(Some(Field::PressureAltitudeFt)),
                |f| f.indicated_ft,
            )
        } else {
            self.reading(Field::PressureAltitudeFt, now)
        }
    }
    pub fn external_baro(&self, now: u64) -> Option<Reading> {
        self.external
            .map(|e| e.read(now, self.external_fault(None), |f| f.baro_inhg))
    }
    pub fn takeoff(&self) -> takeoff::Takeoff {
        self.takeoff
    }
    pub fn now_ms(&self) -> u64 {
        self.last_tick.unwrap_or(0)
    }
    pub fn selected(&self) -> Instrument {
        self.selected
    }
    pub fn running(&self) -> bool {
        self.running
    }
    pub fn baro_inhg(&self) -> f32 {
        self.baro_inhg
    }
    pub fn timing_fault(&self) -> bool {
        self.timing_fault
    }
    pub fn max_update_gap_ms(&self) -> u64 {
        self.max_update_gap_ms
    }
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }
    pub const fn new() -> Self {
        Self {
            external: None,
            highest_session: 0,
            control: baro_control::BaroControl {
                enabled: false,
                next_id: 0,
                pending: None,
            },
            takeoff: takeoff::Takeoff::new(),
            attitude_demo: attitude_demo::Demo::new(),
            selected: Instrument::Altimeter,
            running: true,
            baro_inhg: 29.92,
            timing_fault: false,
            max_update_gap_ms: 0,
            tick_count: 0,
            held: [None; 8],
            samples: [None; 8],
            faults: [false; 8],
            last_tick: None,
            publish_phase_ms: None,
        }
    }
    pub fn tick(&mut self, now: u64) {
        if let Some(last) = self.last_tick {
            match now.checked_sub(last) {
                Some(gap) => {
                    self.max_update_gap_ms = self.max_update_gap_ms.max(gap);
                    if gap > MAX_UPDATE_GAP_MS {
                        self.timing_fault = true;
                    }
                }
                None => self.timing_fault = true,
            }
        }
        self.last_tick = Some(now);
        self.refresh_control();
        self.tick_count = self.tick_count.saturating_add(1);
        if self.timing_fault || !self.running || self.external.is_some() {
            self.takeoff.stop(self.timing_fault);
            self.attitude_demo.stop(self.timing_fault);
            return;
        }
        if let Some([pitch, roll]) = self.attitude_demo.advance(now) {
            self.held[Field::PitchDeg as usize] = Some(pitch);
            self.held[Field::RollDeg as usize] = Some(roll);
        }
        if let Some(altitude) = self.takeoff.advance(now) {
            self.held[Field::PressureAltitudeFt as usize] = Some(altitude);
        }
        if self
            .publish_phase_ms
            .is_none_or(|last| now.saturating_sub(last) >= SOURCE_PERIOD_MS)
        {
            for f in Field::ALL {
                let i = f as usize;
                if !self.faults[i] {
                    self.samples[i] = self.held[i].map(|value| Sample { value, at_ms: now });
                }
            }
            // Preserve cadence across scheduler jitter. Publish only the current
            // sample, stamped above with actual acquisition time; never replay
            // missed slots or backdate data to this scheduling phase.
            let phase = self.publish_phase_ms.unwrap_or(now);
            self.publish_phase_ms = Some(now - (now - phase) % SOURCE_PERIOD_MS);
        }
    }
    pub fn reading(&self, field: Field, now: u64) -> Reading {
        if let Some(e) = self.external {
            return e.read(now, self.external_fault(Some(field)), |f| {
                if field == Field::PressureAltitudeFt {
                    f.pressure_ft
                } else {
                    simulator_link::Datum::Missing
                }
            });
        }
        let i = field as usize;
        let age = self.samples[i].and_then(|s| now.checked_sub(s.at_ms));
        let quality = if self.timing_fault || self.samples[i].is_some_and(|s| now < s.at_ms) {
            Quality::TimingFault
        } else if self.faults[i] {
            Quality::Failed
        } else if self.samples[i].is_none() {
            Quality::Missing
        } else if age.is_some_and(|a| a >= field.stale_after_ms()) {
            Quality::Stale
        } else {
            Quality::Valid
        };
        Reading {
            value: if quality == Quality::Valid {
                self.samples[i].map(|s| s.value)
            } else {
                None
            },
            quality,
            age_ms: age,
        }
    }
    pub fn apply(&mut self, command: Command) -> Result<(), ApplyError> {
        self.refresh_control();
        if self.external.is_some()
            && matches!(
                command,
                Command::AttitudeDemo(_)
                    | Command::AttitudeDemoStop
                    | Command::Takeoff(_)
                    | Command::TakeoffStop
                    | Command::Set(..)
                    | Command::Clear(_)
                    | Command::Pause
                    | Command::Resume
                    | Command::LevelPreset
            )
        {
            return Err(ApplyError::Unsupported);
        }
        match command {
            Command::SourceStatus => (),
            Command::SourceDemo => {
                self.external = None;
                self.control = Default::default();
                self.samples = [None; 8];
                self.held = [None; 8];
                self.publish_phase_ms = None;
                self.running = true;
                self.takeoff.stop(false);
                self.attitude_demo.stop(false);
            }
            Command::SourceExternal(provider, session) => {
                if self.timing_fault {
                    return Err(ApplyError::NotReady);
                }
                if session == 0 || session <= self.highest_session {
                    return Err(ApplyError::OutOfRange);
                }
                self.highest_session = session;
                self.external = Some(external::External::new(provider, session));
                self.control = Default::default();
                self.samples = [None; 8];
                self.held = [None; 8];
                self.takeoff.stop(false);
                self.attitude_demo.stop(false);
            }
            Command::Telemetry(frame) => {
                if self.timing_fault {
                    return Err(ApplyError::NotReady);
                }
                let now = self.now_ms();
                self.external
                    .as_mut()
                    .ok_or(ApplyError::Unsupported)?
                    .accept(frame, now)?;
                if let simulator_link::Datum::Value(v) = frame.baro_inhg {
                    self.baro_inhg = v;
                }
            }
            Command::AttitudeDemo(seconds) => {
                if !(5..=600).contains(&seconds) {
                    return Err(ApplyError::OutOfRange);
                }
                if self.timing_fault
                    || self.faults[Field::PitchDeg as usize]
                    || self.faults[Field::RollDeg as usize]
                {
                    return Err(ApplyError::NotReady);
                }
                self.takeoff.stop(false);
                self.attitude_demo.start(self.now_ms(), seconds);
                self.selected = Instrument::Attitude;
                self.running = true;
                for field in [Field::PitchDeg, Field::RollDeg] {
                    self.held[field as usize] = Some(0.);
                    self.samples[field as usize] = None;
                }
                self.publish_phase_ms = None;
            }
            Command::AttitudeDemoStop => self.attitude_demo.stop(false),
            Command::AttitudeDemoStatus => (),
            Command::Takeoff(seconds) => {
                if !(5..=600).contains(&seconds) {
                    return Err(ApplyError::OutOfRange);
                }
                if self.timing_fault || self.faults[Field::PressureAltitudeFt as usize] {
                    return Err(ApplyError::NotReady);
                }
                self.attitude_demo.stop(false);
                self.takeoff.start(self.now_ms(), seconds);
                self.selected = Instrument::Altimeter;
                self.baro_inhg = 29.92;
                self.running = true;
                self.held[Field::PressureAltitudeFt as usize] = Some(0.);
                // Never present the previous altitude as the new sweep's zero.
                self.samples[Field::PressureAltitudeFt as usize] = None;
                self.publish_phase_ms = None;
            }
            Command::TakeoffStop => self.takeoff.stop(false),
            Command::TakeoffStatus => (),
            Command::Echo(_) | Command::Imu(_) | Command::Display(_) => {
                return Err(ApplyError::Unsupported)
            }
            Command::Select(i) => self.selected = i,
            Command::Set(f, v) => {
                if !f.accepts(v) {
                    return Err(ApplyError::OutOfRange);
                }
                if f == Field::PressureAltitudeFt {
                    self.takeoff.stop(false);
                }
                if matches!(f, Field::PitchDeg | Field::RollDeg) {
                    self.attitude_demo.stop(false);
                }
                self.held[f as usize] = Some(v);
            }
            Command::Clear(f) => {
                if f == Field::PressureAltitudeFt {
                    self.takeoff.stop(false);
                }
                if matches!(f, Field::PitchDeg | Field::RollDeg) {
                    self.attitude_demo.stop(false);
                }
                self.held[f as usize] = None;
                self.samples[f as usize] = None;
            }
            Command::Pause => {
                self.takeoff.stop(false);
                self.attitude_demo.stop(false);
                self.running = false;
            }
            Command::Resume => self.running = true,
            Command::LevelPreset => {
                self.takeoff.stop(false);
                self.attitude_demo.stop(false);
                self.held = [
                    Some(4520.0),
                    Some(0.0),
                    Some(0.0),
                    Some(0.0),
                    Some(120.0),
                    Some(0.0),
                    Some(0.0),
                    Some(0.0),
                ];
            }
            Command::Fault(f, active) => {
                if f == Field::PressureAltitudeFt {
                    if let Some(external) = &mut self.external {
                        external.clear_altitude();
                    }
                }
                if f == Field::PressureAltitudeFt && active {
                    self.takeoff.stop(true);
                }
                if matches!(f, Field::PitchDeg | Field::RollDeg) && active {
                    self.attitude_demo.stop(true);
                }
                self.faults[f as usize] = active;
                self.samples[f as usize] = None;
            }
            Command::ControlEnable(session) => self.enable_control(session)?,
            Command::ControlPoll => (),
            Command::ControlReply(reply) => self.control_reply(reply)?,
            Command::Baro(v) => {
                if self.external.is_some() {
                    return self.request_baro(v);
                }
                if !v.is_finite() || !(28.1..=31.0).contains(&v) {
                    return Err(ApplyError::OutOfRange);
                }
                self.baro_inhg = v;
            }
            Command::Help | Command::Status | Command::Instruments => (),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_jitter_preserves_cadence_and_actual_acquisition_age() {
        let mut app = App::new();
        app.apply(Command::Set(Field::PitchDeg, 0.)).unwrap();
        app.tick(0);
        app.apply(Command::Set(Field::PitchDeg, 9.)).unwrap();
        app.tick(9);
        assert_eq!(app.reading(Field::PitchDeg, 9).value, Some(0.));
        for now in [19, 29, 39, 59, 69] {
            app.apply(Command::Set(Field::PitchDeg, now as f32))
                .unwrap();
            app.tick(now);
            let reading = app.reading(Field::PitchDeg, now + 1);
            assert_eq!(reading.value, Some(now as f32));
            assert_eq!(reading.age_ms, Some(1));
        }
        app.apply(Command::Pause).unwrap();
        app.tick(79);
        assert_eq!(app.reading(Field::PitchDeg, 79).age_ms, Some(10));
        app.tick(89);
        assert_eq!(app.reading(Field::PitchDeg, 89).age_ms, Some(20));
        app.apply(Command::Resume).unwrap();
        app.tick(99);
        assert_eq!(app.reading(Field::PitchDeg, 99).age_ms, Some(0));
    }
    #[test]
    fn attitude_demo_is_bounded_and_manual_pause_fault_and_source_changes_stop_it() {
        let mut a = App::new();
        a.tick(0);
        for seconds in [0, 4, 601, u32::MAX] {
            assert_eq!(
                a.apply(Command::AttitudeDemo(seconds)),
                Err(ApplyError::OutOfRange)
            );
        }
        a.apply(Command::AttitudeDemo(5)).unwrap();
        for now in (20..=5000).step_by(20) {
            a.tick(now);
            let pitch = a.reading(Field::PitchDeg, now).value.unwrap();
            let roll = a.reading(Field::RollDeg, now).value.unwrap();
            assert!((-6.001..=8.001).contains(&pitch) && (-30.001..=30.001).contains(&roll));
        }
        assert_eq!(a.attitude_demo().state, takeoff::State::Complete);
        assert_eq!(a.reading(Field::PitchDeg, 5000).value, Some(0.));
        assert_eq!(a.reading(Field::RollDeg, 5000).value, Some(0.));
        for command in [
            Command::Set(Field::PitchDeg, 5.),
            Command::Clear(Field::RollDeg),
            Command::Pause,
            Command::LevelPreset,
        ] {
            a.apply(Command::AttitudeDemo(5)).unwrap();
            a.apply(command).unwrap();
            assert!(!a.attitude_demo().running());
        }
        a.apply(Command::AttitudeDemo(5)).unwrap();
        a.apply(Command::Set(Field::RollDeg, f32::NAN)).unwrap_err();
        assert!(a.attitude_demo().running());
        a.apply(Command::Fault(Field::RollDeg, true)).unwrap();
        assert_eq!(a.attitude_demo().state, takeoff::State::Faulted);
        assert_eq!(a.apply(Command::AttitudeDemo(5)), Err(ApplyError::NotReady));
        a.apply(Command::Fault(Field::RollDeg, false)).unwrap();
        a.apply(Command::AttitudeDemo(5)).unwrap();
        a.apply(Command::SourceExternal(simulator_link::Provider::Replay, 1))
            .unwrap();
        assert!(!a.attitude_demo().running());
        assert_eq!(
            a.apply(Command::AttitudeDemo(5)),
            Err(ApplyError::Unsupported)
        );
        assert!(a.reading(Field::PitchDeg, 5000).value.is_none());
        let mut a = App::new();
        a.tick(0);
        a.apply(Command::AttitudeDemo(5)).unwrap();
        a.tick(101);
        assert_eq!(a.attitude_demo().state, takeoff::State::Faulted);
    }
    #[test]
    fn takeoff_has_ground_hold_monotonic_climb_and_exact_endpoint() {
        let mut a = App::new();
        a.tick(0);
        a.apply(Command::Takeoff(5)).unwrap();
        assert_eq!(
            a.reading(Field::PressureAltitudeFt, 0).quality,
            Quality::Missing
        );
        let mut previous = 0.;
        for now in (20..=6200).step_by(20) {
            a.tick(now);
            let value = a.reading(Field::PressureAltitudeFt, now).value.unwrap();
            assert!(value >= previous && value <= 10_000.);
            if now <= 1000 {
                assert_eq!(value, 0.);
            }
            if now == 3500 {
                assert_eq!(value, 5000.);
            }
            if now >= 6000 {
                assert_eq!(value, 10_000.);
            }
            previous = value;
        }
        assert_eq!(a.takeoff().state, takeoff::State::Complete);
        assert_eq!(a.takeoff().elapsed_ms, 6000);
        assert!(!a.timing_fault());
    }
    #[test]
    fn takeoff_stop_manual_override_and_pause_do_not_restart_the_ramp() {
        let mut a = App::new();
        a.tick(0);
        a.apply(Command::Takeoff(5)).unwrap();
        for now in (20..=2000).step_by(20) {
            a.tick(now);
        }
        a.apply(Command::TakeoffStop).unwrap();
        let held = a.reading(Field::PressureAltitudeFt, 2000).value;
        a.tick(2020);
        assert_eq!(a.reading(Field::PressureAltitudeFt, 2020).value, held);
        assert_eq!(a.takeoff().state, takeoff::State::Stopped);
        a.apply(Command::Takeoff(5)).unwrap();
        assert!(a
            .apply(Command::Set(Field::PressureAltitudeFt, f32::NAN))
            .is_err());
        assert!(a.takeoff().running());
        a.apply(Command::Set(Field::PressureAltitudeFt, 8500.))
            .unwrap();
        a.tick(2040);
        assert_eq!(
            a.reading(Field::PressureAltitudeFt, 2040).value,
            Some(8500.)
        );
        assert!(!a.takeoff().running());
        a.apply(Command::Takeoff(5)).unwrap();
        a.tick(2060);
        a.apply(Command::Pause).unwrap();
        assert_eq!(
            a.reading(Field::PressureAltitudeFt, 3060).quality,
            Quality::Stale
        );
        a.apply(Command::Resume).unwrap();
        a.tick(2080);
        assert!(!a.takeoff().running());
    }
    #[test]
    fn takeoff_preserves_faults_and_rejects_invalid_start_atomically() {
        let mut a = App::new();
        a.tick(0);
        a.apply(Command::Set(Field::PressureAltitudeFt, 8500.))
            .unwrap();
        a.apply(Command::Baro(30.12)).unwrap();
        for seconds in [0, 4, 601, u32::MAX] {
            assert_eq!(
                a.apply(Command::Takeoff(seconds)),
                Err(ApplyError::OutOfRange)
            );
        }
        a.tick(20);
        assert_eq!(a.reading(Field::PressureAltitudeFt, 20).value, Some(8500.));
        assert_eq!(a.baro_inhg(), 30.12);
        a.apply(Command::Fault(Field::PressureAltitudeFt, true))
            .unwrap();
        assert_eq!(a.apply(Command::Takeoff(5)), Err(ApplyError::NotReady));
        a.apply(Command::Fault(Field::PressureAltitudeFt, false))
            .unwrap();
        a.apply(Command::Takeoff(5)).unwrap();
        a.tick(40);
        a.apply(Command::Fault(Field::PressureAltitudeFt, true))
            .unwrap();
        assert_eq!(a.takeoff().state, takeoff::State::Faulted);
        assert_eq!(a.reading(Field::PressureAltitudeFt, 40).value, None);
        a.apply(Command::Fault(Field::PressureAltitudeFt, false))
            .unwrap();
        a.apply(Command::Takeoff(5)).unwrap();
        a.tick(141);
        assert_eq!(a.takeoff().state, takeoff::State::Faulted);
        assert_eq!(a.apply(Command::Takeoff(5)), Err(ApplyError::NotReady));
    }
    #[test]
    fn missing_is_not_zero_and_boundary_is_stale() {
        let mut a = App::new();
        assert_eq!(a.reading(Field::PitchDeg, 0).quality, Quality::Missing);
        a.apply(Command::Set(Field::PitchDeg, 0.0)).unwrap();
        a.tick(0);
        assert_eq!(a.reading(Field::PitchDeg, 99).value, Some(0.0));
        assert_eq!(a.reading(Field::PitchDeg, 100).value, None);
        assert_eq!(a.reading(Field::PitchDeg, 100).quality, Quality::Stale);
    }
    #[test]
    fn pause_expires_fields_independently_and_reading_never_refreshes() {
        let mut a = App::new();
        a.apply(Command::LevelPreset).unwrap();
        a.tick(0);
        a.apply(Command::Pause).unwrap();
        assert_eq!(a.reading(Field::PitchDeg, 100).quality, Quality::Stale);
        assert_eq!(
            a.reading(Field::PressureAltitudeFt, 100).quality,
            Quality::Valid
        );
        assert_eq!(
            a.reading(Field::PressureAltitudeFt, 1000).quality,
            Quality::Stale
        );
        assert_eq!(a.reading(Field::PitchDeg, 1001).age_ms, Some(1001));
    }
    #[test]
    fn invalid_values_cannot_change_previous_input() {
        let mut a = App::new();
        a.apply(Command::Set(Field::IasKt, 120.0)).unwrap();
        a.tick(0);
        for v in [f32::NAN, f32::INFINITY, -1.0, 201.0] {
            assert_eq!(
                a.apply(Command::Set(Field::IasKt, v)),
                Err(ApplyError::OutOfRange)
            );
        }
        a.tick(20);
        assert_eq!(a.reading(Field::IasKt, 20).value, Some(120.0));
        assert!(a.apply(Command::Baro(f32::NAN)).is_err());
        assert_eq!(a.baro_inhg, 29.92);
    }
    #[test]
    fn fault_recovery_requires_new_sample_and_preserves_other_fields() {
        let mut a = App::new();
        a.apply(Command::LevelPreset).unwrap();
        a.tick(0);
        a.apply(Command::Fault(Field::PitchDeg, true)).unwrap();
        a.tick(20);
        assert_eq!(a.reading(Field::PitchDeg, 20).quality, Quality::Failed);
        assert_eq!(a.reading(Field::IasKt, 20).quality, Quality::Valid);
        a.apply(Command::Fault(Field::PitchDeg, false)).unwrap();
        assert_eq!(a.reading(Field::PitchDeg, 20).quality, Quality::Missing);
        a.tick(40);
        assert_eq!(a.reading(Field::PitchDeg, 40).quality, Quality::Valid);
    }
    #[test]
    fn scheduling_fault_latches_and_cannot_be_cleared_by_resume() {
        let mut a = App::new();
        a.apply(Command::LevelPreset).unwrap();
        a.tick(0);
        a.tick(101);
        assert!(a.timing_fault);
        a.apply(Command::Resume).unwrap();
        a.tick(121);
        assert_eq!(a.reading(Field::IasKt, 121).quality, Quality::TimingFault);
        let mut b = App::new();
        b.tick(20);
        b.tick(19);
        assert!(b.timing_fault);
    }
    #[test]
    fn view_switch_does_not_reset_source_or_references() {
        let mut a = App::new();
        a.apply(Command::LevelPreset).unwrap();
        a.tick(0);
        a.apply(Command::Baro(30.12)).unwrap();
        a.apply(Command::Select(Instrument::Attitude)).unwrap();
        assert_eq!(a.reading(Field::PressureAltitudeFt, 10).value, Some(4520.0));
        assert_eq!(a.baro_inhg, 30.12);
    }
    #[test]
    fn field_envelopes_accept_boundaries_and_reject_invalid_neighbors() {
        for (field, low, high) in [
            (Field::PressureAltitudeFt, -2000.0, 60000.0),
            (Field::PitchDeg, -90.0, 90.0),
            (Field::RollDeg, -180.0, 180.0),
            (Field::IasKt, 0.0, 200.0),
            (Field::TurnDps, -30.0, 30.0),
            (Field::Ball, -1.0, 1.0),
            (Field::VsiFpm, -6000.0, 6000.0),
        ] {
            assert!(field.accepts(low));
            assert!(field.accepts(high));
            for invalid in [
                low - 1.0,
                high + 1.0,
                f32::NAN,
                f32::NEG_INFINITY,
                f32::INFINITY,
            ] {
                assert!(!field.accepts(invalid), "{field:?}: {invalid}");
            }
        }
        assert!(Field::HeadingDeg.accepts(0.0));
        assert!(Field::HeadingDeg.accepts(359.999));
        assert!(!Field::HeadingDeg.accepts(360.0));
        assert!(!Field::HeadingDeg.accepts(-0.001));
    }
}
