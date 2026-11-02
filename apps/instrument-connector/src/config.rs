use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub version: u32,
    pub profile: String,
    pub usb_vid: u16,
    pub usb_pid: u16,
    pub usb_serial: String,
}
#[derive(Debug)]
pub struct Candidate {
    pub port: String,
    pub serial: Option<String>,
}
pub fn candidates() -> io::Result<Vec<Candidate>> {
    Ok(serialport::available_ports()
        .map_err(io::Error::other)?
        .into_iter()
        .filter_map(|p| match p.port_type {
            serialport::SerialPortType::UsbPort(u) if u.vid == 0x1a86 && u.pid == 0x55d3 => {
                Some(Candidate {
                    port: p.port_name,
                    serial: u.serial_number,
                })
            }
            _ => None,
        })
        .collect())
}
impl Configuration {
    fn validate(&self) -> io::Result<()> {
        if self.version != 1
            || self.profile != "altimeter-v1"
            || self.usb_vid != 0x1a86
            || self.usb_pid != 0x55d3
            || self.usb_serial.is_empty()
            || self.usb_serial.len() > 128
            || !self.usb_serial.bytes().all(|b| b.is_ascii_graphic())
        {
            return Err(io::Error::other(
                "unsupported configuration/profile/device identity",
            ));
        }
        Ok(())
    }
    pub fn load(path: &Path) -> io::Result<Self> {
        let mut data = Vec::new();
        File::open(path)?.take(8193).read_to_end(&mut data)?;
        if data.len() > 8192 {
            return Err(io::Error::other("configuration too large"));
        }
        let config: Self = serde_json::from_slice(&data).map_err(io::Error::other)?;
        config.validate()?;
        Ok(config)
    }
    pub fn enroll(port: &str, path: &Path) -> io::Result<()> {
        let choices = candidates()?;
        let matches: Vec<_> = choices
            .iter()
            .filter(|p| p.port.eq_ignore_ascii_case(port))
            .collect();
        if matches.len() != 1 {
            return Err(io::Error::other("select one connected CH343 UART port"));
        }
        let config = Self {
            version: 1,
            profile: "altimeter-v1".into(),
            usb_vid: 0x1a86,
            usb_pid: 0x55d3,
            usb_serial: matches[0]
                .serial
                .clone()
                .ok_or_else(|| io::Error::other("USB serial identity missing"))?,
        };
        config.validate()?;
        let data = serde_json::to_vec_pretty(&config).map_err(io::Error::other)?;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?
            .write_all(&data)
    }
    pub fn resolve(&self, choices: &[Candidate]) -> io::Result<String> {
        self.validate()?;
        let matching: Vec<_> = choices
            .iter()
            .filter(|p| p.serial.as_deref() == Some(&self.usb_serial))
            .collect();
        if matching.len() != 1 {
            return Err(io::Error::other(
                "enrolled device absent or ambiguous; no port opened",
            ));
        }
        Ok(matching[0].port.clone())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn follows_usb_identity_not_port_and_refuses_ambiguity() {
        let config = Configuration {
            version: 1,
            profile: "altimeter-v1".into(),
            usb_vid: 0x1a86,
            usb_pid: 0x55d3,
            usb_serial: "test-board".into(),
        };
        let mut choices = vec![
            Candidate {
                port: "COM9".into(),
                serial: Some("test-board".into()),
            },
            Candidate {
                port: "COM5".into(),
                serial: Some("another-board".into()),
            },
        ];
        assert_eq!(config.resolve(&choices).unwrap(), "COM9");
        choices.push(Candidate {
            port: "COM10".into(),
            serial: Some("test-board".into()),
        });
        assert!(config.resolve(&choices).is_err());
        assert!(config.resolve(&[]).is_err());
        let mut unsupported = config;
        unsupported.profile = "attitude".into();
        assert!(unsupported.resolve(&choices).is_err());
    }
}
