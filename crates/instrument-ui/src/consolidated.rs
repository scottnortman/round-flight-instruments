//! Combined round panel. Every component uses the same validated models and
//! original sample ages as its standalone face. No inferred navigation data.
use crate::{
    raster::{rgb, Canvas, Target},
    render::Text,
    Snapshot,
};
use core::fmt::Write;

const WHITE: u16 = rgb(238, 240, 230);
const GREEN: u16 = rgb(140, 224, 65);
const YELLOW: u16 = rgb(255, 224, 60);
const RED: u16 = rgb(255, 103, 103);
const FACE: u16 = rgb(8, 11, 14);
const PANEL: u16 = rgb(20, 25, 29);
const OUTLINE: u16 = rgb(57, 65, 70);

/// Exact physical horizon coverage, including the side/bottom panel clip.
pub(crate) fn opaque_span(w: u32, h: u32, y: usize) -> (usize, usize) {
    let scale = w.min(h) as f32 / 480.;
    let x = ((w as f32 - 480. * scale) / 2. + 102. * scale) as i32;
    let top = ((h as f32 - 480. * scale) / 2. + 82. * scale) as i32;
    let center = (276. * scale) as i32 / 2;
    let radius = (224. * ((276. * scale) as u32) as f32 / 480.) as i32;
    let dy = y as i32 - top - center;
    if (y as i32) < top || (y as i32) >= top + (231. * scale) as i32 || dy.abs() > radius {
        return (0, 0);
    }
    let dx = crate::attitude::isqrt((radius * radius - dy * dy) as u32) as i32;
    let lo = (x + center - dx)
        .max(x + (16. * scale) as i32)
        .clamp(0, w as i32) as usize;
    let hi = (x + center + dx + 1)
        .min(x + (260. * scale) as i32)
        .clamp(0, w as i32) as usize;
    if lo < hi {
        (lo, hi)
    } else {
        (0, 0)
    }
}

/// A physical subviewport forwards rows directly into the caller's framebuffer.
/// No additional bitmap or allocation is required for the smaller horizon.
struct Viewport<'a, T> {
    target: &'a mut T,
    x: i32,
    y: i32,
    side: u32,
    left: i32,
    right: i32,
    bottom: i32,
}
impl<T: Target> Target for Viewport<'_, T> {
    fn size(&self) -> (u32, u32) {
        (self.side, self.side)
    }
    fn pixel(&mut self, x: i32, y: i32, color: u16) {
        if (self.left..self.right).contains(&x) && (0..self.bottom).contains(&y) {
            self.target.pixel(self.x + x, self.y + y, color);
        }
    }
    fn pixels(&mut self, y: i32, start: i32, colors: &[Option<u16>]) {
        let lo = start.max(self.left);
        let hi = (start + colors.len() as i32).min(self.right);
        if (0..self.bottom).contains(&y) && lo < hi {
            self.target.pixels(
                self.y + y,
                self.x + lo,
                &colors[(lo - start) as usize..(hi - start) as usize],
            );
        }
    }
    fn span(&mut self, y: i32, start: i32, end: i32, color: u16) {
        let lo = start.clamp(self.left, self.right);
        let hi = end.clamp(self.left, self.right);
        if (0..self.bottom).contains(&y) && lo < hi {
            self.target
                .span(self.y + y, self.x + lo, self.x + hi, color);
        }
    }
}

pub fn draw<T: Target>(target: &mut T, s: Snapshot) {
    let (w, h) = target.size();
    let scale = w.min(h) as f32 / 480.;
    let mut viewport = Viewport {
        target,
        x: ((w as f32 - 480. * scale) / 2. + 102. * scale) as i32,
        y: ((h as f32 - 480. * scale) / 2. + 82. * scale) as i32,
        side: (276. * scale) as u32,
        left: (16. * scale) as i32,
        right: (260. * scale) as i32,
        bottom: (231. * scale) as i32,
    };
    crate::attitude::draw_combined(&mut viewport, s.attitude_model());
    let mut c = Canvas::new(target);
    c.polygon(&[(229., 86.), (251., 86.), (240., 101.)], YELLOW);
    c.text(240., 70., 12., s.source_label, WHITE);
    tape(&mut c, false, s.airspeed_model().ias_kt);
    tape(&mut c, true, s.model().indicated_ft);
    draw_turn(&mut c, s);
    draw_heading(&mut c, s.heading_model().magnetic_deg);
    draw_vsi(&mut c, s.vsi_model());
    let mut qnh = Text::new();
    if s.baro_inhg.is_finite() && (28.1..=31.).contains(&s.baro_inhg) {
        let _ = write!(qnh, "QNH {:.2} INHG", s.baro_inhg);
    } else {
        let _ = qnh.write_str("QNH --.-- INHG");
    }
    let r=crate::interaction::COMBINED_QNH;
    let background=if s.kollsman_pressed && s.editor.is_none() {rgb(0,90,156)}else{FACE};
    c.rect(r.x as f32,r.y as f32,r.w as f32,r.h as f32,OUTLINE);
    c.rect(r.x as f32+1.,r.y as f32+1.,r.w as f32-2.,r.h as f32-2.,background);
    c.text_on(r.x as f32+r.w as f32/2., r.y as f32+r.h as f32/2., 14., qnh.str(), (WHITE,background));
}

/// Fixed artwork lives in the existing backdrop, independently of sample data.
pub(crate) fn background<T: Target>(target: &mut T) {
    let mut c = Canvas::new(target);
    c.text(240., 49., 18., "FLIGHT PANEL", GREEN);
    for (x, center, label) in [(38., 78., "IAS KT"), (362., 402., "ALT FT")] {
        c.rect(x, 139., 80., 177., PANEL);
        c.text_on(center, 155., 12., label, (WHITE, PANEL));
    }
    for (x, label) in [(132., "TURN"), (240., "MAG HDG"), (348., "V/S")] {
        c.circle(x, 364., 51., OUTLINE);
        c.circle(x, 364., 49., FACE);
        c.text_on(x, 338., 11., label, (WHITE, FACE));
    }
    c.text_on(101., 360., 10., "L", (WHITE, FACE));
    c.text_on(163., 360., 10., "R", (WHITE, FACE));
    c.rect(107., 383., 50., 11., WHITE);
    c.rect(108., 384., 48., 9., PANEL);
    for x in [127., 137.] {
        c.stroke((x, 383.), (x, 394.), 1., WHITE);
    }
    c.stroke((315., 364.), (323., 364.), 1., WHITE);
    c.text_on(308., 365., 10., "0", (WHITE, FACE));
    c.stroke((373., 364.), (381., 364.), 1., WHITE);
    c.text_on(348., 405., 10., "FPM", (WHITE, FACE));
}

fn tape<T: Target>(c: &mut Canvas<T>, altitude: bool, value: Option<f32>) {
    let (x, center, edge, direction, step) = if altitude {
        (362., 402., 368., 1., 100.)
    } else {
        (38., 78., 112., -1., 10.)
    };
    if let Some(value) = value {
        let base = libm::floorf(value / step) as i32;
        for offset in -4..=4 {
            let n = base + offset;
            let y = 220. - (n as f32 * step - value) / step * 24.;
            if !(172.0..=306.).contains(&y) || (!altitude && n < 0) {
                continue;
            }
            c.stroke(
                (edge, y),
                (edge + direction * if n % 2 == 0 { 10. } else { 6. }, y),
                1.,
                WHITE,
            );
            if n % 2 == 0 {
                let mut text = Text::new();
                let _ = write!(text, "{:.0}", n as f32 * step);
                c.text_on(
                    center + direction * 4.,
                    y + 2.,
                    12.,
                    text.str(),
                    (WHITE, PANEL),
                );
            }
        }
    }
    // Opaque value box covers the scrolling scale at the fixed index.
    c.rect(
        x - 2.,
        204.,
        84.,
        33.,
        if value.is_some() { GREEN } else { RED },
    );
    c.rect(x - 1., 205., 82., 31., FACE);
    let mut text = Text::new();
    if let Some(value) = value {
        let _ = write!(text, "{value:.0}");
    } else {
        let _ = text.write_str("---");
    }
    c.text_on(
        center,
        225.,
        if text.str().len() > 4 { 18. } else { 22. },
        text.str(),
        (if value.is_some() { GREEN } else { RED }, FACE),
    );
}

fn draw_heading<T: Target>(c: &mut Canvas<T>, heading: Option<f32>) {
    let Some(heading) = heading else {
        c.text_on(240., 371., 18., "---", (RED, FACE));
        return;
    };
    let angle = (-heading).to_radians();
    let (hs, hc) = (libm::sinf(angle), libm::cosf(angle));
    const TICKS: [(f32, f32); 6] = [
        (0., 1.),
        (0.25881904, 0.9659258),
        (0.5, 0.8660254),
        (0.70710677, 0.70710677),
        (0.8660254, 0.5),
        (0.9659258, 0.25881904),
    ];
    for i in 0..24 {
        let (a, b) = TICKS[i % 6];
        let (a, b) = match i / 6 {
            0 => (a, b),
            1 => (b, -a),
            2 => (-a, -b),
            _ => (-b, a),
        };
        let (sin, cos) = (a * hc + b * hs, b * hc - a * hs);
        c.stroke(
            (240. + 39. * sin, 369. - 39. * cos),
            (240. + 44. * sin, 369. - 44. * cos),
            1.,
            WHITE,
        );
    }
    c.rect(205., 328., 70., 16., FACE);
    c.text_on(240., 338., 11., "MAG HDG", (WHITE, FACE));
    c.polygon(&[(236., 346.), (244., 346.), (240., 352.)], YELLOW);
    let mut text = Text::new();
    let _ = write!(text, "{:03.0}", libm::roundf(heading) % 360.);
    c.text_on(240., 376., 22., text.str(), (GREEN, FACE));
    c.rect(210., 389., 60., 15., FACE);
    c.text_on(240., 398., 10., "DEG MAG", (WHITE, FACE));
}
fn draw_turn<T: Target>(c: &mut Canvas<T>, s: Snapshot) {
    let turn = s.turn_model();
    if let Some(angle) = turn.aircraft_deg {
        let radians = angle.to_radians();
        c.stroke(
            (
                132. - 23. * libm::cosf(radians),
                363. - 23. * libm::sinf(radians),
            ),
            (
                132. + 23. * libm::cosf(radians),
                363. + 23. * libm::sinf(radians),
            ),
            3.,
            GREEN,
        );
        c.stroke((132., 355.), (132., 370.), 2., GREEN);
    } else {
        c.text_on(132., 366., 14., "---", (RED, FACE));
    }
    if let Some((x, _)) = turn.ball {
        c.circle(132. + (x - 240.) / 5.5, 388., 4., GREEN);
    } else {
        c.text_on(132., 390., 10., "X", (RED, PANEL));
    }
}
fn draw_vsi<T: Target>(c: &mut Canvas<T>, model: crate::vsi::Vsi) {
    if let Some(fpm) = model.fpm {
        if let Some(angle) = model.needle_deg {
        let angle = angle.to_radians();
        c.stroke(
            (348., 364.),
            (
                348. + 28. * libm::sinf(angle),
                364. - 28. * libm::cosf(angle),
            ),
            2.,
            GREEN,
        );
        c.circle(348., 364., 3., GREEN);
        }
        let mut text = Text::new();
        let _ = write!(text, "{fpm:+.0}");
        c.text_on(
            348.,
            391.,
            if text.str().len() > 5 { 13. } else { 17. },
            text.str(),
            (GREEN, FACE),
        );
        if model.over_range {
            c.text_on(348., 351., 10., "OVER", (YELLOW, FACE));
        }
    } else {
        c.text_on(348., 389., 17., "---", (RED, FACE));
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::{
        damage::FrameHistory,
        render::{draw_frame, CachedRenderer},
    };
    use instrument_core::{App, Command, Instrument, Quality};
    fn snapshot() -> Snapshot {
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.tick(0);
        let mut s = Snapshot::from_app(&app);
        s.selected = Instrument::Consolidated;
        s
    }
    #[test]
    fn vsi_zero_points_left_and_climb_descent_match_standalone() {
        for (fpm,dx,dy) in [(0.,-1.,0.),(1000.,0.,-1.),(-1000.,0.,1.)] {
            let mut pixels=std::vec![0;480*480];
            let mut target=crate::raster::Framebuffer::new(&mut pixels,480,480).unwrap();
            let mut c=Canvas::new(&mut target);
            let model=crate::vsi::model(instrument_core::Reading {value:Some(fpm),quality:Quality::Valid,age_ms:Some(0)},500);
            draw_vsi(&mut c,model);
            let angle=model.needle_deg.unwrap().to_radians();let x=348.+25.*libm::sinf(angle);let y=364.-25.*libm::cosf(angle);
            assert_eq!(pixels[y as usize*480+x as usize],GREEN);
            if dx<0. {assert!(x<348.);}
            if dy<0. {assert!(y<364.);}
            if dy>0. {assert!(y>364.);}
        }
    }
    #[test]
    fn qnh_editor_reuses_altimeter_pixels_and_restores_three_buffers() {
        use crate::interaction::{EditorView,Button,COMBINED_QNH};
        let mut s=snapshot();let mut bg=std::vec![0;480*480];
        let mut renderer=CachedRenderer::new(&mut bg,480,480).unwrap();
        let mut buffers=core::array::from_fn::<_,3,_>(|_|std::vec![0;480*480]);
        let mut histories=core::array::from_fn::<_,3,_>(|_|FrameHistory::new());
        for phase in 0..6 {
            s.kollsman_pressed=phase==1;
            s.editor=if (2..=4).contains(&phase){Some(EditorView{cents:2993,pressed:(phase==3).then_some(Button::Plus)})}else{None};
            if phase==4{s.ias.quality=Quality::Failed;assert!(s.baro_editor_visible());}
            let mut full=std::vec![0;480*480];draw_frame(&mut full,480,480,s,false);
            for b in 0..3 {renderer.draw_incremental(&mut buffers[b],&mut histories[b],s,false);assert_eq!(buffers[b],full,"phase {phase}, buffer {b}");}
            assert_eq!(full[COMBINED_QNH.y as usize*480+COMBINED_QNH.x as usize],OUTLINE);
            if s.editor.is_some() {
                let mut alt=s;alt.selected=Instrument::Altimeter;let mut reference=std::vec![0;480*480];draw_frame(&mut reference,480,480,alt,false);
                for y in 128..368 {assert_eq!(&full[y*480+80..y*480+400],&reference[y*480+80..y*480+400]);}
            }
        }
    }
    #[test]
    fn expiry_tracks_each_visible_component_without_extending_any_sample() {
        let mut s = snapshot();
        s.external = true;
        s.external_attitude = true;
        s.baro_age_ms = Some(0);
        s.pitch.age_ms = Some(170);
        s.roll.age_ms = Some(190);
        assert_eq!(s.numeric_lifetime_ms(), Some(10));
        let expired = s.aged(10);
        assert!(expired.attitude_model().angles.is_none());
        assert!(expired.model().indicated_ft.is_some());
        assert_eq!(expired.numeric_lifetime_ms(), Some(190)); // turn rate/slip expire next
        s.ias.quality = Quality::Failed;
        assert_eq!(s.displayed_quality(), Some(Quality::Failed));
        assert!(s.model().indicated_ft.is_some());
        assert_eq!(s.numeric_lifetime_ms(), Some(10));
        assert_eq!(s.aged(2000).numeric_lifetime_ms(), None);
        s.menu = Some(crate::navigation::View {
            info: false,
            generation: 1,
            pressed: None,
            context: crate::navigation::Context {
                demo: false, selected: s.selected,
                external: true,
                external_attitude: true,
                imu: false,
                session: 0,
            },
        });
        assert_eq!(s.numeric_lifetime_ms(), None);
    }
    #[test]
    fn rotating_buffers_restore_motion_failures_and_switches_at_each_size() {
        for size in [64, 241, 480] {
            let mut bg = std::vec![0;size*size];
            let mut renderer = CachedRenderer::new(&mut bg, size as u32, size as u32).unwrap();
            let mut frames = core::array::from_fn::<_, 3, _>(|_| std::vec![0;size*size]);
            let mut histories = core::array::from_fn::<_, 3, _>(|_| FrameHistory::new());
            let mut expected = std::vec![0;size*size];
            for n in 0..24 {
                let mut s = snapshot();
                s.pitch.value = Some(n as f32 * 5. - 60.);
                s.roll.value = Some(n as f32 * 15. - 180.);
                s.heading.value = Some((n * 17 % 360) as f32);
                s.altitude.value = Some(n as f32 * 2000. - 2000.);
                s.ias.value = Some(n as f32 * 400.);
                s.vsi.value = Some(n as f32 * 400. - 4500.);
                if n % 4 == 0 {
                    s.roll.quality = Quality::Failed;
                    s.ias.quality = Quality::Missing;
                }
                if n % 5 == 0 {
                    s = s.aged(2000);
                }
                if n % 6 == 0 {
                    s.selected = Instrument::Altimeter;
                    s.altitude_hidden = n % 12 == 0;
                }
                assert!(draw_frame(
                    &mut expected,
                    size as u32,
                    size as u32,
                    s,
                    false
                ));
                let slot = n % 3;
                assert!(renderer.draw_incremental(
                    &mut frames[slot],
                    &mut histories[slot],
                    s,
                    false
                ));
                assert!(frames[slot] == expected, "size {size}, frame {n}");
            }
        }
    }
    #[test]
    fn altitude_hiding_keeps_border_and_only_changes_the_readout_rectangle() {
        let mut s = snapshot();
        s.selected = Instrument::Altimeter;
        s.altitude.value = Some(6500.); // Hundred-foot hand points through the window.
        let mut visible = std::vec![0;480*480];
        draw_frame(&mut visible, 480, 480, s, false);
        let mut bg = visible.clone();
        let mut renderer = CachedRenderer::new(&mut bg, 480, 480).unwrap();
        let mut frames = core::array::from_fn::<_, 3, _>(|_| visible.clone());
        let mut histories = core::array::from_fn::<_, 3, _>(|_| FrameHistory::new());
        for n in 0..12 {
            s.altitude_hidden = n % 2 == 0;
            let mut expected = visible.clone();
            draw_frame(&mut expected, 480, 480, s, false);
            let slot = n % 3;
            renderer.draw_incremental(&mut frames[slot], &mut histories[slot], s, false);
            assert!(frames[slot] == expected);
            for y in 0..480 {
                for x in 0..480 {
                    let r = crate::visibility::ALTITUDE;
                    if !r.contains(x, y) {
                        assert_eq!(
                            expected[y as usize * 480 + x as usize],
                            visible[y as usize * 480 + x as usize]
                        );
                    } else if x == 124 || x == 355 || y == 284 || y == 323 {
                        assert_eq!(expected[y as usize * 480 + x as usize], WHITE);
                    }
                }
            }
            if s.altitude_hidden {
                assert_eq!(expected[300 * 480 + 240], WHITE, "hidden window masked the hand");
                assert_eq!(expected[300 * 480 + 130], rgb(9,14,19), "hidden window left its fill");
            }
        }
    }
}
