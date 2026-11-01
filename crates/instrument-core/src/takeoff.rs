//! Bounded bench altitude animation, driven by the owner's monotonic clock.
pub const GROUND_HOLD_MS: u64 = 1000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Running,
    Complete,
    Stopped,
    Faulted,
}
impl State {
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::Complete => "complete",
            Self::Stopped => "stopped",
            Self::Faulted => "faulted",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Takeoff {
    pub state: State,
    pub elapsed_ms: u64,
    pub climb_ms: u64,
    started_ms: u64,
}
impl Takeoff {
    pub const fn new() -> Self {
        Self {
            state: State::Idle,
            elapsed_ms: 0,
            climb_ms: 60_000,
            started_ms: 0,
        }
    }
    pub fn running(&self) -> bool {
        self.state == State::Running
    }
    pub fn total_ms(&self) -> u64 {
        GROUND_HOLD_MS + self.climb_ms
    }
    pub(crate) fn start(&mut self, now: u64, seconds: u32) {
        *self = Self {
            state: State::Running,
            elapsed_ms: 0,
            climb_ms: seconds as u64 * 1000,
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
    pub(crate) fn advance(&mut self, now: u64) -> Option<f32> {
        if !self.running() {
            return None;
        }
        let Some(elapsed) = now.checked_sub(self.started_ms) else {
            self.stop(true);
            return None;
        };
        self.elapsed_ms = elapsed.min(self.total_ms());
        let climb = self
            .elapsed_ms
            .saturating_sub(GROUND_HOLD_MS)
            .min(self.climb_ms);
        if self.elapsed_ms == self.total_ms() {
            self.state = State::Complete;
        }
        Some(10_000.0 * (climb as f32 / self.climb_ms as f32))
    }
}
impl Default for Takeoff {
    fn default() -> Self {
        Self::new()
    }
}
