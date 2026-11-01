#![forbid(unsafe_code)]

use attitude_estimator::{
    AttitudeEstimator, Calibration, Estimate, Frame, InputQuality, Observation, Output, Pipeline,
    Quality, ResetReason,
};
use instrument_core::ImuCommand;
use qmi8658::{Bus, Driver, Error, Identity, Sample, SelfTest};
use std::{
    io,
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc::{self, TryRecvError, TrySendError},
        Arc, Mutex, TryLockError,
    },
    thread,
    time::{Duration, Instant},
};
use text_cli::{reply, Response};

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
        let (commands, receiver) = mpsc::sync_channel(2);
        let snapshot = Arc::new(Mutex::new(Snapshot::new()));
        let shared = snapshot.clone();
        thread::Builder::new()
            .name("imu".into())
            .stack_size(8192)
            .spawn(move || {
                let mut driver = Driver::new(bus);
                let mut state = Snapshot::new();
                let mut pipeline = Pipeline::new(estimator);
                let clock = Instant::now();
                let mut epoch = 0u32;
                state.estimator_name = pipeline.name();
                loop {
                    match receiver.try_recv() {
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
                            let _ = driver.disable();
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
                                    state.estimate = Some(pipeline.update(Observation {
                                        source_epoch: epoch,
                                        received_us: now.duration_since(clock).as_micros() as u64,
                                        acquired_us: None,
                                        sensor_counter: Some(sample.counter),
                                        frame: Frame::SensorNative,
                                        quality: if sample.saturated() {
                                            InputQuality::Saturated
                                        } else {
                                            InputQuality::Fresh
                                        },
                                        calibration: Calibration::Unverified,
                                        accel_g: sample.accel_g(),
                                        gyro_dps: sample.gyro_dps(),
                                        temperature_c: Some(sample.temperature_c()),
                                        accel_covariance_g2: None,
                                        gyro_covariance_dps2: None,
                                    }));
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
                            let _ = driver.disable();
                        }
                    }
                    if let Ok(mut s) = shared.lock() {
                        *s = state;
                    } else {
                        return;
                    }
                    // ESP-IDF usleep busy-waits below its 10 ms RTOS tick. Use a
                    // full yielding interval so the idle/watchdog and owner run.
                    thread::sleep(Duration::from_millis(20));
                }
            })?;
        Ok(Self {
            commands,
            snapshot,
            next_request: AtomicU32::new(1),
        })
    }
    pub fn handle(&self, command: ImuCommand) -> Response {
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
}

fn snapshot_response(s: Snapshot, cmd: ImuCommand, now: Instant) -> Response {
    let quality = s.quality(now);
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
        return reply(format_args!("OK imu estimator={} quality={} input_quality={} {} yaw_deg={:?} attitude_covariance={} bias_covariance={} gyro_bias_rad_s={:?} frame={:?} calibration=unverified timebase=receipt acquired_us=unknown received_us={:?} gauge_eligible=false", s.estimator_name,
            match estimate.quality {Quality::Unavailable=>"unavailable",Quality::BenchOnly=>"bench_only",Quality::Estimated=>"estimated"}, quality, angles, estimate.yaw_deg,
            if estimate.attitude_error_covariance_rad2.is_some() {"available"} else {"unknown"},
            if estimate.gyro_bias_covariance_rad2_s2.is_some() {"available"} else {"unknown"}, estimate.gyro_bias_rad_s, estimate.frame,
            s.estimate.filter(|_|quality=="valid").map(|o|o.received_us)));
    }
    if cmd == ImuCommand::Status {
        let (address, who, revision) = s
            .identity
            .map(|i| (i.address, i.who, i.revision))
            .unwrap_or((0, 0, 0));
        return reply(format_args!("OK imu request={} completed={} state={} quality={} detected={} address=0x{:02x} who=0x{:02x} revision=0x{:02x} samples={} skipped={} duplicates={} errors={} max_gap_ms={} age_ms={} error={:?} accel_fs_g=4 gyro_fs_dps=256 odr_hz=112.1 frame=sensor calibration=none gauge_source=unchanged selftest={} selftest_accel_g={:?} selftest_gyro_dps={:?} audit={}",s.request,s.completed,s.state.name(),quality,s.identity.is_some(),address,who,revision,s.samples,s.skipped,s.duplicates,s.errors,s.max_gap_ms,age.map(|a|a.to_string()).unwrap_or_else(||"none".into()),s.last_error,s.self_test.map(|t|if t.passed() {"passed"} else {"failed"}).unwrap_or("not_run"),s.self_test.map(|t|t.accel_g),s.self_test.map(|t|t.gyro_dps),s.audit.map(|a|format!("config_match:{};data_match:{};ctrl:{:02x?};byte_gy:{};burst_gy:{}",a.configuration_matches(),a.data_match,a.configuration,a.bytewise_gyro_y,a.burst_gyro_y)).unwrap_or_else(||"not_run".into())));
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
