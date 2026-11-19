//! Deterministic desktop-only preview. Never reads or writes simulator/device data.
use crate::{Gauges, ValidationError};
use instrument_core::{App, Command, Field, Instrument};
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub first: f32,
    pub second: f32,
}
impl Sample {
    pub fn defaults(gauge: Instrument) -> Self {
        let (first, second) = match gauge {
            Instrument::Altimeter => (4520., 29.92),
            Instrument::Attitude | Instrument::Consolidated | Instrument::Debug => (5., 20.),
            Instrument::Airspeed => (85., 0.),
            Instrument::Heading => (90., 0.),
            Instrument::Turn => (3., 0.),
            Instrument::Vsi => (1000., 0.),
        };
        Self { first, second }
    }
}
pub fn render(
    gauges: &Gauges,
    gauge: Instrument,
    sample: Sample,
    pixels: &mut [u16],
    size: u32,
) -> Result<(), ValidationError> {
    gauges.validate()?;
    let mut app = App::new();
    app.apply(Command::LevelPreset).unwrap();
    let mut set = |field, value| {
        app.apply(Command::Set(field, value))
            .map_err(|_| ValidationError("Preview value is out of range.".into()))
    };
    match gauge {
        Instrument::Altimeter => {
            set(Field::PressureAltitudeFt, sample.first)?;
            app.apply(Command::Baro(sample.second)).map_err(|_| {
                ValidationError("Preview barometer must be 28.10–31.00 inHg.".into())
            })?;
        }
        Instrument::Attitude | Instrument::Consolidated | Instrument::Debug => {
            set(Field::PitchDeg, sample.first)?;
            set(Field::RollDeg, sample.second)?;
        }
        // The preview supports configured ranges beyond the local CLI's 200-kt bench limit.
        Instrument::Airspeed => {
            if !sample.first.is_finite() || !(0.0..=10000.).contains(&sample.first) {
                return Err(ValidationError("Preview IAS must be 0–10000 KIAS.".into()));
            }
        }
        Instrument::Heading => set(Field::HeadingDeg, sample.first)?,
        Instrument::Turn => {
            set(Field::TurnDps, sample.first)?;
            set(Field::Ball, sample.second)?;
        }
        Instrument::Vsi => {
            if !instrument_core::simulator_link::measurements::Quantity::VerticalSpeedFpm
                .accepts(sample.first as f64)
            {
                return Err(ValidationError(
                    "Preview vertical speed is outside its supported range.".into(),
                ));
            }
        }
    }
    app.apply(Command::Select(gauge)).unwrap();
    app.tick(0);
    let mut snapshot = instrument_ui::Snapshot::from_app(&app);
    snapshot.source_label = "PREVIEW";
    if gauge == Instrument::Airspeed {
        snapshot.ias = instrument_core::Reading {
            value: Some(sample.first),
            quality: instrument_core::Quality::Valid,
            age_ms: Some(0),
        };
    }
    if gauge == Instrument::Vsi {
        snapshot.vsi = instrument_core::Reading {
            value: Some(sample.first),
            quality: instrument_core::Quality::Valid,
            age_ms: Some(0),
        };
    }
    let mut profile=crate::Profile::new(instrument_core::device::Uuid::parse("00000000-0000-4000-8000-000000000001").unwrap());
    profile.aircraft.package="preview".into();profile.aircraft.variant="preview".into();profile.revision=1;profile.gauges=gauges.clone();
    snapshot.gauge_profile=Some(profile.live()?);
    if !instrument_ui::render::draw_frame(pixels,size,size,snapshot,false) {
        return Err(ValidationError("Invalid preview dimensions.".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delivered_profile_matches_preview_across_three_buffers_and_revisions() {
        use instrument_ui::{Snapshot,render::{draw_frame,CachedRenderer},damage::FrameHistory};
        use instrument_core::{gauge_profile,simulator_link::{Provider,Lifecycle,multi,measurements::{Measurements,Measurement,Key,Quantity,Value}}};
        let mut p=crate::Profile::new(instrument_core::device::Uuid::parse("00000000-0000-4000-8000-000000000001").unwrap());
        p.aircraft.package="test".into();p.aircraft.variant="test".into();p.revision=1;
        p.gauges.airspeed.limits=Some(crate::AirspeedLimits {vs0:45.,vs1:54.,vfe:100.,vno:125.,vne:165.});p.gauges.airspeed.limits_reference="Test fixture".into();
        let mut app=App::new();app.tick(0);
        let mut backdrop=vec![0;480*480];let mut renderer=CachedRenderer::new(&mut backdrop,480,480).unwrap();
        let mut buffers=[vec![0;480*480],vec![0;480*480],vec![0;480*480]];
        let mut histories=core::array::from_fn::<_,3,_>(|_|FrameHistory::new());
        for session in 1..=3 {
            app.apply(Command::SourceExternal2(Provider::Synthetic,session)).unwrap();
            if session==2 {p.revision=2;p.gauges.airspeed.scale=crate::AirspeedScale::Linear;p.gauges.airspeed.full_scale_kias=300;p.gauges.vertical_speed.full_scale_fpm=4000;}
            if session==3 {p.revision=3;p.gauges.airspeed.limits=None;}
            let mut packet=String::new();p.live().unwrap().encode(session,&mut packet).unwrap();
            let response=text_cli::execute(&mut app,text_cli::parse(&packet),0);assert!(response.as_str().starts_with("OK gauge_profile"));
            let mut measurements=Measurements::new();
            for (quantity,value) in [(Quantity::IndicatedAirspeedKt,85.),(Quantity::VerticalSpeedFpm,1000.)] {
                measurements.push(Measurement{key:Key::primary(quantity),value:Value::Valid(value),age_ms:0}).unwrap();
            }
            app.apply(Command::Telemetry2(multi::Frame{session,sequence:1,receiver_ms:0,state:Lifecycle::Running,measurements})).unwrap();
            assert!(app.apply(Command::GaugeProfile(gauge_profile::Command::Set(session,p.live().unwrap()))).is_err(),"cannot reconfigure during live telemetry");
            for gauge in [Instrument::Airspeed,Instrument::Vsi] {
                app.apply(Command::Select(gauge)).unwrap();let mut s=Snapshot::from_app(&app);s.source_label="PREVIEW";
                let mut expected=vec![0;480*480];render(&p.gauges,gauge,Sample::defaults(gauge),&mut expected,480).unwrap();
                let mut complete=vec![0;480*480];draw_frame(&mut complete,480,480,s,false);assert_eq!(complete,expected,"preview vs delivered {gauge:?}");
                for b in 0..3 {assert!(renderer.draw_incremental(&mut buffers[b],&mut histories[b],s,false));assert_eq!(buffers[b],expected,"retained {gauge:?}, session {session}, buffer {b}");}
            }
            assert!(app.control_request().is_none());
        }
    }
    #[test]
    fn default_configured_faces_match_existing_instrument_pixels() {
        let gauges = Gauges::default();
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.tick(0);
        for gauge in [Instrument::Airspeed, Instrument::Vsi] {
            let sample = Sample::defaults(gauge);
            let mut expected = vec![0; 480 * 480];
            let mut actual = expected.clone();
            let mut snapshot = instrument_ui::Snapshot::from_app(&app);
            snapshot.selected = gauge;
            snapshot.external = true;
            snapshot.external_attitude = true;
            snapshot.source_label = "PREVIEW";
            if gauge == Instrument::Airspeed {
                snapshot.ias.value = Some(sample.first);
            } else {
                snapshot.vsi.value = Some(sample.first);
            }
            assert!(instrument_ui::render::draw_frame(
                &mut expected,
                480,
                480,
                snapshot,
                false
            ));
            render(&gauges, gauge, sample, &mut actual, 480).unwrap();
            let diffs: Vec<_> = actual
                .iter()
                .zip(&expected)
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(n, (a, b))| (n % 480, n / 480, *a, *b))
                .take(20)
                .collect();
            assert!(diffs.is_empty(), "{gauge:?}: {diffs:?}");
        }
    }
    #[test]
    fn all_six_previews_render_and_profile_changes_change_actual_scale_pixels() {
        let mut gauges = Gauges::default();
        let mut pixels = vec![0; 240 * 240];
        for gauge in Instrument::ALL {
            render(&gauges, gauge, Sample::defaults(gauge), &mut pixels, 240).unwrap();
            assert!(pixels.iter().any(|p| *p != 0));
        }
        render(
            &gauges,
            Instrument::Airspeed,
            Sample::defaults(Instrument::Airspeed),
            &mut pixels,
            240,
        )
        .unwrap();
        let old = pixels.clone();
        gauges.airspeed.full_scale_kias = 400;
        render(
            &gauges,
            Instrument::Airspeed,
            Sample {
                first: 300.,
                second: 0.,
            },
            &mut pixels,
            240,
        )
        .unwrap();
        assert_ne!(pixels, old);
        render(
            &gauges,
            Instrument::Vsi,
            Sample::defaults(Instrument::Vsi),
            &mut pixels,
            240,
        )
        .unwrap();
        let old = pixels.clone();
        gauges.vertical_speed.full_scale_fpm = 4000;
        render(
            &gauges,
            Instrument::Vsi,
            Sample::defaults(Instrument::Vsi),
            &mut pixels,
            240,
        )
        .unwrap();
        assert_ne!(pixels, old);
        assert!(render(
            &gauges,
            Instrument::Attitude,
            Sample {
                first: f32::NAN,
                second: 0.
            },
            &mut pixels,
            240
        )
        .is_err());
    }
}
