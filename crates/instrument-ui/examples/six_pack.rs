//! Reproducible images from the production renderer, without hardware.
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
    for (name, selected, value, quality) in [
        ("heading-north", Instrument::Heading, 0., Quality::Valid),
        ("heading-east", Instrument::Heading, 90., Quality::Valid),
        ("heading-wrap", Instrument::Heading, 359., Quality::Valid),
        ("heading-missing", Instrument::Heading, 0., Quality::Missing),
        ("vsi-level", Instrument::Vsi, 0., Quality::Valid),
        ("vsi-climb", Instrument::Vsi, 1000., Quality::Valid),
        ("vsi-descent", Instrument::Vsi, -1000., Quality::Valid),
        ("vsi-over-range", Instrument::Vsi, 3500., Quality::Valid),
        ("vsi-missing", Instrument::Vsi, 0., Quality::Missing),
    ] {
        let mut s = Snapshot::from_app(&app);
        s.selected = selected;
        let r = if selected == Instrument::Heading {
            &mut s.heading
        } else {
            &mut s.vsi
        };
        r.value = Some(value);
        r.quality = quality;
        r.age_ms = Some(0);
        let mut pixels = vec![0; 480 * 480];
        assert!(draw_frame(&mut pixels, 480, 480, s, false));
        let mut f = fs::File::create(format!("output/gauge/{name}-480.ppm"))?;
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
