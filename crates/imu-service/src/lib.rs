#![forbid(unsafe_code)]

use bounded_mailbox::{self as mpsc, TryRecvError, TrySendError};

pub use attitude_estimator::mount::Mount;
use attitude_estimator::{
    AttitudeEstimator, Calibration, Estimate, Frame, InputQuality, Observation, Output, Pipeline,
    Quality, ResetReason,
};
use instrument_core::ImuCommand;
use qmi8658::{Bus, Driver, Error, Identity, Sample, SelfTest};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex, TryLockError,
    },
    thread,
    time::{Duration, Instant},
};
use text_cli::{reply, Response};

pub use attitude_estimator::calibration;
use calibration::{Record, Session, Phase, Reason, View as CalibrationView};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreStatus { Busy, Unavailable, Pending, Saved(u32), Failed(i32) }

pub const STALE_MS: u128 = 100;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Testing,
    Off,
    Starting,
    Running,
    Failed,
}
impl State {
    fn name(self) -> &'static str {
        match self {
            Self::Testing => "testing",
            Self::Off => "off",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Failed => "failed",
        }
    }
}
#[derive(Clone, Copy)]
struct Snapshot {
    calibration: CalibrationView,
    profile: Record,
    storage_available: bool,
    estimate_rejections: u64,
    raw_tilt_rejections: u64,
    estimator_name: &'static str,
    estimate: Option<Output>,
    audit: Option<qmi8658::Audit>,
    request: u32,
    completed: u32,
    state: State,
    identity: Option<Identity>,
    sample: Option<(Sample, Instant)>,
    samples: u64,
    skipped: u64,
    duplicates: u64,
    errors: u64,
    max_gap_ms: u128,
    last_error: Option<Error>,
    self_test: Option<SelfTest>,
}
impl Snapshot {
    fn new() -> Self {
        Self {
            calibration: CalibrationView::default(),
            profile: Record::default(),
            storage_available: false,
            estimate_rejections: 0,
            raw_tilt_rejections: 0,
            estimator_name: "accel_tilt",
            estimate: None,
            audit: None,
            request: 0,
            completed: 0,
            state: State::Off,
            identity: None,
            sample: None,
            samples: 0,
            skipped: 0,
            duplicates: 0,
            errors: 0,
            max_gap_ms: 0,
            last_error: None,
            self_test: None,
        }
    }
    fn accept(&mut self, sample: Sample, now: Instant) -> Result<(), Error> {
        if let Some((old, at)) = self.sample {
            let delta = sample.counter.wrapping_sub(old.counter) & 0xffffff;
            if delta == 0 {
                self.duplicates += 1;
                return Ok(());
            }
            if delta > 0x7fffff {
                return Err(Error::CounterRegression);
            }
            self.skipped += (delta - 1) as u64;
            self.max_gap_ms = self.max_gap_ms.max(now.duration_since(at).as_millis());
        }
        self.samples += 1;
        self.sample = Some((sample, now));
        Ok(())
    }
    fn quality(&self, now: Instant) -> &'static str {
        if self.state != State::Running {
            return self.state.name();
        }
        match self.sample {
            None => "missing",
            Some((_, at)) if now.duration_since(at).as_millis() >= STALE_MS => "stale",
            Some((s, _)) if s.saturated() => "saturated",
            Some(_) => "valid",
        }
    }
}

pub struct Service {
    calibration_reserved: Arc<AtomicBool>,
    commands: mpsc::SyncSender<(u32, ImuCommand)>,
    next_request: AtomicU32,
    snapshot: Arc<Mutex<Snapshot>>,
}
impl Service {
    pub fn start<B: Bus + Send + 'static>(bus: B) -> io::Result<Self> {
        Self::start_with_estimator(bus, BenchTilt)
    }
    /// Replacement estimators run only on the sensor worker. Their update/reset
    /// methods must be bounded and must not perform I/O.
    pub fn start_with_estimator<B: Bus + Send + 'static, E: AttitudeEstimator + Send + 'static>(
        bus: B,
        estimator: E,
    ) -> io::Result<Self> {
        Self::start_with_bus_poll(bus, estimator, |_, _| true)
    }
    /// One worker owns the bus. Poll other device addresses only between
    /// complete IMU operations, including while IMU sampling is off. The hook
    /// must use bounded I/O; consumers must expire data during long self-tests.
    /// The second argument is true before a queued IMU command: silence timed
    /// outputs then. Return false to reject that command if quiescing fails.
    pub fn start_with_bus_poll<B, E, P>(bus: B, estimator: E, poll: P) -> io::Result<Self>
    where
        B: Bus + Send + 'static,
        E: AttitudeEstimator + Send + 'static,
        P: FnMut(&mut B, bool) -> bool + Send + 'static,
    {
        Self::start_with_mount(bus, estimator, None, poll)
    }
    /// Fixed board mounting is applied to estimator inputs only. Raw diagnostics
    /// remain sensor-native; mounting never upgrades unverified calibration.
    pub fn start_with_mount<B, E, P>(
        bus: B,
        estimator: E,
        mount: Option<Mount>,
        poll: P,
    ) -> io::Result<Self>
    where
        B: Bus + Send + 'static,
        E: AttitudeEstimator + Send + 'static,
        P: FnMut(&mut B, bool) -> bool + Send + 'static,
    {
        Self::start_with_calibration(bus, estimator, mount, poll, None, |_| StoreStatus::Unavailable)
    }
    pub fn start_with_calibration<B, E, P, F>(
        bus: B, estimator: E, mount: Option<Mount>, mut poll: P,
        initial: Option<Record>, mut persist: F,
    ) -> io::Result<Self>
    where
        B: Bus + Send + 'static,
        E: AttitudeEstimator + Send + 'static,
        P: FnMut(&mut B, bool) -> bool + Send + 'static,
        F: FnMut(Option<Record>) -> StoreStatus + Send + 'static,
    {
        let storage_available = !matches!(persist(None), StoreStatus::Unavailable | StoreStatus::Failed(_));
        let calibration_reserved = Arc::new(AtomicBool::new(false));
        let worker_reserved = calibration_reserved.clone();
        let (commands, receiver) = mpsc::sync_channel(2);
        let mut initial_snapshot = Snapshot::new();
        initial_snapshot.storage_available = storage_available;
        initial_snapshot.profile = initial.filter(|r| r.valid()).unwrap_or_default();
        initial_snapshot.calibration.calibrated = initial_snapshot.profile.level;
        initial_snapshot.calibration.generation = initial_snapshot.profile.generation;
        let snapshot = Arc::new(Mutex::new(initial_snapshot));
        let shared = snapshot.clone();
        thread::Builder::new()
            .name("imu".into())
            .stack_size(12288)
            // Keep the large capture out of the nested pthread/unwind launch
            // frames on Xtensa. Only the boxed pointer crosses those frames.
            .spawn(Box::new(move || {
                let mut driver = Driver::new(bus);
                let mut state = initial_snapshot;
                let mut profile = state.profile;
                let mut session = Session::default();
                let mut restore_off = false;
                let mut save_enqueued = false;
                let mut pipeline = Pipeline::new(estimator);
                let clock = Instant::now();
                let mut epoch = 0u32;
                state.estimator_name = pipeline.name();
                loop {
                    let poll_started = Instant::now();
                    poll(driver.bus_mut(), false);
                    let received = receiver.try_recv();
                    if let Ok((request, command)) = received {
                        // Review/save/cancel are short local operations. They
                        // must remain usable even if buzzer shutoff has failed.
                        let local = matches!(command, ImuCommand::CalibrationReset |
                            ImuCommand::CalibrationSave(_) | ImuCommand::CalibrationCancel(_));
                        if !local && !poll(driver.bus_mut(), true) {
                            if matches!(command, ImuCommand::CalibrateLevel | ImuCommand::CalibrateFull) {
                                session.start(request, command == ImuCommand::CalibrateFull, profile,
                                    clock.elapsed().as_micros() as u64);
                            }
                            if session.view.busy() && session.view.phase != Phase::Saving {
                                session.fail(Reason::Sensor);
                            }
                            if !session.view.busy() { worker_reserved.store(false, Ordering::Release); }
                            state.calibration = session.view;
                            state.request = request;
                            state.completed = request;
                            state.state = State::Failed;
                            state.sample = None;
                            state.estimate = None;
                            state.errors += 1;
                            state.last_error = Some(Error::Bus(-1));
                            pipeline.reset(ResetReason::InvalidInput);
                            if let Ok(mut s) = shared.lock() {
                                *s = state;
                            } else {
                                return;
                            }
                            thread::sleep(Duration::from_millis(20));
                            continue;
                        }
                    }
                    match received {
                        Ok((request, ImuCommand::CalibrationReset)) => {
                            session.prepare_reset(request,profile,clock.elapsed().as_micros() as u64);
                            restore_off=false;save_enqueued=false;state.request=request;state.completed=request;
                        }
                        Ok((request, command @ (ImuCommand::CalibrateLevel | ImuCommand::CalibrateFull))) => {
                            restore_off = state.state != State::Running;
                            let full = command == ImuCommand::CalibrateFull;
                            session.start(request, full, profile, clock.elapsed().as_micros() as u64);
                            state.request = request; state.calibration = session.view;
                            if let Ok(mut s) = shared.lock() { *s = state; } else { return; }
                            let result = (|| {
                                if full {
                                    let result = driver.self_test()?;
                                    state.self_test = Some(result);
                                    if !result.passed() { return Err(Error::AuditMismatch); }
                                }
                                if full || state.state != State::Running { driver.enable()?; }
                                let audit = driver.audit()?;
                                state.audit = Some(audit);
                                if !audit.configuration_matches() || !audit.data_match {return Err(Error::AuditMismatch);}
                                Ok(())
                            })();
                            match result {
                                Ok(()) => {
                                    session.start(request, full, profile, clock.elapsed().as_micros() as u64);
                                    state.state = State::Running;
                                    state.identity = driver.identity();
                                    state.sample = None; state.estimate = None;
                                    epoch = epoch.wrapping_add(1); pipeline.reset(ResetReason::SourceChanged);
                                    save_enqueued = false;
                                }
                                Err(error) => {
                                    session.fail(Reason::Sensor); state.state = State::Failed;
                                    state.errors += 1; state.last_error = Some(error);
                                    state.sample = None; state.estimate = None;
                                    let _ = driver.disable();
                                }
                            }
                            state.completed = request;
                        }
                        Ok((request, ImuCommand::CalibrationSave(job))) => {
                            if session.view.job == job && session.view.phase == Phase::Review {
                                session.view.phase = Phase::Saving; save_enqueued = false;
                            }
                            state.request = request; state.completed = request;
                        }
                        Ok((request, ImuCommand::CalibrationCancel(job))) => {
                            if session.view.job == job && session.view.phase != Phase::Saving {session.cancel();}
                            state.request = request; state.completed = request;
                        }
                        Ok((request, ImuCommand::SelfTest)) => {
                            pipeline.reset(ResetReason::SourceChanged);
                            epoch = epoch.wrapping_add(1);
                            state = Snapshot::new();
                            state.estimator_name = pipeline.name();
                            state.request = request;
                            state.state = State::Testing;
                            if let Ok(mut s) = shared.lock() {
                                *s = state;
                            } else {
                                return;
                            }
                            match driver.self_test() {
                                Ok(result) => {
                                    state.state = State::Off;
                                    state.self_test = Some(result);
                                }
                                Err(e) => {
                                    state.state = State::Failed;
                                    state.errors += 1;
                                    state.last_error = Some(e);
                                }
                            }
                            state.identity = driver.identity();
                            state.completed = request;
                        }
                        Ok((request, command)) => {
                            if command == ImuCommand::Audit {
                                state.audit = None;
                                if state.state == State::Running {
                                    match driver.audit() {
                                        Ok(audit) => {
                                            state.audit = Some(audit);
                                            if !audit.configuration_matches() || !audit.data_match {
                                                state.state = State::Failed;
                                                state.sample = None;
                                                state.estimate = None;
                                                state.errors += 1;
                                                state.last_error = Some(Error::AuditMismatch);
                                                pipeline.reset(ResetReason::InvalidInput);
                                                let _ = driver.disable();
                                            }
                                        }
                                        Err(e) => {
                                            state.state = State::Failed;
                                            state.sample = None;
                                            state.estimate = None;
                                            state.errors += 1;
                                            state.last_error = Some(e);
                                            pipeline.reset(ResetReason::InvalidInput);
                                            let _ = driver.disable();
                                        }
                                    }
                                }
                            }
                            if command == ImuCommand::On && session.view.busy() { restore_off = false; }
                            if command == ImuCommand::On && state.state != State::Running {
                                pipeline.reset(ResetReason::SourceChanged);
                                epoch = epoch.wrapping_add(1);
                                state = Snapshot::new();
                                state.estimator_name = pipeline.name();
                                state.request = request;
                                state.state = State::Starting;
                                if let Ok(mut s) = shared.lock() {
                                    *s = state;
                                } else {
                                    return;
                                }
                                match driver.enable() {
                                    Ok(id) => {
                                        state.identity = Some(id);
                                        state.state = State::Running;
                                    }
                                    Err(e) => {
                                        state.state = State::Failed;
                                        state.errors += 1;
                                        state.last_error = Some(e);
                                        state.identity = driver.identity();
                                    }
                                }
                            } else if command == ImuCommand::Off {
                                if session.view.busy() && session.view.phase != Phase::Saving { session.cancel(); }
                                restore_off = false;
                                pipeline.reset(ResetReason::SourceChanged);
                                state.estimate = None;
                                state.sample = None;
                                match driver.disable() {
                                    Ok(()) => {
                                        state.state = State::Off;
                                        state.last_error = None;
                                    }
                                    Err(e) => {
                                        state.state = State::Failed;
                                        state.errors += 1;
                                        state.last_error = Some(e);
                                    }
                                }
                            }
                            state.request = request;
                            state.completed = request;
                        }
                        Err(TryRecvError::Disconnected) => {
                            if poll(driver.bus_mut(), true) {
                                let _ = driver.disable();
                            }
                            return;
                        }
                        Err(TryRecvError::Empty) => (),
                    }
                    if state.state == State::Running {
                        let result = match driver.sample() {
                            Ok(Some(sample)) => {
                                let now = Instant::now();
                                let count = state.samples;
                                let result = state.accept(sample, now);
                                if result.is_ok() && state.samples != count {
                                    if tilt_degrees(mount.map_or_else(
                                        || sample.accel_g(),
                                        |m| m.apply(sample.accel_g()),
                                    ))
                                    .is_none()
                                    {
                                        state.raw_tilt_rejections += 1;
                                    }
                                    let raw_a = mount.map_or_else(|| sample.accel_g(), |m| m.apply(sample.accel_g()));
                                    let raw_g = mount.map_or_else(|| sample.gyro_dps(), |m| m.apply(sample.gyro_dps()));
                                    session.ingest(raw_a, raw_g, sample.temperature_c(), now.duration_since(clock).as_micros() as u64, sample.saturated());
                                    let (corrected_a, corrected_g) = if profile.level { profile.apply(raw_a, raw_g) } else { (raw_a, raw_g) };
                                    state.estimate = Some(pipeline.update(Observation {
                                        source_epoch: epoch,
                                        received_us: now.duration_since(clock).as_micros() as u64,
                                        acquired_us: None,
                                        sensor_counter: Some(sample.counter),
                                        frame: if mount.is_some() {
                                            Frame::BodyForwardRightDown
                                        } else {
                                            Frame::SensorNative
                                        },
                                        quality: if sample.saturated() {
                                            InputQuality::Saturated
                                        } else {
                                            InputQuality::Fresh
                                        },
                                        calibration: if profile.level && profile.temperature_ok(sample.temperature_c()) { Calibration::Verified } else { Calibration::Unverified },
                                        accel_g: corrected_a,
                                        gyro_dps: corrected_g,
                                        temperature_c: Some(sample.temperature_c()),
                                        accel_covariance_g2: None,
                                        gyro_covariance_dps2: None,
                                    }));
                                    if state
                                        .estimate
                                        .is_some_and(|o| o.estimate.quality == Quality::Unavailable)
                                    {
                                        state.estimate_rejections += 1;
                                    }
                                }
                                result
                            }
                            Ok(None) => Ok(()),
                            Err(e) => Err(e),
                        };
                        if let Err(e) = result {
                            state.state = State::Failed;
                            state.sample = None;
                            state.estimate = None;
                            pipeline.reset(ResetReason::InvalidInput);
                            state.errors += 1;
                            state.last_error = Some(e);
                            if poll(driver.bus_mut(), true) {
                                let _ = driver.disable();
                            }
                        }
                    }
                    session.check_timeout(clock.elapsed().as_micros() as u64);
                    if !session.view.reset && session.view.busy() && session.view.phase != Phase::Saving && state.state == State::Failed {session.fail(Reason::Sensor);}
                    if !session.view.reset && session.view.busy() && session.view.phase != Phase::Saving && state.sample.is_some_and(|(_, at)|at.elapsed().as_millis()>=100) {
                        session.fail(Reason::Gap);
                    }
                    if session.view.phase == Phase::Saving {
                        let pending = session.candidate.expect("saving requires reviewed candidate");
                        let status = persist((!save_enqueued).then_some(pending));
                        match status {
                            StoreStatus::Pending => save_enqueued = true,
                            StoreStatus::Saved(generation) if generation == pending.generation => {
                                profile = pending; session.view.phase = Phase::Saved;
                                session.view.generation = generation; session.view.calibrated = profile.level;
                                epoch = epoch.wrapping_add(1); pipeline.reset(ResetReason::CalibrationChanged);
                                state.estimate = None;
                            }
                            StoreStatus::Failed(_) | StoreStatus::Unavailable => session.fail(Reason::Storage),
                            _ => (),
                        }
                    }
                    if !session.view.busy() {
                        worker_reserved.store(false, Ordering::Release);
                        if restore_off {
                            restore_off = false;
                            if state.state == State::Running {
                                match driver.disable() {Ok(())=>state.state=State::Off,Err(e)=>{state.state=State::Failed;state.last_error=Some(e);state.errors+=1;}}
                                state.sample = None; state.estimate = None;
                                pipeline.reset(ResetReason::SourceChanged);
                            }
                        }
                    }
                    state.calibration = session.view;
                    state.calibration.calibrated = profile.level;
                    state.calibration.full_calibrated = profile.full;
                    state.calibration.accel_rejected = pipeline.acceleration_rejected();
                    state.calibration.storage_available = storage_available;
                    state.calibration.reference_temperature_centi=(profile.temperature_c*100.) as i32;
                    state.calibration.temperature_warning=profile.level && state.sample.is_some_and(|(sample,_)|!profile.temperature_ok(sample.temperature_c()));
                    if !matches!(session.view.phase,Phase::Review|Phase::Saving) {
                        state.calibration.temperature_available=state.sample.is_some();
                        if let Some((sample,_))=state.sample {state.calibration.temperature_centi=(sample.temperature_c()*100.) as i32;}
                    }
                    if !matches!(session.view.phase,Phase::Review|Phase::Saving) {state.calibration.high_bias=profile.high_bias();}
                    state.calibration.generation = profile.generation;
                    state.profile = profile; state.storage_available = storage_available;
                    if let Ok(mut s) = shared.lock() {
                        *s = state;
                    } else {
                        return;
                    }
                    // Schedule a 5 ms start-to-start budget, not another 5 ms
                    // after bus work. Always block at least one board RTOS tick;
                    // overruns do not cause a catch-up spin. Duplicate sensor
                    // counts still cannot renew receipt age.
                    thread::sleep(
                        Duration::from_millis(5)
                            .saturating_sub(poll_started.elapsed())
                            .max(Duration::from_millis(1)),
                    );
                }
            }))?;
        Ok(Self {
            calibration_reserved,
            commands,
            snapshot,
            next_request: AtomicU32::new(1),
        })
    }
    /// Nonblocking lifecycle hint for temporary display demonstrations. Testing
    /// is unknown so a showcase never interrupts an explicit self-test.
    pub fn acquiring(&self) -> Option<bool> {
        if self.calibration_reserved.load(Ordering::Acquire) { return None; }
        self.snapshot.try_lock().ok().and_then(|s| match s.state {
            State::Running | State::Starting => Some(true),
            State::Off | State::Failed => Some(false),
            State::Testing => None,
        })
    }
    pub fn calibration_view(&self) -> Option<CalibrationView> {
        self.snapshot.try_lock().ok().map(|s| s.calibration)
    }
    pub fn handle(&self, command: ImuCommand) -> Response {
        let starts = matches!(command, ImuCommand::CalibrateLevel | ImuCommand::CalibrateFull | ImuCommand::CalibrationReset);
        let calibration_command = starts || matches!(command, ImuCommand::CalibrationSave(_) | ImuCommand::CalibrationCancel(_));
        if starts {
            let Ok(s) = self.snapshot.try_lock() else {return reply(format_args!("ERR imu_busy"));};
            if !s.storage_available {return reply(format_args!("ERR calibration_storage_unavailable"));}
            if s.state == State::Testing || self.calibration_reserved.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err() {
                return reply(format_args!("ERR imu_calibration_busy"));
            }
        } else if self.calibration_reserved.load(Ordering::Acquire) && matches!(command, ImuCommand::SelfTest) {
            return reply(format_args!("ERR imu_calibration_busy"));
        }
        if let ImuCommand::CalibrationSave(job) | ImuCommand::CalibrationCancel(job) = command {
            let Ok(s) = self.snapshot.try_lock() else {return reply(format_args!("ERR imu_busy"));};
            if s.calibration.job != job || (matches!(command,ImuCommand::CalibrationSave(_)) && s.calibration.phase != Phase::Review)
                || s.calibration.phase == Phase::Saving {
                return reply(format_args!("ERR calibration_not_ready"));
            }
        }
        if calibration_command {
            let request=self.next_request.fetch_add(1,Ordering::Relaxed);
            return match self.commands.try_send((request,command)) {
                Ok(())=>reply(format_args!("OK imu calibration_queued=true request={request} completion=check_calibration_status")),
                Err(_) => {if starts {self.calibration_reserved.store(false,Ordering::Release);}reply(format_args!("ERR imu_busy"))}
            };
        }
        if matches!(
            command,
            ImuCommand::On | ImuCommand::Off | ImuCommand::SelfTest | ImuCommand::Audit
        ) {
            let request = self.next_request.fetch_add(1, Ordering::Relaxed);
            return match self.commands.try_send((request, command)) {
                Ok(()) => reply(format_args!(
                    "OK imu requested={} completion=check_status gauge_source=unchanged request={}",
                    match command {
                        ImuCommand::On => "on",
                        ImuCommand::Off => "off",
                        ImuCommand::Audit => "audit",
                        _ => "selftest",
                    },
                    request
                )),
                Err(TrySendError::Full(_)) => reply(format_args!("ERR imu_busy")),
                Err(TrySendError::Disconnected(_)) => reply(format_args!("ERR imu_worker_stopped")),
            };
        }
        let snapshot = match self.snapshot.try_lock() {
            Ok(s) => *s,
            Err(TryLockError::WouldBlock) => return reply(format_args!("ERR imu_busy")),
            Err(TryLockError::Poisoned(_)) => return reply(format_args!("ERR imu_worker_failed")),
        };
        snapshot_response(snapshot, command, Instant::now())
    }
    /// Latest typed bench estimate, never I/O or a blocking lock. Contention
    /// returns None so the owner ages its prior sample instead of renewing it.
    pub fn attitude(&self) -> Option<instrument_core::imu_attitude::Input> {
        match self.snapshot.try_lock() {
            Ok(s) => Some(bench_input(*s, Instant::now())),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(_)) => Some(
                instrument_core::imu_attitude::Input::unavailable(instrument_core::Quality::Failed),
            ),
        }
    }
}

fn bench_input(s: Snapshot, now: Instant) -> instrument_core::imu_attitude::Input {
    use instrument_core::{imu_attitude::Input, Quality as Q};
    let quality = match s.quality(now) {
        "valid" => Q::Valid,
        "stale" => Q::Stale,
        "off" | "starting" | "testing" | "missing" => Q::Missing,
        _ => Q::Failed,
    };
    let Some((_, at)) = s.sample else {
        return Input::unavailable(quality);
    };
    let Some(output) = s.estimate else {
        return Input::unavailable(Q::Missing);
    };
    let estimate = output.estimate;
    let usable =
        estimate.frame == Frame::BodyForwardRightDown && estimate.quality != Quality::Unavailable;
    Input {
        received_us: output.received_us,
        age_ms: now.duration_since(at).as_millis().min(u64::MAX as u128) as u64,
        pitch_roll_deg: estimate
            .roll_pitch_deg
            .filter(|_| usable && quality == Q::Valid)
            .map(|[r, p]| [p, r]),
        quality: if quality == Q::Valid && !usable {
            Q::Failed
        } else {
            quality
        },
    }
}

fn snapshot_response(s: Snapshot, cmd: ImuCommand, now: Instant) -> Response {
    let quality = s.quality(now);
    if cmd == ImuCommand::CalibrationStatus {
        let c=s.calibration; let p=s.profile;
        return reply(format_args!("OK imu_calibration job={} phase={:?} reason={:?} full={} pose={}/24 progress={} calibrated={} generation={} temperature_warning={} high_bias={} candidate_high_bias={} candidate_bias_milli_dps={:?} candidate_noise_milli_dps={} candidate_rms_milli_g={} bias_dps={:?} gyro_noise_dps={:?} temperature_c={:.2} rms_g={:.6} max_error_g={:.6} samples={} storage={} coeff_accel_bias_g={:?} coeff_accel_matrix={:?} level_rotation={:?}",
            c.job,c.phase,c.reason,c.full,c.pose,c.progress,p.level,p.generation,c.temperature_warning,p.high_bias(),c.high_bias,c.gyro_bias_milli,c.noise_milli,c.rms_milli,p.gyro_bias_dps,p.gyro_noise_dps,p.temperature_c,p.rms_g,p.max_error_g,p.samples,s.storage_available,p.accel_bias_g,p.accel_matrix,p.level_rotation));
    }
    let age = s.sample.map(|(_, at)| now.duration_since(at).as_millis());
    if cmd == ImuCommand::Estimator {
        let estimate = s
            .estimate
            .filter(|_| quality == "valid")
            .map(|o| o.estimate)
            .unwrap_or_else(|| Estimate::unavailable(Frame::SensorNative));
        let angles = estimate
            .roll_pitch_deg
            .map(|[r, p]| format!("roll_deg={r:.3} pitch_deg={p:.3}"))
            .unwrap_or_else(|| "roll_deg=none pitch_deg=none".into());
        return reply(format_args!("OK imu estimator={} quality={} input_quality={} acceleration_rejected={} {} yaw_deg={:?} attitude_covariance={} bias_covariance={} gyro_bias_rad_s={:?} frame={:?} calibration={} timebase={} acquired_us=unknown received_us={:?} gauge_eligible=false bench_display_eligible={}", s.estimator_name,
            match estimate.quality {Quality::Unavailable=>"unavailable",Quality::BenchOnly=>"bench_only",Quality::Estimated=>"estimated"}, quality, s.calibration.accel_rejected, angles, estimate.yaw_deg,
            if estimate.attitude_error_covariance_rad2.is_some() {"available"} else {"unknown"},
            if estimate.gyro_bias_covariance_rad2_s2.is_some() {"available"} else {"unknown"}, estimate.gyro_bias_rad_s, estimate.frame, if s.profile.full {"full"} else if s.profile.level {"level"} else {"none"}, if s.profile.level && !s.calibration.temperature_warning && s.estimator_name.starts_with("madgwick") {"counter_with_receipt_guard"}else{"receipt"},
            s.estimate.filter(|_|quality=="valid").map(|o|o.received_us),bench_input(s,now).pitch_roll_deg.is_some()));
    }
    if cmd == ImuCommand::Status {
        let (address, who, revision) = s
            .identity
            .map(|i| (i.address, i.who, i.revision))
            .unwrap_or((0, 0, 0));
        return reply(format_args!("OK imu request={} completed={} state={} quality={} detected={} address=0x{:02x} who=0x{:02x} revision=0x{:02x} samples={} skipped={} duplicates={} errors={} estimate_rejections={} raw_tilt_rejections={} max_gap_ms={} age_ms={} error={:?} accel_fs_g=4 gyro_fs_dps=256 odr_hz=112.1 frame=sensor calibration={} gauge_source=unchanged selftest={} selftest_accel_g={:?} selftest_gyro_dps={:?} audit={}",s.request,s.completed,s.state.name(),quality,s.identity.is_some(),address,who,revision,s.samples,s.skipped,s.duplicates,s.errors,s.estimate_rejections,s.raw_tilt_rejections,s.max_gap_ms,age.map(|a|a.to_string()).unwrap_or_else(||"none".into()),s.last_error,if s.profile.full {"full"}else if s.profile.level {"level"}else{"none"},s.self_test.map(|t|if t.passed() {"passed"} else {"failed"}).unwrap_or("not_run"),s.self_test.map(|t|t.accel_g),s.self_test.map(|t|t.gyro_dps),s.audit.map(|a|format!("config_match:{};data_match:{};ctrl:{:02x?};byte_gy:{};burst_gy:{}",a.configuration_matches(),a.data_match,a.configuration,a.bytewise_gyro_y,a.burst_gyro_y)).unwrap_or_else(||"not_run".into())));
    }
    if quality != "valid" {
        return reply(format_args!("ERR imu_{}", quality));
    }
    let (sample, _) = s.sample.expect("valid quality requires a sample");
    let a = sample.accel_g();
    let g = sample.gyro_dps();
    let norm = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    let tilt = tilt_degrees(a);
    let tilt_text = tilt
        .map(|[p, r]| format!("tilt_pitch_deg={p:.3} tilt_roll_deg={r:.3}"))
        .unwrap_or_else(|| "tilt_pitch_deg=none tilt_roll_deg=none".into());
    reply(format_args!("OK imu sample={} counter={} age_ms={} ax_g={:.5} ay_g={:.5} az_g={:.5} gx_dps={:.5} gy_dps={:.5} gz_dps={:.5} temp_c={:.3} norm_g={:.5} raw_ax={} raw_ay={} raw_az={} raw_gx={} raw_gy={} raw_gz={} {} tilt=accel_only frame=sensor calibration=none",s.samples,sample.counter,age.unwrap_or(0),a[0],a[1],a[2],g[0],g[1],g[2],sample.temperature_c(),norm,sample.raw_accel[0],sample.raw_accel[1],sample.raw_accel[2],sample.raw_gyro[0],sample.raw_gyro[1],sample.raw_gyro[2],tilt_text))
}

/// Initial adapter deliberately ignores gyro rates, with no hidden bias removal.
pub struct BenchTilt;
impl AttitudeEstimator for BenchTilt {
    fn name(&self) -> &'static str {
        "accel_tilt"
    }
    fn reset(&mut self, _: ResetReason) {}
    fn update(&mut self, sample: &Observation, _: f32) -> Estimate {
        let mut estimate = Estimate::unavailable(sample.frame);
        if let Some([pitch, roll]) = tilt_degrees(sample.accel_g) {
            estimate.quality = Quality::BenchOnly;
            estimate.roll_pitch_deg = Some([roll, pitch]);
        }
        estimate
    }
}
/// Fifteen-sample causal acceleration average, bounded to less than 200 ms of history.
/// Invalid RAW gravity/geometry resets immediately; averaging never conceals it.
/// Output is a bench estimate at the newest receipt, with ~60-65 ms typical
/// averaging lag. No gyro integration, extrapolation or timestamp renewal.
#[derive(Default)]
pub struct AveragedTilt {
    history: [Option<([f32; 3], u64)>; 15],
    next: usize,
}
impl AttitudeEstimator for AveragedTilt {
    fn name(&self) -> &'static str {
        "accel_mean15"
    }
    fn reset(&mut self, _: ResetReason) {
        self.history = [None; 15];
        self.next = 0;
    }
    fn update(&mut self, sample: &Observation, dt: f32) -> Estimate {
        if dt == 0. {
            self.reset(ResetReason::Startup);
        }
        if tilt_degrees(sample.accel_g).is_none() {
            self.reset(ResetReason::InvalidInput);
            return Estimate::unavailable(sample.frame);
        }
        self.history[self.next] = Some((sample.accel_g, sample.received_us));
        self.next = (self.next + 1) % 15;
        let mut sum = [0.; 3];
        let mut count = 0;
        for slot in &mut self.history {
            if let Some((a, t)) = *slot {
                if sample
                    .received_us
                    .checked_sub(t)
                    .is_some_and(|age| age < 200_000)
                {
                    for axis in 0..3 {
                        sum[axis] += a[axis];
                    }
                    count += 1;
                } else {
                    *slot = None;
                }
            }
        }
        // Require a full fresh window on startup and after invalid raw input.
        if count != 15 {
            return Estimate::unavailable(sample.frame);
        }
        let mut input = *sample;
        input.accel_g = sum.map(|v| v / 15.);
        BenchTilt.update(&input, dt)
    }
}
/// Bench tilt only. A near-1g norm does NOT establish a stationary sensor.
pub fn tilt_degrees(a: [f32; 3]) -> Option<[f32; 2]> {
    if !a.iter().all(|v| v.is_finite()) {
        return None;
    }
    let norm = a.iter().map(|v| v * v).sum::<f32>().sqrt();
    if !(0.8..=1.2).contains(&norm) {
        return None;
    }
    let yz = (a[1] * a[1] + a[2] * a[2]).sqrt();
    if yz < 0.001 {
        return None;
    } // roll is undefined at the pitch pole.
    Some([
        (-a[0]).atan2(yz).to_degrees(),
        a[1].atan2(a[2]).to_degrees(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fifteen_sample_history_tolerates_delivery_jitter_but_not_stale_input() {
        let mut p = Pipeline::new(AveragedTilt::default());
        let mut s = observation();
        s.accel_g = [0., 0., 1.];
        for n in 0..20 {
            s.received_us = 1 + n * 12_000;
            s.sensor_counter = Some(n as u32);
            let out = p.update(s);
            assert_eq!(out.received_us, s.received_us);
            assert_eq!(
                out.estimate.quality,
                if n < 14 {
                    Quality::Unavailable
                } else {
                    Quality::BenchOnly
                }
            );
        }
        s.received_us += 100_000;
        s.sensor_counter = Some(20);
        assert_eq!(p.update(s).estimate.quality, Quality::Unavailable);
        for n in 21..36 {
            s.received_us += 9_000;
            s.sensor_counter = Some(n);
            assert_eq!(
                p.update(s).estimate.quality,
                if n < 35 {
                    Quality::Unavailable
                } else {
                    Quality::BenchOnly
                }
            );
        }
        s.received_us += 9_000;
        s.sensor_counter = Some(36);
        s.quality = InputQuality::Stale;
        assert_eq!(p.update(s).estimate.quality, Quality::Unavailable);
    }
    #[test]
    fn averaging_reduces_jitter_without_angle_wrap_and_invalid_raw_resets() {
        let mut p = Pipeline::new(AveragedTilt::default());
        let mut s = observation();
        for n in 0..20 {
            s.received_us = 1 + n * 10_000;
            s.sensor_counter = Some(n as u32);
            s.accel_g = [0., [0.02, -0.02, 0.][n as usize % 3], 1.];
            let e = p.update(s).estimate;
            if n < 14 {
                assert!(e.roll_pitch_deg.is_none());
            } else {
                assert!(e.roll_pitch_deg.unwrap()[0].abs() < 0.01);
            }
        }
        s.received_us = 200_001;
        s.sensor_counter = Some(20);
        s.accel_g = [0., 2., 0.];
        assert_eq!(p.update(s).estimate.quality, Quality::Unavailable);
        for n in 0..15 {
            s.received_us += 10_000;
            s.sensor_counter = Some(21 + n);
            s.accel_g = [0., 0., 1.];
            let e = p.update(s).estimate;
            if n < 14 {
                assert!(e.roll_pitch_deg.is_none());
            } else {
                assert_eq!(e.roll_pitch_deg, Some([0., 0.]));
            }
        }
        p.reset(ResetReason::SourceChanged);
        for n in 0..20 {
            s.received_us = 400_001 + n * 10_000;
            s.sensor_counter = Some(n as u32);
            s.accel_g = [0., if n % 2 == 0 { 0.01 } else { -0.01 }, -1.];
            let e = p.update(s).estimate;
            if n >= 14 {
                assert!(e.roll_pitch_deg.unwrap()[0].abs() > 179.);
            }
        }
        // Sparse delivery cannot accumulate an unbounded history, even when
        // each individual interval is below the separate 100 ms source limit.
        for n in 20..30 {
            s.received_us += 60_000;
            s.sensor_counter = Some(n);
            s.accel_g = [0., 0., -1.];
            let e = p.update(s).estimate;
            if n >= 23 {
                assert!(e.roll_pitch_deg.is_none());
            }
        }
    }
    #[test]
    fn mounted_bench_output_preserves_receipt_age_and_rejects_native_or_invalid_input() {
        let mount = Mount::new([3, -2, 1]).unwrap();
        for (raw, expected) in [
            ([1., 0., 0.], [0., 0.]),
            ([0.8660254, -0.5, 0.], [0., 30.]),
            ([0.8660254, 0., -0.5], [30., 0.]),
        ] {
            let mut observation = observation();
            observation.frame = Frame::BodyForwardRightDown;
            observation.accel_g = mount.apply(raw);
            let mut pipeline = Pipeline::new(BenchTilt);
            let out = pipeline.update(observation);
            let now = Instant::now();
            let mut s = Snapshot::new();
            s.state = State::Running;
            s.sample = Some((sample(1), now));
            s.estimate = Some(out);
            let input = bench_input(s, now + Duration::from_millis(17));
            let [p, r] = input.pitch_roll_deg.unwrap();
            assert!((p - expected[0]).abs() < 0.001 && (r - expected[1]).abs() < 0.001);
            assert_eq!(input.age_ms, 17);
            assert_eq!(input.received_us, observation.received_us);
            assert_eq!(out.estimate.quality, Quality::BenchOnly);
            assert!(bench_input(s, now + Duration::from_millis(100))
                .pitch_roll_deg
                .is_none());
            for state in [State::Off, State::Testing, State::Starting, State::Failed] {
                s.state = state;
                assert!(bench_input(s, now).pitch_roll_deg.is_none());
            }
            s.state = State::Running;
            s.estimate.as_mut().unwrap().estimate.frame = Frame::SensorNative;
            assert!(bench_input(s, now).pitch_roll_deg.is_none());
        }
    }
    fn observation() -> Observation {
        Observation {
            source_epoch: 1,
            received_us: 1,
            acquired_us: None,
            sensor_counter: Some(1),
            frame: Frame::SensorNative,
            quality: InputQuality::Fresh,
            calibration: Calibration::Unverified,
            accel_g: [0., 0.5, 0.8660254],
            gyro_dps: [-1.6, 30.19, 0.],
            temperature_c: Some(30.),
            accel_covariance_g2: None,
            gyro_covariance_dps2: None,
        }
    }
    #[test]
    fn estimator_preserves_raw_gyro_and_withholds_stale_or_invalid_angles() {
        let input = observation();
        let mut p = Pipeline::new(BenchTilt);
        let output = p.update(input);
        assert_eq!(input.gyro_dps, [-1.6, 30.19, 0.]);
        assert_eq!(output.estimate.quality, Quality::BenchOnly);
        let [r, pitch] = output.estimate.roll_pitch_deg.unwrap();
        assert!((r - 30.).abs() < 0.001 && pitch.abs() < 0.001);
        assert!(output.estimate.gyro_bias_rad_s.is_none() && output.estimate.yaw_deg.is_none());
        let now = Instant::now();
        let mut s = Snapshot::new();
        s.state = State::Running;
        s.sample = Some((sample(1), now));
        s.estimate = Some(output);
        assert!(snapshot_response(s, ImuCommand::Estimator, now)
            .as_str()
            .contains("quality=bench_only"));
        for at in [
            now + Duration::from_millis(100),
            now + Duration::from_secs(1),
        ] {
            let response = snapshot_response(s, ImuCommand::Estimator, at);
            assert!(response.as_str().contains("quality=unavailable"));
            assert!(response.as_str().contains("roll_deg=none pitch_deg=none"));
        }
        for state in [State::Off, State::Testing, State::Failed] {
            s.state = state;
            assert!(snapshot_response(s, ImuCommand::Estimator, now)
                .as_str()
                .contains("roll_deg=none"));
        }
        let mut input = observation();
        input.received_us = 30_001;
        input.sensor_counter = Some(2);
        input.accel_g = [0.; 3];
        assert_eq!(p.update(input).estimate.quality, Quality::Unavailable);
    }
    fn sample(counter: u32) -> Sample {
        Sample {
            counter,
            raw_accel: [0, 0, 8192],
            raw_gyro: [0; 3],
            raw_temperature: 0,
        }
    }
    #[test]
    fn counter_wrap_duplicates_and_staleness() {
        let t = Instant::now();
        let mut s = Snapshot::new();
        s.state = State::Running;
        s.accept(sample(0xffffff), t).unwrap();
        s.accept(sample(0), t + Duration::from_millis(10)).unwrap();
        assert_eq!(s.samples, 2);
        assert_eq!(s.skipped, 0);
        s.accept(sample(0), t + Duration::from_millis(90)).unwrap();
        assert_eq!(s.duplicates, 1);
        assert_eq!(s.quality(t + Duration::from_millis(110)), "stale");
        assert!(
            snapshot_response(s, ImuCommand::Read, t + Duration::from_millis(110))
                .as_str()
                .starts_with("ERR imu_stale")
        );
        assert!(s
            .accept(sample(0xfffffe), t + Duration::from_millis(120))
            .is_err());
    }
    #[test]
    fn tilt_signs_and_invalid_geometry() {
        assert_eq!(tilt_degrees([0., 0., 1.]), Some([0., 0.]));
        let [p, r] = tilt_degrees([-0.5, 0., 0.8660254]).unwrap();
        assert!((p - 30.).abs() < 0.001);
        assert_eq!(r, 0.);
        let [p, r] = tilt_degrees([0., 0.5, 0.8660254]).unwrap();
        assert_eq!(p, 0.);
        assert!((r - 30.).abs() < 0.001);
        for a in [[0.; 3], [2., 0., 0.], [1., 0., 0.], [f32::NAN, 0., 1.]] {
            assert!(tilt_degrees(a).is_none());
        }
    }
    #[test]
    fn failed_and_off_never_expose_old_sample() {
        let t = Instant::now();
        let mut s = Snapshot::new();
        s.sample = Some((sample(1), t));
        assert_eq!(
            snapshot_response(s, ImuCommand::Read, t).as_str(),
            "ERR imu_off\n"
        );
        s.state = State::Failed;
        assert_eq!(
            snapshot_response(s, ImuCommand::Read, t).as_str(),
            "ERR imu_failed\n"
        );
    }
    #[test]
    fn failed_bus_quiesce_rejects_command_before_sensor_io() {
        struct UntouchedBus(Arc<AtomicU32>);
        impl Bus for UntouchedBus {
            fn read(&mut self, _: u8, _: u8, _: &mut [u8]) -> Result<(), Error> {
                self.0.fetch_add(1, Ordering::Relaxed);
                Err(Error::Bus(99))
            }
            fn write(&mut self, _: u8, _: u8, _: u8) -> Result<(), Error> {
                self.0.fetch_add(1, Ordering::Relaxed);
                Err(Error::Bus(99))
            }
            fn delay_us(&mut self, _: u32) {}
        }
        let calls = Arc::new(AtomicU32::new(0));
        let service =
            Service::start_with_bus_poll(UntouchedBus(calls.clone()), BenchTilt, |_, quiesce| {
                !quiesce
            })
            .unwrap();
        assert!(service
            .handle(ImuCommand::SelfTest)
            .as_str()
            .starts_with("OK imu requested=selftest"));
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let response = service.handle(ImuCommand::Status);
            if response.as_str().contains("completed=1") {
                assert!(response.as_str().contains("state=failed"));
                assert!(response.as_str().contains("error=Some(Bus(-1))"));
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn sampling_failure_quiesces_before_command_handshake_cleanup() {
        struct FailingSampleBus {
            regs: [u8; 128],
            failed: bool,
            quiesced: bool,
            cleanup: Arc<AtomicU32>,
        }
        impl Bus for FailingSampleBus {
            fn read(&mut self, _: u8, reg: u8, bytes: &mut [u8]) -> Result<(), Error> {
                if reg == 0x2d && self.regs[8] == 0x83 {
                    self.failed = true;
                    self.quiesced = false;
                    return Err(Error::Bus(77));
                }
                bytes.copy_from_slice(&self.regs[reg as usize..reg as usize + bytes.len()]);
                Ok(())
            }
            fn write(&mut self, _: u8, reg: u8, value: u8) -> Result<(), Error> {
                if reg == 0x0a {
                    if self.failed {
                        assert!(self.quiesced, "long cleanup ran before buzzer shutoff");
                        self.cleanup.fetch_add(1, Ordering::Relaxed);
                    }
                    self.regs[0x2d] = if value == 0 { 0 } else { 0x80 };
                }
                self.regs[reg as usize] = value;
                Ok(())
            }
            fn delay_us(&mut self, _: u32) {}
        }
        let cleanup = Arc::new(AtomicU32::new(0));
        let mut regs = [0; 128];
        regs[0] = 5;
        regs[0x4d] = 0x80;
        let service = Service::start_with_bus_poll(
            FailingSampleBus {
                regs,
                failed: false,
                quiesced: false,
                cleanup: cleanup.clone(),
            },
            BenchTilt,
            |bus, quiesce| {
                if quiesce {
                    bus.quiesced = true;
                }
                true
            },
        )
        .unwrap();
        assert!(service
            .handle(ImuCommand::On)
            .as_str()
            .starts_with("OK imu requested=on"));
        let deadline = Instant::now() + Duration::from_secs(2);
        while cleanup.load(Ordering::Relaxed) == 0 {
            assert!(Instant::now() < deadline, "cleanup did not run");
            thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn blocked_sensor_bus_does_not_block_console_owner() {
        use std::sync::{atomic::Ordering, Condvar};
        struct BlockedBus(Arc<(Mutex<bool>, Condvar)>);
        impl Bus for BlockedBus {
            fn read(&mut self, _: u8, _: u8, _: &mut [u8]) -> Result<(), Error> {
                let (lock, wake) = &*self.0;
                let mut released = lock.lock().unwrap();
                while !*released {
                    released = wake.wait(released).unwrap();
                }
                Err(Error::Bus(1))
            }
            fn write(&mut self, _: u8, _: u8, _: u8) -> Result<(), Error> {
                Ok(())
            }
            fn delay_us(&mut self, _: u32) {}
        }
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let service = Service::start(BlockedBus(gate.clone())).unwrap();
        let metrics = Arc::new(console_runtime::Metrics::default());
        let m = metrics.clone();
        let worker = thread::spawn(move || {
            let mut input = b"imu on\n".to_vec();
            input.extend(b"status\n".repeat(20));
            console_runtime::run_with_imu(
                std::io::Cursor::new(input),
                std::io::sink(),
                m,
                |_| Ok(()),
                |c| service.handle(c),
            )
        });
        thread::sleep(Duration::from_millis(180));
        let ticks = metrics.ticks.load(Ordering::Relaxed);
        *gate.0.lock().unwrap() = true;
        gate.1.notify_all();
        worker.join().unwrap().unwrap();
        assert!(
            ticks >= 3,
            "console owner stopped while sensor I/O was blocked"
        );
    }
}

/// Fuse calibrated rates only. Before calibration, preserve the accepted bench tilt.
#[derive(Default)]
pub struct CalibratedFusion { tilt: AveragedTilt, fusion: attitude_estimator::fusion::Madgwick }
impl AttitudeEstimator for CalibratedFusion {
    fn name(&self)->&'static str { "madgwick6dof_or_tilt" }
    fn acceleration_rejected(&self)->bool {self.fusion.accel_rejected}
    fn reset(&mut self,r:ResetReason){ self.tilt.reset(r); self.fusion.reset(r); }
    fn update(&mut self,s:&Observation,dt:f32)->Estimate {
        if s.calibration==Calibration::Verified { self.fusion.update(s,dt) } else { self.tilt.update(s,dt) }
    }
}

#[cfg(test)]
mod calibration_transaction_tests {
    use super::*;
    struct NoIo;
    impl Bus for NoIo {
        fn read(&mut self,_:u8,_:u8,_:&mut[u8])->Result<(),Error>{panic!("reset/review must not touch the sensor")}
        fn write(&mut self,_:u8,_:u8,_:u8)->Result<(),Error>{panic!("reset/review must not touch the sensor")}
        fn delay_us(&mut self,_:u32){}
    }
    fn command(service:&Service, c:ImuCommand)->Response {
        let until=Instant::now()+Duration::from_secs(1);
        loop {let r=service.handle(c);if !r.as_str().starts_with("ERR imu_busy") || Instant::now()>=until{return r;}thread::sleep(Duration::from_millis(1));}
    }
    fn wait(service:&Service,phase:Phase)->CalibrationView {
        let end=Instant::now()+Duration::from_secs(2);
        loop {if let Some(v)=service.calibration_view(){if v.phase==phase{return v;}}
            assert!(Instant::now()<end,"calibration phase {phase:?} did not complete");thread::sleep(Duration::from_millis(5));}
    }
    #[test]
    fn review_cancel_stale_jobs_and_persistence_failure_never_replace_active_calibration() {
        struct Store {durable:Record,pending:Option<Record>,ack:bool,fail:bool,submits:u32}
        let old=Record {generation:11,level:true,gyro_bias_dps:[-1.,30.,0.],..Record::default()};
        let store=Arc::new(Mutex::new(Store {durable:old,pending:None,ack:false,fail:false,submits:0}));
        let worker=store.clone();
        let service=Service::start_with_calibration(NoIo,CalibratedFusion::default(),Some(Mount::new([3,-2,1]).unwrap()),|_,_|true,Some(old),move |candidate| {
            let mut s=worker.lock().unwrap();
            if let Some(r)=candidate {s.pending=Some(r);s.submits+=1;return StoreStatus::Pending;}
            if let Some(r)=s.pending {if s.fail {s.pending=None;return StoreStatus::Failed(-99);}
                if !s.ack {return StoreStatus::Pending;}s.durable=r;s.pending=None;}
            StoreStatus::Saved(s.durable.generation)
        }).unwrap();
        assert!(command(&service, ImuCommand::CalibrationReset).as_str().starts_with("OK "));
        let review=wait(&service,Phase::Review);assert_eq!(service.snapshot.lock().unwrap().profile,old);
        assert!(command(&service, ImuCommand::CalibrationSave(review.job+1)).as_str().starts_with("ERR "));
        assert!(command(&service, ImuCommand::SelfTest).as_str().contains("calibration_busy"));
        assert_eq!(store.lock().unwrap().submits,0);
        assert!(command(&service, ImuCommand::CalibrationSave(review.job)).as_str().starts_with("OK "));
        wait(&service,Phase::Saving);thread::sleep(Duration::from_millis(30));
        assert_eq!(service.snapshot.lock().unwrap().profile,old);assert_eq!(store.lock().unwrap().submits,1);
        assert!(command(&service, ImuCommand::CalibrationCancel(review.job)).as_str().starts_with("ERR "));
        store.lock().unwrap().ack=true;wait(&service,Phase::Saved);
        assert_eq!(service.snapshot.lock().unwrap().profile.generation,12);assert!(!service.snapshot.lock().unwrap().profile.level);
        assert!(command(&service, ImuCommand::CalibrationReset).as_str().starts_with("OK "));
        let review=wait(&service,Phase::Review);
        command(&service, ImuCommand::CalibrationCancel(review.job));wait(&service,Phase::Cancelled);
        assert_eq!(store.lock().unwrap().submits,1);
        assert!(command(&service, ImuCommand::CalibrationReset).as_str().starts_with("OK "));
        let review=wait(&service,Phase::Review);store.lock().unwrap().fail=true;
        command(&service, ImuCommand::CalibrationSave(review.job));wait(&service,Phase::Failed);
        assert_eq!(service.snapshot.lock().unwrap().profile.generation,12);
        assert_eq!(store.lock().unwrap().durable.generation,12);
    }
    #[test]
    fn missing_storage_refuses_calibration_without_sensor_io() {
        let service=Service::start(NoIo).unwrap();
        assert!(command(&service, ImuCommand::CalibrateLevel).as_str().contains("storage_unavailable"));
        assert!(command(&service, ImuCommand::CalibrationStatus).as_str().starts_with("OK imu_calibration"));
    }
    #[test]
    fn failed_bus_quiesce_reports_the_job_and_allows_local_reset_and_cancel() {
        let service=Service::start_with_calibration(NoIo,CalibratedFusion::default(),None,
            |_,quiesce|!quiesce,None,|_|StoreStatus::Saved(0)).unwrap();
        assert!(command(&service, ImuCommand::CalibrateLevel).as_str().starts_with("OK "));
        let failed=wait(&service,Phase::Failed);
        assert!(failed.job>0);assert_eq!(failed.reason,Reason::Sensor);
        assert!(command(&service, ImuCommand::CalibrationReset).as_str().starts_with("OK "));
        let review=wait(&service,Phase::Review);assert!(review.job>failed.job);
        assert!(command(&service, ImuCommand::CalibrationCancel(review.job)).as_str().starts_with("OK "));
        wait(&service,Phase::Cancelled);
    }
}
