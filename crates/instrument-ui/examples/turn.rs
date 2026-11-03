//! Reproducible previews of the same portable face used on hardware.
use instrument_core::{App, Command, Field, Instrument, Quality};
use instrument_ui::{render::draw_frame, Snapshot};
use std::{fs, io::Write};
fn main() -> std::io::Result<()> {
    fs::create_dir_all("output/turn")?;
    let mut app = App::new();
    app.apply(Command::LevelPreset).unwrap();
    app.apply(Command::Select(Instrument::Turn)).unwrap();
    app.tick(0);
    for (label, rate, ball, rq, bq) in [
        ("centered", 0., 0., Quality::Valid, Quality::Valid),
        ("right", 3., 0., Quality::Valid, Quality::Valid),
        ("left", -3., 0., Quality::Valid, Quality::Valid),
        ("right-ball-left", 3., -0.75, Quality::Valid, Quality::Valid),
        ("right-ball-right", 3., 0.75, Quality::Valid, Quality::Valid),
        ("rate-missing", 0., 0., Quality::Missing, Quality::Valid),
        ("ball-failed", 3., 0., Quality::Valid, Quality::Failed),
        ("stale", 0., 0., Quality::Stale, Quality::Stale),
    ] {
        app.apply(Command::Set(Field::TurnDps, rate)).unwrap();
        app.apply(Command::Set(Field::Ball, ball)).unwrap();
        for size in [240, 480, 720] {
            let mut s = Snapshot::from_app(&app);
            s.turn_rate.value = Some(rate);
            s.turn_rate.quality = rq;
            s.ball.value = Some(ball);
            s.ball.quality = bq;
            let mut pixels = vec![0; size * size];
            assert!(draw_frame(&mut pixels, size as u32, size as u32, s, false));
            let mut f = fs::File::create(format!("output/turn/{label}-{size}.ppm"))?;
            write!(f, "P6\n{size} {size}\n255\n")?;
            let bytes: Vec<u8> = pixels
                .into_iter()
                .flat_map(|p| {
                    [
                        (((p >> 11) & 31) * 255 / 31) as u8,
                        (((p >> 5) & 63) * 255 / 63) as u8,
                        ((p & 31) * 255 / 31) as u8,
                    ]
                })
                .collect();
            f.write_all(&bytes)?;
        }
    }
    Ok(())
}
