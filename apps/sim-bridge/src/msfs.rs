//! Primary MSFS 2020 altimeter profile. SDK dependencies stay behind Api.
use crate::{Capabilities, Observation, SimulatorProvider, READ_ONLY};
use crate::{Control, ControlError, RecoveryReason};
use simulator_link::control::{Reply, Request, Status, CONFIRM_MS, READBACK_TOLERANCE};
use simulator_link::{
    measurements::{Key, Measurement, Measurements, Quantity, Value},
    Lifecycle, Provider,
};
use std::io;

#[derive(Clone, Copy, Debug)]
pub enum Notice {
    Open,
    Ignore,
    Quit,
    Exception {
        code: u32,
        send: u32,
        index: u32,
    },
    System {
        event: u32,
        value: u32,
    },
    Heartbeat {
        request: u32,
        running: u32,
    },
    Data {
        request: u32,
        object: u32,
        values: [f64; 4],
    },
}
pub trait Api {
    fn set_baro(&mut self, _pressure16: u32) -> io::Result<()> {
        Err(io::Error::other("write unsupported"))
    }
    /// Must be nonblocking. The SDK-owned packet is copied before returning.
    fn next(&mut self) -> io::Result<Option<Notice>>;
    fn request(&mut self, id: u32, heartbeat: bool) -> io::Result<()>;
}
pub struct Msfs<A> {
    api: A,
    control_session: Option<u64>,
    control_last_id: u64,
    control_pending: Option<(Request, u64, Status)>,
    last_valid: Option<u64>,
    sim: Option<bool>,
    pause: Option<bool>,
    opened: bool,
    terminal: bool,
    recovery: Option<RecoveryReason>,
    published: bool,
    object: Option<u32>,
    next_id: u32,
    pending: Option<(u32, u64)>,
    heartbeat: Option<(u32, u64)>,
    last_heartbeat: Option<u64>,
    last_poll: Option<u64>,
    last_state_publish: Option<u64>,
    started: Option<u64>,
    latest: Option<Observation>,
    state: Lifecycle,
}
impl<A: Api> Msfs<A> {
    pub fn new(api: A) -> Self {
        Self {
            api,
            control_session: None,
            control_last_id: 0,
            control_pending: None,
            last_valid: None,
            sim: None,
            pause: None,
            opened: false,
            terminal: false,
            recovery: None,
            published: false,
            object: None,
            next_id: 1,
            pending: None,
            heartbeat: None,
            last_heartbeat: None,
            last_poll: None,
            last_state_publish: None,
            started: None,
            latest: None,
            state: Lifecycle::Loading,
        }
    }
    fn empty(&mut self, now: u64, state: Lifecycle) {
        self.last_valid = None;
        if let Some((r, at, Status::Sent)) = self.control_pending {
            self.control_pending = Some((r, at, Status::Rejected));
        }
        self.pending = None;
        self.state = state;
        self.latest = Some(Observation {
            at_ms: now,
            state,
            measurements: Measurements::new(),
        });
    }
    fn end(&mut self, now: u64, reason: impl std::fmt::Display) {
        if !self.terminal {
            eprintln!("MSFS session ended: {reason}. A fresh session is required.");
            self.terminal = true;
            self.empty(now, Lifecycle::Disconnected);
        }
    }
    fn recoverable_end(&mut self, now: u64, recovery: RecoveryReason, reason: &str) {
        if !self.terminal {
            self.recovery = Some(recovery);
            self.end(now, reason);
        }
    }
    fn lifecycle(&mut self, now: u64) {
        let state = if self.opened && self.pause == Some(true) {
            Lifecycle::Paused
        } else if self.opened && self.sim == Some(true) {
            match self.pause {
                Some(false) => Lifecycle::Running,
                Some(true) => Lifecycle::Paused,
                None => Lifecycle::Loading,
            }
        } else {
            Lifecycle::Loading
        };
        if state != self.state {
            self.empty(now, state);
        }
    }
    fn running(&mut self, now: u64, value: u32) {
        match value {
            0 => self.sim = Some(false),
            1 => self.sim = Some(true),
            _ => {
                self.end(now, "unknown Sim state");
                return;
            }
        }
        self.lifecycle(now);
    }
    fn event(&mut self, now: u64, notice: Notice) {
        match notice {
            Notice::Ignore => (),
            Notice::Open => {
                self.opened = true;
                self.lifecycle(now);
            }
            Notice::Quit => {
                self.recoverable_end(now, RecoveryReason::ConnectionLost, "simulator quit")
            }
            Notice::Exception { code, send, index } => self.end(
                now,
                format_args!("SDK exception {code}, packet {send}, parameter {index}"),
            ),
            Notice::System { event: 1, value } => self.running(now, value),
            Notice::System { event: 2, value } => {
                if value & !15 != 0 {
                    self.end(now, "unknown Pause_EX1 flags");
                    return;
                }
                self.pause = Some(value != 0);
                self.lifecycle(now);
            }
            Notice::System {
                event: 3 | 4 | 6, ..
            } => {
                if self.published {
                    self.recoverable_end(
                        now,
                        RecoveryReason::SessionChanged,
                        "aircraft/flight/position changed",
                    );
                } else {
                    self.pending = None;
                    self.latest = None;
                }
            }
            Notice::System { event: 5 | 7, .. } => self.end(now, "aircraft crash/reset"),
            Notice::System { .. } => self.end(now, "unrecognized subscribed event"),
            Notice::Heartbeat { request, running } => {
                if self.heartbeat.is_some_and(|(id, _)| id == request) {
                    if self
                        .heartbeat
                        .is_some_and(|(_, at)| now.saturating_sub(at) >= 2000)
                    {
                        self.recoverable_end(
                            now,
                            RecoveryReason::ConnectionLost,
                            "expired heartbeat response",
                        );
                        return;
                    }
                    self.heartbeat = None;
                    self.last_heartbeat = Some(now);
                    self.running(now, running);
                }
            }
            Notice::Data {
                request,
                object,
                values,
            } => {
                let Some((id, at)) = self.pending else {
                    return;
                };
                if request != id {
                    return;
                } // Old/cancelled replies never refresh data.
                self.pending = None;
                if self.state != Lifecycle::Running {
                    return;
                }
                if now.checked_sub(at).is_none_or(|age| age >= 1000) {
                    self.recoverable_end(
                        now,
                        RecoveryReason::ConnectionLost,
                        "expired data response",
                    );
                    return;
                }
                if self.object.is_some_and(|bound| bound != object) {
                    self.recoverable_end(
                        now,
                        RecoveryReason::SessionChanged,
                        "resolved user-aircraft object changed",
                    );
                    return;
                }
                match normalize(at, values) {
                    Ok(o) => {
                        // Every data request targets USER. Only a matching, fresh,
                        // validated reply can bind its resolved object for this session.
                        self.object = Some(object);
                        self.last_valid = (values[3] == 0.).then_some(at);
                        if let Some((r, sent, Status::Sent)) = self.control_pending {
                            if now.saturating_sub(sent) >= CONFIRM_MS {
                                self.control_pending = Some((r, sent, Status::Timeout));
                            } else if at >= sent
                                && (values[2] - f64::from(r.cents) / 100.).abs()
                                    <= f64::from(READBACK_TOLERANCE)
                            {
                                self.control_pending = Some((r, sent, Status::Confirmed));
                            }
                        }
                        self.latest = Some(o);
                        self.published = true;
                    }
                    Err(e) => self.end(now, e),
                }
            }
        }
    }
    fn send(&mut self, now: u64, heartbeat: bool) {
        let Some(next) = self.next_id.checked_add(1) else {
            self.end(now, "request IDs exhausted");
            return;
        };
        let id = self.next_id;
        self.next_id = next;
        if let Err(e) = self.api.request(id, heartbeat) {
            self.api_error(now, e);
            return;
        }
        if heartbeat {
            self.heartbeat = Some((id, now));
        } else {
            self.pending = Some((id, now));
        }
    }
    fn api_error(&mut self, now: u64, error: io::Error) {
        if matches!(
            error.kind(),
            io::ErrorKind::ConnectionReset | io::ErrorKind::BrokenPipe
        ) {
            self.recoverable_end(now, RecoveryReason::ConnectionLost, &error.to_string());
        } else {
            self.end(now, error);
        }
    }
}
impl<A: Api> SimulatorProvider for Msfs<A> {
    fn recovery_reason(&self) -> Option<RecoveryReason> {
        if self.terminal {
            self.recovery
        } else {
            None
        }
    }
    fn kind(&self) -> Provider {
        Provider::Msfs2020
    }
    fn capabilities(&self) -> Capabilities<'_> {
        if self.control_session.is_some() {
            Capabilities {
                readings: READ_ONLY.readings,
                controls: &[(Control::SetBarometer, 0)],
            }
        } else {
            READ_ONLY
        }
    }
    fn enable_controls(&mut self, session: u64) -> Result<(), ControlError> {
        if session == 0 || self.control_session.is_some() {
            return Err(ControlError::Invalid);
        }
        self.control_session = Some(session);
        Ok(())
    }
    fn control_status(&self) -> Option<Reply> {
        self.control_pending.map(|(r, _, status)| Reply {
            session: r.session,
            id: r.id,
            status,
        })
    }
    fn control(&mut self, r: Request, now: u64) -> Result<Reply, ControlError> {
        if r.validate().is_err()
            || self.control_session != Some(r.session)
            || r.id <= self.control_last_id
        {
            return Err(ControlError::Invalid);
        }
        self.control_last_id = r.id; // Consume even rejected requests; never retry an uncertain write.
                                     // Process queued lifecycle events before a write, without consuming the
                                     // resulting observation that the next telemetry poll must publish.
        let mut drained = false;
        for _ in 0..64 {
            match self.api.next() {
                Ok(None) => {
                    drained = true;
                    break;
                }
                Ok(Some(n)) => self.event(now, n),
                Err(e) => self.api_error(now, e),
            }
            if self.terminal {
                break;
            }
        }
        if !drained && !self.terminal {
            self.end(now, "SDK event queue exceeded control budget");
        }
        if self.terminal
            || self.state != Lifecycle::Running
            || self.object.is_none()
            || self
                .last_valid
                .is_none_or(|at| now.checked_sub(at).is_none_or(|age| age >= 500))
            || self
                .control_pending
                .is_some_and(|(_, _, s)| s == Status::Sent)
        {
            return Err(ControlError::Invalid);
        }
        let pressure16 = baro_event_units(r.cents).ok_or(ControlError::Invalid)?;
        if let Err(error) = self.api.set_baro(pressure16) {
            self.api_error(now, error);
            return Err(ControlError::Invalid);
        }
        // Cancel the pre-write read. Only a subsequently issued request may confirm.
        self.pending = None;
        self.control_pending = Some((r, now, Status::Sent));
        Ok(Reply {
            session: r.session,
            id: r.id,
            status: Status::Sent,
        })
    }
    fn poll(&mut self, now: u64) -> Option<Observation> {
        if self.terminal {
            return self.latest.take();
        }
        if self.last_poll.is_some_and(|last| now < last) {
            self.end(now, "host monotonic clock regressed");
            return self.latest.take();
        }
        self.last_poll = Some(now);
        if let Some((r, at, Status::Sent)) = self.control_pending {
            if now.saturating_sub(at) >= CONFIRM_MS {
                self.control_pending = Some((r, at, Status::Timeout));
            }
        }
        let start = *self.started.get_or_insert(now);
        // Never drain unbounded historical data. One data request and one
        // heartbeat at most; overload is a terminal diagnostic, not stale replay.
        let mut drained = false;
        for _ in 0..64 {
            match self.api.next() {
                Ok(None) => {
                    drained = true;
                    break;
                }
                Ok(Some(event)) => self.event(now, event),
                Err(e) => self.api_error(now, e),
            }
            if self.terminal {
                return self.latest.take();
            }
        }
        if !drained {
            self.end(now, "SDK event queue exceeded poll budget");
        }
        if self
            .heartbeat
            .is_some_and(|(_, at)| now.saturating_sub(at) >= 2000)
        {
            self.recoverable_end(
                now,
                RecoveryReason::ConnectionLost,
                "connection heartbeat timed out",
            );
        }
        if self
            .pending
            .is_some_and(|(_, at)| now.saturating_sub(at) >= 1000)
        {
            self.recoverable_end(
                now,
                RecoveryReason::ConnectionLost,
                "data response timed out",
            );
        }
        if now.saturating_sub(start) >= 5000
            && (!self.opened || self.sim.is_none() || self.pause.is_none())
        {
            self.recoverable_end(
                now,
                RecoveryReason::ConnectionLost,
                "initial lifecycle not received",
            );
        }
        if !self.terminal && self.opened {
            if self.heartbeat.is_none()
                && self
                    .last_heartbeat
                    .is_none_or(|at| now.saturating_sub(at) >= 1000)
            {
                self.send(now, true);
            }
            if !self.terminal && self.state == Lifecycle::Running && self.pending.is_none() {
                self.send(now, false);
            }
        }
        // State-only heartbeat: carries no measurements. Pause remains explicit
        // while the SDK heartbeat is healthy; loss still ends the session.
        if !self.terminal
            && self.state != Lifecycle::Running
            && self
                .last_heartbeat
                .is_some_and(|at| now.saturating_sub(at) < 2000)
            && self
                .last_state_publish
                .is_none_or(|at| now.saturating_sub(at) >= 500)
        {
            self.empty(now, self.state);
        }
        if self.latest.is_some() {
            self.last_state_publish = Some(now);
        }
        self.latest.take()
    }
}
/// SimConnect explicitly converts PRESSURE ALTITUDE from its default metres to
/// requested feet. No local atmospheric correction or assumed unit conversion.
fn normalize(at: u64, v: [f64; 4]) -> io::Result<Observation> {
    if !v.iter().all(|v| v.is_finite()) || ![0., 1., 2.].contains(&v[3]) {
        return Err(io::Error::other("invalid altimeter values/failure enum"));
    }
    let mut measurements = Measurements::new();
    for (quantity, value) in [
        (
            Quantity::IndicatedAltitudeFt,
            if v[3] == 0. {
                Value::Valid(v[0])
            } else {
                Value::Failed
            },
        ),
        (Quantity::PressureAltitudeFt, Value::Valid(v[1])),
        (Quantity::BarometerInHg, Value::Valid(v[2])),
    ] {
        measurements
            .push(Measurement {
                key: Key::primary(quantity),
                value,
                age_ms: 0,
            })
            .map_err(|_| io::Error::other("MSFS reading outside engineering envelope"))?;
    }
    let o = Observation {
        at_ms: at,
        state: Lifecycle::Running,
        measurements,
    };
    crate::to_frame(o, 1, 1, at)
        .map_err(|_| io::Error::other("MSFS reading outside board altimeter profile"))?;
    Ok(o)
}

#[cfg(all(windows, feature = "msfs"))]
impl Api for simconnect_native::Connection {
    fn set_baro(&mut self, pressure16: u32) -> io::Result<()> {
        self.set_baro(pressure16)
    }
    fn next(&mut self) -> io::Result<Option<Notice>> {
        self.poll().map(|v| v.map(native_notice))
    }
    fn request(&mut self, id: u32, heartbeat: bool) -> io::Result<()> {
        self.request(id, heartbeat)
    }
}

#[cfg(all(windows, feature = "msfs"))]
pub fn native_notice(e: simconnect_native::Event) -> Notice {
    match e.kind {
        0 => Notice::Ignore,
        1 => Notice::Open,
        2 => Notice::Exception {
            code: e.code,
            send: e.request,
            index: e.detail,
        },
        3 => Notice::Quit,
        4 => Notice::System {
            event: e.code,
            value: e.detail,
        },
        5 => Notice::Data {
            request: e.request,
            object: e.code,
            values: e.values,
        },
        6 => Notice::Heartbeat {
            request: e.request,
            running: e.code,
        },
        _ => Notice::Exception {
            code: u32::MAX,
            send: 0,
            index: 0,
        },
    }
}

/// Conventional inch of mercury (0 C): 1 inHg = 33.8638866667 hPa.
/// MSFS KOHLSMAN_SET uses integer hPa*16; round once, never truncate.
fn baro_event_units(cents: u16) -> Option<u32> {
    (2810..=3100)
        .contains(&cents)
        .then(|| (f64::from(cents) / 100. * 33.8638866667 * 16.).round() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    #[test]
    fn worker_connection_loss_recovers_but_protocol_errors_do_not() {
        struct Broken(io::ErrorKind);
        impl Api for Broken {
            fn next(&mut self) -> io::Result<Option<Notice>> {
                Err(io::Error::from(self.0))
            }
            fn request(&mut self, _: u32, _: bool) -> io::Result<()> {
                unreachable!()
            }
        }
        for (kind, expected) in [
            (
                io::ErrorKind::ConnectionReset,
                Some(RecoveryReason::ConnectionLost),
            ),
            (
                io::ErrorKind::BrokenPipe,
                Some(RecoveryReason::ConnectionLost),
            ),
            (io::ErrorKind::InvalidData, None),
        ] {
            let mut m = Msfs::new(Broken(kind));
            assert_eq!(m.poll(0).unwrap().state, Lifecycle::Disconnected);
            assert_eq!(m.recovery_reason(), expected);
        }
    }
    #[derive(Default)]
    struct Fake {
        events: VecDeque<Notice>,
        requests: Vec<(u32, bool)>,
        writes: Vec<u32>,
    }
    impl Api for Fake {
        fn set_baro(&mut self, value: u32) -> io::Result<()> {
            self.writes.push(value);
            Ok(())
        }
        fn next(&mut self) -> io::Result<Option<Notice>> {
            Ok(self.events.pop_front())
        }
        fn request(&mut self, id: u32, heartbeat: bool) -> io::Result<()> {
            self.requests.push((id, heartbeat));
            Ok(())
        }
    }
    fn ready() -> Msfs<Fake> {
        let mut m = Msfs::new(Fake::default());
        m.api.events.extend([
            Notice::Open,
            Notice::System { event: 1, value: 1 },
            Notice::System { event: 2, value: 0 },
        ]);
        assert_eq!(m.poll(0).unwrap().state, Lifecycle::Running);
        m
    }
    #[test]
    fn simulator_lifecycle_and_exceptions_cancel_pending_controls() {
        for event in [
            Notice::Quit,
            Notice::Exception {
                code: 1,
                send: 9,
                index: 0,
            },
            Notice::System { event: 3, value: 0 },
            Notice::System { event: 6, value: 0 },
        ] {
            let mut m = ready();
            reply(&mut m, 40);
            m.enable_controls(77).unwrap();
            let request = Request {
                session: 77,
                id: 1,
                channel: 0,
                cents: 2992,
                age_ms: 0,
            };
            m.control(request, 50).unwrap();
            m.api.events.push_back(event);
            assert_eq!(m.poll(60).unwrap().state, Lifecycle::Disconnected);
            assert_eq!(m.control_status().unwrap().status, Status::Rejected);
            assert!(m.control(Request { id: 2, ..request }, 70).is_err());
            assert_eq!(m.api.writes.len(), 1);
        }
        assert_eq!(baro_event_units(2810), Some(15225));
        assert_eq!(baro_event_units(3100), Some(16796));
        assert_eq!(baro_event_units(2810 - 1), None);
    }
    #[test]
    fn barometer_write_is_opt_in_correlated_once_and_confirmed_only_after_write() {
        let mut m = ready();
        reply(&mut m, 40);
        let r = Request {
            session: 77,
            id: 1,
            channel: 0,
            cents: 2992,
            age_ms: 0,
        };
        assert!(m.control(r, 50).is_err());
        assert!(m.api.writes.is_empty());
        m.enable_controls(77).unwrap();
        let pre_write = m.pending.unwrap().0;
        assert_eq!(m.control(r, 50).unwrap().status, Status::Sent);
        assert_eq!(m.api.writes, vec![16211]);
        assert!(m.control(r, 51).is_err());
        assert_eq!(m.api.writes.len(), 1);
        m.api.events.push_back(Notice::Data {
            request: pre_write,
            object: 1,
            values: [12., 12., 29.92, 0.],
        });
        m.poll(60);
        assert_eq!(m.control_status().unwrap().status, Status::Sent);
        let fresh = m.pending.unwrap().0;
        m.api.events.push_back(Notice::Data {
            request: fresh,
            object: 1,
            values: [12., 12., 29.921, 0.],
        });
        m.poll(80);
        assert_eq!(m.control_status().unwrap().status, Status::Confirmed);
        reply(&mut m, 100); // Ordinary cockpit changes are never echoed as writes.
        assert_eq!(m.api.writes.len(), 1);
    }
    #[test]
    fn controls_reject_stale_wrong_session_and_queued_pause_and_never_retry() {
        let mut m = ready();
        reply(&mut m, 40);
        m.enable_controls(77).unwrap();
        let r = Request {
            session: 77,
            id: 1,
            channel: 0,
            cents: 3012,
            age_ms: 0,
        };
        assert!(m.control(Request { session: 78, ..r }, 50).is_err());
        assert!(m.control(Request { age_ms: 501, ..r }, 50).is_err());
        m.api
            .events
            .push_back(Notice::System { event: 2, value: 4 });
        assert!(m.control(r, 50).is_err());
        assert!(m.api.writes.is_empty());
        m.api
            .events
            .push_back(Notice::System { event: 2, value: 0 });
        m.poll(60);
        reply(&mut m, 80);
        assert!(m.control(r, 90).is_err()); // Consumed rejected ID.
        assert_eq!(
            m.control(Request { id: 2, ..r }, 90).unwrap().status,
            Status::Sent
        );
        m.api
            .events
            .push_back(Notice::System { event: 2, value: 1 });
        m.poll(100);
        assert_eq!(m.control_status().unwrap().status, Status::Rejected);
        m.api
            .events
            .push_back(Notice::System { event: 2, value: 0 });
        m.poll(120);
        reply(&mut m, 140);
        assert_eq!(m.api.writes.len(), 1);
        assert!(m.control(Request { id: 3, ..r }, 700).is_err()); // Last read too old.
    }
    #[test]
    fn control_timeout_and_state_heartbeats_do_not_confirm_or_republish_numbers() {
        let mut m = ready();
        reply(&mut m, 40);
        m.enable_controls(77).unwrap();
        m.control(
            Request {
                session: 77,
                id: 1,
                channel: 0,
                cents: 2992,
                age_ms: 0,
            },
            50,
        )
        .unwrap();
        m.poll(2050);
        assert_eq!(m.control_status().unwrap().status, Status::Timeout);
        assert_eq!(m.api.writes.len(), 1);
        let mut m = ready();
        let hb = m.heartbeat.unwrap().0;
        m.api.events.extend([
            Notice::Heartbeat {
                request: hb,
                running: 1,
            },
            Notice::System { event: 2, value: 1 },
        ]);
        assert_eq!(m.poll(20).unwrap().state, Lifecycle::Paused);
        let o = m.poll(520).unwrap();
        assert_eq!(o.state, Lifecycle::Paused);
        assert!(o.measurements.iter().next().is_none());
        m.poll(1020);
        // No SDK heartbeat reply: eventually disconnect instead of claiming pause forever.
        assert_eq!(m.poll(3020).unwrap().state, Lifecycle::Disconnected);
    }
    fn reply(m: &mut Msfs<Fake>, now: u64) -> Observation {
        let id = m.pending.unwrap().0;
        m.api.events.push_back(Notice::Data {
            request: id,
            object: 1,
            values: [8500., 8300., 30.12, 0.],
        });
        m.poll(now).unwrap()
    }
    #[test]
    fn resolved_object_binds_only_to_fresh_matching_valid_response() {
        let mut m = ready();
        let request = m.pending.unwrap().0;
        m.api.events.push_back(Notice::Data {
            request: request + 100,
            object: 99,
            values: [12., 12., 29.92, 0.],
        });
        assert!(m.poll(10).is_none());
        assert_eq!(m.object, None);
        assert_eq!(reply(&mut m, 20).state, Lifecycle::Running);
        assert_eq!(m.object, Some(1));
        assert_eq!(reply(&mut m, 40).state, Lifecycle::Running);
        let request = m.pending.unwrap().0;
        m.api.events.push_back(Notice::Data {
            request,
            object: 7,
            values: [9999., 9999., 29.92, 0.],
        });
        let o = m.poll(60).unwrap();
        assert_eq!(o.state, Lifecycle::Disconnected);
        assert!(o.measurements.iter().next().is_none());
        assert!(m.poll(80).is_none());

        // A new session can resolve to a different object; never hardcode 1.
        let mut fresh = ready();
        let request = fresh.pending.unwrap().0;
        fresh.api.events.push_back(Notice::Data {
            request,
            object: 7,
            values: [12., 12., 29.92, 0.],
        });
        assert_eq!(fresh.poll(20).unwrap().state, Lifecycle::Running);
        assert_eq!(fresh.object, Some(7));

        let mut expired = ready();
        assert_eq!(reply(&mut expired, 1000).state, Lifecycle::Disconnected);
        assert_eq!(expired.object, None);
        let mut invalid = ready();
        let request = invalid.pending.unwrap().0;
        invalid.api.events.push_back(Notice::Data {
            request,
            object: 1,
            values: [f64::NAN, 12., 29.92, 0.],
        });
        assert_eq!(invalid.poll(20).unwrap().state, Lifecycle::Disconnected);
        assert_eq!(invalid.object, None);
    }
    #[test]
    fn coherent_indication_age_and_failure_flags_are_preserved() {
        let mut m = ready();
        let o = reply(&mut m, 40);
        let f = crate::to_frame(o, 1, 1, 40).unwrap();
        assert_eq!(f.age_ms, 40); // Request time, not later queue-drain time.
        assert_eq!(f.indicated_ft, simulator_link::Datum::Value(8500.));
        assert_eq!(f.pressure_ft, simulator_link::Datum::Value(8300.));
        for failure in [1., 2.] {
            let f = crate::to_frame(
                normalize(0, [8500., 8300., 30.12, failure]).unwrap(),
                1,
                1,
                0,
            )
            .unwrap();
            assert_eq!(f.indicated_ft, simulator_link::Datum::Failed);
        }
        for v in [
            [f64::NAN, 0., 29.92, 0.],
            [0., 0., 32., 0.],
            [0., 0., 29.92, 3.],
        ] {
            assert!(normalize(0, v).is_err());
        }
    }
    #[test]
    fn pause_cancels_pending_and_resume_needs_new_matching_reply() {
        let mut m = ready();
        let old = m.pending.unwrap().0;
        m.api.events.extend([
            Notice::System { event: 2, value: 4 },
            Notice::Data {
                request: old,
                object: 1,
                values: [8500., 8300., 30.12, 0.],
            },
        ]);
        assert_eq!(m.poll(20).unwrap().state, Lifecycle::Paused);
        assert_eq!(m.object, None);
        m.api
            .events
            .push_back(Notice::System { event: 2, value: 0 });
        assert!(m.poll(40).unwrap().measurements.iter().next().is_none());
        m.api.events.push_back(Notice::Data {
            request: old,
            object: 1,
            values: [9999., 8300., 30.12, 0.],
        });
        assert!(m.poll(50).is_none());
        assert_eq!(m.object, None);
        assert_eq!(reply(&mut m, 80).at_ms, 40);
    }
    #[test]
    fn transition_in_same_batch_overrides_numeric_data() {
        let mut m = ready();
        let id = m.pending.unwrap().0;
        m.api.events.extend([
            Notice::Data {
                request: id,
                object: 1,
                values: [8500., 8300., 30.12, 0.],
            },
            Notice::System { event: 3, value: 0 },
        ]);
        assert_eq!(m.poll(40).unwrap().state, Lifecycle::Disconnected);
        assert_eq!(m.recovery_reason(), Some(RecoveryReason::SessionChanged));
        assert!(m.poll(60).is_none());
    }
    #[test]
    fn recovery_hints_distinguish_lifecycle_changes_from_invalid_data() {
        for event in [3, 4, 6] {
            let mut m = ready();
            reply(&mut m, 40);
            assert_eq!(m.recovery_reason(), None);
            m.api.events.push_back(Notice::System { event, value: 0 });
            assert_eq!(m.poll(60).unwrap().state, Lifecycle::Disconnected);
            assert_eq!(m.recovery_reason(), Some(RecoveryReason::SessionChanged));
        }
        let mut m = ready();
        reply(&mut m, 40);
        let id = m.pending.unwrap().0;
        m.api.events.push_back(Notice::Data {
            request: id,
            object: 2,
            values: [8000., 7900., 29.92, 0.],
        });
        assert_eq!(m.poll(60).unwrap().state, Lifecycle::Disconnected);
        assert_eq!(m.recovery_reason(), Some(RecoveryReason::SessionChanged));
        for event in [
            Notice::Exception {
                code: 1,
                send: 0,
                index: 0,
            },
            Notice::System { event: 5, value: 0 },
            Notice::System {
                event: 2,
                value: 0xffff,
            },
        ] {
            let mut m = ready();
            reply(&mut m, 40);
            m.api.events.push_back(event);
            m.poll(60);
            assert_eq!(m.recovery_reason(), None);
        }
        let mut m = ready();
        let id = m.pending.unwrap().0;
        m.api.events.push_back(Notice::Data {
            request: id,
            object: 1,
            values: [f64::NAN, 0., 29.92, 0.],
        });
        m.poll(40);
        assert_eq!(m.recovery_reason(), None);
        let mut m = ready();
        m.poll(2000);
        assert_eq!(m.recovery_reason(), Some(RecoveryReason::ConnectionLost));
    }
    #[test]
    fn delayed_data_heartbeat_loss_and_clock_regression_end_session() {
        let mut m = ready();
        assert_eq!(reply(&mut m, 1000).state, Lifecycle::Disconnected);
        let mut m = ready();
        m.api
            .events
            .push_back(Notice::System { event: 2, value: 1 });
        m.poll(10);
        assert_eq!(m.poll(2000).unwrap().state, Lifecycle::Disconnected);
        let mut m = ready();
        m.poll(10);
        assert_eq!(m.poll(9).unwrap().state, Lifecycle::Disconnected);
        let mut m = ready();
        m.api.events.extend([Notice::Ignore; 64]);
        assert_eq!(m.poll(20).unwrap().state, Lifecycle::Disconnected);
    }
}
