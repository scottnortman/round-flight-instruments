//! Actual production pixels for reviewing both display additions.
use instrument_core::{App, Command, Field, Instrument, Quality};
use instrument_ui::{render::draw_frame, Snapshot};
use std::{
    fs,
    io::{self, Write},
};

fn main() -> io::Result<()> {
    fs::create_dir_all("output/consolidated")?;
    let mut app = App::new();
    app.apply(Command::LevelPreset).unwrap();
    for (field, value) in [
        (Field::IasKt, 135.),
        (Field::PressureAltitudeFt, 6500.),
        (Field::HeadingDeg, 315.),
        (Field::VsiFpm, 1250.),
    ] {
        app.apply(Command::Set(field, value)).unwrap();
    }
    app.tick(0);
    for (name, selected, hidden, pitch, roll, failed) in [
        ("debug", Instrument::Debug, false, 5., 20., false),
        ("level", Instrument::Consolidated, false, 0., 0., false),
        (
            "climb-bank",
            Instrument::Consolidated,
            false,
            10.,
            30.,
            false,
        ),
        (
            "partial-failure",
            Instrument::Consolidated,
            false,
            0.,
            0.,
            true,
        ),
        (
            "altitude-visible",
            Instrument::Altimeter,
            false,
            0.,
            0.,
            false,
        ),
        (
            "altitude-hidden",
            Instrument::Altimeter,
            true,
            0.,
            0.,
            false,
        ),
    ] {
        let mut s = Snapshot::from_app(&app);
        s.selected = selected;
        s.altitude_hidden = hidden;
        s.pitch.value = Some(pitch);
        s.roll.value = Some(roll);
        if failed {
            s.ias.quality = Quality::Failed;
            s.roll.quality = Quality::Failed;
        }
        save(name, s)?;
    }
    let mut s = Snapshot::from_app(&app);
    s.selected = Instrument::Consolidated;
    s.kollsman_pressed=true;save("qnh-pressed",s)?;
    s.kollsman_pressed=false;
    s.editor=Some(instrument_ui::interaction::EditorView {cents:2993,pressed:None});save("qnh-editor",s)?;
    s.editor=None;
    s.menu = Some(instrument_ui::navigation::View {
        info: false,
        generation: 1,
        pressed: None,
        context: instrument_ui::navigation::Context {
            demo: false, selected: s.selected,
            external: false,
            external_attitude: false,
            imu: false,
            session: 0,
        },
    });
    save("menu", s)?;
    s.menu.as_mut().unwrap().context.demo=true;
    save("menu-demo", s)?;
    app.apply(Command::Select(Instrument::Consolidated)).unwrap();
    app.apply(Command::Demo(Some(true))).unwrap();
    for t in (10..=15000).step_by(10) {app.tick(t);}
    save("demo-auto",Snapshot::from_app(&app))
}
fn save(name: &str, s: Snapshot) -> io::Result<()> {
    let mut pixels = vec![0; 480 * 480];
    assert!(draw_frame(&mut pixels, 480, 480, s, false));
    let mut file = fs::File::create(format!("output/consolidated/{name}.ppm"))?;
    write!(file, "P6\n480 480\n255\n")?;
    for p in pixels {
        file.write_all(&[
            (((p >> 11) & 31) * 255 / 31) as u8,
            (((p >> 5) & 63) * 255 / 63) as u8,
            ((p & 31) * 255 / 31) as u8,
        ])?;
    }
    Ok(())
}
