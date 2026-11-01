//! Portable press-to-chirp policy; board code owns pulse timing and transport.
use crate::interaction::{Contact, Input, TOUCH_STALE_MS};

#[derive(Default)]
pub struct PressSound {
    armed: bool,
    last_beep: Option<u64>,
}
impl PressSound {
    pub fn update(&mut self, now: u64, input: Option<Input>, enabled: bool) -> bool {
        let contact = input
            .filter(|i| {
                now.checked_sub(i.at_ms)
                    .is_some_and(|age| age < TOUCH_STALE_MS)
            })
            .map(|i| i.contact);
        if !enabled {
            self.armed = false;
            return false;
        }
        match contact {
            Some(Contact::Up) => self.armed = true,
            Some(Contact::Down { .. }) => {
                let beep = self.armed
                    && self.last_beep.is_none_or(|last| {
                        now.checked_sub(last).is_some_and(|elapsed| elapsed >= 150)
                    });
                self.armed = false;
                if beep {
                    self.last_beep = Some(now);
                }
                return beep;
            }
            _ => self.armed = false,
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(at_ms: u64, contact: Contact) -> Option<Input> {
        Some(Input { at_ms, contact })
    }
    const DOWN: Contact = Contact::Down {
        id: 0,
        x: 240,
        y: 240,
    };
    #[test]
    fn sound_requires_release_and_does_not_repeat_or_queue_rapid_taps() {
        let mut sound = PressSound::default();
        assert!(!sound.update(0, input(0, DOWN), true));
        assert!(!sound.update(20, input(20, Contact::Up), true));
        assert!(sound.update(40, input(40, DOWN), true));
        assert!(!sound.update(60, input(60, DOWN), true));
        sound.update(80, input(80, Contact::Up), true);
        assert!(!sound.update(100, input(100, DOWN), true));
        assert!(!sound.update(200, input(200, DOWN), true));
        sound.update(220, input(220, Contact::Up), true);
        assert!(sound.update(240, input(240, DOWN), true));
    }
    #[test]
    fn muted_stale_unknown_and_multiple_contacts_disarm_sound() {
        for mode in 0..5 {
            let mut sound = PressSound::default();
            sound.update(0, input(0, Contact::Up), true);
            match mode {
                0 => assert!(!sound.update(20, input(20, DOWN), false)),
                1 => assert!(!sound.update(120, input(0, DOWN), true)),
                2 => assert!(!sound.update(20, None, true)),
                3 => assert!(!sound.update(20, input(20, Contact::Multiple), true)),
                _ => assert!(!sound.update(20, input(30, DOWN), true)),
            }
            assert!(!sound.update(200, input(200, DOWN), true));
        }
    }
}
