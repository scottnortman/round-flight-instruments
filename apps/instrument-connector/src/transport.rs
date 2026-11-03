use connector_runtime::DeviceLink;
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};
const LIMIT: usize = 1024;
const TIMEOUT: Duration = Duration::from_millis(200);

pub trait Wire {
    fn send(&mut self, bytes: &[u8], deadline: Instant) -> io::Result<()>;
    fn line(&mut self, deadline: Instant) -> io::Result<Vec<u8>>;
}
pub struct TextDevice<W: Wire> {
    wire: W,
    poisoned: bool,
    restore_echo: bool,
    negotiating: bool,
}
impl<W: Wire> TextDevice<W> {
    pub fn connect(wire: W) -> io::Result<Self> {
        let mut device = Self {
            wire,
            poisoned: false,
            restore_echo: false,
            negotiating: true,
        };
        match device.request("echo status")?.as_str() {
            "OK echo=on" => {
                device.restore_echo = true;
                if device.request("echo off")? != "OK echo=off" {
                    device.poisoned = true;
                    return Err(io::Error::other("echo negotiation failed"));
                }
            }
            "OK echo=off" => (),
            _ => {
                device.poisoned = true;
                return Err(io::Error::other("unsupported device protocol"));
            }
        }
        device.negotiating = false;
        Ok(device)
    }
    fn transact(&mut self, command: &str) -> io::Result<String> {
        if self.poisoned {
            return Err(io::Error::other(
                "transport ended; reopen before further I/O",
            ));
        }
        if command.len() > simulator_link::multi::MAX_FRAME
            || !command.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
        {
            return Err(io::Error::other("invalid outgoing command"));
        }
        let deadline = Instant::now() + TIMEOUT;
        self.wire
            .send(format!("{command}\n").as_bytes(), deadline)?;
        for _ in 0..64 {
            let bytes = self.wire.line(deadline)?;
            let line = match decode_line(&bytes) {
                Ok(line) => line,
                // ESP ROM startup text is 115200 baud while the application is
                // 921600. During initial echo negotiation only, bounded non-ACK
                // startup lines may be undecodable. Never relax ACK validation,
                // size/deadline bounds, or framing once a session can be active.
                Err(_)
                    if self.negotiating
                        && bytes.len() <= LIMIT
                        && bytes.ends_with(b"\n")
                        && !bytes.starts_with(b"OK ")
                        && !bytes.starts_with(b"ERR ") =>
                {
                    continue
                }
                Err(error) => return Err(error),
            };
            if line == command || line.is_empty() || line.starts_with("READY ") {
                continue;
            }
            if line.starts_with("OK ") || line.starts_with("ERR ") {
                return Ok(line.into());
            }
            // Bounded boot/diagnostic chatter never counts as acknowledgement.
        }
        Err(io::Error::other("too much unsolicited device output"))
    }
}
fn decode_line(bytes: &[u8]) -> io::Result<&str> {
    if bytes.len() > LIMIT || !bytes.ends_with(b"\n") || !bytes.is_ascii() {
        return Err(io::Error::other(format!(
            "invalid/oversized device response: length={} prefix={:02X?}",
            bytes.len(),
            &bytes[..bytes.len().min(48)]
        )));
    }
    let line = std::str::from_utf8(bytes)
        .map_err(io::Error::other)?
        .trim_end_matches(['\r', '\n']);
    if !line.bytes().all(|b| b.is_ascii_graphic() || b == b' ') {
        return Err(io::Error::other("invalid response characters"));
    }
    Ok(line)
}
impl<W: Wire> DeviceLink for TextDevice<W> {
    fn abandon(&mut self) {
        self.poisoned = true;
    }
    fn request(&mut self, command: &str) -> io::Result<String> {
        let result = self.transact(command);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
}
impl<W: Wire> Drop for TextDevice<W> {
    fn drop(&mut self) {
        if self.restore_echo && !self.poisoned {
            let _ = self.request("echo on");
        }
    }
}

pub struct SerialWire {
    port: Box<dyn serialport::SerialPort>,
    partial: Vec<u8>,
}
impl SerialWire {
    pub fn open(port: &str) -> io::Result<Self> {
        let mut port = serialport::new(port, 921600)
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::One)
            .flow_control(serialport::FlowControl::None)
            .timeout(Duration::from_millis(5))
            .dtr_on_open(false)
            .open()
            .map_err(io::Error::other)?;
        // Builder's Windows DTR operation is best-effort; check explicitly too.
        port.write_data_terminal_ready(false)
            .map_err(io::Error::other)?;
        port.write_request_to_send(false)
            .map_err(io::Error::other)?;
        Ok(Self {
            port,
            partial: Vec::with_capacity(LIMIT),
        })
    }
}
fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "device transaction deadline"))
}
impl Wire for SerialWire {
    fn send(&mut self, mut bytes: &[u8], deadline: Instant) -> io::Result<()> {
        while !bytes.is_empty() {
            remaining(deadline)?;
            match self.port.write(bytes) {
                Ok(0) => return Err(io::Error::new(io::ErrorKind::WriteZero, "serial write")),
                Ok(n) => bytes = &bytes[n..],
                Err(e) if e.kind() == io::ErrorKind::Interrupted => (),
                Err(e) => return Err(e),
            }
        }
        // Serial flush can wait indefinitely on some OS drivers. ACK is the
        // completion signal; every read/write here has a configured timeout.
        Ok(())
    }
    fn line(&mut self, deadline: Instant) -> io::Result<Vec<u8>> {
        loop {
            remaining(deadline)?;
            if let Some(i) = self.partial.iter().position(|b| *b == b'\n') {
                return Ok(self.partial.drain(..=i).collect());
            }
            if self.partial.len() == LIMIT {
                return Err(io::Error::other("serial line too long"));
            }
            let mut bytes = [0; 512];
            let count = (self.port.bytes_to_read().map_err(io::Error::other)? as usize)
                .max(1)
                .min(bytes.len())
                .min(LIMIT - self.partial.len());
            match self.port.read(&mut bytes[..count]) {
                Ok(n) => self.partial.extend_from_slice(&bytes[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e),
            }
        }
    }
}

pub struct DesktopWire {
    child: Child,
    input: ChildStdin,
    lines: Receiver<io::Result<Vec<u8>>>,
}
impl DesktopWire {
    pub fn open(executable: &Path) -> io::Result<Self> {
        let mut child = Command::new(executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("child stdin missing"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("child stdout missing"))?;
        let (tx, lines) = mpsc::sync_channel(16);
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::with_capacity(LIMIT);
                let result = reader
                    .by_ref()
                    .take((LIMIT + 1) as u64)
                    .read_until(b'\n', &mut line)
                    .and_then(|n| {
                        if n == 0 {
                            Err(io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "desktop stopped",
                            ))
                        } else {
                            decode_line(&line).map(|_| line.clone())
                        }
                    });
                let failed = result.is_err();
                if tx.send(result).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            lines,
        })
    }
}
impl Wire for DesktopWire {
    fn send(&mut self, bytes: &[u8], deadline: Instant) -> io::Result<()> {
        remaining(deadline)?;
        // Developer-only child process: one <=257-byte command fits its pipe.
        self.input.write_all(bytes)?;
        self.input.flush()
    }
    fn line(&mut self, deadline: Instant) -> io::Result<Vec<u8>> {
        self.lines
            .recv_timeout(remaining(deadline)?)
            .map_err(|e| io::Error::new(io::ErrorKind::TimedOut, e))?
    }
}
impl Drop for DesktopWire {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, collections::VecDeque, rc::Rc};
    struct Fake {
        lines: VecDeque<io::Result<Vec<u8>>>,
        writes: Rc<Cell<usize>>,
    }
    impl Wire for Fake {
        fn send(&mut self, _: &[u8], _: Instant) -> io::Result<()> {
            self.writes.set(self.writes.get() + 1);
            Ok(())
        }
        fn line(&mut self, _: Instant) -> io::Result<Vec<u8>> {
            self.lines.pop_front().unwrap()
        }
    }
    #[test]
    fn timeout_poisoning_forbids_late_reply_and_echo_restore() {
        let writes = Rc::new(Cell::new(0));
        let wire = Fake {
            writes: writes.clone(),
            lines: [
                Ok(b"OK echo=on\n".to_vec()),
                Ok(b"OK echo=off\n".to_vec()),
                Err(io::ErrorKind::TimedOut.into()),
                Ok(b"OK late\n".to_vec()),
            ]
            .into(),
        };
        let mut device = TextDevice::connect(wire).unwrap();
        assert!(device.request("status").is_err());
        assert!(device.request("status").is_err());
        drop(device);
        assert_eq!(writes.get(), 3);
    }
    #[test]
    fn response_framing_rejects_truncation_and_non_ascii() {
        for line in [b"OK x".as_slice(), b"OK \xff\n", b"OK x\nOK y\n"] {
            assert!(decode_line(line).is_err());
        }
        assert!(decode_line(&vec![b'x'; LIMIT + 1]).is_err());
        assert_eq!(decode_line(b"OK echo=off\r\n").unwrap(), "OK echo=off");
    }
    #[test]
    fn startup_noise_is_bounded_and_never_relaxes_active_ack_validation() {
        let writes = Rc::new(Cell::new(0));
        let wire = Fake {
            writes: writes.clone(),
            lines: [
                Ok(b"\xff\x80rom\n".to_vec()),
                Ok(b"OK echo=off\n".to_vec()),
                Ok(b"\xff\x80reset\n".to_vec()),
            ]
            .into(),
        };
        let mut device = TextDevice::connect(wire).unwrap();
        assert!(device.request("source status").is_err());
        assert!(device.request("source status").is_err());
        assert_eq!(writes.get(), 2);
        let wire = Fake {
            writes,
            lines: [Ok(b"OK echo=\xff\n".to_vec())].into(),
        };
        assert!(TextDevice::connect(wire).is_err());
    }
}
