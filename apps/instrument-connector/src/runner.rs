//! Application supervisor. Recovery reuses the healthy device handle but never
//! the old SDK connection, session identity, pending writes or cached values.
//! The user's configured communication direction persists across switches.
use crate::logging::Log;
use connector_runtime::{DeviceLink, MonotonicClock, RetryDelay, Session, State, Statistics, Tick};
use serde_json::{json, Value};
use sim_bridge::SimulatorProvider;
use simulator_link::{Datum, Frame, Lifecycle};
use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub struct Options {
    pub writeback: bool,
    pub diagnostics: bool,
    pub frame_timing: bool,
    pub hardware: bool,
    pub duration: Option<u64>,
}
fn stopped(stop: &AtomicBool, start: Instant, duration: Option<u64>) -> bool {
    stop.load(Ordering::Acquire)
        || duration.is_some_and(|s| start.elapsed() >= Duration::from_secs(s))
}
fn wait(
    delay: u64,
    stop: &AtomicBool,
    start: Instant,
    duration: Option<u64>,
    log: &Log,
) -> io::Result<bool> {
    let deadline = Instant::now() + Duration::from_millis(delay);
    while Instant::now() < deadline {
        if stopped(stop, start, duration) {
            return Ok(false);
        }
        log.check()?;
        std::thread::sleep(
            Duration::from_millis(25).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
    Ok(!stopped(stop, start, duration))
}
fn accumulate(total: &mut Statistics, part: Statistics) {
    total.frames = total.frames.saturating_add(part.frames);
    total.max_ack_ms = total.max_ack_ms.max(part.max_ack_ms);
    total.max_sample_gap_ms = total.max_sample_gap_ms.max(part.max_sample_gap_ms);
}
pub fn run(
    provider: Box<dyn SimulatorProvider>,
    device: Box<dyn DeviceLink>,
    initial_id: u64,
    options: Options,
    stop: &AtomicBool,
    log: &Log,
    factory: &mut dyn FnMut() -> io::Result<Box<dyn SimulatorProvider>>,
) -> io::Result<Value> {
    let kind = provider.kind().name();
    let mut session = Some(Session::open(
        provider,
        device,
        MonotonicClock::default(),
        initial_id,
        options.writeback,
    )?);
    let start = Instant::now();
    let mut diagnostic = start;
    let mut total = Statistics::default();
    let mut sessions = 1u64;
    let mut attempts = 0u64;
    let mut session_id = initial_id;
    let mut probe_due = None;
    let mut numeric_seen = false;
    println!("Connected: source={kind} session={session_id} profile=altimeter-v1 writeback={}; Ctrl+C to stop.",options.writeback);
    let outcome = (|| -> io::Result<&str> {
        log.record(json!({"start":{"session":session_id,"source":kind,"profile":"altimeter-v1","writeback":options.writeback}}))?;
        loop {
            let at = Instant::now();
            let active = session
                .as_mut()
                .ok_or_else(|| io::Error::other("missing active session"))?;
            if stopped(stop, start, options.duration) {
                record_tick(log, active.stop()?, start)?;
                return Ok("stopped");
            }
            let tick = active.poll()?;
            if !numeric_seen {
                if let Some((wire, _)) = &tick.telemetry {
                    let frame = Frame::decode(wire).map_err(|error| {
                        io::Error::other(format!("encoded telemetry: {error:?}"))
                    })?;
                    if frame.state == Lifecycle::Running
                        && matches!(frame.indicated_ft, Datum::Value(_))
                        && matches!(frame.baro_inhg, Datum::Value(_))
                    {
                        numeric_seen = true;
                        // Allow several presentations, without sleeping or stopping
                        // telemetry. One readback per session, not steady polling.
                        probe_due = Some(Instant::now() + Duration::from_millis(250));
                        log.record(json!({"telemetry_ready":{"session":session_id,
                            "sequence":frame.sequence,"elapsed_ms":start.elapsed().as_millis(),
                            "frame":wire}}))?;
                    }
                }
            }
            record_tick(log, tick, start)?;
            if active.state() == State::Disconnected {
                let Some(reason) = active.recovery_reason() else {
                    if kind == "msfs2020" {
                        return Err(io::Error::other("MSFS session ended with a non-recoverable fault; see SDK diagnostic above"));
                    }
                    return Ok("source_disconnected");
                };
                log.record(
                    json!({"session_end":{"session":session_id,"reason":format!("{reason:?}"),
                    "frames":active.statistics().frames,"elapsed_ms":start.elapsed().as_millis()}}),
                )?;
                accumulate(&mut total, active.statistics());
                let handoff = session.take().unwrap().into_recovery()?;
                let next_id = handoff.session_id();
                let writeback = handoff.writeback_enabled();
                println!("Source changed/unavailable ({reason:?}). Automatically reconnecting session {next_id}; new simulator readings will replace gauge values.");
                let mut retry = RetryDelay::default();
                loop {
                    let delay = retry.next_ms();
                    log.record(json!({"recovery_wait":{"previous_session":session_id,"next_session":next_id,
                        "delay_ms":delay,"elapsed_ms":start.elapsed().as_millis(),"writeback":writeback}}))?;
                    if !wait(delay, stop, start, options.duration, log)? {
                        return Ok("stopped_while_reconnecting");
                    }
                    attempts = attempts.saturating_add(1);
                    match factory() {
                        Ok(provider) => {
                            if stopped(stop, start, options.duration) {
                                return Ok("stopped_while_reconnecting");
                            }
                            // Preserve configured direction, but never pending writes.
                            // Fresh simulator readings are required before any new edit.
                            log.record(json!({"recovery_device_begin":{"session":next_id,
                                "elapsed_ms":start.elapsed().as_millis()}}))?;
                            session = Some(handoff.resume(provider, MonotonicClock::default())?);
                            session_id = next_id;
                            sessions = sessions.saturating_add(1);
                            diagnostic = Instant::now();
                            probe_due = None;
                            numeric_seen = false;
                            log.record(json!({"start":{"session":session_id,"source":kind,"profile":"altimeter-v1",
                                "writeback":writeback,"recovered":true,"elapsed_ms":start.elapsed().as_millis()}}))?;
                            println!("Reconnected: session={session_id}; waiting for fresh {kind} readings. Automatic gauge synchronization is active, writeback={writeback}. No restart or flag change needed.");
                            break;
                        }
                        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                            log.record(json!({"reconnect_failed":{"attempt":attempts,"error":error.to_string(),
                                "next_session":next_id,"elapsed_ms":start.elapsed().as_millis()}}))?;
                            eprintln!(
                                "Simulator not available yet; waiting. Ctrl+C stops reconnection."
                            );
                        }
                        Err(error) => return Err(error),
                    }
                }
                continue;
            }
            if probe_due.is_some_and(|due| Instant::now() >= due) {
                let mut record =
                    json!({"session":session_id,"elapsed_ms":start.elapsed().as_millis()});
                record["source"] = json!(active.source_status()?);
                if options.hardware {
                    record["display"] = json!(active.display_status(false)?);
                    record["sample"] = json!(active.display_status(true)?);
                }
                record["query_end_elapsed_ms"] = json!(start.elapsed().as_millis());
                log.record(json!({"session_probe":record}))?;
                probe_due = None;
            }
            if (options.diagnostics || options.frame_timing)
                && diagnostic.elapsed() >= Duration::from_secs(1)
            {
                let mut record = json!({"elapsed_ms":start.elapsed().as_millis(),"source":active.source_status()?});
                if options.hardware {
                    record["display"] = json!(active.display_status(false)?);
                    if options.frame_timing {
                        record["sample"] = json!(active.display_status(true)?);
                    }
                }
                log.record(json!({"diagnostic":record}))?;
                diagnostic = Instant::now();
            }
            log.check()?;
            std::thread::sleep(Duration::from_millis(33).saturating_sub(at.elapsed()));
        }
    })();
    if let Some(active) = session.as_mut() {
        if outcome.is_err() && active.state() == State::Active {
            let _ = active.stop();
        }
        accumulate(&mut total, active.statistics());
    }
    let elapsed = start.elapsed().as_secs_f64();
    let report = json!({"end":outcome.as_ref().copied().unwrap_or("error"),"error":outcome.as_ref().err().map(ToString::to_string),
        "frames":total.frames,"sessions":sessions,"reconnect_attempts":attempts,"last_session":session_id,
        "elapsed_s":elapsed,"accepted_hz":total.frames as f64/elapsed,"max_ack_ms":total.max_ack_ms,
        "max_sample_gap_ms":total.max_sample_gap_ms,"gap_scope":"within sessions; recovery waits excluded",
        "state":session.as_ref().map(|s|format!("{:?}",s.state())).unwrap_or_else(||"Reconnecting".into())});
    let _ = log.record(json!({"summary":report}));
    Ok(report)
}
fn record_tick(log: &Log, tick: Tick, start: Instant) -> io::Result<()> {
    if let Some((frame, ack)) = tick.telemetry {
        log.record(json!({"elapsed_ms":start.elapsed().as_millis(),"frame":frame,"ack":ack}))?;
    }
    for (request, result) in tick.controls {
        log.record(match request {
            Some(request) => json!({"control_request":request,"result":result}),
            None => json!({"control_result":result}),
        })?;
    }
    Ok(())
}
