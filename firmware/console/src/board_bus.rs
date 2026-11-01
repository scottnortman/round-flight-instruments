//! SKU 29086's single synchronous I2C master. Handles and peripheral tokens
//! remain private; all transactions require an exclusive borrow of the bus.
use esp_idf_svc::{
    hal::{
        gpio::{Gpio15, Gpio7},
        i2c::I2C0,
    },
    sys::{self, EspError},
};

const ADDRESSES: [u8; 4] = [0x20, 0x5d, 0x6a, 0x6b];
const TRANSACTION_TIMEOUT_MS: i32 = 10;

pub struct Bus {
    handle: sys::i2c_master_bus_handle_t,
    devices: [sys::i2c_master_dev_handle_t; 4],
    _port: I2C0<'static>,
    _sda: Gpio15<'static>,
    _scl: Gpio7<'static>,
}
// SAFETY: ownership moves once from startup to the sole bus worker. No handles
// escape, no async callbacks exist, and methods require &mut self. Not Sync.
unsafe impl Send for Bus {}

impl Bus {
    pub fn new(
        port: I2C0<'static>,
        sda: Gpio15<'static>,
        scl: Gpio7<'static>,
    ) -> Result<Self, EspError> {
        let mut config = sys::i2c_master_bus_config_t {
            i2c_port: 0,
            sda_io_num: 15,
            scl_io_num: 7,
            glitch_ignore_cnt: 7,
            ..Default::default()
        };
        config.__bindgen_anon_1.clk_source = sys::soc_periph_i2c_clk_src_t_I2C_CLK_SRC_DEFAULT;
        config.flags.set_enable_internal_pullup(1);
        let mut handle = core::ptr::null_mut();
        // SAFETY: config and output handle live for this synchronous call.
        sys::esp!(unsafe { sys::i2c_new_master_bus(&config, &mut handle) })?;
        let mut bus = Self {
            handle,
            devices: [core::ptr::null_mut(); 4],
            _port: port,
            _sda: sda,
            _scl: scl,
        };
        for (i, address) in ADDRESSES.into_iter().enumerate() {
            let config = sys::i2c_device_config_t {
                dev_addr_length: sys::i2c_addr_bit_len_t_I2C_ADDR_BIT_LEN_7,
                device_address: address as u16,
                scl_speed_hz: 400_000,
                // IDF's documented default stretch allowance, in microseconds.
                scl_wait_us: 2000,
                ..Default::default()
            };
            // SAFETY: bus owns the live handle; output belongs to this bus.
            // On failure Drop removes all successfully added devices.
            sys::esp!(unsafe {
                sys::i2c_master_bus_add_device(bus.handle, &config, &mut bus.devices[i])
            })?;
        }
        Ok(bus)
    }
    fn device(&self, address: u8) -> Result<sys::i2c_master_dev_handle_t, EspError> {
        ADDRESSES
            .iter()
            .position(|a| *a == address)
            .map(|i| self.devices[i])
            .ok_or_else(|| EspError::from(sys::ESP_ERR_INVALID_ARG).unwrap())
    }
    pub fn write_read(
        &mut self,
        address: u8,
        write: &[u8],
        read: &mut [u8],
    ) -> Result<(), EspError> {
        let device = self.device(address)?;
        // SAFETY: no completion callback is registered, so the call is
        // synchronous and neither buffer is retained beyond its return.
        sys::esp!(unsafe {
            sys::i2c_master_transmit_receive(
                device,
                write.as_ptr(),
                write.len(),
                read.as_mut_ptr(),
                read.len(),
                TRANSACTION_TIMEOUT_MS,
            )
        })
    }
    pub fn write(&mut self, address: u8, data: &[u8]) -> Result<(), EspError> {
        let device = self.device(address)?;
        // SAFETY: same synchronous lifetime guarantee as write_read.
        sys::esp!(unsafe {
            sys::i2c_master_transmit(device, data.as_ptr(), data.len(), TRANSACTION_TIMEOUT_MS)
        })
    }
}
impl Drop for Bus {
    fn drop(&mut self) {
        // SAFETY: exclusive owner, no outstanding asynchronous transactions.
        // If removal fails, retain the bus rather than freeing a live device.
        let mut removed = true;
        for device in self.devices {
            if !device.is_null() && unsafe { sys::i2c_master_bus_rm_device(device) } != 0 {
                removed = false;
            }
        }
        if removed {
            unsafe {
                sys::i2c_del_master_bus(self.handle);
            }
        }
    }
}
