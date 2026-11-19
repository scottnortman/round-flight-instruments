//! Board-local calibration, robust multi-position gravity ellipsoid fit, and
//! bounded stationary acquisition. No I/O, allocation, or invented references.
use libm::{acosf, sqrt, sqrtf};

pub const RECORD_SIZE: usize = 144;
pub const POSES: usize = 24;
pub const LEVEL_SAMPLES: u32 = 480;
pub const POSE_SAMPLES: u32 = 180;
pub const MOUNT_ID: u8 = 1; // SKU29086 upright: [Z,-Y,X].
pub type Vec3 = [f32; 3];
pub type Mat3 = [[f32; 3]; 3];
pub const IDENTITY: Mat3 = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
pub fn norm(a: Vec3) -> f32 {
    sqrtf(dot(a, a))
}
pub fn unit(a: Vec3) -> Option<Vec3> {
    let n = norm(a);
    (n.is_finite() && n > 0.0001).then(|| a.map(|v| v / n))
}
pub fn mat(m: Mat3, v: Vec3) -> Vec3 {
    m.map(|row| dot(row, v))
}
pub fn rotate(q: [f32; 4], v: Vec3) -> Vec3 {
    let [w, x, y, z] = q;
    let t = [
        2. * (y * v[2] - z * v[1]),
        2. * (z * v[0] - x * v[2]),
        2. * (x * v[1] - y * v[0]),
    ];
    [
        v[0] + w * t[0] + y * t[2] - z * t[1],
        v[1] + w * t[1] + z * t[0] - x * t[2],
        v[2] + w * t[2] + x * t[1] - y * t[0],
    ]
}
pub fn gravity_rotation(a: Vec3) -> Option<[f32; 4]> {
    let a = unit(a)?;
    if a[2] < -0.99999 {
        return Some([0., 1., 0., 0.]);
    }
    let q = [1. + a[2], a[1], -a[0], 0.];
    let n = sqrtf(q.iter().map(|v| v * v).sum());
    Some(q.map(|v| v / n))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record {
    pub generation: u32,
    pub level: bool,
    pub full: bool,
    pub level_rotation: [f32; 4],
    pub accel_bias_g: Vec3,
    pub accel_matrix: Mat3,
    pub gyro_bias_dps: Vec3,
    pub gyro_noise_dps: Vec3,
    pub accel_noise_g: Vec3,
    pub temperature_c: f32,
    pub min_temperature_c: f32,
    pub max_temperature_c: f32,
    pub rms_g: f32,
    pub max_error_g: f32,
    pub samples: u32,
}
impl Default for Record {
    fn default() -> Self {
        Self {
            generation: 0,
            level: false,
            full: false,
            level_rotation: [1., 0., 0., 0.],
            accel_bias_g: [0.; 3],
            accel_matrix: IDENTITY,
            gyro_bias_dps: [0.; 3],
            gyro_noise_dps: [0.; 3],
            accel_noise_g: [0.; 3],
            temperature_c: 25.,
            min_temperature_c: 25.,
            max_temperature_c: 25.,
            rms_g: 0.,
            max_error_g: 0.,
            samples: 0,
        }
    }
}
impl Record {
    pub fn high_bias(self) -> bool {
        self.gyro_bias_dps.iter().any(|v| v.abs() > 10.)
    }
    pub fn temperature_ok(self, t: f32) -> bool {
        t.is_finite() && t >= self.min_temperature_c - 3. && t <= self.max_temperature_c + 3.
    }
    pub fn corrected_accel(self, a: Vec3) -> Vec3 {
        mat(
            self.accel_matrix,
            core::array::from_fn(|i| a[i] - self.accel_bias_g[i]),
        )
    }
    pub fn apply(self, a: Vec3, g: Vec3) -> (Vec3, Vec3) {
        (
            rotate(self.level_rotation, self.corrected_accel(a)),
            rotate(
                self.level_rotation,
                core::array::from_fn(|i| g[i] - self.gyro_bias_dps[i]),
            ),
        )
    }
    pub fn valid(self) -> bool {
        let finite = self
            .level_rotation
            .iter()
            .chain(self.accel_bias_g.iter())
            .chain(self.accel_matrix.iter().flatten())
            .chain(self.gyro_bias_dps.iter())
            .chain(self.gyro_noise_dps.iter())
            .chain(self.accel_noise_g.iter())
            .all(|v| v.is_finite());
        finite
            && (!self.full || self.level)
            && (self.level_rotation.iter().map(|v| v * v).sum::<f32>() - 1.).abs() < 0.0001
            && self.accel_bias_g.iter().all(|v| v.abs() <= 0.2)
            && self.accel_matrix.iter().enumerate().all(|(i, r)| {
                r.iter().enumerate().all(|(j, v)| {
                    if i == j {
                        (0.8..=1.2).contains(v)
                    } else {
                        v.abs() <= 0.15
                    }
                })
            })
            && self.gyro_bias_dps.iter().all(|v| v.abs() <= 64.)
            && self.gyro_noise_dps.iter().all(|v| (0.0..=0.25).contains(v))
            && self.accel_noise_g.iter().all(|v| (0.0..=0.005).contains(v))
            && self.temperature_c.is_finite()
            && self.min_temperature_c.is_finite()
            && self.max_temperature_c.is_finite()
            && (-20.0..=85.).contains(&self.min_temperature_c)
            && self.min_temperature_c <= self.temperature_c
            && self.temperature_c <= self.max_temperature_c
            && self.max_temperature_c <= 85.
            && self.max_temperature_c - self.min_temperature_c <= 3.
            && (0.0..=0.012).contains(&self.rms_g)
            && (0.0..=0.035).contains(&self.max_error_g)
    }
    pub fn encode(self) -> [u8; RECORD_SIZE] {
        let mut b = [0; RECORD_SIZE];
        b[..4].copy_from_slice(b"RFIC");
        b[4] = 1;
        b[5] = MOUNT_ID;
        b[6] = u8::from(self.level) | (u8::from(self.full) << 1);
        b[8..12].copy_from_slice(&self.generation.to_le_bytes());
        let mut at = 12;
        for v in self
            .level_rotation
            .into_iter()
            .chain(self.accel_bias_g)
            .chain(self.accel_matrix.into_iter().flatten())
            .chain(self.gyro_bias_dps)
            .chain(self.gyro_noise_dps)
            .chain(self.accel_noise_g)
            .chain([
                self.temperature_c,
                self.min_temperature_c,
                self.max_temperature_c,
                self.rms_g,
                self.max_error_g,
            ])
        {
            b[at..at + 4].copy_from_slice(&v.to_le_bytes());
            at += 4;
        }
        b[132..136].copy_from_slice(&self.samples.to_le_bytes());
        let c = crc(&b[..140]);
        b[140..].copy_from_slice(&c.to_le_bytes());
        b
    }
    pub fn decode(b: &[u8]) -> Result<Self, RecordError> {
        if b.len() != RECORD_SIZE || &b[..4] != b"RFIC" || crc(&b[..140]).to_le_bytes() != b[140..]
        {
            return Err(RecordError::Corrupt);
        }
        if b[4] != 1 || b[5] != MOUNT_ID {
            return Err(RecordError::Unsupported);
        }
        if b[6] & !3 != 0 || b[7] != 0 || b[136..140] != [0; 4] {
            return Err(RecordError::Corrupt);
        }
        let mut f = [0.; 30];
        for (i, v) in f.iter_mut().enumerate() {
            *v = f32::from_le_bytes(b[12 + i * 4..16 + i * 4].try_into().unwrap());
        }
        let r = Self {
            generation: u32::from_le_bytes(b[8..12].try_into().unwrap()),
            level: b[6] & 1 != 0,
            full: b[6] & 2 != 0,
            level_rotation: f[..4].try_into().unwrap(),
            accel_bias_g: f[4..7].try_into().unwrap(),
            accel_matrix: [
                f[7..10].try_into().unwrap(),
                f[10..13].try_into().unwrap(),
                f[13..16].try_into().unwrap(),
            ],
            gyro_bias_dps: f[16..19].try_into().unwrap(),
            gyro_noise_dps: f[19..22].try_into().unwrap(),
            accel_noise_g: f[22..25].try_into().unwrap(),
            temperature_c: f[25],
            min_temperature_c: f[26],
            max_temperature_c: f[27],
            rms_g: f[28],
            max_error_g: f[29],
            samples: u32::from_le_bytes(b[132..136].try_into().unwrap()),
        };
        if r.valid() {
            Ok(r)
        } else {
            Err(RecordError::Corrupt)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordError {
    Corrupt,
    Unsupported,
}
fn crc(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in bytes {
        c ^= b as u32;
        for _ in 0..8 {
            c = (c >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(c & 1)));
        }
    }
    !c
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Moments {
    pub n: u32,
    pub mean: [f64; 7],
    m2: [f64; 7],
}
impl Moments {
    pub fn push(&mut self, a: Vec3, g: Vec3, t: f32) {
        self.n += 1;
        for (i, v) in a.into_iter().chain(g).chain([t]).enumerate() {
            let d = v as f64 - self.mean[i];
            self.mean[i] += d / self.n as f64;
            self.m2[i] += d * (v as f64 - self.mean[i]);
        }
    }
    pub fn sd(self) -> [f32; 7] {
        self.m2
            .map(|v| sqrt((v / (self.n.max(2) - 1) as f64).max(0.)) as f32)
    }
    pub fn accel(self) -> Vec3 {
        core::array::from_fn(|i| self.mean[i] as f32)
    }
    pub fn gyro(self) -> Vec3 {
        core::array::from_fn(|i| self.mean[i + 3] as f32)
    }
    pub fn stationary(self) -> bool {
        let s = self.sd();
        s[..3].iter().all(|v| *v <= 0.005) && s[3..6].iter().all(|v| *v <= 0.25)
    }
}

/// Independent orientations: 18 fitting poses, then six withheld validation poses.
/// Direction is only a coverage guide; the ellipsoid fit does NOT assume users
/// placed the board at exact angles.
pub fn target(pose: usize) -> Vec3 {
    let v = match pose {
        0 => [1., 0., 0.],
        1 => [-1., 0., 0.],
        2 => [0., 1., 0.],
        3 => [0., -1., 0.],
        4 => [0., 0., 1.],
        5 => [0., 0., -1.],
        6 => [1., 1., 0.],
        7 => [1., -1., 0.],
        8 => [-1., 1., 0.],
        9 => [-1., -1., 0.],
        10 => [1., 0., 1.],
        11 => [1., 0., -1.],
        12 => [-1., 0., 1.],
        13 => [-1., 0., -1.],
        14 => [0., 1., 1.],
        15 => [0., 1., -1.],
        16 => [0., -1., 1.],
        17 => [0., -1., -1.],
        18 => [1., 1., 1.],
        19 => [1., -1., 1.],
        20 => [-1., 1., 1.],
        21 => [-1., -1., 1.],
        22 => [1., 1., -1.],
        _ => [-1., -1., -1.],
    };
    unit(v).unwrap()
}
fn features(v: Vec3) -> [f64; 9] {
    let [x, y, z] = v.map(|v| v as f64);
    [
        x * x,
        y * y,
        z * z,
        2. * x * y,
        2. * x * z,
        2. * y * z,
        x,
        y,
        z,
    ]
}
fn solve<const N: usize>(mut a: [[f64; N]; N], mut b: [f64; N]) -> Option<[f64; N]> {
    for i in 0..N {
        let pivot = (i..N).max_by(|&j, &k| a[j][i].abs().total_cmp(&a[k][i].abs()))?;
        a.swap(i, pivot);
        b.swap(i, pivot);
        if !a[i][i].is_finite() || a[i][i].abs() < 1e-7 {
            return None;
        }
        let d = a[i][i];
        for k in i..N {
            a[i][k] /= d;
        }
        b[i] /= d;
        for j in 0..N {
            if j != i {
                let d = a[j][i];
                for k in i..N {
                    a[j][k] -= d * a[i][k];
                }
                b[j] -= d * b[i];
            }
        }
    }
    b.iter().all(|v| v.is_finite()).then_some(b)
}
/// Huber IRLS quadratic ellipsoid fit, symmetric positive-definite correction,
/// bounds and held-out RMS/max residuals. Rotation is resolved by level capture.
pub fn fit(points: [Vec3; POSES]) -> Option<(Vec3, Mat3, f32, f32)> {
    if !points.iter().flatten().all(|v| v.is_finite()) {
        return None;
    }
    let mut weights = [1f64; 18];
    let mut theta = [0.; 9];
    for _ in 0..4 {
        let mut a = [[0.; 9]; 9];
        let mut b = [0.; 9];
        for (v, w) in points[..18].iter().zip(weights) {
            let f = features(*v);
            for i in 0..9 {
                b[i] += w * f[i];
                for j in 0..9 {
                    a[i][j] += w * f[i] * f[j];
                }
            }
        }
        theta = solve(a, b)?;
        for (i, v) in points[..18].iter().enumerate() {
            let r = (features(*v)
                .into_iter()
                .zip(theta)
                .map(|(a, b)| a * b)
                .sum::<f64>()
                - 1.)
                .abs();
            weights[i] = (0.004 / r.max(0.004)).min(1.);
        }
    }
    let a = [
        [theta[0], theta[3], theta[4]],
        [theta[3], theta[1], theta[5]],
        [theta[4], theta[5], theta[2]],
    ];
    let b = solve(a, [-theta[6] * 0.5, -theta[7] * 0.5, -theta[8] * 0.5])?;
    let k = 1.
        + (0..3)
            .map(|i| b[i] * (0..3).map(|j| a[i][j] * b[j]).sum::<f64>())
            .sum::<f64>();
    if !k.is_finite() || k <= 0. {
        return None;
    }
    let mut l = [[0.; 3]; 3];
    for i in 0..3 {
        for j in 0..=i {
            let s = a[i][j] / k - (0..j).map(|n| l[i][n] * l[j][n]).sum::<f64>();
            l[i][j] = if i == j {
                if s <= 0. {
                    return None;
                }
                sqrt(s)
            } else {
                s / l[j][j]
            };
        }
    }
    // Cholesky above establishes positive definiteness. Use the symmetric
    // square root (Newton-Schulz) rather than imposing a triangular frame
    // rotation that gravity-only observations cannot identify.
    let trace = (0..3).map(|i| a[i][i] / k).sum::<f64>();
    let mut y = core::array::from_fn::<_, 3, _>(|i| {
        core::array::from_fn::<_, 3, _>(|j| a[i][j] / k / trace)
    });
    let mut z = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    let product = |a: [[f64; 3]; 3], b: [[f64; 3]; 3]| {
        core::array::from_fn::<_, 3, _>(|i| {
            core::array::from_fn::<_, 3, _>(|j| (0..3).map(|n| a[i][n] * b[n][j]).sum::<f64>())
        })
    };
    for _ in 0..12 {
        let zy = product(z, y);
        let t = core::array::from_fn(|i| {
            core::array::from_fn(|j| 0.5 * (if i == j { 3. } else { 0. } - zy[i][j]))
        });
        y = product(y, t);
        z = product(t, z);
    }
    let correction = core::array::from_fn(|i| {
        core::array::from_fn(|j| ((y[i][j] + y[j][i]) * 0.5 * sqrt(trace)) as f32)
    });
    let bias = b.map(|v| v as f32);
    let mut max = 0f32;
    let mut square = 0f32;
    for (i, p) in points.into_iter().enumerate() {
        let e = (norm(mat(correction, core::array::from_fn(|i| p[i] - bias[i]))) - 1.).abs();
        max = max.max(e);
        if i >= 18 {
            square += e * e;
        }
    }
    let rms = sqrtf(square / 6.);
    let candidate = Record {
        level: true,
        full: true,
        accel_bias_g: bias,
        accel_matrix: correction,
        rms_g: rms,
        max_error_g: max,
        ..Record::default()
    };
    candidate.valid().then_some((bias, correction, rms, max))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn record_roundtrip_rejects_torn_corrupt_foreign_and_nonphysical_calibration() {
        let r = Record {
            generation: 3,
            level: true,
            gyro_bias_dps: [-1.5, 30.1, -0.1],
            samples: 480,
            ..Record::default()
        };
        assert!(r.valid() && r.high_bias());
        let bytes = r.encode();
        assert_eq!(Record::decode(&bytes), Ok(r));
        for i in 0..bytes.len() {
            assert!(Record::decode(&bytes[..i]).is_err());
            let mut bad = bytes;
            bad[i] ^= 1;
            assert!(Record::decode(&bad).is_err());
        }
        for field in 0..30 {
            let mut bad = bytes;
            bad[12 + 4 * field..16 + 4 * field].copy_from_slice(&f32::NAN.to_le_bytes());
            let c = crc(&bad[..140]);
            bad[140..].copy_from_slice(&c.to_le_bytes());
            assert!(Record::decode(&bad).is_err());
        }
        let mut future = bytes;
        future[4] = 2;
        let c = crc(&future[..140]);
        future[140..].copy_from_slice(&c.to_le_bytes());
        assert_eq!(Record::decode(&future), Err(RecordError::Unsupported));
    }
    #[test]
    fn ellipsoid_fit_recovers_bias_scale_cross_axis_and_validates_unfitted_directions() {
        let bias = [0.035, -0.025, 0.018];
        let distortion = [[1.04, 0.015, -0.01], [0., 0.97, 0.012], [0., 0., 1.02]];
        let points = core::array::from_fn(|i| {
            let v = mat(distortion, target(i));
            core::array::from_fn(|n| v[n] + bias[n])
        });
        let (b, m, rms, max) = fit(points).expect("well-covered realistic data must calibrate");
        for n in 0..3 {
            assert!((b[n] - bias[n]).abs() < 0.0001);
        }
        for point in points {
            let v = mat(m, core::array::from_fn(|n| point[n] - b[n]));
            assert!((norm(v) - 1.).abs() < 0.0002);
        }
        assert!(rms < 0.0002 && max < 0.0002);
        let mut bad = points;
        bad[23][0] += 0.15;
        assert!(
            fit(bad).is_none(),
            "withheld failure must reject, not overfit"
        );
        assert!(fit([[0., 0., 1.]; 24]).is_none());
        let biased = core::array::from_fn(|i| {
            let v = target(i);
            [v[0] + 0.4, v[1], v[2]]
        });
        assert!(fit(biased).is_none());
    }
    #[test]
    fn level_collects_consecutive_still_samples_and_requires_review_before_use() {
        let old = Record::default();
        let mut s = Session::default();
        s.start(41, false, old, 0);
        for i in 0..1000 {
            s.ingest(
                [0.08, -0.04, 0.99],
                [-1.5, 30.1, -0.1],
                29.,
                1 + i * 8921,
                false,
            );
        }
        assert_eq!(s.view.phase, Phase::Review);
        assert!(s.view.high_bias);
        let r = s.candidate.unwrap();
        assert_eq!(r.generation, 1);
        assert_eq!(r.gyro_bias_dps, [-1.5, 30.1, -0.1]);
        let (a, g) = r.apply([0.08, -0.04, 0.99], [-1.5, 30.1, -0.1]);
        assert!(a[0].abs() < 0.00001 && a[1].abs() < 0.00001 && norm(g) < 0.00001);
        assert_eq!(s.old, old);
        s.cancel();
        assert!(s.candidate.is_none());
        assert_eq!(s.old, old);
    }
    #[test]
    fn full_guided_session_fits_then_requires_final_upright_level_reference() {
        let mut s = Session::default();
        s.start(7, true, Record::default(), 0);
        let mut at = 1u64;
        while s.view.phase != Phase::Level && at < 150_000_000 {
            let a = target(s.view.pose as usize);
            let a = [a[0] * 1.03 + 0.02, a[1] * 0.98 - 0.01, a[2] * 1.01 + 0.015];
            s.ingest(a, [1., -2., 0.1], 28., at, false);
            at += 8921;
        }
        assert_eq!(s.view.phase, Phase::Level);
        assert_eq!(s.view.pose, 24);
        while s.view.phase == Phase::Level && at < 170_000_000 {
            s.ingest([0.02, -0.01, 1.025], [1., -2., 0.1], 28., at, false);
            at += 8921;
        }
        assert_eq!(s.view.phase, Phase::Review);
        assert!(s.candidate.unwrap().full);
    }
    #[test]
    fn moving_noisy_wrong_pose_and_stale_input_cannot_make_a_level_calibration() {
        for mode in 0..4 {
            let mut s = Session::default();
            s.start(1, false, Record::default(), 0);
            for i in 0..1400 {
                let a = if mode == 0 {
                    [1., 0., 0.]
                } else {
                    [if mode == 1 && i % 2 == 0 { 0.02 } else { 0. }, 0., 1.]
                };
                let g = if mode == 2 {
                    [if i % 2 == 0 { 1. } else { -1. }, 0., 0.]
                } else {
                    [0.; 3]
                };
                s.ingest(
                    a,
                    g,
                    25.,
                    1 + i * if mode == 3 { 110_000 } else { 8921 },
                    false,
                );
            }
            assert_ne!(s.view.phase, Phase::Review);
            assert!(s.candidate.is_none());
        }
    }
    #[test]
    fn thermal_excursions_require_fresh_full_calibration_and_metadata_counts_real_samples() {
        let old = Record {
            generation: 9,
            level: true,
            full: true,
            samples: 5000,
            ..Record::default()
        };
        assert!(old.temperature_ok(28.));
        assert!(!old.temperature_ok(28.01));
        assert!(!old.temperature_ok(f32::NAN));
        let mut s = Session::default();
        s.start(10, false, old, 0);
        for i in 0..1000 {
            s.ingest([0., 0., 1.], [0.; 3], 30., 1 + i * 8921, false);
        }
        assert_eq!(s.view.phase, Phase::Failed);
        assert_eq!(s.view.reason, Reason::Temperature);
        assert!(s.candidate.is_none());
        s.start(11, false, Record::default(), 0);
        for i in 0..1000 {
            s.ingest([0., 0., 1.], [0.; 3], 30., 1 + i * 8921, false);
        }
        assert_eq!(s.view.phase, Phase::Review);
        assert!(s.candidate.unwrap().samples > LEVEL_SAMPLES);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    Warmup,
    Collect,
    Level,
    Review,
    Saving,
    Saved,
    Cancelled,
    Failed,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reason {
    #[default]
    None,
    Still,
    Direction,
    Noise,
    Temperature,
    Gap,
    Saturated,
    Geometry,
    Storage,
    Timeout,
    Sensor,
    LevelTilt,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub job: u32,
    pub phase: Phase,
    pub reason: Reason,
    pub full: bool,
    pub pose: u8,
    pub progress: u8,
    pub high_bias: bool,
    pub gyro_bias_milli: [i32; 3],
    pub noise_milli: u32,
    pub rms_milli: u32,
    pub temperature_centi: i32,
    pub temperature_available: bool,
    pub reference_temperature_centi: i32,
    pub temperature_warning: bool,
    pub calibrated: bool,
    pub full_calibrated: bool,
    pub generation: u32,
    pub accel_rejected: bool,
    pub reset: bool,
    pub storage_available: bool,
}
impl View {
    pub fn busy(self) -> bool {
        matches!(
            self.phase,
            Phase::Warmup | Phase::Collect | Phase::Level | Phase::Review | Phase::Saving
        )
    }
}

pub struct Session {
    pub view: View,
    pub candidate: Option<Record>,
    old: Record,
    points: [Vec3; POSES],
    window: Moments,
    last: Option<(Vec3, Vec3, u64)>,
    started: u64,
    window_at: u64,
    warm_until: u64,
    temperature_min: f32,
    temperature_max: f32,
    total: Moments,
    accepted_samples: u32,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            view: View::default(),
            candidate: None,
            old: Record::default(),
            points: [[0.; 3]; POSES],
            window: Moments::default(),
            last: None,
            started: 0,
            window_at: 0,
            warm_until: 0,
            temperature_min: 85.,
            temperature_max: -20.,
            total: Moments::default(),
            accepted_samples: 0,
        }
    }
}
impl Session {
    pub fn prepare_reset(&mut self, job: u32, old: Record, now: u64) {
        self.start(job, false, old, now);
        self.view.phase = Phase::Review;
        self.view.reset = true;
        self.candidate = Some(Record {
            generation: old.generation.wrapping_add(1).max(1),
            ..Record::default()
        });
    }
    pub fn check_timeout(&mut self, at: u64) {
        if self.view.busy()
            && self.view.phase != Phase::Saving
            && at.saturating_sub(self.started) > 600_000_000
        {
            self.fail(Reason::Timeout);
        }
    }
    pub fn start(&mut self, job: u32, full: bool, old: Record, now: u64) {
        *self = Self {
            old,
            started: now,
            warm_until: now + 3_000_000,
            view: View {
                job,
                full,
                phase: Phase::Warmup,
                calibrated: old.level,
                generation: old.generation,
                ..View::default()
            },
            ..Self::default()
        };
        if !full && old.full {
            self.temperature_min = old.min_temperature_c;
            self.temperature_max = old.max_temperature_c;
        }
    }
    pub fn cancel(&mut self) {
        self.view.phase = Phase::Cancelled;
        self.candidate = None;
    }
    pub fn fail(&mut self, r: Reason) {
        self.view.phase = Phase::Failed;
        self.view.reason = r;
        self.candidate = None;
    }
    pub fn ingest(&mut self, a: Vec3, g: Vec3, t: f32, at: u64, saturated: bool) {
        if !matches!(
            self.view.phase,
            Phase::Warmup | Phase::Collect | Phase::Level
        ) {
            return;
        }
        if at.saturating_sub(self.started) > 600_000_000 {
            self.fail(Reason::Timeout);
            return;
        }
        if saturated
            || !a.iter().chain(g.iter()).all(|v| v.is_finite())
            || !(-20.0..=85.).contains(&t)
        {
            self.fail(Reason::Saturated);
            return;
        }
        let gap = self
            .last
            .is_some_and(|(_, _, last)| at <= last || at - last >= 100_000);
        let moved = self.last.is_some_and(|(old, gyro, _)| {
            norm(core::array::from_fn(|i| a[i] - old[i])) > 0.025
                || norm(core::array::from_fn(|i| g[i] - gyro[i])) > 1.5
        });
        self.last = Some((a, g, at));
        self.view.temperature_centi = (t * 100.) as i32;
        self.view.temperature_available = true;
        if gap || moved || !(0.7..=1.3).contains(&norm(a)) || g.iter().any(|v| v.abs() > 64.) {
            self.window = Moments::default();
            self.window_at = at;
            self.view.progress = 0;
            self.view.reason = if gap { Reason::Gap } else { Reason::Still };
            return;
        }
        if at < self.warm_until {
            return;
        }
        if self.view.phase == Phase::Warmup {
            self.view.phase = if self.view.full {
                Phase::Collect
            } else {
                Phase::Level
            };
        }
        let direction = if self.view.phase == Phase::Level {
            [0., 0., 1.]
        } else {
            target(self.view.pose as usize)
        };
        let corrected = if self.view.phase == Phase::Level {
            self.candidate.unwrap_or(self.old).corrected_accel(a)
        } else {
            a
        };
        if unit(corrected).is_none_or(|v| dot(v, direction) < 0.9063)
            || (self.view.phase == Phase::Collect
                && self.points[..self.view.pose as usize]
                    .iter()
                    .any(|p| unit(*p).zip(unit(a)).is_some_and(|(p, a)| dot(p, a) > 0.98)))
        {
            self.window = Moments::default();
            self.window_at = at;
            self.view.progress = 0;
            self.view.reason = if self.view.phase == Phase::Level {
                Reason::LevelTilt
            } else {
                Reason::Direction
            };
            return;
        }
        if self.window.n == 0 {
            self.window_at = at;
        }
        self.window.push(a, g, t);
        let needed = if self.view.phase == Phase::Level {
            LEVEL_SAMPLES
        } else {
            POSE_SAMPLES
        };
        self.view.progress = (self.window.n.saturating_mul(100) / needed).min(100) as u8;
        if self.window.n >= 30 && !self.window.stationary() {
            self.window = Moments::default();
            self.window_at = at;
            self.view.progress = 0;
            self.view.reason = Reason::Noise;
            return;
        }
        self.view.reason = Reason::Still;
        if self.window.n < needed
            || at - self.window_at
                < if self.view.phase == Phase::Level {
                    4_500_000
                } else {
                    1_700_000
                }
        {
            return;
        }
        let m = self.window;
        self.accepted_samples = self.accepted_samples.saturating_add(m.n);
        self.temperature_min = self.temperature_min.min(m.mean[6] as f32);
        self.temperature_max = self.temperature_max.max(m.mean[6] as f32);
        if self.temperature_max - self.temperature_min > 3. {
            self.fail(Reason::Temperature);
            return;
        }
        self.total.push(m.accel(), m.gyro(), m.mean[6] as f32);
        if self.total.n >= 4 && self.total.sd()[3..6].iter().any(|v| *v > 0.5) {
            self.fail(Reason::Noise);
            return;
        }
        if self.view.phase == Phase::Collect {
            self.points[self.view.pose as usize] = m.accel();
            self.view.pose += 1;
            self.view.progress = 0;
            if self.view.pose as usize == POSES {
                let Some((bias, matrix, rms, max)) = fit(self.points) else {
                    self.fail(Reason::Geometry);
                    return;
                };
                self.candidate = Some(Record {
                    full: true,
                    accel_bias_g: bias,
                    accel_matrix: matrix,
                    rms_g: rms,
                    max_error_g: max,
                    ..self.old
                });
                self.view.phase = Phase::Level;
            }
            self.window = Moments::default();
            self.warm_until = at + 700_000;
            return;
        }
        let mut candidate = self.candidate.unwrap_or(self.old);
        let Some(rotation) = gravity_rotation(candidate.corrected_accel(m.accel())) else {
            self.fail(Reason::Geometry);
            return;
        };
        candidate.level_rotation = rotation;
        candidate.level = true;
        candidate.generation = self.old.generation.wrapping_add(1).max(1);
        candidate.gyro_bias_dps = m.gyro();
        candidate.gyro_noise_dps = core::array::from_fn(|i| m.sd()[i + 3]);
        candidate.accel_noise_g = core::array::from_fn(|i| m.sd()[i]);
        candidate.temperature_c = m.mean[6] as f32;
        candidate.min_temperature_c = self.temperature_min;
        candidate.max_temperature_c = self.temperature_max;
        candidate.samples = self.accepted_samples;
        if !candidate.valid() {
            self.fail(Reason::Geometry);
            return;
        }
        self.view.gyro_bias_milli = candidate.gyro_bias_dps.map(|v| (v * 1000.) as i32);
        self.view.noise_milli =
            (candidate.gyro_noise_dps.into_iter().fold(0f32, f32::max) * 1000.) as u32;
        self.view.high_bias = candidate.high_bias();
        self.view.rms_milli = (candidate.rms_g * 1000.) as u32;
        self.view.phase = Phase::Review;
        self.view.reason = Reason::None;
        self.candidate = Some(candidate);
    }
    pub fn target_error_deg(&self, a: Vec3) -> Option<f32> {
        Some(
            acosf(dot(unit(a)?, target(self.view.pose.min(23) as usize)).clamp(-1., 1.))
                .to_degrees(),
        )
    }
}
