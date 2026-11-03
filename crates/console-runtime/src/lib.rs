#![forbid(unsafe_code)]

use bounded_mailbox::{self as mpsc, TryRecvError, TrySendError};

use instrument_core::{App, Command, DisplayCommand, ImuCommand, SOURCE_PERIOD_MS};
use std::{
    io::{self, Read, Write},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use text_cli::{Error, Framer, Response};
mod echo;

const COMMAND_CAPACITY: usize = 8;
const RESPONSE_CAPACITY: usize = 4;
// Reserve bounded packet storage once with the mailbox, never per-frame boxes.
#[allow(clippy::large_enum_variant)]
enum Input {
    Command(Result<Command, Error>),
    Echo(bool),
    End(Option<io::Error>),
}

#[derive(Default)]
pub struct Options {
    /// True only when the platform supplies a mounted, typed IMU attitude input.
    pub imu_attitude: bool,
    pub display_managed: bool,
    pub echo: bool,
    /// Platform scheduler adapter. Must block for at least one scheduler tick;
    /// immediate yields cannot provide the idle watchdog a scheduling window.
    pub owner_wait: Option<fn(Duration)>,
}

#[derive(Default)]
pub struct Metrics {
    pub ticks: AtomicU32,
    pub max_gap_ms: AtomicU32,
    pub tx_backpressure_ticks: AtomicU32,
}

/// Input and output may block only in their own threads. The owner never waits
/// on console I/O or a full queue. Bounded channels are allocated at startup.
pub fn run<R, W, H>(reader: R, writer: W, metrics: Arc<Metrics>, health_hook: H) -> io::Result<()>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
    H: FnMut(&App) -> io::Result<()>,
{
    run_with_imu(reader, writer, metrics, health_hook, |_| {
        text_cli::reply(format_args!("ERR imu_unsupported_target"))
    })
}

/// The IMU handler must only enqueue work or read a snapshot without blocking.
pub fn run_with_imu<R, W, H, I>(
    reader: R,
    writer: W,
    metrics: Arc<Metrics>,
    health_hook: H,
    imu: I,
) -> io::Result<()>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
    H: FnMut(&App) -> io::Result<()>,
    I: FnMut(ImuCommand) -> Response,
{
    run_with_devices(
        reader,
        writer,
        metrics,
        health_hook,
        imu,
        |_| text_cli::reply(format_args!("ERR display_unsupported_target")),
        Options::default(),
    )
}
pub fn run_with_devices<R, W, H, I, D>(
    reader: R,
    writer: W,
    metrics: Arc<Metrics>,
    mut health_hook: H,
    imu: I,
    display: D,
    options: Options,
) -> io::Result<()>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
    H: FnMut(&App) -> io::Result<()>,
    I: FnMut(ImuCommand) -> Response,
    D: FnMut(DisplayCommand) -> Response,
{
    run_with_owner_input(
        reader,
        writer,
        metrics,
        |app| health_hook(app),
        imu,
        display,
        options,
    )
}
/// A bounded, nonblocking physical-input hook runs on the single state owner
/// every tick, even when UART output is backpressured. It may apply validated
/// application commands before publishing the snapshot. No hardware I/O here.
pub fn run_with_owner_input<R, W, H, I, D>(
    mut reader: R,
    mut writer: W,
    metrics: Arc<Metrics>,
    mut health_hook: H,
    mut imu: I,
    mut display: D,
    options: Options,
) -> io::Result<()>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
    H: FnMut(&mut App) -> io::Result<()>,
    I: FnMut(ImuCommand) -> Response,
    D: FnMut(DisplayCommand) -> Response,
{
    let epoch = Instant::now();
    let (input_tx, input_rx) = mpsc::sync_channel(COMMAND_CAPACITY);
    let (output_tx, output_rx) = mpsc::sync_channel::<Response>(RESPONSE_CAPACITY);
    let echo_tx = output_tx.clone();
    let tx_failed = Arc::new(AtomicBool::new(false));
    let failure_flag = tx_failed.clone();
    let output = thread::Builder::new()
        .name("cli-tx".into())
        .stack_size(8192)
        .spawn(move || {
            for response in output_rx {
                if let Err(error) = writer
                    .write_all(response.as_bytes())
                    .and_then(|_| writer.flush())
                {
                    failure_flag.store(true, Ordering::Release);
                    return Err(error);
                }
            }
            Ok(())
        })?;
    let _input = thread::Builder::new()
        .name("cli-rx".into())
        // T2 parses a bounded 24-measurement snapshot by value. Debug host
        // builds retain extra copies; embedded builds optimize these away.
        .stack_size(if cfg!(target_os = "espidf") {
            32768
        } else {
            262144
        })
        .spawn(move || {
            let mut framer = Framer::new();
            let mut echo = echo::Echo::new(options.echo);
            let mut byte = [0u8; 1];
            loop {
                match reader.read(&mut byte) {
                    Ok(0) => {
                        let _ = input_tx.send(Input::End(None));
                        return;
                    }
                    Ok(_) => {
                        let received_at = elapsed_ms(epoch);
                        // One TX worker serializes echoed characters and whole
                        // replies. Only this RX worker may wait for echo space;
                        // the state owner still uses nonblocking sends.
                        if let Some(chunk) = echo.push(byte[0]) {
                            if echo_tx.send(chunk).is_err() {
                                return;
                            }
                        }
                        if let Some(request) = framer.push(byte[0], received_at) {
                            let input = match request {
                                Ok(Command::Echo(setting)) => {
                                    if let Some(enabled) = setting {
                                        echo.enabled = enabled;
                                    }
                                    Input::Echo(echo.enabled)
                                }
                                _ => Input::Command(request),
                            };
                            if input_tx.send(input).is_err() {
                                return;
                            }
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
                    Err(error) => {
                        let _ = input_tx.send(Input::End(Some(error)));
                        return;
                    }
                }
            }
        })?;
    let mut app = App::new();
    let mut pending = Some(if options.display_managed {
        text_cli::reply(format_args!(
            "READY protocol=1 source=sim display=managed echo={} commands=help,status,display,echo",
            if options.echo { "on" } else { "off" }
        ))
    } else {
        text_cli::ready()
    });
    loop {
        let started = Instant::now();
        let now = elapsed_ms(epoch);
        let sweep_was_running = app.takeoff().running();
        app.tick(now);
        if sweep_was_running && !app.takeoff().running() {
            let _ = display(DisplayCommand::StatsStop);
        }
        metrics.ticks.fetch_add(1, Ordering::Relaxed);
        metrics.max_gap_ms.store(
            app.max_update_gap_ms().min(u32::MAX as u64) as u32,
            Ordering::Relaxed,
        );
        health_hook(&mut app)?;
        if tx_failed.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "CLI output failed",
            ));
        }
        if pending.is_none() {
            match input_rx.try_recv() {
                Ok(Input::Command(command)) => {
                    let sweep_was_running = app.takeoff().running();
                    let starts_sweep = matches!(command, Ok(Command::Takeoff(_)));
                    pending = Some(match command {
                        Ok(Command::SourceImu) => {
                            if !options.imu_attitude {
                                text_cli::reply(format_args!("ERR imu_unsupported_target"))
                            } else if app.timing_fault() {
                                text_cli::reply(format_args!("ERR not_ready timing_fault=true"))
                            } else {
                                let response = imu(ImuCommand::On);
                                if response.as_str().starts_with("OK ") {
                                    text_cli::execute_with_display(
                                        &mut app,
                                        command,
                                        now,
                                        if options.display_managed {
                                            "managed"
                                        } else {
                                            "off"
                                        },
                                    )
                                } else {
                                    response
                                }
                            }
                        }
                        Ok(Command::Imu(cmd)) => imu(cmd),
                        Ok(Command::Display(cmd)) => display(cmd),
                        _ => text_cli::execute_with_display(
                            &mut app,
                            command,
                            now,
                            if options.display_managed {
                                "managed"
                            } else {
                                "off"
                            },
                        ),
                    });
                    if starts_sweep
                        && pending
                            .as_ref()
                            .is_some_and(|r| r.as_str().starts_with("OK "))
                    {
                        // Platform operations only enqueue/update metadata. The
                        // simulation and its timing remain portable owner state.
                        let _ = display(DisplayCommand::Gauge);
                        let _ = display(DisplayCommand::On);
                        let _ = display(DisplayCommand::StatsReset);
                    } else if sweep_was_running && !app.takeoff().running() {
                        let _ = display(DisplayCommand::StatsStop);
                    }
                }
                Ok(Input::Echo(enabled)) => {
                    pending = Some(text_cli::reply(format_args!(
                        "OK echo={}",
                        if enabled { "on" } else { "off" }
                    )));
                }
                Ok(Input::End(error)) => {
                    // EOF is meaningful for the desktop runner. It is a fault on
                    // the board, whose caller deliberately stops feeding its watchdog.
                    drop(output_tx);
                    let written = output
                        .join()
                        .map_err(|_| io::Error::other("CLI output panicked"))?;
                    written?;
                    return error.map_or(Ok(()), Err);
                }
                Err(TryRecvError::Empty) => (),
                Err(TryRecvError::Disconnected) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "CLI input disconnected",
                    ))
                }
            }
        }
        // Send newly produced acknowledgements in this tick. Deferring every
        // response until the next owner tick needlessly caps a stop/wait bridge.
        // try_send remains bounded and never waits for UART/output readiness.
        if let Some(response) = pending.take() {
            match output_tx.try_send(response) {
                Ok(()) => (),
                Err(TrySendError::Full(response)) => {
                    pending = Some(response);
                    metrics
                        .tx_backpressure_ticks
                        .fetch_add(1, Ordering::Relaxed);
                }
                Err(TrySendError::Disconnected(_)) => {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "CLI output disconnected",
                    ))
                }
            }
        }
        // Sub-tick platform sleeps can become a non-blocking yield and starve
        // idle tasks. The platform adapter guarantees at least one actual tick,
        // even when work consumes the entire source period.
        // Tick lateness remains measured by App; this never fabricates samples.
        // Physical receipt and owner publication are separate timing stages.
        // A faster owner handoff in bench-IMU mode does not resample old inputs.
        let period = if app.imu_bench() || app.external().is_some_and(|e| e.multi_profile) {
            5
        } else {
            SOURCE_PERIOD_MS
        };
        let rest = Duration::from_millis(period).saturating_sub(started.elapsed());
        if let Some(wait) = options.owner_wait {
            wait(rest);
        } else {
            thread::sleep(rest.max(Duration::from_millis(10)));
        }
    }
}
fn elapsed_ms(epoch: Instant) -> u64 {
    epoch.elapsed().as_millis().min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imu_source_requires_platform_support_and_successful_enqueue() {
        #[derive(Clone)]
        struct Capture(Arc<Mutex<Vec<u8>>>);
        impl Write for Capture {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        for (available, enqueue, expected) in [
            (false, true, "ERR imu_unsupported_target"),
            (true, false, "ERR imu_busy"),
            (true, true, "OK source=imu_bench"),
        ] {
            let bytes = Arc::new(Mutex::new(Vec::new()));
            let mut requested = 0;
            let mut selected = false;
            run_with_owner_input(
                Cursor::new(b"source imu\nstatus\n".to_vec()),
                Capture(bytes.clone()),
                Arc::new(Metrics::default()),
                |app| {
                    selected |= app.imu_bench();
                    Ok(())
                },
                |cmd| {
                    assert_eq!(cmd, ImuCommand::On);
                    requested += 1;
                    text_cli::reply(format_args!(
                        "{}",
                        if enqueue {
                            "OK imu requested=on"
                        } else {
                            "ERR imu_busy"
                        }
                    ))
                },
                |_| unreachable!(),
                Options {
                    imu_attitude: available,
                    ..Options::default()
                },
            )
            .unwrap();
            let output = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
            assert!(output.contains(expected), "{output}");
            assert_eq!(requested, usize::from(available));
            assert_eq!(selected, available && enqueue);
        }
    }
    use std::sync::mpsc;
    use std::{
        io::Cursor,
        sync::{Condvar, Mutex},
    };
    struct GatedWriter(Arc<(Mutex<bool>, Condvar)>);
    impl Write for GatedWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let (lock, wake) = &*self.0;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = wake.wait(released).unwrap();
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn blocked_output_does_not_block_state_updates() {
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let metrics = Arc::new(Metrics::default());
        let m = metrics.clone();
        let g = gate.clone();
        let applied = Arc::new(AtomicBool::new(false));
        let applied_by_owner = applied.clone();
        let worker = thread::spawn(move || {
            let pressure = m.clone();
            run_with_owner_input(
                Cursor::new(b"status\n".repeat(20)),
                GatedWriter(g),
                m,
                move |app| {
                    if pressure.tx_backpressure_ticks.load(Ordering::Relaxed) >= 5 {
                        app.apply(Command::Baro(30.01)).unwrap();
                        applied_by_owner.store(app.baro_inhg() == 30.01, Ordering::Relaxed);
                    }
                    Ok(())
                },
                |_| unreachable!(),
                |_| unreachable!(),
                Options::default(),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(3);
        while metrics.tx_backpressure_ticks.load(Ordering::Relaxed) < 5 && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(10));
        }
        let before = metrics.ticks.load(Ordering::Relaxed);
        thread::sleep(Duration::from_millis(100));
        let after = metrics.ticks.load(Ordering::Relaxed);
        let blocked = metrics.tx_backpressure_ticks.load(Ordering::Relaxed);
        *gate.0.lock().unwrap() = true;
        gate.1.notify_all();
        worker.join().unwrap().unwrap();
        assert!(blocked >= 5, "test did not create backpressure");
        assert!(after > before, "sampling stalled with console output");
        assert!(
            applied.load(Ordering::Relaxed),
            "physical-input hook blocked on UART output"
        );
    }
    #[test]
    fn broken_output_terminates_instead_of_silently_losing_control() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "injected"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert!(run(
            Cursor::new(b"status\n".repeat(10)),
            Broken,
            Arc::new(Metrics::default()),
            |_| Ok(())
        )
        .is_err());
    }

    #[test]
    fn character_echo_precedes_enter_and_can_be_disabled_without_changing_commands() {
        struct Reader(mpsc::Receiver<u8>);
        impl Read for Reader {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                match self.0.recv() {
                    Ok(byte) => {
                        bytes[0] = byte;
                        Ok(1)
                    }
                    Err(_) => Ok(0),
                }
            }
        }
        struct Writer(mpsc::Sender<Vec<u8>>);
        impl Write for Writer {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.0.send(bytes.to_vec()).unwrap();
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let (send, input) = mpsc::channel();
        let (output, receive) = mpsc::channel();
        let worker = thread::spawn(move || {
            run_with_devices(
                Reader(input),
                Writer(output),
                Arc::new(Metrics::default()),
                |_| Ok(()),
                |_| unreachable!(),
                |_| unreachable!(),
                Options {
                    imu_attitude: false,
                    owner_wait: None,
                    display_managed: false,
                    echo: true,
                },
            )
        });
        let next =
            || String::from_utf8(receive.recv_timeout(Duration::from_secs(3)).unwrap()).unwrap();
        assert!(next().starts_with("READY "));
        send.send(b's').unwrap();
        assert_eq!(next(), "s", "must echo before a line terminator arrives");
        for byte in b"tatus\r\n" {
            send.send(*byte).unwrap();
        }
        let mut echoed = String::new();
        loop {
            let chunk = next();
            if chunk.starts_with("OK status") {
                assert!(chunk.contains("sim=running"));
                break;
            }
            echoed.push_str(&chunk);
        }
        assert_eq!(echoed, "tatus\n");
        for byte in b"echo off\n" {
            send.send(*byte).unwrap();
        }
        while next() != "OK echo=off\n" {}
        for byte in b"status\n" {
            send.send(*byte).unwrap();
        }
        assert!(next().starts_with("OK status"), "echo must now be silent");
        drop(send);
        worker.join().unwrap().unwrap();
    }

    #[test]
    fn blocked_echo_output_does_not_block_state_updates() {
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let metrics = Arc::new(Metrics::default());
        let m = metrics.clone();
        let g = gate.clone();
        let worker = thread::spawn(move || {
            run_with_devices(
                Cursor::new(b"status\n".repeat(20)),
                GatedWriter(g),
                m,
                |_| Ok(()),
                |_| unreachable!(),
                |_| unreachable!(),
                Options {
                    imu_attitude: false,
                    owner_wait: None,
                    display_managed: false,
                    echo: true,
                },
            )
        });
        let deadline = Instant::now() + Duration::from_secs(3);
        while metrics.ticks.load(Ordering::Relaxed) < 5 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let before = metrics.ticks.load(Ordering::Relaxed);
        thread::sleep(Duration::from_millis(100));
        let after = metrics.ticks.load(Ordering::Relaxed);
        *gate.0.lock().unwrap() = true;
        gate.1.notify_all();
        worker.join().unwrap().unwrap();
        assert!(before >= 5 && after > before, "owner stalled behind echo");
    }
}
