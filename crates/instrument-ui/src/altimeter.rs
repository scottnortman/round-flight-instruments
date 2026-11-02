use instrument_core::{Field, Quality, Reading};

/// Deliberately preserves the browser's simulated barometric correction model.
/// This is not calibrated air-data/ISA inversion for an operational altimeter.
pub fn simulated_indicated_ft(pressure_ft: f32, baro_inhg: f32) -> Option<f32> {
    if !Field::PressureAltitudeFt.accepts(pressure_ft)
        || !baro_inhg.is_finite()
        || !(28.1..=31.).contains(&baro_inhg)
    {
        return None;
    }
    Some(
        pressure_ft
            + (44330.76923076923f64 / 0.3048
                * (libm::pow(baro_inhg as f64 / 29.92f32 as f64, 0.1902632365084836) - 1.))
                as f32,
    )
}
#[derive(Clone, Copy, Debug)]
pub struct Altimeter {
    pub indicated_ft: Option<f32>,
    /// Clockwise degrees from twelve o'clock: hundreds, thousands, ten-thousands.
    pub hands_deg: Option<[f32; 3]>,
    pub quality: Quality,
}
pub fn model(reading: Reading, baro: f32) -> Altimeter {
    model_with_semantics(reading, baro, false)
}
pub fn model_with_semantics(reading: Reading, baro: f32, already_indicated: bool) -> Altimeter {
    let mut quality = reading.quality;
    let altitude = if quality == Quality::Valid && reading.age_ms.is_some_and(|age| age < 1000) {
        reading.value.and_then(|p| {
            if already_indicated {
                if p.is_finite() && (-3000.0..=65000.0).contains(&p) {
                    Some(p)
                } else {
                    None
                }
            } else {
                simulated_indicated_ft(p, baro)
            }
        })
    } else {
        None
    };
    if quality == Quality::Valid && altitude.is_none() {
        quality = Quality::Failed;
    }
    let hands = altitude.map(|a| {
        [1000., 10000., 100000.].map(|period| {
            let turns = a / period;
            (turns - libm::floorf(turns)) * 360.
        })
    });
    Altimeter {
        indicated_ft: altitude,
        hands_deg: hands,
        quality,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn r(v: f32) -> Reading {
        Reading {
            value: Some(v),
            quality: Quality::Valid,
            age_ms: Some(0),
        }
    }
    #[test]
    fn external_indication_is_never_corrected_twice() {
        assert_eq!(
            model_with_semantics(r(8500.), 30.12, true).indicated_ft,
            Some(8500.)
        );
        assert_eq!(
            model_with_semantics(r(8500.), f32::NAN, true).indicated_ft,
            Some(8500.)
        );
        assert_ne!(model(r(8500.), 30.12).indicated_ft, Some(8500.));
        let stale = Reading {
            age_ms: Some(1000),
            ..r(8500.)
        };
        assert!(model_with_semantics(stale, 30.12, true).hands_deg.is_none());
    }
    #[test]
    fn reference_positions_wrap_and_negative_altitude() {
        let m = model(r(4520.), 29.92);
        assert_eq!(m.indicated_ft, Some(4520.));
        for (got, expected) in m
            .hands_deg
            .unwrap()
            .into_iter()
            .zip([187.2, 162.72, 16.272])
        {
            assert!((got - expected).abs() < 0.001);
        }
        assert_eq!(model(r(1000.), 29.92).hands_deg.unwrap()[0], 0.);
        assert!((model(r(-100.), 29.92).hands_deg.unwrap()[0] - 324.).abs() < 0.001);
        assert_eq!(model(r(0.), 29.92).hands_deg.unwrap(), [0.; 3]);
    }
    #[test]
    fn reference_change_is_applied_once_and_invalid_data_has_no_hands() {
        let high = simulated_indicated_ft(0., 30.92).unwrap();
        assert!((high - 912.187).abs() < 1.);
        assert!(simulated_indicated_ft(0., 28.92).unwrap() < 0.);
        for q in [
            Quality::Missing,
            Quality::Stale,
            Quality::Failed,
            Quality::TimingFault,
        ] {
            let mut v = r(4520.);
            v.quality = q;
            assert!(model(v, 29.92).hands_deg.is_none());
        }
        for v in [f32::NAN, f32::INFINITY, 60001.] {
            assert!(model(r(v), 29.92).hands_deg.is_none());
        }
        let mut v = r(0.);
        v.age_ms = Some(1000);
        assert!(model(v, 29.92).hands_deg.is_none());
    }
}
