//! Modal local IMU calibration controls. No aircraft controls or storage I/O.
use crate::{
    interaction::{Contact, Input, Rect, TOUCH_STALE_MS},
    raster::{rgb, Canvas, Target},
    render::Text,
};
use core::fmt::Write;
use instrument_core::{
    calibration::{self, Phase, Reason, View as SensorView},
    ImuCommand,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Level,
    Full,
    Reset,
    Back,
    Cancel,
    Save,
    Retry,
}
impl Button {
    pub fn rect(self, choose: bool) -> Rect {
        let (x, y) = if choose {
            match self {
                Self::Level => (92, 270),
                Self::Full => (244, 270),
                Self::Reset => (92, 322),
                _ => (244, 322),
            }
        } else {
            match self {
                Self::Cancel | Self::Back => (244, 332),
                _ => (92, 332),
            }
        };
        Rect {
            x,
            y,
            w: 144,
            h: 40,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Level => "LEVEL",
            Self::Full => "FULL 24",
            Self::Reset => "RESET",
            Self::Back => "BACK",
            Self::Cancel => "CANCEL",
            Self::Save => "SAVE",
            Self::Retry => "RETRY",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    pub generation: u32,
    pub choose: bool,
    pub awaiting: bool,
    pub available: bool,
    pub status: SensorView,
    pub pressed: Option<Button>,
    pub command_failed: bool,
}
impl View {
    pub fn buttons(self) -> [Option<Button>; 4] {
        if self.awaiting || self.status.phase == Phase::Saving {
            return [None; 4];
        }
        if self.choose {
            return [
                self.available.then_some(Button::Level),
                self.available.then_some(Button::Full),
                self.available.then_some(Button::Reset),
                Some(Button::Back),
            ];
        }
        match self.status.phase {
            Phase::Review => [Some(Button::Save), Some(Button::Cancel), None, None],
            Phase::Saved => [Some(Button::Back), None, None, None],
            Phase::Failed => [
                self.available.then_some(Button::Retry),
                Some(Button::Back),
                None,
                None,
            ],
            _ => [Some(Button::Cancel), None, None, None],
        }
    }
}
#[derive(Default)]
pub struct Controller {
    view: Option<View>,
    generation: u32,
    armed: bool,
    pressed_id: Option<u8>,
    last_tick: Option<u64>,
    activity: u64,
    refreshed: u64,
    pending: Option<ImuCommand>,
    closing: bool,
    pub press_started: bool,
}
pub struct Context {
    pub healthy: bool,
    pub visible: bool,
    pub presented: u32,
    pub status: SensorView,
    pub available: bool,
}
pub enum DismissError {
    Busy,
}
impl Controller {
    pub fn dismiss(&mut self) -> Result<Option<ImuCommand>, DismissError> {
        let Some(v) = self.view else {
            return Ok(None);
        };
        if v.awaiting || v.status.phase == Phase::Saving {
            return Err(DismissError::Busy);
        }
        if !v.choose && v.status.busy() {
            self.closing = true;
            Ok(self.request(ImuCommand::CalibrationCancel(v.status.job)))
        } else {
            self.view = None;
            Ok(None)
        }
    }
    pub fn view(&self) -> Option<View> {
        self.view
    }
    pub fn open(&mut self, status: SensorView, available: bool, now: u64) {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.view = Some(View {
            generation: self.generation,
            choose: !status.busy(),
            awaiting: false,
            available,
            status,
            pressed: None,
            command_failed: false,
        });
        self.armed = false;
        self.pressed_id = None;
        self.activity = now;
        self.refreshed = now;
        self.pending = None;
        self.closing = false;
    }
    pub fn acknowledge_command(&mut self, ok: bool) {
        if !ok {
            self.generation = self.generation.wrapping_add(1).max(1);
            if let Some(v) = &mut self.view {
                v.generation = self.generation;
                v.awaiting = false;
                v.command_failed = true;
                if self.pending.is_some_and(|c| {
                    matches!(
                        c,
                        ImuCommand::CalibrateLevel
                            | ImuCommand::CalibrateFull
                            | ImuCommand::CalibrationReset
                    )
                }) {
                    v.choose = true;
                }
            }
            self.pending = None;
            self.closing = false;
            self.armed = false;
        }
    }
    fn request(&mut self, command: ImuCommand) -> Option<ImuCommand> {
        let v = self.view.as_mut()?;
        v.awaiting = true;
        v.command_failed = false;
        v.choose = false;
        v.pressed = None;
        self.generation = self.generation.wrapping_add(1).max(1);
        v.generation = self.generation;
        self.pending = Some(command);
        self.armed = false;
        self.pressed_id = None;
        Some(command)
    }
    pub fn update(
        &mut self,
        now: u64,
        input: Option<Input>,
        context: Context,
    ) -> Option<ImuCommand> {
        let Context {
            healthy,
            visible,
            presented,
            status,
            available,
        } = context;
        self.press_started = false;
        let gap = self
            .last_tick
            .is_some_and(|at| now.checked_sub(at).is_none_or(|d| d >= TOUCH_STALE_MS));
        self.last_tick = Some(now);
        let mut v = self.view?;
        let changed = status.phase != v.status.phase || status.job != v.status.job;
        if changed || now.saturating_sub(self.refreshed) >= 200 {
            let ready = match self.pending {
                Some(
                    ImuCommand::CalibrateLevel
                    | ImuCommand::CalibrateFull
                    | ImuCommand::CalibrationReset,
                ) => status.job != v.status.job,
                Some(ImuCommand::CalibrationSave(job)) => {
                    status.job == job && status.phase != Phase::Review
                }
                Some(ImuCommand::CalibrationCancel(job)) => status.job == job && !status.busy(),
                _ => true,
            };
            if ready {
                if self.closing {
                    self.view = None;
                    self.pending = None;
                    return None;
                }
                if changed && (!v.choose || status.busy()) {
                    self.generation = self.generation.wrapping_add(1).max(1);
                    v.generation = self.generation;
                    v.pressed = None;
                    self.armed = false;
                    if status.busy() {
                        v.choose = false;
                    }
                }
                v.status = status;
                v.awaiting = false;
                self.pending = None;
                self.refreshed = now;
            }
        }
        v.available = available;
        self.view = Some(v);
        if v.choose && now.saturating_sub(self.activity) >= 30_000 {
            self.view = None;
            return None;
        }
        if !v.choose
            && v.status.phase == Phase::Review
            && now.saturating_sub(self.activity) >= 120_000
            && !v.awaiting
        {
            self.closing = true;
            return self.request(ImuCommand::CalibrationCancel(v.status.job));
        }
        let input = input.filter(|i| now.checked_sub(i.at_ms).is_some_and(|d| d < TOUCH_STALE_MS));
        if !healthy
            || !visible
            || gap
            || input.is_none()
            || input.is_some_and(|i| i.contact == Contact::Multiple)
            || presented != v.generation
        {
            self.armed = false;
            self.pressed_id = None;
            if let Some(v) = &mut self.view {
                v.pressed = None;
            }
            return None;
        }
        match input.unwrap().contact {
            Contact::Up => {
                self.armed = true;
                if let Some(b) = v.pressed {
                    self.activity = now;
                    self.pressed_id = None;
                    if let Some(v) = &mut self.view {
                        v.pressed = None;
                    }
                    match b {
                        Button::Level => return self.request(ImuCommand::CalibrateLevel),
                        Button::Full => return self.request(ImuCommand::CalibrateFull),
                        Button::Reset => return self.request(ImuCommand::CalibrationReset),
                        Button::Save => {
                            return self.request(ImuCommand::CalibrationSave(v.status.job))
                        }
                        Button::Cancel => {
                            self.closing = true;
                            return self.request(ImuCommand::CalibrationCancel(v.status.job));
                        }
                        Button::Back => {
                            self.view = None;
                            return None;
                        }
                        Button::Retry => {
                            return self.request(if v.status.reset {
                                ImuCommand::CalibrationReset
                            } else if v.status.full {
                                ImuCommand::CalibrateFull
                            } else {
                                ImuCommand::CalibrateLevel
                            })
                        }
                    }
                }
            }
            Contact::Down { id, x, y } => {
                if let Some(b) = v.pressed {
                    if self.pressed_id != Some(id) || !b.rect(v.choose).contains(x, y) {
                        self.view.as_mut().unwrap().pressed = None;
                        self.pressed_id = None;
                    }
                } else if self.armed {
                    let hit = v
                        .buttons()
                        .into_iter()
                        .flatten()
                        .find(|b| b.rect(v.choose).contains(x, y));
                    self.view.as_mut().unwrap().pressed = hit;
                    self.pressed_id = hit.map(|_| id);
                    self.press_started = hit.is_some();
                }
                self.armed = false;
            }
            Contact::Multiple => unreachable!(),
        }
        None
    }
}

const WHITE: u16 = rgb(235, 240, 245);
const AMBER: u16 = rgb(255, 195, 60);
const RED: u16 = rgb(255, 110, 110);
const GREEN: u16 = rgb(140, 225, 80);
const DIM: u16 = rgb(155, 175, 190);
fn reason(r: Reason) -> &'static str {
    match r {
        Reason::None => "",
        Reason::Still => "KEEP STILL",
        Reason::Direction => "ROTATE TO TARGET POSE",
        Reason::Noise => "MOTION / VIBRATION: HOLD STILL",
        Reason::Temperature => "WARM UP / USE FULL CALIBRATION",
        Reason::Gap => "SAMPLE GAP: TRY AGAIN",
        Reason::Saturated => "SATURATION / INVALID SAMPLE",
        Reason::Geometry => "FIT / COVERAGE REJECTED",
        Reason::Storage => "FLASH SAVE NOT VERIFIED",
        Reason::Timeout => "CALIBRATION TIMED OUT",
        Reason::Sensor => "SENSOR / SELF TEST FAILED",
        Reason::LevelTilt => "HOLD SCREEN UPRIGHT / TOP UP",
    }
}
pub fn draw<T: Target>(target: &mut T, v: View) {
    let mut c = Canvas::new(target);
    c.circle(240., 240., 238., rgb(70, 82, 93));
    c.circle(240., 240., 229., 0);
    // Fixed display axes remain behind the instructions and opaque buttons.
    let axes = rgb(75, 92, 105);
    c.stroke((14., 240.), (466., 240.), 1., axes);
    c.stroke((240., 14.), (240., 466.), 1., axes);
    c.text(240., 70., 20., "IMU CALIBRATION", WHITE);
    let status = v.status;
    if v.awaiting {
        c.text(240., 155., 17., "WAITING FOR SENSOR", AMBER);
        c.text(240., 190., 12., "COMMAND QUEUED", DIM);
    } else if v.choose {
        for (y, text) in [
            (115., "SCREEN FACING YOU / TOP UP"),
            (142., "USE A LEVEL FIXTURE"),
            (180., "LEVEL: GYRO ZERO + ALIGN"),
            (205., "FULL: 24 POSES THEN LEVEL"),
            (230., "KEEP CABLE SLACK / HOLD STILL"),
        ] {
            c.text(240., y, 12., text, DIM);
        }
        let state = if !v.available {
            "IMU / STORAGE UNAVAILABLE"
        } else if v.command_failed {
            "COMMAND BUSY: TRY AGAIN"
        } else if status.temperature_warning {
            "RECALIBRATE: TEMP CHANGED"
        } else if status.high_bias {
            "STORED HIGH GYRO BIAS"
        } else if status.full_calibrated {
            "FULL CALIBRATION STORED"
        } else if status.calibrated {
            "LEVEL CALIBRATION STORED"
        } else {
            "NOT CALIBRATED"
        };
        c.text(240., 392., 12., state, AMBER);
        let mut temp = Text::new();
        if status.temperature_available {
            let _ = write!(temp, "DIE {:.1}C", status.temperature_centi as f32 / 100.);
        } else {
            let _ = temp.write_str("DIE --.-C");
        }
        if status.calibrated {
            let _ = write!(
                temp,
                " / REF {:.1}C",
                status.reference_temperature_centi as f32 / 100.
            );
        } else {
            let _ = temp.write_str(" / REF NONE");
        }
        c.text(240., 372., 11., temp.str(), DIM);
    } else {
        match status.phase {
            Phase::Warmup => {
                c.text(240., 145., 18., "SENSOR CHECK / SETTLE", AMBER);
                c.text(240., 190., 13., "KEEP BOARD STILL", WHITE);
            }
            Phase::Collect | Phase::Level => {
                let mut title = Text::new();
                if status.phase == Phase::Level {
                    let _ = title.write_str("UPRIGHT LEVEL REFERENCE");
                } else {
                    let _ = write!(title, "POSE {} / 24", status.pose + 1);
                }
                c.text(240., 125., 15., title.str(), WHITE);
                if status.phase == Phase::Collect {
                    let target = calibration::target(status.pose as usize);
                    let mut line = 0;
                    for (axis, labels) in [
                        (0, ["SCREEN UP", "SCREEN DOWN"]),
                        (1, ["RIGHT EDGE DOWN", "LEFT EDGE DOWN"]),
                        (2, ["TOP EDGE UP", "TOP EDGE DOWN"]),
                    ] {
                        if target[axis] != 0. {
                            c.text(
                                240.,
                                170. + line as f32 * 23.,
                                14.,
                                labels[usize::from(target[axis] < 0.)],
                                AMBER,
                            );
                            line += 1;
                        }
                    }
                    c.text(240., 246., 11., "TILT BETWEEN NAMED DIRECTIONS", DIM);
                } else {
                    c.text(240., 177., 14., "SCREEN FACING YOU / TOP UP", AMBER);
                    c.text(240., 210., 12., "LEVEL FIXTURE / STILL FOR 5S", WHITE);
                }
                c.rect(100., 272., 280., 18., rgb(35, 45, 55));
                c.rect(100., 272., 2.8 * status.progress as f32, 18., GREEN);
                c.text(240., 309., 11., reason(status.reason), AMBER);
            }
            Phase::Review => {
                c.text(240., 112., 15., "REVIEW - NOT APPLIED", AMBER);
                if status.reset {
                    c.text(240., 170., 14., "REMOVE STORED CALIBRATION?", WHITE);
                    c.text(240., 210., 12., "RETURN TO UNCALIBRATED TILT", DIM);
                } else {
                    for (axis, name) in ["GX", "GY", "GZ"].into_iter().enumerate() {
                        let mut line = Text::new();
                        let _ = write!(
                            line,
                            "{} {:+.3} D/S",
                            name,
                            status.gyro_bias_milli[axis] as f32 / 1000.
                        );
                        c.text(240., 150. + axis as f32 * 25., 14., line.str(), WHITE);
                    }
                    let mut line = Text::new();
                    let _ = write!(line, "NOISE {:.3} D/S", status.noise_milli as f32 / 1000.);
                    c.text(240., 229., 12., line.str(), DIM);
                    if status.full {
                        let mut line = Text::new();
                        let _ = write!(
                            line,
                            "VALIDATION RMS {:.3} G",
                            status.rms_milli as f32 / 1000.
                        );
                        c.text(240., 253., 12., line.str(), DIM);
                    }
                    if status.high_bias {
                        c.text(240., 277., 13., "HIGH BIAS: OUT OF SPEC", RED);
                        c.text(240., 301., 11., "SAVE ONLY IF BOARD WAS STILL", AMBER);
                    }
                }
            }
            Phase::Saving => {
                c.text(240., 155., 17., "SAVING / VERIFYING FLASH", AMBER);
                c.text(240., 200., 12., "KEEP USB POWER CONNECTED", WHITE);
            }
            Phase::Saved => {
                c.text(240., 150., 19., "CALIBRATION SAVED", GREEN);
                c.text(240., 195., 13., "RESTORED AFTER REBOOT", WHITE);
            }
            Phase::Failed => {
                c.text(240., 145., 18., "CALIBRATION FAILED", RED);
                c.text(240., 191., 12., reason(status.reason), AMBER);
                c.text(240., 230., 11., "PREVIOUS CALIBRATION RETAINED", DIM);
            }
            _ => {
                c.text(240., 160., 17., "CALIBRATION CANCELLED", AMBER);
            }
        }
    }
    for b in v.buttons().into_iter().flatten() {
        let r = b.rect(v.choose);
        let bg = if v.pressed == Some(b) {
            rgb(0, 90, 156)
        } else {
            rgb(66, 80, 92)
        };
        c.rect(r.x as f32, r.y as f32, r.w as f32, r.h as f32, bg);
        c.text_on(
            r.x as f32 + r.w as f32 / 2.,
            r.y as f32 + r.h as f32 / 2.,
            16.,
            b.label(),
            (WHITE, bg),
        );
    }
    c.text(240., 422., 10., "REAL IMU ONLY / NO AIRCRAFT WRITES", DIM);
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    fn status(phase: Phase, job: u32) -> SensorView {
        SensorView {
            job,
            phase,
            storage_available: true,
            ..SensorView::default()
        }
    }
    fn feed(
        c: &mut Controller,
        at: u64,
        contact: Contact,
        status: SensorView,
    ) -> Option<ImuCommand> {
        let generation = c.view().map_or(0, |v| v.generation);
        c.update(
            at,
            Some(Input { at_ms: at, contact }),
            Context {
                healthy: true,
                visible: true,
                presented: generation,
                status,
                available: true,
            },
        )
    }
    fn down(button: Button, choose: bool) -> Contact {
        let r = button.rect(choose);
        Contact::Down {
            id: 1,
            x: r.x + 10,
            y: r.y + 10,
        }
    }
    #[test]
    fn level_requires_visible_release_and_start_failure_does_not_fake_progress() {
        let mut c = Controller::default();
        let s = status(Phase::Idle, 0);
        c.open(s, true, 0);
        let gen = c.view().unwrap().generation;
        c.update(
            0,
            Some(Input {
                at_ms: 0,
                contact: Contact::Up,
            }),
            Context {
                healthy: true,
                visible: true,
                presented: gen - 1,
                status: s,
                available: true,
            },
        );
        assert_eq!(feed(&mut c, 10, down(Button::Level, true), s), None);
        assert_eq!(feed(&mut c, 20, Contact::Up, s), None);
        feed(&mut c, 30, down(Button::Level, true), s);
        assert_eq!(
            feed(&mut c, 40, Contact::Up, s),
            Some(ImuCommand::CalibrateLevel)
        );
        assert!(c.view().unwrap().awaiting);
        assert!(c.view().unwrap().buttons().into_iter().all(|b| b.is_none()));
        c.acknowledge_command(false);
        assert!(c.view().unwrap().choose && c.view().unwrap().command_failed);
    }
    #[test]
    fn old_jobs_interrupted_touches_and_unseen_pages_cannot_save() {
        for cause in 0..4 {
            let mut c = Controller::default();
            let s = status(Phase::Review, 9);
            c.open(s, true, 0);
            feed(&mut c, 0, Contact::Up, s);
            feed(&mut c, 10, down(Button::Save, false), s);
            match cause {
                0 => {
                    feed(
                        &mut c,
                        20,
                        Contact::Down {
                            id: 1,
                            x: 430,
                            y: 430,
                        },
                        s,
                    );
                }
                1 => {
                    feed(&mut c, 20, Contact::Multiple, s);
                }
                2 => {
                    feed(&mut c, 20, Contact::Up, status(Phase::Review, 10));
                }
                _ => {
                    c.update(
                        20,
                        None,
                        Context {
                            healthy: true,
                            visible: false,
                            presented: 0,
                            status: s,
                            available: true,
                        },
                    );
                }
            }
            assert_eq!(feed(&mut c, 30, Contact::Up, s), None);
        }
    }
    #[test]
    fn save_uses_exact_job_and_cancel_waits_for_worker_confirmation() {
        let mut c = Controller::default();
        let review = status(Phase::Review, 77);
        c.open(review, true, 0);
        feed(&mut c, 0, Contact::Up, review);
        feed(&mut c, 10, down(Button::Save, false), review);
        assert_eq!(
            feed(&mut c, 20, Contact::Up, review),
            Some(ImuCommand::CalibrationSave(77))
        );
        c.acknowledge_command(true);
        feed(&mut c, 30, Contact::Up, status(Phase::Saving, 77));
        assert!(c.view().unwrap().buttons().into_iter().all(|b| b.is_none()));
        feed(&mut c, 40, Contact::Up, status(Phase::Saved, 77));
        assert_eq!(c.view().unwrap().status.phase, Phase::Saved);
        let collect = status(Phase::Collect, 78);
        c.open(collect, true, 100);
        feed(&mut c, 100, Contact::Up, collect);
        feed(&mut c, 110, down(Button::Cancel, false), collect);
        assert_eq!(
            feed(&mut c, 120, Contact::Up, collect),
            Some(ImuCommand::CalibrationCancel(78))
        );
        c.acknowledge_command(true);
        feed(&mut c, 130, Contact::Up, collect);
        assert!(c.view().is_some());
        feed(&mut c, 140, Contact::Up, status(Phase::Cancelled, 78));
        assert!(c.view().is_none());
    }
    #[test]
    fn opaque_calibration_pages_and_closing_restore_all_three_buffers_exactly() {
        use crate::{
            damage::FrameHistory,
            render::{draw_frame, CachedRenderer},
            Snapshot,
        };
        let mut app = instrument_core::App::new();
        app.apply(instrument_core::Command::LevelPreset).unwrap();
        app.tick(0);
        let base = Snapshot::from_app(&app);
        let mut background = std::vec![0;480*480];
        let mut renderer = CachedRenderer::new(&mut background, 480, 480).unwrap();
        let mut buffers = core::array::from_fn::<_, 3, _>(|_| std::vec![0;480*480]);
        let mut histories = core::array::from_fn::<_, 3, _>(|_| FrameHistory::new());
        let mut full = std::vec![0;480*480];
        for (n, phase) in [
            Phase::Idle,
            Phase::Warmup,
            Phase::Collect,
            Phase::Level,
            Phase::Review,
            Phase::Saving,
            Phase::Saved,
            Phase::Failed,
            Phase::Idle,
        ]
        .into_iter()
        .enumerate()
        {
            let mut s = base;
            s.calibration = (n != 8).then_some(View {
                generation: 1,
                choose: n == 0,
                awaiting: false,
                available: true,
                status: SensorView {
                    phase,
                    job: 9,
                    pose: 14,
                    progress: 65,
                    high_bias: true,
                    gyro_bias_milli: [-1000, 30100, 25],
                    ..SensorView::default()
                },
                pressed: None,
                command_failed: false,
            });
            assert!(draw_frame(&mut full, 480, 480, s, false));
            for (pixels, history) in buffers.iter_mut().zip(&mut histories) {
                assert!(renderer.draw_incremental(pixels, history, s, false));
                assert_eq!(&*pixels, &full, "page {phase:?}");
            }
            if s.calibration.is_some() {
                assert_eq!(s.numeric_lifetime_ms(), None);
                assert_eq!(s.displayed_quality(), None);
            }
        }
    }
}
