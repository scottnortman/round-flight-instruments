//! Bounded grace for a late physical presentation, independent of data validity.
//! No timestamps are renewed. Only a confirmed fresh LCD frame resets the run.

pub const DEFAULT_MISSES: u32 = 3;
pub const MAX_MISSES: u32 = 10;

/// A renderer submission alone proves nothing. Carry this ORIGINAL lifetime
/// into the LCD fence; the ISR still has to confirm it before expiry. Repeated
/// pixels and menus cannot renew the miss run. A partial warning view qualifies
/// only for numeric components that are actually valid and displayed.
pub fn fresh_lifetime(snapshot: crate::Snapshot, repeated: bool, pattern: bool) -> Option<u64> {
    if repeated || pattern {
        return None;
    }
    // IMU bench presentation is intentionally unbounded, but only a genuinely
    // fresh IMU estimate (not its retained-pose fallback) may clear a miss run.
    if snapshot.imu_bench && snapshot.selected == instrument_core::Instrument::Attitude {
        if snapshot.attitude_model().quality != instrument_core::Quality::Valid {
            return None;
        }
        return crate::attitude::STALE_MS
            .checked_sub(snapshot.displayed_age_ms()?)
            .filter(|ttl| *ttl > 0);
    }
    snapshot.numeric_lifetime_ms().filter(|ttl| *ttl > 0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    pub allow: bool,
    pub holding: bool,
    pub misses: u32,
    pub new_misses: u32,
    pub started: bool,
}

#[derive(Default)]
pub struct Supervisor {
    fresh_generation: u32,
    seen_fresh: bool,
    episode: Option<(u64, u64)>, // original missed deadline, frame period
    misses: u32,
}
impl Supervisor {
    /// `deadline=None` denotes an intentional nonnumeric view (menu/warning).
    /// `fresh_generation` advances ONLY after a real, still-fresh numeric frame
    /// reaches the LCD. Repeated/stale/invalid frames must not advance it.
    pub fn observe(
        &mut self,
        now: u64,
        deadline: Option<u64>,
        fresh_generation: u32,
        allowed: u32,
        frame_period_ms: u64,
    ) -> Decision {
        if fresh_generation != self.fresh_generation {
            self.fresh_generation = fresh_generation;
            self.seen_fresh = true;
            self.episode = None;
            self.misses = 0;
        }
        let mut result = Decision {
            allow: true,
            holding: false,
            misses: self.misses,
            new_misses: 0,
            started: false,
        };
        let Some(deadline) = deadline else {
            return result;
        };
        if self.episode.is_none() && now < deadline {
            return result;
        }
        if !self.seen_fresh {
            result.allow = false; // no last usable numeric frame exists yet
            return result;
        }
        if self.episode.is_none() {
            // The deadline, not the 10 ms guard poll, anchors missed slots.
            self.episode = Some((deadline, frame_period_ms.clamp(1, 100)));
            result.started = true;
        }
        let (since, period) = self.episode.unwrap();
        let missed = now.checked_sub(since).map_or(u32::MAX, |age| {
            (age / period).saturating_add(1).min(u32::MAX as u64) as u32
        });
        result.new_misses = missed.saturating_sub(self.misses);
        self.misses = self.misses.max(missed);
        result.misses = self.misses;
        result.allow = self.seen_fresh && self.misses <= allowed.min(MAX_MISSES);
        result.holding = result.allow;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_valid_numeric_frames_can_carry_freshness_to_the_lcd_fence() {
        use instrument_core::{App, Command, Instrument, Quality};
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.tick(0);
        for gauge in crate::navigation::GAUGES {
            app.apply(Command::Select(gauge)).unwrap();
            let s = crate::Snapshot::from_app(&app);
            assert!(fresh_lifetime(s, false, false).is_some());
            assert_eq!(fresh_lifetime(s, true, false), None);
            assert_eq!(fresh_lifetime(s, false, true), None);
        }
        app.apply(Command::Select(Instrument::Attitude)).unwrap();
        let mut s = crate::Snapshot::from_app(&app);
        s.pitch.quality = Quality::Failed;
        assert_eq!(fresh_lifetime(s, false, false), None);
        s = crate::Snapshot::from_app(&app);
        s.pitch.value = None;
        assert_eq!(fresh_lifetime(s, false, false), None);
        s = crate::Snapshot::from_app(&app);
        s.menu = Some(crate::navigation::View { info: false,
            generation: 1,
            context: crate::navigation::Context::from_app(&app),
            pressed: None,
        });
        assert_eq!(fresh_lifetime(s, false, false), None);
        app.apply(Command::Select(Instrument::Turn)).unwrap();
        s = crate::Snapshot::from_app(&app);
        s.ball.quality = Quality::Failed;
        assert_eq!(fresh_lifetime(s, false, false), Some(100)); // fresh aircraft cue, failed ball removed
        s.turn_rate.quality = Quality::Failed;
        assert_eq!(fresh_lifetime(s, false, false), None);
        app.apply(Command::Select(Instrument::Altimeter)).unwrap();
        s = crate::Snapshot::from_app(&app);
        s.altitude.value = None;
        assert_eq!(fresh_lifetime(s, false, false), None); // malformed valid tag cannot renew a run
        s.external = true;
        s.baro_age_ms = Some(0);
        assert_eq!(fresh_lifetime(s, false, false), Some(1000)); // fresh QNH digits remain
        s.baro_inhg = 99.;
        assert_eq!(fresh_lifetime(s, false, false), None);
        app.apply(Command::Select(Instrument::Attitude)).unwrap();
        s = crate::Snapshot::from_app(&app);
        s.imu_bench = true;
        assert_eq!(s.numeric_lifetime_ms(), None);
        assert_eq!(fresh_lifetime(s, false, false), Some(100));
        assert_eq!(fresh_lifetime(s.aged(100), false, false), None);
    }
    #[test]
    fn holds_exactly_three_frame_slots_and_blanks_on_four_without_poll_counting() {
        let mut s = Supervisor::default();
        assert!(s.observe(50, Some(100), 1, 3, 32).allow);
        for (now, misses, allow) in [
            (100, 1, true),
            (110, 1, true),
            (131, 1, true),
            (132, 2, true),
            (163, 2, true),
            (164, 3, true),
            (195, 3, true),
            (196, 4, false),
            (228, 5, false),
        ] {
            let d = s.observe(now, Some(100), 1, 3, 32);
            assert_eq!((d.misses, d.allow), (misses, allow), "time {now}");
            assert_eq!(d.holding, allow);
        }
        let recovered = s.observe(230, Some(300), 2, 3, 32);
        assert!(recovered.allow && !recovered.holding && recovered.misses == 0);
    }
    #[test]
    fn old_frames_new_deadlines_or_nonnumeric_views_cannot_rearm_a_hold() {
        let mut s = Supervisor::default();
        s.observe(10, Some(100), 1, 3, 32);
        assert!(s.observe(100, Some(100), 1, 3, 32).holding);
        assert!(s.observe(150, Some(1000), 1, 3, 32).holding);
        let menu = s.observe(160, None, 1, 3, 32);
        assert!(menu.allow && !menu.holding && menu.misses == 2);
        assert!(!s.observe(196, Some(1000), 1, 3, 32).allow);
        assert!(!s.observe(300, Some(2000), 1, 3, 32).allow);
        assert!(s.observe(301, Some(350), 2, 3, 32).allow);
    }
    #[test]
    fn first_unproven_frame_never_holds_and_zero_restores_immediate_expiry() {
        let mut s = Supervisor::default();
        assert!(!s.observe(100, Some(99), 0, 3, 32).allow);
        assert!(s.observe(101, Some(200), 1, 0, 32).allow);
        assert!(!s.observe(200, Some(200), 1, 0, 32).allow);
        assert_eq!(s.observe(232, Some(200), 1, 0, 32).new_misses, 1);
    }
    #[test]
    fn delayed_poll_clock_failure_and_generation_wrap_remain_bounded() {
        let mut s = Supervisor::default();
        s.observe(0, Some(100), u32::MAX, 3, 32);
        assert_eq!(s.observe(200, Some(100), u32::MAX, 3, 32).misses, 4);
        assert_eq!(s.observe(201, Some(250), 0, 3, 32).misses, 0);
        s.observe(250, Some(250), 0, 3, 32);
        assert!(!s.observe(249, Some(250), 0, 3, 32).allow);
    }
    #[test]
    fn held_numeric_data_cannot_override_owner_renderer_or_scanout_failures() {
        let mut s = Supervisor::default();
        s.observe(0, Some(100), 1, 3, 32);
        assert!(s.observe(150, Some(100), 1, 3, 32).allow);
        for (now, owner, presented, scanout) in [
            (150, 50, 140, 140),
            (300, 290, 50, 290),
            (150, 140, 140, 50),
        ] {
            assert!(!crate::presentation_healthy(
                now,
                owner,
                presented,
                u64::MAX,
                scanout
            ));
        }
        assert!(crate::presentation_healthy(150, 140, 140, u64::MAX, 140));
    }
    #[test]
    fn changing_limits_or_frame_period_cannot_reset_an_existing_episode() {
        let mut s = Supervisor::default();
        s.observe(0, Some(100), 1, 3, 32);
        assert!(s.observe(100, Some(100), 1, 3, 32).holding);
        assert!(!s.observe(164, Some(100), 1, 1, 64).allow);
        assert!(s.observe(164, Some(100), 1, 3, 64).holding);
        assert!(!s.observe(196, Some(100), 1, 3, 64).allow); // retains original 32 ms
    }
}
