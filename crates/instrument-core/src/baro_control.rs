use crate::{App, ApplyError, Quality};
use simulator_link::{
    control::{Reply, Request, Status, CONFIRM_MS, READBACK_TOLERANCE, REQUEST_MAX_AGE_MS},
    Lifecycle, Provider,
};
#[derive(Clone, Copy, Debug)]
pub struct Pending {
    request: Request,
    at: u64,
    sequence: u64,
    status: Status,
    pub(crate) claimed: bool,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct BaroControl {
    pub enabled: bool,
    pub(crate) next_id: u64,
    pub(crate) pending: Option<Pending>,
}
impl App {
    pub(crate) fn claim_control(&mut self, session: u64, id: u64) -> Result<(), ApplyError> {
        let r = self.control_request().ok_or(ApplyError::NotReady)?;
        if !self.external.is_some_and(|e| e.multi_profile) || r.session != session || r.id != id {
            return Err(ApplyError::NotReady);
        }
        let pending = self.control.pending.as_mut().ok_or(ApplyError::NotReady)?;
        if pending.claimed {
            return Err(ApplyError::NotReady);
        }
        pending.claimed = true;
        Ok(())
    }
    pub(crate) fn control_dispatch_in_progress(&self) -> bool {
        self.control.pending.is_some_and(|p| p.claimed)
            && self.control_status() == Some(Status::Sent)
    }
    pub(crate) fn refresh_control(&mut self) {
        if let (Some(mut p), Some(status)) = (self.control.pending, self.control_status()) {
            p.status = status;
            self.control.pending = Some(p);
        }
    }
    pub(crate) fn control_data_ready(&self) -> bool {
        self.selected == crate::Instrument::Altimeter
            && self
                .external
                .is_some_and(|e| e.state() == Lifecycle::Running)
            && self.altimeter_reading(self.now_ms()).quality == Quality::Valid
            && self
                .external_baro(self.now_ms())
                .is_some_and(|r| r.quality == Quality::Valid)
    }
    pub fn control_status(&self) -> Option<Status> {
        let p = self.control.pending?;
        if p.status != Status::Sent {
            return Some(p.status);
        }
        if !self.control.enabled || !self.control_data_ready() {
            return Some(Status::Rejected);
        }
        if self
            .now_ms()
            .checked_sub(p.at)
            .is_none_or(|a| a >= CONFIRM_MS)
        {
            Some(Status::Timeout)
        } else {
            Some(Status::Sent)
        }
    }
    pub fn baro_edit_allowed(&self) -> bool {
        if self.external.is_none() {
            return self.altimeter_reading(self.now_ms()).quality == Quality::Valid;
        }
        self.control.enabled
            && self.control_data_ready()
            && self.control_status() != Some(Status::Sent)
    }
    pub fn control_enabled(&self) -> bool {
        self.control.enabled
    }
    pub fn control_request(&self) -> Option<Request> {
        if self.control_status() != Some(Status::Sent) {
            return None;
        }
        let p = self.control.pending?;
        let age = self.now_ms().checked_sub(p.at)?;
        if age > u64::from(REQUEST_MAX_AGE_MS) {
            return None;
        }
        Some(Request {
            age_ms: age as u32,
            ..p.request
        })
    }
    pub(crate) fn enable_control(&mut self, session: u64) -> Result<(), ApplyError> {
        if !self.external.is_some_and(|e| {
            e.session == session
                && e.provider == Provider::Msfs2020
                && e.state() != Lifecycle::Disconnected
        }) {
            return Err(ApplyError::Unsupported);
        }
        self.control.enabled = true;
        Ok(())
    }
    pub(crate) fn request_baro(&mut self, value: f32) -> Result<(), ApplyError> {
        if !self.control.enabled {
            return Err(ApplyError::Unsupported);
        }
        if !self.baro_edit_allowed() {
            return Err(ApplyError::NotReady);
        }
        if !value.is_finite() || !(28.1..=31.).contains(&value) {
            return Err(ApplyError::OutOfRange);
        }
        let cents = (value * 100. + 0.5) as u16;
        let e = self.external.ok_or(ApplyError::NotReady)?;
        let id = self
            .control
            .next_id
            .checked_add(1)
            .ok_or(ApplyError::NotReady)?;
        self.control.next_id = id;
        self.control.pending = Some(Pending {
            request: Request {
                session: e.session,
                id,
                channel: 0,
                cents,
                age_ms: 0,
            },
            at: self.now_ms(),
            sequence: e.sequence(),
            status: Status::Sent,
            claimed: false,
        });
        Ok(())
    }
    pub(crate) fn control_reply(&mut self, r: Reply) -> Result<(), ApplyError> {
        let p = self.control.pending.ok_or(ApplyError::NotReady)?;
        if r.session != p.request.session || r.id != p.request.id {
            return Err(ApplyError::NotReady);
        }
        // A terminal decision cannot be undone by a delayed result.
        if self.control_status() != Some(Status::Sent) {
            return Err(ApplyError::NotReady);
        }
        if r.status == Status::Confirmed
            && (self.external.is_none_or(|e| e.sequence() <= p.sequence)
                || !self
                    .external_baro(self.now_ms())
                    .and_then(|r| r.value)
                    .is_some_and(|v| {
                        (v - p.request.cents as f32 / 100.).abs() <= READBACK_TOLERANCE
                    }))
        {
            return Err(ApplyError::NotReady);
        }
        self.control.pending = Some(Pending {
            status: r.status,
            ..p
        });
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use simulator_link::{Datum, Frame};
    fn feed(a: &mut App, at: u64, sequence: u64, baro: f32, state: Lifecycle) {
        let start = a.now_ms();
        for t in (start..=at).step_by(20) {
            a.tick(t);
        }
        a.tick(at);
        a.apply(Command::Telemetry(Frame {
            session: 10,
            sequence,
            receiver_ms: at,
            age_ms: 0,
            state,
            indicated_ft: Datum::Value(12.),
            pressure_ft: Datum::Value(12.),
            baro_inhg: Datum::Value(baro),
        }))
        .unwrap();
    }
    fn ready() -> App {
        let mut a = App::new();
        a.tick(0);
        a.apply(Command::SourceExternal(Provider::Msfs2020, 10))
            .unwrap();
        feed(&mut a, 20, 1, 30.12, Lifecycle::Running);
        a
    }
    #[test]
    fn edits_are_requests_never_local_corrections_and_require_matching_readback() {
        let mut a = ready();
        assert!(!a.baro_edit_allowed());
        assert!(a.apply(Command::Baro(29.92)).is_err());
        a.apply(Command::ControlEnable(10)).unwrap();
        a.apply(Command::Baro(29.92)).unwrap();
        let req = a.control_request().unwrap();
        assert_eq!(req.cents, 2992);
        assert_eq!(a.baro_inhg(), 30.12);
        assert_eq!(a.altimeter_reading(20).value, Some(12.));
        assert!(!a.baro_edit_allowed());
        assert!(a.apply(Command::Baro(30.01)).is_err());
        let result = Reply {
            session: 10,
            id: req.id,
            status: Status::Confirmed,
        };
        assert!(a.apply(Command::ControlReply(result)).is_err());
        feed(&mut a, 40, 2, 29.921, Lifecycle::Running);
        assert!(a
            .apply(Command::ControlReply(Reply { id: 9, ..result }))
            .is_err());
        a.apply(Command::ControlReply(result)).unwrap();
        assert_eq!(a.control_status(), Some(Status::Confirmed));
        assert!(a.baro_edit_allowed());
        assert_eq!(a.baro_inhg(), 29.921);
    }
    #[test]
    fn pause_timeout_and_new_sessions_cannot_revive_or_replay_an_edit() {
        let mut a = ready();
        a.apply(Command::ControlEnable(10)).unwrap();
        a.apply(Command::Baro(29.92)).unwrap();
        feed(&mut a, 40, 2, 30.12, Lifecycle::Paused);
        feed(&mut a, 60, 3, 30.12, Lifecycle::Running);
        assert_eq!(a.control_status(), Some(Status::Rejected));
        assert!(a.control_request().is_none());
        a.apply(Command::Baro(29.92)).unwrap();
        for i in 4..=105 {
            feed(&mut a, (i - 1) * 20, i, 30.12, Lifecycle::Running);
        }
        assert_eq!(a.control_status(), Some(Status::Timeout));
        assert!(a.control_request().is_none());
        a.apply(Command::SourceExternal(Provider::Msfs2020, 11))
            .unwrap();
        assert!(!a.control_enabled());
        assert!(a.control_request().is_none());
        assert!(a.apply(Command::ControlEnable(10)).is_err());
    }
    #[test]
    fn hidden_altimeter_cannot_write_and_switching_back_cannot_replay_an_edit() {
        for hidden in [crate::Instrument::Attitude, crate::Instrument::Turn] {
            let mut a = ready();
            a.apply(Command::ControlEnable(10)).unwrap();
            a.apply(Command::Baro(29.92)).unwrap();
            let old = a.control_request().unwrap();
            a.apply(Command::Select(hidden)).unwrap();
            assert!(!a.baro_edit_allowed());
            assert!(a.control_request().is_none());
            assert!(a.apply(Command::Baro(30.01)).is_err());
            // Hidden telemetry still follows the simulator without a local edit.
            feed(&mut a, 40, 2, 30.20, Lifecycle::Running);
            assert_eq!(a.baro_inhg(), 30.20);
            a.apply(Command::Select(crate::Instrument::Altimeter))
                .unwrap();
            assert!(a.control_request().is_none());
            assert!(a
                .apply(Command::ControlReply(Reply {
                    session: old.session,
                    id: old.id,
                    status: Status::Confirmed,
                }))
                .is_err());
            a.apply(Command::Baro(29.92)).unwrap();
            assert!(a.control_request().unwrap().id > old.id);
        }
    }
}
