//! Keep the native SDK outside the USB-owning process. One worker per session;
//! no DLL unload/reload cycle in the parent, and no replay of uncertain writes.
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sim_bridge::msfs::{native_notice, Api, Notice};
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    os::windows::process::CommandExt,
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const MAX_LINE: u64 = 4096;
const QUEUE: usize = 128;
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Read { id: u32, heartbeat: bool },
    Baro { pressure16: u32 },
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Message {
    Ready,
    Event {
        event: u32,
        request: u32,
        code: u32,
        detail: u32,
        bits: [u64; 4],
    },
    Error {
        retry: bool,
        message: String,
    },
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn write_message(w: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    if bytes.len() + 1 > MAX_LINE as usize {
        return Err(invalid("worker message too long"));
    }
    bytes.push(b'\n');
    w.write_all(&bytes)?;
    w.flush()
}
struct Inbox<T> {
    rx: Receiver<io::Result<T>>,
    overflow: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}
fn inbox<T: DeserializeOwned + Send + 'static>(input: impl Read + Send + 'static) -> Inbox<T> {
    let (tx, rx) = mpsc::sync_channel(QUEUE);
    let overflow = Arc::new(AtomicBool::new(false));
    let full = overflow.clone();
    let reader = thread::spawn(move || {
        let mut input = BufReader::new(input);
        loop {
            let mut bytes = Vec::new();
            let result = match input
                .by_ref()
                .take(MAX_LINE + 1)
                .read_until(b'\n', &mut bytes)
            {
                Ok(0) => break,
                Ok(_) if bytes.len() as u64 > MAX_LINE || bytes.last() != Some(&b'\n') => {
                    Err(invalid("oversized or truncated worker message"))
                }
                Ok(_) => serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string())),
                Err(e) => Err(e),
            };
            let failed = result.is_err();
            if tx.try_send(result).is_err() {
                full.store(true, Ordering::Release);
                break;
            }
            if failed {
                break;
            }
        }
    });
    Inbox {
        rx,
        overflow,
        reader: Some(reader),
    }
}

pub struct Connection {
    child: Child,
    input: Option<ChildStdin>,
    messages: Inbox<Message>,
}
impl Connection {
    pub fn open(dll: &Path) -> io::Result<Self> {
        // Path/configuration errors are fatal; unavailable simulator and worker
        // crashes retry with the runner's bounded backoff.
        let dll = dll.canonicalize()?;
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("__simconnect-worker").arg(dll);
        Self::spawn(&mut command)
    }
    fn spawn(command: &mut Command) -> io::Result<Self> {
        let mut child = command
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let input = child.stdin.take();
        let messages = inbox(child.stdout.take().expect("piped worker stdout"));
        let mut connection = Self {
            child,
            input,
            messages,
        };
        match connection.messages.rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(Message::Ready)) => Ok(connection),
            Ok(Ok(Message::Error { retry, message })) => Err(io::Error::new(
                if retry {
                    io::ErrorKind::ConnectionRefused
                } else {
                    io::ErrorKind::Other
                },
                message,
            )),
            Ok(Ok(_)) => Err(invalid("worker event before ready")),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                connection.loss_description("worker exited or timed out during startup"),
            )),
        }
    }
    fn loss_description(&mut self, reason: &str) -> String {
        match self.child.try_wait() {
            Ok(Some(status)) => format!(
                "{reason}; worker exit={status} hex={:08X}",
                status.code().unwrap_or(-1) as u32
            ),
            _ => reason.to_string(),
        }
    }
    fn send(&mut self, request: Request) -> io::Result<()> {
        write_message(self.input.as_mut().expect("live worker stdin"), &request).map_err(|e| {
            io::Error::new(
                io::ErrorKind::ConnectionReset,
                format!("worker command failed: {e}"),
            )
        })
    }
}
impl Api for Connection {
    fn request(&mut self, id: u32, heartbeat: bool) -> io::Result<()> {
        self.send(Request::Read { id, heartbeat })
    }
    fn set_baro(&mut self, pressure16: u32) -> io::Result<()> {
        self.send(Request::Baro { pressure16 })
    }
    fn next(&mut self) -> io::Result<Option<Notice>> {
        if self.messages.overflow.load(Ordering::Acquire) {
            return Err(invalid("worker event queue overflow"));
        }
        // Check exit before consuming queued data: never replay readings left
        // behind by a dead worker as if they were freshly acquired.
        if let Some(status) = self.child.try_wait()? {
            return Err(io::Error::new(
                if status.code() == Some(1) {
                    io::ErrorKind::Other
                } else {
                    io::ErrorKind::ConnectionReset
                },
                self.loss_description("SimConnect worker exited"),
            ));
        }
        match self.messages.rx.try_recv() {
            Ok(Ok(Message::Event {
                event,
                request,
                code,
                detail,
                bits,
            })) => Ok(Some(native_notice(simconnect_native::Event {
                kind: event,
                request,
                code,
                detail,
                values: bits.map(f64::from_bits),
            }))),
            Ok(Ok(Message::Error { message, .. })) => Err(io::Error::other(message)),
            Ok(Ok(Message::Ready)) => Err(invalid("duplicate worker ready")),
            Ok(Err(e)) => Err(e),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                self.loss_description("SimConnect worker pipe closed"),
            )),
        }
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.input.take();
        // Do not let a wedged SDK shutdown stall USB servicing or recovery.
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.messages.reader.take() {
            let _ = reader.join();
        }
    }
}

pub fn entry(dll: &Path) -> i32 {
    let mut out = io::stdout().lock();
    if let Err(e) = worker(dll, &mut out) {
        eprintln!("SimConnect worker error: {e}");
        let _ = write_message(
            &mut out,
            &Message::Error {
                retry: e.kind() == io::ErrorKind::ConnectionRefused,
                message: e.to_string(),
            },
        );
        return 1;
    }
    0
}
fn worker(dll: &Path, out: &mut impl Write) -> io::Result<()> {
    let mut native = simconnect_native::Connection::open(dll)?;
    write_message(out, &Message::Ready)?;
    let requests = inbox::<Request>(io::stdin());
    loop {
        if requests.overflow.load(Ordering::Acquire) {
            return Err(invalid("worker command queue overflow"));
        }
        for _ in 0..8 {
            match requests.rx.try_recv() {
                Ok(Ok(Request::Read { id, heartbeat })) => native.request(id, heartbeat)?,
                Ok(Ok(Request::Baro { pressure16 })) => native.set_baro(pressure16)?,
                Ok(Err(e)) => return Err(e),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }
        for _ in 0..64 {
            let Some(e) = native.poll()? else {
                break;
            };
            if e.kind == 0 {
                continue;
            }
            write_message(
                out,
                &Message::Event {
                    event: e.kind,
                    request: e.request,
                    code: e.code,
                    detail: e.detail,
                    bits: e.values.map(f64::to_bits),
                },
            )?;
        }
        thread::sleep(Duration::from_millis(2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    fn fixture(script: &str) -> Command {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
        command
    }
    #[test]
    fn native_crash_is_contained_and_does_not_block_polling() {
        let mut c = Connection::spawn(&mut fixture(
            "[Console]::Out.WriteLine('{\"kind\":\"ready\"}'); [Console]::Out.Flush(); Start-Sleep -Milliseconds 200; exit -1073741819"
        )).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let start = Instant::now();
            let result = c.next();
            assert!(start.elapsed() < Duration::from_millis(100));
            if let Err(e) = result {
                assert_eq!(e.kind(), io::ErrorKind::ConnectionReset);
                assert!(c.child.wait().unwrap().code() == Some(-1073741819));
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn startup_crash_retries_and_hung_worker_is_reaped() {
        let error = Connection::spawn(&mut fixture("exit -1073741819"))
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::ConnectionRefused);
        let c = Connection::spawn(&mut fixture(
            "[Console]::Out.WriteLine('{\"kind\":\"ready\"}'); [Console]::Out.Flush(); Start-Sleep -Seconds 60"
        )).unwrap();
        let start = Instant::now();
        drop(c);
        assert!(start.elapsed() < Duration::from_secs(2));
    }
    #[test]
    fn malformed_and_oversize_messages_fail_closed() {
        for line in ["not json\n".to_string(), "x".repeat(MAX_LINE as usize + 1)] {
            let input = inbox::<Message>(io::Cursor::new(line.into_bytes()));
            assert_eq!(
                input
                    .rx
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap()
                    .err()
                    .unwrap()
                    .kind(),
                io::ErrorKind::InvalidData
            );
        }
    }
    #[test]
    fn nonfinite_readings_survive_transport_for_validation() {
        let mut bytes = Vec::new();
        let values = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 29.92];
        write_message(
            &mut bytes,
            &Message::Event {
                event: 5,
                request: 1,
                code: 1,
                detail: 0,
                bits: values.map(f64::to_bits),
            },
        )
        .unwrap();
        let Message::Event { bits, .. } = serde_json::from_slice(&bytes).unwrap() else {
            panic!()
        };
        assert_eq!(bits, values.map(f64::to_bits));
    }
}
