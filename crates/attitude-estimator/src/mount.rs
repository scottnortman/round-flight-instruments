//! Right-handed signed axis permutation, separate from bias calibration/filtering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mount([i8; 3]);
impl Mount {
    /// Output forward/right/down axes expressed as signed input axes 1=X,2=Y,3=Z.
    /// Reflections, duplicates and invalid axes are rejected.
    pub fn new(axes: [i8; 3]) -> Option<Self> {
        if axes.iter().any(|a| !matches!(*a, -3..=-1 | 1..=3)) {
            return None;
        }
        let basis = axes.map(|a| {
            let mut row = [0i8; 3];
            row[a.unsigned_abs() as usize - 1] = a.signum();
            row
        });
        let [a, b, c] = basis;
        let det = a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
        (det == 1).then_some(Self(axes))
    }
    pub fn apply(self, v: [f32; 3]) -> [f32; 3] {
        self.0
            .map(|a| v[a.unsigned_abs() as usize - 1] * a.signum() as f32)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_only_the_24_proper_axis_rotations_and_preserves_norm() {
        let mut accepted = 0;
        for a in -3..=3 {
            for b in -3..=3 {
                for c in -3..=3 {
                    if let Some(m) = Mount::new([a, b, c]) {
                        accepted += 1;
                        let v = m.apply([1., 2., 3.]);
                        assert_eq!(v.iter().map(|x| x * x).sum::<f32>(), 14.);
                    }
                }
            }
        }
        assert_eq!(accepted, 24);
        for axes in [[i8::MIN, 2, 3], [4, 2, 3], [1, 1, 3], [1, 2, -3]] {
            assert!(Mount::new(axes).is_none());
        }
        assert_eq!(
            Mount::new([3, -2, 1]).unwrap().apply([1., 2., 3.]),
            [3., -2., 1.]
        );
    }
}
