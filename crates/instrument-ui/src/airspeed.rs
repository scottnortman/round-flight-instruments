//! Indicated airspeed, in knots. Dial geometry is separate from input limits.
//! The nonlinear scale and example arcs match the phone-tested prototype.
use crate::raster::{rgb, Canvas, Target};
use core::fmt::Write;
use instrument_core::simulator_link::measurements::Quantity;
use instrument_core::{Quality, Reading};

pub const MAX_KT: f32 = 200.;
pub const SCALE: [(f32, f32); 11] = [
    (0., 0.),
    (20., 10.),
    (40., 60.),
    (60., 105.),
    (80., 160.),
    (100., 205.),
    (120., 242.),
    (140., 274.),
    (160., 299.),
    (180., 322.),
    (200., 342.),
];
const FACE: u16 = rgb(8, 11, 14);
const MARK: u16 = rgb(238, 240, 230);
const RED: u16 = rgb(255, 103, 103);
const NEEDLE_EDGE: u16 = rgb(255, 128, 0);

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub white: [f32; 2],
    pub green: [f32; 2],
    pub yellow: [f32; 2],
    pub red: f32,
}
pub const DEMO_LIMITS: Limits = Limits {
    white: [40., 85.],
    green: [48., 129.],
    yellow: [129., 163.],
    red: 163.,
};
/// Validated borrowed profile. A future host-configuration layer can own the
/// bounded points/limits and supply this view; no source or board dependency.
#[derive(Clone, Copy, Debug)]
pub struct Profile<'a> {
    scale: &'a [(f32, f32)],
    limits: Option<Limits>,
}
impl<'a> Profile<'a> {
    pub fn new(scale: &'a [(f32, f32)], limits: Option<Limits>) -> Option<Self> {
        if !(2..=32).contains(&scale.len()) || scale[0] != (0., 0.) {
            return None;
        }
        if scale.iter().any(|&(kt, a)| {
            !kt.is_finite()
                || !a.is_finite()
                || !(0.0..=10000.).contains(&kt)
                || !(0.0..360.).contains(&a)
        }) || scale
            .windows(2)
            .any(|p| p[1].0 <= p[0].0 || p[1].1 <= p[0].1)
        {
            return None;
        }
        let p = Self { scale, limits };
        if let Some(l) = limits {
            for [low, high] in [l.white, l.green, l.yellow] {
                if !low.is_finite()
                    || !high.is_finite()
                    || low < 0.
                    || low >= high
                    || high > p.max_kt()
                {
                    return None;
                }
            }
            if !l.red.is_finite()
                || l.green[1] != l.yellow[0]
                || l.yellow[1] != l.red
                || l.white[1] > l.red
            {
                return None;
            }
        }
        Some(p)
    }
    pub fn max_kt(self) -> f32 {
        self.scale[self.scale.len() - 1].0
    }
    pub fn limits(self) -> Option<Limits> {
        self.limits
    }
    pub fn angle_at(self, kt: f32) -> Option<f32> {
        if !kt.is_finite() || !(0.0..=self.max_kt()).contains(&kt) {
            return None;
        }
        self.scale
            .windows(2)
            .find(|p| kt <= p[1].0)
            .map(|p| p[0].1 + (p[1].1 - p[0].1) * (kt - p[0].0) / (p[1].0 - p[0].0))
    }
}
pub const DEFAULT_PROFILE: Profile<'static> = Profile {
    scale: &SCALE,
    limits: None,
};

#[derive(Clone, Copy, Debug)]
pub struct Airspeed {
    pub ias_kt: Option<f32>,
    pub needle_deg: Option<f32>,
    pub over_range: bool,
    pub quality: Quality,
}
pub fn angle_at(kt: f32) -> Option<f32> {
    DEFAULT_PROFILE.angle_at(kt)
}
pub fn model(reading: Reading, stale_ms: u64) -> Airspeed {
    model_with_profile(reading, stale_ms, DEFAULT_PROFILE)
}
pub fn model_with_profile(reading: Reading, stale_ms: u64, profile: Profile<'_>) -> Airspeed {
    let mut quality = reading.quality;
    if quality == Quality::Valid {
        if reading
            .value
            .is_none_or(|v| !Quantity::IndicatedAirspeedKt.accepts(v as f64))
            || reading.age_ms.is_none()
        {
            quality = Quality::Failed;
        } else if reading.age_ms.is_some_and(|age| age >= stale_ms) {
            quality = Quality::Stale;
        }
    }
    let ias_kt = (quality == Quality::Valid)
        .then_some(reading.value)
        .flatten();
    Airspeed {
        ias_kt,
        needle_deg: ias_kt.and_then(|kt| profile.angle_at(kt)),
        over_range: ias_kt.is_some_and(|v| v > profile.max_kt()),
        quality,
    }
}
pub fn point(radius: f32, deg: f32) -> (f32, f32) {
    let a = deg * core::f32::consts::PI / 180.;
    (240. + radius * libm::sinf(a), 240. - radius * libm::cosf(a))
}
fn arc<T: Target>(c: &mut Canvas<T>, low: f32, high: f32, radius: f32, width: f32, color: u16) {
    let (Some(mut a), Some(end)) = (angle_at(low), angle_at(high)) else {
        return;
    };
    while a < end {
        let b = (a + 1.).min(end);
        c.polygon(
            &[
                point(radius - width / 2., a),
                point(radius + width / 2., a),
                point(radius + width / 2., b),
                point(radius - width / 2., b),
            ],
            color,
        );
        a = b;
    }
}
pub(crate) fn background<T: Target>(target: &mut T, demo_limits: bool) {
    let mut c = Canvas::new(target);
    c.circle(240., 240., 228., FACE);
    // These are the reference prototype's C172S EXAMPLE, never generic live
    // aircraft limits. The caller deliberately omits them for external sources.
    if demo_limits {
        let l = DEMO_LIMITS;
        arc(&mut c, l.green[0], l.green[1], 216., 9., rgb(60, 197, 107));
        arc(
            &mut c,
            l.yellow[0],
            l.yellow[1],
            216.,
            9.,
            rgb(244, 216, 77),
        );
        arc(&mut c, l.white[0], l.white[1], 203., 7., MARK);
        let red = angle_at(l.red).unwrap_or(0.);
        c.stroke(point(194., red), point(223., red), 4.5, RED);
    }
    for kt in (0..=200).step_by(5) {
        let major = kt % 20 == 0;
        let inner = if major {
            184.
        } else if kt % 10 == 0 {
            190.
        } else {
            194.
        };
        let angle = angle_at(kt as f32).unwrap_or(0.);
        c.stroke(
            point(inner, angle),
            point(198., angle),
            if major { 4. } else { 2. },
            MARK,
        );
        if major && kt > 0 {
            let (x, y) = point(148., angle);
            let mut text = crate::render::Text::new();
            let _ = write!(text, "{kt}");
            c.text(x, y + 10., 28., text.str(), MARK);
        }
    }
    c.text(240., 175., 23., "AIRSPEED", MARK);
    c.text(240., 305., 26., "KNOTS", MARK);
    c.text(
        240.,
        355.,
        15.,
        if demo_limits {
            "C172S DEMO"
        } else {
            "LIMITS NOT SET"
        },
        rgb(175, 185, 192),
    );
}
pub(crate) fn draw<T: Target>(target: &mut T, m: Airspeed, source: &str) {
    let mut c = Canvas::new(target);
    c.text(240., 335., 18., source, rgb(255, 191, 70));
    if let Some(angle) = m.needle_deg {
        let a = angle * core::f32::consts::PI / 180.;
        let (s, co) = (libm::sinf(a), libm::cosf(a));
        let rotate = |x: f32, y: f32| {
            (
                240. + (x - 240.) * co - (y - 240.) * s,
                240. + (x - 240.) * s + (y - 240.) * co,
            )
        };
        let shape = [
            (237., 277.),
            (237., 95.),
            (240., 53.),
            (243., 95.),
            (243., 277.),
            (240., 283.),
        ];
        let points = shape.map(|(x, y)| rotate(x, y));
        c.polygon(&points, MARK);
        for i in 0..points.len() {
            c.stroke(points[i], points[(i + 1) % points.len()], 1., NEEDLE_EDGE);
        }
        c.circle(240., 240., 15., rgb(23, 32, 40));
        c.circle(240., 240., 5., rgb(72, 84, 94));
    } else if m.over_range {
        c.text(240., 225., 23., "OVER RANGE", RED);
        let mut text = crate::render::Text::new();
        let _ = write!(text, "{:.0} KIAS", m.ias_kt.unwrap_or(0.));
        c.text(240., 270., 26., text.str(), MARK);
    } else {
        c.stroke((132., 174.), (348., 282.), 7., RED);
        c.stroke((348., 174.), (132., 282.), 7., RED);
        c.rect(104., 205., 272., 44., rgb(25, 15, 18));
        c.text(
            240.,
            235.,
            22.,
            match m.quality {
                Quality::Stale => "IAS DATA STALE",
                Quality::Failed => "IAS DATA FAILED",
                Quality::TimingFault => "IAS TIMING FAULT",
                _ => "IAS DATA MISSING",
            },
            RED,
        );
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    fn r(v: f32, age: u64) -> Reading {
        Reading {
            value: Some(v),
            quality: Quality::Valid,
            age_ms: Some(age),
        }
    }
    #[test]
    fn phone_scale_matches_reference_and_is_monotonic_without_wrapping() {
        for (kt, expected) in [
            (0., 0.),
            (40., 60.),
            (85., 171.25),
            (120., 242.),
            (129., 256.4),
            (163., 302.45),
            (200., 342.),
        ] {
            assert!((angle_at(kt).unwrap() - expected).abs() < 0.001);
        }
        let mut previous = -1.;
        for n in 0..=800 {
            let a = angle_at(n as f32 / 4.).unwrap();
            assert!(a > previous);
            previous = a;
        }
        for v in [-1., 201., f32::NAN, f32::INFINITY] {
            assert_eq!(angle_at(v), None);
        }
    }
    #[test]
    fn zero_is_real_data_and_invalid_stale_or_over_range_is_never_a_false_pointer() {
        assert_eq!(model(r(0., 0), 500).needle_deg, Some(0.));
        assert!(model(r(120., 499), 500).needle_deg.is_some());
        assert_eq!(model(r(120., 500), 500).quality, Quality::Stale);
        let over = model(r(350., 0), 500);
        assert!(over.over_range && over.ias_kt == Some(350.) && over.needle_deg.is_none());
        for quality in [
            Quality::Missing,
            Quality::Stale,
            Quality::Failed,
            Quality::TimingFault,
        ] {
            let m = model(
                Reading {
                    quality,
                    ..r(120., 0)
                },
                500,
            );
            assert!(m.ias_kt.is_none() && m.needle_deg.is_none());
            assert_eq!(m.quality, quality);
        }
        for v in [-1., 10001., f32::NAN, f32::INFINITY] {
            assert_eq!(model(r(v, 0), 500).quality, Quality::Failed);
        }
        assert_eq!(
            model(
                Reading {
                    age_ms: None,
                    ..r(120., 0)
                },
                500
            )
            .quality,
            Quality::Failed
        );
    }
    #[test]
    fn profiles_validate_geometry_and_limits_separately_from_measurements() {
        assert!(Profile::new(&SCALE, Some(DEMO_LIMITS)).is_some());
        for bad in [
            &[(0., 0.), (20., 30.), (10., 50.)][..],
            &[(0., 0.), (200., 360.)][..],
            &[(1., 0.), (200., 350.)][..],
        ] {
            assert!(Profile::new(bad, None).is_none());
        }
        assert!(Profile::new(
            &SCALE,
            Some(Limits {
                red: 160.,
                ..DEMO_LIMITS
            })
        )
        .is_none());
        assert!(Profile::new(
            &SCALE,
            Some(Limits {
                white: [f32::NAN, 85.],
                ..DEMO_LIMITS
            })
        )
        .is_none());
        let points = [(0., 0.), (200., 150.), (400., 320.)];
        let custom = Profile::new(&points, None).unwrap();
        assert_eq!(
            model_with_profile(r(300., 0), 500, custom).needle_deg,
            Some(235.)
        );
        assert!(custom.limits().is_none());
    }
    #[test]
    fn rotating_buffers_restore_motion_faults_menu_and_source_specific_arcs() {
        use crate::{
            damage::FrameHistory,
            navigation::{Button, Context, View},
            render::{draw_frame, CachedRenderer},
            Snapshot,
        };
        use instrument_core::{App, Command, Instrument};
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.tick(0);
        app.apply(Command::Select(Instrument::Airspeed)).unwrap();
        for size in [240, 480] {
            let mut background = std::vec![0;size*size];
            let mut renderer =
                CachedRenderer::new(&mut background, size as u32, size as u32).unwrap();
            let mut buffers = core::array::from_fn::<_, 3, _>(|_| std::vec![0;size*size]);
            let mut histories = core::array::from_fn::<_, 3, _>(|_| FrameHistory::new());
            let mut expected = std::vec![0;size*size];
            for n in 0..30 {
                let mut s = Snapshot::from_app(&app);
                s.ias = r(
                    if n % 5 == 0 {
                        350.
                    } else {
                        (n * 17 % 201) as f32
                    },
                    0,
                );
                s.ias.quality = match n % 7 {
                    0 => Quality::Missing,
                    1 => Quality::Failed,
                    2 => Quality::Stale,
                    _ => Quality::Valid,
                };
                s.external = n >= 15;
                s.external_attitude = s.external;
                s.source_label = if s.external {
                    "MSFS INPUT"
                } else {
                    "SIM INPUT"
                };
                s.center_pressed = n % 4 == 0;
                if n % 9 < 3 {
                    s.menu = Some(View {
                        generation: 1,
                        context: Context {
                            selected: Instrument::Airspeed,
                            external: s.external,
                            external_attitude: s.external,
                            imu: false,
                            session: 0,
                        },
                        pressed: (n % 9 == 1).then_some(Button::Cancel),
                    });
                }
                draw_frame(&mut expected, size as u32, size as u32, s, false);
                for b in 0..3 {
                    assert!(renderer.draw_incremental(
                        &mut buffers[b],
                        &mut histories[b],
                        s,
                        false
                    ));
                    assert_eq!(buffers[b], expected, "size {size}, step {n}, buffer {b}");
                }
            }
            let mut s = Snapshot::from_app(&app);
            s.external = true;
            s.external_attitude = true;
            renderer.draw_incremental(&mut buffers[0], &mut histories[0], s, false);
            assert!(
                !buffers[0]
                    .iter()
                    .any(|&p| p == rgb(60, 197, 107) || p == rgb(244, 216, 77)),
                "unverified live limits must not remain in reused buffers"
            );
        }
    }
}
