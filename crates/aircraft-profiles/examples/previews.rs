use aircraft_profiles::{
    preview::{self, Sample},
    Gauges,
};
use instrument_core::Instrument;
use std::{fs, io::Write};
fn main() -> std::io::Result<()> {
    fs::create_dir_all("output/profile-editor")?;
    for gauge in Instrument::ALL {
        let mut pixels = vec![0u16; 480 * 480];
        preview::render(
            &Gauges::default(),
            gauge,
            Sample::defaults(gauge),
            &mut pixels,
            480,
        )
        .unwrap();
        let mut file = fs::File::create(format!("output/profile-editor/{}.ppm", gauge.name()))?;
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
