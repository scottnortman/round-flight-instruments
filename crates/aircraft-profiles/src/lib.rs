//! Host-owned aircraft profiles. This crate has no device/simulator connection API.
#![forbid(unsafe_code)]
pub mod preview;
pub mod store;
use instrument_core::device::Uuid;
use serde::{Deserialize, Serialize};
use std::{fmt, io};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_PROFILE_BYTES: usize = 65_536;
pub const MAX_PROFILES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Simulator {
    Msfs2020,
    Msfs2024,
    Dcs,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Aircraft {
    pub simulator: Simulator,
    /// Exact package/add-on identity supplied by the user, not a fuzzy aircraft name.
    pub package: String,
    pub variant: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AirspeedScale {
    Prototype,
    Linear,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirspeedLimits {
    pub vs0: f32,
    pub vs1: f32,
    pub vfe: f32,
    pub vno: f32,
    pub vne: f32,
}
impl AirspeedLimits {
    pub fn render(self) -> instrument_ui::airspeed::Limits {
        instrument_ui::airspeed::Limits {
            white: [self.vs0, self.vfe],
            green: [self.vs1, self.vno],
            yellow: [self.vno, self.vne],
            red: self.vne,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Airspeed {
    pub full_scale_kias: u16,
    pub scale: AirspeedScale,
    pub limits: Option<AirspeedLimits>,
    pub limits_reference: String,
}
impl Airspeed {
    pub fn points(&self) -> Vec<(f32, f32)> {
        match self.scale {
            AirspeedScale::Prototype => instrument_ui::airspeed::SCALE
                .iter()
                .map(|&(kt, a)| (kt * self.full_scale_kias as f32 / 200., a))
                .collect(),
            AirspeedScale::Linear => vec![(0., 0.), (self.full_scale_kias as f32, 342.)],
        }
    }
    pub fn validate(&self) -> Result<(), ValidationError> {
        if !(40..=1000).contains(&self.full_scale_kias) || self.full_scale_kias % 10 != 0 {
            return Err(invalid(
                "Airspeed full scale must be 40–1000 KIAS in steps of 10.",
            ));
        }
        text(&self.limits_reference, 512, "Limits reference", false)?;
        if let Some(l) = self.limits {
            if [l.vs0, l.vs1, l.vfe, l.vno, l.vne]
                .iter()
                .any(|v| !v.is_finite())
                || !(0. < l.vs0
                    && l.vs0 <= l.vs1
                    && l.vs1 < l.vno
                    && l.vs0 < l.vfe
                    && l.vfe <= l.vne
                    && l.vno < l.vne)
            {
                return Err(invalid(
                    "Check limits: 0 < Vs0 ≤ Vs1 < Vno < Vne, and Vs0 < Vfe ≤ Vne.",
                ));
            }
            text(&self.limits_reference, 512, "Limits reference", true)?;
        }
        let points = self.points();
        if instrument_ui::airspeed::Profile::new(&points, self.limits.map(AirspeedLimits::render))
            .is_none()
        {
            return Err(invalid("Airspeed limits must fit the configured scale."));
        }
        Ok(())
    }
}
// Only units/references actually supported by the current faces are accepted.
// Future unit modes require a versioned schema and matching render/data semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AltitudeUnit {
    Feet,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PressureUnit {
    Inhg,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AngleUnit {
    Degrees,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadingReference {
    Magnetic,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Altimeter {
    pub altitude_unit: AltitudeUnit,
    pub pressure_unit: PressureUnit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attitude {
    pub angle_unit: AngleUnit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Heading {
    pub reference: HeadingReference,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Turn {
    pub standard_rate_deg_per_sec: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerticalSpeed {
    pub full_scale_fpm: u16,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gauges {
    pub airspeed: Airspeed,
    pub altimeter: Altimeter,
    pub attitude: Attitude,
    pub heading: Heading,
    pub turn: Turn,
    pub vertical_speed: VerticalSpeed,
}
impl Gauges {
    pub fn validate(&self) -> Result<(), ValidationError> {
        self.airspeed.validate()?;
        let max = self.vertical_speed.full_scale_fpm;
        if !(500..=10000).contains(&max) || max % 100 != 0 {
            return Err(invalid(
                "VSI full scale must be 500–10000 ft/min in steps of 100.",
            ));
        }
        if self.turn.standard_rate_deg_per_sec != 3 {
            return Err(invalid(
                "The current turn coordinator requires a 3°/second standard rate.",
            ));
        }
        Ok(())
    }
}
impl Default for Gauges {
    fn default() -> Self {
        Self {
            airspeed: Airspeed {
                full_scale_kias: 200,
                scale: AirspeedScale::Prototype,
                limits: None,
                limits_reference: String::new(),
            },
            altimeter: Altimeter {
                altitude_unit: AltitudeUnit::Feet,
                pressure_unit: PressureUnit::Inhg,
            },
            attitude: Attitude {
                angle_unit: AngleUnit::Degrees,
            },
            heading: Heading {
                reference: HeadingReference::Magnetic,
            },
            turn: Turn {
                standard_rate_deg_per_sec: 3,
            },
            vertical_speed: VerticalSpeed {
                full_scale_fpm: 2000,
            },
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub schema_version: u32,
    pub profile_uuid: String,
    /// Store assigns monotonically increasing revisions after a successful save.
    pub revision: u64,
    pub name: String,
    pub aircraft: Aircraft,
    pub gauges: Gauges,
}
impl Profile {
    pub fn new(id: Uuid) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            profile_uuid: id.to_string(),
            revision: 0,
            name: "New aircraft".into(),
            aircraft: Aircraft {
                simulator: Simulator::Msfs2020,
                package: String::new(),
                variant: String::new(),
            },
            gauges: Gauges::default(),
        }
    }
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(invalid("Unsupported aircraft profile schema version."));
        }
        if Uuid::parse(&self.profile_uuid).is_none() {
            return Err(invalid("Invalid profile UUID."));
        }
        text(&self.name, 100, "Profile name", true)?;
        text(&self.aircraft.package, 160, "Aircraft package", true)?;
        text(&self.aircraft.variant, 160, "Aircraft variant", true)?;
        self.gauges.validate()
    }
    pub fn from_json(bytes: &[u8]) -> Result<Self, ValidationError> {
        if bytes.len() > MAX_PROFILE_BYTES {
            return Err(invalid("Profile exceeds the 64 KiB size limit."));
        }
        let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
        let mut result: Self = serde_json::from_slice(bytes)
            .map_err(|e| invalid(&format!("Invalid profile JSON: {e}")))?;
        result.validate()?;
        result.profile_uuid = Uuid::parse(&result.profile_uuid).unwrap().to_string();
        Ok(result)
    }
    pub fn live(&self) -> Result<instrument_core::gauge_profile::Profile, ValidationError> {
        self.validate()?;
        if self.aircraft.simulator!=Simulator::Msfs2020 {return Err(invalid("Live profile delivery currently supports MSFS 2020 only."));}
        let p=instrument_core::gauge_profile::Profile {id:Uuid::parse(&self.profile_uuid).unwrap(),revision:self.revision,
            airspeed_max:self.gauges.airspeed.full_scale_kias,linear:self.gauges.airspeed.scale==AirspeedScale::Linear,
            limits:self.gauges.airspeed.limits.map(|l|[l.vs0,l.vs1,l.vfe,l.vno,l.vne]),vsi_max:self.gauges.vertical_speed.full_scale_fpm};
        if !p.valid() {return Err(invalid("Save a valid profile before applying it."));}Ok(p)
    }
    pub fn to_json(&self) -> Result<Vec<u8>, ValidationError> {
        self.validate()?;
        let mut bytes = serde_json::to_vec_pretty(self).map_err(|e| invalid(&e.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError(pub String);
impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ValidationError {}
impl From<ValidationError> for io::Error {
    fn from(value: ValidationError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, value)
    }
}
fn invalid(message: &str) -> ValidationError {
    ValidationError(message.into())
}
fn text(value: &str, max: usize, name: &str, required: bool) -> Result<(), ValidationError> {
    if (required && value.trim().is_empty())
        || value.chars().count() > max
        || value.chars().any(char::is_control)
    {
        return Err(invalid(&format!(
            "{name} must {}contain at most {max} characters and no control characters.",
            if required { "be provided, " } else { "" }
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn live_delivery_requires_a_saved_msfs2020_profile() {
        let mut p=fixture();p.revision=0;assert!(p.live().is_err());
        p.revision=1;assert!(p.live().is_ok());
        for simulator in [Simulator::Msfs2024,Simulator::Dcs] {p.aircraft.simulator=simulator;assert!(p.live().is_err());}
    }
    use super::*;
    pub fn fixture() -> Profile {
        let mut p = Profile::new(Uuid::v4([1; 16]));
        p.name = "Test aircraft".into();
        p.aircraft.package = "example.package".into();
        p.aircraft.variant = "exact-variant".into();
        p
    }
    #[test]
    fn full_six_schema_roundtrips_and_requires_exact_identity_without_assumed_limits() {
        let p = fixture();
        assert_eq!(Profile::from_json(&p.to_json().unwrap()).unwrap(), p);
        assert!(p.gauges.airspeed.limits.is_none());
        for field in [0, 1, 2] {
            let mut bad = p.clone();
            match field {
                0 => bad.name.clear(),
                1 => bad.aircraft.package.clear(),
                _ => bad.aircraft.variant.clear(),
            };
            assert!(bad.validate().is_err());
        }
        let mut newer = p.clone();
        newer.schema_version = 2;
        assert!(newer.validate().is_err());
        assert!(Profile::from_json(&vec![b' '; MAX_PROFILE_BYTES + 1]).is_err());
        let mut value = serde_json::to_value(p).unwrap();
        value["gauges"]["heading"]["reference"] = "true".into();
        assert!(Profile::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    #[test]
    fn limits_require_source_finite_order_and_fit_including_no_silent_range_clamping() {
        let mut a = Gauges::default().airspeed;
        a.limits = Some(AirspeedLimits {
            vs0: 40.,
            vs1: 48.,
            vfe: 85.,
            vno: 129.,
            vne: 163.,
        });
        assert!(a.validate().is_err());
        a.limits_reference = "TEST FIXTURE - not aircraft limits".into();
        assert!(a.validate().is_ok());
        for change in 0..5 {
            let mut bad = a.clone();
            let l = bad.limits.as_mut().unwrap();
            match change {
                0 => l.vs0 = f32::NAN,
                1 => l.vne = 201.,
                2 => l.vs1 = 150.,
                3 => l.vfe = 170.,
                _ => l.vs0 = 60.,
            };
            assert!(bad.validate().is_err());
        }
        a.full_scale_kias = 150;
        assert!(a.validate().is_err());
        let mut g = Gauges::default();
        g.vertical_speed.full_scale_fpm = 1500;
        assert!(g.validate().is_ok());
        g.vertical_speed.full_scale_fpm = 1510;
        assert!(g.validate().is_err());
    }
    #[test]
    fn hostile_or_unsupported_imports_are_rejected() {
        let mut p = fixture();
        p.profile_uuid = "../../outside".into();
        assert!(p.validate().is_err());
        p = fixture();
        p.name = "bad\nname".into();
        assert!(p.validate().is_err());
        let mut json = serde_json::to_value(fixture()).unwrap();
        json["surprise"] = true.into();
        assert!(Profile::from_json(&serde_json::to_vec(&json).unwrap()).is_err());
        assert!(Profile::from_json(br#"{"schema_version":1,"schema_version":2}"#).is_err());
    }
    #[test]
    fn windows_utf8_bom_and_uuid_case_are_normalized_without_relaxing_schema() {
        let p = fixture();
        let mut value = serde_json::to_value(&p).unwrap();
        value["profile_uuid"] = p.profile_uuid.to_ascii_uppercase().into();
        let mut bytes = vec![0xef, 0xbb, 0xbf];
        bytes.extend_from_slice(&serde_json::to_vec(&value).unwrap());
        assert_eq!(Profile::from_json(&bytes).unwrap(), p);
    }
}
