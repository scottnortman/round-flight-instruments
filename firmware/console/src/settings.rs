//! The only flash owner. The state owner only observes/enqueues; never waits for NVS.
use esp_idf_svc::{
    hal::{cpu::Core, task::thread::ThreadSpawnConfiguration},
    sys,
};
use instrument_core::{
    device::{self, Command, Info, SelectionSave, Uuid},
    Instrument,
};
use std::{
    io,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use text_cli::{reply, Response};

const ID_KEY: &[u8] = b"uuid\0";
const GAUGE_KEY: &[u8] = b"selection\0";
const BAD_RECORD: i32 = -1;
struct Store(sys::nvs_handle_t);
impl Store {
    fn open() -> Result<Self, i32> {
        // Never erase on full/new-version initialization failures: preserve identity.
        let init = unsafe { sys::nvs_flash_init() };
        if init != 0 {
            return Err(init);
        }
        let mut handle = 0;
        // SAFETY: static NUL-terminated namespace; exclusively owned handle.
        let result = unsafe {
            sys::nvs_open(
                b"rfi_device\0".as_ptr().cast(),
                sys::nvs_open_mode_t_NVS_READWRITE,
                &mut handle,
            )
        };
        if result != 0 {
            Err(result)
        } else {
            Ok(Self(handle))
        }
    }
    fn read<const N: usize>(&self, key: &[u8]) -> Result<Option<[u8; N]>, i32> {
        let mut bytes = [0; N];
        let mut size = N;
        // SAFETY: bounded, exclusive live buffer and static NUL-terminated key.
        let result = unsafe {
            sys::nvs_get_blob(
                self.0,
                key.as_ptr().cast(),
                bytes.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if result == sys::ESP_ERR_NVS_NOT_FOUND {
            return Ok(None);
        }
        if result != 0 {
            return Err(result);
        }
        if size != N {
            return Err(BAD_RECORD);
        }
        Ok(Some(bytes))
    }
    fn write<const N: usize>(&self, key: &[u8], bytes: &[u8; N]) -> Result<(), i32> {
        // Do not erase the old key first. NVS replaces a blob as a complete value.
        // SAFETY: this worker exclusively owns the handle and live input bytes.
        let result =
            unsafe { sys::nvs_set_blob(self.0, key.as_ptr().cast(), bytes.as_ptr().cast(), N) };
        if result != 0 {
            return Err(result);
        }
        let result = unsafe { sys::nvs_commit(self.0) };
        if result != 0 {
            return Err(result);
        }
        if self.read::<N>(key)? != Some(*bytes) {
            return Err(BAD_RECORD);
        }
        Ok(())
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        unsafe { sys::nvs_close(self.0) }
    }
}

struct State {
    info: Info,
    selection: SelectionSave,
    uuid_pending: Option<Uuid>,
    uuid_writing: bool,
    writable: bool,
    preferences_writable: bool,
    boot_status: &'static str,
    writes: u32,
    failures: u32,
    error: i32,
    max_write_ms: u32,
}
#[derive(Clone)]
pub struct Service {
    state: Arc<Mutex<State>>,
    epoch: Instant,
    startup: Instrument,
}
impl Service {
    /// Runs before LCD/peripheral initialization, including entropy and first identity commit.
    pub fn start(firmware: &'static str) -> io::Result<Self> {
        let store = Store::open();
        let mut info = Info {
            firmware,
            uuid: None,
        };
        let mut saved = None;
        let mut boot_status = "loaded";
        let mut error = 0;
        let mut writes = 0;
        let mut preferences_writable = true;
        if let Ok(store) = &store {
            match store.read::<28>(ID_KEY) {
                Ok(Some(bytes)) => match device::decode_uuid(&bytes) {
                    Ok(id) => info.uuid = Some(id),
                    Err(_) => {
                        boot_status = "identity_invalid";
                        error = BAD_RECORD;
                    }
                },
                Ok(None) => {
                    let mut entropy = [0; 16];
                    // SAFETY: before ADC, radio and other peripherals are initialized.
                    // Enable the documented hardware entropy source for this short interval.
                    unsafe {
                        sys::bootloader_random_enable();
                        sys::esp_fill_random(entropy.as_mut_ptr().cast(), entropy.len());
                        sys::bootloader_random_disable();
                    }
                    let id = Uuid::v4(entropy);
                    match store.write(ID_KEY, &device::encode_uuid(id)) {
                        Ok(()) => {
                            info.uuid = Some(id);
                            writes += 1;
                            boot_status = "created";
                        }
                        Err(code) => {
                            error = code;
                            boot_status = "identity_write_failed";
                        }
                    }
                }
                Err(code) => {
                    error = code;
                    boot_status = "identity_read_failed";
                }
            }
            match store.read::<12>(GAUGE_KEY) {
                Ok(Some(bytes)) => match device::decode_selection(&bytes) {
                    Ok(gauge) => saved = Some(gauge),
                    Err(device::RecordError::Unsupported) => {
                        preferences_writable = false;
                        boot_status = "preferences_newer_version";
                    }
                    Err(_) => {
                        boot_status = "preferences_invalid_default";
                    }
                },
                Ok(None) => (),
                Err(code) => {
                    preferences_writable = false;
                    error = code;
                    boot_status = "preferences_read_failed";
                }
            }
        } else if let Err(code) = &store {
            error = *code;
            boot_status = "storage_unavailable";
        }
        let selection = SelectionSave::new(saved);
        let startup = selection.selected;
        let state = Arc::new(Mutex::new(State {
            info,
            selection,
            uuid_pending: None,
            uuid_writing: false,
            writable: store.is_ok(),
            preferences_writable,
            boot_status,
            writes,
            failures: u32::from(error != 0),
            error,
            max_write_ms: 0,
        }));
        let service = Self {
            state: state.clone(),
            epoch: Instant::now(),
            startup,
        };
        if let Ok(store) = store {
            let mut config = ThreadSpawnConfiguration::get().unwrap_or_default();
            let prior_priority = config.priority;
            let prior_core = config.pin_to_core;
            config.priority = 2;
            config.pin_to_core = Some(Core::Core0);
            config.set().map_err(io::Error::other)?;
            let epoch = service.epoch;
            let spawned = std::thread::Builder::new()
                .name("settings".into())
                .stack_size(8192)
                .spawn(move || loop {
                    let task = {
                        let mut s = state.lock().unwrap();
                        if let Some(id) = s.uuid_pending.take() {
                            s.uuid_writing = true;
                            Some(Work::Identity(id))
                        } else if s.preferences_writable {
                            s.selection.due(ms(epoch)).map(Work::Selection)
                        } else {
                            None
                        }
                    };
                    if let Some(work) = task {
                        let start = Instant::now();
                        let result = match work {
                            Work::Identity(id) => store.write(ID_KEY, &device::encode_uuid(id)),
                            Work::Selection(gauge) => {
                                store.write(GAUGE_KEY, &device::encode_selection(gauge))
                            }
                        };
                        let mut s = state.lock().unwrap();
                        match work {
                            Work::Identity(id) => {
                                s.uuid_writing = false;
                                if result.is_ok() {
                                    s.info.uuid = Some(id);
                                }
                            }
                            Work::Selection(gauge) => {
                                s.selection.completed(gauge, result.is_ok(), ms(epoch))
                            }
                        }
                        s.max_write_ms = s
                            .max_write_ms
                            .max(start.elapsed().as_millis().min(u32::MAX as u128) as u32);
                        match result {
                            Ok(()) => {
                                s.writes = s.writes.saturating_add(1);
                                s.error = 0;
                            }
                            Err(code) => {
                                s.failures = s.failures.saturating_add(1);
                                s.error = code;
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_millis(25));
                });
            // Restore the caller's scheduling policy before display/console workers start.
            config.priority = prior_priority;
            config.pin_to_core = prior_core;
            config.set().map_err(io::Error::other)?;
            spawned?;
        }
        Ok(service)
    }
    pub fn startup(&self) -> Instrument {
        self.startup
    }
    pub fn observe(&self, gauge: Instrument) {
        if let Ok(mut s) = self.state.try_lock() {
            s.selection.observe(gauge, ms(self.epoch));
        }
    }
    pub fn info(&self) -> Option<Info> {
        self.state.try_lock().ok().map(|s| s.info)
    }
    pub fn handle(&self, command: Command) -> Response {
        let Ok(mut s) = self.state.try_lock() else {
            return reply(format_args!("ERR settings_busy"));
        };
        match command {
            Command::Info=>{
                if let Some(id)=s.info.uuid {reply(format_args!("OK device firmware={} uuid={id} identity_persisted=true",s.info.firmware))}
                else{reply(format_args!("OK device firmware={} uuid=unavailable identity_persisted=false error={}",s.info.firmware,s.error))}
            }
            Command::SettingsStatus=>reply(format_args!(
                "OK settings selected={} saved={} dirty={} writable={} preferences_writable={} uuid_pending={} writes={} failures={} error={} max_write_ms={} boot={}",
                s.selection.selected.name(),s.selection.saved.map_or("none",Instrument::name),s.selection.dirty(),
                s.writable,s.preferences_writable,s.uuid_pending.is_some()||s.uuid_writing,s.writes,s.failures,s.error,s.max_write_ms,s.boot_status)),
            Command::Save if !s.writable||!s.preferences_writable=>reply(format_args!("ERR settings_read_only error={}",s.error)),
            Command::Save=>{s.selection.flush();reply(format_args!("OK settings save_queued=true"))}
            Command::SetUuid(_) if !s.writable=>reply(format_args!("ERR storage_unavailable error={}",s.error)),
            Command::SetUuid(_) if s.uuid_pending.is_some()||s.uuid_writing=>reply(format_args!("ERR settings_busy")),
            Command::SetUuid(id) if s.info.uuid==Some(id)=>reply(format_args!("OK device uuid={id} identity_persisted=true")),
            Command::SetUuid(id)=>{s.uuid_pending=Some(id);reply(format_args!("OK device uuid_queued={id} identity_persisted=false"))}
        }
    }
}
#[derive(Clone, Copy)]
enum Work {
    Identity(Uuid),
    Selection(Instrument),
}
fn ms(epoch: Instant) -> u64 {
    epoch.elapsed().as_millis().min(u64::MAX as u128) as u64
}
