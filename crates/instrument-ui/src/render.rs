use crate::{
    raster::{rgb, Canvas, Target},
    Snapshot,
};
use core::fmt::{self, Write};
use instrument_core::Instrument;
const WHITE: u16 = rgb(238, 240, 230);
const WARN: u16 = rgb(255, 191, 70);
const RED: u16 = rgb(255, 85, 85);
const HAND_EDGE: u16 = rgb(255, 128, 0);
const ALTITUDE_PANEL: u16 = rgb(190, 190, 190);
// Logical 480-unit face geometry; Canvas scales these for each display size.
const DIAL_NUMBER_RADIUS: f32 = 169.;
const KOLLSMAN_TOP: f32 = 211.;
const KOLLSMAN_PRESSED: u16 = rgb(0, 90, 156);
/// Optional caller-owned backdrop cache. No allocation or hardware dependence;
/// every output frame is restored completely before drawing dynamic content.
pub struct CachedRenderer<'a> {
    background: &'a mut [u16],
    width: u32,
    height: u32,
    key: Option<(Instrument, bool)>,
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
        self.draw_inner(pixels, None, snapshot, pattern)
    }
    /// History belongs exclusively to this pixel buffer. A new history forces
    /// complete initialization; layout/view changes force full restore.
    pub fn draw_incremental(
        &mut self,
        pixels: &mut [u16],
        history: &mut crate::damage::FrameHistory,
        snapshot: Snapshot,
        pattern: bool,
    ) -> bool {
        self.draw_inner(pixels, Some(history), snapshot, pattern)
    }
    fn draw_inner(
        &mut self,
        pixels: &mut [u16],
        history: Option<&mut crate::damage::FrameHistory>,
        snapshot: Snapshot,
        pattern: bool,
    ) -> bool {
        if pixels.len() != self.background.len() {
            return false;
        }
        let key = (snapshot.selected, pattern);
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
        if let Some(history) = history {
            let frame_key = (self.width, self.height, snapshot.selected, pattern);
            if history.key == Some(frame_key) {
                history.restore(
                    pixels,
                    self.background,
                    self.width as usize,
                    self.height as usize,
                );
            } else {
                pixels.copy_from_slice(self.background);
                history.clear();
                history.key = Some(frame_key);
            }
            if !pattern {
                let Some(fb) = crate::raster::Framebuffer::new(pixels, self.width, self.height)
                else {
                    return false;
                };
                draw_overlay(
                    &mut crate::damage::Tracked {
                        target: fb,
                        history,
                    },
                    snapshot,
                );
            }
            return true;
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
fn hand<T: Target>(
    c: &mut Canvas<T>,
    points: &[(f32, f32)],
    angle: f32,
    color: u16,
    edge: Option<u16>,
) {
    let a = angle * core::f32::consts::PI / 180.;
    let (s, co) = (libm::sinf(a), libm::cosf(a));
    let mut out = [(0., 0.); 8];
    for (i, &(x, y)) in points.iter().enumerate() {
        let (x, y) = (x - 240., y - 240.);
        out[i] = (240. + x * co - y * s, 240. + x * s + y * co);
    }
    c.polygon(&out[..points.len()], color);
    if let Some(edge) = edge {
        for i in 0..points.len() {
            c.stroke(out[i], out[(i + 1) % points.len()], 2., edge);
        }
    }
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
        let (x, y) = point(DIAL_NUMBER_RADIUS, i as f32 * 36.);
        let mut n = Text::new();
        let _ = write!(n, "{i}");
        c.text(x, y, 46., n.str(), WHITE);
    }
    c.text(240., 142., 25., "ALT", WHITE);
    c.text(240., 171., 18., "100 FEET", rgb(175, 185, 192));
    // Widen toward the hub for larger digits, reserving the gap before the
    // right-hand tick marks for the external units label.
    c.rect(270., KOLLSMAN_TOP, 136., 42., rgb(54, 63, 69));
    c.text_bold_on(428., KOLLSMAN_TOP + 24., 22., "HG", (WHITE, rgb(9, 14, 19)));
}
fn draw_kollsman_value<T: Target>(c: &mut Canvas<T>, baro: f32, background: u16) {
    c.rect(273., KOLLSMAN_TOP + 3., 130., 36., background);
    let mut b = Text::new();
    if baro.is_finite() && (28.1..=31.).contains(&baro) {
        let _ = write!(b, "{baro:.2}");
    } else {
        let _ = b.write_str("--.--");
    }
    c.text_on(338., KOLLSMAN_TOP + 24., 32., b.str(), (WHITE, background));
}
fn draw_overlay<T: Target>(target: &mut T, s: Snapshot) {
    if s.selected != Instrument::Altimeter {
        return;
    }
    let mut c = Canvas::new(target);
    // Barometer digits belong to dynamic damage tracking, not the cached face.
    draw_kollsman_value(
        &mut c,
        s.baro_inhg,
        if s.kollsman_pressed && s.editor.is_none() && s.model().hands_deg.is_some() {
            KOLLSMAN_PRESSED
        } else {
            0
        },
    );
    let m = s.model();
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
            None,
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
            Some(HAND_EDGE),
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
            None,
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
            Some(HAND_EDGE),
        );
        c.circle(240., 240., 17., rgb(85, 98, 110));
        c.circle(240., 240., 15., rgb(16, 23, 29));
        c.circle(240., 240., 5., rgb(60, 72, 81));
        let mut a = Text::new();
        let _ = write!(a, "{:.0} FT", m.indicated_ft.unwrap_or(0.));
        // Eight cells cover the full supported signed/baro-corrected range.
        // Keep padding around the larger readout and clearance below Kollsman.
        c.rect(124., 284., 232., 40., ALTITUDE_PANEL);
        c.text_bold_on(240., 307., 32., a.str(), (0, ALTITUDE_PANEL));
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
    c.text(240., 351., 18., s.source_label, WARN);
    if m.indicated_ft.is_some() {
        if let Some(editor) = s.editor {
            draw_editor(&mut c, editor, s.external);
        }
    }
}

fn draw_editor<T: Target>(
    c: &mut Canvas<T>,
    editor: crate::interaction::EditorView,
    external: bool,
) {
    use crate::interaction::Button;
    let panel = rgb(22, 29, 36);
    c.rect(80., 128., 320., 240., rgb(145, 155, 165));
    c.rect(83., 131., 314., 234., panel);
    c.text(240., 150., 21., "BARO IN HG", WHITE);
    let mut value = Text::new();
    let _ = write!(value, "{}.{:02}", editor.cents / 100, editor.cents % 100);
    c.text(240., 189., 38., value.str(), WHITE);
    for (button, label, size) in [
        (Button::Minus, "-", 36.),
        (Button::Standard, "29.92", 24.),
        (Button::Plus, "+", 36.),
        (Button::Cancel, "CANCEL", 19.),
        (Button::Apply, "APPLY", 19.),
    ] {
        let r = button.rect();
        let background = if editor.pressed == Some(button) {
            KOLLSMAN_PRESSED
        } else {
            rgb(66, 80, 92)
        };
        c.rect(r.x as f32, r.y as f32, r.w as f32, r.h as f32, background);
        c.text_on(
            r.x as f32 + r.w as f32 / 2.,
            r.y as f32 + r.h as f32 / 2.,
            size,
            label,
            (WHITE, background),
        );
    }
    c.text(
        240.,
        354.,
        14.,
        if external {
            "APPLY TO SIM"
        } else {
            "SIM INPUT"
        },
        WARN,
    );
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
    fn changing_baro_does_not_rebuild_the_backdrop() {
        let mut app = instrument_core::App::new();
        app.apply(instrument_core::Command::LevelPreset).unwrap();
        app.tick(0);
        let mut bg = std::vec![0; 480 * 480];
        let mut pixels = bg.clone();
        let mut renderer = CachedRenderer::new(&mut bg, 480, 480).unwrap();
        renderer.draw(&mut pixels, Snapshot::from_app(&app), false);
        let key = renderer.key;
        app.apply(instrument_core::Command::Baro(30.12)).unwrap();
        renderer.draw(&mut pixels, Snapshot::from_app(&app), false);
        assert_eq!(
            renderer.key, key,
            "barometer change invalidates full background"
        );
    }
    #[test]
    fn cached_frames_match_full_render_across_motion_faults_and_settings() {
        for size in [240, 480, 720] {
            let mut background = std::vec![0u16; size * size];
            let mut cached =
                CachedRenderer::new(&mut background, size as u32, size as u32).unwrap();
            let mut output = std::vec![0xffff; size * size];
            let mut reference = output.clone();
            let mut snapshot = Snapshot {
                external: false,
                source_session: 0,
                source_sequence: 0,
                source_received_ms: 0,
                baro_age_ms: None,
                source_label: "SIM INPUT",
                kollsman_pressed: false,
                selected: Instrument::Altimeter,
                altitude: Reading {
                    value: Some(0.),
                    quality: Quality::Valid,
                    age_ms: Some(0),
                },
                editor: None,
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
    fn incremental_triple_buffers_match_full_redraw_through_motion_and_failures() {
        for (w, h) in [(240, 240), (480, 480), (720, 720), (241, 239), (1024, 64)] {
            let len = w * h;
            let mut background = std::vec![0; len];
            let mut cached = CachedRenderer::new(&mut background, w as u32, h as u32).unwrap();
            let mut frames: [std::vec::Vec<u16>; 3] =
                core::array::from_fn(|_| std::vec![0xaaaa; len]);
            let mut histories =
                core::array::from_fn::<_, 3, _>(|_| crate::damage::FrameHistory::new());
            let mut reference = std::vec![0; len];
            for n in 0..60 {
                let snapshot = Snapshot {
                    external: false,
                    source_session: 0,
                    source_sequence: 0,
                    source_received_ms: 0,
                    baro_age_ms: None,
                    source_label: "SIM INPUT",
                    kollsman_pressed: n % 5 < 2,
                    selected: if n % 19 < 3 {
                        Instrument::Attitude
                    } else {
                        Instrument::Altimeter
                    },
                    altitude: Reading {
                        value: Some(n as f32 * 1137. - 100.),
                        quality: [
                            Quality::Valid,
                            Quality::Valid,
                            Quality::Valid,
                            Quality::Missing,
                            Quality::Stale,
                            Quality::Failed,
                            Quality::TimingFault,
                        ][(n / 3) % 7],
                        age_ms: Some(10),
                    },
                    editor: if n % 11 < 6 {
                        Some(crate::interaction::EditorView {
                            cents: 2810 + n as u16,
                            pressed: if n % 2 == 0 {
                                Some(crate::interaction::Button::Plus)
                            } else {
                                None
                            },
                        })
                    } else {
                        None
                    },
                    baro_inhg: if n % 23 < 5 { 30.92 } else { 29.92 },
                };
                let pattern = n % 17 < 4;
                let slot = n % 3;
                assert!(cached.draw_incremental(
                    &mut frames[slot],
                    &mut histories[slot],
                    snapshot,
                    pattern
                ));
                assert!(draw_frame(
                    &mut reference,
                    w as u32,
                    h as u32,
                    snapshot,
                    pattern
                ));
                assert_eq!(
                    frames[slot], reference,
                    "incremental mismatch {w}x{h} frame {n}"
                );
            }
        }
    }
    #[test]
    fn press_feedback_is_local_and_restores_all_three_buffers() {
        let mut app = instrument_core::App::new();
        app.apply(instrument_core::Command::LevelPreset).unwrap();
        app.tick(0);
        let mut snapshot = Snapshot::from_app(&app);
        let mut normal = std::vec![0; 480 * 480];
        assert!(draw_frame(&mut normal, 480, 480, snapshot, false));
        let mut backdrop = std::vec![0; normal.len()];
        let mut cached = CachedRenderer::new(&mut backdrop, 480, 480).unwrap();
        let mut frames = core::array::from_fn::<_, 3, _>(|_| normal.clone());
        let mut histories = core::array::from_fn::<_, 3, _>(|_| crate::damage::FrameHistory::new());
        for pressed in [false, true, false, true, true, false] {
            snapshot.kollsman_pressed = pressed;
            for (frame, history) in frames.iter_mut().zip(&mut histories) {
                assert!(cached.draw_incremental(frame, history, snapshot, false));
                if !pressed {
                    assert_eq!(frame, &normal);
                } else {
                    assert!(frame.iter().filter(|&&p| p == KOLLSMAN_PRESSED).count() > 1000);
                    for (i, (&actual, &original)) in frame.iter().zip(&normal).enumerate() {
                        if !crate::interaction::KOLLSMAN
                            .contains((i % 480) as u16, (i / 480) as u16)
                        {
                            assert_eq!(actual, original, "press changed pixels outside window");
                        }
                    }
                }
            }
        }
        snapshot.altitude.quality = Quality::Failed;
        snapshot.kollsman_pressed = false;
        assert!(draw_frame(&mut normal, 480, 480, snapshot, false));
        snapshot.kollsman_pressed = true;
        assert!(cached.draw_incremental(&mut frames[0], &mut histories[0], snapshot, false));
        assert_eq!(
            frames[0], normal,
            "press feedback must not cover invalid-data warnings"
        );
    }
    #[test]
    fn every_editor_button_highlight_restores_all_buffers() {
        use crate::interaction::{Button, EditorView};
        let mut app = instrument_core::App::new();
        app.apply(instrument_core::Command::LevelPreset).unwrap();
        app.tick(0);
        let mut snapshot = Snapshot::from_app(&app);
        snapshot.editor = Some(EditorView {
            cents: 2992,
            pressed: None,
        });
        let mut normal = std::vec![0; 480 * 480];
        assert!(draw_frame(&mut normal, 480, 480, snapshot, false));
        let mut backdrop = normal.clone();
        let mut cached = CachedRenderer::new(&mut backdrop, 480, 480).unwrap();
        let mut frames = core::array::from_fn::<_, 3, _>(|_| normal.clone());
        let mut histories = core::array::from_fn::<_, 3, _>(|_| crate::damage::FrameHistory::new());
        for button in [
            Button::Minus,
            Button::Plus,
            Button::Standard,
            Button::Cancel,
            Button::Apply,
        ] {
            for pressed in [Some(button), None] {
                snapshot.editor.as_mut().unwrap().pressed = pressed;
                for (frame, history) in frames.iter_mut().zip(&mut histories) {
                    assert!(cached.draw_incremental(frame, history, snapshot, false));
                    let r = button.rect();
                    if pressed.is_some() {
                        assert_eq!(
                            frame[(r.y as usize + 2) * 480 + r.x as usize + 2],
                            KOLLSMAN_PRESSED
                        );
                        for (i, (&actual, &original)) in frame.iter().zip(&normal).enumerate() {
                            if !r.contains((i % 480) as u16, (i / 480) as u16) {
                                assert_eq!(actual, original);
                            }
                        }
                    } else {
                        assert_eq!(frame, &normal);
                    }
                }
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
    fn external_lifecycle_labels_and_barometer_restore_all_buffers() {
        let mut app = instrument_core::App::new();
        app.tick(0);
        app.apply(instrument_core::Command::LevelPreset).unwrap();
        app.tick(20);
        let mut s = Snapshot::from_app(&app);
        let mut bg = std::vec![0; 480 * 480];
        let mut buffers = [
            std::vec![0; 480 * 480],
            std::vec![0; 480 * 480],
            std::vec![0; 480 * 480],
        ];
        let mut histories = core::array::from_fn::<_, 3, _>(|_| crate::damage::FrameHistory::new());
        let mut renderer = CachedRenderer::new(&mut bg, 480, 480).unwrap();
        let mut expected = std::vec![0; 480 * 480];
        for (external, label, baro, quality) in [
            (false, "SIM INPUT", 29.92, Quality::Valid),
            (true, "TEST INPUT", 30.12, Quality::Valid),
            (true, "REPLAY", 30.12, Quality::Valid),
            (true, "PAUSED", f32::NAN, Quality::Missing),
            (true, "LINK STALE", f32::NAN, Quality::Stale),
            (true, "NO LINK", f32::NAN, Quality::Missing),
            (false, "SIM INPUT", 29.92, Quality::Valid),
        ] {
            s.external = external;
            s.source_label = label;
            s.baro_inhg = baro;
            s.altitude.quality = quality;
            assert!(draw_frame(&mut expected, 480, 480, s, false));
            for i in 0..3 {
                assert!(renderer.draw_incremental(&mut buffers[i], &mut histories[i], s, false));
                assert_eq!(buffers[i], expected, "source={label}, buffer={i}");
            }
        }
    }
    #[test]
    fn invalid_frames_remove_pointer_at_all_supported_test_sizes() {
        for size in [240, 480, 720] {
            let mut pixels = std::vec![0u16;size*size];
            let valid = Snapshot {
                external: false,
                source_session: 0,
                source_sequence: 0,
                source_received_ms: 0,
                baro_age_ms: None,
                source_label: "SIM INPUT",
                kollsman_pressed: false,
                selected: Instrument::Altimeter,
                altitude: Reading {
                    quality: Quality::Valid,
                    value: Some(0.),
                    age_ms: Some(0),
                },
                editor: None,
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
