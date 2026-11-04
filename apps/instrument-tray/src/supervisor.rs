//! Windows presentation supervisor. The connector owns all simulator/USB work.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, BufRead, BufReader, Read},
    os::windows::{fs::MetadataExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub version: u32,
    pub dll: Option<PathBuf>,
    #[serde(default = "bundled_default")]
    pub use_bundled_runtime: bool,
    pub writeback: bool,
    pub connect: bool,
}
fn bundled_default() -> bool { true }
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            dll: None,
            use_bundled_runtime: true,
            writeback: true,
            connect: true,
        }
    }
}
#[derive(Default)]
struct Message {
    text: String,
    simulator: Option<String>,
    at: Option<Instant>,
}
/// Retain the seven latest completed process sessions before starting an eighth.
/// Only direct, numeric session folders under our canonical app log root qualify.
fn prune_logs(root: &Path) -> io::Result<()> {
    fs::create_dir_all(root)?;
    if fs::symlink_metadata(root)?.file_attributes() & 0x400 != 0 {
        return Err(io::Error::other("Log root must not be a reparse point"));
    }
    let root = root.canonicalize()?;
    let mut sessions = Vec::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = name
            .strip_prefix("session-")
            .and_then(|s| s.parse::<u64>().ok())
        else {
            continue;
        };
        let metadata = fs::symlink_metadata(entry.path())?;
        if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
            continue;
        }
        sessions.push((id, entry.path()));
    }
    sessions.sort_by_key(|(id, _)| *id);
    for (_, path) in sessions.iter().take(sessions.len().saturating_sub(7)) {
        let resolved = path.canonicalize()?;
        if resolved.parent() != Some(root.as_path()) {
            return Err(io::Error::other("Log cleanup escaped app directory"));
        }
        fs::remove_dir_all(resolved)?;
    }
    Ok(())
}
fn simulator_identity(line: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(line.strip_prefix("SIMULATOR ")?.trim()).ok()?;
    let name = v["name"].as_str()?;
    if name.len() > 1024 || name.chars().any(char::is_control) { return None; }
    let version = |key: &str| -> Option<String> {
        let fields = v[key].as_array()?;
        if fields.len() != 4 { return None; }
        fields.iter().map(|n| n.as_u64().filter(|n| *n <= u32::MAX as u64).map(|n|n.to_string()))
            .collect::<Option<Vec<_>>>().map(|v|v.join("."))
    };
    Some(format!("{} {} (SimConnect {})",name,version("application_version")?,version("simconnect_version")?))
}
pub struct Supervisor {
    pub root: PathBuf,
    pub bin: PathBuf,
    pub settings: Settings,
    child: Option<Child>,
    stop_at: Option<Instant>,
    retry: Instant,
    failures: u32,
    message: Arc<Mutex<Message>>,
}
impl Supervisor {
    pub fn open() -> io::Result<Self> {
        let root = PathBuf::from(
            std::env::var_os("LOCALAPPDATA")
                .ok_or_else(|| io::Error::other("LOCALAPPDATA unavailable"))?,
        )
        .join("RoundFlightInstruments");
        fs::create_dir_all(&root)?;
        let bin = std::env::current_exe()?
            .parent()
            .ok_or_else(|| io::Error::other("application directory unavailable"))?
            .to_owned();
        let settings_path = root.join("settings.json");
        let mut settings: Settings = if settings_path.exists() {
            let mut bytes = Vec::new();
            fs::File::open(settings_path)?
                .take(8193)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 8192 {
                return Err(io::Error::other("settings too large"));
            }
            serde_json::from_slice(&bytes)?
        } else {
            Settings::default()
        };
        if settings.version != 1 {
            return Err(io::Error::other("unsupported settings version"));
        }
        if settings.use_bundled_runtime && bin.join("SimConnect.dll").is_file() {
            settings.dll = Some(bin.join("SimConnect.dll"));
        } else if settings.dll.as_ref().is_none_or(|p| !p.is_file()) {
            settings.dll = [
                bin.join("SimConnect.dll"),
                PathBuf::from(r"C:\MSFS SDK\SimConnect SDK\lib\SimConnect.dll"),
            ]
            .into_iter()
            .find(|p| p.is_file());
        }
        Ok(Self {
            root,
            bin,
            settings,
            child: None,
            stop_at: None,
            retry: Instant::now(),
            failures: 0,
            message: Arc::default(),
        })
    }
    pub fn save(&self) -> io::Result<()> {
        let temp = self.root.join("settings.tmp");
        fs::write(&temp, serde_json::to_vec_pretty(&self.settings)?)?;
        fs::rename(temp, self.root.join("settings.json"))
    }
    fn note(&self, text: impl Into<String>) {
        if let Ok(mut m) = self.message.lock() {
            m.text = text.into();
            m.at = Some(Instant::now());
        }
    }
    pub fn status(&self) -> String {
        let m = self.message.lock().unwrap();
        format!("Round Instruments 0.2.0-rc.7 prerelease\r\n\r\n{}\r\nUpdated {} seconds ago\r\n\r\nLast connected simulator: {}\r\n\r\nControls: {}\r\nDevice: {}\r\n\r\nBackground gauges are read-only. Only explicit active-gauge actions can change MSFS.\r\nAirspeed, heading and VSI faces remain unavailable.\r\n\r\nLogs: {}",m.text,m.at.map_or(0,|at|at.elapsed().as_secs()),m.simulator.as_deref().unwrap_or("Not reported yet; start MSFS and connect"),if self.settings.writeback {"Active gauge actions enabled"}else{"Read-only"},if self.root.join("device.json").is_file(){"Enrolled by USB identity"}else{"Select a device from the tray menu"},self.root.join("logs").display())
    }
    pub fn devices() -> io::Result<Vec<(String, String)>> {
        Ok(serialport::available_ports()
            .map_err(io::Error::other)?
            .into_iter()
            .filter_map(|p| match p.port_type {
                serialport::SerialPortType::UsbPort(u) if u.vid == 0x1a86 && u.pid == 0x55d3 => {
                    u.serial_number.map(|id| (p.port_name, id))
                }
                _ => None,
            })
            .collect())
    }
    pub fn enroll(&mut self, port: &str) -> io::Result<()> {
        if self.child.is_some() {
            return Err(io::Error::other(
                "Pause connection first, then choose the device.",
            ));
        }
        let matches: Vec<_> = Self::devices()?
            .into_iter()
            .filter(|(p, _)| p == port)
            .collect();
        if matches.len() != 1 {
            return Err(io::Error::other("Device unavailable or ambiguous"));
        }
        let config = serde_json::json!({"version":1,"profile":"altimeter-v1","usb_vid":0x1a86,"usb_pid":0x55d3,"usb_serial":matches[0].1});
        let temp = self.root.join("device.tmp");
        fs::write(&temp, serde_json::to_vec_pretty(&config)?)?;
        fs::rename(temp, self.root.join("device.json"))?;
        self.retry = Instant::now();
        self.note("Device selected. Waiting for simulator.");
        Ok(())
    }
    pub fn restart(&mut self) {
        self.stop();
        self.retry = Instant::now();
        self.failures = 0;
    }
    pub fn stop(&mut self) {
        if self.child.is_some() && self.stop_at.is_none() {
            match fs::write(self.root.join("stop.request"), b"stop") {
                Ok(()) => self.stop_at = Some(Instant::now()),
                Err(e) => {
                    self.note(format!("Cannot request clean stop: {e}"));
                    self.stop_at = Some(Instant::now() - Duration::from_secs(8));
                }
            }
        }
    }
    pub fn tick(&mut self) {
        if let Some(child) = self.child.as_mut() {
            if self
                .stop_at
                .is_some_and(|t| t.elapsed() > Duration::from_secs(7))
            {
                let _ = child.kill();
            }
            match child.try_wait() {
                Ok(Some(code)) => {
                    let requested = self.stop_at.take().is_some();
                    self.child = None;
                    self.failures = self.failures.saturating_add(1);
                    self.retry = Instant::now()
                        + Duration::from_secs(if requested {
                            0
                        } else {
                            1u64 << self.failures.min(3)
                        });
                    if requested {
                        self.note("Connection stopped.");
                    } else {
                        self.note(format!("Connector ended ({code}); automatic retry in a few seconds. Open logs for details."));
                    }
                }
                Ok(None) => return,
                Err(e) => {
                    self.note(format!("Connector status error: {e}"));
                    self.stop();
                    return;
                }
            }
        }
        if !self.settings.connect || Instant::now() < self.retry {
            return;
        }
        if !self.root.join("device.json").is_file() {
            self.note("Select your Waveshare UART device from the tray menu.");
            self.retry = Instant::now() + Duration::from_secs(5);
            return;
        }
        let Some(dll) = self.settings.dll.as_ref().filter(|p| p.is_file()) else {
            self.note("Choose SimConnect.dll from your working MSFS test package or SDK.");
            self.retry = Instant::now() + Duration::from_secs(5);
            return;
        };
        let stop = self.root.join("stop.request");
        if stop.exists() {
            if let Err(e) = fs::remove_file(&stop) {
                self.note(format!("Cannot clear stop request: {e}"));
                return;
            }
        }
        let mut cmd = Command::new(self.bin.join("instrument-connector.exe"));
        if let Err(e) = prune_logs(&self.root.join("logs")) {
            self.note(format!("Cannot maintain bounded logs: {e}"));
            self.retry = Instant::now() + Duration::from_secs(8);
            return;
        }
        cmd.args(["run", "--sixpack", "--app-status", "--config"])
            .arg(self.root.join("device.json"))
            .args(["--msfs", "--dll"])
            .arg(dll)
            .arg("--log-dir")
            .arg(self.root.join("logs"))
            .arg("--stop-file")
            .arg(stop)
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if self.settings.writeback {
            cmd.arg("--writeback");
        }
        match cmd.spawn() {
            Ok(mut child) => {
                self.note("Connecting to MSFS and the enrolled device...");
                self.capture(child.stdout.take().unwrap());
                self.capture(child.stderr.take().unwrap());
                self.child = Some(child);
            }
            Err(e) => {
                self.note(format!("Cannot start connector: {e}"));
                self.retry = Instant::now() + Duration::from_secs(8);
            }
        }
    }
    fn capture(&self, input: impl io::Read + Send + 'static) {
        let message = self.message.clone();
        let root = self.root.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(input);
            loop {
                let mut line = String::new();
                // Bound malformed child output even if it never includes a newline.
                match reader.by_ref().take(8193).read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) if line.len() > 8192 => break,
                    _ => {}
                }
                if let Some(info) = simulator_identity(&line) {
                    if let Ok(mut m) = message.lock() { m.simulator = Some(info); }
                    continue;
                }
                let text = if let Some(json) = line.strip_prefix("STATUS ") {
                    match serde_json::from_str::<serde_json::Value>(json) {
                        Ok(v) => format!(
                            "MSFS: {}\r\nDisplayed gauge: {}\r\nReceived frames: {}",
                            v["state"].as_str().unwrap_or("unknown"),
                            v["instrument"].as_str().unwrap_or("unknown"),
                            v["frames"]
                        ),
                        Err(_) => "Malformed connector status".into(),
                    }
                } else if line.trim().is_empty() || line.trim_start().starts_with(['{', '}', '"']) {
                    continue;
                } else {
                    line.trim().to_owned()
                };
                if let Ok(mut m) = message.lock() {
                    m.text = text.clone();
                    m.at = Some(Instant::now());
                }
                // One bounded latest-message file; detailed wire logs rotate in the connector.
                let _ = fs::write(root.join("last-message.txt"), text);
            }
        });
    }
    pub fn choose_dll(&mut self, path: PathBuf) -> io::Result<()> {
        if !path.is_file()
            || path
                .file_name()
                .is_none_or(|n| !n.to_string_lossy().eq_ignore_ascii_case("SimConnect.dll"))
        {
            return Err(io::Error::other("Select SimConnect.dll"));
        }
        self.settings.dll = Some(path.canonicalize()?);
        self.settings.use_bundled_runtime = false;
        self.save()?;
        self.restart();
        Ok(())
    }
    pub fn export(&self) -> io::Result<PathBuf> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_secs();
        let destination = self.root.join(format!("diagnostics-{stamp}.zip"));
        let status = Command::new("powershell.exe")
            .creation_flags(0x08000000)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(self.bin.join("Export-Diagnostics.ps1"))
            .arg("-DataDirectory")
            .arg(&self.root)
            .arg("-Destination")
            .arg(&destination)
            .status()?;
        if !status.success() {
            return Err(io::Error::other("Diagnostic export failed"));
        }
        Ok(destination)
    }
    pub fn running(&self) -> bool {
        self.child.is_some()
    }
    pub fn wait_stopped(&mut self) {
        self.settings.connect = false;
        self.stop();
        while self.running() {
            self.tick();
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        self.wait_stopped();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reports_server_versions_without_guessing_from_sdk_and_rejects_malformed_identity() {
        assert_eq!(simulator_identity(r#"SIMULATOR {"name":"MSFS fixture","application_version":[1,38,2,0],"simconnect_version":[10,0,0,0]}"#).unwrap(), "MSFS fixture 1.38.2.0 (SimConnect 10.0.0.0)");
        assert!(simulator_identity(r#"SIMULATOR {"name":"bad","application_version":[1],"simconnect_version":[10,0,0,0]}"#).is_none());
        let old: Settings=serde_json::from_str(r#"{"version":1,"dll":null,"connect":true,"writeback":true}"#).unwrap();
        assert!(old.use_bundled_runtime);
    }
    use super::*;
    #[test]
    fn settings_default_to_auto_sync_with_explicit_active_controls() {
        let s = Settings::default();
        assert!(s.connect && s.writeback);
        assert_eq!(s.version, 1);
        assert!(serde_json::from_str::<Settings>(
            r#"{"version":1,"dll":null,"connect":true,"writeback":true,"unknown":1}"#
        )
        .is_err());
    }
    #[test]
    fn exited_child_is_retried_and_pause_stops_restarts() {
        let root = std::env::temp_dir().join(format!(
            "rfi-supervisor-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        // The test harness rejects the connector's CLI arguments and exits.
        // It never opens hardware or loads a simulator runtime.
        fs::copy(
            std::env::current_exe().unwrap(),
            root.join("instrument-connector.exe"),
        )
        .unwrap();
        fs::write(root.join("SimConnect.dll"), b"test placeholder").unwrap();
        fs::write(root.join("device.json"), b"{}").unwrap();
        let mut supervisor = Supervisor {
            root: root.clone(),
            bin: root.clone(),
            settings: Settings {
                dll: Some(root.join("SimConnect.dll")),
                ..Settings::default()
            },
            child: None,
            stop_at: None,
            retry: Instant::now(),
            failures: 0,
            message: Arc::default(),
        };
        supervisor.tick();
        assert!(supervisor.running());
        let deadline = Instant::now() + Duration::from_secs(3);
        while supervisor.running() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
            supervisor.tick();
        }
        assert!(!supervisor.running());
        assert_eq!(supervisor.failures, 1);
        assert!(supervisor.retry > Instant::now());
        supervisor.tick();
        assert!(!supervisor.running());
        supervisor.retry = Instant::now();
        supervisor.tick();
        assert!(supervisor.running());
        supervisor.wait_stopped();
        supervisor.tick();
        assert!(!supervisor.running());
        drop(supervisor);
        assert_eq!(
            root.canonicalize().unwrap().parent(),
            Some(std::env::temp_dir().canonicalize().unwrap().as_path())
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn log_retention_keeps_recent_sessions_and_unrelated_folders() {
        let root = std::env::temp_dir().join(format!(
            "rfi-log-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        for i in 1..=10 {
            fs::create_dir(root.join(format!("session-{i}"))).unwrap();
        }
        fs::create_dir(root.join("user-notes")).unwrap();
        prune_logs(&root).unwrap();
        for i in 1..=10 {
            assert_eq!(root.join(format!("session-{i}")).exists(), i >= 4);
        }
        assert!(root.join("user-notes").exists());
        assert_eq!(
            root.canonicalize().unwrap().parent(),
            Some(std::env::temp_dir().canonicalize().unwrap().as_path())
        );
        fs::remove_dir_all(root).unwrap();
    }
}
