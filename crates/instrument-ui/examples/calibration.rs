//! Production pixels for calibration guidance, review, and error states.
use instrument_core::{App,Command,calibration::{View as SensorView,Phase,Reason}};
use instrument_ui::{Snapshot,calibration::View,render::draw_frame};
use std::{fs,io::{self,Write}};
fn main()->io::Result<()> {
    fs::create_dir_all("output/calibration")?;
    let mut app=App::new();app.apply(Command::LevelPreset).unwrap();app.tick(0);
    let mut s=Snapshot::from_app(&app);
    s.menu=Some(instrument_ui::navigation::View {info:false,generation:1,context:instrument_ui::navigation::Context::from_app(&app),pressed:None});
    save("menu",s)?;s.menu=None;
    for (name,phase,choose,pose) in [("choose",Phase::Idle,true,0),("warmup",Phase::Warmup,false,0),
        ("pose",Phase::Collect,false,18),("level",Phase::Level,false,24),("review",Phase::Review,false,24),
        ("saving",Phase::Saving,false,24),("saved",Phase::Saved,false,24),("failed",Phase::Failed,false,8)] {
        s.calibration=Some(View {generation:1,choose,awaiting:false,available:true,pressed:None,command_failed:false,
            status:SensorView {job:5,phase,pose,full:true,progress:65,reason:if phase==Phase::Failed {Reason::Geometry}else{Reason::Still},
                high_bias:true,gyro_bias_milli:[-1590,30120,-80],noise_milli:81,rms_milli:3,storage_available:true,..SensorView::default()}});
        save(name,s)?;
    }
    Ok(())
}
fn save(name:&str,s:Snapshot)->io::Result<()> {
    let mut pixels=vec![0;480*480];assert!(draw_frame(&mut pixels,480,480,s,false));
    let mut f=fs::File::create(format!("output/calibration/{name}.ppm"))?;write!(f,"P6\n480 480\n255\n")?;
    for p in pixels {f.write_all(&[(((p>>11)&31)*255/31)as u8,(((p>>5)&63)*255/63)as u8,((p&31)*255/31)as u8])?;}
    Ok(())
}
