//! Shared bounded clock/curve engine for single-value bench demonstrations.
use crate::takeoff::State;
#[derive(Clone, Copy, Debug)]
enum Kind {
    Airspeed,
    Heading,
    Vsi,
}
#[derive(Clone, Copy, Debug)]
pub struct Demo {
    pub state: State,
    pub elapsed_ms: u64,
    pub duration_ms: u64,
    started_ms: u64,
    kind: Kind,
}
impl Default for Demo {
    fn default() -> Self {
        Self::new()
    }
}
impl Demo {
    pub const fn new() -> Self {
        Self {
            state: State::Idle,
            elapsed_ms: 0,
            duration_ms: 60000,
            started_ms: 0,
            kind: Kind::Airspeed,
        }
    }
    pub const fn heading() -> Self {
        Self {
            kind: Kind::Heading,
            ..Self::new()
        }
    }
    pub const fn vsi() -> Self {
        Self {
            kind: Kind::Vsi,
            ..Self::new()
        }
    }
    pub fn running(&self) -> bool {
        self.state == State::Running
    }
    pub(crate) fn start(&mut self, now: u64, seconds: u32) {
        self.state = State::Running;
        self.elapsed_ms = 0;
        self.duration_ms = seconds as u64 * 1000;
        self.started_ms = now;
    }
    pub(crate) fn stop(&mut self, fault: bool) {
        if self.running() {
            self.state = if fault {
                State::Faulted
            } else {
                State::Stopped
            };
        }
    }
    pub(crate) fn advance(&mut self, now: u64) -> Option<f32> {
        if !self.running() {
            return None;
        }
        let Some(elapsed) = now.checked_sub(self.started_ms) else {
            self.stop(true);
            return None;
        };
        self.elapsed_ms = elapsed.min(self.duration_ms);
        let keys: &[f32] = match self.kind {
            Kind::Airspeed => &[0., 0., 60., 85., 120., 150., 170., 200., 120., 85.],
            Kind::Heading => &[350., 350., 370., 450., 540., 720., 540., 360., 350.],
            Kind::Vsi => &[0., 500., 1000., 2000., 1000., 0., -500., -1000., -2000., 0.],
        };
        if self.elapsed_ms == self.duration_ms {
            self.state = State::Complete;
            return keys.last().copied();
        }
        let pos = self.elapsed_ms as f32 / self.duration_ms as f32 * (keys.len() - 1) as f32;
        let i = (pos as usize).min(keys.len() - 2);
        let t = pos - i as f32;
        let ease = t * t * (3. - 2. * t);
        let value = keys[i] + (keys[i + 1] - keys[i]) * ease;
        Some(if matches!(self.kind, Kind::Heading) {
            value % 360.
        } else {
            value
        })
    }
}
