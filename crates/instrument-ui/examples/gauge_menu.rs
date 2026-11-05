//! Reproducible previews of the actual portable menu renderer.
use instrument_core::{App, Command, Instrument};
use instrument_ui::{
    navigation::{Button, Context, View},
    render::draw_frame,
    Snapshot,
};
use std::{fs, io::Write};
fn main() -> std::io::Result<()> {
    fs::create_dir_all("output/gauge-menu")?;
    let mut app = App::new();
    app.apply(Command::LevelPreset).unwrap();
    app.tick(0);
    for (label, selected, external, pressed) in [
        ("altimeter", Instrument::Altimeter, false, None),
        (
            "attitude",
            Instrument::Attitude,
            false,
            Some(Button::Gauge(Instrument::Altimeter)),
        ),
        ("external", Instrument::Altimeter, true, None),
        ("info", Instrument::Altimeter, false, Some(Button::Info)),
        ("device-info", Instrument::Altimeter, false, None),
        ("device-info-back", Instrument::Altimeter, false, Some(Button::Back)),
    ] {
        app.apply(Command::Select(selected)).unwrap();
        let mut s = Snapshot::from_app(&app);
        s.menu = Some(View {
            info: label.starts_with("device-info"),
            generation: 1,
            context: Context {
                external,
                ..Context::from_app(&app)
            },
            pressed,
        });
        s.device=instrument_core::device::Info {
            firmware:"v0.2.0-rc.9-12-g1234567-dirty",
            uuid:instrument_core::device::Uuid::parse("11223344-5566-4788-9aab-ccddeeff0011"),
        };
        let mut pixels = vec![0; 480 * 480];
        assert!(draw_frame(&mut pixels, 480, 480, s, false));
        let mut f = fs::File::create(format!("output/gauge-menu/{label}.ppm"))?;
        write!(f, "P6\n480 480\n255\n")?;
        for p in pixels {
            f.write_all(&[
                (((p >> 11) & 31) * 255 / 31) as u8,
                (((p >> 5) & 63) * 255 / 63) as u8,
                ((p & 31) * 255 / 31) as u8,
            ])?;
        }
    }
    Ok(())
}
