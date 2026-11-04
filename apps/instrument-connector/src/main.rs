#![forbid(unsafe_code)]
mod config;
mod logging;
#[cfg(all(windows, feature = "msfs"))]
mod msfs_worker;
mod runner;
mod switch_demo;
mod transport;
use connector_runtime::DeviceLink;
use serde_json::json;
use sim_bridge::{Replay, SimulatorProvider, Synthetic};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, BufReader},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};
const HELP: &str = r#"Round Instruments native connector (development milestone)
  instrument-connector devices
  instrument-connector enroll --port COM5 --config device.json
  instrument-connector run --config device.json --msfs --dll PATH [--writeback]
  instrument-connector run --desktop PATH_TO_DESKTOP_CLI --synthetic 5
  instrument-connector run --desktop PATH_TO_DESKTOP_CLI --replay FILE
  instrument-connector run --desktop PATH_TO_DESKTOP_CLI --switch-demo
Run options: --log-dir PATH (default logs), --duration SECONDS,
             --diagnostics, --frame-timing (hardware only), --sixpack (T2 firmware)
Bench only: --synthetic-age-ms 0..1000 (requires --synthetic)
Default: automatically synchronize altitude and QNH from MSFS to the gauge.
Aircraft switches reconnect automatically; no restart or flag change is needed.
--writeback optionally enables gauge-to-MSFS edits and persists across switches.
Ctrl+C sends Disconnected and releases the port.
--switch-demo runs three synthetic sessions to test recovery without MSFS.
"#;
fn main() {
    #[cfg(all(windows, feature = "msfs"))]
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "__simconnect-worker")
    {
        let args: Vec<_> = std::env::args_os().collect();
        let code = if args.len() == 3 {
            msfs_worker::entry(Path::new(&args[2]))
        } else {
            2
        };
        std::process::exit(code);
    }
    if let Err(e) = run() {
        eprintln!("Connector stopped: {e}. External data expires; no demo fallback.");
        std::process::exit(1);
    }
}
fn options(args: &[String]) -> io::Result<BTreeMap<String, String>> {
    let mut options = BTreeMap::new();
    let mut it = args.iter();
    while let Some(key) = it.next() {
        let val = match key.as_str() {
            "--msfs" | "--writeback" | "--diagnostics" | "--frame-timing" | "--switch-demo"
            | "--sixpack" | "--app-status" => String::new(),
            "--port" | "--config" | "--dll" | "--desktop" | "--synthetic" | "--replay"
            | "--log-dir" | "--duration" | "--stop-file" | "--synthetic-age-ms" => it
                .next()
                .filter(|v| !v.starts_with("--"))
                .ok_or_else(|| io::Error::other(format!("value needed for {key}")))?
                .clone(),
            _ => return Err(io::Error::other(format!("unknown option {key}"))),
        };
        if options.insert(key.clone(), val).is_some() {
            return Err(io::Error::other(format!("duplicate option {key}")));
        }
    }
    Ok(options)
}
fn seconds(s: &str) -> io::Result<u64> {
    let value = s.parse::<u64>().map_err(io::Error::other)?;
    if value == 0 || value > 86400 {
        return Err(io::Error::other("duration must be 1..86400 seconds"));
    }
    Ok(value)
}
fn run() -> io::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() || args == ["--help"] {
        print!("{HELP}");
        return Ok(());
    }
    let mode = &args[0];
    let opts = options(&args[1..])?;
    if mode == "devices" && opts.is_empty() {
        for p in config::candidates()? {
            println!(
                "{} CH343 serial={}",
                p.port,
                p.serial.as_deref().unwrap_or("<missing>")
            );
        }
        return Ok(());
    }
    if mode == "enroll" && opts.len() == 2 {
        let port = opts
            .get("--port")
            .ok_or_else(|| io::Error::other("--port required"))?;
        let path = opts
            .get("--config")
            .ok_or_else(|| io::Error::other("--config required"))?;
        config::Configuration::enroll(port, Path::new(path))?;
        println!("Enrolled USB identity in {path}; no device commands sent.");
        return Ok(());
    }
    if mode != "run" || opts.contains_key("--port") {
        return Err(io::Error::other(HELP));
    }
    let msfs = opts.contains_key("--msfs");
    if opts.contains_key("--synthetic-age-ms") && !opts.contains_key("--synthetic") {
        return Err(io::Error::other("--synthetic-age-ms requires --synthetic"));
    }
    let writeback = opts.contains_key("--writeback");
    let hardware = opts.contains_key("--config");
    if ["--msfs", "--synthetic", "--replay", "--switch-demo"]
        .iter()
        .filter(|k| opts.contains_key(**k))
        .count()
        != 1
        || hardware == opts.contains_key("--desktop")
        || (writeback && !msfs)
        || (opts.contains_key("--dll") && !msfs)
        || (opts.contains_key("--frame-timing") && !hardware)
    {
        return Err(io::Error::other("select one provider and one device; writeback requires MSFS, frame timing requires hardware"));
    }
    let duration = opts.get("--duration").map(|s| seconds(s)).transpose()?;
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();
    ctrlc::set_handler(move || stop_flag.store(true, Ordering::Release))
        .map_err(io::Error::other)?;
    if let Some(path) = opts.get("--stop-file") {
        let path = PathBuf::from(path);
        let flag = stop.clone();
        std::thread::spawn(move || {
            while !flag.load(Ordering::Acquire) {
                if path.exists() {
                    flag.store(true, Ordering::Release);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        });
    }
    let session: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_micros()
        .try_into()
        .map_err(io::Error::other)?;
    let root = PathBuf::from(opts.get("--log-dir").map(String::as_str).unwrap_or("logs"));
    let mut log = logging::Log::open(&root, session)?;
    println!("Session {session}; logs: {}", log.directory.display());
    log.record(
        json!({"run_info":{"connector_version":env!("CARGO_PKG_VERSION"),
        "diagnostics_schema":2,"msfs_enabled":cfg!(feature = "msfs")}}),
    )?;
    let result = execute(&opts, session, &stop, &log, duration);
    stop.store(true, Ordering::Release);
    let summary = match &result {
        Ok(value) => value.clone(),
        Err(e) => json!({"end":"error","error":e.to_string(),
        "note":"Device may retain its last observation until independent freshness expiry."}),
    };
    // Summary is outside the telemetry loop and saved even if the async log failed.
    let summary_result = (|| -> io::Result<()> {
        let file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(log.directory.join("summary.json"))?;
        serde_json::to_writer_pretty(file, &summary).map_err(io::Error::other)
    })();
    let finish = log.finish();
    println!("{summary}");
    result?;
    summary_result?;
    finish?;
    if summary["end"] == "error" {
        return Err(io::Error::other(
            summary["error"].as_str().unwrap_or("session failed"),
        ));
    }
    Ok(())
}
fn create_provider(
    opts: &BTreeMap<String, String>,
    demo_index: &mut u32,
) -> io::Result<Box<dyn SimulatorProvider>> {
    if opts.contains_key("--msfs") {
        let path = opts
            .get("--dll")
            .ok_or_else(|| io::Error::other("--dll PATH required for MSFS"))?;
        #[cfg(all(windows, feature = "msfs"))]
        {
            Ok(Box::new(sim_bridge::msfs::Msfs::new(
                msfs_worker::Connection::open(Path::new(path))?,
            )))
        }
        #[cfg(not(all(windows, feature = "msfs")))]
        {
            let _ = path;
            Err(io::Error::other("build on Windows with --features msfs"))
        }
    } else if let Some(s) = opts.get("--synthetic") {
        let age = opts.get("--synthetic-age-ms").map_or(Ok(0), |v| v.parse::<u32>().map_err(io::Error::other))?;
        Ok(Box::new(Synthetic::new(seconds(s)? * 1000)?.with_sample_age(age)?))
    } else if opts.contains_key("--switch-demo") {
        switch_demo::next(demo_index)
    } else {
        Ok(Box::new(Replay::read(BufReader::new(File::open(
            &opts["--replay"],
        )?))?))
    }
}
fn execute(
    opts: &BTreeMap<String, String>,
    session_id: u64,
    stop: &AtomicBool,
    log: &logging::Log,
    duration: Option<u64>,
) -> io::Result<serde_json::Value> {
    #[cfg(all(windows, feature = "msfs"))]
    let _timer = simconnect_native::timing::TimerResolution::one_ms()?;
    let mut demo_index = 0;
    let provider_clock = Instant::now();
    let mut open_attempt = 0u64;
    let mut factory = || {
        open_attempt = open_attempt.saturating_add(1);
        let began = Instant::now();
        log.record(json!({"provider_open_begin":{"attempt":open_attempt,
            "execute_elapsed_ms":provider_clock.elapsed().as_millis()}}))?;
        println!("Opening simulator source (attempt {open_attempt})...");
        let result = create_provider(opts, &mut demo_index);
        log.record(json!({"provider_open_end":{"attempt":open_attempt,
            "execute_elapsed_ms":provider_clock.elapsed().as_millis(),
            "duration_ms":began.elapsed().as_millis(),
            "ok":result.is_ok(),"error":result.as_ref().err().map(ToString::to_string)}}))?;
        result
    };
    // Initial simulator connection still fails before touching the device.
    // Only an established session's explicit recovery hint permits retries.
    let provider = factory()?;
    let device: Box<dyn DeviceLink> = if let Some(path) = opts.get("--config") {
        let config = config::Configuration::load(Path::new(path))?;
        let port = config.resolve(&config::candidates()?)?;
        println!("Connecting enrolled device on {port}");
        Box::new(transport::TextDevice::connect(
            transport::SerialWire::open(&port)?,
        )?)
    } else {
        Box::new(transport::TextDevice::connect(
            transport::DesktopWire::open(Path::new(&opts["--desktop"]))?,
        )?)
    };
    runner::run(
        provider,
        device,
        session_id,
        runner::Options {
            writeback: opts.contains_key("--writeback"),
            multi: opts.contains_key("--sixpack"),
            app_status: opts.contains_key("--app-status"),
            diagnostics: opts.contains_key("--diagnostics"),
            frame_timing: opts.contains_key("--frame-timing"),
            hardware: opts.contains_key("--config"),
            duration,
        },
        stop,
        log,
        &mut factory,
    )
}
