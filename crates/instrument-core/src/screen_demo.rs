//! Display-only showcase data. Never enters live samples or aircraft controls.
use crate::{Field, Quality, Reading};

#[derive(Clone, Copy)]
pub struct Frame {
    pub readings: [Reading; 8],
    pub using_imu: bool,
}
pub(crate) fn frame(elapsed_ms: u64) -> Frame {
    // One repeating minute: takeoff, climb, banks, descent and recovery.
    const KEYS: [[f32; 8]; 5] = [
        [0., 0., 0., 0., 0., 0., 0., 0.],
        [3000., 12., 35., 90., 95., 3., 0.8, 1600.],
        [10000., 0., -35., 180., 170., -3., -0.8, 0.],
        [6000., -10., 15., 270., 140., 1.5, 0.4, -1600.],
        [0., 0., 0., 360., 0., 0., 0., 0.],
    ];
    let phase = (elapsed_ms % 60_000) as f32 / 15_000.;
    let index = phase as usize;
    let t = phase - index as f32;
    let ease = t * t * (3. - 2. * t);
    Frame {
        readings: core::array::from_fn(|n| {
            let mut value = KEYS[index][n] + (KEYS[index + 1][n] - KEYS[index][n]) * ease;
            if n == Field::HeadingDeg as usize {
                value %= 360.;
            }
            Reading {
                value: Some(value),
                quality: Quality::Valid,
                age_ms: Some(0),
            }
        }),
        using_imu: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{imu_attitude::Input, App, Command, Instrument};
    #[test]
    fn complete_cycles_stay_inside_all_field_limits_and_repeat() {
        let first = frame(0);
        let mut changed = [false; 8];
        for t in (0..180_000).step_by(17) {
            let f = frame(t);
            for field in Field::ALL {
                let r = f.readings[field as usize];
                assert!(field.accepts(r.value.unwrap()), "{field:?} at {t}");
                assert_eq!(r.quality, Quality::Valid);
                changed[field as usize] |= r.value != first.readings[field as usize].value;
            }
        }
        assert!(changed.into_iter().all(|v| v));
        assert_eq!(frame(60_000).readings[0].value, first.readings[0].value);
    }
    #[test]
    fn demo_is_idempotent_and_leaves_live_samples_settings_and_screen_untouched() {
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.tick(0);
        app.apply(Command::Select(Instrument::Altimeter)).unwrap();
        let altitude = app.altimeter_reading(0).value;
        app.apply(Command::Demo(Some(true))).unwrap();
        for t in (10..=10_000).step_by(10) {
            app.tick(t);
        }
        let demo = app.demo_frame().unwrap().readings[0].value;
        app.apply(Command::Demo(Some(true))).unwrap();
        assert_eq!(app.demo_frame().unwrap().readings[0].value, demo);
        assert_eq!(app.altimeter_reading(app.now_ms()).value, altitude);
        assert_eq!(app.selected(), Instrument::Altimeter);
        assert!(!app.baro_edit_allowed());
        assert!(app.apply(Command::Baro(30.01)).is_err());
        assert_eq!(app.baro_inhg(), 29.92);
        app.apply(Command::Demo(Some(false))).unwrap();
        assert!(app.baro_edit_allowed());
        assert!(app.demo_frame().is_none());
        app.apply(Command::Demo(Some(true))).unwrap();
        let phase = app.demo_frame().unwrap().readings[0].value;
        for gauge in Instrument::ALL {
            app.apply(Command::Select(gauge)).unwrap();
            assert!(app.demo_active());
            assert_eq!(app.demo_frame().unwrap().readings[0].value, phase);
        }
        app.apply(Command::Demo(Some(true))).unwrap();
        app.apply(Command::SourceDemo).unwrap();
        assert!(!app.demo_active());
    }
    #[test]
    fn fresh_imu_drives_tilt_and_demo_turn_without_renewing_age_or_holding_stale_pose() {
        for gauge in [
            Instrument::Attitude,
            Instrument::Turn,
            Instrument::Consolidated,
        ] {
            let mut app = App::new();
            app.tick(0);
            app.apply(Command::Select(gauge)).unwrap();
            app.apply(Command::Demo(Some(true))).unwrap();
            assert!(!app.demo_frame().unwrap().using_imu);
            app.accept_imu(Input {
                received_us: 1,
                age_ms: 5,
                pitch_roll_deg: Some([10., 30.]),
                quality: Quality::Valid,
            });
            let frame = app.demo_frame().unwrap();
            assert!(frame.using_imu);
            assert_eq!(frame.readings[Field::PitchDeg as usize].value, Some(10.));
            assert_eq!(frame.readings[Field::TurnDps as usize].value, Some(3.));
            for t in (10..=90).step_by(10) {
                app.tick(t);
            }
            app.accept_imu(Input {
                received_us: 1,
                age_ms: 0,
                pitch_roll_deg: Some([10., 30.]),
                quality: Quality::Valid,
            });
            assert_eq!(
                app.demo_frame().unwrap().readings[Field::RollDeg as usize].age_ms,
                Some(95)
            );
            app.tick(100);
            assert!(!app.demo_frame().unwrap().using_imu);
            app.tick(201);
            assert!(app.timing_fault());
            assert!(app
                .demo_frame()
                .unwrap()
                .readings
                .into_iter()
                .all(|r| r.value.is_none() && r.quality == Quality::TimingFault));
        }
    }
}
