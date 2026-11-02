//! Bounded synthetic pitch/bank exercise. No sensor or flight dynamics claim.
use crate::takeoff::State;
#[derive(Clone, Copy, Debug)]
pub struct Demo {
    pub state: State,
    pub elapsed_ms: u64,
    pub duration_ms: u64,
    started_ms: u64,
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
            duration_ms: 60_000,
            started_ms: 0,
        }
    }
    pub fn running(&self) -> bool {
        self.state == State::Running
    }
    pub(crate) fn start(&mut self, now: u64, seconds: u32) {
        *self = Self {
            state: State::Running,
            elapsed_ms: 0,
            duration_ms: seconds as u64 * 1000,
            started_ms: now,
        };
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
    pub(crate) fn advance(&mut self, now: u64) -> Option<[f32; 2]> {
        if !self.running() {
            return None;
        }
        let Some(elapsed) = now.checked_sub(self.started_ms) else {
            self.stop(true);
            return None;
        };
        self.elapsed_ms = elapsed.min(self.duration_ms);
        if self.elapsed_ms == self.duration_ms {
            self.state = State::Complete;
            return Some([0., 0.]);
        }
        let keys = [
            [0., 0.],
            [8., 0.],
            [0., 30.],
            [0., -30.],
            [-6., 0.],
            [0., 0.],
        ];
        let position = self.elapsed_ms as f32 / self.duration_ms as f32 * 5.;
        let i = (position as usize).min(4);
        let t = position - i as f32;
        let ease = t * t * (3. - 2. * t);
        Some(core::array::from_fn(|axis| {
            keys[i][axis] + (keys[i + 1][axis] - keys[i][axis]) * ease
        }))
    }
}
