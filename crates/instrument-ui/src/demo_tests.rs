extern crate std;
use crate::{
    damage::FrameHistory,
    render::{draw_frame, CachedRenderer, NavigationCache},
    Snapshot,
};
use instrument_core::{imu_attitude::Input, App, Command, Field, Instrument, Quality};

#[test]
fn every_screen_can_demo_without_live_data_and_all_buffer_paths_match() {
    for gauge in Instrument::ALL {
        let mut app = App::new();
        app.tick(0);
        app.apply(Command::Select(gauge)).unwrap();
        app.apply(Command::Demo(Some(true))).unwrap();
        let mut background = std::vec![0;480*480];
        let mut renderer = CachedRenderer::new(&mut background, 480, 480).unwrap();
        let mut buffers = core::array::from_fn::<_, 3, _>(|_| std::vec![0;480*480]);
        let mut histories = core::array::from_fn::<_, 3, _>(|_| FrameHistory::new());
        let mut expected = std::vec![0;480*480];
        for n in 0..12 {
            let next = n * 5500;
            for t in (app.now_ms()..=next).step_by(10) {
                app.tick(t);
            }
            app.tick(next);
            if n % 3 == 1 && app.demo_uses_imu() {
                app.accept_imu(Input {
                    received_us: next + 1,
                    age_ms: 3,
                    pitch_roll_deg: Some([12., -30.]),
                    quality: Quality::Valid,
                });
            }
            let s = Snapshot::from_app(&app);
            assert!(s.source_label.starts_with("DEMO "));
            assert!(!s.external && !s.imu_bench);
            assert_eq!(s.displayed_quality(), Some(Quality::Valid), "{gauge:?} {n}");
            assert!(s.numeric_lifetime_ms().is_some());
            let slot = n as usize % 3;
            assert!(draw_frame(&mut expected, 480, 480, s, false));
            assert!(renderer.draw_incremental(&mut buffers[slot], &mut histories[slot], s, false));
            assert!(buffers[slot] == expected, "{gauge:?}, frame {n}");
        }
        app.apply(Command::Demo(Some(false))).unwrap();
        let s = Snapshot::from_app(&app);
        assert_eq!(s.displayed_quality(), Some(Quality::Missing));
        draw_frame(&mut expected, 480, 480, s, false);
        for n in 0..3 {
            renderer.draw_incremental(&mut buffers[n], &mut histories[n], s, false);
            assert!(buffers[n] == expected, "{gauge:?} demo exit left pixels");
        }
    }
}

#[test]
fn demo_keeps_external_stream_live_and_restores_latest_real_data_and_original_deadlines() {
    use instrument_core::simulator_link::{Datum, Frame, Lifecycle, Provider};
    let mut app = App::new();
    app.tick(0);
    app.apply(Command::SourceExternal(Provider::Replay, 7))
        .unwrap();
    let feed = |app: &mut App, sequence, indicated_ft| {
        app.apply(Command::Telemetry(Frame {
            session: 7,
            sequence,
            receiver_ms: app.now_ms(),
            age_ms: 5,
            state: Lifecycle::Running,
            indicated_ft: Datum::Value(indicated_ft),
            pressure_ft: Datum::Value(100.),
            baro_inhg: Datum::Value(30.12),
        }))
        .unwrap()
    };
    feed(&mut app, 1, 4200.);
    app.apply(Command::Demo(Some(true))).unwrap();
    app.tick(20);
    feed(&mut app, 2, 7300.);
    let s = Snapshot::from_app(&app);
    assert!(s.model().indicated_ft.is_some());
    assert_eq!(s.source_label, "DEMO AUTO");
    assert!(!s.external);
    assert_eq!(app.external().unwrap().session, 7);
    assert_eq!(app.external().unwrap().sequence(), 2);
    assert_eq!(app.altimeter_reading(20).value, Some(7300.));
    assert!(s.aged(100).attitude_model().angles.is_none());
    assert!(s.aged(100).model().indicated_ft.is_some());
    app.apply(Command::Demo(Some(false))).unwrap();
    let real = Snapshot::from_app(&app);
    assert!(real.external);
    assert_eq!(real.source_session, 7);
    assert_eq!(real.model().indicated_ft, Some(7300.));
    assert_eq!(real.altitude.age_ms, Some(5));
    assert_eq!(real.baro_inhg, 30.12);
    assert_eq!(app.reading(Field::PressureAltitudeFt, 20).value, Some(100.));
}

#[test]
fn demo_button_state_and_blue_press_restore_from_shared_menu_cache() {
    use crate::navigation::{Button, Context, View};
    let mut app = App::new();
    app.tick(0);
    let mut background = std::vec![0;480*480];
    let mut menu_pixels = std::vec![0;480*480];
    let mut renderer = CachedRenderer::new(&mut background, 480, 480).unwrap();
    let mut menu = NavigationCache::new(&mut menu_pixels).unwrap();
    let mut buffers = core::array::from_fn::<_, 3, _>(|_| std::vec![0;480*480]);
    let mut histories = core::array::from_fn::<_, 3, _>(|_| FrameHistory::new());
    let mut expected = std::vec![0;480*480];
    for n in 0..12 {
        app.apply(Command::Demo(Some(n / 3 % 2 == 1))).unwrap();
        let mut s = Snapshot::from_app(&app);
        s.menu = Some(View {
            info: false,
            context: Context::from_app(&app),
            generation: n + 1,
            pressed: (n % 3 == 1).then_some(Button::Demo),
        });
        assert!(renderer.draw_incremental_with_menu(
            &mut buffers[n as usize % 3],
            &mut histories[n as usize % 3],
            s,
            &mut menu,
            &mut || {}
        ));
        draw_frame(&mut expected, 480, 480, s, false);
        assert!(buffers[n as usize % 3] == expected, "menu frame {n}");
    }
}
