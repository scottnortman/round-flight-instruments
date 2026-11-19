//! Atomic, session-bound display configuration. Never changes aircraft controls.
use crate::device::Uuid;
use core::fmt::{self, Write};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    pub id: Uuid,
    pub revision: u64,
    pub airspeed_max: u16,
    pub linear: bool,
    /// Vs0, Vs1, Vfe, Vno, Vne in KIAS. None means no aircraft limits asserted.
    pub limits: Option<[f32; 5]>,
    pub vsi_max: u16,
}
impl Profile {
    pub fn valid(self) -> bool {
        self.revision > 0 && (40..=1000).contains(&self.airspeed_max)
            && self.airspeed_max % 10 == 0
            && (500..=10000).contains(&self.vsi_max) && self.vsi_max % 100 == 0
            && self.limits.is_none_or(|[vs0,vs1,vfe,vno,vne]| {
                [vs0,vs1,vfe,vno,vne].iter().all(|v|v.is_finite()) &&
                0. < vs0 && vs0 <= vs1 && vs1 < vno && vs0 < vfe &&
                vfe <= vne && vno < vne && vne <= self.airspeed_max as f32
            })
    }
    pub fn encode(self, session: u64, out: &mut impl Write) -> fmt::Result {
        let mut body = Packet::default();
        write!(body,"G1 {session} {} {} {} {}",self.id,self.revision,self.airspeed_max,if self.linear {"linear"}else{"prototype"})?;
        if let Some(l)=self.limits {for v in l {write!(body," {v}")?;}}
        else {body.write_str(" - - - - -")?;}
        write!(body," {}",self.vsi_max)?;
        write!(out,"{} {:08X}",body.as_str(),crc(body.as_str().as_bytes()))
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command { Set(u64, Profile), Status }

pub fn decode(line: &str) -> Option<Command> {
    if line.len()>256 || !line.bytes().all(|b|b.is_ascii_graphic() || b==b' ') {return None;}
    let (body,sum)=line.rsplit_once(' ')?;
    if sum.len()!=8 || u32::from_str_radix(sum,16).ok()?!=crc(body.as_bytes()) {return None;}
    let mut w=body.split_ascii_whitespace();
    if w.next()? != "G1" {return None;}
    let session=w.next()?.parse().ok()?;
    let id=Uuid::parse(w.next()?)?;
    let revision=w.next()?.parse().ok()?;
    let airspeed_max=w.next()?.parse().ok()?;
    let linear=match w.next()? {"linear"=>true,"prototype"=>false,_=>return None};
    let fields=[w.next()?,w.next()?,w.next()?,w.next()?,w.next()?];
    let limits=if fields==["-";5] {None} else {
        let mut values=[0.;5];for (v,s) in values.iter_mut().zip(fields) {*v=s.parse().ok()?;}Some(values)
    };
    let p=Profile {id,revision,airspeed_max,linear,limits,vsi_max:w.next()?.parse().ok()?};
    if w.next().is_some() || session==0 || !p.valid() {return None;}
    Some(Command::Set(session,p))
}
fn crc(b:&[u8])->u32 {let mut c=!0u32;for b in b {c^=*b as u32;for _ in 0..8 {c=(c>>1)^(0xedb88320 & (0u32.wrapping_sub(c&1)));}}!c}
struct Packet {bytes:[u8;256],len:usize}
impl Default for Packet {fn default()->Self {Self {bytes:[0;256],len:0}}}
impl Packet {fn as_str(&self)->&str {core::str::from_utf8(&self.bytes[..self.len]).unwrap()}}
impl Write for Packet {fn write_str(&mut self,s:&str)->fmt::Result {let end=self.len+s.len();if end>self.bytes.len(){return Err(fmt::Error);}self.bytes[self.len..end].copy_from_slice(s.as_bytes());self.len=end;Ok(())}}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_roundtrip_rejects_corruption_bad_limits_and_cross_session_configuration() {
        let p=Profile {id:Uuid::parse("00000000-0000-4000-8000-000000000001").unwrap(),revision:4,airspeed_max:200,linear:false,limits:Some([45.,54.,100.,125.,165.]),vsi_max:3000};
        let mut b=Packet::default();p.encode(7,&mut b).unwrap();assert_eq!(decode(b.as_str()),Some(Command::Set(7,p)));
        let body=b.as_str().rsplit_once(' ').unwrap().0;
        let mut large=[b' ';300];large[..body.len()].copy_from_slice(body.as_bytes());
        let mut sum=Packet::default();write!(sum,"{:08X}",crc(&large[..291])).unwrap();large[292..].copy_from_slice(sum.as_str().as_bytes());
        assert!(decode(core::str::from_utf8(&large).unwrap()).is_none());
        b.bytes[4]=b'8';assert_eq!(decode(b.as_str()),None);
        let mut a=crate::App::new();a.tick(0);a.apply(crate::Command::SourceExternal2(crate::simulator_link::Provider::Synthetic,7)).unwrap();
        assert!(a.apply(crate::Command::GaugeProfile(Command::Set(8,p))).is_err());assert!(a.gauge_profile().is_none());
        a.apply(crate::Command::GaugeProfile(Command::Set(7,p))).unwrap();assert_eq!(a.gauge_profile(),Some(p));
        a.apply(crate::Command::SourceExternal2(crate::simulator_link::Provider::Synthetic,8)).unwrap();assert!(a.gauge_profile().is_none());
        let mut bad=p;bad.limits=Some([45.,126.,100.,125.,165.]);assert!(!bad.valid());
    }
}
