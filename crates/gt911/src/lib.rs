#![no_std]
#![forbid(unsafe_code)]

/// Synchronous, bounded register transactions. Registers are sent big-endian;
/// coordinate and resolution fields returned by the controller are little-endian.
pub trait Bus {
    fn read(&mut self, address: u8, register: u16, bytes: &mut [u8]) -> Result<(), Error>;
    fn write(&mut self, address: u8, register: u16, value: u8) -> Result<(), Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Bus(i32),
    Identity,
    Resolution,
    Malformed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Contact {
    Up,
    Down { id: u8, x: u16, y: u16 },
    Multiple,
}
pub struct Driver {
    address: u8,
    width: u16,
    height: u16,
}
impl Driver {
    pub fn probe(bus: &mut impl Bus, address: u8, width: u16, height: u16) -> Result<Self, Error> {
        let mut id = [0; 3];
        bus.read(address, 0x8140, &mut id)?;
        if &id != b"911" {
            return Err(Error::Identity);
        }
        let mut size = [0; 4];
        bus.read(address, 0x8048, &mut size)?;
        if width == 0
            || height == 0
            || u16::from_le_bytes([size[0], size[1]]) != width
            || u16::from_le_bytes([size[2], size[3]]) != height
        {
            return Err(Error::Resolution);
        }
        Ok(Self {
            address,
            width,
            height,
        })
    }
    /// None means no NEW report, not finger-up. Publish only after the report
    /// has been acknowledged successfully. Multiple contacts are never actions.
    pub fn poll(&self, bus: &mut impl Bus) -> Result<Option<Contact>, Error> {
        let mut status = [0];
        bus.read(self.address, 0x814e, &mut status)?;
        if status[0] & 0x80 == 0 {
            return Ok(None);
        }
        let count = status[0] & 15;
        let contact = match count {
            0 => Ok(Contact::Up),
            1 => {
                let mut p = [0; 8];
                bus.read(self.address, 0x814f, &mut p)?;
                let x = u16::from_le_bytes([p[1], p[2]]);
                let y = u16::from_le_bytes([p[3], p[4]]);
                if x >= self.width || y >= self.height || p[0] > 15 {
                    Err(Error::Malformed)
                } else {
                    Ok(Contact::Down { id: p[0], x, y })
                }
            }
            2..=5 => Ok(Contact::Multiple),
            _ => Err(Error::Malformed),
        };
        bus.write(self.address, 0x814e, 0)?;
        contact.map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        status: u8,
        point: [u8; 8],
        ack: bool,
        fail_ack: bool,
    }
    impl Bus for Fake {
        fn read(&mut self, _: u8, r: u16, out: &mut [u8]) -> Result<(), Error> {
            let data: &[u8] = match r {
                0x8140 => b"911",
                0x8048 => &[224, 1, 224, 1],
                0x814e => core::slice::from_ref(&self.status),
                0x814f => &self.point,
                _ => panic!("register"),
            };
            out.copy_from_slice(data);
            Ok(())
        }
        fn write(&mut self, _: u8, r: u16, v: u8) -> Result<(), Error> {
            assert_eq!((r, v), (0x814e, 0));
            if self.fail_ack {
                return Err(Error::Bus(1));
            }
            self.ack = true;
            Ok(())
        }
    }
    #[test]
    fn reports_coordinates_release_and_no_data_without_fabrication() {
        let mut b = Fake {
            status: 0,
            point: [0, 82, 1, 235, 0, 0, 0, 0],
            ack: false,
            fail_ack: false,
        };
        let d = Driver::probe(&mut b, 0x5d, 480, 480).unwrap();
        assert_eq!(d.poll(&mut b), Ok(None));
        assert!(!b.ack);
        b.status = 0x81;
        assert_eq!(
            d.poll(&mut b),
            Ok(Some(Contact::Down {
                id: 0,
                x: 338,
                y: 235
            }))
        );
        assert!(b.ack);
        b.status = 0x80;
        assert_eq!(d.poll(&mut b), Ok(Some(Contact::Up)));
        b.status = 0x82;
        assert_eq!(d.poll(&mut b), Ok(Some(Contact::Multiple)));
        b.status = 0x86;
        assert_eq!(d.poll(&mut b), Err(Error::Malformed));
        b.status = 0x81;
        b.point[2] = 2;
        assert_eq!(d.poll(&mut b), Err(Error::Malformed));
        b.point[2] = 1;
        b.fail_ack = true;
        assert_eq!(d.poll(&mut b), Err(Error::Bus(1)));
        assert!(matches!(
            Driver::probe(&mut b, 0x5d, 240, 480),
            Err(Error::Resolution)
        ));
    }
}
