//! Single-device session owner. No OS, serial, SDK or UI calls live here.
//! Providers keep their full quantity model; only this explicit device profile
//! projects it onto the current altimeter wire protocol. Errors end the session.
#![forbid(unsafe_code)]
use sim_bridge::{to_frame, Control, Observation, RecoveryReason, SimulatorProvider};
use simulator_link::{
    control::{Reply, Request, Status, REQUEST_MAX_AGE_MS},
    measurements::{Key, Measurements, Quantity},
    Lifecycle, Provider,
};
use std::{io, time::Instant};

pub trait DeviceLink {
    /// One bounded transaction; never retry an uncertain command. After a
    /// transport error, implementations must refuse further transactions.
    fn request(&mut self, command: &str) -> io::Result<String>;
    /// Suppress cleanup traffic after an identity/protocol fault as well as I/O errors.
    fn abandon(&mut self) {}
}
impl DeviceLink for Box<dyn DeviceLink> {
    fn request(&mut self, command: &str) -> io::Result<String> {
        self.as_mut().request(command)
    }
    fn abandon(&mut self) {
        self.as_mut().abandon();
    }
}
pub trait Clock {
    fn now_ms(&self) -> u64;
}
pub struct MonotonicClock(Instant);
impl Default for MonotonicClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}
impl Clock for MonotonicClock {
    fn now_ms(&self) -> u64 {
        self.0.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Active,
    Disconnected,
    Faulted,
}
#[derive(Default, Debug)]
pub struct Tick {
    pub telemetry: Option<(String, String)>,
    /// At most one request and one terminal result per tick.
    pub controls: Vec<(Option<String>, String)>,
}
#[derive(Default, Clone, Copy, Debug)]
pub struct Statistics {
    pub frames: u64,
    pub max_ack_ms: u64,
    pub max_sample_gap_ms: u64,
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
pub fn attribute<'a>(line: &'a str, key: &str) -> io::Result<&'a str> {
    let mut values = line.split_ascii_whitespace().filter_map(|word| {
        word.split_once('=')
            .and_then(|(k, v)| (k == key).then_some(v))
    });
    let value = values
        .next()
        .ok_or_else(|| invalid(format!("missing {key}")))?;
    if values.next().is_some() {
        return Err(invalid(format!("duplicate {key}")));
    }
    Ok(value)
}
fn number(line: &str, key: &str) -> io::Result<u64> {
    let v = attribute(line, key)?;
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid(format!("invalid {key}")));
    }
    v.parse().map_err(|_| invalid(format!("invalid {key}")))
}
fn prefix(line: &str, expected: &str) -> io::Result<()> {
    if !line.starts_with(expected) {
        return Err(invalid(format!("expected {expected:?}, got {line:?}")));
    }
    Ok(())
}

pub struct Session<D: DeviceLink, C: Clock> {
    provider: Box<dyn SimulatorProvider>,
    device: D,
    clock: C,
    session: u64,
    sequence: u64,
    receiver_ms: u64,
    last_ack: u64,
    last_frame: u64,
    last_clock: u64,
    writeback: bool,
    seen_control: u64,
    active_control: Option<(u64, u64)>, // request ID and pre-write telemetry sequence
    state: State,
    stats: Statistics,
}
/// Owns a healthy, already-invalidated device while the old provider is closed.
/// Recovery preserves the user's direction setting, not pending operations or
/// cached measurements. MSFS observations remain authoritative in either mode.
pub struct RecoveryHandoff<D: DeviceLink> {
    device: D,
    next_session: u64,
    provider_kind: Provider,
    writeback: bool,
}
impl<D: DeviceLink> RecoveryHandoff<D> {
    pub fn session_id(&self) -> u64 {
        self.next_session
    }
    pub fn writeback_enabled(&self) -> bool {
        self.writeback
    }
    pub fn resume<C: Clock>(
        self,
        provider: Box<dyn SimulatorProvider>,
        clock: C,
    ) -> io::Result<Session<D, C>> {
        if provider.kind() != self.provider_kind {
            return Err(invalid("recovery provider changed"));
        }
        Session::open(
            provider,
            self.device,
            clock,
            self.next_session,
            self.writeback,
        )
    }
}

/// Bounded exponential delay for connection attempts, not simulator writes.
#[derive(Default)]
pub struct RetryDelay {
    failures: u8,
}
impl RetryDelay {
    pub fn next_ms(&mut self) -> u64 {
        let delay = 1000u64 << self.failures.min(3);
        self.failures = self.failures.saturating_add(1);
        delay
    }
}
impl<D: DeviceLink, C: Clock> Session<D, C> {
    pub fn open(
        mut provider: Box<dyn SimulatorProvider>,
        mut device: D,
        clock: C,
        session: u64,
        writeback: bool,
    ) -> io::Result<Self> {
        if session == 0 {
            return Err(invalid("zero session"));
        }
        let capabilities = provider.capabilities();
        for q in [
            Quantity::IndicatedAltitudeFt,
            Quantity::PressureAltitudeFt,
            Quantity::BarometerInHg,
        ] {
            if !capabilities.readings.contains(&Key::primary(q)) {
                return Err(invalid("provider does not support altimeter-v1"));
            }
        }
        if writeback {
            if provider.kind() != Provider::Msfs2020 {
                return Err(invalid("writeback unsupported for this provider/profile"));
            }
            provider
                .enable_controls(session)
                .map_err(|_| invalid("provider refused controls"))?;
            if !provider
                .capabilities()
                .controls
                .contains(&(Control::SetBarometer, 0))
            {
                return Err(invalid("provider lacks enabled barometer capability"));
            }
        }
        let handshake = (|| -> io::Result<(u64, u64)> {
            let response = device.request(&format!(
                "source external {} {session}",
                provider.kind().name()
            ))?;
            prefix(&response, &format!("OK source={} ", provider.kind().name()))?;
            if number(&response, "session")? != session || number(&response, "sequence")? != 0 {
                return Err(invalid("device did not establish new session"));
            }
            let receiver_ms = number(&response, "now_ms")?;
            let ack_at = clock.now_ms();
            if writeback {
                let response = device.request(&format!("control enable {session}"))?;
                prefix(&response, "OK control enabled=true")?;
            }
            Ok((receiver_ms, ack_at))
        })();
        let (receiver_ms, ack_at) = match handshake {
            Ok(value) => value,
            Err(error) => {
                device.abandon();
                return Err(error);
            }
        };
        let now = clock.now_ms();
        Ok(Self {
            provider,
            device,
            clock,
            session,
            sequence: 0,
            receiver_ms,
            last_ack: ack_at,
            last_frame: now,
            last_clock: now,
            writeback,
            seen_control: 0,
            active_control: None,
            state: State::Active,
            stats: Statistics::default(),
        })
    }
    pub fn state(&self) -> State {
        self.state
    }
    pub fn statistics(&self) -> Statistics {
        self.stats
    }
    pub fn recovery_reason(&self) -> Option<RecoveryReason> {
        if self.state == State::Disconnected {
            self.provider.recovery_reason()
        } else {
            None
        }
    }
    pub fn into_recovery(self) -> io::Result<RecoveryHandoff<D>> {
        if self.recovery_reason().is_none() {
            return Err(invalid("session is not safely recoverable"));
        }
        let next_session = self
            .session
            .checked_add(1)
            .ok_or_else(|| invalid("session IDs exhausted"))?;
        let provider_kind = self.provider.kind();
        drop(self.provider); // release SDK handle before the next connection attempt
        Ok(RecoveryHandoff {
            device: self.device,
            next_session,
            provider_kind,
            writeback: self.writeback,
        })
    }
    fn now(&mut self) -> io::Result<u64> {
        let now = self.clock.now_ms();
        if now < self.last_clock {
            return Err(invalid("host clock moved backwards"));
        }
        self.last_clock = now;
        Ok(now)
    }
    fn check_active(&self) -> io::Result<()> {
        if self.state != State::Active {
            return Err(invalid("session ended; construct a fresh session"));
        }
        Ok(())
    }
    fn guard<T>(&mut self, result: io::Result<T>) -> io::Result<T> {
        if result.is_err() {
            self.state = State::Faulted;
            self.active_control = None;
            self.device.abandon();
        }
        result
    }
    fn refresh(&mut self) -> io::Result<String> {
        let r = self.device.request("source status")?;
        prefix(&r, &format!("OK source={} ", self.provider.kind().name()))?;
        if number(&r, "session")? != self.session || number(&r, "sequence")? != self.sequence {
            return Err(invalid("source identity/sequence changed"));
        }
        let now = number(&r, "now_ms")?;
        if now < self.receiver_ms {
            return Err(invalid("device clock moved backwards"));
        }
        self.receiver_ms = now;
        self.last_ack = self.now()?;
        Ok(r)
    }
    pub fn source_status(&mut self) -> io::Result<String> {
        self.check_active()?;
        let result = self.refresh();
        self.guard(result)
    }
    pub fn display_status(&mut self, sample: bool) -> io::Result<String> {
        self.check_active()?;
        let result = (|| {
            let r = self.device.request(if sample {
                "display sample"
            } else {
                "display status"
            })?;
            if sample && r == "ERR display_sample_busy" {
                return Ok(r);
            }
            prefix(
                &r,
                if sample {
                    "OK display_sample "
                } else {
                    "OK display "
                },
            )?;
            Ok(r)
        })();
        self.guard(result)
    }
    pub fn poll(&mut self) -> io::Result<Tick> {
        self.check_active()?;
        let result = self.poll_inner();
        self.guard(result)
    }
    fn send(&mut self, observation: Observation, tick: &mut Tick) -> io::Result<u64> {
        if self.now()?.saturating_sub(self.last_ack) >= 100 {
            self.refresh()?;
        }
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| invalid("sequence exhausted"))?;
        let now = self.now()?;
        let mut frame = to_frame(observation, self.session, sequence, now)
            .map_err(|_| invalid("observation outside altimeter-v1 profile"))?;
        frame.receiver_ms = self.receiver_ms;
        // Include the sub-millisecond uncertainty of the host's integer clock.
        frame.age_ms = frame.age_ms.saturating_add(1);
        let packet = frame.encode().map_err(|_| invalid("invalid frame"))?;
        let ack = self.device.request(packet.as_str())?;
        prefix(&ack, "OK telemetry ")?;
        if number(&ack, "session")? != self.session || number(&ack, "seq")? != sequence {
            return Err(invalid("telemetry acknowledgement identity mismatch"));
        }
        let receiver_ms = number(&ack, "now_ms")?;
        if receiver_ms < self.receiver_ms {
            return Err(invalid("device clock moved backwards"));
        }
        let control = number(&ack, "control")?;
        self.receiver_ms = receiver_ms;
        self.last_ack = self.now()?;
        self.sequence = sequence;
        self.stats.frames += 1;
        self.stats.max_ack_ms = self.stats.max_ack_ms.max(self.last_ack - now);
        self.stats.max_sample_gap_ms = self
            .stats
            .max_sample_gap_ms
            .max(self.last_ack - self.last_frame);
        self.last_frame = self.last_ack;
        tick.telemetry = Some((packet.as_str().into(), ack));
        if observation.state == Lifecycle::Disconnected {
            self.active_control = None;
            self.state = State::Disconnected;
        }
        Ok(control)
    }
    fn result(&mut self, reply: Reply, expected: u64, tick: &mut Tick) -> io::Result<()> {
        if reply.session != self.session || reply.id != expected {
            return Err(invalid("control result identity mismatch"));
        }
        let line = reply
            .encode()
            .map_err(|_| invalid("invalid control result"))?;
        if reply.status != Status::Sent {
            let r = self.device.request(line.as_str())?;
            if r != "ERR not_ready" && !r.starts_with("ERR not_ready ") {
                prefix(&r, "OK control result")?;
            }
            tick.controls.push((None, line.as_str().into()));
            self.active_control = None;
        }
        Ok(())
    }
    fn poll_inner(&mut self) -> io::Result<Tick> {
        let mut tick = Tick::default();
        let now = self.now()?;
        if let Some(observation) = self.provider.poll(now) {
            let advertised = self.send(observation, &mut tick)?;
            if self.state == State::Disconnected {
                return Ok(tick);
            }
            if self.writeback && advertised > self.seen_control {
                if self.active_control.is_some() {
                    return Err(invalid("overlapping device controls"));
                }
                let sent_at = self.now()?;
                let response = self.device.request("control poll")?;
                if let Some(line) = response.strip_prefix("OK control C1 ") {
                    let packet = format!("C1 {line}");
                    let mut request =
                        Request::decode(&packet).map_err(|_| invalid("invalid control request"))?;
                    if request.session != self.session || request.id != advertised {
                        return Err(invalid("control request identity mismatch"));
                    }
                    self.seen_control = advertised; // consumed even on rejection; never replay
                    let now = self.now()?;
                    request.age_ms = request.age_ms.saturating_add(
                        (now - sent_at).saturating_add(1).min(u32::MAX as u64) as u32,
                    );
                    let rejected = Reply {
                        session: self.session,
                        id: advertised,
                        status: Status::Rejected,
                    };
                    let reply = if request.age_ms > REQUEST_MAX_AGE_MS {
                        rejected
                    } else {
                        self.provider.control(request, now).unwrap_or(rejected)
                    };
                    if reply.session != self.session || reply.id != advertised {
                        return Err(invalid("provider control identity mismatch"));
                    }
                    // A newly accepted request must be sent or rejected. Confirmation
                    // is delivered only after a subsequent telemetry publication.
                    if !matches!(reply.status, Status::Sent | Status::Rejected) {
                        return Err(invalid("premature provider confirmation"));
                    }
                    let encoded = reply.encode().map_err(|_| invalid("invalid reply"))?;
                    tick.controls.push((Some(packet), encoded.as_str().into()));
                    if reply.status == Status::Sent {
                        self.active_control = Some((advertised, self.sequence));
                    } else {
                        self.result(reply, advertised, &mut tick)?;
                    }
                } else {
                    prefix(&response, "OK control none ")?;
                }
            }
        }
        if let Some((id, sequence_floor)) = self.active_control {
            let reply = self
                .provider
                .control_status()
                .ok_or_else(|| invalid("provider lost pending control"))?;
            if reply.session != self.session || reply.id != id {
                return Err(invalid("pending control identity mismatch"));
            }
            // Never confirm using the pre-write observation from this same tick.
            if reply.status != Status::Confirmed
                || (tick.telemetry.is_some() && self.sequence > sequence_floor)
            {
                self.result(reply, id, &mut tick)?;
            }
        }
        Ok(tick)
    }
    /// Clean user stop. Never used after transport/identity failure, where a late
    /// reply could be mistaken for another request. Firmware expiry is independent.
    pub fn stop(&mut self) -> io::Result<Tick> {
        self.check_active()?;
        let result = (|| {
            let mut tick = Tick::default();
            let observation = Observation {
                at_ms: self.now()?,
                state: Lifecycle::Disconnected,
                measurements: Measurements::new(),
            };
            self.send(observation, &mut tick)?;
            Ok(tick)
        })();
        self.guard(result)
    }
}

#[cfg(test)]
mod tests;
