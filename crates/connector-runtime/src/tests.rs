use super::*;
use instrument_core::{App, Command, Quality};
use sim_bridge::{Capabilities, ControlError};
use simulator_link::Datum;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};
#[derive(Clone)]
struct TestClock(Rc<Cell<u64>>);
impl Clock for TestClock {
    fn now_ms(&self) -> u64 {
        self.0.get()
    }
}
#[derive(Default)]
struct ProviderData {
    observations: VecDeque<Observation>,
    writes: Vec<Request>,
    reply: Option<Reply>,
    enabled: bool,
    immediate_confirmation: bool,
    status_confirms_immediately: bool,
    recovery: Option<RecoveryReason>,
}
struct TestProvider(Rc<RefCell<ProviderData>>);
const READINGS: &[Key] = &[
    Key::primary(Quantity::IndicatedAltitudeFt),
    Key::primary(Quantity::PressureAltitudeFt),
    Key::primary(Quantity::BarometerInHg),
];
impl SimulatorProvider for TestProvider {
    fn recovery_reason(&self) -> Option<RecoveryReason> {
        self.0.borrow().recovery
    }
    fn kind(&self) -> Provider {
        Provider::Msfs2020
    }
    fn capabilities(&self) -> Capabilities<'_> {
        Capabilities {
            readings: READINGS,
            controls: if self.0.borrow().enabled {
                &[(Control::SetBarometer, 0)]
            } else {
                &[]
            },
        }
    }
    fn enable_controls(&mut self, _: u64) -> Result<(), ControlError> {
        self.0.borrow_mut().enabled = true;
        Ok(())
    }
    fn poll(&mut self, _: u64) -> Option<Observation> {
        self.0.borrow_mut().observations.pop_front()
    }
    fn control(&mut self, request: Request, _: u64) -> Result<Reply, ControlError> {
        let mut data = self.0.borrow_mut();
        data.writes.push(request);
        let reply = Reply {
            session: request.session,
            id: request.id,
            status: if data.immediate_confirmation {
                Status::Confirmed
            } else {
                Status::Sent
            },
        };
        data.reply = Some(reply);
        Ok(reply)
    }
    fn control_status(&self) -> Option<Reply> {
        let data = self.0.borrow();
        data.reply.map(|r| {
            if data.status_confirms_immediately {
                Reply {
                    status: Status::Confirmed,
                    ..r
                }
            } else {
                r
            }
        })
    }
}
struct DeviceData {
    app: App,
    commands: Vec<String>,
    bad_ack: bool,
    duplicate_control: bool,
    control_delay: u64,
    switch_before_claim: bool,
}
impl Default for DeviceData {
    fn default() -> Self {
        Self {
            app: App::new(),
            commands: vec![],
            bad_ack: false,
            duplicate_control: false,
            control_delay: 0,
            switch_before_claim: false,
        }
    }
}
struct TestDevice {
    data: Rc<RefCell<DeviceData>>,
    clock: TestClock,
}
fn tick(app: &mut App, now: u64) {
    while app.now_ms() < now {
        app.tick((app.now_ms() + 10).min(now));
    }
}
impl DeviceLink for TestDevice {
    fn request(&mut self, command: &str) -> io::Result<String> {
        let mut d = self.data.borrow_mut();
        d.commands.push(command.into());
        let now = self.clock.now_ms() + 5000; // receiver epoch deliberately differs
        tick(&mut d.app, now);
        if command.starts_with("control claim ") && d.switch_before_claim {
            d.app
                .apply(Command::Select(instrument_core::Instrument::Attitude))
                .unwrap();
        }
        let mut response = text_cli::execute(&mut d.app, text_cli::parse(command), now)
            .as_str()
            .trim()
            .to_string();
        if command.starts_with("T1 ") {
            if d.bad_ack {
                response = response.replace("session=10 ", "session=11 ");
            }
            if d.duplicate_control {
                response = response.replace("control=0", "control=1");
            }
        }
        if command == "control poll" {
            self.clock.0.set(self.clock.now_ms() + d.control_delay);
        }
        Ok(response)
    }
}
struct Rig {
    session: Session<TestDevice, TestClock>,
    clock: TestClock,
    provider: Rc<RefCell<ProviderData>>,
    device: Rc<RefCell<DeviceData>>,
}
fn rig(writeback: bool) -> Rig {
    rig_selected(writeback, instrument_core::Instrument::Altimeter)
}
fn rig_selected(writeback: bool, selected: instrument_core::Instrument) -> Rig {
    rig_profile(writeback, selected, false)
}
fn rig_profile(writeback: bool, selected: instrument_core::Instrument, multi: bool) -> Rig {
    let clock = TestClock(Rc::new(Cell::new(0)));
    let device = Rc::new(RefCell::new(DeviceData::default()));
    device
        .borrow_mut()
        .app
        .apply(Command::Select(selected))
        .unwrap();
    let provider = Rc::new(RefCell::new(ProviderData::default()));
    let session = Session::open_profile(
        Box::new(TestProvider(provider.clone())),
        TestDevice {
            data: device.clone(),
            clock: clock.clone(),
        },
        clock.clone(),
        10,
        writeback,
        multi,
    )
    .unwrap();
    Rig {
        session,
        clock,
        provider,
        device,
    }
}
#[test]
fn altimeter_profile_selects_its_face_on_initial_open_and_recovery() {
    use instrument_core::Instrument;
    let mut r = rig_selected(false, Instrument::Attitude);
    r.feed(0, Lifecycle::Running, 29.92);
    assert_eq!(r.device.borrow().app.selected(), Instrument::Altimeter);
    assert_eq!(
        r.device.borrow().app.altimeter_reading(5000).value,
        Some(4520.)
    );
    r.provider.borrow_mut().recovery = Some(RecoveryReason::SessionChanged);
    r.feed(66, Lifecycle::Disconnected, 29.92);
    r.device
        .borrow_mut()
        .app
        .apply(Command::Select(Instrument::Attitude))
        .unwrap();
    let next = Rc::new(RefCell::new(ProviderData::default()));
    let mut session = r
        .session
        .into_recovery()
        .unwrap()
        .resume(Box::new(TestProvider(next.clone())), r.clock.clone())
        .unwrap();
    assert_eq!(r.device.borrow().app.selected(), Instrument::Altimeter);
    assert_eq!(r.device.borrow().app.altimeter_reading(5066).value, None);
    next.borrow_mut()
        .observations
        .push_back(observation(100, Lifecycle::Running, 30.01));
    r.clock.0.set(100);
    session.poll().unwrap();
    assert_eq!(
        r.device.borrow().app.altimeter_reading(5100).value,
        Some(4520.)
    );
}
fn observation(at: u64, state: Lifecycle, baro: f32) -> Observation {
    Observation::altimeter(
        at,
        state,
        Datum::Value(4520.),
        Datum::Value(4500.),
        Datum::Value(baro),
    )
    .unwrap()
}
impl Rig {
    fn feed(&mut self, at: u64, state: Lifecycle, baro: f32) -> Tick {
        self.clock.0.set(at);
        self.provider
            .borrow_mut()
            .observations
            .push_back(observation(at, state, baro));
        self.session.poll().unwrap()
    }
    fn edit(&mut self, baro: f32) {
        self.device
            .borrow_mut()
            .app
            .apply(Command::Baro(baro))
            .unwrap();
    }
}
#[test]
fn receiver_clock_and_measurement_age_are_not_host_timestamps() {
    let mut r = rig(false);
    r.clock.0.set(200);
    r.provider
        .borrow_mut()
        .observations
        .push_back(observation(100, Lifecycle::Running, 29.92));
    let t = r.session.poll().unwrap();
    let frame = simulator_link::Frame::decode(&t.telemetry.unwrap().0).unwrap();
    assert_eq!(frame.receiver_ms, 5200);
    assert_eq!(frame.age_ms, 101);
    assert_eq!(
        r.device.borrow().app.altimeter_reading(5200).value,
        Some(4520.)
    );
    // Idle polls must not republish cached observations or refresh their age.
    r.clock.0.set(1300);
    assert!(r.session.poll().unwrap().telemetry.is_none());
    tick(&mut r.device.borrow_mut().app, 6300);
    assert_eq!(
        r.device.borrow().app.altimeter_reading(6300).quality,
        Quality::Stale
    );
}
#[test]
fn read_only_never_enables_controls_and_pause_hides_numeric_values() {
    let mut r = rig(false);
    r.feed(0, Lifecycle::Running, 29.92);
    assert!(r.device.borrow_mut().app.apply(Command::Baro(30.)).is_err());
    r.feed(33, Lifecycle::Paused, 29.92);
    assert_eq!(r.device.borrow().app.altimeter_reading(5033).value, None);
    assert!(!r.provider.borrow().enabled);
    assert!(!r
        .device
        .borrow()
        .commands
        .iter()
        .any(|c| c.starts_with("control enable")));
}
#[test]
fn writeback_requires_later_telemetry_and_is_not_replayed() {
    let mut r = rig(true);
    r.feed(0, Lifecycle::Running, 29.92);
    r.edit(29.93);
    r.feed(33, Lifecycle::Running, 29.92);
    assert_eq!(r.provider.borrow().writes.len(), 1);
    assert_eq!(
        r.device.borrow().app.external_baro(5033).unwrap().value,
        Some(29.92)
    );
    r.provider.borrow_mut().reply.as_mut().unwrap().status = Status::Confirmed;
    r.clock.0.set(50);
    r.session.poll().unwrap(); // no delivered observation yet
    assert_eq!(r.device.borrow().app.control_status(), Some(Status::Sent));
    r.feed(66, Lifecycle::Running, 29.93);
    assert_eq!(
        r.device.borrow().app.control_status(),
        Some(Status::Confirmed)
    );
    r.device.borrow_mut().duplicate_control = true;
    r.feed(99, Lifecycle::Running, 29.93);
    assert_eq!(r.provider.borrow().writes.len(), 1);
}
#[test]
fn expired_request_is_rejected_without_simulator_write() {
    let mut r = rig(true);
    r.feed(0, Lifecycle::Running, 29.92);
    r.edit(29.93);
    r.device.borrow_mut().control_delay = 501;
    r.feed(33, Lifecycle::Running, 29.92);
    assert!(r.provider.borrow().writes.is_empty());
    assert_eq!(
        r.device.borrow().app.control_status(),
        Some(Status::Rejected)
    );
}
#[test]
fn mismatched_ack_latches_fault_and_cannot_send_more_commands() {
    let mut r = rig(true);
    r.device.borrow_mut().bad_ack = true;
    r.provider
        .borrow_mut()
        .observations
        .push_back(observation(0, Lifecycle::Running, 29.92));
    assert!(r.session.poll().is_err());
    let commands = r.device.borrow().commands.len();
    assert_eq!(r.session.state(), State::Faulted);
    assert!(r.session.poll().is_err());
    assert!(r.session.stop().is_err());
    assert_eq!(r.device.borrow().commands.len(), commands);
}
#[test]
fn disconnect_discards_controls_and_never_resumes_same_session() {
    let mut r = rig(true);
    r.feed(0, Lifecycle::Running, 29.92);
    r.edit(29.93);
    r.feed(33, Lifecycle::Running, 29.92);
    r.feed(66, Lifecycle::Disconnected, 29.92);
    assert_eq!(r.session.state(), State::Disconnected);
    assert_eq!(
        r.device.borrow().app.control_status(),
        Some(Status::Rejected)
    );
    assert!(r.session.poll().is_err());
    assert_eq!(r.provider.borrow().writes.len(), 1);
}
#[test]
fn provider_cannot_confirm_in_initial_write_response() {
    let mut r = rig(true);
    r.feed(0, Lifecycle::Running, 29.92);
    r.edit(29.93);
    r.provider.borrow_mut().immediate_confirmation = true;
    r.provider
        .borrow_mut()
        .observations
        .push_back(observation(33, Lifecycle::Running, 29.92));
    r.clock.0.set(33);
    assert!(r.session.poll().is_err());
    assert_eq!(r.session.state(), State::Faulted);
    assert_eq!(r.device.borrow().app.control_status(), Some(Status::Sent));
}
#[test]
fn clean_stop_sends_missing_disconnected_and_backward_clock_faults() {
    let mut r = rig(false);
    r.feed(0, Lifecycle::Running, 29.92);
    let t = r.session.stop().unwrap();
    let f = simulator_link::Frame::decode(&t.telemetry.unwrap().0).unwrap();
    assert_eq!(f.state, Lifecycle::Disconnected);
    assert_eq!(f.indicated_ft, Datum::Missing);
    let mut r = rig(false);
    r.feed(33, Lifecycle::Running, 29.92);
    r.clock.0.set(32);
    assert!(r.session.poll().is_err());
}
#[test]
fn duplicate_identity_fields_are_rejected() {
    assert!(attribute("OK session=1 session=2", "session").is_err());
    assert!(number("OK session=+1", "session").is_err());
}

#[test]
fn status_confirmation_cannot_use_the_pre_write_frame() {
    let mut r = rig(true);
    r.feed(0, Lifecycle::Running, 29.92);
    // A target equal to current value still needs a post-write observation.
    r.edit(29.92);
    r.provider.borrow_mut().status_confirms_immediately = true;
    let t = r.feed(33, Lifecycle::Running, 29.92);
    assert_eq!(t.controls.len(), 1);
    assert_eq!(r.device.borrow().app.control_status(), Some(Status::Sent));
    r.feed(66, Lifecycle::Running, 29.92);
    assert_eq!(
        r.device.borrow().app.control_status(),
        Some(Status::Confirmed)
    );
}

#[test]
fn late_rejection_after_pause_is_tolerated_but_never_retried() {
    let mut r = rig(true);
    r.feed(0, Lifecycle::Running, 29.92);
    r.edit(29.93);
    r.feed(33, Lifecycle::Running, 29.92);
    r.provider.borrow_mut().reply.as_mut().unwrap().status = Status::Rejected;
    r.feed(66, Lifecycle::Paused, 29.92);
    assert_eq!(r.session.state(), State::Active);
    assert_eq!(
        r.device.borrow().app.control_status(),
        Some(Status::Rejected)
    );
    r.feed(99, Lifecycle::Running, 29.92);
    assert_eq!(r.provider.borrow().writes.len(), 1);
}

#[test]
fn recovered_session_preserves_mode_but_discards_old_writes_and_uses_simulator_values() {
    let mut r = rig(true);
    r.feed(0, Lifecycle::Running, 29.92);
    r.edit(29.93);
    r.feed(33, Lifecycle::Running, 29.92);
    let old_request = r.provider.borrow().writes[0];
    r.provider.borrow_mut().recovery = Some(RecoveryReason::SessionChanged);
    assert_eq!(r.session.recovery_reason(), None); // hint cannot bypass ACKed disconnect
    r.feed(66, Lifecycle::Disconnected, 29.92);
    let handoff = r.session.into_recovery().unwrap();
    assert_eq!(handoff.session_id(), 11);
    let next = Rc::new(RefCell::new(ProviderData::default()));
    let mut session = handoff
        .resume(Box::new(TestProvider(next.clone())), r.clock.clone())
        .unwrap();
    assert_eq!(r.device.borrow().app.altimeter_reading(5066).value, None);
    assert!(r.device.borrow().app.control_enabled());
    // Capability is retained, but editing remains blocked until fresh readings.
    assert!(r
        .device
        .borrow_mut()
        .app
        .apply(Command::Baro(29.92))
        .is_err());
    assert!(r
        .device
        .borrow_mut()
        .app
        .apply(Command::ControlReply(Reply {
            session: old_request.session,
            id: old_request.id,
            status: Status::Confirmed
        }))
        .is_err());
    next.borrow_mut()
        .observations
        .push_back(observation(100, Lifecycle::Running, 30.01));
    r.clock.0.set(100);
    let packet = session.poll().unwrap().telemetry.unwrap().0;
    let frame = simulator_link::Frame::decode(&packet).unwrap();
    assert_eq!((frame.session, frame.sequence), (11, 1));
    assert_eq!(
        r.device.borrow().app.external_baro(5100).unwrap().value,
        Some(30.01)
    );
    assert!(r
        .device
        .borrow_mut()
        .app
        .apply(Command::Telemetry(simulator_link::Frame {
            session: 10,
            sequence: 999,
            receiver_ms: 5100,
            ..frame
        }))
        .is_err());
    assert!(next.borrow().enabled);
    assert!(next.borrow().writes.is_empty());
    assert_eq!(r.provider.borrow().writes.len(), 1);
    assert_eq!(
        r.device
            .borrow()
            .commands
            .iter()
            .filter(|c| c.starts_with("control enable"))
            .count(),
        2
    );
    // Only a NEW user edit can write to the new aircraft, never the old 29.93 request.
    r.device
        .borrow_mut()
        .app
        .apply(Command::Baro(30.02))
        .unwrap();
    next.borrow_mut()
        .observations
        .push_back(observation(133, Lifecycle::Running, 30.01));
    r.clock.0.set(133);
    session.poll().unwrap();
    assert_eq!(next.borrow().writes.len(), 1);
    assert_eq!(
        (
            next.borrow().writes[0].session,
            next.borrow().writes[0].cents
        ),
        (11, 3002)
    );
    assert_eq!(
        r.device.borrow().app.external_baro(5133).unwrap().value,
        Some(30.01)
    );
    next.borrow_mut().reply.as_mut().unwrap().status = Status::Confirmed;
    next.borrow_mut()
        .observations
        .push_back(observation(166, Lifecycle::Running, 30.02));
    r.clock.0.set(166);
    session.poll().unwrap();
    assert_eq!(
        r.device.borrow().app.external_baro(5166).unwrap().value,
        Some(30.02)
    );
    assert_eq!(
        r.device.borrow().app.control_status(),
        Some(Status::Confirmed)
    );
}

#[test]
fn default_recovery_overwrites_altitude_and_qnh_without_writeback_or_user_commands() {
    let mut r = rig(false);
    r.feed(0, Lifecycle::Running, 30.12);
    r.provider.borrow_mut().recovery = Some(RecoveryReason::SessionChanged);
    r.feed(66, Lifecycle::Disconnected, 30.12);
    let next = Rc::new(RefCell::new(ProviderData::default()));
    let mut session = r
        .session
        .into_recovery()
        .unwrap()
        .resume(Box::new(TestProvider(next.clone())), r.clock.clone())
        .unwrap();
    assert_eq!(r.device.borrow().app.altimeter_reading(5066).value, None);
    assert!(!r.device.borrow().app.control_enabled());
    next.borrow_mut().observations.push_back(
        Observation::altimeter(
            100,
            Lifecycle::Running,
            Datum::Value(9100.),
            Datum::Value(9000.),
            Datum::Value(29.70),
        )
        .unwrap(),
    );
    r.clock.0.set(100);
    session.poll().unwrap();
    assert_eq!(
        r.device.borrow().app.altimeter_reading(5100).value,
        Some(9100.)
    );
    assert_eq!(
        r.device.borrow().app.external_baro(5100).unwrap().value,
        Some(29.70)
    );
    assert!(next.borrow().writes.is_empty());
    assert!(!next.borrow().enabled);
    assert!(!r
        .device
        .borrow()
        .commands
        .iter()
        .any(|c| c.starts_with("control enable")));
}

#[test]
fn recovery_refuses_clean_eof_faults_active_sessions_and_id_exhaustion() {
    let mut r = rig(false);
    r.feed(0, Lifecycle::Disconnected, 29.92);
    assert!(r.session.into_recovery().is_err());
    let r = rig(false);
    assert!(r.session.into_recovery().is_err());
    let mut r = rig(false);
    r.provider.borrow_mut().recovery = Some(RecoveryReason::SessionChanged);
    r.device.borrow_mut().bad_ack = true;
    r.provider
        .borrow_mut()
        .observations
        .push_back(observation(0, Lifecycle::Disconnected, 29.92));
    assert!(r.session.poll().is_err());
    assert!(r.session.into_recovery().is_err());
    let mut r = rig(false);
    r.feed(0, Lifecycle::Disconnected, 29.92);
    r.provider.borrow_mut().recovery = Some(RecoveryReason::SessionChanged);
    r.session.session = u64::MAX;
    assert!(r.session.into_recovery().is_err());
}

#[test]
fn retry_delay_is_bounded_and_never_wraps() {
    let mut retry = RetryDelay::default();
    assert_eq!(
        (0..5).map(|_| retry.next_ms()).collect::<Vec<_>>(),
        vec![1000, 2000, 4000, 8000, 8000]
    );
    for _ in 0..1000 {
        assert_eq!(retry.next_ms(), 8000);
    }
}

#[test]
fn multi_profile_preserves_active_face_and_only_claimed_explicit_edits_write() {
    use instrument_core::Instrument;
    let mut r = rig_profile(true, Instrument::Attitude, true);
    r.feed(0, Lifecycle::Running, 29.92);
    assert_eq!(r.device.borrow().app.selected(), Instrument::Attitude);
    assert!(r.provider.borrow().writes.is_empty());
    assert!(r.device.borrow_mut().app.apply(Command::Baro(30.)).is_err());
    r.device
        .borrow_mut()
        .app
        .apply(Command::Select(Instrument::Altimeter))
        .unwrap();
    r.edit(29.93);
    r.feed(33, Lifecycle::Running, 29.92);
    assert_eq!(r.provider.borrow().writes.len(), 1);
    // A dispatch already admitted by the board finishes before deactivation.
    assert!(r
        .device
        .borrow_mut()
        .app
        .apply(Command::Select(Instrument::Attitude))
        .is_err());
    assert!(r
        .device
        .borrow()
        .commands
        .iter()
        .any(|c| c == "control claim 10 1"));
    r.provider.borrow_mut().reply.as_mut().unwrap().status = Status::Confirmed;
    r.feed(66, Lifecycle::Running, 29.93);
    r.device
        .borrow_mut()
        .app
        .apply(Command::Select(Instrument::Attitude))
        .unwrap();
    r.feed(99, Lifecycle::Running, 30.01);
    assert_eq!(r.provider.borrow().writes.len(), 1);
    assert_eq!(r.device.borrow().app.baro_inhg(), 30.01);
    r.provider.borrow_mut().recovery = Some(RecoveryReason::SessionChanged);
    r.feed(132, Lifecycle::Disconnected, 30.01);
    let next = Rc::new(RefCell::new(ProviderData::default()));
    r.session = r
        .session
        .into_recovery()
        .unwrap()
        .resume(Box::new(TestProvider(next.clone())), r.clock.clone())
        .unwrap();
    assert_eq!(r.device.borrow().app.selected(), Instrument::Attitude);
    assert!(next.borrow().writes.is_empty());
}

#[test]
fn gauge_switch_between_poll_and_claim_prevents_aircraft_write() {
    let mut r = rig_profile(true, instrument_core::Instrument::Altimeter, true);
    r.feed(0, Lifecycle::Running, 29.92);
    r.edit(29.93);
    r.device.borrow_mut().switch_before_claim = true;
    r.feed(33, Lifecycle::Running, 29.92);
    assert!(r.provider.borrow().writes.is_empty());
    assert!(r.device.borrow().app.control_request().is_none());
    r.device
        .borrow_mut()
        .app
        .apply(Command::Select(instrument_core::Instrument::Altimeter))
        .unwrap();
    r.feed(66, Lifecycle::Running, 30.01);
    assert!(r.provider.borrow().writes.is_empty());
}

#[test]
fn multi_clock_reference_advances_between_ack_and_new_measurement_without_renewing_old_data() {
    let mut r = rig_profile(false, instrument_core::Instrument::Attitude, true);
    r.feed(0, Lifecycle::Running, 29.92);
    r.clock.0.set(40);
    let mut reading = observation(40, Lifecycle::Running, 29.92);
    reading.measurements.push(simulator_link::measurements::Measurement {
        key: Key::primary(Quantity::PitchDeg), age_ms: 35,
        value: simulator_link::measurements::Value::Valid(12.),
    }).unwrap();
    r.provider.borrow_mut().observations.push_back(reading);
    let tick = r.session.poll().unwrap();
    let packet = simulator_link::multi::Frame::decode(&tick.telemetry.unwrap().0).unwrap();
    // The frame is acquired after the previous ACK. Only clock-alignment
    // uncertainty should be added, not the whole 40 ms since that ACK.
    let age = r.device.borrow().app.reading(instrument_core::Field::PitchDeg, 5040).age_ms.unwrap();
    assert!(age <= 38, "35 ms sample was incorrectly aged to {age} ms");
    assert!(packet.receiver_ms > 5000 && packet.receiver_ms <= 5040);
    // Holding an old observation must still add its full actual elapsed age.
    r.clock.0.set(70);
    r.provider.borrow_mut().observations.push_back(observation(40, Lifecycle::Running, 29.92));
    let tick = r.session.poll().unwrap();
    let packet = simulator_link::multi::Frame::decode(&tick.telemetry.unwrap().0).unwrap();
    assert!(packet.measurements.iter().all(|m| m.age_ms >= 31));
}
