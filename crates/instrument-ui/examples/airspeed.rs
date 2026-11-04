use instrument_core::{App, Command, Instrument, Quality};
use instrument_ui::{render::draw_frame, Snapshot};
use std::{
    fs,
    io::{self, Write},
};
fn main() -> io::Result<()> {
    fs::create_dir_all("output/gauge")?;
    let mut app = App::new();
    app.apply(Command::LevelPreset).unwrap();
    app.tick(0);
    app.apply(Command::Select(Instrument::Airspeed)).unwrap();
    for (name, ias, external, quality) in [
        ("airspeed", 85., false, Quality::Valid),
        ("airspeed-live", 120., true, Quality::Valid),
        ("airspeed-over-range", 350., true, Quality::Valid),
        ("airspeed-missing", 0., true, Quality::Missing),
    ] {
        let mut s = Snapshot::from_app(&app);
        s.ias.value = Some(ias);
        s.ias.quality = quality;
        s.external = external;
        s.external_attitude = external;
        s.source_label = if external { "MSFS INPUT" } else { "SIM INPUT" };
        let mut pixels = vec![0; 480 * 480];
        draw_frame(&mut pixels, 480, 480, s, false);
        let mut file = fs::File::create(format!("output/gauge/{name}-480.ppm"))?;
        write!(file, "P6\n480 480\n255\n")?;
        for p in pixels {
            file.write_all(&[
                (((p >> 11) & 31) * 255 / 31) as u8,
                (((p >> 5) & 63) * 255 / 63) as u8,
                ((p & 31) * 255 / 31) as u8,
            ])?;
        }
    }
    Ok(())
}
