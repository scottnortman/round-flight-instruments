//! Vertical speed directly in feet/minute, positive climb. No altitude derivative.
use crate::{
    dial,
    raster::{rgb, Canvas, Target},
};
use core::fmt::Write;
use instrument_core::simulator_link::measurements::Quantity;
use instrument_core::{Quality, Reading};
#[derive(Clone, Copy, Debug)]
pub struct Profile {
    full_scale_fpm: f32,
    span_deg: f32,
}
impl Profile {
    pub fn new(full_scale_fpm: f32, span_deg: f32) -> Option<Self> {
        (full_scale_fpm.is_finite()
            && span_deg.is_finite()
            && (1.0..=100000.).contains(&full_scale_fpm)
            && (1.0..180.).contains(&span_deg))
        .then_some(Self {
            full_scale_fpm,
            span_deg,
        })
    }
    pub fn angle(self, fpm: f32) -> Option<f32> {
        (fpm.is_finite() && fpm.abs() <= self.full_scale_fpm)
            .then_some(270. + fpm / self.full_scale_fpm * self.span_deg)
    }
}
pub const DEFAULT_PROFILE: Profile = Profile {
    full_scale_fpm: 2000.,
    span_deg: 165.,
};
#[derive(Clone, Copy, Debug)]
pub struct Vsi {
    pub fpm: Option<f32>,
    pub needle_deg: Option<f32>,
    pub over_range: bool,
    pub quality: Quality,
}
pub fn model(r: Reading, ttl: u64) -> Vsi {
    model_with_profile(r, ttl, DEFAULT_PROFILE)
}
pub fn model_with_profile(r: Reading, ttl: u64, p: Profile) -> Vsi {
    let (fpm, quality) = dial::validate(r, Quantity::VerticalSpeedFpm, ttl);
    Vsi {
        fpm,
        needle_deg: fpm.and_then(|v| p.angle(v)),
        over_range: fpm.is_some_and(|v| v.abs() > p.full_scale_fpm),
        quality,
    }
}
pub(crate) fn background<T: Target>(target: &mut T) {
    let mut c = Canvas::new(target);
    c.circle(240., 240., 228., dial::FACE);
    for n in -20i32..=20 {
        let angle = DEFAULT_PROFILE.angle(n as f32 * 100.).unwrap_or(270.);
        let major = n % 5 == 0;
        c.stroke(
            dial::point(if major { 184. } else { 197. }, angle),
            dial::point(210., angle),
            if major { 4. } else { 2. },
            dial::MARK,
        );
        if major {
            let (x, y) = dial::point(if n == 0 { 159. } else { 151. }, angle);
            let mut text = crate::render::Text::new();
            let _ = write!(text, "{}", n.abs());
            c.text(x, y, 30., text.str(), dial::MARK);
        }
    }
    c.text(240., 175., 18., "VERTICAL SPEED", dial::MARK);
    c.text(145., 199., 18., "UP", dial::MARK);
    c.text(145., 281., 18., "DN", dial::MARK);
    c.text(240., 305., 19., "100 FT/MIN", dial::MARK);
}
pub(crate) fn draw<T: Target>(target: &mut T, m: Vsi, source: &str) {
    let mut c = Canvas::new(target);
    c.text(240., 335., 18., source, rgb(255, 191, 70));
    if let Some(angle) = m.needle_deg {
        dial::needle(&mut c, angle);
    } else if m.over_range {
        c.text(240., 225., 22., "OVER RANGE", dial::RED);
        let mut text = crate::render::Text::new();
        let _ = write!(text, "{:+.0} FT/MIN", m.fpm.unwrap_or(0.));
        c.text(240., 270., 23., text.str(), dial::MARK);
    } else {
        dial::invalid(&mut c, "VSI", m.quality);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn r(v: f32) -> Reading {
        Reading {
            value: Some(v),
            quality: Quality::Valid,
            age_ms: Some(0),
        }
    }
    #[test]
    fn climb_descent_zero_and_over_range_never_alias() {
        assert_eq!(model(r(0.), 500).needle_deg, Some(270.));
        for v in [-2000., -1000., 500., 2000.] {
            let p = dial::point(187., model(r(v), 500).needle_deg.unwrap());
            assert_eq!(p.1 < 240., v > 0.);
        }
        for v in [-3500., 3500.] {
            let m = model(r(v), 500);
            assert!(m.over_range && m.needle_deg.is_none() && m.fpm == Some(v));
        }
        assert_eq!(
            model(
                Reading {
                    age_ms: Some(500),
                    ..r(0.)
                },
                500
            )
            .quality,
            Quality::Stale
        );
        assert!(model(
            Reading {
                quality: Quality::Missing,
                ..r(0.)
            },
            500
        )
        .needle_deg
        .is_none());
        assert!(Profile::new(0., 165.).is_none());
        assert!(Profile::new(2000., 180.).is_none());
        assert!(
            model_with_profile(r(4000.), 500, Profile::new(6000., 165.).unwrap())
                .needle_deg
                .is_some()
        );
    }
}
