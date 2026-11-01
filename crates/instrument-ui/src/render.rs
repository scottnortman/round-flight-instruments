use crate::{
    altimeter,
    raster::{rgb, Canvas, Target},
    Snapshot,
};
use core::fmt::{self, Write};
use instrument_core::Instrument;
const WHITE: u16 = rgb(238, 240, 230);
const WARN: u16 = rgb(255, 191, 70);
const RED: u16 = rgb(255, 85, 85);
/// Optional caller-owned backdrop cache. No allocation or hardware dependence;
/// every output frame is restored completely before drawing dynamic content.
pub struct CachedRenderer<'a> {
    background: &'a mut [u16],
    width: u32,
    height: u32,
    key: Option<(Instrument, u32, bool)>,
}
impl<'a> CachedRenderer<'a> {
    pub fn new(background: &'a mut [u16], width: u32, height: u32) -> Option<Self> {
        crate::raster::Framebuffer::new(background, width, height)?;
        Some(Self {
            background,
            width,
            height,
            key: None,
        })
    }
    pub fn draw(&mut self, pixels: &mut [u16], snapshot: Snapshot, pattern: bool) -> bool {
        if pixels.len() != self.background.len() {
            return false;
        }
        let key = (snapshot.selected, snapshot.baro_inhg.to_bits(), pattern);
        if self.key != Some(key) {
            let Some(mut fb) =
                crate::raster::Framebuffer::new(self.background, self.width, self.height)
            else {
                return false;
            };
            if pattern {
                test_pattern(&mut fb);
            } else {
                draw_background(&mut fb, snapshot);
            }
            self.key = Some(key);
        }
        pixels.copy_from_slice(self.background);
        if !pattern {
            let Some(mut fb) = crate::raster::Framebuffer::new(pixels, self.width, self.height)
            else {
                return false;
            };
            draw_overlay(&mut fb, snapshot);
        }
        true
    }
}
/// Concrete entry point keeps raster monomorphization inside the UI crate,
/// allowing platforms to optimize it separately from control and FFI code.
pub fn draw_frame(
    pixels: &mut [u16],
    width: u32,
    height: u32,
    snapshot: Snapshot,
    pattern: bool,
) -> bool {
    let Some(mut fb) = crate::raster::Framebuffer::new(pixels, width, height) else {
        return false;
    };
    if pattern {
        test_pattern(&mut fb);
    } else {
        draw(&mut fb, snapshot);
    }
    true
}
struct Text {
    bytes: [u8; 48],
    len: usize,
}
impl Text {
    fn new() -> Self {
        Self {
            bytes: [0; 48],
            len: 0,
        }
    }
    fn str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("TEXT ERROR")
    }
}
impl Write for Text {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.len + s.len() > 48 {
            return Err(fmt::Error);
        }
        self.bytes[self.len..self.len + s.len()].copy_from_slice(s.as_bytes());
        self.len += s.len();
        Ok(())
    }
}
fn point(radius: f32, deg: f32) -> (f32, f32) {
    let a = deg * core::f32::consts::PI / 180.;
    (240. + radius * libm::sinf(a), 240. - radius * libm::cosf(a))
}
fn hand<T: Target>(c: &mut Canvas<T>, points: &[(f32, f32)], angle: f32, color: u16) {
    let a = angle * core::f32::consts::PI / 180.;
    let (s, co) = (libm::sinf(a), libm::cosf(a));
    let mut out = [(0., 0.); 8];
    for (i, &(x, y)) in points.iter().enumerate() {
        let (x, y) = (x - 240., y - 240.);
        out[i] = (240. + x * co - y * s, 240. + x * s + y * co);
    }
    c.polygon(&out[..points.len()], color);
}
pub fn draw<T: Target>(target: &mut T, s: Snapshot) {
    draw_background(target, s);
    draw_overlay(target, s);
}
fn draw_background<T: Target>(target: &mut T, s: Snapshot) {
    let (w, h) = target.size();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            target.pixel(x, y, 0);
        }
    }
    let mut c = Canvas::new(target);
    c.circle(240., 240., 238., rgb(70, 82, 93));
    c.circle(240., 240., 229., rgb(9, 14, 19));
    if s.selected != Instrument::Altimeter {
        c.text(240., 180., 25., "SIMULATION", WARN);
        let mut name = Text::new();
        for ch in s.selected.name().bytes() {
            let _ = name.write_char(ch.to_ascii_uppercase() as char);
        }
        c.text(240., 230., 28., name.str(), WHITE);
        c.text(240., 280., 18., "NOT IMPLEMENTED", RED);
        return;
    }
    for i in 0..50 {
        let major = i % 5 == 0;
        // esp rustc 1.97.0's -Oz backend cannot select the float lookup table
        // synthesized for these choices. Keep the exact integer dimensions
        // opaque until conversion, avoiding that compiler crash without changing
        // geometry or weakening checks. Covered by host renders and board build.
        let radius = core::hint::black_box(if major { 199u32 } else { 210u32 }) as f32;
        let width = core::hint::black_box(if major { 4u32 } else { 2u32 }) as f32;
        c.line(
            point(radius, i as f32 * 7.2),
            point(221., i as f32 * 7.2),
            width,
            WHITE,
        );
    }
    for i in 0..10 {
        let (x, y) = point(179., i as f32 * 36.);
        let mut n = Text::new();
        let _ = write!(n, "{i}");
        c.text(x, y, 46., n.str(), WHITE);
    }
    c.text(240., 142., 25., "ALT", WHITE);
    c.text(240., 171., 18., "100 FEET", rgb(175, 185, 192));
    c.rect(311., 217., 95., 64., rgb(54, 63, 69));
    c.rect(314., 220., 89., 58., 0);
    c.text(358., 232., 12., "IN HG", WHITE);
    let mut b = Text::new();
    if s.baro_inhg.is_finite() && (28.1..=31.).contains(&s.baro_inhg) {
        let _ = write!(b, "{:.2}", s.baro_inhg);
    } else {
        let _ = b.write_str("--.--");
    }
    c.text(358., 257., 20., b.str(), WHITE);
}
fn draw_overlay<T: Target>(target: &mut T, s: Snapshot) {
    if s.selected != Instrument::Altimeter {
        return;
    }
    let mut c = Canvas::new(target);
    let m = altimeter::model(s.altitude, s.baro_inhg);
    if let Some([hundreds, thousands, tenthousands]) = m.hands_deg {
        c.line(
            point(-4., tenthousands),
            point(185., tenthousands),
            2.,
            rgb(195, 203, 208),
        );
        hand(
            &mut c,
            &[(231., 41.), (249., 41.), (240., 57.)],
            tenthousands,
            WHITE,
        );
        hand(
            &mut c,
            &[
                (232., 255.),
                (229., 162.),
                (240., 133.),
                (251., 162.),
                (248., 255.),
            ],
            thousands,
            rgb(226, 231, 230),
        );
        hand(
            &mut c,
            &[
                (237., 230.),
                (237., 167.),
                (240., 152.),
                (243., 167.),
                (243., 230.),
            ],
            thousands,
            rgb(137, 149, 158),
        );
        hand(
            &mut c,
            &[
                (235., 263.),
                (236., 105.),
                (240., 76.),
                (244., 105.),
                (245., 263.),
            ],
            hundreds,
            WHITE,
        );
        c.circle(240., 240., 17., rgb(85, 98, 110));
        c.circle(240., 240., 15., rgb(16, 23, 29));
        c.circle(240., 240., 5., rgb(60, 72, 81));
        let mut a = Text::new();
        let _ = write!(a, "{:.0} FT", m.indicated_ft.unwrap_or(0.));
        c.rect(175., 292., 130., 28., rgb(9, 14, 19));
        c.text(240., 306., 20., a.str(), WHITE);
    } else {
        c.line((155., 185.), (296., 285.), 7., RED);
        c.line((296., 185.), (155., 285.), 7., RED);
        c.rect(124., 216., 194., 50., rgb(25, 15, 18));
        c.text(221., 240., 24., "ALT DATA", RED);
        let mut q = Text::new();
        for ch in m.quality.name().bytes() {
            let _ = q.write_char(ch.to_ascii_uppercase() as char);
        }
        c.text(240., 306., 19., q.str(), RED);
    }
    c.text(240., 351., 18., "SIM INPUT", WARN);
}

pub fn test_pattern<T: Target>(target: &mut T) {
    let (w, h) = target.size();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            target.pixel(x, y, 0);
        }
    }
    let mut c = Canvas::new(target);
    c.circle(240., 240., 228., rgb(35, 35, 35));
    c.rect(120., 185., 80., 100., rgb(255, 0, 0));
    c.rect(200., 185., 80., 100., rgb(0, 255, 0));
    c.rect(280., 185., 80., 100., rgb(0, 0, 255));
    c.text(240., 100., 28., "TOP", WHITE);
    c.text(240., 365., 22., "RGB TEST", WHITE);
    c.line((240., 20.), (240., 65.), 4., WHITE);
    c.line((20., 240.), (65., 240.), 4., WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::Framebuffer;
    use instrument_core::{Quality, Reading};
    extern crate std;
    #[test]
    fn cached_frames_match_full_render_across_motion_faults_and_settings() {
        for size in [240, 480, 720] {
            let mut background = std::vec![0u16; size * size];
            let mut cached =
                CachedRenderer::new(&mut background, size as u32, size as u32).unwrap();
            let mut output = std::vec![0xffff; size * size];
            let mut reference = output.clone();
            let mut snapshot = Snapshot {
                selected: Instrument::Altimeter,
                altitude: Reading {
                    value: Some(0.),
                    quality: Quality::Valid,
                    age_ms: Some(0),
                },
                baro_inhg: 29.92,
            };
            for (altitude, quality, baro, instrument, pattern) in [
                (0., Quality::Valid, 29.92, Instrument::Altimeter, false),
                (4520., Quality::Valid, 29.92, Instrument::Altimeter, false),
                (8500., Quality::Valid, 30.12, Instrument::Altimeter, false),
                (10000., Quality::Valid, 29.92, Instrument::Altimeter, false),
                (10000., Quality::Stale, 29.92, Instrument::Altimeter, false),
                (0., Quality::Missing, 29.92, Instrument::Altimeter, false),
                (0., Quality::Failed, 29.92, Instrument::Altimeter, false),
                (
                    0.,
                    Quality::TimingFault,
                    29.92,
                    Instrument::Altimeter,
                    false,
                ),
                (-100., Quality::Valid, 29.92, Instrument::Altimeter, false),
                (0., Quality::Valid, 29.92, Instrument::Attitude, false),
                (0., Quality::Valid, 29.92, Instrument::Altimeter, true),
                (1234., Quality::Valid, 29.92, Instrument::Altimeter, false),
                (
                    1234.,
                    Quality::Valid,
                    f32::NAN,
                    Instrument::Altimeter,
                    false,
                ),
            ] {
                snapshot.altitude.value = Some(altitude);
                snapshot.altitude.quality = quality;
                snapshot.baro_inhg = baro;
                snapshot.selected = instrument;
                assert!(cached.draw(&mut output, snapshot, pattern));
                assert!(draw_frame(
                    &mut reference,
                    size as u32,
                    size as u32,
                    snapshot,
                    pattern
                ));
                assert_eq!(
                    output, reference,
                    "cache mismatch at {size}: {snapshot:?}, pattern={pattern}"
                );
            }
        }
    }
    #[test]
    fn repeated_manual_setting_repaints_identically_over_previous_frames() {
        let mut app = instrument_core::App::new();
        app.tick(0);
        let mut pixels = std::vec![0xffff; 480 * 480];
        assert!(draw_frame(
            &mut pixels,
            480,
            480,
            Snapshot::from_app(&app),
            false
        ));
        app.apply(instrument_core::Command::Set(
            instrument_core::Field::PressureAltitudeFt,
            8500.,
        ))
        .unwrap();
        app.tick(20);
        assert!(draw_frame(
            &mut pixels,
            480,
            480,
            Snapshot::from_app(&app),
            false
        ));
        let first = pixels.clone();
        app.apply(instrument_core::Command::Set(
            instrument_core::Field::PressureAltitudeFt,
            8500.,
        ))
        .unwrap();
        app.tick(40);
        assert!(draw_frame(
            &mut pixels,
            480,
            480,
            Snapshot::from_app(&app),
            false
        ));
        assert_eq!(
            first, pixels,
            "identical commands must not corrupt a reused framebuffer"
        );
        let mut fresh = std::vec![0x5a5a; 480 * 480];
        assert!(draw_frame(
            &mut fresh,
            480,
            480,
            Snapshot::from_app(&app),
            false
        ));
        assert_eq!(
            pixels, fresh,
            "frame output must not depend on previous contents"
        );
    }
    #[test]
    fn invalid_frames_remove_pointer_at_all_supported_test_sizes() {
        for size in [240, 480, 720] {
            let mut pixels = std::vec![0u16;size*size];
            let valid = Snapshot {
                selected: Instrument::Altimeter,
                altitude: Reading {
                    quality: Quality::Valid,
                    value: Some(0.),
                    age_ms: Some(0),
                },
                baro_inhg: 29.92,
            };
            draw(
                &mut Framebuffer::new(&mut pixels, size as u32, size as u32).unwrap(),
                valid,
            );
            let index = (100 * size / 480) * size + 240 * size / 480;
            assert_eq!(
                pixels[index], WHITE,
                "zero is a valid twelve-o'clock pointer"
            );
            for quality in [
                Quality::Missing,
                Quality::Stale,
                Quality::Failed,
                Quality::TimingFault,
            ] {
                let mut invalid = valid;
                invalid.altitude.quality = quality;
                draw(
                    &mut Framebuffer::new(&mut pixels, size as u32, size as u32).unwrap(),
                    invalid,
                );
                assert_ne!(
                    pixels[index], WHITE,
                    "old pointer remained after invalidation"
                );
                assert_eq!(pixels[0], 0, "round viewport corner must remain black");
            }
        }
    }
}
