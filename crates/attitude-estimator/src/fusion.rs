//! Six-axis Madgwick gradient-descent quaternion estimator (2010 report,
//! sections 3.1/3.2/3.3). Calibrated vectors only; no absolute yaw claim.
//! Norm/innovation gating prevents treating detected linear acceleration as
//! gravity. A conservative stationary-only residual gyro-bias tracker handles
//! small temperature drift without learning ordinary turns as zero rate.
use crate::calibration::{dot, gravity_rotation, norm, unit, Vec3};
use crate::{AttitudeEstimator, Calibration, Estimate, Frame, Observation, Quality, ResetReason};
use libm::{asinf, atan2f, sqrtf};

#[derive(Clone, Copy, Debug)]
pub struct Madgwick {
    q: Option<[f32; 4]>,
    counter: Option<u32>,
    clock_origin: Option<(u32, u64)>,
    sample_period: f32,
    residual_bias: Vec3,
    still_origin: Option<(u64, Vec3)>,
    pub accel_rejected: bool,
}
impl Default for Madgwick {
    fn default() -> Self {
        Self {
            q: None,
            counter: None,
            clock_origin: None,
            sample_period: 1. / 112.1,
            residual_bias: [0.; 3],
            still_origin: None,
            accel_rejected: false,
        }
    }
}
impl Madgwick {
    fn dt(&mut self, s: &Observation, receipt_dt: f32) -> f32 {
        let Some(count) = s.sensor_counter else {
            return receipt_dt;
        };
        if let Some((old, at)) = self.clock_origin {
            let n = count.wrapping_sub(old) & 0xffffff;
            if s.received_us.saturating_sub(at) >= 1_000_000 && n >= 90 {
                let period = (s.received_us - at) as f32 / 1_000_000. / n as f32;
                if (0.008..=0.010).contains(&period) {
                    self.sample_period = 0.8 * self.sample_period + 0.2 * period;
                }
                self.clock_origin = Some((count, s.received_us));
            }
        } else {
            self.clock_origin = Some((count, s.received_us));
        }
        let dt = self.counter.map_or(0., |old| {
            (count.wrapping_sub(old) & 0xffffff) as f32 * self.sample_period
        });
        self.counter = Some(count);
        dt
    }
}
impl AttitudeEstimator for Madgwick {
    fn acceleration_rejected(&self) -> bool {
        self.accel_rejected
    }
    fn name(&self) -> &'static str {
        "madgwick6dof"
    }
    fn reset(&mut self, _: ResetReason) {
        *self = Self::default();
    }
    fn update(&mut self, s: &Observation, receipt_dt: f32) -> Estimate {
        if s.calibration != Calibration::Verified
            || s.frame != Frame::BodyForwardRightDown
            || !s
                .gyro_dps
                .iter()
                .chain(s.accel_g.iter())
                .all(|v| v.is_finite())
        {
            self.reset(ResetReason::InvalidInput);
            return Estimate::unavailable(s.frame);
        }
        let magnitude = norm(s.accel_g);
        if self.q.is_none() {
            if !(0.92..=1.08).contains(&magnitude) {
                return Estimate::unavailable(s.frame);
            }
            self.q = gravity_rotation(s.accel_g);
        }
        let mut q = self.q.unwrap();
        let dt = self.dt(s, receipt_dt);
        if !(0.0..0.1).contains(&dt) {
            self.reset(ResetReason::TimingGap);
            return Estimate::unavailable(s.frame);
        }
        let [w, x, y, z] = q;
        let predicted = [
            2. * (x * z - w * y),
            2. * (w * x + y * z),
            1. - 2. * (x * x + y * y),
        ];
        let a = unit(s.accel_g).unwrap_or([0.; 3]);
        // cos(15deg) innovation gate, independent 0.08g norm gate.
        self.accel_rejected = !(0.92..=1.08).contains(&magnitude) || dot(predicted, a) < 0.9659258;
        let corrected = core::array::from_fn::<_, 3, _>(|i| s.gyro_dps[i] - self.residual_bias[i]);
        let still =
            !self.accel_rejected && (magnitude - 1.).abs() < 0.015 && norm(corrected) < 0.75;
        if still {
            if let Some((at, reference)) = self.still_origin {
                if norm(core::array::from_fn(|i| reference[i] - a[i])) > 0.002 {
                    self.still_origin = Some((s.received_us, a));
                } else if s.received_us.saturating_sub(at) > 3_000_000 {
                    for (i, v) in corrected.into_iter().enumerate() {
                        self.residual_bias[i] = (self.residual_bias[i]
                            + v * (1. - libm::expf(-0.02 * 2. * core::f32::consts::PI * dt)))
                        .clamp(-1., 1.);
                    }
                }
            } else {
                self.still_origin = Some((s.received_us, a));
            }
        } else {
            self.still_origin = None;
        }
        let [gx, gy, gz] = corrected.map(|v| v * core::f32::consts::PI / 180.);
        let mut derivative = [
            0.5 * (-x * gx - y * gy - z * gz),
            0.5 * (w * gx + y * gz - z * gy),
            0.5 * (w * gy - x * gz + z * gx),
            0.5 * (w * gz + x * gy - y * gx),
        ];
        if !self.accel_rejected {
            let f = [
                predicted[0] - a[0],
                predicted[1] - a[1],
                predicted[2] - a[2],
            ];
            let gradient = [
                -2. * y * f[0] + 2. * x * f[1],
                2. * z * f[0] + 2. * w * f[1] - 4. * x * f[2],
                -2. * w * f[0] + 2. * z * f[1] - 4. * y * f[2],
                2. * x * f[0] + 2. * y * f[1],
            ];
            let n = sqrtf(gradient.iter().map(|v| v * v).sum());
            if n > 0.00001 {
                let trust = (1. - (magnitude - 1.).abs() / 0.08).clamp(0., 1.);
                for i in 0..4 {
                    derivative[i] -= 0.04 * trust * gradient[i] / n;
                }
            }
        }
        for i in 0..4 {
            q[i] += derivative[i] * dt;
        }
        let n = sqrtf(q.iter().map(|v| v * v).sum());
        if !n.is_finite() || n < 0.5 {
            self.reset(ResetReason::InvalidOutput);
            return Estimate::unavailable(s.frame);
        }
        q = q.map(|v| v / n);
        self.q = Some(q);
        let [w, x, y, z] = q;
        let roll = atan2f(2. * (w * x + y * z), 1. - 2. * (x * x + y * y)).to_degrees();
        let pitch = asinf((2. * (w * y - z * x)).clamp(-1., 1.)).to_degrees();
        Estimate {
            quality: Quality::BenchOnly,
            frame: s.frame,
            roll_pitch_deg: Some([roll, pitch]),
            yaw_deg: None,
            quaternion_wxyz: Some(q),
            attitude_error_covariance_rad2: None,
            gyro_bias_rad_s: Some(self.residual_bias.map(|v| v * core::f32::consts::PI / 180.)),
            gyro_bias_covariance_rad2_s2: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InputQuality, Pipeline};
    fn observation(i: u32, a: Vec3, g: Vec3) -> Observation {
        Observation {
            source_epoch: 1,
            received_us: 1 + i as u64 * 8921,
            acquired_us: None,
            sensor_counter: Some(i),
            frame: Frame::BodyForwardRightDown,
            quality: InputQuality::Fresh,
            calibration: Calibration::Verified,
            accel_g: a,
            gyro_dps: g,
            temperature_c: Some(25.),
            accel_covariance_g2: None,
            gyro_covariance_dps2: None,
        }
    }
    #[test]
    fn static_tilt_is_normalized_and_never_claims_absolute_yaw_or_covariance() {
        for degrees in [-170f32, -90., -45., 0., 45., 90., 170.] {
            let t = degrees.to_radians();
            let a = [0., libm::sinf(t), libm::cosf(t)];
            let mut p = Pipeline::new(Madgwick::default());
            for i in 0..600 {
                let e = p.update(observation(i, a, [0.; 3])).estimate;
                assert!((e.roll_pitch_deg.unwrap()[0] - degrees).abs() < 0.05);
                assert!(e.yaw_deg.is_none() && e.attitude_error_covariance_rad2.is_none());
            }
        }
    }
    #[test]
    fn gyro_tracks_fast_rotations_and_rejects_linear_acceleration() {
        let mut p = Pipeline::new(Madgwick::default());
        p.update(observation(0, [0., 0., 1.], [0.; 3]));
        for i in 1..=112 {
            let e = p
                .update(observation(i, [1.5, 0., 1.], [90., 0., 0.]))
                .estimate;
            assert!(e.quaternion_wxyz.is_some());
            if i == 112 {
                assert!((e.roll_pitch_deg.unwrap()[0] - 89.92).abs() < 0.25);
            }
        }
    }
    #[test]
    fn uncalibrated_and_gapped_samples_cannot_integrate_or_revive_old_attitude() {
        let mut p = Pipeline::new(Madgwick::default());
        let mut s = observation(0, [0., 0., 1.], [0.; 3]);
        s.calibration = Calibration::Unverified;
        assert_eq!(p.update(s).estimate.quality, Quality::Unavailable);
        p.update(observation(1, [0., 0., 1.], [0.; 3]));
        let mut s = observation(2, [0., 0., 1.], [0.; 3]);
        s.received_us += 100_000;
        assert_eq!(p.update(s).estimate.quality, Quality::Unavailable);
    }
    #[test]
    fn residual_bias_adapts_at_rest_but_does_not_learn_a_measured_roll_as_zero() {
        let mut p = Pipeline::new(Madgwick::default());
        let mut last = None;
        for i in 0..4000 {
            last = Some(
                p.update(observation(i, [0., 0., 1.], [0.08, 0., 0.]))
                    .estimate,
            );
        }
        assert!(last.unwrap().gyro_bias_rad_s.unwrap()[0].to_degrees() > 0.07);
        let mut p = Pipeline::new(Madgwick::default());
        for i in 0..2000 {
            let angle = (i as f32 / 112.1 * 0.5).to_radians();
            let e = p
                .update(observation(
                    i,
                    [0., libm::sinf(angle), libm::cosf(angle)],
                    [0.5, 0., 0.],
                ))
                .estimate;
            assert!(e.gyro_bias_rad_s.unwrap()[0].abs() < 0.0001);
        }
    }
}
