#![no_std]
#![forbid(unsafe_code)]

/// Axes must be identified before fusion; native sensor axes are NOT aircraft axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame {
    SensorNative,
    BodyForwardRightDown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputQuality {
    Fresh,
    Stale,
    Saturated,
    Fault,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Calibration {
    Unverified,
    Verified,
}

/// Symmetric positive semidefinite 3x3 covariance. Units are named by each field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Covariance3([[f32; 3]; 3]);
impl Covariance3 {
    pub fn new(matrix: [[f32; 3]; 3]) -> Option<Self> {
        if !matrix.iter().flatten().all(|x| x.is_finite()) {
            return None;
        }
        // f64 accommodates products of any three finite f32 entries. Reject
        // negative principal minors conservatively; never hide small invalid
        // covariance behind an absolute epsilon (units may be very small).
        let m = matrix.map(|row| row.map(|v| v as f64));
        for i in 0..3 {
            if m[i][i] < 0.0 {
                return None;
            }
            for j in 0..3 {
                if matrix[i][j] != matrix[j][i] {
                    return None;
                }
                if m[i][i] * m[j][j] - m[i][j] * m[j][i] < 0.0 {
                    return None;
                }
            }
        }
        let determinant = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
        if determinant < 0.0 {
            None
        } else {
            Some(Self(matrix))
        }
    }
    pub fn matrix(self) -> [[f32; 3]; 3] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Observation {
    /// New epoch on hardware reinitialization or calibration/frame changes.
    pub source_epoch: u32,
    pub received_us: u64,
    /// Same monotonic time origin as received_us. None until acquisition time
    /// can be established; do not substitute receipt time.
    pub acquired_us: Option<u64>,
    pub sensor_counter: Option<u32>,
    pub frame: Frame,
    pub quality: InputQuality,
    pub calibration: Calibration,
    pub accel_g: [f32; 3],
    pub gyro_dps: [f32; 3],
    pub temperature_c: Option<f32>,
    pub accel_covariance_g2: Option<Covariance3>,
    pub gyro_covariance_dps2: Option<Covariance3>,
}
impl Observation {
    pub fn gyro_rad_s(self) -> [f32; 3] {
        self.gyro_dps.map(|v| v * (core::f32::consts::PI / 180.))
    }
    pub fn accel_m_s2(self) -> [f32; 3] {
        self.accel_g.map(|v| v * 9.80665)
    }
    fn finite(self) -> bool {
        self.accel_g
            .iter()
            .chain(self.gyro_dps.iter())
            .all(|v| v.is_finite())
            && self.temperature_c.is_none_or(|v| v.is_finite())
            && self.accel_m_s2().iter().all(|v| v.is_finite())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetReason {
    Startup,
    SourceChanged,
    FrameChanged,
    CalibrationChanged,
    TimingGap,
    InvalidInput,
    InvalidOutput,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    Unavailable,
    BenchOnly,
    Estimated,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Estimate {
    pub quality: Quality,
    pub frame: Frame,
    pub roll_pitch_deg: Option<[f32; 2]>,
    /// An unaided six-axis estimator must not claim absolute heading.
    pub yaw_deg: Option<f32>,
    /// Optional rotation from `frame` into the estimator's documented reference.
    /// Quaternion order is w,x,y,z. Reference conventions belong to the algorithm.
    pub quaternion_wxyz: Option<[f32; 4]>,
    /// Small-angle error covariance in radians squared, not Euler-degree variance.
    pub attitude_error_covariance_rad2: Option<Covariance3>,
    pub gyro_bias_rad_s: Option<[f32; 3]>,
    pub gyro_bias_covariance_rad2_s2: Option<Covariance3>,
}
impl Estimate {
    pub fn unavailable(frame: Frame) -> Self {
        Self {
            quality: Quality::Unavailable,
            frame,
            roll_pitch_deg: None,
            yaw_deg: None,
            quaternion_wxyz: None,
            attitude_error_covariance_rad2: None,
            gyro_bias_rad_s: None,
            gyro_bias_covariance_rad2_s2: None,
        }
    }
    fn valid(self) -> bool {
        if self.roll_pitch_deg.is_some_and(|v| {
            !v.iter().all(|n| n.is_finite()) || v[0].abs() > 180. || v[1].abs() > 90.
        }) {
            return false;
        }
        if self
            .yaw_deg
            .is_some_and(|v| !v.is_finite() || !(-180.0..=180.0).contains(&v))
        {
            return false;
        }
        if self
            .gyro_bias_rad_s
            .is_some_and(|v| !v.iter().all(|n| n.is_finite()))
        {
            return false;
        }
        if let Some(q) = self.quaternion_wxyz {
            let norm = q.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>();
            if !norm.is_finite() || (norm - 1.).abs() > 0.001 {
                return false;
            }
        }
        self.quality == Quality::Unavailable
            || self.roll_pitch_deg.is_some()
            || self.quaternion_wxyz.is_some()
    }
}

/// Synchronous bounded math only: no hardware, allocation, I/O, or scheduling.
/// A later Kalman implementation owns its state, P/Q/R, bias model and gating.
pub trait AttitudeEstimator {
    fn name(&self) -> &'static str;
    fn reset(&mut self, reason: ResetReason);
    /// dt is measured receipt interval; it is NOT a claimed acquisition interval.
    /// Zero marks the first observation after reset; implementations must initialize.
    fn update(&mut self, sample: &Observation, dt_received_s: f32) -> Estimate;
}

pub const MAX_GAP_US: u64 = 100_000;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Output {
    pub received_us: u64,
    pub estimate: Estimate,
}
pub struct Pipeline<E> {
    estimator: E,
    previous: Option<Observation>,
}
impl<E: AttitudeEstimator> Pipeline<E> {
    pub fn new(mut estimator: E) -> Self {
        estimator.reset(ResetReason::Startup);
        Self {
            estimator,
            previous: None,
        }
    }
    pub fn name(&self) -> &'static str {
        self.estimator.name()
    }
    pub fn reset(&mut self, reason: ResetReason) {
        self.previous = None;
        self.estimator.reset(reason);
    }
    pub fn update(&mut self, sample: Observation) -> Output {
        let unavailable = Output {
            received_us: sample.received_us,
            estimate: Estimate::unavailable(sample.frame),
        };
        if sample.quality != InputQuality::Fresh
            || !sample.finite()
            || sample
                .acquired_us
                .is_some_and(|t| t > sample.received_us || sample.received_us - t >= MAX_GAP_US)
        {
            self.reset(ResetReason::InvalidInput);
            return unavailable;
        }
        let mut dt = 0.;
        if let Some(last) = self.previous {
            let reason = if last.source_epoch != sample.source_epoch {
                Some(ResetReason::SourceChanged)
            } else if last.frame != sample.frame {
                Some(ResetReason::FrameChanged)
            } else if last.calibration != sample.calibration {
                Some(ResetReason::CalibrationChanged)
            } else {
                if let (Some(current), Some(previous)) = (sample.acquired_us, last.acquired_us) {
                    if current <= previous {
                        self.reset(ResetReason::TimingGap);
                        return unavailable;
                    }
                }
                None
            };
            if let Some(reason) = reason {
                self.reset(reason);
            } else {
                let gap = sample.received_us.checked_sub(last.received_us);
                if gap.is_none_or(|g| g == 0 || g >= MAX_GAP_US) {
                    self.reset(ResetReason::TimingGap);
                    return unavailable;
                }
                if sample.sensor_counter.is_some() && sample.sensor_counter == last.sensor_counter {
                    self.reset(ResetReason::InvalidInput);
                    return unavailable;
                }
                dt = gap.unwrap_or(0) as f32 / 1_000_000.;
            }
        }
        let mut estimate = self.estimator.update(&sample, dt);
        if estimate.frame != sample.frame || !estimate.valid() {
            self.reset(ResetReason::InvalidOutput);
            return unavailable;
        }
        if estimate.quality == Quality::Unavailable {
            estimate = Estimate::unavailable(sample.frame);
        } else if sample.calibration != Calibration::Verified
            || sample.frame != Frame::BodyForwardRightDown
        {
            estimate.quality = Quality::BenchOnly;
        }
        self.previous = Some(sample);
        Output {
            received_us: sample.received_us,
            estimate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Dummy {
        dt: f32,
        reset: ResetReason,
        bad: bool,
    }
    impl AttitudeEstimator for Dummy {
        fn name(&self) -> &'static str {
            "dummy"
        }
        fn reset(&mut self, r: ResetReason) {
            self.reset = r;
        }
        fn update(&mut self, s: &Observation, dt: f32) -> Estimate {
            self.dt = dt;
            let mut e = Estimate::unavailable(s.frame);
            e.quality = Quality::Estimated;
            e.roll_pitch_deg = Some([if self.bad { f32::NAN } else { 1. }, 2.]);
            e
        }
    }
    fn sample(t: u64) -> Observation {
        Observation {
            source_epoch: 1,
            received_us: t,
            acquired_us: None,
            sensor_counter: None,
            frame: Frame::SensorNative,
            quality: InputQuality::Fresh,
            calibration: Calibration::Unverified,
            accel_g: [0., 0., 1.],
            gyro_dps: [0.; 3],
            temperature_c: None,
            accel_covariance_g2: None,
            gyro_covariance_dps2: None,
        }
    }
    fn pipeline() -> Pipeline<Dummy> {
        Pipeline::new(Dummy {
            dt: 0.,
            reset: ResetReason::Startup,
            bad: false,
        })
    }
    #[test]
    fn actual_intervals_and_unknown_uncertainty_are_preserved() {
        let mut p = pipeline();
        assert_eq!(p.update(sample(20)).estimate.quality, Quality::BenchOnly);
        let out = p.update(sample(30_020));
        assert_eq!(p.estimator.dt, 0.03);
        assert!(out.estimate.attitude_error_covariance_rad2.is_none());
        assert!(out.estimate.yaw_deg.is_none());
    }
    #[test]
    fn gaps_clock_regression_and_duplicates_invalidate() {
        for t in [0, 10, 100_010] {
            let mut p = pipeline();
            p.update(sample(10));
            assert_eq!(p.update(sample(t)).estimate.quality, Quality::Unavailable);
        }
        let mut p = pipeline();
        let mut s = sample(10);
        s.sensor_counter = Some(9);
        p.update(s);
        s.received_us = 20;
        assert_eq!(p.update(s).estimate.quality, Quality::Unavailable);
    }
    #[test]
    fn source_frame_and_calibration_changes_reset_filter_memory() {
        let mut p = pipeline();
        p.update(sample(10));
        let mut s = sample(20);
        s.source_epoch = 2;
        p.update(s);
        assert_eq!(p.estimator.reset, ResetReason::SourceChanged);
        assert_eq!(p.estimator.dt, 0.);
        s.received_us = 30;
        s.frame = Frame::BodyForwardRightDown;
        p.update(s);
        assert_eq!(p.estimator.reset, ResetReason::FrameChanged);
        s.received_us = 40;
        s.calibration = Calibration::Verified;
        assert_eq!(p.update(s).estimate.quality, Quality::Estimated);
        assert_eq!(p.estimator.reset, ResetReason::CalibrationChanged);
    }
    #[test]
    fn invalid_input_and_filter_output_are_not_published() {
        for quality in [
            InputQuality::Stale,
            InputQuality::Fault,
            InputQuality::Saturated,
        ] {
            let mut p = pipeline();
            let mut s = sample(0);
            s.quality = quality;
            assert_eq!(p.update(s).estimate.quality, Quality::Unavailable);
        }
        let mut p = pipeline();
        p.estimator.bad = true;
        assert_eq!(p.update(sample(0)).estimate.quality, Quality::Unavailable);
        assert_eq!(p.estimator.reset, ResetReason::InvalidOutput);
        let mut s = sample(1);
        s.accel_g[0] = f32::NAN;
        assert_eq!(p.update(s).estimate.quality, Quality::Unavailable);
    }
    #[test]
    fn covariance_requires_finite_symmetric_psd_matrix() {
        assert!(Covariance3::new([[0., 1e-20, 0.], [1e-20, 0., 0.], [0., 0., 0.]]).is_none());
        assert!(Covariance3::new([[1., 0., 0.], [0., 2., 0.], [0., 0., 0.]]).is_some());
        assert!(Covariance3::new([[1., 2., 0.], [2., 1., 0.], [0., 0., 1.]]).is_none());
        assert!(Covariance3::new([[1., 1., 0.], [0., 1., 0.], [0., 0., 1.]]).is_none());
        assert!(Covariance3::new([[f32::NAN, 0., 0.], [0., 1., 0.], [0., 0., 1.]]).is_none());
    }
    #[test]
    fn acquisition_time_must_be_fresh_and_monotonic_when_known() {
        for acquired in [0, 200_001] {
            let mut p = pipeline();
            let mut s = sample(200_000);
            s.acquired_us = Some(acquired);
            assert_eq!(p.update(s).estimate.quality, Quality::Unavailable);
        }
        let mut p = pipeline();
        let mut s = sample(20);
        s.acquired_us = Some(10);
        p.update(s);
        s.received_us = 30;
        assert_eq!(p.update(s).estimate.quality, Quality::Unavailable);
    }
}
