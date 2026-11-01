//! SKU 29086 active buzzer: TCA9554 output bit 7 (vendor EXIO8), active high.
//! Only the shared bus worker performs I/O. Owner/CLI access uses atomics.
use crate::board_bus::Bus;
use esp_idf_svc::sys::EspError;
use instrument_core::DisplayCommand;
use instrument_ui::{feedback::PressSound, interaction::Input};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use text_cli::{reply, Response};

const MASK: u8 = 0x80;
const PULSE_MS: u64 = 40;
#[derive(Default)]
struct Shared {
    enabled: AtomicBool,
    ready: AtomicBool,
    // True also means an on write may have reached hardware before an error.
    active_or_unknown: AtomicBool,
    beeps: AtomicU32,
    errors: AtomicU32,
    last_error: AtomicI32,
}
#[derive(Clone)]
pub struct Service(Arc<Shared>);
impl Service {
    pub fn new() -> Self {
        let shared = Shared::default();
        shared.enabled.store(true, Ordering::Relaxed);
        Self(Arc::new(shared))
    }
    pub fn handle(&self, command: DisplayCommand) -> Option<Response> {
        match command {
            DisplayCommand::TouchSound(enabled) => self.0.enabled.store(enabled, Ordering::Release),
            DisplayCommand::TouchSoundStatus => (),
            _ => return None,
        }
        Some(reply(format_args!("OK touch sound enabled={} ready={} active_or_unknown={} pulse_ms={} beeps={} errors={} last_error={}",
            self.0.enabled.load(Ordering::Acquire), self.0.ready.load(Ordering::Acquire), self.0.active_or_unknown.load(Ordering::Acquire),
            PULSE_MS, self.0.beeps.load(Ordering::Relaxed), self.0.errors.load(Ordering::Relaxed), self.0.last_error.load(Ordering::Relaxed))))
    }
}
pub struct Worker {
    service: Service,
    press: PressSound,
    deadline: Option<Instant>,
}
fn read(bus: &mut Bus, register: u8) -> Result<u8, EspError> {
    let mut value = [0];
    bus.write_read(0x20, &[register], &mut value)?;
    Ok(value[0])
}
fn modify(bus: &mut Bus, register: u8, high: bool) -> Result<(), EspError> {
    let value = read(bus, register)?;
    let value = if high { value | MASK } else { value & !MASK };
    bus.write(0x20, &[register, value])?;
    if read(bus, register)? != value {
        return Err(EspError::from_infallible::<-1>());
    }
    Ok(())
}
impl Worker {
    pub fn new(service: Service, bus: &mut Bus) -> Self {
        let mut worker = Self {
            service,
            press: PressSound::default(),
            deadline: None,
        };
        // Set output LOW before enabling output direction. Preserve every other bit.
        let initialized = modify(bus, 1, false).and_then(|_| modify(bus, 3, false));
        match initialized {
            Ok(()) => worker.service.0.ready.store(true, Ordering::Release),
            Err(error) => worker.fault(error),
        }
        worker
    }
    fn fault(&mut self, error: EspError) {
        self.service.0.ready.store(false, Ordering::Release);
        self.service.0.errors.fetch_add(1, Ordering::Relaxed);
        self.service
            .0
            .last_error
            .store(error.code(), Ordering::Relaxed);
        self.service
            .0
            .active_or_unknown
            .store(true, Ordering::Release);
        self.deadline = Some(Instant::now() + Duration::from_millis(100));
    }
    /// Stop before potentially long IMU commands; no new sound until a release.
    /// False prevents the command from occupying the bus while shutoff is uncertain.
    pub fn quiesce(&mut self, bus: &mut Bus) -> bool {
        self.press.update(0, None, false);
        if self.service.0.active_or_unknown.load(Ordering::Acquire) {
            match modify(bus, 1, false) {
                Ok(()) => {
                    self.service
                        .0
                        .active_or_unknown
                        .store(false, Ordering::Release);
                    self.deadline = None;
                }
                Err(error) => {
                    self.fault(error);
                    return false;
                }
            }
        }
        true
    }
    pub fn poll(&mut self, bus: &mut Bus, now_ms: u64, input: Option<Input>) {
        let enabled = self.service.0.enabled.load(Ordering::Acquire);
        let ready = self.service.0.ready.load(Ordering::Acquire);
        let now = Instant::now();
        if self
            .deadline
            .is_some_and(|at| now >= at || (!enabled && ready))
        {
            // Keep the contact state here: ending a pulse must not re-arm it.
            match modify(bus, 1, false) {
                Ok(()) => {
                    self.service
                        .0
                        .active_or_unknown
                        .store(false, Ordering::Release);
                    self.deadline = None;
                }
                Err(error) => self.fault(error),
            }
        }
        if self.press.update(
            now_ms,
            input,
            enabled && self.service.0.ready.load(Ordering::Acquire),
        ) && self.deadline.is_none()
        {
            self.service
                .0
                .active_or_unknown
                .store(true, Ordering::Release);
            match modify(bus, 1, true) {
                Ok(()) => {
                    self.service.0.beeps.fetch_add(1, Ordering::Relaxed);
                    self.deadline = Some(Instant::now() + Duration::from_millis(PULSE_MS));
                }
                Err(error) => self.fault(error),
            }
        }
    }
}
