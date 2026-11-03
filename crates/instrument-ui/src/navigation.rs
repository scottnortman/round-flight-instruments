//! Gauge navigation policy; independent of touch hardware, rendering and sources.
use crate::interaction::{Contact, Input, Rect, EDIT_TIMEOUT_MS, KOLLSMAN, TOUCH_STALE_MS};
use instrument_core::{App, Instrument};

pub const GAUGES: [Instrument; 6] = [
    Instrument::Airspeed,
    Instrument::Attitude,
    Instrument::Altimeter,
    Instrument::Heading,
    Instrument::Turn,
    Instrument::Vsi,
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Gauge(Instrument),
    Cancel,
}
impl Button {
    pub fn rect(self) -> Rect {
        match self {
            Self::Cancel => Rect {
                x: 168,
                y: 326,
                w: 144,
                h: 42,
            },
            Self::Gauge(i) => {
                let n = GAUGES.iter().position(|v| *v == i).unwrap_or(0) as u16;
                Rect {
                    x: 92 + (n % 2) * 152,
                    y: 138 + (n / 2) * 60,
                    w: 144,
                    h: 52,
                }
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context {
    pub selected: Instrument,
    pub external: bool,
    pub external_attitude: bool,
    pub imu: bool,
    pub session: u64,
}
impl Context {
    pub fn from_app(app: &App) -> Self {
        Self {
            selected: app.selected(),
            external: app.external().is_some(),
            external_attitude: app.external().is_some_and(|e| e.multi_profile),
            imu: app.imu_bench(),
            session: app.external().map_or(0, |e| e.session),
        }
    }
    pub fn enabled(self, instrument: Instrument) -> bool {
        match instrument {
            Instrument::Altimeter => !self.imu,
            Instrument::Turn => !self.imu && (!self.external || self.external_attitude),
            Instrument::Attitude => !self.external || self.external_attitude,
            _ => false,
        }
    }
    pub fn reason(self, instrument: Instrument) -> &'static str {
        match instrument {
            Instrument::Altimeter | Instrument::Turn if self.imu => "NO IMU DATA",
            Instrument::Turn if self.external && !self.external_attitude => "NO SIM DATA",
            Instrument::Attitude if self.external && !self.external_attitude => "NO SIM DATA",
            Instrument::Altimeter | Instrument::Attitude | Instrument::Turn => "",
            _ => "NOT BUILT",
        }
    }
    fn hit(self, x: u16, y: u16) -> Option<Button> {
        GAUGES
            .into_iter()
            .filter(|i| self.enabled(*i))
            .map(Button::Gauge)
            .chain(core::iter::once(Button::Cancel))
            .find(|b| b.rect().contains(x, y))
    }
    fn center(self, x: u16, y: u16) -> bool {
        let dx = x as i64 - 240;
        let dy = y as i64 - 240;
        dx * dx + dy * dy <= 50 * 50
            && !(self.selected == Instrument::Altimeter && KOLLSMAN.contains(x, y))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    pub generation: u32,
    pub context: Context,
    pub pressed: Option<Button>,
}
pub struct Controller {
    hold_ms: u32,
    armed: bool,
    holding: Option<(u64, u8, u16, u16)>,
    view: Option<View>,
    pressed_id: Option<u8>,
    last_tick: Option<u64>,
    context: Option<Context>,
    activity: u64,
    generation: u32,
    press_started: bool,
}
impl Default for Controller {
    fn default() -> Self {
        Self {
            hold_ms: 3000,
            armed: false,
            holding: None,
            view: None,
            pressed_id: None,
            last_tick: None,
            context: None,
            activity: 0,
            generation: 0,
            press_started: false,
        }
    }
}
impl Controller {
    pub fn hold_ms(&self) -> u32 {
        self.hold_ms
    }
    pub fn set_hold_ms(&mut self, ms: u32) -> bool {
        if !(250..=5000).contains(&ms) {
            return false;
        }
        self.cancel();
        self.hold_ms = ms;
        true
    }
    pub fn view(&self) -> Option<View> {
        self.view
    }
    pub fn center_pressed(&self) -> bool {
        self.holding.is_some()
    }
    pub fn busy(&self) -> bool {
        self.holding.is_some() || self.view.is_some()
    }
    pub fn press_started(&self) -> bool {
        self.press_started
    }
    pub fn cancel(&mut self) {
        self.armed = false;
        self.holding = None;
        self.view = None;
        self.pressed_id = None;
        self.press_started = false;
    }
    pub fn update(
        &mut self,
        now: u64,
        input: Option<Input>,
        allowed: bool,
        context: Context,
        presented: u32,
    ) -> Option<Instrument> {
        self.update_visible(now, input, allowed, true, context, presented)
    }
    /// Numeric data can expire while the display pipeline is operational.
    /// Preserve an open menu during that brief dark interval, but discard any
    /// gesture and require a fresh visible release before another press.
    pub fn update_visible(
        &mut self,
        now: u64,
        input: Option<Input>,
        allowed: bool,
        visible: bool,
        context: Context,
        presented: u32,
    ) -> Option<Instrument> {
        self.press_started = false;
        let bad_gap = self
            .last_tick
            .is_some_and(|last| now.checked_sub(last).is_none_or(|d| d >= TOUCH_STALE_MS));
        self.last_tick = Some(now);
        let changed = self.context.is_some_and(|old| old != context);
        self.context = Some(context);
        let Some(input) = input.filter(|i| {
            now.checked_sub(i.at_ms)
                .is_some_and(|age| age < TOUCH_STALE_MS)
        }) else {
            self.cancel();
            return None;
        };
        if !allowed || bad_gap || changed || input.contact == Contact::Multiple {
            self.cancel();
            return None;
        }
        if self.view.is_some() && now.saturating_sub(self.activity) >= EDIT_TIMEOUT_MS {
            self.cancel();
            return None;
        }
        if !visible {
            self.armed = false;
            self.holding = None;
            self.pressed_id = None;
            if let Some(view) = &mut self.view {
                view.pressed = None;
            }
            return None;
        }
        if let Some(v) = self.view.as_mut() {
            if presented != v.generation {
                self.armed = false;
                v.pressed = None;
                self.pressed_id = None;
                return None;
            }
        }
        match input.contact {
            Contact::Up => {
                self.holding = None;
                self.armed = true;
                if let Some(button) = self.view.and_then(|v| v.pressed) {
                    self.cancel();
                    if let Button::Gauge(i) = button {
                        return context.enabled(i).then_some(i);
                    }
                }
            }
            Contact::Down { id, x, y } => {
                if let Some(v) = self.view.as_mut() {
                    if let Some(b) = v.pressed {
                        if self.pressed_id != Some(id) || !b.rect().contains(x, y) {
                            v.pressed = None;
                            self.pressed_id = None;
                        }
                    } else if self.armed {
                        v.pressed = context.hit(x, y);
                        self.pressed_id = v.pressed.map(|_| id);
                        self.press_started = v.pressed.is_some();
                        if self.press_started {
                            self.activity = now;
                        }
                    }
                    self.armed = false;
                } else if let Some((start, first_id, ox, oy)) = self.holding {
                    if id != first_id
                        || !context.center(x, y)
                        || ox.abs_diff(x) > 16
                        || oy.abs_diff(y) > 16
                    {
                        self.holding = None;
                    } else if input.at_ms.saturating_sub(start) >= self.hold_ms as u64 {
                        self.generation = self.generation.wrapping_add(1).max(1);
                        self.view = Some(View {
                            generation: self.generation,
                            context,
                            pressed: None,
                        });
                        self.holding = None;
                        self.activity = now;
                    }
                } else if self.armed && context.center(x, y) {
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
    fn ctx() -> Context {
        Context {
            selected: Instrument::Altimeter,
            external: false,
            external_attitude: false,
            imu: false,
            session: 0,
        }
    }
    fn down(x: u16, y: u16) -> Contact {
        Contact::Down { id: 1, x, y }
    }
    fn feed(c: &mut Controller, t: u64, v: Contact) -> Option<Instrument> {
        let generation = c.view().map_or(0, |v| v.generation);
        c.update(
            t,
            Some(Input {
                at_ms: t,
                contact: v,
            }),
            true,
            ctx(),
            generation,
        )
    }
    fn open(c: &mut Controller) -> u64 {
        feed(c, 0, Contact::Up);
        for t in (10..3010).step_by(10) {
            feed(c, t, down(240, 240));
            assert_eq!(c.press_started(), t == 10);
            assert!(c.view().is_none());
        }
        feed(c, 3010, down(240, 240));
        assert!(c.view().is_some());
        3010
    }
    #[test]
    fn hold_release_then_button_selects_once() {
        let mut c = Controller::default();
        let t = open(&mut c);
        assert!(feed(&mut c, t + 10, down(300, 160)).is_none()); // opening finger cannot click
        assert!(c.view().unwrap().pressed.is_none());
        feed(&mut c, t + 20, Contact::Up);
        feed(&mut c, t + 30, down(300, 160));
        assert!(c.press_started());
        assert_eq!(
            c.view().unwrap().pressed,
            Some(Button::Gauge(Instrument::Attitude))
        );
        assert_eq!(
            feed(&mut c, t + 40, Contact::Up),
            Some(Instrument::Attitude)
        );
        assert_eq!(feed(&mut c, t + 50, Contact::Up), None);
    }
    #[test]
    fn brief_numeric_blanking_preserves_menu_but_never_accepts_a_hidden_gesture() {
        let mut c = Controller::default();
        let t = open(&mut c);
        let generation = c.view().unwrap().generation;
        let target = Button::Gauge(Instrument::Attitude).rect();
        let down = Contact::Down {
            id: 0,
            x: target.x + 10,
            y: target.y + 10,
        };
        let update = |c: &mut Controller, dt, contact, visible, allowed| {
            c.update_visible(
                t + dt,
                Some(Input {
                    at_ms: t + dt,
                    contact,
                }),
                allowed,
                visible,
                ctx(),
                generation,
            )
        };
        assert_eq!(update(&mut c, 10, Contact::Up, true, true), None);
        assert_eq!(update(&mut c, 20, down, true, true), None);
        assert!(c.view().unwrap().pressed.is_some());
        assert_eq!(update(&mut c, 30, Contact::Up, false, true), None);
        assert!(c.view().is_some() && c.view().unwrap().pressed.is_none());
        assert_eq!(update(&mut c, 40, down, true, true), None);
        assert!(c.view().unwrap().pressed.is_none());
        assert_eq!(update(&mut c, 50, Contact::Up, true, true), None);
        assert_eq!(update(&mut c, 60, down, true, true), None);
        assert_eq!(
            update(&mut c, 70, Contact::Up, true, true),
            Some(Instrument::Attitude)
        );
        let mut c = Controller::default();
        let t = open(&mut c);
        let generation = c.view().unwrap().generation;
        c.update_visible(
            t + 10,
            Some(Input {
                at_ms: t + 10,
                contact: Contact::Up,
            }),
            false,
            false,
            ctx(),
            generation,
        );
        assert!(
            c.view().is_none(),
            "Actual display unavailability still cancels"
        );
    }
    #[test]
    fn disabled_drag_stale_multitouch_and_unpresented_never_select() {
        for mode in 0..5 {
            let mut c = Controller::default();
            let t = open(&mut c);
            feed(&mut c, t + 10, Contact::Up);
            feed(&mut c, t + 20, down(if mode == 0 { 150 } else { 300 }, 160));
            match mode {
                0 => assert!(!c.press_started()), // unfinished airspeed
                1 => {
                    feed(&mut c, t + 30, down(430, 160));
                    feed(&mut c, t + 40, down(300, 160));
                }
                2 => {
                    c.update(t + 30, None, true, ctx(), 0);
                }
                3 => {
                    feed(&mut c, t + 30, Contact::Multiple);
                }
                _ => {
                    c.update(
                        t + 30,
                        Some(Input {
                            at_ms: t + 30,
                            contact: Contact::Up,
                        }),
                        true,
                        ctx(),
                        0,
                    );
                }
            }
            assert_eq!(feed(&mut c, t + 50, Contact::Up), None);
        }
    }
    #[test]
    fn changed_source_cancels_and_capabilities_are_explicit() {
        let mut c = Controller::default();
        let t = open(&mut c);
        let external = Context {
            external: true,
            external_attitude: false,
            session: 1,
            ..ctx()
        };
        c.update(
            t + 10,
            Some(Input {
                at_ms: t + 10,
                contact: Contact::Up,
            }),
            true,
            external,
            1,
        );
        assert!(c.view().is_none());
        assert!(!external.enabled(Instrument::Attitude));
        assert!(!external.enabled(Instrument::Turn));
        assert!(Context {
            external_attitude: true,
            ..external
        }
        .enabled(Instrument::Turn));
        assert!(ctx().enabled(Instrument::Turn));
        assert!(!Context { imu: true, ..ctx() }.enabled(Instrument::Turn));
        assert!(external.enabled(Instrument::Altimeter));
        assert!(!Context { imu: true, ..ctx() }.enabled(Instrument::Altimeter));
        assert!(!ctx().center(280, 240)); // pressure window owns overlap
        assert!(!c.set_hold_ms(249));
        assert!(c.set_hold_ms(5000));
    }
    #[test]
    fn startup_hold_drag_contact_change_and_configured_threshold_are_safe() {
        let mut c = Controller::default();
        for t in (0..4000).step_by(10) {
            feed(&mut c, t, down(240, 240));
        }
        assert!(!c.busy());
        assert!(!c.press_started());
        for bad in [
            down(260, 240),
            Contact::Down {
                id: 2,
                x: 240,
                y: 240,
            },
            Contact::Multiple,
        ] {
            let mut c = Controller::default();
            feed(&mut c, 0, Contact::Up);
            feed(&mut c, 10, down(240, 240));
            assert!(c.press_started());
            feed(&mut c, 20, bad);
            assert!(!c.center_pressed());
            feed(&mut c, 30, down(240, 240));
            assert!(!c.busy());
        }
        let mut c = Controller::default();
        assert!(c.set_hold_ms(250));
        feed(&mut c, 0, Contact::Up);
        for t in (10..260).step_by(10) {
            feed(&mut c, t, down(240, 240));
            assert!(c.view().is_none());
        }
        feed(&mut c, 260, down(240, 240));
        assert!(c.view().is_some());
        let generation = c.view().unwrap().generation;
        c.cancel();
        feed(&mut c, 270, Contact::Up);
        for t in (280..=530).step_by(10) {
            feed(&mut c, t, down(240, 240));
        }
        assert_ne!(c.view().unwrap().generation, generation);
        c.update(
            540,
            Some(Input {
                at_ms: 540,
                contact: Contact::Up,
            }),
            true,
            ctx(),
            generation,
        );
        c.update(
            550,
            Some(Input {
                at_ms: 550,
                contact: down(300, 160),
            }),
            true,
            ctx(),
            generation,
        );
        assert!(!c.press_started());
        assert_eq!(feed(&mut c, 560, Contact::Up), None);
    }
    #[test]
    fn long_hold_survives_brief_mailbox_contention_without_renewing_samples() {
        use crate::interaction::InputCache;
        let mut cache = InputCache::default();
        let mut c = Controller::default();
        c.update(
            0,
            cache.read(
                0,
                Some(Some(Input {
                    at_ms: 0,
                    contact: Contact::Up,
                })),
            ),
            true,
            ctx(),
            0,
        );
        for t in (10..=3010).step_by(10) {
            let update = if t == 500 || t == 1750 {
                None
            } else {
                Some(Some(Input {
                    at_ms: t,
                    contact: down(240, 240),
                }))
            };
            let input = cache.read(t, update);
            c.update(t, input, true, ctx(), 0);
        }
        assert!(
            c.view().is_some(),
            "Mailbox contention cancelled a healthy long press"
        );
        assert!(cache.read(3129, None).is_some());
        assert!(cache.read(3130, None).is_none());
        assert!(cache
            .read(
                3140,
                Some(Some(Input {
                    at_ms: 3140,
                    contact: down(240, 240)
                }))
            )
            .is_some());
        assert!(cache.read(3140, Some(None)).is_none());
        assert!(cache.read(3141, None).is_none());
    }
    #[test]
    fn cancel_and_inactivity_close_without_selection() {
        let mut c = Controller::default();
        let t = open(&mut c);
        feed(&mut c, t + 10, Contact::Up);
        feed(&mut c, t + 20, down(240, 344));
        assert_eq!(feed(&mut c, t + 30, Contact::Up), None);
        assert!(c.view().is_none());
        let mut c = Controller::default();
        let t = open(&mut c);
        for time in (t + 10..=t + 30000).step_by(10) {
            assert!(feed(&mut c, time, Contact::Up).is_none());
        }
        assert!(c.view().is_none());
    }
}
