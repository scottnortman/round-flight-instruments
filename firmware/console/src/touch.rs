//! Board-only GT911 reset/transport. Polling is called by the single shared
//! I2C worker; the application owner reads only a bounded snapshot.
use esp_idf_svc::sys;
use gt911::{Contact, Driver, Error};
use instrument_ui::interaction;
use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

pub struct Bus<'a>(pub &'a mut crate::board_bus::Bus);
impl gt911::Bus for Bus<'_> {
    fn read(&mut self, a: u8, r: u16, b: &mut [u8]) -> Result<(), Error> {
        self.0
            .write_read(a, &r.to_be_bytes(), b)
            .map_err(|e| Error::Bus(e.code()))
    }
    fn write(&mut self, a: u8, r: u16, v: u8) -> Result<(), Error> {
        let [hi, lo] = r.to_be_bytes();
        self.0
            .write(a, &[hi, lo, v])
            .map_err(|e| Error::Bus(e.code()))
    }
}
fn gpio(code: i32) -> Result<(), Error> {
    if code == 0 {
        Ok(())
    } else {
        Err(Error::Bus(code))
    }
}
pub fn initialize(bus: &mut crate::board_bus::Bus) -> Result<Driver, Error> {
    // Vendor sequence: EXIO2 (mask 0x02) is touch reset, GPIO16 selects 0x5d.
    // Preserve the LCD, chip-select and other expander pins with read/modify/write.
    let mut output = [0];
    let mut config = [0];
    bus.write_read(0x20, &[1], &mut output)
        .map_err(|e| Error::Bus(e.code()))?;
    bus.write_read(0x20, &[3], &mut config)
        .map_err(|e| Error::Bus(e.code()))?;
    unsafe {
        gpio(sys::gpio_reset_pin(16))?;
        gpio(sys::gpio_set_direction(
            16,
            sys::gpio_mode_t_GPIO_MODE_OUTPUT,
        ))?;
        gpio(sys::gpio_set_level(16, 0))?;
    }
    bus.write(0x20, &[1, output[0] & !2])
        .map_err(|e| Error::Bus(e.code()))?;
    bus.write(0x20, &[3, config[0] & !2])
        .map_err(|e| Error::Bus(e.code()))?;
    thread::sleep(Duration::from_millis(150));
    bus.write(0x20, &[1, output[0] | 2])
        .map_err(|e| Error::Bus(e.code()))?;
    thread::sleep(Duration::from_millis(50));
    unsafe {
        gpio(sys::gpio_set_level(16, 1))?;
        gpio(sys::gpio_set_direction(
            16,
            sys::gpio_mode_t_GPIO_MODE_INPUT,
        ))?;
    }
    thread::sleep(Duration::from_millis(50));
    Driver::probe(&mut Bus(bus), 0x5d, 480, 480)
}
#[derive(Clone, Copy, Debug)]
pub struct State {
    pub ready: bool,
    pub polls: u64,
    pub reports: u64,
    pub errors: u64,
    pub error: Option<Error>,
    pub sample: Option<(Contact, Instant)>,
}
#[derive(Clone)]
pub struct Service {
    state: Arc<Mutex<State>>,
}
impl Service {
    pub fn new(result: &Result<Driver, Error>) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                ready: result.is_ok(),
                polls: 0,
                reports: 0,
                errors: 0,
                error: result.as_ref().err().copied(),
                // Identity/resolution probing does not observe contact state.
                // Only an acknowledged controller report may establish Up.
                sample: None,
            })),
        }
    }
    pub fn poll(&self, driver: &Driver, bus: &mut crate::board_bus::Bus) {
        let at = Instant::now();
        let result = driver.poll(&mut Bus(bus));
        if let Ok(mut state) = self.state.lock() {
            state.polls += 1;
            match result {
                Ok(Some(contact)) => {
                    state.reports += 1;
                    state.error = None;
                    state.sample = Some((contact, at));
                }
                Ok(None) => {
                    // Successful empty polls prove liveness only for an already
                    // released contact. Never extend the age of a held finger.
                    if matches!(state.sample, Some((Contact::Up, _))) {
                        state.sample = Some((Contact::Up, at));
                    }
                }
                Err(e) => {
                    state.errors += 1;
                    state.error = Some(e);
                    state.sample = None;
                }
            }
        }
    }
    pub fn snapshot(&self) -> Option<State> {
        self.state.try_lock().ok().map(|s| *s)
    }
}
impl State {
    pub fn input(self, now: u64) -> Option<interaction::Input> {
        let (contact, at) = self.sample?;
        let at_ms = now.checked_sub(at.elapsed().as_millis() as u64)?;
        let contact = match contact {
            Contact::Up => interaction::Contact::Up,
            Contact::Down { id, x, y } => interaction::Contact::Down { id, x, y },
            Contact::Multiple => interaction::Contact::Multiple,
        };
        Some(interaction::Input { at_ms, contact })
    }
}
