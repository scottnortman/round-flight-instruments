//! Shared bounded primitives for scalar instrument faces.
use crate::raster::{rgb, Canvas, Target};
use core::fmt::Write;
use instrument_core::simulator_link::measurements::Quantity;
use instrument_core::{Quality, Reading};
pub(crate) const FACE: u16 = rgb(8, 11, 14);
pub(crate) const MARK: u16 = rgb(238, 240, 230);
pub(crate) const RED: u16 = rgb(255, 103, 103);
pub(crate) fn validate(r: Reading, q: Quantity, ttl: u64) -> (Option<f32>, Quality) {
    let mut quality = r.quality;
    if quality == Quality::Valid {
        if r.value.is_none_or(|v| !q.accepts(v as f64)) || r.age_ms.is_none() {
            quality = Quality::Failed;
        } else if r.age_ms.is_some_and(|age| age >= ttl) {
            quality = Quality::Stale;
        }
    }
    (
        (quality == Quality::Valid).then_some(r.value).flatten(),
        quality,
    )
}
pub(crate) fn point(r: f32, deg: f32) -> (f32, f32) {
    let a = deg * core::f32::consts::PI / 180.;
    (240. + r * libm::sinf(a), 240. - r * libm::cosf(a))
}
pub(crate) fn needle<T: Target>(c: &mut Canvas<T>, angle: f32) {
    let a = angle * core::f32::consts::PI / 180.;
    let (s, co) = (libm::sinf(a), libm::cosf(a));
    let points = [
        (237., 277.),
        (237., 95.),
        (240., 53.),
        (243., 95.),
        (243., 277.),
        (240., 283.),
    ]
    .map(|(x, y)| {
        (
            240. + (x - 240.) * co - (y - 240.) * s,
            240. + (x - 240.) * s + (y - 240.) * co,
        )
    });
    c.polygon(&points, MARK);
    for i in 0..points.len() {
        c.stroke(
            points[i],
            points[(i + 1) % points.len()],
            1.,
            rgb(255, 128, 0),
        );
    }
    c.circle(240., 240., 15., rgb(23, 32, 40));
    c.circle(240., 240., 5., rgb(72, 84, 94));
}
pub(crate) fn invalid<T: Target>(c: &mut Canvas<T>, name: &str, quality: Quality) {
    c.stroke((132., 174.), (348., 282.), 7., RED);
    c.stroke((348., 174.), (132., 282.), 7., RED);
    c.rect(83., 205., 314., 44., rgb(25, 15, 18));
    let mut text = crate::render::Text::new();
    let suffix = match quality {
        Quality::Stale => "STALE",
        Quality::Failed => "FAILED",
        Quality::TimingFault => "TIMING FAULT",
        _ => "MISSING",
    };
    let _ = write!(text, "{name} DATA {suffix}");
    c.text(240., 235., 20., text.str(), RED);
}
