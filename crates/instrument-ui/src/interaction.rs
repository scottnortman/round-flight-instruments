//! Portable touch policy. Only a completed Apply tap returns a new setting;
//! the caller must pass it through the application's normal Baro command.
pub const TOUCH_STALE_MS: u64 = 120;
pub const EDIT_TIMEOUT_MS: u64 = 30_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}
impl Rect {
    pub fn contains(self, x: u16, y: u16) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}
pub const KOLLSMAN: Rect = Rect {
    x: 270,
    y: 211,
    w: 136,
    h: 42,
};
pub const MINUS: Rect = Rect {
    x: 104,
    y: 220,
    w: 72,
    h: 56,
};
pub const PLUS: Rect = Rect {
    x: 304,
    y: 220,
    w: 72,
    h: 56,
};
pub const STANDARD: Rect = Rect {
    x: 184,
    y: 220,
    w: 112,
    h: 56,
};
pub const CANCEL: Rect = Rect {
    x: 104,
    y: 292,
    w: 128,
    h: 48,
};
pub const APPLY: Rect = Rect {
    x: 248,
    y: 292,
    w: 128,
    h: 48,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Contact {
    Up,
    Down { id: u8, x: u16, y: u16 },
    Multiple,
}
#[derive(Clone, Copy, Debug)]
pub struct Input {
    pub at_ms: u64,
    pub contact: Contact,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorView {
    pub cents: u16,
    pub pressed: Option<Button>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Minus,
    Plus,
    Standard,
    Cancel,
    Apply,
}
impl Button {
    pub(crate) fn rect(self) -> Rect {
        match self {
            Self::Minus => MINUS,
            Self::Plus => PLUS,
            Self::Standard => STANDARD,
            Self::Cancel => CANCEL,
            Self::Apply => APPLY,
        }
    }
    fn hit(x: u16, y: u16) -> Option<Self> {
        [
            Self::Minus,
            Self::Plus,
            Self::Standard,
            Self::Cancel,
            Self::Apply,
        ]
        .into_iter()
        .find(|b| b.rect().contains(x, y))
    }
}
#[derive(Clone, Copy)]
struct Edit {
    original: u32,
    cents: u16,
    activity: u64,
    pressed: Option<(Button, u8)>,
}
pub struct Controller {
    hold_ms: u32,
    armed: bool,
    holding: Option<(u64, u8, u16, u16)>,
    edit: Option<Edit>,
    last_tick: Option<u64>,
    press_started: bool,
}
impl Default for Controller {
    fn default() -> Self {
        Self {
            hold_ms: 250,
            armed: false,
            holding: None,
            edit: None,
            last_tick: None,
            press_started: false,
        }
    }
}
impl Controller {
    pub fn hold_ms(&self) -> u32 {
        self.hold_ms
    }
    pub fn set_hold_ms(&mut self, value: u32) -> bool {
        if !(250..=5000).contains(&value) {
            return false;
        }
        self.cancel();
        self.hold_ms = value;
        true
    }
    pub fn view(&self) -> Option<EditorView> {
        self.edit.map(|e| EditorView {
            cents: e.cents,
            pressed: e.pressed.map(|(button, _)| button),
        })
    }
    /// Feedback only for an accepted, still-pending opening gesture.
    pub fn window_pressed(&self) -> bool {
        self.holding.is_some()
    }
    /// True only on the update that accepts a new press on an active control.
    pub fn press_started(&self) -> bool {
        self.press_started
    }
    pub fn cancel(&mut self) {
        self.edit = None;
        self.holding = None;
        self.armed = false;
        self.press_started = false;
    }
    pub fn update(
        &mut self,
        now: u64,
        input: Option<Input>,
        allowed: bool,
        baro: f32,
        editor_presented: bool,
    ) -> Option<f32> {
        self.press_started = false;
        let gap_bad = self.last_tick.is_some_and(|last| {
            now.checked_sub(last)
                .is_none_or(|gap| gap >= TOUCH_STALE_MS)
        });
        self.last_tick = Some(now);
        let Some(input) = input.filter(|i| {
            now.checked_sub(i.at_ms)
                .is_some_and(|age| age < TOUCH_STALE_MS)
        }) else {
            self.cancel();
            return None;
        };
        if !allowed
            || gap_bad
            || !baro.is_finite()
            || !(28.1..=31.).contains(&baro)
            || input.contact == Contact::Multiple
        {
            self.cancel();
            return None;
        }
        if self.edit.is_some_and(|e| {
            e.original != baro.to_bits() || now.saturating_sub(e.activity) >= EDIT_TIMEOUT_MS
        }) {
            self.cancel();
            return None;
        }
        if let Some(mut e) = self.edit {
            if !editor_presented {
                self.armed = false;
                e.pressed = None;
                self.edit = Some(e);
                return None;
            }
        }
        match input.contact {
            Contact::Up => {
                self.holding = None;
                self.armed = true;
                if let Some(mut e) = self.edit {
                    if let Some((button, _)) = e.pressed.take() {
                        e.activity = now;
                        match button {
                            Button::Minus => e.cents = e.cents.saturating_sub(1).max(2810),
                            Button::Plus => e.cents = (e.cents + 1).min(3100),
                            Button::Standard => e.cents = 2992,
                            Button::Cancel => {
                                self.cancel();
                                return None;
                            }
                            Button::Apply => {
                                self.cancel();
                                return Some(e.cents as f32 / 100.);
                            }
                        }
                    }
                    self.edit = Some(e);
                }
            }
            Contact::Down { id, x, y } => {
                if let Some(mut e) = self.edit {
                    if let Some((button, pressed_id)) = e.pressed {
                        if id != pressed_id || !button.rect().contains(x, y) {
                            e.pressed = None;
                        }
                    } else if self.armed {
                        e.pressed = Button::hit(x, y).map(|b| (b, id));
                        self.press_started = e.pressed.is_some();
                        e.activity = now;
                    }
                    self.armed = false;
                    self.edit = Some(e);
                } else if let Some((start, first_id, ox, oy)) = self.holding {
                    if id != first_id
                        || !KOLLSMAN.contains(x, y)
                        || ox.abs_diff(x) > 16
                        || oy.abs_diff(y) > 16
                    {
                        self.holding = None;
                    } else if input.at_ms.saturating_sub(start) >= self.hold_ms as u64 {
                        self.edit = Some(Edit {
                            original: baro.to_bits(),
                            cents: libm::roundf(baro * 100.) as u16,
                            activity: now,
                            pressed: None,
                        });
                        self.holding = None;
                    }
                } else if self.armed && KOLLSMAN.contains(x, y) {
                    self.holding = Some((input.at_ms, id, x, y));
                    self.press_started = true;
                    self.armed = false;
                } else {
                    self.armed = false;
                }
            }
            Contact::Multiple => unreachable!(),
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn feed(c: &mut Controller, t: u64, v: Contact) -> Option<f32> {
        c.update(
            t,
            Some(Input {
                at_ms: t,
                contact: v,
            }),
            true,
            29.92,
            true,
        )
    }
    fn down(x: u16, y: u16) -> Contact {
        Contact::Down { id: 0, x, y }
    }
    fn open(c: &mut Controller) -> u64 {
        feed(c, 0, Contact::Up);
        for t in (20..270).step_by(20) {
            feed(c, t, down(338, 235));
            assert!(c.view().is_none());
            assert!(c.window_pressed());
        }
        feed(c, 269, down(338, 235));
        assert!(c.view().is_none()); // 249 ms is still too short
        feed(c, 270, down(338, 235));
        assert_eq!(
            c.view(),
            Some(EditorView {
                cents: 2992,
                pressed: None
            })
        );
        assert!(!c.window_pressed());
        270
    }
    #[test]
    fn startup_unknown_or_down_cannot_arm_without_a_release() {
        let mut c = Controller::default();
        assert_eq!(c.update(0, None, true, 29.92, true), None);
        for t in (20..=620).step_by(20) {
            feed(&mut c, t, down(338, 235));
            assert!(!c.window_pressed());
            assert!(c.view().is_none());
        }
        feed(&mut c, 640, Contact::Up);
        for t in (660..910).step_by(20) {
            feed(&mut c, t, down(338, 235));
            assert!(c.window_pressed());
            assert!(c.view().is_none());
        }
        feed(&mut c, 910, down(338, 235));
        assert!(c.view().is_some());
    }
    #[test]
    fn press_feedback_clears_on_release_and_every_abort_path() {
        for mode in 0..7 {
            let mut c = Controller::default();
            feed(&mut c, 0, Contact::Up);
            feed(&mut c, 20, down(338, 235));
            assert!(c.window_pressed());
            match mode {
                0 => {
                    feed(&mut c, 40, Contact::Up);
                }
                1 => {
                    feed(&mut c, 40, down(100, 100));
                }
                2 => {
                    feed(&mut c, 40, Contact::Multiple);
                }
                3 => {
                    c.update(40, None, true, 29.92, true);
                }
                4 => {
                    c.update(
                        40,
                        Some(Input {
                            at_ms: 40,
                            contact: down(338, 235),
                        }),
                        false,
                        29.92,
                        true,
                    );
                }
                5 => {
                    feed(&mut c, 200, down(338, 235));
                }
                _ => c.cancel(),
            }
            assert!(!c.window_pressed());
            assert!(c.view().is_none());
        }
    }
    #[test]
    fn long_press_release_step_and_apply_are_separate_actions() {
        let mut c = Controller::default();
        let t = open(&mut c);
        feed(&mut c, t + 20, down(300, 310)); // opening finger cannot Apply
        assert_eq!(feed(&mut c, t + 40, Contact::Up), None);
        feed(&mut c, t + 60, down(340, 245));
        feed(&mut c, t + 80, Contact::Up);
        assert_eq!(c.view().unwrap().cents, 2993);
        feed(&mut c, t + 100, down(300, 310));
        assert_eq!(feed(&mut c, t + 120, Contact::Up), Some(29.93));
        assert!(c.view().is_none());
    }
    #[test]
    fn drag_multitouch_stale_gap_and_conflicting_cli_cancel() {
        for mode in 0..7 {
            let mut c = Controller::default();
            let t = open(&mut c);
            match mode {
                0 => {
                    feed(&mut c, t + 20, Contact::Multiple);
                }
                1 => {
                    c.update(t + 20, None, true, 29.92, true);
                }
                2 => {
                    feed(&mut c, t + 200, Contact::Up);
                }
                3 => {
                    c.update(
                        t + 20,
                        Some(Input {
                            at_ms: t + 20,
                            contact: Contact::Up,
                        }),
                        true,
                        30.,
                        true,
                    );
                }
                4 => {
                    c.update(
                        t + 20,
                        Some(Input {
                            at_ms: t + 20,
                            contact: Contact::Up,
                        }),
                        false,
                        29.92,
                        true,
                    );
                }
                5 => {
                    c.update(
                        t + 20,
                        Some(Input {
                            at_ms: 0,
                            contact: Contact::Up,
                        }),
                        true,
                        29.92,
                        true,
                    );
                }
                _ => {
                    c.update(
                        10,
                        Some(Input {
                            at_ms: 10,
                            contact: Contact::Up,
                        }),
                        true,
                        29.92,
                        true,
                    );
                }
            }
            assert!(c.view().is_none());
        }
        let mut c = Controller::default();
        feed(&mut c, 0, Contact::Up);
        feed(&mut c, 20, down(338, 235));
        feed(&mut c, 40, down(100, 100));
        for t in (60..2500).step_by(20) {
            feed(&mut c, t, down(338, 235));
        }
        assert!(c.view().is_none());
        let mut c = Controller::default();
        let t = open(&mut c);
        feed(&mut c, t + 20, Contact::Up);
        feed(&mut c, t + 40, down(300, 310));
        feed(&mut c, t + 60, down(50, 50));
        assert_eq!(feed(&mut c, t + 80, Contact::Up), None);
        assert!(c.view().is_some());
    }
    #[test]
    fn idle_timeout_cancel_and_setting_bounds() {
        let mut c = Controller::default();
        let t = open(&mut c);
        feed(&mut c, t + 20, Contact::Up);
        for now in ((t + 40)..(t + EDIT_TIMEOUT_MS + 40)).step_by(20) {
            assert_eq!(feed(&mut c, now, Contact::Up), None);
        }
        assert!(c.view().is_none());
        assert!(!c.set_hold_ms(0));
        assert_eq!(c.hold_ms(), 250);
        assert!(c.set_hold_ms(2000));
        assert_eq!(c.hold_ms(), 2000);
        let mut c = Controller::default();
        let t = open(&mut c);
        feed(&mut c, t + 20, Contact::Up);
        feed(&mut c, t + 40, down(150, 310));
        assert_eq!(feed(&mut c, t + 60, Contact::Up), None);
        assert!(c.view().is_none());
    }
    #[test]
    fn display_ack_bounds_and_apply_change_offset_not_source() {
        let mut c = Controller::default();
        let t = open(&mut c);
        for (n, contact) in [Contact::Up, down(300, 310), Contact::Up]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                c.update(
                    t + 20 + n as u64 * 20,
                    Some(Input {
                        at_ms: t + 20 + n as u64 * 20,
                        contact
                    }),
                    true,
                    29.92,
                    false
                ),
                None
            );
        }
        assert!(c.view().is_some());
        feed(&mut c, t + 80, down(300, 310)); // still requires release after display ack
        assert_eq!(feed(&mut c, t + 100, Contact::Up), None);
        c.edit.as_mut().unwrap().cents = 3100;
        feed(&mut c, t + 120, down(340, 245));
        feed(&mut c, t + 140, Contact::Up);
        assert_eq!(c.view().unwrap().cents, 3100);
        c.edit.as_mut().unwrap().cents = 2810;
        feed(&mut c, t + 160, down(150, 245));
        feed(&mut c, t + 180, Contact::Up);
        assert_eq!(c.view().unwrap().cents, 2810);
        c.edit.as_mut().unwrap().cents = 2993;
        feed(&mut c, t + 200, down(300, 310));
        let value = feed(&mut c, t + 220, Contact::Up).unwrap();
        let mut app = instrument_core::App::new();
        app.apply(instrument_core::Command::LevelPreset).unwrap();
        app.tick(0);
        let pressure = app.reading(instrument_core::Field::PressureAltitudeFt, 0);
        let old = crate::altimeter::model(pressure, app.baro_inhg())
            .indicated_ft
            .unwrap();
        app.apply(instrument_core::Command::Baro(value)).unwrap();
        assert_eq!(
            app.reading(instrument_core::Field::PressureAltitudeFt, 0),
            pressure
        );
        assert!(
            crate::altimeter::model(pressure, app.baro_inhg())
                .indicated_ft
                .unwrap()
                > old
        );
    }
    #[test]
    fn standard_button_changes_only_draft_until_apply() {
        let mut c = Controller::default();
        let t = open(&mut c);
        feed(&mut c, t + 20, Contact::Up);
        feed(&mut c, t + 40, down(340, 245));
        feed(&mut c, t + 60, Contact::Up);
        assert_eq!(c.view().unwrap().cents, 2993);
        feed(&mut c, t + 80, down(240, 245));
        assert_eq!(feed(&mut c, t + 100, Contact::Up), None);
        assert_eq!(c.view().unwrap().cents, 2992);
        feed(&mut c, t + 120, down(300, 310));
        assert_eq!(feed(&mut c, t + 140, Contact::Up), Some(29.92));
    }
    #[test]
    fn every_button_feedback_tracks_only_an_accepted_press() {
        for button in [
            Button::Minus,
            Button::Plus,
            Button::Standard,
            Button::Cancel,
            Button::Apply,
        ] {
            let mut c = Controller::default();
            let t = open(&mut c);
            let r = button.rect();
            let contact = down(r.x + r.w / 2, r.y + r.h / 2);
            feed(&mut c, t + 20, contact);
            assert!(!c.press_started());
            assert_eq!(c.view().unwrap().pressed, None); // opening finger is disarmed
            feed(&mut c, t + 40, Contact::Up);
            assert_eq!(feed(&mut c, t + 60, contact), None);
            assert!(c.press_started());
            assert_eq!(c.view().unwrap().pressed, Some(button));
            assert_eq!(c.view().unwrap().cents, 2992); // feedback never activates
            feed(&mut c, t + 80, down(50, 50));
            assert!(!c.press_started());
            assert_eq!(c.view().unwrap().pressed, None);
            feed(&mut c, t + 100, contact);
            assert!(!c.press_started());
            assert_eq!(c.view().unwrap().pressed, None); // dragging back cannot re-arm
            assert_eq!(feed(&mut c, t + 120, Contact::Up), None);
            assert_eq!(c.view().unwrap().cents, 2992);
            feed(&mut c, t + 140, contact);
            assert!(c.press_started());
            assert_eq!(c.view().unwrap().pressed, Some(button));
            let applied = feed(&mut c, t + 160, Contact::Up);
            assert!(!c.press_started());
            match button {
                Button::Apply => {
                    assert_eq!(applied, Some(29.92));
                    assert!(c.view().is_none());
                }
                Button::Cancel => {
                    assert_eq!(applied, None);
                    assert!(c.view().is_none());
                }
                _ => assert_eq!(c.view().unwrap().pressed, None),
            }
        }
    }
    #[test]
    fn only_active_controls_emit_sound_events_and_holds_do_not_repeat() {
        let mut c = Controller::default();
        feed(&mut c, 0, Contact::Up);
        // Dial centre and hidden popup buttons are inactive on the main gauge.
        for (n, (x, y)) in [(240, 240), (140, 245), (240, 245), (140, 310), (300, 310)]
            .into_iter()
            .enumerate()
        {
            let t = 20 + n as u64 * 60;
            feed(&mut c, t, down(x, y));
            assert!(!c.press_started());
            feed(&mut c, t + 20, down(338, 235)); // entering a control from outside is not a new press
            assert!(!c.press_started());
            feed(&mut c, t + 40, Contact::Up);
        }
        feed(&mut c, 320, down(338, 235));
        assert!(c.press_started());
        for t in (340..=580).step_by(20) {
            feed(&mut c, t, down(338, 235));
            assert!(!c.press_started());
        }
        assert!(c.view().is_some());
        feed(&mut c, 600, Contact::Up);
        feed(&mut c, 620, down(240, 190)); // popup value is not a button
        assert!(!c.press_started());
        feed(&mut c, 640, down(340, 245));
        assert!(!c.press_started());
    }
    #[test]
    fn unavailable_stale_or_unpresented_controls_never_emit_sound_events() {
        for mode in 0..4 {
            let mut c = Controller::default();
            feed(&mut c, 0, Contact::Up);
            c.update(
                20,
                if mode == 0 {
                    None
                } else {
                    Some(Input {
                        at_ms: if mode == 1 { 21 } else { 20 },
                        contact: down(338, 235),
                    })
                },
                mode != 2,
                if mode == 3 { f32::NAN } else { 29.92 },
                true,
            );
            assert!(!c.press_started());
        }
        let mut c = Controller::default();
        let t = open(&mut c);
        feed(&mut c, t + 20, Contact::Up);
        c.update(
            t + 40,
            Some(Input {
                at_ms: t + 40,
                contact: down(340, 245),
            }),
            true,
            29.92,
            false,
        );
        assert!(!c.press_started());
        c.cancel();
        assert!(!c.press_started());
    }
}
