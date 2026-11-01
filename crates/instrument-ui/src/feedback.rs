//! Rate/freshness policy for UI-accepted press events, never raw contacts.
use crate::interaction::TOUCH_STALE_MS;

#[derive(Default)]
pub struct PressSound {
    last_beep: Option<u32>,
}
impl PressSound {
    /// The caller consumes each request once. Wrapping clocks are milliseconds;
    /// pending requests expire long before a complete 32-bit clock cycle.
    pub fn update(&mut self, now: u32, requested_at: Option<u32>, enabled: bool) -> bool {
        let fresh = requested_at.is_some_and(|at| now.wrapping_sub(at) < TOUCH_STALE_MS as u32);
        let ready = self
            .last_beep
            .is_none_or(|last| now.wrapping_sub(last) >= 150);
        if enabled && fresh && ready {
            self.last_beep = Some(now);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requests_expire_are_rate_limited_and_never_repeat_without_an_event() {
        let mut sound = PressSound::default();
        assert!(!sound.update(0, None, true));
        assert!(sound.update(20, Some(0), true));
        assert!(!sound.update(40, Some(40), true));
        assert!(!sound.update(200, None, true));
        assert!(!sound.update(220, Some(100), true));
        assert!(!sound.update(240, Some(260), true));
        assert!(!sound.update(260, Some(260), false));
        assert!(!sound.update(280, None, true));
        assert!(sound.update(300, Some(290), true));
    }
    #[test]
    fn request_age_and_cooldown_handle_clock_wrap() {
        let mut sound = PressSound::default();
        assert!(sound.update(u32::MAX - 10, Some(u32::MAX - 20), true));
        assert!(!sound.update(10, Some(0), true));
        assert!(sound.update(160, Some(150), true));
        assert!(!sound.update(300, Some(u32::MAX - 10), true));
    }
}
