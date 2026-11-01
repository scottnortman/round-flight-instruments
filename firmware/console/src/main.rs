use esp_idf_svc::hal::{
    delay::{Ets, TickType},
    task::thread::ThreadSpawnConfiguration,
};
mod board_bus;
mod buzzer;
mod display;
mod panel_init;
mod touch;
use esp_idf_svc::{
    hal::{gpio::PinDriver, peripherals::Peripherals},
    sys,
};
use std::{io, sync::Arc, time::Duration};

struct ImuBus(board_bus::Bus);
impl qmi8658::Bus for ImuBus {
    fn read(&mut self, address: u8, register: u8, bytes: &mut [u8]) -> Result<(), qmi8658::Error> {
        self.0
            .write_read(address, &[register], bytes)
            .map_err(|e| qmi8658::Error::Bus(e.code()))
    }
    fn write(&mut self, address: u8, register: u8, value: u8) -> Result<(), qmi8658::Error> {
        self.0
            .write(address, &[register, value])
            .map_err(|e| qmi8658::Error::Bus(e.code()))
    }
    fn delay_us(&mut self, us: u32) {
        if us >= 1000 {
            std::thread::sleep(Duration::from_micros(us as u64));
        } else {
            Ets::delay_us(us);
        }
    }
}

// UART0 has exactly one reader. Read bytes directly so VFS line buffering
// cannot hide an incomplete line from the shared framer's timeout.
struct UartRx;
impl io::Read for UartRx {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        // SAFETY: the driver is installed before this reader is constructed;
        // the mutable buffer is live and exclusive for the synchronous call.
        let n = unsafe { sys::uart_read_bytes(0, bytes.as_mut_ptr().cast(), 1, u32::MAX) };
        if n == 1 {
            Ok(1)
        } else {
            Err(io::Error::other("UART read failed"))
        }
    }
}

fn main() {
    sys::link_patches();
    let peripherals = Peripherals::take().expect("peripherals unavailable");
    // SKU 29086: hold GPIO6 LOW until the independent display guard observes
    // healthy owner/renderer/scanout progress and a non-expired frame.
    let mut backlight =
        PinDriver::output(peripherals.pins.gpio6).expect("backlight pin unavailable");
    backlight.set_low().expect("cannot disable backlight");
    // SKU 29086 shared I2C: SDA15 / SCL7. One worker owns IMU and touch polling.
    let mut bus = board_bus::Bus::new(
        peripherals.i2c0,
        peripherals.pins.gpio15,
        peripherals.pins.gpio7,
    )
    .map_err(|e| io::Error::other(e.to_string()));
    let panel_ready = match &mut bus {
        Ok(bus) => display::initialize(bus).is_ok(),
        Err(_) => false,
    };
    let touch_driver = match &mut bus {
        Ok(bus) => touch::initialize(bus),
        Err(_) => Err(gt911::Error::Bus(-1)),
    };
    let touch = touch::Service::new(&touch_driver);
    let buzzer = buzzer::Service::new();
    let mut buzzer_worker = bus
        .as_mut()
        .ok()
        .map(|bus| buzzer::Worker::new(buzzer.clone(), bus));
    let display =
        display::Service::start(backlight, panel_ready).expect("display supervisor startup");
    let imu = bus.and_then(|bus| {
        // I2C completion must preempt CPU-bound drawing: equal-priority time
        // slicing can consume the entire 10 ms transaction deadline. Keep this
        // scheduling policy at the board boundary and restore it for later
        // console threads. Owner (10) and display guard (11) still preempt IMU.
        let mut configuration = ThreadSpawnConfiguration::get().unwrap_or_default();
        let previous_priority = configuration.priority;
        configuration.priority = 6;
        configuration.set().map_err(io::Error::other)?;
        let touch_poller = touch.clone();
        let touch_clock = std::time::Instant::now();
        let result = imu_service::Service::start_with_bus_poll(
            ImuBus(bus),
            imu_service::BenchTilt,
            move |bus, quiesce| {
                if quiesce {
                    return buzzer_worker
                        .as_mut()
                        .is_none_or(|worker| worker.quiesce(&mut bus.0));
                }
                if let Ok(driver) = &touch_driver {
                    touch_poller.poll(driver, &mut bus.0);
                }
                if let Some(worker) = &mut buzzer_worker {
                    let now = touch_clock.elapsed().as_millis() as u64;
                    worker.poll(
                        &mut bus.0,
                        now,
                        touch_poller.snapshot().and_then(|s| s.input(now)),
                    );
                }
                true
            },
        );
        configuration.priority = previous_priority;
        configuration
            .set()
            .expect("restore thread spawn configuration");
        result
    });
    // UART0 is the existing console routed to the CH343 (GPIO43/44).
    // FFI stays here; ESP-IDF owns the driver's buffers and VFS lifetime.
    unsafe {
        sys::esp!(sys::uart_driver_install(
            0,
            2048,
            0,
            0,
            core::ptr::null_mut(),
            0
        ))
        .expect("UART RX driver");
        // Keep ROM/boot diagnostics at 115200; the application CLI uses
        // 921600/8N1 with no flow control. Finish early logs before switching.
        sys::esp!(sys::uart_wait_tx_done(0, TickType::new_millis(100).ticks()))
            .expect("UART startup log drain");
        sys::esp!(sys::uart_set_baudrate(0, 921_600)).expect("UART CLI baud rate");
        sys::esp_vfs_dev_uart_use_driver(0);
        sys::esp!(sys::esp_task_wdt_add(core::ptr::null_mut()))
            .expect("task watchdog subscription");
        // State acquisition/validation must preempt the CPU-bound renderer
        // (ESP-IDF main defaults to priority 1; pthread workers default to 5).
        sys::vTaskPrioritySet(core::ptr::null_mut(), 10);
    }
    let result = console_runtime::run_with_owner_input(
        UartRx,
        io::stdout(),
        Arc::new(console_runtime::Metrics::default()),
        |app| {
            display.publish(app, touch.snapshot());
            #[cfg(feature = "watchdog-self-test")]
            if app.tick_count() == 100 {
                // Bench-only fault injection. The normal build has no stall.
                std::thread::sleep(Duration::from_secs(3));
            }
            if !app.timing_fault() {
                // Feed only after the owner makes progress within its timing budget.
                let result = unsafe { sys::esp_task_wdt_reset() };
                if result != 0 {
                    return Err(io::Error::other("watchdog feed failed"));
                }
            }
            Ok(())
        },
        |command| match &imu {
            Ok(service) => service.handle(command),
            Err(_) => text_cli::reply(format_args!("ERR imu_bus_unavailable")),
        },
        |command| {
            buzzer
                .handle(command)
                .unwrap_or_else(|| display.handle(command))
        },
        console_runtime::Options {
            display_managed: true,
            echo: true,
        },
    );
    // Losing the owner stops snapshot publication: the display guard blanks,
    // and this task stops feeding its watchdog to allow reset/recovery.
    let _ = result;
    loop {
        std::thread::sleep(Duration::from_millis(100));
    }
}
