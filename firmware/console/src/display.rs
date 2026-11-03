//! SKU 29086 hardware boundary. All raw pointers/interrupts stay in this module.
use crate::panel_init;
use esp_idf_svc::{
    hal::{
        cpu::Core,
        delay::{Ets, TickType},
        gpio::{Output, PinDriver},
        task::{queue::Queue, thread::ThreadSpawnConfiguration},
    },
    sys,
};
use instrument_core::{App, Command, DisplayCommand, Instrument};
use instrument_ui::interaction::Controller;
use instrument_ui::{
    pipeline::{FramePipeline, PresentationFence},
    presentation_healthy, render, Snapshot,
};
use std::{
    cell::RefCell,
    io,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use text_cli::{reply, Response};

static VSYNC: AtomicU32 = AtomicU32::new(0);
static COMPLETE: AtomicU32 = AtomicU32::new(0);
struct CallbackState {
    wake: sys::QueueHandle_t,
    shared: Arc<Shared>,
}
fn now() -> u32 {
    unsafe { (sys::esp_timer_get_time() / 1000) as u32 }
}
fn check(code: i32) -> io::Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(io::Error::other(format!("display ESP error {code}")))
    }
}
// ISR callbacks use lock-free atomics and a bounded ISR-safe wake queue.
// No allocation, blocking waits or logging in the callbacks.
unsafe extern "C" fn vsync(
    _: sys::esp_lcd_panel_handle_t,
    _: *const sys::esp_lcd_rgb_panel_event_data_t,
    _: *mut core::ffi::c_void,
) -> bool {
    VSYNC.fetch_add(1, Ordering::Release);
    false
}
unsafe extern "C" fn complete(
    _: sys::esp_lcd_panel_handle_t,
    _: *const sys::esp_lcd_rgb_panel_event_data_t,
    context: *mut core::ffi::c_void,
) -> bool {
    let callback = &*context.cast::<CallbackState>();
    // This ISR is the sole counter writer. Publish health before the counter,
    // so the worker cannot retire/re-arm while the prior update is incomplete.
    let count = COMPLETE.load(Ordering::Relaxed).wrapping_add(1);
    if let Some(deadline) = callback.shared.fence.complete(count) {
        callback.shared.deadline.store(deadline, Ordering::Release);
        callback.shared.presented.store(now(), Ordering::Release);
    }
    COMPLETE.store(count, Ordering::Release);
    // Queue is allocated before registration and retained until reset, even if
    // the worker exits. No task-handle or stack-pointer lifetime crosses the ISR.
    let token = 1u32;
    let mut woken = 0;
    sys::xQueueGenericSendFromISR(callback.wake, (&token as *const u32).cast(), &mut woken, 0);
    woken != 0
}

/// Initialization owns GPIO1/2 and the LCD RGB pins. It borrows I2C before the
/// bus is moved into its long-lived IMU owner; no competing I2C driver exists.
pub fn initialize(bus: &mut crate::board_bus::Bus) -> io::Result<()> {
    let read = |bus: &mut crate::board_bus::Bus, reg: u8| -> io::Result<u8> {
        let mut b = [0];
        bus.write_read(0x20, &[reg], &mut b)
            .map_err(io::Error::other)?;
        Ok(b[0])
    };
    let write = |bus: &mut crate::board_bus::Bus, reg: u8, value: u8| -> io::Result<()> {
        bus.write(0x20, &[reg, value]).map_err(io::Error::other)?;
        if read(bus, reg)? != value {
            return Err(io::Error::other("EXIO readback mismatch"));
        }
        Ok(())
    };
    let output = (read(bus, 1)? | 5) & !0x80;
    write(bus, 1, output)?;
    let config = read(bus, 3)? & !0x85;
    write(bus, 3, config)?;
    write(bus, 1, output & !1)?;
    thread::sleep(Duration::from_millis(20));
    write(bus, 1, output)?;
    thread::sleep(Duration::from_millis(100));
    // Command-only 9-bit SPI, fixed 3 us phases, no DMA lifetime or SPI bus waits.
    for pin in [1, 2] {
        check(unsafe { sys::gpio_reset_pin(pin) })?;
        check(unsafe { sys::gpio_set_direction(pin, sys::gpio_mode_t_GPIO_MODE_OUTPUT) })?;
    }
    check(unsafe { sys::gpio_set_level(2, 0) })?;
    write(bus, 1, output & !4)?;
    for &(data, byte) in panel_init::INIT {
        let word = ((data as u16) << 8) | byte as u16;
        for shift in (0..9).rev() {
            check(unsafe { sys::gpio_set_level(1, ((word >> shift) & 1) as u32) })?;
            Ets::delay_us(3);
            check(unsafe { sys::gpio_set_level(2, 1) })?;
            Ets::delay_us(3);
            check(unsafe { sys::gpio_set_level(2, 0) })?;
        }
        if !data && byte == 0x11 {
            thread::sleep(Duration::from_millis(120));
        }
    }
    write(bus, 1, output)?;
    Ok(())
}

#[derive(Clone, Copy, Default)]
struct Presented {
    session: u64,
    sequence: u64,
    received_ms: u64,
    presented_ms: u32,
    source_age_ms: Option<u64>,
    altitude: Option<f32>,
    pitch: Option<f32>,
    roll: Option<f32>,
    instrument: Option<Instrument>,
    quality: u32,
    holding: bool,
}
struct Shared {
    sample: Mutex<Presented>,
    fence: PresentationFence,
    requested_pclk: AtomicU32,
    applied_pclk: AtomicU32,
    snapshot: Mutex<Option<(Snapshot, Instant)>>,
    owner: AtomicU32,
    presented: AtomicU32,
    deadline: AtomicU32,
    scanout: AtomicU32,
    frames: AtomicU32,
    render_max: AtomicU32,
    transfer_max: AtomicU32,
    stack_min: AtomicU32,
    enabled: AtomicBool,
    light: AtomicBool,
    failed: AtomicBool,
    ready: AtomicBool,
    pattern: AtomicBool,
    stall: AtomicU32,
    blanked_at: AtomicU32,
    expired_ticks: AtomicU32,
    expired_source_valid: AtomicU32,
    expired_source_invalid: AtomicU32,
    shown_quality: AtomicU32,
    shown_altitude: AtomicU32,
    shown_instrument: AtomicU32,
    shown_editor: AtomicBool,
    shown_menu: AtomicU32,
}
pub struct Service {
    shared: Arc<Shared>,
    stats: Mutex<Stats>,
    interaction: RefCell<Controller>,
    navigation: RefCell<instrument_ui::navigation::Controller>,
    touch_input: RefCell<instrument_ui::interaction::InputCache>,
    touch_snapshot_misses: AtomicU32,
    touch_state: RefCell<Option<crate::touch::State>>,
}
struct Stats {
    started: u32,
    first_frame: u32,
    stopped: Option<(u32, u32, u32, u32)>,
}
impl Service {
    pub fn start(mut backlight: PinDriver<'static, Output>, panel_ready: bool) -> io::Result<Self> {
        let s = Arc::new(Shared {
            fence: PresentationFence::new(),
            sample: Mutex::new(Presented {
                quality: 1,
                ..Presented::default()
            }),
            requested_pclk: AtomicU32::new(18_000_000),
            applied_pclk: AtomicU32::new(0),
            snapshot: Mutex::new(None),
            owner: AtomicU32::new(0),
            presented: AtomicU32::new(0),
            deadline: AtomicU32::new(0),
            scanout: AtomicU32::new(0),
            frames: AtomicU32::new(0),
            render_max: AtomicU32::new(0),
            transfer_max: AtomicU32::new(0),
            stack_min: AtomicU32::new(u32::MAX),
            enabled: AtomicBool::new(true),
            light: AtomicBool::new(false),
            failed: AtomicBool::new(!panel_ready),
            ready: AtomicBool::new(false),
            pattern: AtomicBool::new(false),
            stall: AtomicU32::new(0),
            blanked_at: AtomicU32::new(0),
            expired_ticks: AtomicU32::new(0),
            expired_source_valid: AtomicU32::new(0),
            expired_source_invalid: AtomicU32::new(0),
            shown_quality: AtomicU32::new(6),
            shown_altitude: AtomicU32::new(0),
            shown_instrument: AtomicU32::new(2),
            shown_editor: AtomicBool::new(false),
            shown_menu: AtomicU32::new(0),
        });
        let guard = s.clone();
        thread::Builder::new()
            .name("display-guard".into())
            .stack_size(4096)
            .spawn(move || {
                // Supervisor must preempt both the owner (10) and renderer (5).
                // A CPU-bound owner fault must not prevent backlight blanking.
                unsafe {
                    sys::vTaskPrioritySet(core::ptr::null_mut(), 11);
                }
                if unsafe { sys::esp_task_wdt_add(core::ptr::null_mut()) } != 0 {
                    guard.failed.store(true, Ordering::Release);
                }
                let mut prior = VSYNC.load(Ordering::Acquire);
                loop {
                    let t = now();
                    let count = VSYNC.load(Ordering::Acquire);
                    if count != prior {
                        guard.scanout.store(t, Ordering::Release);
                        prior = count;
                    }
                    // Read asynchronous timestamps before the evaluation clock.
                    // An ISR/other core can publish between any two loads; using
                    // an earlier clock would misclassify that update as future.
                    let owner_at = guard.owner.load(Ordering::Acquire);
                    let presented_at = guard.presented.load(Ordering::Acquire);
                    let deadline = guard.deadline.load(Ordering::Acquire);
                    let scanout_at = guard.scanout.load(Ordering::Acquire);
                    let t = now();
                    // Measure missed numeric deadlines even with the backlight
                    // commanded off, so timing can be qualified without flicker.
                    if guard.ready.load(Ordering::Acquire) && t >= deadline {
                        guard.expired_ticks.fetch_add(1, Ordering::Relaxed);
                        if let Ok(latest) = guard.snapshot.try_lock() {
                            if let Some((snapshot, at)) = *latest {
                                let valid = snapshot
                                    .aged(at.elapsed().as_millis() as u64)
                                    .displayed_quality()
                                    == Some(instrument_core::Quality::Valid);
                                let counter = if valid {
                                    &guard.expired_source_valid
                                } else {
                                    &guard.expired_source_invalid
                                };
                                counter.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                    let healthy = guard.ready.load(Ordering::Acquire)
                        && !guard.failed.load(Ordering::Acquire)
                        && guard.enabled.load(Ordering::Acquire)
                        && presentation_healthy(
                            t as u64,
                            owner_at as u64,
                            presented_at as u64,
                            deadline as u64,
                            scanout_at as u64,
                        );
                    // This task alone owns the backlight. The renderer cannot override it.
                    let was = guard.light.load(Ordering::Acquire);
                    let result = if healthy {
                        backlight.set_high()
                    } else {
                        backlight.set_low()
                    };
                    if result.is_err() {
                        guard.failed.store(true, Ordering::Release);
                        let _ = backlight.set_low();
                    }
                    guard
                        .light
                        .store(healthy && result.is_ok(), Ordering::Release);
                    if was && !healthy {
                        guard.blanked_at.store(t, Ordering::Release);
                    }
                    if unsafe { sys::esp_task_wdt_reset() } != 0 {
                        guard.failed.store(true, Ordering::Release);
                    }
                    thread::sleep(Duration::from_millis(10));
                }
            })?;
        if panel_ready {
            let worker = s.clone();
            // Allocate RGB interrupts on core 0, then move the exclusively
            // owned rendering resources to a core-1 worker.
            let mut configuration = ThreadSpawnConfiguration::get().unwrap_or_default();
            let previous_core = configuration.pin_to_core;
            configuration.pin_to_core = Some(Core::Core0);
            configuration.set().map_err(io::Error::other)?;
            let spawned = thread::Builder::new()
                .name("display-init".into())
                .stack_size(12288)
                .spawn(move || {
                    if let Err(e) = start_display_worker(worker.clone()) {
                        worker.failed.store(true, Ordering::Release);
                        eprintln!("display worker failed: {e}");
                    }
                });
            configuration.pin_to_core = previous_core;
            configuration.set().map_err(io::Error::other)?;
            spawned?;
        }
        Ok(Self {
            interaction: RefCell::new(Controller::default()),
            navigation: RefCell::new(instrument_ui::navigation::Controller::default()),
            touch_input: RefCell::new(instrument_ui::interaction::InputCache::default()),
            touch_snapshot_misses: AtomicU32::new(0),
            touch_state: RefCell::new(None),
            shared: s,
            stats: Mutex::new(Stats {
                started: now(),
                first_frame: 0,
                stopped: None,
            }),
        })
    }
    pub fn publish(&self, app: &mut App, touch: Option<crate::touch::State>) -> bool {
        let s = &self.shared;
        if touch.is_some() {
            *self.touch_state.borrow_mut() = touch;
        } else {
            self.touch_snapshot_misses.fetch_add(1, Ordering::Relaxed);
        }
        let mut interaction = self.interaction.borrow_mut();
        let mut navigation = self.navigation.borrow_mut();
        let healthy = s.light.load(Ordering::Acquire)
            && !s.pattern.load(Ordering::Acquire)
            && s.enabled.load(Ordering::Acquire)
            && s.stall.load(Ordering::Acquire) == 0
            && !s.failed.load(Ordering::Acquire);
        let input = self
            .touch_input
            .borrow_mut()
            .read(app.now_ms(), touch.map(|t| t.input(app.now_ms())));
        let menu_was_busy = navigation.busy();
        // Numeric expiry must invalidate displayed gauge data, but it must not
        // destroy an open navigation menu. Use independent pipeline supervision
        // here; update_visible suppresses all gestures while actually dark.
        let owner_at = s.owner.load(Ordering::Acquire);
        let presented_at = s.presented.load(Ordering::Acquire);
        let scanout_at = s.scanout.load(Ordering::Acquire);
        let navigation_healthy = s.ready.load(Ordering::Acquire)
            && s.enabled.load(Ordering::Acquire)
            && !s.pattern.load(Ordering::Acquire)
            && s.stall.load(Ordering::Acquire) == 0
            && !s.failed.load(Ordering::Acquire)
            && presentation_healthy(now() as u64, owner_at as u64, presented_at as u64, u64::MAX, scanout_at as u64);
        let selection = navigation.update_visible(
            app.now_ms(),
            input,
            navigation_healthy && interaction.view().is_none() && !interaction.window_pressed(),
            s.light.load(Ordering::Acquire),
            instrument_ui::navigation::Context::from_app(app),
            s.shown_menu.load(Ordering::Acquire),
        );
        let allowed = healthy
            && app.baro_edit_allowed()
            && app.selected() == Instrument::Altimeter
            && !menu_was_busy
            && !navigation.busy()
            && selection.is_none();
        if let Some(value) = interaction.update(
            app.now_ms(),
            input,
            allowed,
            app.baro_inhg(),
            s.shown_editor.load(Ordering::Acquire),
        ) {
            let _ = app.apply(Command::Baro(value));
        }
        if let Some(instrument) = selection {
            let _ = app.apply(Command::Select(instrument));
        }
        if app.timing_fault() {
            s.failed.store(true, Ordering::Release);
            return false;
        }
        if let Ok(mut v) = s.snapshot.try_lock() {
            let mut snapshot = Snapshot::from_app(app);
            snapshot.editor = interaction.view();
            snapshot.kollsman_pressed = interaction.window_pressed();
            snapshot.menu = navigation.view();
            snapshot.center_pressed = navigation.center_pressed();
            *v = Some((snapshot, Instant::now()));
            s.owner.store(now(), Ordering::Release);
        }
        interaction.press_started() || navigation.press_started()
    }
    pub fn handle(&self, command: DisplayCommand) -> Response {
        let s = &self.shared;
        match command {
            DisplayCommand::SampleStatus => {
                let v = match s.sample.try_lock() {
                    Ok(v) => *v,
                    Err(_) => return reply(format_args!("ERR display_sample_busy")),
                };
                let at = now();
                return reply(format_args!("OK display_sample now_display_ms={} presented_display_ms={} session={} sequence={} received_owner_ms={} source_age_at_present_ms={:?} source_age_now_ms={:?} shown_alt_ft={:?} quality={} renderer={} shown_pitch_deg={:?} shown_roll_deg={:?} holding={}",at,v.presented_ms,v.session,v.sequence,v.received_ms,v.source_age_ms,v.source_age_ms.map(|age|age.saturating_add(at.wrapping_sub(v.presented_ms) as u64)),v.altitude,["valid","missing","stale","failed","timing_fault","placeholder","pattern"][v.quality as usize],v.instrument.map_or("unknown",Instrument::name),v.pitch,v.roll,v.holding));
            }
            DisplayCommand::TouchSound(_) | DisplayCommand::TouchSoundStatus => {
                return reply(format_args!("ERR touch_sound_unavailable"))
            }
            DisplayCommand::TouchHold(ms) => {
                if !self.interaction.borrow_mut().set_hold_ms(ms) {
                    return reply(format_args!("ERR range"));
                }
                return reply(format_args!("OK touch hold_ms={ms} edit=cancelled"));
            }
            DisplayCommand::MenuHold(ms) => {
                if !self.navigation.borrow_mut().set_hold_ms(ms) {
                    return reply(format_args!("ERR range"));
                }
                return reply(format_args!("OK touch menu_hold_ms={ms} menu=cancelled"));
            }
            DisplayCommand::TouchCancel => {
                self.interaction.borrow_mut().cancel();
                self.navigation.borrow_mut().cancel();
                return reply(format_args!("OK touch edit=cancelled"));
            }
            DisplayCommand::TouchStatus => {
                let ui = self.interaction.borrow();
                let state = *self.touch_state.borrow();
                let sample = state.and_then(|s| s.sample);
                let menu = self.navigation.borrow();
                return reply(format_args!("OK touch ready={} hold_ms={} overlay={} draft_cents={:?} contact={:?} age_ms={:?} polls={} reports={} errors={} last_error={:?} menu={} menu_hold_ms={} center_pressed={} menu_generation={} shown_menu={} snapshot_misses={}",
                    state.is_some_and(|s|s.ready),ui.hold_ms(),ui.view().is_some(),ui.view().map(|v|v.cents),sample.map(|s|s.0),sample.map(|s|s.1.elapsed().as_millis()),
                    state.map_or(0,|s|s.polls),state.map_or(0,|s|s.reports),state.map_or(0,|s|s.errors),state.and_then(|s|s.error),
                    menu.view().is_some(),menu.hold_ms(),menu.center_pressed(),menu.view().map_or(0,|v|v.generation),s.shown_menu.load(Ordering::Acquire), self.touch_snapshot_misses.load(Ordering::Relaxed)));
            }
            DisplayCommand::Stats | DisplayCommand::StatsReset | DisplayCommand::StatsStop => {
                let Ok(mut stats) = self.stats.try_lock() else {
                    return reply(format_args!("ERR display_stats_busy"));
                };
                let t = now();
                let frames = s.frames.load(Ordering::Acquire);
                if command == DisplayCommand::StatsReset {
                    *stats = Stats {
                        started: t,
                        first_frame: frames,
                        stopped: None,
                    };
                    s.render_max.store(0, Ordering::Relaxed);
                    s.transfer_max.store(0, Ordering::Relaxed);
                }
                if command == DisplayCommand::StatsStop && stats.stopped.is_none() {
                    stats.stopped = Some((
                        t,
                        frames,
                        s.render_max.load(Ordering::Relaxed),
                        s.transfer_max.load(Ordering::Relaxed),
                    ));
                }
                let (end, frames, render_ms, transfer_ms) = stats.stopped.unwrap_or((
                    t,
                    frames,
                    s.render_max.load(Ordering::Relaxed),
                    s.transfer_max.load(Ordering::Relaxed),
                ));
                let elapsed = end.wrapping_sub(stats.started);
                let count = frames.wrapping_sub(stats.first_frame);
                let fps = if elapsed == 0 {
                    0.
                } else {
                    count as f32 * 1000. / elapsed as f32
                };
                return reply(format_args!("OK display_stats state={} frames={} elapsed_ms={} rendered_fps={:.2} render_max_ms={} transfer_max_ms={} backlight={} failed={} measurement=completed_presentations", if stats.stopped.is_some(){"stopped"}else{"running"}, count, elapsed, fps, render_ms, transfer_ms, s.light.load(Ordering::Acquire), s.failed.load(Ordering::Acquire)));
            }
            DisplayCommand::PixelClock(hz) if matches!(hz, 12_000_000 | 18_000_000) => {
                s.requested_pclk.store(hz, Ordering::Release);
            }
            DisplayCommand::PixelClock(_) => return reply(format_args!("ERR range")),
            DisplayCommand::On => s.enabled.store(true, Ordering::Release),
            DisplayCommand::Off => s.enabled.store(false, Ordering::Release),
            DisplayCommand::Pattern => s.pattern.store(true, Ordering::Release),
            DisplayCommand::Gauge => s.pattern.store(false, Ordering::Release),
            DisplayCommand::Stall => s.stall.store(1, Ordering::Release),
            DisplayCommand::TransferStall => s.stall.store(2, Ordering::Release),
            DisplayCommand::Recover => s.stall.store(0, Ordering::Release),
            DisplayCommand::Status => {
                return reply(format_args!("OK display pclk_requested_hz={} pclk_applied_hz={} ready={} failed={} backlight={} enabled={} mode={} stall={} frames={} frame_age_ms={} owner_age_ms={} render_max_ms={} transfer_max_ms={} stack_free_bytes={} psram_bytes={} internal_free_bytes={} blanked_at_ms={} expired_ticks={} expired_source_valid={} expired_source_invalid={} source={} renderer={} shown_quality={} shown_alt_ft={:?}",s.requested_pclk.load(Ordering::Acquire),s.applied_pclk.load(Ordering::Acquire),s.ready.load(Ordering::Acquire),s.failed.load(Ordering::Acquire),s.light.load(Ordering::Acquire),s.enabled.load(Ordering::Acquire),if s.pattern.load(Ordering::Acquire){"pattern"}else{"gauge"},s.stall.load(Ordering::Acquire),s.frames.load(Ordering::Acquire),now().wrapping_sub(s.presented.load(Ordering::Acquire)),now().wrapping_sub(s.owner.load(Ordering::Acquire)),s.render_max.load(Ordering::Acquire),s.transfer_max.load(Ordering::Acquire),s.stack_min.load(Ordering::Acquire),unsafe{sys::esp_psram_get_size()},unsafe{sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL)},s.blanked_at.load(Ordering::Acquire),s.expired_ticks.load(Ordering::Acquire),s.expired_source_valid.load(Ordering::Acquire),s.expired_source_invalid.load(Ordering::Acquire),s.snapshot.try_lock().ok().and_then(|v|v.as_ref().map(|(v,_)|if v.imu_bench {"imu_bench"} else if v.external {"external"} else {"sim"})).unwrap_or("unknown"),["altimeter","attitude","placeholder","turn"][s.shown_instrument.load(Ordering::Acquire) as usize],["valid","missing","stale","failed","timing_fault","placeholder","pattern"][s.shown_quality.load(Ordering::Acquire) as usize],if s.shown_quality.load(Ordering::Acquire)==0 && s.shown_instrument.load(Ordering::Acquire)==0 {Some(f32::from_bits(s.shown_altitude.load(Ordering::Acquire)))}else{None}));
            }
        }
        reply(format_args!(
            "OK display requested={:?} completion=check_status",
            command
        ))
    }
}

/// Panel and framebuffers intentionally live until reset. On any failure they
/// are not freed under DMA; the guard blanks and a reboot permits clean retry.
struct Backend {
    panel: sys::esp_lcd_panel_handle_t,
    buffers: [*mut core::ffi::c_void; 3],
    backdrop: &'static mut [u16],
    attitude_backdrop: &'static mut [u16],
    turn_backdrop: &'static mut [u16],
    foreground_pixels: &'static mut [u16],
    navigation_pixels: &'static mut [u16],
    turn_navigation_pixels: &'static mut [u16],
    wake: Queue<u32>,
}
// Initialization transfers this handle exactly once to the sole drawing task.
// IRQs only read submitted buffers; FramePipeline excludes those from mutation.
// Native allocations and the borrowed wake queue remain alive until reset.
unsafe impl Send for Backend {}

fn start_display_worker(shared: Arc<Shared>) -> io::Result<()> {
    let backend = create_backend(shared.clone())?;
    let mut configuration = ThreadSpawnConfiguration::get().unwrap_or_default();
    configuration.pin_to_core = Some(Core::Core1);
    configuration.set().map_err(io::Error::other)?;
    thread::Builder::new()
        .name("display".into())
        .stack_size(20480)
        .spawn(move || {
            if let Err(error) = run_display(&shared, backend) {
                shared.failed.store(true, Ordering::Release);
                eprintln!("display worker failed: {error}");
            }
        })?;
    Ok(())
}
fn create_backend(shared: Arc<Shared>) -> io::Result<Backend> {
    let mut cfg = sys::esp_lcd_rgb_panel_config_t::default();
    cfg.clk_src = sys::soc_periph_lcd_clk_src_t_LCD_CLK_SRC_DEFAULT;
    cfg.data_width = 16;
    cfg.bits_per_pixel = 16;
    cfg.num_fbs = 3;
    // 10-line refills missed DMA deadlines under shared-PSRAM load. Sixteen
    // lines retain the 18 MHz/30+ FPS budget with zero recovery in stress tests.
    cfg.bounce_buffer_size_px = 480 * 16;
    cfg.sram_trans_align = 4;
    cfg.__bindgen_anon_1.dma_burst_size = 64;
    cfg.hsync_gpio_num = 38;
    cfg.vsync_gpio_num = 39;
    cfg.de_gpio_num = 40;
    cfg.pclk_gpio_num = 41;
    cfg.disp_gpio_num = -1;
    cfg.data_gpio_nums = [5, 45, 48, 47, 21, 14, 13, 12, 11, 10, 9, 46, 3, 8, 18, 17];
    cfg.timings.pclk_hz = 18_000_000;
    cfg.timings.h_res = 480;
    cfg.timings.v_res = 480;
    cfg.timings.hsync_back_porch = 10;
    cfg.timings.hsync_front_porch = 50;
    cfg.timings.hsync_pulse_width = 8;
    cfg.timings.vsync_back_porch = 18;
    cfg.timings.vsync_front_porch = 8;
    cfg.timings.vsync_pulse_width = 2;
    cfg.flags.set_fb_in_psram(1);
    let mut panel = core::ptr::null_mut();
    check(unsafe { sys::esp_lcd_new_rgb_panel(&cfg, &mut panel) })?;
    let mut buffers = [core::ptr::null_mut::<core::ffi::c_void>(); 3];
    let [fb0, fb1, fb2] = &mut buffers;
    check(unsafe { sys::esp_lcd_rgb_panel_get_frame_buffer(panel, 3, fb0, fb1, fb2) })?;
    if buffers.iter().any(|p| p.is_null())
        || buffers[0] == buffers[1]
        || buffers[0] == buffers[2]
        || buffers[1] == buffers[2]
    {
        return Err(io::Error::other("invalid LCD framebuffers"));
    }
    // An additional CPU-only PSRAM buffer caches the static face. It is never given
    // to the panel driver, and its mutable slice never aliases either DMA frame.
    let backdrop = unsafe {
        sys::heap_caps_calloc(480 * 480, 2, sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT)
    };
    if backdrop.is_null() {
        return Err(io::Error::other("LCD backdrop allocation"));
    }
    let backdrop = unsafe { core::slice::from_raw_parts_mut(backdrop.cast::<u16>(), 480 * 480) };
    let attitude_backdrop = unsafe {
        sys::heap_caps_calloc(480 * 480, 2, sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT)
    };
    if attitude_backdrop.is_null() {
        return Err(io::Error::other("LCD attitude backdrop allocation"));
    }
    let attitude_backdrop =
        unsafe { core::slice::from_raw_parts_mut(attitude_backdrop.cast::<u16>(), 480 * 480) };
    let turn_backdrop = unsafe { sys::heap_caps_calloc(480 * 480, 2, sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT) };
    if turn_backdrop.is_null() { return Err(io::Error::other("LCD turn backdrop allocation")); }
    let turn_backdrop = unsafe { core::slice::from_raw_parts_mut(turn_backdrop.cast::<u16>(), 480 * 480) };
    let navigation_pixels = unsafe {
        sys::heap_caps_calloc(480 * 480, 2, sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT)
    };
    if navigation_pixels.is_null() {
        return Err(io::Error::other("LCD navigation cache allocation"));
    }
    let navigation_pixels =
        unsafe { core::slice::from_raw_parts_mut(navigation_pixels.cast::<u16>(), 480 * 480) };
    // CPU-only artwork, never submitted to DMA. Turn needs bounded menu cost
    // under the same 100 ms local lifetime as Attitude.
    let turn_navigation_pixels = unsafe {
        sys::heap_caps_calloc(480 * 480, 2, sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT)
    };
    if turn_navigation_pixels.is_null() {
        return Err(io::Error::other("LCD turn navigation cache allocation"));
    }
    let turn_navigation_pixels = unsafe {
        core::slice::from_raw_parts_mut(turn_navigation_pixels.cast::<u16>(), 480 * 480)
    };
    let foreground_pixels =
        unsafe { sys::heap_caps_calloc(49152, 2, sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT) };
    if foreground_pixels.is_null() {
        return Err(io::Error::other("LCD foreground allocation"));
    }
    let foreground_pixels =
        unsafe { core::slice::from_raw_parts_mut(foreground_pixels.cast::<u16>(), 49152) };
    // Before scanout starts all allocated RGB565 buffers are exclusively CPU owned.
    for &p in &buffers {
        unsafe { core::ptr::write_bytes(p, 0, 480 * 480 * 2) };
    }
    // A single token coalesces wakeups. COMPLETE, not queue depth, establishes
    // buffer ownership. Keep the queue allocation alive with the panel on errors.
    let wake_handle = unsafe { sys::xQueueGenericCreate(1, core::mem::size_of::<u32>() as u32, 0) };
    if wake_handle.is_null() {
        return Err(io::Error::other("LCD wake queue allocation"));
    }
    let wake = unsafe { Queue::<u32>::new_borrowed(wake_handle) };
    let mut cbs = sys::esp_lcd_rgb_panel_event_callbacks_t::default();
    cbs.on_vsync = Some(vsync);
    cbs.__bindgen_anon_1.on_frame_buf_complete = Some(complete);
    let callback = Box::into_raw(Box::new(CallbackState {
        wake: wake_handle,
        shared,
    }));
    check(unsafe {
        sys::esp_lcd_rgb_panel_register_event_callbacks(panel, &cbs, callback.cast())
    })?;
    check(unsafe { sys::esp_lcd_panel_reset(panel) })?;
    check(unsafe { sys::esp_lcd_panel_init(panel) })?;
    Ok(Backend {
        panel,
        buffers,
        backdrop,
        attitude_backdrop,
        turn_backdrop,
        foreground_pixels,
        navigation_pixels,
        turn_navigation_pixels,
        wake,
    })
}
fn run_display(s: &Shared, backend: Backend) -> io::Result<()> {
    let Backend {
        panel,
        buffers,
        backdrop,
        attitude_backdrop,
        turn_backdrop,
        foreground_pixels,
        navigation_pixels,
        turn_navigation_pixels,
        wake,
    } = backend;
    let mut foreground_runs = vec![instrument_ui::attitude::CopyRun::default(); instrument_ui::attitude::FOREGROUND_RUN_CAPACITY];
    let mut renderer = render::CachedRenderer::new(backdrop, 480, 480)
        .ok_or_else(|| io::Error::other("LCD backdrop size"))?;
    let mut turn_renderer = render::CachedRenderer::new(turn_backdrop,480,480)
        .ok_or_else(|| io::Error::other("LCD turn backdrop size"))?
        .with_navigation_pixels(turn_navigation_pixels);
    let mut attitude_renderer = render::CachedRenderer::with_foreground_cache(
        attitude_backdrop,
        480,
        480,
        &mut foreground_runs,
    )
    .ok_or_else(|| io::Error::other("LCD backdrop size"))?
    .with_foreground_pixels(foreground_pixels)
    .with_navigation_pixels(navigation_pixels);
    // Warm both static faces before the first visible frame. Switching views
    // must not discard/rebuild the other face while a 100 ms attitude deadline
    // is running. These CPU-only caches contain artwork, not sampled readings.
    let mut warm = loop {
        let input = s
            .snapshot
            .lock()
            .map_err(|_| io::Error::other("display snapshot poisoned"))?
            .as_ref()
            .map(|(snapshot, _)| *snapshot);
        if let Some(snapshot) = input {
            break snapshot;
        }
        thread::sleep(Duration::from_millis(10));
    };
    warm.selected = Instrument::Altimeter;
    if !renderer.prepare_background(warm) {
        return Err(io::Error::other("altimeter preparation"));
    }
    warm.selected = Instrument::Turn;
    if !turn_renderer.prepare_background(warm) { return Err(io::Error::other("turn preparation")); }
    warm.selected = Instrument::Attitude;
    if !attitude_renderer.prepare_background(warm) {
        return Err(io::Error::other("attitude preparation"));
    }
    let mut applied_pclk = 18_000_000;
    s.applied_pclk.store(applied_pclk, Ordering::Release);
    let mut histories =
        core::array::from_fn::<_, 3, _>(|_| instrument_ui::damage::FrameHistory::new());
    let mut pipeline = FramePipeline::<RenderedFrame>::new();
    let mut idle_budget = Instant::now();
    let mut last_draw_ms = 30u32;
    let mut last_submit_ms = 0u32;
    let mut previous_view = None;
    loop {
        // Completed transfers can overlap all CPU work. Independently budget
        // idle time even when no completion wait blocks; keep both idle-task
        // watchdog subscriptions enabled. Never spin continuously on the CPU.
        if idle_budget.elapsed() >= Duration::from_millis(100) {
            // One actual 1 ms RTOS tick gives idle time without a 10 ms pause.
            unsafe {
                sys::vTaskDelay(1);
            }
            idle_budget = Instant::now();
        }
        let requested_pclk = s.requested_pclk.load(Ordering::Acquire);
        if requested_pclk != applied_pclk {
            check(unsafe { sys::esp_lcd_rgb_panel_set_pclk(panel, requested_pclk) })?;
            // The driver applies clock changes at VSYNC, not at API return.
            let before = VSYNC.load(Ordering::Acquire);
            let began = Instant::now();
            while VSYNC.load(Ordering::Acquire).wrapping_sub(before) < 2 {
                if began.elapsed() > Duration::from_millis(100) {
                    return Err(io::Error::other("LCD clock transition timeout"));
                }
                thread::sleep(Duration::from_millis(10));
            }
            applied_pclk = requested_pclk;
            s.applied_pclk.store(applied_pclk, Ordering::Release);
        }
        if s.stall.load(Ordering::Acquire) == 1 {
            thread::sleep(Duration::from_millis(20));
            continue;
        }
        // Late capture: avoid spending the spare buffer's source lifetime
        // waiting behind an earlier transfer. Predict only scheduling time;
        // completion counters still exclusively determine DMA ownership.
        if matches!(previous_view, Some(Instrument::Attitude | Instrument::Turn)) && pipeline.is_pending() {
            let frame_ms = (548u64 * 508 * 1000).div_ceil(applied_pclk as u64) as u32;
            loop {
                // A new view must start immediately: the old attitude frame
                // still has a 100 ms expiry while the new face is composed.
                // Re-check during the wait so a touch/CLI selection arriving
                // after its first iteration cannot spend the old view's slack.
                let next = s
                    .snapshot
                    .lock()
                    .map_err(|_| io::Error::other("display snapshot poisoned"))?
                    .as_ref()
                    .map_or(previous_view.unwrap_or(Instrument::Attitude), |(v, _)| v.selected);
                let delay = instrument_ui::pipeline::capture_delay_ms(
                    previous_view,
                    next,
                    frame_ms,
                    last_draw_ms,
                );
                if s.pattern.load(Ordering::Acquire) || now().wrapping_sub(last_submit_ms) >= delay
                {
                    break;
                }
                let blocked_at = Instant::now();
                wake.recv_front(TickType::new_millis(1).ticks());
                if blocked_at.elapsed() >= Duration::from_millis(1) {
                    idle_budget = Instant::now();
                }
                if let Some((frame, elapsed)) = pipeline
                    .poll_confirmed(COMPLETE.load(Ordering::Acquire), now(), !s.fence.pending())
                    .map_err(|_| io::Error::other("LCD completion timeout"))?
                {
                    record_presentation(s, frame, elapsed);
                    break;
                }
            }
        }
        let input = s
            .snapshot
            .lock()
            .map_err(|_| io::Error::other("display snapshot poisoned"))?
            .as_ref()
            .copied();
        let Some((snapshot, at)) = input else {
            thread::sleep(Duration::from_millis(20));
            continue;
        };
        let started = Instant::now();
        let stamp = now();
        let snapshot = snapshot.aged(at.elapsed().as_millis() as u64);
        let pattern = s.pattern.load(Ordering::Acquire);
        let lifetime = if pattern {
            None
        } else {
            snapshot.numeric_lifetime_ms()
        };
        // The pipeline exposes only the spare CPU buffer. Rendering overlaps
        // the previous submission, while its front/pending buffers stay immutable.
        let back = pipeline.drawing();
        // The mutable slice is scoped to drawing and gone before submitting to DMA.
        {
            let pixels =
                unsafe { core::slice::from_raw_parts_mut(buffers[back].cast::<u16>(), 480 * 480) };
            let mut completion_error = None;
            let mut progress = || {
                if completion_error.is_none() {
                    match pipeline.poll_confirmed(
                        COMPLETE.load(Ordering::Acquire),
                        now(),
                        !s.fence.pending(),
                    ) {
                        Ok(Some((frame, elapsed))) => record_presentation(s, frame, elapsed),
                        Ok(None) => {}
                        Err(error) => completion_error = Some(error),
                    }
                }
            };
            progress();
            let active_renderer = if snapshot.selected == Instrument::Attitude && !pattern {
                &mut attitude_renderer
            } else if snapshot.selected == Instrument::Turn && !pattern {
                &mut turn_renderer
            } else {
                &mut renderer
            };
            if !active_renderer.draw_incremental_with_progress(
                pixels,
                &mut histories[back],
                snapshot,
                pattern,
                &mut progress,
            ) {
                return Err(io::Error::other("frame size"));
            }
            if completion_error.is_some() {
                return Err(io::Error::other("LCD completion timeout"));
            }
        }
        last_draw_ms = started.elapsed().as_millis() as u32;
        previous_view = (!pattern).then_some(snapshot.selected);
        s.render_max.fetch_max(last_draw_ms, Ordering::Relaxed);
        while s.stall.load(Ordering::Acquire) == 2 {
            thread::sleep(Duration::from_millis(20));
        }
        // Retire the previous frame only after two POST-submission completions.
        // The next frame has already been drawn in independent storage. Queue
        // waits wake immediately from ISR, removing 10 ms polling quantization.
        while pipeline.is_pending() {
            if let Some((frame, elapsed)) = pipeline
                .poll_confirmed(COMPLETE.load(Ordering::Acquire), now(), !s.fence.pending())
                .map_err(|_| io::Error::other("LCD completion timeout"))?
            {
                record_presentation(s, frame, elapsed);
            } else {
                let wait_started = Instant::now();
                wake.recv_front(TickType::new_millis(10).ticks());
                // Actual blocking allows the idle task to run; queued tokens
                // returning immediately do not count. Keep the one-tick
                // blocking fallback and both independent idle watchdogs enabled.
                if wait_started.elapsed() >= Duration::from_millis(1) {
                    idle_budget = Instant::now();
                }
            }
        }
        // A very delayed arm may need one more ISR event to consume its fence.
        // Never overwrite a still-armed payload, even after buffer retirement.
        let fence_wait = Instant::now();
        while s.fence.pending() {
            if fence_wait.elapsed() > Duration::from_millis(100) {
                return Err(io::Error::other("LCD presentation fence timeout"));
            }
            wake.recv_front(TickType::new_millis(10).ticks());
        }
        // Aging includes drawing AND waiting for the earlier frame. Discard an
        // expired prepared frame; never refresh its source timestamp to keep it.
        if lifetime.is_some_and(|ttl| now().wrapping_sub(stamp) as u64 >= ttl) {
            continue;
        }
        check(unsafe { sys::esp_lcd_panel_draw_bitmap(panel, 0, 0, 480, 480, buffers[back]) })?;
        let after_submit = COMPLETE.load(Ordering::Acquire);
        last_submit_ms = now();
        let deadline = lifetime
            .map(|ttl| stamp.saturating_add(ttl as u32))
            .unwrap_or(u32::MAX);
        if !s.fence.arm(after_submit, deadline) {
            return Err(io::Error::other("LCD presentation fence busy"));
        }
        pipeline
            .submitted(
                after_submit,
                now(),
                RenderedFrame {
                    snapshot,
                    pattern,
                    stamp,
                },
            )
            .map_err(|_| io::Error::other("LCD pipeline busy"))?;
    }
}

#[derive(Clone, Copy)]
struct RenderedFrame {
    snapshot: Snapshot,
    pattern: bool,
    stamp: u32,
}

fn record_presentation(s: &Shared, frame: RenderedFrame, elapsed: u32) {
    let RenderedFrame {
        snapshot,
        pattern,
        stamp,
    } = frame;
    s.transfer_max.fetch_max(elapsed, Ordering::Relaxed);
    let model = snapshot.model();
    let quality = if pattern {
        6
    } else if let Some(quality) = snapshot.displayed_quality() {
        match quality {
            instrument_core::Quality::Valid => 0,
            instrument_core::Quality::Missing => 1,
            instrument_core::Quality::Stale => 2,
            instrument_core::Quality::Failed => 3,
            instrument_core::Quality::TimingFault => 4,
        }
    } else {
        5
    };
    let presented_ms = s.presented.load(Ordering::Acquire);
    if let Ok(mut v) = s.sample.lock() {
        *v = Presented {
            session: snapshot.source_session,
            sequence: snapshot.source_sequence,
            received_ms: snapshot.source_received_ms,
            presented_ms,
            source_age_ms: snapshot
                .displayed_age_ms()
                .map(|age| age.saturating_add(presented_ms.wrapping_sub(stamp) as u64)),
            altitude: if quality == 0 && snapshot.selected == Instrument::Altimeter {
                model.indicated_ft
            } else {
                None
            },
            pitch: if !pattern && snapshot.selected == Instrument::Attitude {
                snapshot.attitude_model().angles.map(|a| a.pitch_deg)
            } else {
                None
            },
            roll: if !pattern && snapshot.selected == Instrument::Attitude {
                snapshot.attitude_model().angles.map(|a| a.roll_deg)
            } else {
                None
            },
            instrument: if pattern {
                None
            } else {
                Some(snapshot.selected)
            },
            quality,
            holding: !pattern
                && snapshot.selected == Instrument::Attitude
                && snapshot.imu_holding(),
        };
    }
    s.shown_altitude.store(
        model.indicated_ft.unwrap_or(0.).to_bits(),
        Ordering::Release,
    );
    s.shown_instrument.store(
        match snapshot.selected {
            Instrument::Altimeter => 0,
            Instrument::Attitude => 1,
            Instrument::Turn => 3,
            _ => 2,
        },
        Ordering::Release,
    );
    s.shown_quality.store(quality, Ordering::Release);
    s.shown_editor
        .store(quality == 0 && snapshot.editor.is_some(), Ordering::Release);
    s.shown_menu.store(
        if pattern {
            0
        } else {
            snapshot.menu.map_or(0, |v| v.generation)
        },
        Ordering::Release,
    );
    s.frames.fetch_add(1, Ordering::Release);
    s.ready.store(true, Ordering::Release);
    s.stack_min.fetch_min(
        unsafe { sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut()) },
        Ordering::Relaxed,
    );
}
