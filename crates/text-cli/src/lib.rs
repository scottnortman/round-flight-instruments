#![no_std]
#![forbid(unsafe_code)]

use core::fmt::{self, Write};
use instrument_core::{App, Command, DisplayCommand, Field, ImuCommand, Instrument};

pub const MAX_LINE: usize = 128;
pub const LINE_TIMEOUT_MS: u64 = 30_000;
pub const MAX_RESPONSE: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Syntax,
    UnknownField,
    UnknownInstrument,
    Number,
    Range,
    TooLong,
    InvalidByte,
    Timeout,
}
impl Error {
    pub fn name(self) -> &'static str {
        match self {
            Self::Syntax => "syntax",
            Self::UnknownField => "unknown_field",
            Self::UnknownInstrument => "unknown_instrument",
            Self::Number => "number",
            Self::Range => "range",
            Self::TooLong => "line_too_long",
            Self::InvalidByte => "invalid_byte",
            Self::Timeout => "line_timeout",
        }
    }
}

pub fn parse(line: &str) -> Result<Command, Error> {
    if line.len() > MAX_LINE {
        return Err(Error::TooLong);
    }
    if !line
        .bytes()
        .all(|b| b.is_ascii_graphic() || b == b' ' || b == b'\t')
    {
        return Err(Error::InvalidByte);
    }
    let mut words = [""; 5];
    let mut count = 0;
    for word in line.split_ascii_whitespace() {
        if count == words.len() {
            return Err(Error::Syntax);
        }
        words[count] = word;
        count += 1;
    }
    let number = |s: &str| -> Result<f32, Error> {
        let v: f32 = s.parse().map_err(|_| Error::Number)?;
        if v.is_finite() {
            Ok(v)
        } else {
            Err(Error::Number)
        }
    };
    match &words[..count] {
        ["sim", "takeoff"] => Ok(Command::Takeoff(60)),
        ["sim", "takeoff", "stop"] => Ok(Command::TakeoffStop),
        ["sim", "takeoff", "status"] => Ok(Command::TakeoffStatus),
        ["sim", "takeoff", seconds] => {
            let seconds = seconds.parse::<u32>().map_err(|_| Error::Number)?;
            if !(5..=600).contains(&seconds) {
                return Err(Error::Range);
            }
            Ok(Command::Takeoff(seconds))
        }
        ["touch", "status"] => Ok(Command::Display(DisplayCommand::TouchStatus)),
        ["touch", "sound", "on"] => Ok(Command::Display(DisplayCommand::TouchSound(true))),
        ["touch", "sound", "off"] => Ok(Command::Display(DisplayCommand::TouchSound(false))),
        ["touch", "sound", "status"] => Ok(Command::Display(DisplayCommand::TouchSoundStatus)),
        ["touch", "cancel"] => Ok(Command::Display(DisplayCommand::TouchCancel)),
        ["touch", "hold", value] => {
            let ms = value.parse::<u32>().map_err(|_| Error::Range)?;
            if !(250..=5000).contains(&ms) {
                return Err(Error::Range);
            }
            Ok(Command::Display(DisplayCommand::TouchHold(ms)))
        }
        ["display", "stats"] => Ok(Command::Display(DisplayCommand::Stats)),
        ["display", "stats", "reset"] => Ok(Command::Display(DisplayCommand::StatsReset)),
        ["display", "stats", "stop"] => Ok(Command::Display(DisplayCommand::StatsStop)),
        ["echo", "on"] => Ok(Command::Echo(Some(true))),
        ["echo", "off"] => Ok(Command::Echo(Some(false))),
        ["echo", "status"] => Ok(Command::Echo(None)),
        ["display", "status"] => Ok(Command::Display(DisplayCommand::Status)),
        ["display", "clock", "12"] => Ok(Command::Display(DisplayCommand::PixelClock(12_000_000))),
        ["display", "clock", "18"] => Ok(Command::Display(DisplayCommand::PixelClock(18_000_000))),
        ["display", "on"] => Ok(Command::Display(DisplayCommand::On)),
        ["display", "off"] => Ok(Command::Display(DisplayCommand::Off)),
        ["display", "pattern"] => Ok(Command::Display(DisplayCommand::Pattern)),
        ["display", "gauge"] => Ok(Command::Display(DisplayCommand::Gauge)),
        ["display", "stall"] => Ok(Command::Display(DisplayCommand::Stall)),
        ["display", "transfer-stall"] => Ok(Command::Display(DisplayCommand::TransferStall)),
        ["display", "recover"] => Ok(Command::Display(DisplayCommand::Recover)),
        ["imu", "on"] => Ok(Command::Imu(ImuCommand::On)),
        ["imu", "estimator"] => Ok(Command::Imu(ImuCommand::Estimator)),
        ["imu", "audit"] => Ok(Command::Imu(ImuCommand::Audit)),
        ["imu", "selftest"] => Ok(Command::Imu(ImuCommand::SelfTest)),
        ["imu", "off"] => Ok(Command::Imu(ImuCommand::Off)),
        ["imu", "status"] => Ok(Command::Imu(ImuCommand::Status)),
        ["imu", "read"] => Ok(Command::Imu(ImuCommand::Read)),
        ["help"] => Ok(Command::Help),
        ["status"] => Ok(Command::Status),
        ["instrument", "list"] => Ok(Command::Instruments),
        ["instrument", name] => Ok(Command::Select(
            Instrument::parse(name).ok_or(Error::UnknownInstrument)?,
        )),
        ["sim", "pause"] => Ok(Command::Pause),
        ["sim", "resume"] => Ok(Command::Resume),
        ["sim", "preset", "level"] => Ok(Command::LevelPreset),
        ["sim", "clear", name] => Ok(Command::Clear(
            Field::parse(name).ok_or(Error::UnknownField)?,
        )),
        ["sim", "set", name, value] => {
            let f = Field::parse(name).ok_or(Error::UnknownField)?;
            let v = number(value)?;
            if !f.accepts(v) {
                return Err(Error::Range);
            }
            Ok(Command::Set(f, v))
        }
        ["fault", name, flag] if *flag == "on" || *flag == "off" => Ok(Command::Fault(
            Field::parse(name).ok_or(Error::UnknownField)?,
            *flag == "on",
        )),
        ["baro", value] => {
            let v = number(value)?;
            if !(28.1..=31.0).contains(&v) {
                return Err(Error::Range);
            }
            Ok(Command::Baro(v))
        }
        _ => Err(Error::Syntax),
    }
}

/// Incremental, bounded ASCII framing. Errors discard the ENTIRE line.
pub struct Framer {
    bytes: [u8; MAX_LINE],
    len: usize,
    started: Option<u64>,
    error: Option<Error>,
    after_cr: bool,
}
impl Default for Framer {
    fn default() -> Self {
        Self::new()
    }
}
impl Framer {
    pub const fn new() -> Self {
        Self {
            bytes: [0; MAX_LINE],
            len: 0,
            started: None,
            error: None,
            after_cr: false,
        }
    }
    pub fn push(&mut self, b: u8, now: u64) -> Option<Result<Command, Error>> {
        if self.after_cr && b == b'\n' {
            self.after_cr = false;
            return None;
        }
        self.after_cr = false;
        if self.started.is_some_and(|start| {
            now.checked_sub(start)
                .is_none_or(|age| age >= LINE_TIMEOUT_MS)
        }) {
            self.error.get_or_insert(Error::Timeout);
        }
        if b == b'\r' || b == b'\n' {
            self.after_cr = b == b'\r';
            let result = if let Some(e) = self.error {
                Some(Err(e))
            } else if self.len == 0
                || self.bytes[..self.len]
                    .iter()
                    .all(|b| *b == b' ' || *b == b'\t')
            {
                None
            } else {
                Some(
                    core::str::from_utf8(&self.bytes[..self.len])
                        .map_err(|_| Error::InvalidByte)
                        .and_then(parse),
                )
            };
            self.len = 0;
            self.started = None;
            self.error = None;
            return result;
        }
        self.started.get_or_insert(now);
        if !(b.is_ascii_graphic() || b == b' ' || b == b'\t') {
            self.error.get_or_insert(Error::InvalidByte);
        }
        if self.len == MAX_LINE {
            self.error.get_or_insert(Error::TooLong);
        } else {
            self.bytes[self.len] = b;
            self.len += 1;
        }
        None
    }
}

pub struct Response {
    bytes: [u8; MAX_RESPONSE],
    len: usize,
}
impl Default for Response {
    fn default() -> Self {
        Self::new()
    }
}
impl Response {
    pub const fn new() -> Self {
        Self {
            bytes: [0; MAX_RESPONSE],
            len: 0,
        }
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(self.as_bytes()).unwrap_or("ERR internal\n")
    }
}
impl Write for Response {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.len + s.len() > self.bytes.len() {
            return Err(fmt::Error);
        }
        self.bytes[self.len..self.len + s.len()].copy_from_slice(s.as_bytes());
        self.len += s.len();
        Ok(())
    }
}
pub fn error_response(error: Error) -> Response {
    let mut r = Response::new();
    let _ = writeln!(r, "ERR {}", error.name());
    r
}
pub fn reply(args: fmt::Arguments<'_>) -> Response {
    let mut r = Response::new();
    if r.write_fmt(args).and_then(|_| r.write_char('\n')).is_err() {
        let mut fallback = Response::new();
        let _ = fallback.write_str("ERR internal_response_overflow\n");
        return fallback;
    }
    r
}
pub fn ready() -> Response {
    let mut r = Response::new();
    let _ = writeln!(
        r,
        "READY protocol=1 source=sim display=off commands=help,status"
    );
    r
}

pub fn execute(app: &mut App, request: Result<Command, Error>, now: u64) -> Response {
    execute_with_display(app, request, now, "off")
}
/// Display state is supplied by the platform; "managed" means consult display status.
pub fn execute_with_display(
    app: &mut App,
    request: Result<Command, Error>,
    now: u64,
    display: &str,
) -> Response {
    let cmd = match request {
        Ok(c) => c,
        Err(e) => return error_response(e),
    };
    if matches!(cmd, Command::Echo(_)) {
        return reply(format_args!("ERR echo_requires_console"));
    }
    if matches!(cmd, Command::Imu(_)) {
        return reply(format_args!("ERR imu_unsupported_target"));
    }
    if matches!(cmd, Command::Display(_)) {
        return reply(format_args!("ERR display_unsupported_target"));
    }
    if let Err(error) = app.apply(cmd) {
        return if error == instrument_core::ApplyError::NotReady {
            reply(format_args!("ERR not_ready timing_or_altitude_fault=true"))
        } else {
            error_response(Error::Range)
        };
    }
    let mut r = Response::new();
    let result = match cmd {
        Command::Takeoff(_) | Command::TakeoffStop | Command::TakeoffStatus => {
            let sweep = app.takeoff();
            writeln!(r, "OK takeoff state={} elapsed_ms={} total_ms={} climb_ms={} from_ft=0 to_ft=10000 ground_hold_ms=1000 source=sim render_stats=display_stats", sweep.state.name(), sweep.elapsed_ms, sweep.total_ms(), sweep.climb_ms)
        }
        Command::Echo(_) | Command::Imu(_) | Command::Display(_) => unreachable!(),
        Command::Help => writeln!(r, "OK help commands: touch status|cancel|hold MS; touch sound on|off|status; status; sim takeoff [SECONDS|stop|status]; display stats [reset|stop]; echo on|off|status; display status|on|off|pattern|gauge|stall|transfer-stall|recover; imu on|off|status|read|selftest|audit|estimator; instrument list|altimeter|attitude|airspeed|heading|turn|vsi; sim preset level; sim set FIELD VALUE; sim clear FIELD; sim pause|resume; fault FIELD on|off; baro INHG. fields: pressure_altitude_ft,pitch_deg,roll_deg,heading_deg,ias_kt,turn_dps,ball,vsi_fpm. SIMULATION ONLY; baro is a setting, pressure altitude is not corrected here."),
        Command::Instruments => writeln!(r, "OK instruments altimeter attitude airspeed heading turn vsi renderers=altimeter"),
        Command::Status => status(&mut r, app, now,display),
        Command::Set(f, _) => writeln!(r, "OK sim field={} applies=next_sample", f.name()),
        Command::Clear(f) => writeln!(r, "OK sim field={} cleared=true", f.name()),
        Command::Fault(f, active) => writeln!(r, "OK fault field={} active={}", f.name(), active),
        Command::Select(i) => writeln!(r, "OK instrument={} display={}", i.name(),display),
        Command::Pause => writeln!(r, "OK sim=paused"), Command::Resume => writeln!(r, "OK sim=running"),
        Command::LevelPreset => writeln!(r, "OK preset=level applies=next_sample faults=unchanged"),
        Command::Baro(v) => writeln!(r, "OK baro_inhg={:.2} pressure_altitude=unchanged", v),
    };
    if result.is_err() {
        let mut failure = Response::new();
        let _ = writeln!(failure, "ERR internal_response_overflow");
        failure
    } else {
        r
    }
}
fn status(r: &mut Response, app: &App, now: u64, display: &str) -> fmt::Result {
    write!(r, "OK status protocol=1 source=sim display={} instrument={} sim={} baro_inhg={:.2} timing_fault={} max_gap_ms={} ticks={}", display,app.selected().name(), if app.running() {"running"} else {"paused"}, app.baro_inhg(), app.timing_fault(), app.max_update_gap_ms(), app.tick_count())?;
    for f in Field::ALL {
        let sample = app.reading(f, now);
        write!(r, " {}=", f.name())?;
        if let Some(v) = sample.value {
            write!(r, "{:.3}", v)?;
        } else {
            r.write_str("-")?;
        }
        write!(r, ":{}:", sample.quality.name())?;
        if let Some(age) = sample.age_ms {
            write!(r, "{}ms", age)?;
        } else {
            r.write_str("none")?;
        }
    }
    r.write_char('\n')
}

#[cfg(test)]
mod tests {
    #[test]
    fn touch_controls_are_bounded_and_desktop_cannot_fabricate_touch() {
        assert_eq!(
            parse("touch sound on"),
            Ok(Command::Display(DisplayCommand::TouchSound(true)))
        );
        assert_eq!(
            parse("touch sound off"),
            Ok(Command::Display(DisplayCommand::TouchSound(false)))
        );
        assert_eq!(
            parse("touch sound status"),
            Ok(Command::Display(DisplayCommand::TouchSoundStatus))
        );
        assert!(parse("touch sound on extra").is_err());
        assert!(parse("touch sound loud").is_err());
        use super::*;
        assert_eq!(
            parse("touch status"),
            Ok(Command::Display(DisplayCommand::TouchStatus))
        );
        assert_eq!(
            parse("touch cancel"),
            Ok(Command::Display(DisplayCommand::TouchCancel))
        );
        assert_eq!(
            parse("touch hold 2000"),
            Ok(Command::Display(DisplayCommand::TouchHold(2000)))
        );
        for line in [
            "touch hold 249",
            "touch hold 5001",
            "touch hold NaN",
            "touch hold -1",
            "touch hold 2000 extra",
            "touch press 338 235",
        ] {
            assert!(parse(line).is_err());
        }
        let mut app = App::new();
        assert_eq!(
            execute(&mut app, parse("touch status"), 0).as_str(),
            "ERR display_unsupported_target\n"
        );
        assert_eq!(app.baro_inhg(), 29.92);
    }
    use super::*;
    #[test]
    fn takeoff_duration_and_status_are_bounded_and_explicit() {
        assert_eq!(parse("sim takeoff"), Ok(Command::Takeoff(60)));
        assert_eq!(parse("sim takeoff 5"), Ok(Command::Takeoff(5)));
        assert_eq!(parse("sim takeoff 600"), Ok(Command::Takeoff(600)));
        assert_eq!(parse("sim takeoff stop"), Ok(Command::TakeoffStop));
        for input in [
            "sim takeoff 0",
            "sim takeoff 601",
            "sim takeoff NaN",
            "sim takeoff 5.5",
            "sim takeoff -5",
            "sim takeoff 5 extra",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
        let mut app = App::new();
        app.tick(0);
        assert!(execute(&mut app, parse("sim takeoff 5"), 0)
            .as_str()
            .contains("total_ms=6000"));
        assert!(execute(&mut app, parse("sim takeoff stop"), 0)
            .as_str()
            .contains("state=stopped"));
        app.apply(Command::Fault(Field::PressureAltitudeFt, true))
            .unwrap();
        assert!(execute(&mut app, parse("sim takeoff"), 0)
            .as_str()
            .starts_with("ERR not_ready"));
        assert!(execute(&mut app, parse("help"), 0)
            .as_str()
            .contains("sim takeoff"));
    }
    #[test]
    fn strict_parsing_rejects_ambiguous_or_nonfinite_commands() {
        for line in [
            "sim set ias_kt NaN",
            "sim set ias_kt inf",
            "sim set pitch_deg 1 extra",
            "baro 29.92 extra",
            "status;sim pause",
            "sim set heading_deg 360",
            "sim set ias_kt -1",
            "baro 31.01",
            "fault pitch_deg yes",
            "sim set pitch 2",
        ] {
            assert!(parse(line).is_err(), "{line}");
        }
        assert_eq!(
            parse("sim set ias_kt 0"),
            Ok(Command::Set(Field::IasKt, 0.0))
        );
    }
    #[test]
    fn framing_handles_crlf_and_discards_overflow_until_delimiter() {
        let mut f = Framer::new();
        for b in b"status" {
            assert!(f.push(*b, 0).is_none());
        }
        assert_eq!(f.push(b'\r', 0), Some(Ok(Command::Status)));
        assert!(f.push(b'\n', 0).is_none());
        for _ in 0..129 {
            f.push(b' ', 0);
        }
        for b in b"sim pause" {
            f.push(*b, 0);
        }
        assert_eq!(f.push(b'\n', 0), Some(Err(Error::TooLong)));
        for b in b"status" {
            f.push(*b, 1);
        }
        assert_eq!(f.push(b'\n', 1), Some(Ok(Command::Status)));
    }
    #[test]
    fn invalid_bytes_and_partial_line_timeout_cannot_execute_suffix() {
        let mut f = Framer::new();
        f.push(0xff, 0);
        for b in b"sim pause" {
            f.push(*b, 1);
        }
        assert_eq!(f.push(b'\n', 1), Some(Err(Error::InvalidByte)));
        for b in b"sim " {
            f.push(*b, 10);
        }
        for b in b"pause" {
            f.push(*b, 10 + LINE_TIMEOUT_MS);
        }
        assert_eq!(
            f.push(b'\n', 10 + LINE_TIMEOUT_MS),
            Some(Err(Error::Timeout))
        );
    }
    #[test]
    fn rejected_requests_do_not_mutate_state_and_status_has_no_nan() {
        let mut app = App::new();
        let r = execute(&mut app, parse("baro NaN"), 0);
        assert!(r.as_str().starts_with("ERR"));
        assert_eq!(app.baro_inhg(), 29.92);
        execute(&mut app, parse("sim preset level"), 0);
        app.tick(0);
        let r = execute(&mut app, parse("status"), 100);
        assert!(r.as_str().contains("pitch_deg=-:stale:100ms"));
        assert!(r
            .as_str()
            .contains("pressure_altitude_ft=4520.000:valid:100ms"));
        assert!(r.as_bytes().len() < MAX_RESPONSE);
    }
    #[test]
    fn all_input_bytes_are_bounded_and_recover_at_newline() {
        for byte in 0..=255u8 {
            let mut framer = Framer::new();
            for _ in 0..300 {
                framer.push(byte, 0);
            }
            framer.push(b'\n', 1);
            for b in b"status" {
                framer.push(*b, 2);
            }
            assert_eq!(
                framer.push(b'\n', 2),
                Some(Ok(Command::Status)),
                "byte {byte}"
            );
        }
    }
    #[test]
    fn status_fits_with_maximum_age_and_all_quantities() {
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.tick(0);
        app.apply(Command::Pause).unwrap();
        let result = execute(&mut app, Ok(Command::Status), u64::MAX);
        assert!(result.as_str().starts_with("OK status"));
        assert!(result
            .as_str()
            .contains("vsi_fpm=-:stale:18446744073709551615ms"));
        assert!(result.as_bytes().ends_with(b"\n"));
        assert!(result.as_bytes().len() < MAX_RESPONSE);
    }
    #[test]
    fn display_clock_diagnostics_accept_only_verified_choices() {
        assert_eq!(
            parse("display clock 12"),
            Ok(Command::Display(DisplayCommand::PixelClock(12_000_000)))
        );
        assert_eq!(
            parse("display clock 18"),
            Ok(Command::Display(DisplayCommand::PixelClock(18_000_000)))
        );
        for invalid in [
            "display clock 0",
            "display clock 19",
            "display clock -12",
            "display clock NaN",
            "display clock 12 extra",
        ] {
            assert!(parse(invalid).is_err(), "{invalid}");
        }
        let mut app = App::new();
        assert_eq!(
            execute(&mut app, parse("display clock 12"), 0).as_str(),
            "ERR display_unsupported_target\n"
        );
    }
    #[test]
    fn imu_commands_are_strict_and_desktop_cannot_fabricate_a_sensor() {
        assert_eq!(parse("imu on"), Ok(Command::Imu(ImuCommand::On)));
        assert_eq!(
            parse("imu estimator"),
            Ok(Command::Imu(ImuCommand::Estimator))
        );
        assert_eq!(parse("imu audit"), Ok(Command::Imu(ImuCommand::Audit)));
        assert!(parse("imu estimator kalman").is_err());
        assert_eq!(
            parse("imu selftest"),
            Ok(Command::Imu(ImuCommand::SelfTest))
        );
        for invalid in [
            "imu on extra",
            "imu read 2",
            "imu write 3 4",
            "imu calibrate",
        ] {
            assert!(parse(invalid).is_err());
        }
        let mut app = App::new();
        assert_eq!(
            execute(&mut app, parse("imu read"), 0).as_str(),
            "ERR imu_unsupported_target\n"
        );
        assert_eq!(
            app.reading(Field::PitchDeg, 0).quality,
            instrument_core::Quality::Missing
        );
    }
}
