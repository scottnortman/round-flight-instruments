//! Read-only telemetry table. Values retain source validity and acquisition age.
//! Missing/expired samples are placeholders, never zero or a held live value.
use crate::{raster::{rgb, Canvas, Target}, render::Text, Snapshot};
use core::fmt::Write;
use instrument_core::{Quality, Reading};

const WHITE: u16 = rgb(235, 240, 245);
const DIM: u16 = rgb(155, 173, 185);
const GREEN: u16 = rgb(139, 230, 96);
const AMBER: u16 = rgb(255, 195, 60);
const RED: u16 = rgb(255, 110, 110);

pub fn rows(s: Snapshot) -> [(&'static str, Reading); 10] {
    [
        ("ALT FT", if s.external { s.altitude } else {
            Reading {value: s.model().indicated_ft, quality: s.model().quality, ..s.altitude}
        }),
        ("PALT FT", s.pressure_altitude), ("IAS KT", s.ias),
        ("PITCH DEG", s.pitch), ("ROLL DEG", s.roll),
        ("HDG MAG", s.heading), ("TURN D/S", s.turn_rate),
        ("SLIP", s.ball), ("VSI FPM", s.vsi),
        ("QNH INHG", Reading { value: s.baro_inhg.is_finite().then_some(s.baro_inhg),
            quality: s.baro_quality, age_ms: s.baro_age_ms }),
    ]
}
pub fn lifetime_ms(s: Snapshot) -> Option<u64> {
    let limits = [1000, 1000, s.airspeed_stale_ms(), s.attitude_stale_ms(),
        s.attitude_stale_ms(), s.heading_stale_ms(), s.turn_stale_ms(),
        s.turn_stale_ms(), s.vsi_stale_ms(), 1000];
    rows(s).into_iter().zip(limits).enumerate()
        .filter(|(n, ((_, r), _))| r.quality == Quality::Valid
            && r.value.is_some_and(f32::is_finite) && (*n != 9 || s.external))
        .map(|(_, ((_, r), ttl))| ttl.saturating_sub(r.age_ms.unwrap_or(ttl))).min()
}
pub fn quality(s: Snapshot) -> Quality {
    let rows = rows(s);
    [Quality::TimingFault, Quality::Failed, Quality::Stale, Quality::Missing]
        .into_iter().find(|q| rows.iter().any(|(_, r)| r.quality == *q))
        .unwrap_or(Quality::Valid)
}
pub fn background<T: Target>(target: &mut T) {
    let mut c = Canvas::new(target);
    c.circle(240., 240., 238., rgb(70, 82, 93));
    c.circle(240., 240., 229., 0);
    c.text(240., 58., 20., "TELEMETRY DEBUG", WHITE);
    for (x, text) in [(124., "VALUE/UNIT"), (240., "READING"), (324., "AGE MS"), (378., "STATE")] {
        c.text(x, 115., 11., text, DIM);
    }
    c.text(240., 435., 12., "HOLD CENTER 1S: MENU", DIM);
}
pub fn draw<T: Target>(target: &mut T, s: Snapshot) {
    let mut c = Canvas::new(target);
    let mut source = Text::new();
    let _ = write!(source, "{}", s.source_label);
    c.text(240., 87., 15., source.str(), AMBER);
    for (n, (label, reading)) in rows(s).into_iter().enumerate() {
        let y = 132. + n as f32 * 24.;
        let (state, color) = match reading.quality {
            Quality::Valid => ("OK", GREEN), Quality::Missing => ("NONE", DIM),
            Quality::Stale => ("STALE", AMBER), Quality::Failed => ("FAIL", RED),
            Quality::TimingFault => ("TIME", RED),
        };
        let mut value = Text::new();
        if let Some(v) = reading.value.filter(|v| v.is_finite() && reading.quality == Quality::Valid) {
            let _ = write!(value, "{v:.2}");
        } else { let _ = value.write_str("---"); }
        let mut age = Text::new();
        match reading.age_ms {
            Some(ms) if ms <= 9999 => { let _ = write!(age, "{ms}"); }
            Some(_) => { let _ = age.write_str(">9999"); }
            None => { let _ = age.write_str("-"); }
        }
        c.text(124., y, 13., label, WHITE);
        c.text(240., y, 14., value.str(), color);
        c.text(324., y, 12., age.str(), DIM);
        c.text(378., y, 11., state, color);
    }
    let mut seq = Text::new();
    let _ = write!(seq, "SEQ {}", s.source_sequence);
    c.text(240., 375., 12., seq.str(), WHITE);
    let mut session = Text::new();
    let _ = write!(session, "SESSION {:016X}", s.source_session);
    c.text(240., 395., 11., session.str(), DIM);
    let mode = if s.source_label.starts_with("DEMO") {"DEMO DATA"}
        else if s.external {"RECEIVED SIM DATA"} else {"LOCAL / BENCH DATA"};
    c.text(240., 415., 12., mode, AMBER);
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::{damage::FrameHistory, render::{draw_frame, CachedRenderer}};
    use instrument_core::{App, Command, Instrument};

    fn sample() -> Snapshot {
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.apply(Command::Select(Instrument::Debug)).unwrap();
        app.tick(0);
        Snapshot::from_app(&app)
    }
    #[test]
    fn debug_retains_real_values_qualities_ages_and_expires_each_row() {
        let mut s = sample();
        s.external = true;
        s.external_attitude = true;
        s.source_session = 0x1234;
        s.source_sequence = 71;
        s.altitude.value = Some(6500.);
        s.pitch.age_ms = Some(59);
        s.roll.quality = Quality::Failed;
        s.baro_age_ms = Some(990);
        assert_eq!(s.displayed_age_ms(), Some(990));
        assert_eq!(rows(s)[0].1.value, Some(6500.));
        assert_eq!(rows(s)[3].1.age_ms, Some(59));
        assert_eq!(rows(s)[4].1.quality, Quality::Failed);
        assert_eq!(s.displayed_quality(), Some(Quality::Failed));
        let aged = s.aged(10);
        assert_eq!(aged.source_session, 0x1234);
        assert_eq!(aged.source_sequence, 71);
        assert_eq!(rows(aged)[3].1.age_ms, Some(69));
        assert_eq!(rows(aged)[9].1.quality, Quality::Stale);
        assert_eq!(rows(aged)[9].1.value, None);
        assert_eq!(aged.altitude.value, Some(6500.));
        let expired = s.aged(1000);
        assert!(rows(expired).iter().all(|(_, r)| r.quality != Quality::Valid));
        assert_eq!(expired.numeric_lifetime_ms(), None);
        assert!(s.numeric_lifetime_ms().is_some_and(|ttl| ttl <= 10));
        s.altitude.value = Some(200000.);
        s.pressure_altitude.value = Some(199000.);
        assert_eq!(rows(s)[0].1.value, Some(200000.));
        assert_eq!(rows(s)[1].1.value, Some(199000.));
        assert!(s.numeric_lifetime_ms().is_some());
    }
    #[test]
    fn debug_changes_and_failures_restore_every_dma_buffer_exactly() {
        for size in [240usize, 480] {
            let mut bg = std::vec![0; size * size];
            let mut renderer = CachedRenderer::new(&mut bg, size as u32, size as u32).unwrap();
            let mut buffers = core::array::from_fn::<_,3,_>(|_| std::vec![0;size*size]);
            let mut histories = core::array::from_fn::<_,3,_>(|_| FrameHistory::new());
            let mut expected = std::vec![0; size*size];
            for n in 0..12 {
                let mut s = sample();
                s.source_label = if n % 2 == 0 {"MSFS 2020"} else {"LINK STALE"};
                s.source_session = u64::MAX - n;
                s.source_sequence = u64::MAX - n * 2000;
                s.altitude.value = Some(10000. - n as f32 * 1200.);
                s.pitch.value = Some(-50. + n as f32 * 8.);
                s.roll.quality = match n % 5 {0=>Quality::Missing,1=>Quality::Stale,2=>Quality::Failed,3=>Quality::TimingFault,_=>Quality::Valid};
                s.roll.age_ms = Some(n * 1001);
                s.baro_quality = s.roll.quality;
                assert!(draw_frame(&mut expected,size as u32,size as u32,s,false));
                for (pixels, history) in buffers.iter_mut().zip(&mut histories) {
                    assert!(renderer.draw_incremental(pixels,history,s,false));
                    assert_eq!(&*pixels, &expected, "buffer retained old debug text, step {n}");
                }
            }
        }
    }
}
