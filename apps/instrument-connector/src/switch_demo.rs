//! Explicit offline session-switch fixture. Always labeled synthetic/TEST INPUT.
use sim_bridge::{Capabilities, Observation, RecoveryReason, SimulatorProvider, Synthetic};
use simulator_link::{Datum, Lifecycle, Provider};
use std::io;

pub fn next(index: &mut u32) -> io::Result<Box<dyn SimulatorProvider>> {
    let phase = match *index {
        0 => 0,
        1 => {
            *index += 1;
            return Err(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                "synthetic switch demo: temporary source unavailability",
            ));
        }
        2 => 1,
        3 => 2,
        _ => return Err(io::Error::other("switch demo complete")),
    };
    let provider = Demo {
        inner: Synthetic::new(2000)?,
        index: phase,
        ended: false,
    };
    *index += 1;
    Ok(Box::new(provider))
}
struct Demo {
    inner: Synthetic,
    index: u32,
    ended: bool,
}
impl SimulatorProvider for Demo {
    fn kind(&self) -> Provider {
        Provider::Synthetic
    }
    fn capabilities(&self) -> Capabilities<'_> {
        self.inner.capabilities()
    }
    fn poll(&mut self, now: u64) -> Option<Observation> {
        let observation = self.inner.poll(now)?;
        if observation.state == Lifecycle::Disconnected {
            self.ended = true;
            return Some(observation);
        }
        let frame =
            sim_bridge::to_frame(observation, 1, 1, now).expect("bounded synthetic profile");
        let shift = |datum| match datum {
            Datum::Value(v) => Datum::Value(v + self.index as f32 * 5000.),
            v => v,
        };
        Some(
            Observation::altimeter(
                observation.at_ms,
                observation.state,
                shift(frame.indicated_ft),
                shift(frame.pressure_ft),
                Datum::Value([29.92, 30.01, 29.80][self.index as usize]),
            )
            .expect("bounded synthetic switch"),
        )
    }
    fn recovery_reason(&self) -> Option<RecoveryReason> {
        (self.ended && self.index < 2).then_some(RecoveryReason::SessionChanged)
    }
}
