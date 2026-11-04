//! Magnetic heading card, following the phone-tested standalone Heading face.
//! No true-heading, track or unreferenced IMU fallback is permitted.
use crate::{
    dial,
    raster::{rgb, Canvas, Target},
};
use core::fmt::Write;
use instrument_core::simulator_link::measurements::Quantity;
use instrument_core::{Quality, Reading};
// Exact quadrants plus 5-degree first-quadrant directions. Compute the
// heading rotation once per frame, not two trig calls per tick endpoint.
const QUARTER: [(f32, f32); 18] = [
    (0.000000000, 1.000000000),
    (0.087155743, 0.996_194_7),
    (0.173_648_18, 0.984_807_7),
    (0.258_819_04, 0.965_925_8),
    (0.342_020_15, 0.939_692_6),
    (0.422_618_27, 0.906_307_8),
    (0.5, 0.866_025_4),
    (0.573_576_45, 0.819_152_06),
    (0.642_787_64, 0.766_044_44),
    (0.707_106_77, 0.707_106_77),
    (0.766_044_44, 0.642_787_64),
    (0.819_152_06, 0.573_576_45),
    (0.866_025_4, 0.5),
    (0.906_307_8, 0.422_618_27),
    (0.939_692_6, 0.342_020_15),
    (0.965_925_8, 0.258_819_04),
    (0.984_807_7, 0.173_648_18),
    (0.996_194_7, 0.087155743),
];
#[derive(Clone, Copy, Debug)]
pub struct Heading {
    pub magnetic_deg: Option<f32>,
    pub quality: Quality,
}
pub fn model(mut r: Reading, ttl: u64) -> Heading {
    // A valid f64 value immediately below 360 can round to 360 in the shared
    // f32 view. Preserve the circular boundary instead of spuriously failing N.
    if r.value == Some(360.) {
        r.value = Some(0.);
    }
    let (magnetic_deg, quality) = dial::validate(r, Quantity::MagneticHeadingDeg, ttl);
    Heading {
        magnetic_deg,
        quality,
    }
}
pub fn relative_bearing(bearing: f32, heading: f32) -> f32 {
    bearing - heading
}
pub(crate) fn background<T: Target>(target: &mut T) {
    let mut c = Canvas::new(target);
    c.circle(240., 240., 228., dial::FACE);
    // Fixed airplane overlays the rotating card; convex pieces retain the
    // phone reference's concave silhouette without a general path engine.
    let plane = rgb(255, 202, 100);
    for shape in [
        &[
            (236., 168.),
            (244., 168.),
            (248., 244.),
            (247., 287.),
            (240., 300.),
            (233., 287.),
            (232., 244.),
        ][..],
        &[(244., 225.), (336., 247.), (336., 257.), (246., 244.)][..],
        &[(236., 225.), (144., 247.), (144., 257.), (234., 244.)][..],
        &[(244., 287.), (274., 301.), (274., 308.), (240., 300.)][..],
        &[(236., 287.), (206., 301.), (206., 308.), (240., 300.)][..],
    ] {
        c.polygon(shape, plane);
    }

    c.polygon(&[(232., 20.), (248., 20.), (240., 36.)], dial::MARK);
    c.text(240., 137., 18., "HEADING", rgb(175, 185, 192));
    c.text(240., 365., 14., "MAGNETIC", rgb(175, 185, 192));
}
pub(crate) fn draw<T: Target>(target: &mut T, m: Heading, source: &str) {
    let mut c = Canvas::new(target);
    if let Some(h) = m.magnetic_deg {
        let radians = h * core::f32::consts::PI / 180.;
        let (hs, hc) = (libm::sinf(radians), libm::cosf(radians));
        for (index, bearing) in (0..360).step_by(5).enumerate() {
            let (s, co) = QUARTER[index % 18];
            let (s, co) = match index / 18 {
                0 => (s, co),
                1 => (co, -s),
                2 => (-s, -co),
                _ => (-co, s),
            };
            let (s, co) = (s * hc - co * hs, co * hc + s * hs);
            let point = |r: f32| (240. + r * s, 240. - r * co);
            let major = bearing % 30 == 0;
            let r = if major {
                184.
            } else if bearing % 10 == 0 {
                192.
            } else {
                198.
            };
            let angle = relative_bearing(bearing as f32, h);
            c.stroke(
                point(r),
                point(210.),
                if major { 4. } else { 2. },
                dial::MARK,
            );
            if major {
                let mut text = crate::render::Text::new();
                match bearing {
                    0 => {
                        let _ = text.write_str("N");
                    }
                    90 => {
                        let _ = text.write_str("E");
                    }
                    180 => {
                        let _ = text.write_str("S");
                    }
                    270 => {
                        let _ = text.write_str("W");
                    }
                    _ => {
                        let _ = write!(text, "{}", bearing / 10);
                    }
                }
                c.text_rotated([point(154.)], 28., text.str(), angle, dial::MARK, None);
            }
        }
    } else {
        dial::invalid(&mut c, "HDG", m.quality);
    }
    c.text(240., 343., 18., source, rgb(255, 191, 70));
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
    fn card_direction_wrap_and_validity_are_explicit() {
        for h in [0., 90., 180., 270., 359.9] {
            assert_eq!(model(r(h), 500).magnetic_deg, Some(h));
            assert_eq!(relative_bearing(h, h), 0.);
        }
        let east = dial::point(154., relative_bearing(90., 90.));
        assert!((east.0 - 240.).abs() < 0.001 && east.1 < 100.);
        let n = dial::point(154., relative_bearing(0., 90.));
        assert!(n.0 < 100. && (n.1 - 240.).abs() < 0.001);
        assert_eq!(model(r(360.), 500).magnetic_deg, Some(0.));
        for bad in [-1., 360.1, f32::NAN] {
            assert_eq!(model(r(bad), 500).quality, Quality::Failed);
        }
        assert_eq!(
            model(
                Reading {
                    age_ms: Some(500),
                    ..r(90.)
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
        .magnetic_deg
        .is_none());
    }
}
