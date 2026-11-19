//! Isolated synthetic profile and production-render previews for delivery QA.
use aircraft_profiles::{Profile,AirspeedLimits,preview::{self,Sample}};
use instrument_core::{device::Uuid,Instrument};
use std::{fs,io::{self,Write},path::PathBuf};
fn main()->io::Result<()> {
    let dir=PathBuf::from(std::env::args().nth(1).expect("output directory"));fs::create_dir_all(&dir)?;
    let mut p=Profile::new(Uuid::parse("00000000-0000-4000-8000-000000000013").unwrap());
    p.name="Live delivery test fixture".into();p.aircraft.package="synthetic-test-only".into();p.aircraft.variant="fixture".into();p.revision=1;
    p.gauges.airspeed.limits=Some(AirspeedLimits{vs0:45.,vs1:54.,vfe:100.,vno:125.,vne:165.});
    p.gauges.airspeed.limits_reference="Synthetic test data only; not aircraft operating limits".into();
    fs::write(dir.join(format!("{}.json",p.profile_uuid)),p.to_json()?)?;
    for g in [Instrument::Airspeed,Instrument::Vsi,Instrument::Consolidated] {
        let mut pixels=vec![0;480*480];preview::render(&p.gauges,g,Sample::defaults(g),&mut pixels,480)?;
        let mut f=fs::File::create(dir.join(format!("{}.ppm",g.name())))?;write!(f,"P6\n480 480\n255\n")?;
        for v in pixels {f.write_all(&[(((v>>11)&31)*255/31)as u8,(((v>>5)&63)*255/63)as u8,((v&31)*255/31)as u8])?;}
    }
    Ok(())
}
