//! Phone-reference turn coordinator. The rate is the instrument reading, not
//! bank angle or raw yaw rate; an adapter must not add roll response twice.
use crate::raster::{rgb, Canvas, Target};
use instrument_core::{Field, Quality, Reading};

pub const LOCAL_STALE_MS: u64 = 100;
const FACE: u16 = rgb(8, 11, 14);
const MARK: u16 = rgb(238, 240, 230);
const RED: u16 = rgb(255, 103, 103);
#[derive(Clone, Copy, Debug)]
pub struct Turn {
    pub aircraft_deg: Option<f32>,
    pub ball: Option<(f32, f32)>,
    pub rate_quality: Quality,
    pub ball_quality: Quality,
}
fn quality(r: Reading, field: Field, stale_ms: u64) -> Quality {
    if r.quality != Quality::Valid {
        return r.quality;
    }
    if r.value.is_none_or(|v| !field.accepts(v)) || r.age_ms.is_none() {
        Quality::Failed
    } else if r.age_ms.is_some_and(|age| age >= stale_ms) {
        Quality::Stale
    } else {
        Quality::Valid
    }
}
pub fn model(rate: Reading, ball: Reading, stale_ms: u64) -> Turn {
    let rate_quality = quality(rate, Field::TurnDps, stale_ms);
    let ball_quality = quality(ball, Field::Ball, stale_ms);
    Turn {
        aircraft_deg: (rate_quality == Quality::Valid)
            .then(|| (rate.value.unwrap_or(0.) / 3. * 20.).clamp(-45., 45.)),
        ball: (ball_quality == Quality::Valid).then(|| ball_position(ball.value.unwrap_or(0.))),
        rate_quality,
        ball_quality,
    }
}
pub fn ball_position(ball: f32) -> (f32, f32) {
    let a = ball.clamp(-1., 1.) * 17. * core::f32::consts::PI / 180.;
    (240. + 350. * libm::sinf(a), 20. + 350. * libm::cosf(a))
}
impl Turn {
    pub fn quality(self) -> Quality {
        [
            Quality::TimingFault,
            Quality::Failed,
            Quality::Stale,
            Quality::Missing,
        ]
        .into_iter()
        .find(|q| [self.rate_quality, self.ball_quality].contains(q))
        .unwrap_or(Quality::Valid)
    }
}
pub(crate) fn background<T: Target>(target: &mut T) {
    let mut c = Canvas::new(target);
    c.circle(240., 240., 228., FACE);
    for (a, b) in [((66., 222.), (88., 222.)), ((392., 222.), (414., 222.))] {
        c.stroke(a, b, 4., MARK);
    }
    let angle = 20. * core::f32::consts::PI / 180.;
    for side in [-1., 1.] {
        c.stroke(
            (
                240. + side * 148. * libm::cosf(angle),
                222. + 148. * libm::sinf(angle),
            ),
            (
                240. + side * 174. * libm::cosf(angle),
                222. + 174. * libm::sinf(angle),
            ),
            4.,
            MARK,
        );
    }
    c.text(240., 103., 23., "TURN COORDINATOR", MARK);
    c.text(86., 324., 28., "L", MARK);
    c.text(394., 324., 28., "R", MARK);
    // Fill the annular tube once, with no radial seams between brush strokes.
    for y in 332..390 {
        for x in 116..365 {
            let (dx, dy) = (x as f32 - 240., y as f32 - 20.);
            let d = dx * dx + dy * dy;
            if dx.abs() > dy * 0.30573067 || !(331.5 * 331.5..=368.5 * 368.5).contains(&d) {
                continue;
            }
            let color = if (335.5 * 335.5..=364.5 * 364.5).contains(&d) {
                rgb(198, 208, 210)
            } else {
                rgb(84, 97, 107)
            };
            c.rect(x as f32, y as f32, 1., 1., color);
        }
    }
    for side in [-1., 1.] {
        let (x, y) = ball_position(side);
        c.circle(x, y, 18.5, rgb(84, 97, 107));
        c.circle(x, y, 14.5, rgb(198, 208, 210));
    }
    c.text(240., 413., 23., "2 MIN", MARK);
    c.text(240., 434., 15., "NO PITCH", MARK);
    c.text(240., 451., 15., "INFORMATION", MARK);
}
pub(crate) fn draw<T: Target>(target: &mut T, m: Turn, source: &str) {
    let mut c = Canvas::new(target);
    c.text(240., 136., 20., source, rgb(255, 191, 70));
    if let Some(angle) = m.aircraft_deg {
        let a = angle * core::f32::consts::PI / 180.;
        let (s, co) = (libm::sinf(a), libm::cosf(a));
        let rotate = |x: f32, y: f32| {
            (
                240. + (x - 240.) * co - (y - 222.) * s,
                222. + (x - 240.) * s + (y - 222.) * co,
            )
        };
        // Convex pieces preserve the silhouette of the phone's concave path.
        for shape in [
            &[(101., 220.), (225., 219.), (228., 230.), (101., 227.)][..],
            &[(255., 219.), (379., 220.), (379., 227.), (252., 230.)][..],
            &[
                (225., 219.),
                (230., 212.),
                (250., 212.),
                (255., 219.),
                (252., 230.),
                (228., 230.),
            ][..],
            &[(210., 207.), (270., 207.), (270., 212.), (210., 212.)][..],
            &[(237., 181.), (243., 181.), (243., 214.), (237., 214.)][..],
        ] {
            let mut points = [(0., 0.); 8];
            for (out, &(x, y)) in points.iter_mut().zip(shape) {
                *out = rotate(x, y);
            }
            c.polygon(&points[..shape.len()], MARK);
        }
        let (x, y) = rotate(240., 229.);
        c.circle(x, y, 11., MARK);
    } else {
        c.rect(343., 160., 54., 32., rgb(141, 32, 42));
        c.text(370., 183., 20., "OFF", MARK);
        c.stroke((164., 184.), (316., 260.), 5., RED);
        c.stroke((316., 184.), (164., 260.), 5., RED);
        c.rect(115., 205., 250., 34., rgb(25, 15, 18));
        c.text(
            240.,
            227.,
            20.,
            match m.rate_quality {
                Quality::Stale => "TURN DATA STALE",
                Quality::Failed => "TURN DATA FAILED",
                Quality::TimingFault => "TURN TIMING FAULT",
                _ => "TURN DATA MISSING",
            },
            RED,
        );
    }
    if let Some((x, y)) = m.ball {
        c.circle(x, y, 13., rgb(2, 4, 5));
        for (r, color) in [
            (11., rgb(16, 21, 25)),
            (8., rgb(31, 38, 43)),
            (4., rgb(66, 74, 80)),
        ] {
            c.circle(x - 2., y - 2., r, color);
        }
    } else {
        c.text(240., 373., 18., "BALL DATA", rgb(134, 35, 43));
    }
    // Center reference bars remain in front of the ball, as in the phone face.
    for x in [222., 258.] {
        c.stroke((x, 350.), (x, 388.), 4., FACE);
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    #[test]
    fn rotating_buffers_match_full_render_through_motion_and_independent_failures() {
        use crate::{
            damage::FrameHistory,
            render::{draw_frame, CachedRenderer},
            Snapshot,
        };
        use instrument_core::{App, Command, Instrument};
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.tick(0);
        app.apply(Command::Select(Instrument::Turn)).unwrap();
        let mut snapshot = Snapshot::from_app(&app);
        let mut backdrop = std::vec![0;480*480];
        let mut renderer = CachedRenderer::new(&mut backdrop, 480, 480).unwrap();
        let mut expected = std::vec![0;480*480];
        let mut frames = core::array::from_fn::<_, 3, _>(|_| expected.clone());
        let mut histories = core::array::from_fn::<_, 3, _>(|_| FrameHistory::new());
        for n in 0..40 {
            snapshot.turn_rate.value = Some((n as f32 * 0.7).sin() * 10.);
            snapshot.ball.value = Some((n as f32 * 0.4).sin());
            snapshot.turn_rate.quality = if n % 9 == 0 {
                Quality::Stale
            } else {
                Quality::Valid
            };
            snapshot.ball.quality = if n % 7 == 0 {
                Quality::Failed
            } else {
                Quality::Valid
            };
            snapshot.center_pressed = n % 4 < 2;
            assert!(draw_frame(&mut expected, 480, 480, snapshot, false));
            for i in 0..3 {
                assert!(renderer.draw_incremental(
                    &mut frames[i],
                    &mut histories[i],
                    snapshot,
                    false
                ));
                assert_eq!(frames[i], expected, "phase {n}, buffer {i}");
            }
        }
    }
    use super::*;
    fn r(value: f32) -> Reading {
        Reading {
            value: Some(value),
            quality: Quality::Valid,
            age_ms: Some(0),
        }
    }
    #[test]
    fn standard_rate_sign_saturation_and_ball_geometry() {
        for (rate, expected) in [(-10., -45.), (-3., -20.), (0., 0.), (3., 20.), (10., 45.)] {
            assert_eq!(model(r(rate), r(0.), 100).aircraft_deg, Some(expected));
        }
        assert_eq!(ball_position(0.), (240., 370.));
        let (left, right) = (ball_position(-1.), ball_position(1.));
        assert!(left.0 < 240. && right.0 > 240. && left.1 < 370.);
        assert!((left.0 + right.0 - 480.).abs() < 0.001);
    }
    #[test]
    fn independent_faults_and_exact_expiry_never_fabricate_centered_data() {
        let stale = Reading {
            age_ms: Some(150),
            ..r(3.)
        };
        let live = Reading {
            age_ms: Some(149),
            ..r(0.)
        };
        let m = model(stale, live, 150);
        assert!(m.aircraft_deg.is_none() && m.ball.is_some());
        assert_eq!(m.rate_quality, Quality::Stale);
        for bad in [
            Reading {
                value: None,
                ..r(0.)
            },
            r(f32::NAN),
            Reading {
                age_ms: None,
                ..r(0.)
            },
        ] {
            let m = model(r(3.), bad, 100);
            assert_eq!(m.aircraft_deg, Some(20.));
            assert!(m.ball.is_none());
        }
        assert!(model(r(3.), r(1.01), 100).ball.is_none());
        assert!(model(
            Reading {
                age_ms: Some(100),
                ..r(3.)
            },
            r(0.),
            100
        )
        .aircraft_deg
        .is_none());
    }
}
