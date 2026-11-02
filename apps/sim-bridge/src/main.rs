use sim_bridge::{to_frame, Replay, SimulatorProvider, Synthetic};
use std::{
    fs::File,
    io::{self, BufRead, Read, Write},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
fn main() {
    if let Err(error) = run() {
        eprintln!("Bridge provider: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let writeback = args.last().is_some_and(|a| a == "--writeback");
    if writeback {
        args.pop();
    }
    let mut provider: Box<dyn SimulatorProvider> = match args.as_slice() {
        [mode, seconds] if mode == "synthetic" => Box::new(Synthetic::new(
            seconds
                .parse::<u64>()?
                .checked_mul(1000)
                .ok_or("duration overflow")?,
        )?),
        [mode, path] if mode == "replay" => {
            Box::new(Replay::read(io::BufReader::new(File::open(path)?))?)
        }
        [mode, flag, path] if mode == "msfs" && flag == "--dll" => {
            #[cfg(all(windows, feature = "msfs"))]
            { Box::new(sim_bridge::msfs::Msfs::new(simconnect_native::Connection::open(std::path::Path::new(path))?)) }
            #[cfg(not(all(windows, feature = "msfs")))]
            { let _ = path; return Err("rebuild on Windows with --features msfs".into()); }
        }
        _ => {
            return Err(
                "usage: sim-bridge synthetic SECONDS | replay FILE | msfs --dll PATH; stdin: next, quit".into(),
            )
        }
    };
    let session = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_micros()
        .try_into()?;
    if writeback {
        provider
            .enable_controls(session)
            .map_err(|_| "writeback unsupported by this provider")?;
    }
    let mut out = io::BufWriter::new(io::stdout());
    writeln!(out, "PROVIDER {} {}", provider.kind().name(), session)?;
    out.flush()?;
    // Start the replay clock with the first poll, after the relay handshake.
    let mut start = None;
    let mut sequence = 0u64;
    let mut input = io::stdin().lock();
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = input.by_ref().take(257).read_line(&mut line)?;
        if bytes == 0 || line.trim() == "quit" {
            break;
        }
        if bytes > 256 {
            return Err("oversized request".into());
        }
        let now = start.get_or_insert_with(Instant::now).elapsed().as_millis() as u64;
        if line.trim() == "control" {
            if let Some(reply) = provider.control_status() {
                writeln!(
                    out,
                    "{}",
                    reply.encode().map_err(|_| "control encoding")?.as_str()
                )?;
            } else {
                writeln!(out, "NONE")?;
            }
            out.flush()?;
            continue;
        }
        if line.starts_with("C1 ") {
            let request = simulator_link::control::Request::decode(line.trim())
                .map_err(|_| "invalid control packet")?;
            let reply = if request.session == session {
                provider
                    .control(request, now)
                    .unwrap_or(simulator_link::control::Reply {
                        session: request.session,
                        id: request.id,
                        status: simulator_link::control::Status::Rejected,
                    })
            } else {
                simulator_link::control::Reply {
                    session: request.session,
                    id: request.id,
                    status: simulator_link::control::Status::Rejected,
                }
            };
            writeln!(
                out,
                "{}",
                reply.encode().map_err(|_| "control encoding")?.as_str()
            )?;
            out.flush()?;
            continue;
        }
        if line.trim() != "next" {
            return Err("expected bounded next request".into());
        }
        if let Some(o) = provider.poll(now) {
            sequence = sequence.checked_add(1).ok_or("sequence exhausted")?;
            let frame = to_frame(o, session, sequence, now)
                .map_err(|_| "unsupported altimeter measurement")?;
            writeln!(
                out,
                "{}",
                frame
                    .encode()
                    .map_err(|_| "invalid provider output")?
                    .as_str()
            )?;
        } else {
            writeln!(out, "IDLE")?;
        }
        out.flush()?;
    }
    Ok(())
}
