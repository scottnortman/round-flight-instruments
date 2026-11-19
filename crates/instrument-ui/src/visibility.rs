//! Local readout visibility; never changes measurements or aircraft controls.
use crate::interaction::{Contact, Input, Rect, TOUCH_STALE_MS};

pub const ALTITUDE: Rect = Rect {
    x: 124,
    y: 284,
    w: 232,
    h: 40,
};

#[derive(Default)]
pub struct Controller {
    hidden: bool,
    armed: bool,
    pressed: Option<u8>,
    last_tick: Option<u64>,
}
impl Controller {
    pub fn hidden(&self) -> bool {
        self.hidden
    }
    pub fn cancel(&mut self) {
        self.armed = false;
        self.pressed = None;
    }
    /// Returns true once on a new press for the ordinary touch chirp.
    pub fn update(&mut self, now: u64, input: Option<Input>, allowed: bool) -> bool {
        let gap = self
            .last_tick
            .is_some_and(|t| now.checked_sub(t).is_none_or(|d| d >= TOUCH_STALE_MS));
        self.last_tick = Some(now);
        let input = input.filter(|i| now.checked_sub(i.at_ms).is_some_and(|a| a < TOUCH_STALE_MS));
        if !allowed
            || gap
            || input.is_none()
            || input.is_some_and(|i| i.contact == Contact::Multiple)
        {
            self.armed = false;
            self.pressed = None;
            return false;
        }
        match input.unwrap().contact {
            Contact::Up => {
                if self.pressed.take().is_some() {
                    self.hidden = !self.hidden;
                }
                self.armed = true;
            }
            Contact::Down { id, x, y } => {
                if let Some(old) = self.pressed {
                    if old != id || !ALTITUDE.contains(x, y) {
                        self.pressed = None;
                    }
                } else if self.armed {
                    self.armed = false;
                    if ALTITUDE.contains(x, y) {
                        self.pressed = Some(id);
                        return true;
                    }
                }
            }
            Contact::Multiple => {}
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn feed(c: &mut Controller, t: u64, contact: Contact, allowed: bool) -> bool {
        c.update(t, Some(Input { at_ms: t, contact }), allowed)
    }
    fn down(x: u16, y: u16) -> Contact {
        Contact::Down { id: 1, x, y }
    }
    #[test]
    fn taps_toggle_once_and_require_release() {
        let mut c = Controller::default();
        feed(&mut c, 0, down(240, 300), true);
        assert!(!c.hidden());
        feed(&mut c, 10, Contact::Up, true);
        assert!(feed(&mut c, 20, down(240, 300), true));
        for t in (30..300).step_by(10) {
            assert!(!feed(&mut c, t, down(240, 300), true));
        }
        assert!(!c.hidden());
        feed(&mut c, 300, Contact::Up, true);
        assert!(c.hidden());
        feed(&mut c, 310, Contact::Up, true);
        feed(&mut c, 320, down(240, 300), true);
        feed(&mut c, 330, Contact::Up, true);
        assert!(!c.hidden());
    }
    #[test]
    fn interrupted_or_outside_gestures_cannot_toggle() {
        for reason in 0..7 {
            let mut c = Controller::default();
            feed(&mut c, 0, Contact::Up, true);
            feed(&mut c, 10, down(240, 300), true);
            match reason {
                0 => {
                    feed(&mut c, 20, down(50, 300), true);
                }
                1 => {
                    feed(&mut c, 20, Contact::Multiple, true);
                }
                2 => {
                    c.update(20, None, true);
                }
                3 => {
                    feed(&mut c, 20, down(240, 300), false);
                }
                4 => {
                    feed(&mut c, 150, down(240, 300), true);
                }
                5 => {
                    feed(
                        &mut c,
                        20,
                        Contact::Down {
                            id: 2,
                            x: 240,
                            y: 300,
                        },
                        true,
                    );
                }
                _ => {
                    c.update(
                        150,
                        Some(Input {
                            at_ms: 10,
                            contact: down(240, 300),
                        }),
                        true,
                    );
                }
            }
            feed(&mut c, 160, Contact::Up, true);
            assert!(!c.hidden(), "interruption {reason}");
        }
    }
}
