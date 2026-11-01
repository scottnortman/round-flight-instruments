#![no_std]
#![forbid(unsafe_code)]

// Register protocol derived from QST QMI8658C Rev A, sections 5 and 13.
// See docs/IMU.md for source provenance and the deliberately fixed configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Bus(i32),
    Identity(u8),
    Verify {
        register: u8,
        expected: u8,
        actual: u8,
    },
    Timeout,
    NotLocked,
    Disabled,
    CounterRegression,
    AuditMismatch,
}

/// Adapters MUST bound each transaction; only the sensor worker may call these.
pub trait Bus {
    fn read(&mut self, address: u8, register: u8, bytes: &mut [u8]) -> Result<(), Error>;
    fn write(&mut self, address: u8, register: u8, value: u8) -> Result<(), Error>;
    fn delay_us(&mut self, us: u32);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    pub address: u8,
    pub who: u8,
    pub revision: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub counter: u32,
    pub raw_accel: [i16; 3],
    pub raw_gyro: [i16; 3],
    pub raw_temperature: i16,
}

#[derive(Clone, Copy, Debug)]
pub struct SelfTest {
    pub accel_g: [f32; 3],
    pub gyro_dps: [f32; 3],
}
/// Diagnostic evidence only, never a calibration or gauge input.
#[derive(Clone, Copy, Debug)]
pub struct Audit {
    /// CTRL1, CTRL2, CTRL3, CTRL5, CTRL7 readback, in that order.
    pub configuration: [u8; 5],
    pub bytewise_gyro_y: i16,
    pub burst_gyro_y: i16,
    pub data_match: bool,
}
impl Audit {
    pub fn configuration_matches(self) -> bool {
        self.configuration == [0x40, 0x16, 0x46, 0, 0x83]
    }
}
impl SelfTest {
    pub fn passed(self) -> bool {
        self.accel_g.iter().all(|v| v.abs() > 0.2) && self.gyro_dps.iter().all(|v| v.abs() > 300.)
    }
}
impl Sample {
    pub fn decode(bytes: [u8; 17]) -> Self {
        let word = |n| i16::from_le_bytes([bytes[n], bytes[n + 1]]);
        Self {
            counter: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]),
            raw_temperature: word(3),
            raw_accel: [word(5), word(7), word(9)],
            raw_gyro: [word(11), word(13), word(15)],
        }
    }
    pub fn accel_g(self) -> [f32; 3] {
        self.raw_accel.map(|v| v as f32 / 8192.0)
    }
    pub fn gyro_dps(self) -> [f32; 3] {
        self.raw_gyro.map(|v| v as f32 / 128.0)
    }
    pub fn temperature_c(self) -> f32 {
        self.raw_temperature as f32 / 256.0
    }
    pub fn saturated(self) -> bool {
        self.raw_accel
            .iter()
            .chain(self.raw_gyro.iter())
            .any(|&v| v == i16::MIN || v == i16::MAX)
    }
}

pub struct Driver<B> {
    bus: B,
    identity: Option<Identity>,
    enabled: bool,
}
impl<B: Bus> Driver<B> {
    pub fn new(bus: B) -> Self {
        Self {
            bus,
            identity: None,
            enabled: false,
        }
    }
    pub fn identity(&self) -> Option<Identity> {
        self.identity
    }
    /// Borrow the bus between complete sensor operations. The single bus owner
    /// may service other device addresses; it must not alter this IMU's state.
    pub fn bus_mut(&mut self) -> &mut B {
        &mut self.bus
    }
    fn read_byte(&mut self, reg: u8) -> Result<u8, Error> {
        let mut b = [0];
        self.bus
            .read(self.identity.ok_or(Error::Disabled)?.address, reg, &mut b)?;
        Ok(b[0])
    }
    fn write(&mut self, reg: u8, value: u8) -> Result<(), Error> {
        self.bus
            .write(self.identity.ok_or(Error::Disabled)?.address, reg, value)
    }
    fn verify(&mut self, reg: u8, expected: u8) -> Result<(), Error> {
        let actual = self.read_byte(reg)?;
        if actual == expected {
            Ok(())
        } else {
            Err(Error::Verify {
                register: reg,
                expected,
                actual,
            })
        }
    }
    fn command(&mut self, cmd: u8) -> Result<(), Error> {
        self.write(0x0a, cmd)?;
        self.wait_command(true)?;
        self.write(0x0a, 0)?;
        self.wait_command(false)
    }
    fn wait_command(&mut self, done: bool) -> Result<(), Error> {
        // A maximum of 50 bounded transactions/delays, never an infinite spin.
        for _ in 0..50 {
            if (self.read_byte(0x2d)? & 0x80 != 0) == done {
                return Ok(());
            }
            self.bus.delay_us(1000);
        }
        Err(Error::Timeout)
    }
    pub fn enable(&mut self) -> Result<Identity, Error> {
        self.enabled = false;
        self.identity = None;
        let mut last_error = Error::Disabled;
        for address in [0x6b, 0x6a] {
            let mut who = [0];
            match self.bus.read(address, 0, &mut who) {
                Ok(()) => {
                    if who[0] != 5 {
                        return Err(Error::Identity(who[0]));
                    }
                    self.identity = Some(Identity {
                        address,
                        who: 5,
                        revision: 0,
                    });
                    break;
                }
                Err(e) => last_error = e,
            }
        }
        if self.identity.is_none() {
            return Err(last_error);
        }
        let result = self.configure();
        if result.is_err() {
            // Best effort only: a broken bus cannot confirm physical shutdown.
            let _ = self.write(0x08, 0);
        }
        result
    }
    fn configure(&mut self) -> Result<Identity, Error> {
        self.write(0x60, 0xb0)?;
        self.bus.delay_us(20_000);
        self.verify(0, 5)?;
        self.verify(0x4d, 0x80)?;
        let revision = self.read_byte(1)?;
        self.identity.as_mut().ok_or(Error::Disabled)?.revision = revision;
        self.write(0x08, 0)?;
        for (reg, value) in [(0x02, 0x40), (0x03, 0x16), (0x04, 0x46), (0x06, 0x00)] {
            self.write(reg, value)?;
            self.verify(reg, value)?;
        }
        // 4g, 256 dps, 112.1 Hz 6DOF; no extra LPF. Little endian + increment.
        // QST requires disabling AHB gating for coherent locked I2C reads.
        self.write(0x0b, 1)?;
        self.command(0x12)?;
        self.write(0x08, 0x83)?;
        self.verify(0x08, 0x83)?;
        self.bus.delay_us(250_000); // Allow gyro start-up before exposing samples.
        self.enabled = true;
        self.identity.ok_or(Error::Disabled)
    }
    pub fn disable(&mut self) -> Result<(), Error> {
        self.enabled = false;
        if self.identity.is_none() {
            return Ok(());
        }
        // Release a potentially locked failed read before disabling acquisition.
        let _ = self.read_byte(0x40);
        self.write(0x08, 0)?;
        self.verify(0x08, 0)?;
        self.write(0x0b, 0)?;
        self.command(0x12)
    }
    pub fn sample(&mut self) -> Result<Option<Sample>, Error> {
        if !self.enabled {
            return Err(Error::Disabled);
        }
        let result = self.sample_inner();
        if result.is_err() {
            self.enabled = false;
            let _ = self.read_byte(0x40); // Release lock even after a partial transfer.
            let _ = self.write(0x08, 0);
        }
        result
    }
    /// Compare individually addressed data bytes to a coherent burst under the
    /// same lock. GZ_H must be read LAST: it releases that lock.
    pub fn audit(&mut self) -> Result<Audit, Error> {
        if !self.enabled {
            return Err(Error::Disabled);
        }
        let result = self.audit_inner();
        if result.is_err() {
            self.enabled = false;
            let _ = self.read_byte(0x40);
            let _ = self.write(8, 0);
        }
        result
    }
    fn audit_inner(&mut self) -> Result<Audit, Error> {
        let mut configuration = [0; 5];
        for (i, reg) in [2, 3, 4, 6, 8].into_iter().enumerate() {
            configuration[i] = self.read_byte(reg)?;
        }
        let mut ready = false;
        for _ in 0..10 {
            let status = self.read_byte(0x2d)?;
            if status & 1 != 0 {
                if status & 2 == 0 {
                    self.bus.delay_us(20);
                    if self.read_byte(0x2d)? & 2 == 0 {
                        return Err(Error::NotLocked);
                    }
                }
                ready = true;
                break;
            }
            self.bus.delay_us(10_000);
        }
        if !ready {
            return Err(Error::Timeout);
        }
        let mut individual = [0; 11];
        for (i, byte) in individual.iter_mut().enumerate() {
            *byte = self.read_byte(0x35 + i as u8)?;
        }
        let mut burst = [0; 17];
        self.bus.read(
            self.identity.ok_or(Error::Disabled)?.address,
            0x30,
            &mut burst,
        )?;
        Ok(Audit {
            configuration,
            bytewise_gyro_y: i16::from_le_bytes([individual[8], individual[9]]),
            burst_gyro_y: Sample::decode(burst).raw_gyro[1],
            data_match: individual == burst[5..16],
        })
    }
    /// QST section 15 electrostatic check-alive; not bias calibration.
    /// Leaves acquisition disabled. The caller must explicitly enable again.
    pub fn self_test(&mut self) -> Result<SelfTest, Error> {
        self.enable()?;
        self.disable()?;
        let result = self.self_test_inner();
        self.enabled = false;
        let clear_accel = self.write(3, 0x03);
        let clear_gyro = self.write(4, 0x46);
        let disable = self.write(8, 0).and_then(|_| self.verify(8, 0));
        result.and_then(|test| {
            clear_accel?;
            clear_gyro?;
            disable?;
            Ok(test)
        })
    }
    fn wait_ready(&mut self, ready: bool) -> Result<(), Error> {
        for _ in 0..200 {
            if (self.read_byte(0x2d)? & 1 != 0) == ready {
                return Ok(());
            }
            self.bus.delay_us(10_000);
        }
        Err(Error::Timeout)
    }
    fn self_test_axes(&mut self, reg: u8, value: u8, scale: f32) -> Result<[f32; 3], Error> {
        self.write(reg, value | 0x80)?;
        self.wait_ready(true)?;
        self.write(reg, value)?;
        self.wait_ready(false)?;
        let mut bytes = [0; 6];
        self.bus.read(
            self.identity.ok_or(Error::Disabled)?.address,
            0x51,
            &mut bytes,
        )?;
        Ok(core::array::from_fn(|i| {
            i16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]]) as f32 / scale
        }))
    }
    fn self_test_inner(&mut self) -> Result<SelfTest, Error> {
        let accel_g = self.self_test_axes(3, 0x03, 2048.)?;
        let gyro_dps = self.self_test_axes(4, 0x46, 16.)?;
        Ok(SelfTest { accel_g, gyro_dps })
    }
    fn sample_inner(&mut self) -> Result<Option<Sample>, Error> {
        let status = self.read_byte(0x2d)?;
        if status & 1 == 0 {
            return Ok(None);
        }
        if status & 2 == 0 {
            self.bus.delay_us(20); // QST table 40: >=12 us for ODR setting 6.
            if self.read_byte(0x2d)? & 2 == 0 {
                return Err(Error::NotLocked);
            }
        }
        let mut bytes = [0; 17];
        self.bus.read(
            self.identity.ok_or(Error::Disabled)?.address,
            0x30,
            &mut bytes,
        )?;
        // Burst includes GZ_H, automatically releasing the sensor's lock.
        Ok(Some(Sample::decode(bytes)))
    }
}

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        regs: [u8; 128],
        writes: std::vec::Vec<(u8, u8)>,
        stuck: bool,
        fail_burst: bool,
        mismatch: bool,
    }
    impl Fake {
        fn new() -> Self {
            let mut regs = [0; 128];
            regs[0] = 5;
            regs[1] = 0x7c;
            regs[0x4d] = 0x80;
            Self {
                regs,
                writes: std::vec::Vec::new(),
                stuck: false,
                fail_burst: false,
                mismatch: false,
            }
        }
    }
    impl Bus for Fake {
        fn read(&mut self, _: u8, reg: u8, bytes: &mut [u8]) -> Result<(), Error> {
            if self.fail_burst && reg == 0x30 {
                return Err(Error::Bus(9));
            }
            bytes.copy_from_slice(&self.regs[reg as usize..reg as usize + bytes.len()]);
            if self.mismatch && reg == 3 {
                bytes[0] ^= 1;
            }
            Ok(())
        }
        fn write(&mut self, _: u8, reg: u8, value: u8) -> Result<(), Error> {
            self.writes.push((reg, value));
            self.regs[reg as usize] = value;
            if reg == 3 || reg == 4 {
                self.regs[0x2d] = if value & 0x80 != 0 { 1 } else { 0 };
                if value & 0x80 != 0 {
                    let response: i16 = if reg == 3 { 1024 } else { 6400 };
                    for axis in 0..3 {
                        self.regs[0x51 + 2 * axis..0x53 + 2 * axis]
                            .copy_from_slice(&response.to_le_bytes());
                    }
                }
            }
            if reg == 0x0a && !self.stuck {
                self.regs[0x2d] = if value == 0 { 0 } else { 0x80 };
            }
            Ok(())
        }
        fn delay_us(&mut self, _: u32) {}
    }
    #[test]
    fn signed_conversion_and_fixed_scales() {
        let mut b = [0; 17];
        b[0..3].copy_from_slice(&[0xff, 0xff, 0xff]);
        for (offset, value) in [
            (3, -256i16),
            (5, 8192),
            (7, -8192),
            (9, 0),
            (11, 128),
            (13, -128),
            (15, i16::MIN),
        ] {
            b[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        let s = Sample::decode(b);
        assert_eq!(s.counter, 0xffffff);
        assert_eq!(s.accel_g(), [1., -1., 0.]);
        assert_eq!(s.gyro_dps(), [1., -1., -256.]);
        assert_eq!(s.temperature_c(), -1.);
        assert!(s.saturated());
    }
    #[test]
    fn identity_and_readback_fail_closed() {
        let mut f = Fake::new();
        f.regs[0] = 4;
        let mut d = Driver::new(f);
        assert_eq!(d.enable(), Err(Error::Identity(4)));
        assert!(d.bus.writes.is_empty());
        let mut f = Fake::new();
        f.mismatch = true;
        let mut d = Driver::new(f);
        assert!(matches!(d.enable(), Err(Error::Verify { register: 3, .. })));
        assert_eq!(d.sample(), Err(Error::Disabled));
    }
    #[test]
    fn command_timeout_is_bounded_and_not_enabled() {
        let mut f = Fake::new();
        f.stuck = true;
        let mut d = Driver::new(f);
        assert_eq!(d.enable(), Err(Error::Timeout));
        assert_eq!(d.sample(), Err(Error::Disabled));
    }
    #[test]
    fn locking_ready_and_bus_failure_recovery() {
        let mut d = Driver::new(Fake::new());
        d.enable().unwrap();
        assert_eq!(d.sample(), Ok(None));
        d.bus.regs[0x2d] = 3;
        assert!(d.sample().unwrap().is_some());
        d.bus.fail_burst = true;
        assert_eq!(d.sample(), Err(Error::Bus(9)));
        assert_eq!(d.sample(), Err(Error::Disabled));
        assert_eq!(d.bus.regs[8], 0);
        d.bus.fail_burst = false;
        d.enable().unwrap();
        d.disable().unwrap();
        assert_eq!(d.sample(), Err(Error::Disabled));
    }
    #[test]
    fn unlocked_data_is_never_published() {
        let mut d = Driver::new(Fake::new());
        d.enable().unwrap();
        d.bus.regs[0x2d] = 1;
        assert_eq!(d.sample(), Err(Error::NotLocked));
        assert_eq!(d.sample(), Err(Error::Disabled));
    }
    #[test]
    fn self_test_handshake_scales_and_shutdown() {
        let mut d = Driver::new(Fake::new());
        let result = d.self_test().unwrap();
        assert_eq!(result.accel_g, [0.5; 3]);
        assert_eq!(result.gyro_dps, [400.; 3]);
        assert!(result.passed());
        assert_eq!(d.sample(), Err(Error::Disabled));
        assert_eq!(d.bus.regs[8], 0);
        assert!(!SelfTest {
            accel_g: [0.2; 3],
            gyro_dps: [400.; 3]
        }
        .passed());
        assert!(!SelfTest {
            accel_g: [0.5; 3],
            gyro_dps: [300.; 3]
        }
        .passed());
    }
    #[test]
    fn audit_compares_signed_independent_reads_and_readback() {
        let mut d = Driver::new(Fake::new());
        d.enable().unwrap();
        d.bus.regs[0x2d] = 3;
        d.bus.regs[0x3d..0x3f].copy_from_slice(&(-3865i16).to_le_bytes());
        let a = d.audit().unwrap();
        assert!(a.data_match && a.configuration_matches());
        assert_eq!(a.bytewise_gyro_y, -3865);
        assert_eq!(a.burst_gyro_y, -3865);
        d.bus.regs[4] = 0x56;
        assert!(!d.audit().unwrap().configuration_matches());
        d.bus.fail_burst = true;
        assert_eq!(d.audit().unwrap_err(), Error::Bus(9));
        assert_eq!(d.sample(), Err(Error::Disabled));
        assert_eq!(d.bus.regs[8], 0);
    }
}
