//! SKU 29086 hardware boundary. All raw pointers/interrupts stay in this module.
use crate::panel_init;
use esp_idf_svc::{
    hal::{
        delay::{Ets, TickType},
        gpio::{Output, PinDriver},
        task::queue::Queue,
    },
    sys,
};
use instrument_core::{App, Command, DisplayCommand, Field, Instrument, Quality};
use instrument_ui::interaction::Controller;
use instrument_ui::{pipeline::FramePipeline, presentation_healthy, render, Snapshot};
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
    COMPLETE.fetch_add(1, Ordering::Release);
    // Queue is allocated before registration and retained until reset, even if
    // the worker exits. No task-handle or stack-pointer lifetime crosses the ISR.
    let token = 1u32;
    let mut woken = 0;
    sys::xQueueGenericSendFromISR(context.cast(), (&token as *const u32).cast(), &mut woken, 0);
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

struct Shared {
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
    shown_quality: AtomicU32,
    shown_altitude: AtomicU32,
    shown_editor: AtomicBool,
}
pub struct Service {
    shared: Arc<Shared>,
    stats: Mutex<Stats>,
    interaction: RefCell<Controller>,
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
            shown_quality: AtomicU32::new(6),
            shown_altitude: AtomicU32::new(0),
            shown_editor: AtomicBool::new(false),
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
                    let healthy = guard.ready.load(Ordering::Acquire)
                        && !guard.failed.load(Ordering::Acquire)
                        && guard.enabled.load(Ordering::Acquire)
                        && presentation_healthy(
                            t as u64,
                            guard.owner.load(Ordering::Acquire) as u64,
                            guard.presented.load(Ordering::Acquire) as u64,
                            guard.deadline.load(Ordering::Acquire) as u64,
                            guard.scanout.load(Ordering::Acquire) as u64,
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
            thread::Builder::new()
                .name("display".into())
                .stack_size(12288)
                .spawn(move || {
                    if let Err(e) = run_display(&worker) {
                        worker.failed.store(true, Ordering::Release);
                        eprintln!("display worker failed: {e}");
                    }
                })?;
        }
        Ok(Self {
            interaction: RefCell::new(Controller::default()),
            touch_state: RefCell::new(None),
            shared: s,
            stats: Mutex::new(Stats {
                started: now(),
                first_frame: 0,
                stopped: None,
            }),
        })
    }
    pub fn publish(&self, app: &mut App, touch: Option<crate::touch::State>) {
        let s = &self.shared;
        *self.touch_state.borrow_mut() = touch;
        let mut interaction = self.interaction.borrow_mut();
        let allowed = app.selected() == Instrument::Altimeter
            && app.reading(Field::PressureAltitudeFt, app.now_ms()).quality == Quality::Valid
            && s.light.load(Ordering::Acquire)
            && !s.pattern.load(Ordering::Acquire)
            && s.enabled.load(Ordering::Acquire)
            && s.stall.load(Ordering::Acquire) == 0
            && !s.failed.load(Ordering::Acquire);
        if let Some(value) = interaction.update(
            app.now_ms(),
            touch.and_then(|t| t.input(app.now_ms())),
            allowed,
            app.baro_inhg(),
            s.shown_editor.load(Ordering::Acquire),
        ) {
            let _ = app.apply(Command::Baro(value));
        }
        if app.timing_fault() {
            s.failed.store(true, Ordering::Release);
            return;
        }
        if let Ok(mut v) = s.snapshot.try_lock() {
            let mut snapshot = Snapshot::from_app(app);
            snapshot.editor = interaction.view();
            snapshot.kollsman_pressed = interaction.window_pressed();
            *v = Some((snapshot, Instant::now()));
            s.owner.store(now(), Ordering::Release);
        }
    }
    pub fn handle(&self, command: DisplayCommand) -> Response {
        let s = &self.shared;
        match command {
            DisplayCommand::TouchSound(_) | DisplayCommand::TouchSoundStatus => {
                return reply(format_args!("ERR touch_sound_unavailable"))
            }
            DisplayCommand::TouchHold(ms) => {
                if !self.interaction.borrow_mut().set_hold_ms(ms) {
                    return reply(format_args!("ERR range"));
                }
                return reply(format_args!("OK touch hold_ms={ms} edit=cancelled"));
            }
            DisplayCommand::TouchCancel => {
                self.interaction.borrow_mut().cancel();
                return reply(format_args!("OK touch edit=cancelled"));
            }
            DisplayCommand::TouchStatus => {
                let ui = self.interaction.borrow();
                let state = *self.touch_state.borrow();
                let sample = state.and_then(|s| s.sample);
                return reply(format_args!("OK touch ready={} hold_ms={} overlay={} draft_cents={:?} contact={:?} age_ms={:?} polls={} reports={} errors={} last_error={:?}",
                    state.is_some_and(|s|s.ready),ui.hold_ms(),ui.view().is_some(),ui.view().map(|v|v.cents),sample.map(|s|s.0),sample.map(|s|s.1.elapsed().as_millis()),
                    state.map_or(0,|s|s.polls),state.map_or(0,|s|s.reports),state.map_or(0,|s|s.errors),state.and_then(|s|s.error)));
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
                return reply(format_args!("OK display pclk_requested_hz={} pclk_applied_hz={} ready={} failed={} backlight={} enabled={} mode={} stall={} frames={} frame_age_ms={} owner_age_ms={} render_max_ms={} transfer_max_ms={} stack_free_bytes={} psram_bytes={} internal_free_bytes={} blanked_at_ms={} source=sim renderer=altimeter shown_quality={} shown_alt_ft={:?}",s.requested_pclk.load(Ordering::Acquire),s.applied_pclk.load(Ordering::Acquire),s.ready.load(Ordering::Acquire),s.failed.load(Ordering::Acquire),s.light.load(Ordering::Acquire),s.enabled.load(Ordering::Acquire),if s.pattern.load(Ordering::Acquire){"pattern"}else{"gauge"},s.stall.load(Ordering::Acquire),s.frames.load(Ordering::Acquire),now().wrapping_sub(s.presented.load(Ordering::Acquire)),now().wrapping_sub(s.owner.load(Ordering::Acquire)),s.render_max.load(Ordering::Acquire),s.transfer_max.load(Ordering::Acquire),s.stack_min.load(Ordering::Acquire),unsafe{sys::esp_psram_get_size()},unsafe{sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL)},s.blanked_at.load(Ordering::Acquire),["valid","missing","stale","failed","timing_fault","placeholder","pattern"][s.shown_quality.load(Ordering::Acquire) as usize],if s.shown_quality.load(Ordering::Acquire)==0 {Some(f32::from_bits(s.shown_altitude.load(Ordering::Acquire)))}else{None}));
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
fn run_display(s: &Shared) -> io::Result<()> {
    let mut cfg = sys::esp_lcd_rgb_panel_config_t::default();
    cfg.clk_src = sys::soc_periph_lcd_clk_src_t_LCD_CLK_SRC_DEFAULT;
    cfg.data_width = 16;
    cfg.bits_per_pixel = 16;
    cfg.num_fbs = 3;
    cfg.bounce_buffer_size_px = 480 * 10;
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
    let mut renderer = render::CachedRenderer::new(backdrop, 480, 480)
        .ok_or_else(|| io::Error::other("LCD backdrop size"))?;
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
    check(unsafe {
        sys::esp_lcd_rgb_panel_register_event_callbacks(panel, &cbs, wake_handle.cast())
    })?;
    check(unsafe { sys::esp_lcd_panel_reset(panel) })?;
    check(unsafe { sys::esp_lcd_panel_init(panel) })?;
    let mut applied_pclk = 18_000_000;
    s.applied_pclk.store(applied_pclk, Ordering::Release);
    let mut histories =
        core::array::from_fn::<_, 3, _>(|_| instrument_ui::damage::FrameHistory::new());
    let mut pipeline = FramePipeline::<RenderedFrame>::new();
    let mut idle_budget = Instant::now();
    loop {
        // Completed transfers can overlap all CPU work. Independently budget
        // idle time even when no completion wait blocks; keep both idle-task
        // watchdog subscriptions enabled. Never spin continuously on the CPU.
        if idle_budget.elapsed() >= Duration::from_millis(100) {
            thread::sleep(Duration::from_millis(10));
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
            if !renderer.draw_incremental(pixels, &mut histories[back], snapshot, pattern) {
                return Err(io::Error::other("frame size"));
            }
        }
        s.render_max
            .fetch_max(started.elapsed().as_millis() as u32, Ordering::Relaxed);
        while s.stall.load(Ordering::Acquire) == 2 {
            thread::sleep(Duration::from_millis(20));
        }
        // Retire the previous frame only after two POST-submission completions.
        // The next frame has already been drawn in independent storage. Queue
        // waits wake immediately from ISR, removing 10 ms polling quantization.
        while pipeline.is_pending() {
            if let Some((frame, elapsed)) =
                pipeline
                    .poll(COMPLETE.load(Ordering::Acquire), now())
                    .map_err(|_| io::Error::other("LCD completion timeout"))?
            {
                record_presentation(s, frame, elapsed);
            } else {
                wake.recv_front(TickType::new_millis(10).ticks());
            }
        }
        // Aging includes drawing AND waiting for the earlier frame. Discard an
        // expired prepared frame; never refresh its source timestamp to keep it.
        if lifetime.is_some_and(|ttl| now().wrapping_sub(stamp) as u64 >= ttl) {
            continue;
        }
        check(unsafe { sys::esp_lcd_panel_draw_bitmap(panel, 0, 0, 480, 480, buffers[back]) })?;
        let after_submit = COMPLETE.load(Ordering::Acquire);
        pipeline
            .submitted(
                after_submit,
                now(),
                RenderedFrame {
                    snapshot,
                    pattern,
                    stamp,
                    lifetime,
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
    lifetime: Option<u64>,
}

fn record_presentation(s: &Shared, frame: RenderedFrame, elapsed: u32) {
    let RenderedFrame {
        snapshot,
        pattern,
        stamp,
        lifetime,
    } = frame;
    s.transfer_max.fetch_max(elapsed, Ordering::Relaxed);
    let deadline = lifetime
        .map(|ttl| stamp.saturating_add(ttl as u32))
        .unwrap_or(u32::MAX);
    let model = instrument_ui::altimeter::model(snapshot.altitude, snapshot.baro_inhg);
    let quality = if pattern {
        6
    } else if snapshot.selected != instrument_core::Instrument::Altimeter {
        5
    } else {
        match model.quality {
            instrument_core::Quality::Valid => 0,
            instrument_core::Quality::Missing => 1,
            instrument_core::Quality::Stale => 2,
            instrument_core::Quality::Failed => 3,
            instrument_core::Quality::TimingFault => 4,
        }
    };
    s.shown_altitude.store(
        model.indicated_ft.unwrap_or(0.).to_bits(),
        Ordering::Release,
    );
    s.shown_quality.store(quality, Ordering::Release);
    s.shown_editor
        .store(quality == 0 && snapshot.editor.is_some(), Ordering::Release);
    s.deadline.store(deadline, Ordering::Release);
    s.presented.store(now(), Ordering::Release);
    s.frames.fetch_add(1, Ordering::Release);
    s.ready.store(true, Ordering::Release);
    s.stack_min.fetch_min(
        unsafe { sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut()) },
        Ordering::Relaxed,
    );
}
