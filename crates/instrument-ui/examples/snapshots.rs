use instrument_core::{Instrument, Quality, Reading};
use instrument_ui::{raster::Framebuffer, render, Snapshot};
use std::{fs, io::Write};
fn main() -> std::io::Result<()> {
    fs::create_dir_all("output/gauge")?;
    for size in [240u32, 480, 720] {
        for (label, quality, value, baro) in [
            ("valid", Quality::Valid, Some(4520.), 29.92),
            ("manual-8500", Quality::Valid, Some(8500.), 29.92),
            ("zero", Quality::Valid, Some(0.), 29.92),
            ("negative", Quality::Valid, Some(-100.), 29.92),
            ("baro", Quality::Valid, Some(4520.), 30.92),
            ("missing", Quality::Missing, None, 29.92),
            ("stale", Quality::Stale, None, 29.92),
            ("failed", Quality::Failed, None, 29.92),
        ] {
            let mut pixels = vec![0u16; (size * size) as usize];
            render::draw(
                &mut Framebuffer::new(&mut pixels, size, size).unwrap(),
                Snapshot {
                    selected: Instrument::Altimeter,
                    altitude: Reading {
                        value,
                        quality,
                        age_ms: Some(0),
                    },
                    baro_inhg: baro,
                },
            );
            let mut file = fs::File::create(format!("output/gauge/{label}-{size}.ppm"))?;
            write!(file, "P6\n{size} {size}\n255\n")?;
            let rgb: Vec<u8> = pixels
                .iter()
                .flat_map(|&p| {
                    [
                        (((p >> 11) & 31) * 255 / 31) as u8,
                        (((p >> 5) & 63) * 255 / 63) as u8,
                        ((p & 31) * 255 / 31) as u8,
                    ]
                })
                .collect();
            file.write_all(&rgb)?;
        }
    }
    Ok(())
}
