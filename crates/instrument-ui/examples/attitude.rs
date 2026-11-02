//! Same standalone face as the firmware. Reproducible previews + CPU-only timing.
use instrument_core::{App, Command, Field, Instrument, Quality};
use instrument_ui::{
    damage::FrameHistory,
    render::{draw_frame, CachedRenderer},
    Snapshot,
};
use std::{fs, io::Write, time::Instant};
fn main() -> std::io::Result<()> {
    fs::create_dir_all("output/attitude")?;
    let mut app = App::new();
    app.apply(Command::LevelPreset).unwrap();
    app.apply(Command::Select(Instrument::Attitude)).unwrap();
    app.tick(0);
    let base = Snapshot::from_app(&app);
    for size in [240, 480, 720] {
        for (label, pitch, roll, quality) in [
            ("level", 0., 0., Quality::Valid),
            ("climb", 10., 0., Quality::Valid),
            ("right-bank", 0., 30., Quality::Valid),
            ("left-bank", 0., -30., Quality::Valid),
            ("climbing-right", 10., 30., Quality::Valid),
            ("descending-left", -10., -30., Quality::Valid),
            ("inverted", 0., 180., Quality::Valid),
            ("nose-up", 90., 0., Quality::Valid),
            ("missing", 0., 0., Quality::Missing),
            ("stale", 0., 0., Quality::Stale),
            ("failed", 0., 0., Quality::Failed),
            ("timing-fault", 0., 0., Quality::TimingFault),
        ] {
            let mut s = base;
            s.pitch.value = Some(pitch);
            s.roll.value = Some(roll);
            s.roll.quality = quality;
            let mut pixels = vec![0; size * size];
            assert!(draw_frame(&mut pixels, size as u32, size as u32, s, false));
            let mut file = fs::File::create(format!("output/attitude/{label}-{size}.ppm"))?;
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
    let mut bg = vec![0; 480 * 480];
    let mut frames: [Vec<u16>; 3] = core::array::from_fn(|_| vec![0; 480 * 480]);
    let mut history: [FrameHistory; 3] = core::array::from_fn(|_| FrameHistory::new());
    let mut runs = vec![instrument_ui::attitude::CopyRun::default(); 2048];
    let mut renderer = CachedRenderer::with_foreground_cache(&mut bg, 480, 480, &mut runs).unwrap();
    let start = Instant::now();
    let mut worst = 0u128;
    for n in 0..600 {
        let t = n as f32 * core::f32::consts::PI / 150.;
        let mut s = base;
        s.pitch.value = Some(t.sin() * 20.);
        s.roll.value = Some((t * 0.5).sin() * 60.);
        let at = Instant::now();
        assert!(renderer.draw_incremental(&mut frames[n % 3], &mut history[n % 3], s, false));
        worst = worst.max(at.elapsed().as_micros());
        std::hint::black_box(&frames[n % 3]);
    }
    println!(
        "Desktop CPU only: 600 frames, {:.2} ms/frame mean, {:.2} ms max; NOT board FPS",
        start.elapsed().as_secs_f64() * 1000. / 600.,
        worst as f64 / 1000.
    );
    // Existing shared CLI fields are the source contract; no sensor is assumed.
    assert!(Field::PitchDeg.accepts(10.));
    Ok(())
}
