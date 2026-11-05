use crate::{
    raster::{rgb, Canvas, Target},
    Snapshot,
};
use core::fmt::{self, Write};
use instrument_core::Instrument;
const WHITE: u16 = rgb(238, 240, 230);
const WARN: u16 = rgb(255, 191, 70);
const RED: u16 = rgb(255, 85, 85);
const HAND_EDGE: u16 = rgb(255, 128, 0);
const ALTITUDE_PANEL: u16 = rgb(190, 190, 190);
// Logical 480-unit face geometry; Canvas scales these for each display size.
const DIAL_NUMBER_RADIUS: f32 = 169.;
const KOLLSMAN_TOP: f32 = 211.;
const KOLLSMAN_PRESSED: u16 = rgb(0, 90, 156);
const NAVIGATION_BACKGROUND: u16 = rgb(9, 14, 19);
const PANEL_PIXELS: usize = 320 * 288;
// Pack both pages into the existing bitmap. Coordinates remain in the common
// 480-unit viewport; no extra framebuffer or per-frame allocation is needed.
struct Panel<'a>(&'a mut [u16]);
impl Target for Panel<'_> {
    fn size(&self)->(u32,u32){(480,480)}
    fn pixel(&mut self,x:i32,y:i32,color:u16){
        if (80..400).contains(&x)&&(96..384).contains(&y){self.0[(y-96)as usize*320+(x-80)as usize]=color;}
    }
    fn span(&mut self,y:i32,start:i32,end:i32,color:u16){
        let lo=start.clamp(80,400);let hi=end.clamp(80,400);
        if (96..384).contains(&y)&&lo<hi {
            let base=(y-96)as usize*320;self.0[base+(lo-80)as usize..base+(hi-80)as usize].fill(color);
        }
    }
}
/// One caller-owned CPU-only menu bitmap shared across any number of faces.
/// Its context key includes the selected gauge, provider session and source mode.
/// No allocation, sampled data or DMA ownership is retained here.
pub struct NavigationCache<'a> {
    pixels: &'a mut [u16],
    prepared: u8,
    key: Option<(crate::navigation::Context, Option<crate::navigation::Button>)>,
    info_key: Option<instrument_core::device::Info>,
    info_prepared: u8,
    info_pressed: bool,
}
impl<'a> NavigationCache<'a> {
    pub fn new(pixels: &'a mut [u16]) -> Option<Self> {
        (pixels.len() == 480 * 480).then_some(Self { pixels, prepared: 0, key: None, info_key:None,info_prepared:0,info_pressed:false })
    }
    /// Warm CPU-only artwork before the first visible frame. This does not
    /// publish pixels or renew any measurement's presentation deadline.
    pub fn prepare_background(&mut self, snapshot: Snapshot) {
        for _ in 0..13 { self.prepare(snapshot); }
    }
    // Spread cold menu/info construction over thirteen normal frames, before the
    // user can finish the hold. Never publish a partially prepared bitmap.
    fn prepare(&mut self, snapshot: Snapshot) {
        let cache = &mut self.pixels[..PANEL_PIXELS];
        let context = snapshot.menu.map_or(
            crate::navigation::Context {
                selected: snapshot.selected,
                external: snapshot.external,
                external_attitude: snapshot.external_attitude,
                imu: snapshot.imu_bench,
                session: snapshot.source_session,
            },
            |v| v.context,
        );
        if self.key.is_none_or(|(old, _)| old != context) {
            // Selection is an inexpensive outline on the output frame. Reuse
            // all cached lettering when only the gauge/session changes; retain
            // pressed state until the next menu view restores that button.
            if let Some((old, pressed)) = self.key {
                if self.prepared == 9 && crate::navigation::GAUGES.into_iter().all(|g|
                    old.enabled(g) == context.enabled(g) && old.reason(g) == context.reason(g)) {
                    self.key = Some((context, pressed));
                    self.prepare_info(snapshot);
                    return;
                }
            }
            // The panel/title never depend on context. Retain them across
            // switches: repainting their large PSRAM area costs a full frame.
            let panel_ready = self.key.is_some();
            self.key = Some((context, None));
            self.prepared = u8::from(panel_ready);
        }
        if self.prepared < 9 {
            let mut fb = Panel(&mut *cache);
            let mut c = Canvas::new(&mut fb);
            if self.prepared == 0 {
                draw_navigation_panel(&mut c);
            } else {
                let button = crate::navigation::GAUGES
                    .get(self.prepared as usize - 1)
                    .copied()
                    .map(crate::navigation::Button::Gauge)
                    .unwrap_or(if self.prepared==7 {crate::navigation::Button::Info}else{crate::navigation::Button::Cancel});
                draw_navigation_cached_button(
                    &mut c,
                    crate::navigation::View {
                        info: false,
                        context,
                        generation: 0,
                        pressed: None,
                    },
                    button,
                );
            }
            self.prepared += 1;
        } else {
            self.prepare_info(snapshot);
        }
    }
    fn prepare_info(&mut self,snapshot:Snapshot) {
        if self.info_key!=Some(snapshot.device){self.info_key=Some(snapshot.device);self.info_prepared=0;self.info_pressed=false;}
        if self.info_prepared<4 {
            let mut fb=Panel(&mut self.pixels[PANEL_PIXELS..2*PANEL_PIXELS]);
            draw_info_part(&mut Canvas::new(&mut fb),snapshot.device,self.info_prepared,false);
            self.info_prepared+=1;
        }
    }
    fn navigation(
        &mut self,
        pixels: &mut [u16],
        history: &mut crate::damage::FrameHistory,
        snapshot: Snapshot,
    ) {
        if snapshot.menu.is_some() {
            self.prepare(snapshot);
            while self.prepared < 9 || (snapshot.menu.is_some_and(|m|m.info) && (self.info_prepared<4||self.info_key!=Some(snapshot.device))) {
                self.prepare(snapshot);
            }
        }
        if let Some(menu) = snapshot.menu {
            let info=menu.info;
            let cache=if info {&mut self.pixels[PANEL_PIXELS..2*PANEL_PIXELS]}else{&mut self.pixels[..PANEL_PIXELS]};
            let key = (menu.context, menu.pressed);
            if info {
                let pressed=menu.pressed==Some(crate::navigation::Button::Back);
                if self.info_pressed!=pressed {
                    let mut fb=Panel(&mut *cache);
                    draw_info_part(&mut Canvas::new(&mut fb),snapshot.device,3,pressed);
                    self.info_pressed=pressed;
                }
            } else if self.key != Some(key) {
                let mut fb = Panel(&mut *cache);
                let mut c = Canvas::new(&mut fb);
                // Only the old and new pressed buttons change. Rebuilding all
                // lettering on each finger transition exceeds attitude's budget.
                if let Some(old) = self.key.and_then(|(_, pressed)| pressed) {
                    draw_navigation_cached_button(&mut c, menu, old);
                }
                if let Some(new) = menu.pressed {
                    draw_navigation_cached_button(&mut c, menu, new);
                }
                self.key = Some(key);
            }
            // The entire panel is opaque. Copy only initialized pixels, then
            // mark them so the moving horizon restores them when it closes.
            for y in 96..384 {
                let row = y * 480 + 80..y * 480 + 400;
                let start=(y-96)*320;
                pixels[row].copy_from_slice(&cache[start..start+320]);
            }
            let mut fb = crate::raster::Framebuffer::new(pixels, 480, 480).unwrap();
            if !info {draw_navigation_selection(&mut Canvas::new(&mut fb), menu.context.selected);}
            history.mark_rect(
                480,
                crate::interaction::Rect {
                    x: 80,
                    y: 96,
                    w: 320,
                    h: 288,
                },
            );
        }
    }
}
/// Optional caller-owned backdrop cache. No allocation or hardware dependence;
/// every output frame is restored completely before drawing dynamic content.
pub struct CachedRenderer<'a> {
    background: &'a mut [u16],
    width: u32,
    height: u32,
    key: Option<(Instrument, bool, &'static str)>,
    foreground_runs: Option<&'a mut [crate::attitude::CopyRun]>,
    foreground_count: Option<usize>,
    foreground_pixels: Option<&'a mut [u16]>,
    compact_foreground: bool,
    digit_offset: Option<usize>,
    navigation_cache: Option<NavigationCache<'a>>,
}
impl<'a> CachedRenderer<'a> {
    pub fn new(background: &'a mut [u16], width: u32, height: u32) -> Option<Self> {
        crate::raster::Framebuffer::new(background, width, height)?;
        Some(Self {
            background,
            width,
            height,
            key: None,
            foreground_runs: None,
            foreground_count: None,
            foreground_pixels: None,
            compact_foreground: false,
            digit_offset: None,
            navigation_cache: None,
        })
    }
    pub fn with_foreground_cache(
        background: &'a mut [u16],
        width: u32,
        height: u32,
        runs: &'a mut [crate::attitude::CopyRun],
    ) -> Option<Self> {
        let mut renderer = Self::new(background, width, height)?;
        renderer.foreground_runs = Some(runs);
        Some(renderer)
    }
    /// Optional fast-memory storage for only the opaque foreground pixels.
    /// Insufficient capacity falls back to the full backdrop without data loss.
    pub fn with_foreground_pixels(mut self, pixels: &'a mut [u16]) -> Self {
        self.foreground_pixels = Some(pixels);
        self
    }
    /// Prepare only static artwork in CPU-owned storage. No live frame is
    /// produced and no measurement timestamp is consumed or refreshed.
    pub fn prepare_background(&mut self, snapshot: Snapshot) -> bool {
        if !self.prepare(snapshot, false) {
            return false;
        }
        if snapshot.selected == Instrument::Attitude {
            for _ in 0..8 {
                self.prepare_navigation(snapshot);
            }
        }
        true
    }
    /// Optional CPU-only menu bitmap for the 480-pixel face. Never DMA-owned.
    /// Other dimensions/capacities use the identical uncached drawing path.
    pub fn with_navigation_pixels(mut self, pixels: &'a mut [u16]) -> Self {
        if self.width == 480 && self.height == 480 && pixels.len() == 480 * 480 {
            self.navigation_cache = NavigationCache::new(pixels);
        }
        self
    }
    fn prepare_navigation(&mut self, snapshot: Snapshot) {
        if let Some(cache) = self.navigation_cache.as_mut() { cache.prepare(snapshot); }
    }
    fn navigation(
        cache: Option<&mut NavigationCache<'_>>,
        size: (u32, u32),
        pixels: &mut [u16],
        history: &mut crate::damage::FrameHistory,
        snapshot: Snapshot,
    ) {
        if let Some(cache) = cache.filter(|_| snapshot.menu.is_some()) {
            cache.navigation(pixels, history, snapshot);
        } else {
            let fb = crate::raster::Framebuffer::new(pixels, size.0, size.1).unwrap();
            draw_navigation(&mut crate::damage::Tracked {
                last_pixel_tile: u16::MAX, target: fb, history,
            }, snapshot);
        }
    }
    /// Draw using a menu cache shared by all faces in the calling display task.
    pub fn draw_incremental_with_menu(
        &mut self, pixels: &mut [u16], history: &mut crate::damage::FrameHistory,
        snapshot: Snapshot, menu: &mut NavigationCache<'_>, progress: &mut dyn FnMut(),
    ) -> bool {
        let menu = (self.width == 480 && self.height == 480).then_some(menu);
        self.draw_inner(pixels, Some(history), snapshot, false, progress, menu)
    }
    pub fn draw(&mut self, pixels: &mut [u16], snapshot: Snapshot, pattern: bool) -> bool {
        self.draw_inner(pixels, None, snapshot, pattern, &mut || {}, None)
    }
    /// History belongs exclusively to this pixel buffer. A new history forces
    /// complete initialization; layout/view changes force full restore.
    pub fn draw_incremental(
        &mut self,
        pixels: &mut [u16],
        history: &mut crate::damage::FrameHistory,
        snapshot: Snapshot,
        pattern: bool,
    ) -> bool {
        self.draw_inner(pixels, Some(history), snapshot, pattern, &mut || {}, None)
    }
    /// Bounded progress notifications let a backend retire earlier transfers
    /// while drawing into independent storage. The callback receives no pixels
    /// or mutable gauge state, and must not block.
    pub fn draw_incremental_with_progress(
        &mut self,
        pixels: &mut [u16],
        history: &mut crate::damage::FrameHistory,
        snapshot: Snapshot,
        pattern: bool,
        progress: &mut dyn FnMut(),
    ) -> bool {
        self.draw_inner(pixels, Some(history), snapshot, pattern, progress, None)
    }
    fn prepare(&mut self, snapshot: Snapshot, pattern: bool) -> bool {
        let key = (
            snapshot.selected,
            pattern,
            if snapshot.selected == Instrument::Attitude {
                snapshot.source_label
            } else if snapshot.airspeed_demo_limits() {
                "C172S DEMO"
            } else {
                ""
            },
        );
        if self.key != Some(key) {
            if self.width == 480 && self.height == 480 && !pattern
                && snapshot.selected == Instrument::Attitude
                && self.foreground_count.is_some()
                && self.key.is_some_and(|(view, pattern, old)|
                    view == Instrument::Attitude && !pattern && old.len() <= 12)
                && snapshot.source_label.len() <= 12
            {
                // A source change only changes this opaque caption. Rescanning
                // the entire face here used over 100 ms of a live sample's lease.
                crate::attitude::refresh_caption(self.background, snapshot.source_label);
                if self.compact_foreground {
                    let mut offset = 0;
                    let pixels = self.foreground_pixels.as_deref_mut().unwrap();
                    let runs = self.foreground_runs.as_deref().unwrap();
                    for run in &runs[..self.foreground_count.unwrap()] {
                        let start = run.start as usize;
                        let len = run.len as usize;
                        if (362..435).contains(&(start / 480)) {
                            pixels[offset..offset + len].copy_from_slice(&self.background[start..start + len]);
                        }
                        offset += len;
                    }
                }
                self.key = Some(key);
                return true;
            }
            let Some(mut fb) =
                crate::raster::Framebuffer::new(self.background, self.width, self.height)
            else {
                return false;
            };
            if pattern {
                test_pattern(&mut fb);
            } else {
                draw_background(&mut fb, snapshot);
            }
            self.key = Some(key);
            self.foreground_count = None;
            self.compact_foreground = false;
            self.digit_offset = None;
            if snapshot.selected == Instrument::Attitude && !pattern {
                if let Some(runs) = self.foreground_runs.as_deref_mut() {
                    self.foreground_count = crate::attitude::cache_foreground(
                        self.background,
                        self.width,
                        self.height,
                        snapshot.source_label,
                        runs,
                    );
                }
                if let (Some(runs), Some(count), Some(pixels)) = (
                    self.foreground_runs.as_deref(),
                    self.foreground_count,
                    self.foreground_pixels.as_deref_mut(),
                ) {
                    let needed: usize = runs[..count].iter().map(|r| r.len as usize).sum();
                    if needed <= pixels.len() {
                        let mut offset = 0;
                        for run in &runs[..count] {
                            let len = run.len as usize;
                            let start = run.start as usize;
                            pixels[offset..offset + len]
                                .copy_from_slice(&self.background[start..start + len]);
                            offset += len;
                        }
                        self.compact_foreground = true;
                        if pixels.len() - offset >= crate::raster::DIGIT_CACHE_LEN {
                            crate::raster::cache_ladder_digits(
                                &mut pixels[offset..],
                                crate::attitude::MARK,
                            );
                            self.digit_offset = Some(offset);
                        }
                    }
                }
            }
        }
        true
    }
    fn draw_inner(
        &mut self,
        pixels: &mut [u16],
        history: Option<&mut crate::damage::FrameHistory>,
        snapshot: Snapshot,
        pattern: bool,
        progress: &mut dyn FnMut(),
        mut menu: Option<&mut NavigationCache<'_>>,
    ) -> bool {
        if pixels.len() != self.background.len() {
            return false;
        }
        if !pattern && snapshot.menu.is_some() {
            // No flight indication is left visible behind navigation. This
            // keeps menu presentation independent of numeric sample expiry.
            if let Some(history) = history {
                // Each DMA buffer retains its own complete opaque menu. A
                // static menu needs no PSRAM writes while source data changes.
                // Repainting every frame competes with scanout and touch I/O.
                if history.menu == snapshot.menu {
                    progress();
                    return true;
                }
                if history.menu.is_none() {
                    pixels.fill(NAVIGATION_BACKGROUND);
                }
                history.clear();
                history.key = None; // Closing must initialize the whole face.
                if let Some(cache) = menu {
                    Self::navigation(Some(cache), (self.width, self.height), pixels, history, snapshot);
                } else {
                    Self::navigation(self.navigation_cache.as_mut(), (self.width, self.height), pixels, history, snapshot);
                }
                history.menu = snapshot.menu;
            } else {
                pixels.fill(NAVIGATION_BACKGROUND);
                let mut fb = crate::raster::Framebuffer::new(pixels, self.width, self.height).unwrap();
                draw_navigation(&mut fb, snapshot);
            }
            progress();
            return true;
        }
        if !self.prepare(snapshot, pattern) {
            return false;
        }
        if !pattern {
            if let Some(cache) = menu.as_deref_mut() { cache.prepare(snapshot); }
            else { self.prepare_navigation(snapshot); }
        }
        if let Some(history) = history {
            let frame_key = (self.width, self.height, snapshot.selected, pattern, snapshot.airspeed_demo_limits());
            if history.key == Some(frame_key)
                && snapshot.selected == Instrument::Attitude
                && !pattern
            {
                // Attitude restores prior markings from its moving horizon, not
                // the static backdrop. Its history belongs to this buffer.
            } else if history.key == Some(frame_key) {
                history.restore(
                    pixels,
                    self.background,
                    self.width as usize,
                    self.height as usize,
                );
            } else {
                if snapshot.selected == Instrument::Attitude && !pattern {
                    // The horizon (or failure face) paints every interior pixel.
                    // Avoid reading/copying that entire PSRAM area twice on a switch.
                    crate::attitude::restore_outer(
                        pixels,
                        self.background,
                        self.width,
                        self.height,
                    );
                } else {
                    pixels.copy_from_slice(self.background);
                }
                history.clear();
                history.key = Some(frame_key);
            }
            if !pattern {
                let Some(fb) = crate::raster::Framebuffer::new(pixels, self.width, self.height)
                else {
                    return false;
                };
                if snapshot.selected == Instrument::Attitude {
                    let _ = fb;
                    crate::attitude::incremental(
                        pixels,
                        (self.width, self.height),
                        history,
                        snapshot,
                        self.foreground_runs
                            .as_deref()
                            .zip(self.foreground_count)
                            .map(|(runs, count)| {
                                (
                                    if self.compact_foreground {
                                        self.foreground_pixels.as_deref().unwrap()
                                    } else {
                                        &*self.background
                                    },
                                    &runs[..count],
                                    self.compact_foreground,
                                )
                            }),
                        self.foreground_pixels
                            .as_deref()
                            .zip(self.digit_offset)
                            .map(|(pixels, offset)| {
                                &pixels[offset..offset + crate::raster::DIGIT_CACHE_LEN]
                            }),
                        progress,
                    );
                } else {
                    draw_gauge_overlay(
                        &mut crate::damage::Tracked {
                            last_pixel_tile: u16::MAX,
                            target: fb,
                            history,
                        },
                        snapshot,
                    );
                }
                if let Some(cache) = menu {
                    Self::navigation(Some(cache), (self.width, self.height), pixels, history, snapshot);
                } else {
                    Self::navigation(self.navigation_cache.as_mut(), (self.width, self.height), pixels, history, snapshot);
                }
            }
            return true;
        }
        pixels.copy_from_slice(self.background);
        if !pattern {
            let Some(mut fb) = crate::raster::Framebuffer::new(pixels, self.width, self.height)
            else {
                return false;
            };
            draw_overlay(&mut fb, snapshot);
        }
        true
    }
}
/// Concrete entry point keeps raster monomorphization inside the UI crate,
/// allowing platforms to optimize it separately from control and FFI code.
pub fn draw_frame(
    pixels: &mut [u16],
    width: u32,
    height: u32,
    snapshot: Snapshot,
    pattern: bool,
) -> bool {
    let Some(mut fb) = crate::raster::Framebuffer::new(pixels, width, height) else {
        return false;
    };
    if pattern {
        test_pattern(&mut fb);
    } else {
        draw(&mut fb, snapshot);
    }
    true
}
pub(crate) struct Text {
    bytes: [u8; 48],
    len: usize,
}
impl Text {
    pub(crate) fn new() -> Self {
        Self {
            bytes: [0; 48],
            len: 0,
        }
    }
    pub(crate) fn str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("TEXT ERROR")
    }
}
impl Write for Text {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.len + s.len() > 48 {
            return Err(fmt::Error);
        }
        self.bytes[self.len..self.len + s.len()].copy_from_slice(s.as_bytes());
        self.len += s.len();
        Ok(())
    }
}
fn point(radius: f32, deg: f32) -> (f32, f32) {
    let a = deg * core::f32::consts::PI / 180.;
    (240. + radius * libm::sinf(a), 240. - radius * libm::cosf(a))
}
fn hand<T: Target>(
    c: &mut Canvas<T>,
    points: &[(f32, f32)],
    angle: f32,
    color: u16,
    edge: Option<u16>,
) {
    let a = angle * core::f32::consts::PI / 180.;
    let (s, co) = (libm::sinf(a), libm::cosf(a));
    let mut out = [(0., 0.); 8];
    for (i, &(x, y)) in points.iter().enumerate() {
        let (x, y) = (x - 240., y - 240.);
        out[i] = (240. + x * co - y * s, 240. + x * s + y * co);
    }
    c.polygon(&out[..points.len()], color);
    if let Some(edge) = edge {
        for i in 0..points.len() {
            c.stroke(out[i], out[(i + 1) % points.len()], 2., edge);
        }
    }
}
pub fn draw<T: Target>(target: &mut T, s: Snapshot) {
    if s.menu.is_some() {
        let (w, h) = target.size();
        for y in 0..h as i32 { target.span(y, 0, w as i32, NAVIGATION_BACKGROUND); }
        draw_navigation(target, s);
        return;
    }
    draw_background(target, s);
    draw_overlay(target, s);
}
fn draw_background<T: Target>(target: &mut T, s: Snapshot) {
    let (w, h) = target.size();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            target.pixel(x, y, 0);
        }
    }
    let mut c = Canvas::new(target);
    c.circle(240., 240., 238., rgb(70, 82, 93));
    c.circle(240., 240., 229., rgb(9, 14, 19));
    if s.selected == Instrument::Attitude {
        return;
    }
    if s.selected == Instrument::Turn {
        crate::turn::background(target);
        return;
    }
    if s.selected == Instrument::Airspeed {
        crate::airspeed::background(target, s.airspeed_demo_limits());
        return;
    }
    if s.selected==Instrument::Heading {crate::heading::background(target);return;}
    if s.selected==Instrument::Vsi {crate::vsi::background(target);return;}
    if s.selected != Instrument::Altimeter {
        c.text(240., 180., 25., "SIMULATION", WARN);
        let mut name = Text::new();
        for ch in s.selected.name().bytes() {
            let _ = name.write_char(ch.to_ascii_uppercase() as char);
        }
        c.text(240., 230., 28., name.str(), WHITE);
        c.text(240., 280., 18., "NOT IMPLEMENTED", RED);
        return;
    }
    for i in 0..50 {
        let major = i % 5 == 0;
        // esp rustc 1.97.0's -Oz backend cannot select the float lookup table
        // synthesized for these choices. Keep the exact integer dimensions
        // opaque until conversion, avoiding that compiler crash without changing
        // geometry or weakening checks. Covered by host renders and board build.
        let radius = core::hint::black_box(if major { 199u32 } else { 210u32 }) as f32;
        let width = core::hint::black_box(if major { 4u32 } else { 2u32 }) as f32;
        c.line(
            point(radius, i as f32 * 7.2),
            point(221., i as f32 * 7.2),
            width,
            WHITE,
        );
    }
    for i in 0..10 {
        let (x, y) = point(DIAL_NUMBER_RADIUS, i as f32 * 36.);
        let mut n = Text::new();
        let _ = write!(n, "{i}");
        c.text(x, y, 46., n.str(), WHITE);
    }
    c.text(240., 142., 25., "ALT", WHITE);
    c.text(240., 171., 18., "100 FEET", rgb(175, 185, 192));
    // Widen toward the hub for larger digits, reserving the gap before the
    // right-hand tick marks for the external units label.
    c.rect(270., KOLLSMAN_TOP, 136., 42., rgb(54, 63, 69));
    c.text_bold_on(428., KOLLSMAN_TOP + 24., 22., "HG", (WHITE, rgb(9, 14, 19)));
}
fn draw_kollsman_value<T: Target>(c: &mut Canvas<T>, baro: f32, background: u16) {
    c.rect(273., KOLLSMAN_TOP + 3., 130., 36., background);
    let mut b = Text::new();
    if baro.is_finite() && (28.1..=31.).contains(&baro) {
        let _ = write!(b, "{baro:.2}");
    } else {
        let _ = b.write_str("--.--");
    }
    c.text_on(338., KOLLSMAN_TOP + 24., 32., b.str(), (WHITE, background));
}
fn draw_overlay<T: Target>(target: &mut T, s: Snapshot) {
    draw_gauge_overlay(target, s);
    draw_navigation(target, s);
}
fn draw_gauge_overlay<T: Target>(target: &mut T, s: Snapshot) {
    if s.selected==Instrument::Heading {crate::heading::draw(target,s.heading_model(),s.source_label);return;}
    if s.selected==Instrument::Vsi {crate::vsi::draw(target,s.vsi_model(),s.source_label);return;}
    if s.selected == Instrument::Airspeed {
        crate::airspeed::draw(target, s.airspeed_model(), s.source_label);
        return;
    }
    if s.selected == Instrument::Turn {
        crate::turn::draw(target, s.turn_model(), s.source_label);
        return;
    }
    if s.selected == Instrument::Attitude {
        if s.imu_bench && s.attitude_model().angles.is_none() {
            crate::attitude::waiting(target);
        } else {
            crate::attitude::draw(target, s.attitude_model(), s.source_label);
        }
        return;
    }
    if s.selected != Instrument::Altimeter {
        return;
    }
    let mut c = Canvas::new(target);
    // Barometer digits belong to dynamic damage tracking, not the cached face.
    draw_kollsman_value(
        &mut c,
        s.baro_inhg,
        if s.kollsman_pressed && s.editor.is_none() && s.model().hands_deg.is_some() {
            KOLLSMAN_PRESSED
        } else {
            0
        },
    );
    let m = s.model();
    if let Some([hundreds, thousands, tenthousands]) = m.hands_deg {
        c.line(
            point(-4., tenthousands),
            point(185., tenthousands),
            2.,
            rgb(195, 203, 208),
        );
        hand(
            &mut c,
            &[(231., 41.), (249., 41.), (240., 57.)],
            tenthousands,
            WHITE,
            None,
        );
        hand(
            &mut c,
            &[
                (232., 255.),
                (229., 162.),
                (240., 133.),
                (251., 162.),
                (248., 255.),
            ],
            thousands,
            rgb(226, 231, 230),
            Some(HAND_EDGE),
        );
        hand(
            &mut c,
            &[
                (237., 230.),
                (237., 167.),
                (240., 152.),
                (243., 167.),
                (243., 230.),
            ],
            thousands,
            rgb(137, 149, 158),
            None,
        );
        hand(
            &mut c,
            &[
                (235., 263.),
                (236., 105.),
                (240., 76.),
                (244., 105.),
                (245., 263.),
            ],
            hundreds,
            WHITE,
            Some(HAND_EDGE),
        );
        c.circle(240., 240., 17., rgb(85, 98, 110));
        c.circle(240., 240., 15., rgb(16, 23, 29));
        c.circle(240., 240., 5., rgb(60, 72, 81));
        let mut a = Text::new();
        let _ = write!(a, "{:.0} FT", m.indicated_ft.unwrap_or(0.));
        // Eight cells cover the full supported signed/baro-corrected range.
        // Keep padding around the larger readout and clearance below Kollsman.
        c.rect(124., 284., 232., 40., ALTITUDE_PANEL);
        c.text_bold_on(240., 307., 32., a.str(), (0, ALTITUDE_PANEL));
    } else {
        // The cross and banner share the dial center. The font's ink sits
        // above its cell center, so lower the lettering three pixels.
        c.line((169.5, 190.), (310.5, 290.), 7., RED);
        c.line((310.5, 190.), (169.5, 290.), 7., RED);
        c.rect(143., 215., 194., 50., rgb(25, 15, 18));
        c.text(240., 243., 24., "ALT DATA", RED);
        let mut q = Text::new();
        for ch in m.quality.name().bytes() {
            let _ = q.write_char(ch.to_ascii_uppercase() as char);
        }
        c.text(240., 306., 19., q.str(), RED);
    }
    c.text(240., 351., 18., s.source_label, WARN);
    if m.indicated_ft.is_some() {
        if let Some(editor) = s.editor {
            draw_editor(&mut c, editor, s.external);
        }
    }
}

fn draw_navigation<T: Target>(target: &mut T, s: Snapshot) {
    use crate::navigation::{Button, GAUGES};
    let mut c = Canvas::new(target);
    if s.center_pressed && s.menu.is_none() {
        c.circle(240., 240., 24., KOLLSMAN_PRESSED);
        c.text_on(240., 240., 12., "MENU", (WHITE, KOLLSMAN_PRESSED));
    }
    let Some(menu) = s.menu else {
        return;
    };
    if menu.info {
        for part in 0..4 {draw_info_part(&mut c,s.device,part,menu.pressed==Some(Button::Back));}
        return;
    }
    draw_navigation_panel(&mut c);
    for b in GAUGES
        .into_iter()
        .map(Button::Gauge)
        .chain([Button::Info,Button::Cancel])
    {
        draw_navigation_button(&mut c, menu, b);
    }
}
fn draw_navigation_panel<T: Target>(c: &mut Canvas<T>) {
    let panel = rgb(22, 29, 36);
    c.rect(80., 96., 320., 288., rgb(145, 155, 165));
    c.rect(83., 99., 314., 282., panel);
    c.text_on(240., 117., 21., "SELECT GAUGE", (WHITE, panel));
}
fn draw_info_part<T:Target>(c:&mut Canvas<T>,info:instrument_core::device::Info,part:u8,pressed:bool){
    let panel=rgb(22,29,36);
    match part {
        0=>{
            c.rect(80.,96.,320.,288.,rgb(145,155,165));c.rect(83.,99.,314.,282.,panel);
            c.text_on(240.,117.,21.,"DEVICE INFO",(WHITE,panel));
        }
        1=>{
            c.text_on(240.,158.,15.,"FIRMWARE",(WARN,panel));
            // The instrument font contains uppercase glyphs. 24 cells at 14px
            // fit inside the 320px panel, including its border/padding.
            let mut revision=Text::new();
            for ch in info.firmware.bytes(){let _=revision.write_char(ch.to_ascii_uppercase() as char);}
            for (line,chunk) in revision.str().as_bytes().chunks(24).take(2).enumerate(){
                if let Ok(text)=core::str::from_utf8(chunk){c.text_on(240.,183.+line as f32*19.,14.,text,(WHITE,panel));}
            }
        }
        2=>{
            c.text_on(240.,237.,15.,"DEVICE UUID",(WARN,panel));
            let mut id=Text::new();
            if let Some(uuid)=info.uuid{let _=write!(id,"{uuid}");}else{let _=write!(id,"UNAVAILABLE");}
            let mut canonical=Text::new();
            for ch in id.str().bytes(){let _=canonical.write_char(ch.to_ascii_uppercase() as char);}
            for (line,chunk) in canonical.str().as_bytes().chunks(18).enumerate(){
                if let Ok(text)=core::str::from_utf8(chunk){c.text_on(240.,265.+line as f32*21.,17.,text,(WHITE,panel));}
            }
        }
        _=>{
            let r=crate::navigation::Button::Back.rect();
            let bg=if pressed{KOLLSMAN_PRESSED}else{rgb(66,80,92)};
            c.rect(r.x as f32,r.y as f32,r.w as f32,r.h as f32,bg);
            c.text_on(240.,349.,18.,"BACK",(WHITE,bg));
        }
    }
}
fn draw_navigation_selection<T: Target>(c: &mut Canvas<T>, selected: Instrument) {
    let r = crate::navigation::Button::Gauge(selected).rect();
    let (x, y, w, h) = (r.x as f32, r.y as f32, r.w as f32, r.h as f32);
    c.rect(x, y, w, 2., WARN);
    c.rect(x, y + h - 2., w, 2., WARN);
    c.rect(x, y + 2., 2., h - 4., WARN);
    c.rect(x + w - 2., y + 2., 2., h - 4., WARN);
}
fn draw_navigation_cached_button<T: Target>(c: &mut Canvas<T>, menu: crate::navigation::View, b: crate::navigation::Button) {
    draw_navigation_button_inner(c, menu, b, false);
}
fn draw_navigation_button<T: Target>(c: &mut Canvas<T>, menu: crate::navigation::View, b: crate::navigation::Button) {
    draw_navigation_button_inner(c, menu, b, true);
}
fn draw_navigation_button_inner<T: Target>(
    c: &mut Canvas<T>, menu: crate::navigation::View, b: crate::navigation::Button,
    include_selection: bool,
) {
    use crate::navigation::Button;
    let (label, enabled, selected, reason) = match b {
        Button::Cancel => ("CANCEL", true, false, ""),
        Button::Info => ("INFO", true, false, ""),
        Button::Back => ("BACK", true, false, ""),
        Button::Gauge(i) => (
            match i {
                Instrument::Altimeter => "ALTIMETER",
                Instrument::Attitude => "ATTITUDE",
                Instrument::Airspeed => "AIRSPEED",
                Instrument::Heading => "HEADING",
                Instrument::Turn => "TURN COORD",
                Instrument::Vsi => "VERT SPEED",
            },
            menu.context.enabled(i),
            menu.context.selected == i,
            menu.context.reason(i),
        ),
    };
    let r = b.rect();
    let bg = if enabled && menu.pressed == Some(b) {
        KOLLSMAN_PRESSED
    } else if enabled {
        rgb(66, 80, 92)
    } else {
        rgb(33, 39, 45)
    };
    c.rect(
        r.x as f32,
        r.y as f32,
        r.w as f32,
        r.h as f32,
        if selected && include_selection { WARN } else { bg },
    );
    c.rect(
        r.x as f32 + 2.,
        r.y as f32 + 2.,
        r.w as f32 - 4.,
        r.h as f32 - 4.,
        bg,
    );
    let x = r.x as f32 + r.w as f32 / 2.;
    let y = r.y as f32 + r.h as f32 / 2.;
    c.text_on(
        x,
        y - if enabled { 0. } else { 8. },
        17.,
        label,
        (if enabled { WHITE } else { rgb(137, 147, 155) }, bg),
    );
    if !enabled {
        c.text_on(x, y + 12., 11., reason, (rgb(137, 147, 155), bg));
    }
}

fn draw_editor<T: Target>(
    c: &mut Canvas<T>,
    editor: crate::interaction::EditorView,
    external: bool,
) {
    use crate::interaction::Button;
    let panel = rgb(22, 29, 36);
    c.rect(80., 128., 320., 240., rgb(145, 155, 165));
    c.rect(83., 131., 314., 234., panel);
    c.text(240., 150., 21., "BARO IN HG", WHITE);
    let mut value = Text::new();
    let _ = write!(value, "{}.{:02}", editor.cents / 100, editor.cents % 100);
    c.text(240., 189., 38., value.str(), WHITE);
    for (button, label, size) in [
        (Button::Minus, "-", 36.),
        (Button::Standard, "29.92", 24.),
        (Button::Plus, "+", 36.),
        (Button::Cancel, "CANCEL", 19.),
        (Button::Apply, "APPLY", 19.),
    ] {
        let r = button.rect();
        let background = if editor.pressed == Some(button) {
            KOLLSMAN_PRESSED
        } else {
            rgb(66, 80, 92)
        };
        c.rect(r.x as f32, r.y as f32, r.w as f32, r.h as f32, background);
        c.text_on(
            r.x as f32 + r.w as f32 / 2.,
            r.y as f32 + r.h as f32 / 2.,
            size,
            label,
            (WHITE, background),
        );
    }
    c.text(
        240.,
        354.,
        14.,
        if external {
            "APPLY TO SIM"
        } else {
            "SIM INPUT"
        },
        WARN,
    );
}

pub fn test_pattern<T: Target>(target: &mut T) {
    let (w, h) = target.size();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            target.pixel(x, y, 0);
        }
    }
    let mut c = Canvas::new(target);
    c.circle(240., 240., 228., rgb(35, 35, 35));
    c.rect(120., 185., 80., 100., rgb(255, 0, 0));
    c.rect(200., 185., 80., 100., rgb(0, 255, 0));
    c.rect(280., 185., 80., 100., rgb(0, 0, 255));
    c.text(240., 100., 28., "TOP", WHITE);
    c.text(240., 365., 22., "RGB TEST", WHITE);
    c.line((240., 20.), (240., 65.), 4., WHITE);
    c.line((20., 240.), (65., 240.), 4., WHITE);
}

#[cfg(test)]
mod tests {
    #[test]
    fn navigation_view_has_no_live_numeric_pixels_or_numeric_lease() {
        use instrument_core::{App, Command};
        let mut app = App::new(); app.apply(Command::LevelPreset).unwrap(); app.tick(0);
        app.apply(Command::Select(Instrument::Attitude)).unwrap();
        let mut first = Snapshot::from_app(&app);
        first.menu = Some(crate::navigation::View { info: false, context:crate::navigation::Context::from_app(&app),generation:1,pressed:None });
        let mut second = first.aged(500);
        second.pitch.value = Some(-25.); second.roll.value = Some(65.);
        let mut a = std::vec![0;480*480];let mut b=a.clone();
        draw_frame(&mut a,480,480,first,false);draw_frame(&mut b,480,480,second,false);
        assert!(a == b,"navigation must not retain or expose stale flight indications");
        assert_eq!(first.numeric_lifetime_ms(),None);
        assert_eq!(second.pitch.age_ms,first.pitch.age_ms.map(|v|v+500));
    }
    #[test]
    fn static_menu_retains_each_buffer_through_source_updates_then_restores_the_face() {
        use instrument_core::{App, Command};
        use crate::navigation::{Button, Context, View};
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap(); app.tick(0);
        for gauge in crate::navigation::GAUGES {
            app.apply(Command::Select(gauge)).unwrap();
            let mut background = std::vec![0;480*480];
            let mut menu_pixels = background.clone();
            let mut renderer = CachedRenderer::new(&mut background,480,480).unwrap();
            let mut menu = NavigationCache::new(&mut menu_pixels).unwrap();
            let mut buffers = core::array::from_fn::<_,3,_>(|_| std::vec![0;480*480]);
            let mut histories = core::array::from_fn::<_,3,_>(|_| crate::damage::FrameHistory::new());
            let mut expected = std::vec![0;480*480];
            for n in 0..36 {
                let mut s = Snapshot::from_app(&app);
                s.pitch.value = Some(n as f32 - 18.);
                s.roll.value = Some(n as f32 * 3. - 50.);
                s.menu = (3..30).contains(&n).then_some(View { info: false,
                    context: Context::from_app(&app), generation: 1,
                    pressed: (12..21).contains(&n).then_some(Button::Cancel),
                });
                let b = n % 3;
                assert!(renderer.draw_incremental_with_menu(&mut buffers[b],&mut histories[b],s,&mut menu,&mut || {}));
                assert!(draw_frame(&mut expected,480,480,s,false));
                assert_eq!(buffers[b],expected,"{gauge:?} frame {n}, buffer {b}");
            }
        }
    }
    #[test]
    fn shared_menu_falls_back_on_other_display_sizes() {
        use instrument_core::{App, Command};
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.tick(0);
        let mut s = Snapshot::from_app(&app);
        s.menu = Some(crate::navigation::View { info: false,
            context: crate::navigation::Context::from_app(&app), generation: 1, pressed: None,
        });
        assert!(NavigationCache::new(&mut [0; 4]).is_none());
        let mut menu_pixels = std::vec![0xffff;480*480];
        let mut menu = NavigationCache::new(&mut menu_pixels).unwrap();
        for size in [64, 240] {
            let mut background = std::vec![0;size*size];
            let mut pixels = background.clone();
            let mut expected = background.clone();
            let mut renderer = CachedRenderer::new(&mut background, size as u32, size as u32).unwrap();
            let mut history = crate::damage::FrameHistory::new();
            assert!(renderer.draw_incremental_with_menu(&mut pixels, &mut history, s, &mut menu, &mut || {}));
            assert!(draw_frame(&mut expected, size as u32, size as u32, s, false));
            assert_eq!(pixels, expected);
        }
    }
    #[test]
    fn shared_info_cache_matches_full_renderer_through_page_identity_and_face_changes() {
        use instrument_core::{App, Command, device::{Info,Uuid}};
        use crate::navigation::{Button,Context,View};
        let mut app=App::new();app.apply(Command::LevelPreset).unwrap();app.tick(0);
        let mut menu_pixels=std::vec![0xffff;480*480];
        let mut menu=NavigationCache::new(&mut menu_pixels).unwrap();
        for gauge in instrument_core::Instrument::ALL {
            app.apply(Command::Select(gauge)).unwrap();
            let mut background=std::vec![0;480*480];
            let mut renderer=CachedRenderer::new(&mut background,480,480).unwrap();
            let mut pixels=std::vec![0;480*480];let mut expected=pixels.clone();
            let mut history=crate::damage::FrameHistory::new();
            for (n,(info,pressed)) in [(false,None),(false,Some(Button::Info)),(true,None),
                (true,Some(Button::Back)),(true,None),(false,None),(false,Some(Button::Cancel))].into_iter().enumerate(){
                let mut s=Snapshot::from_app(&app);
                s.device=Info{firmware:"v0.2.0-rc.9-12-g1234567-dirty",uuid:Some(Uuid::v4([n as u8;16]))};
                s.menu=Some(View{info,context:Context::from_app(&app),generation:1,pressed});
                assert!(renderer.draw_incremental_with_menu(&mut pixels,&mut history,s,&mut menu,&mut||{}));
                assert!(draw_frame(&mut expected,480,480,s,false));
                assert_eq!(pixels,expected,"{gauge:?}, page={info}, press={pressed:?}");
            }
        }
    }

    #[test]
    fn one_menu_cache_survives_face_session_and_pressed_state_changes() {
        use instrument_core::{App, Command, Instrument};
        use crate::navigation::{Button, Context, View};
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.tick(0);
        let mut backgrounds = core::array::from_fn::<_, 3, _>(|_| std::vec![0;480*480]);
        let mut renderers: std::vec::Vec<_> = backgrounds.iter_mut()
            .map(|b| CachedRenderer::new(b, 480, 480).unwrap()).collect();
        let mut menu_pixels = std::vec![0;480*480];
        let mut menu = NavigationCache::new(&mut menu_pixels).unwrap();
        let mut buffers = core::array::from_fn::<_, 3, _>(|_| std::vec![0;480*480]);
        let mut histories = core::array::from_fn::<_, 3, _>(|_| crate::damage::FrameHistory::new());
        let mut expected = std::vec![0;480*480];
        for n in 0..24 {
            let i = (n / 4) % 3;
            let selected = [Instrument::Altimeter, Instrument::Attitude, Instrument::Turn][i];
            app.apply(Command::Select(selected)).unwrap();
            let mut s = Snapshot::from_app(&app);
            s.source_session = (n / 12) as u64;
            s.external = n >= 12;
            s.external_attitude = n >= 12;
            s.center_pressed = n % 4 == 3;
            // Open immediately on a cold context as well as after warming;
            // switch directly while a pressed button remains in the old cache.
            s.menu = (n % 4 != 3).then_some(View { info: false,
                context: Context { selected, external: s.external, external_attitude: s.external_attitude,
                    imu: false, session: s.source_session }, generation: n as u32,
                pressed: match n % 4 { 0 => Some(Button::Cancel), 1 => None,
                    _ => Some(Button::Gauge(Instrument::Altimeter)) },
            });
            assert!(draw_frame(&mut expected, 480, 480, s, false));
            for b in 0..3 {
                assert!(renderers[i].draw_incremental_with_menu(&mut buffers[b], &mut histories[b], s, &mut menu, &mut || {}));
                assert_eq!(buffers[b], expected, "face/session/press step {n}, buffer {b}");
            }
        }
    }

    #[test]
    fn turn_prepares_navigation_cache_before_the_hold_completes() {
        use instrument_core::{App, Command, Instrument};
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.tick(0);
        app.apply(Command::Select(Instrument::Turn)).unwrap();
        let s = Snapshot::from_app(&app);
        let mut background = std::vec![0;480*480];
        let mut menu = std::vec![0;480*480];
        let mut pixels = std::vec![0;480*480];
        let mut history = crate::damage::FrameHistory::new();
        let mut renderer = CachedRenderer::new(&mut background, 480, 480)
            .unwrap()
            .with_navigation_pixels(&mut menu);
        for _ in 0..8 {
            assert!(renderer.draw_incremental(&mut pixels, &mut history, s, false));
        }
        assert_eq!(renderer.navigation_cache.as_ref().unwrap().prepared,8,"Turn must prepare cached menu artwork before a long press; full lettering redraw exhausts its numeric lifetime");
    }
    #[test]
    fn retained_faces_switch_and_center_feedback_match_full_frames() {
        use instrument_core::{App, Command};
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.tick(0);
        let mut s = Snapshot::from_app(&app);
        let mut bg_alt = std::vec![0;480*480];
        let mut bg_att = bg_alt.clone();
        let mut runs = std::vec![crate::attitude::CopyRun::default();512];
        let mut glyphs = std::vec![0;49152];
        let mut menu = bg_alt.clone();
        let mut alt = CachedRenderer::new(&mut bg_alt, 480, 480).unwrap();
        let mut att = CachedRenderer::with_foreground_cache(&mut bg_att, 480, 480, &mut runs)
            .unwrap()
            .with_foreground_pixels(&mut glyphs)
            .with_navigation_pixels(&mut menu);
        s.selected = Instrument::Altimeter;
        assert!(alt.prepare_background(s));
        s.selected = Instrument::Attitude;
        assert!(att.prepare_background(s));
        let mut frames = core::array::from_fn::<_, 3, _>(|_| std::vec![0;480*480]);
        let mut histories = core::array::from_fn::<_, 3, _>(|_| crate::damage::FrameHistory::new());
        let mut expected = std::vec![0;480*480];
        for n in 0..24 {
            s.selected = if n % 8 < 2 {
                Instrument::Altimeter
            } else {
                Instrument::Attitude
            };
            s.center_pressed = n % 8 == 4 || n % 8 == 5;
            s.roll.value = Some(n as f32 - 12.);
            s.pitch.value = Some(n as f32 * 0.5);
            assert!(draw_frame(&mut expected, 480, 480, s, false));
            let renderer = if s.selected == Instrument::Attitude {
                &mut att
            } else {
                &mut alt
            };
            for (frame, history) in frames.iter_mut().zip(&mut histories) {
                assert!(renderer.draw_incremental(frame, history, s, false));
                assert!(
                    frame == &expected,
                    "retained face or center feedback mismatch at {n}"
                );
            }
        }
    }
    #[test]
    fn gauge_menu_and_center_feedback_restore_three_buffers_on_every_face() {
        use crate::{
            damage::FrameHistory,
            navigation::{Button, Context, View},
        };
        use instrument_core::{App, Command};
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.tick(0);
        for use_cache in [false, true] {
            for selected in [
                Instrument::Altimeter,
                Instrument::Attitude,
                Instrument::Turn,
                Instrument::Heading,
                Instrument::Airspeed,
                Instrument::Vsi,
            ] {
                app.apply(Command::Select(selected)).unwrap();
                let mut s = Snapshot::from_app(&app);
                let mut expected = std::vec![0;480*480];
                let mut backdrop = expected.clone();
                let mut menu_cache = expected.clone();
                let mut renderer = CachedRenderer::new(&mut backdrop, 480, 480).unwrap();
                if use_cache {
                    renderer = renderer.with_navigation_pixels(&mut menu_cache);
                }
                let mut buffers = core::array::from_fn::<_, 3, _>(|_| expected.clone());
                let mut history = core::array::from_fn::<_, 3, _>(|_| FrameHistory::new());
                for n in 0..12 {
                    s.center_pressed = n == 1 || n == 10;
                    s.menu = if (2..8).contains(&n) {
                        Some(View { info: false,
                            generation: 1,
                            context: Context::from_app(&app),
                            pressed: match n {
                                2 => Some(Button::Gauge(Instrument::Attitude)),
                                4 => Some(Button::Gauge(Instrument::Altimeter)),
                                6 => Some(Button::Cancel),
                                _ => None,
                            },
                        })
                    } else {
                        None
                    };
                    s.pitch.value = Some(n as f32 - 4.);
                    s.roll.value = Some(n as f32 * 5.);
                    assert!(draw_frame(&mut expected, 480, 480, s, false));
                    for i in 0..3 {
                        assert!(renderer.draw_incremental(
                            &mut buffers[i],
                            &mut history[i],
                            s,
                            false
                        ));
                        assert!(
                            buffers[i] == expected,
                            "menu damage: {selected:?}, phase {n}, buffer {i}"
                        );
                    }
                }
            }
        }
    }
    use super::*;
    use crate::raster::Framebuffer;
    use instrument_core::{Quality, Reading};
    extern crate std;
    #[test]
    fn changing_baro_does_not_rebuild_the_backdrop() {
        let mut app = instrument_core::App::new();
        app.apply(instrument_core::Command::LevelPreset).unwrap();
        app.tick(0);
        let mut bg = std::vec![0; 480 * 480];
        let mut pixels = bg.clone();
        let mut renderer = CachedRenderer::new(&mut bg, 480, 480).unwrap();
        renderer.draw(&mut pixels, Snapshot::from_app(&app), false);
        let key = renderer.key;
        app.apply(instrument_core::Command::Baro(30.12)).unwrap();
        renderer.draw(&mut pixels, Snapshot::from_app(&app), false);
        assert_eq!(
            renderer.key, key,
            "barometer change invalidates full background"
        );
    }
    #[test]
    fn cached_frames_match_full_render_across_motion_faults_and_settings() {
        for size in [240, 480, 720] {
            let mut background = std::vec![0u16; size * size];
            let mut cached =
                CachedRenderer::new(&mut background, size as u32, size as u32).unwrap();
            let mut output = std::vec![0xffff; size * size];
            let mut reference = output.clone();
            let mut snapshot = Snapshot {
                device: instrument_core::device::Info::default(),
                heading: instrument_core::Reading { value: Some(0.), quality: instrument_core::Quality::Valid, age_ms: Some(0) },
                vsi: instrument_core::Reading { value: Some(0.), quality: instrument_core::Quality::Valid, age_ms: Some(0) },
                ias: instrument_core::Reading { value: Some(120.), quality: instrument_core::Quality::Valid, age_ms: Some(0) },
                turn_rate: Reading {
                    value: Some(0.),
                    quality: Quality::Valid,
                    age_ms: Some(0),
                },
                ball: Reading {
                    value: Some(0.),
                    quality: Quality::Valid,
                    age_ms: Some(0),
                },
                pitch: Reading {
                    value: Some(0.),
                    quality: Quality::Valid,
                    age_ms: Some(0),
                },
                roll: Reading {
                    value: Some(0.),
                    quality: Quality::Valid,
                    age_ms: Some(0),
                },
                imu_bench: false,
                imu_last_good: None,
                external: false,
                external_attitude: false,
                source_running: false,
                source_session: 0,
                source_sequence: 0,
                source_received_ms: 0,
                baro_age_ms: None,
                source_label: "SIM INPUT",
                kollsman_pressed: false,
                menu: None,
                center_pressed: false,
                selected: Instrument::Altimeter,
                altitude: Reading {
                    value: Some(0.),
                    quality: Quality::Valid,
                    age_ms: Some(0),
                },
                editor: None,
                baro_inhg: 29.92,
            };
            for (altitude, quality, baro, instrument, pattern) in [
                (0., Quality::Valid, 29.92, Instrument::Altimeter, false),
                (4520., Quality::Valid, 29.92, Instrument::Altimeter, false),
                (8500., Quality::Valid, 30.12, Instrument::Altimeter, false),
                (10000., Quality::Valid, 29.92, Instrument::Altimeter, false),
                (10000., Quality::Stale, 29.92, Instrument::Altimeter, false),
                (0., Quality::Missing, 29.92, Instrument::Altimeter, false),
                (0., Quality::Failed, 29.92, Instrument::Altimeter, false),
                (
                    0.,
                    Quality::TimingFault,
                    29.92,
                    Instrument::Altimeter,
                    false,
                ),
                (-100., Quality::Valid, 29.92, Instrument::Altimeter, false),
                (0., Quality::Valid, 29.92, Instrument::Attitude, false),
                (0., Quality::Valid, 29.92, Instrument::Altimeter, true),
                (1234., Quality::Valid, 29.92, Instrument::Altimeter, false),
                (
                    1234.,
                    Quality::Valid,
                    f32::NAN,
                    Instrument::Altimeter,
                    false,
                ),
            ] {
                snapshot.altitude.value = Some(altitude);
                snapshot.altitude.quality = quality;
                snapshot.baro_inhg = baro;
                snapshot.selected = instrument;
                assert!(cached.draw(&mut output, snapshot, pattern));
                assert!(draw_frame(
                    &mut reference,
                    size as u32,
                    size as u32,
                    snapshot,
                    pattern
                ));
                assert_eq!(
                    output, reference,
                    "cache mismatch at {size}: {snapshot:?}, pattern={pattern}"
                );
            }
        }
    }
    #[test]
    fn incremental_triple_buffers_match_full_redraw_through_motion_and_failures() {
        for (w, h) in [(240, 240), (480, 480), (720, 720), (241, 239), (1024, 64)] {
            let len = w * h;
            let mut background = std::vec![0; len];
            let mut cached = CachedRenderer::new(&mut background, w as u32, h as u32).unwrap();
            let mut frames: [std::vec::Vec<u16>; 3] =
                core::array::from_fn(|_| std::vec![0xaaaa; len]);
            let mut histories =
                core::array::from_fn::<_, 3, _>(|_| crate::damage::FrameHistory::new());
            let mut reference = std::vec![0; len];
            for n in 0..60 {
                let snapshot = Snapshot {
                device: instrument_core::device::Info::default(),
                    heading: instrument_core::Reading { value: Some(0.), quality: instrument_core::Quality::Valid, age_ms: Some(0) },
                    vsi: instrument_core::Reading { value: Some(0.), quality: instrument_core::Quality::Valid, age_ms: Some(0) },
                    ias: instrument_core::Reading { value: Some(120.), quality: instrument_core::Quality::Valid, age_ms: Some(0) },
                    turn_rate: Reading {
                        value: Some(0.),
                        quality: Quality::Valid,
                        age_ms: Some(0),
                    },
                    ball: Reading {
                        value: Some(0.),
                        quality: Quality::Valid,
                        age_ms: Some(0),
                    },
                    pitch: Reading {
                        value: Some(0.),
                        quality: Quality::Valid,
                        age_ms: Some(0),
                    },
                    roll: Reading {
                        value: Some(0.),
                        quality: Quality::Valid,
                        age_ms: Some(0),
                    },
                    imu_bench: false,
                    imu_last_good: None,
                    external: false,
                    external_attitude: false,
                    source_running: false,
                    source_session: 0,
                    source_sequence: 0,
                    source_received_ms: 0,
                    baro_age_ms: None,
                    source_label: "SIM INPUT",
                    kollsman_pressed: n % 5 < 2,
                    menu: None,
                    center_pressed: false,
                    selected: if n % 19 < 3 {
                        Instrument::Attitude
                    } else {
                        Instrument::Altimeter
                    },
                    altitude: Reading {
                        value: Some(n as f32 * 1137. - 100.),
                        quality: [
                            Quality::Valid,
                            Quality::Valid,
                            Quality::Valid,
                            Quality::Missing,
                            Quality::Stale,
                            Quality::Failed,
                            Quality::TimingFault,
                        ][(n / 3) % 7],
                        age_ms: Some(10),
                    },
                    editor: if n % 11 < 6 {
                        Some(crate::interaction::EditorView {
                            cents: 2810 + n as u16,
                            pressed: if n % 2 == 0 {
                                Some(crate::interaction::Button::Plus)
                            } else {
                                None
                            },
                        })
                    } else {
                        None
                    },
                    baro_inhg: if n % 23 < 5 { 30.92 } else { 29.92 },
                };
                let pattern = n % 17 < 4;
                let slot = n % 3;
                assert!(cached.draw_incremental(
                    &mut frames[slot],
                    &mut histories[slot],
                    snapshot,
                    pattern
                ));
                assert!(draw_frame(
                    &mut reference,
                    w as u32,
                    h as u32,
                    snapshot,
                    pattern
                ));
                assert_eq!(
                    frames[slot], reference,
                    "incremental mismatch {w}x{h} frame {n}"
                );
            }
        }
    }
    #[test]
    fn press_feedback_is_local_and_restores_all_three_buffers() {
        let mut app = instrument_core::App::new();
        app.apply(instrument_core::Command::LevelPreset).unwrap();
        app.tick(0);
        let mut snapshot = Snapshot::from_app(&app);
        let mut normal = std::vec![0; 480 * 480];
        assert!(draw_frame(&mut normal, 480, 480, snapshot, false));
        let mut backdrop = std::vec![0; normal.len()];
        let mut cached = CachedRenderer::new(&mut backdrop, 480, 480).unwrap();
        let mut frames = core::array::from_fn::<_, 3, _>(|_| normal.clone());
        let mut histories = core::array::from_fn::<_, 3, _>(|_| crate::damage::FrameHistory::new());
        for pressed in [false, true, false, true, true, false] {
            snapshot.kollsman_pressed = pressed;
            for (frame, history) in frames.iter_mut().zip(&mut histories) {
                assert!(cached.draw_incremental(frame, history, snapshot, false));
                if !pressed {
                    assert_eq!(frame, &normal);
                } else {
                    assert!(frame.iter().filter(|&&p| p == KOLLSMAN_PRESSED).count() > 1000);
                    for (i, (&actual, &original)) in frame.iter().zip(&normal).enumerate() {
                        if !crate::interaction::KOLLSMAN
                            .contains((i % 480) as u16, (i / 480) as u16)
                        {
                            assert_eq!(actual, original, "press changed pixels outside window");
                        }
                    }
                }
            }
        }
        snapshot.altitude.quality = Quality::Failed;
        snapshot.kollsman_pressed = false;
        assert!(draw_frame(&mut normal, 480, 480, snapshot, false));
        snapshot.kollsman_pressed = true;
        assert!(cached.draw_incremental(&mut frames[0], &mut histories[0], snapshot, false));
        assert_eq!(
            frames[0], normal,
            "press feedback must not cover invalid-data warnings"
        );
    }
    #[test]
    fn every_editor_button_highlight_restores_all_buffers() {
        use crate::interaction::{Button, EditorView};
        let mut app = instrument_core::App::new();
        app.apply(instrument_core::Command::LevelPreset).unwrap();
        app.tick(0);
        let mut snapshot = Snapshot::from_app(&app);
        snapshot.editor = Some(EditorView {
            cents: 2992,
            pressed: None,
        });
        let mut normal = std::vec![0; 480 * 480];
        assert!(draw_frame(&mut normal, 480, 480, snapshot, false));
        let mut backdrop = normal.clone();
        let mut cached = CachedRenderer::new(&mut backdrop, 480, 480).unwrap();
        let mut frames = core::array::from_fn::<_, 3, _>(|_| normal.clone());
        let mut histories = core::array::from_fn::<_, 3, _>(|_| crate::damage::FrameHistory::new());
        for button in [
            Button::Minus,
            Button::Plus,
            Button::Standard,
            Button::Cancel,
            Button::Apply,
        ] {
            for pressed in [Some(button), None] {
                snapshot.editor.as_mut().unwrap().pressed = pressed;
                for (frame, history) in frames.iter_mut().zip(&mut histories) {
                    assert!(cached.draw_incremental(frame, history, snapshot, false));
                    let r = button.rect();
                    if pressed.is_some() {
                        assert_eq!(
                            frame[(r.y as usize + 2) * 480 + r.x as usize + 2],
                            KOLLSMAN_PRESSED
                        );
                        for (i, (&actual, &original)) in frame.iter().zip(&normal).enumerate() {
                            if !r.contains((i % 480) as u16, (i / 480) as u16) {
                                assert_eq!(actual, original);
                            }
                        }
                    } else {
                        assert_eq!(frame, &normal);
                    }
                }
            }
        }
    }
    #[test]
    fn repeated_manual_setting_repaints_identically_over_previous_frames() {
        let mut app = instrument_core::App::new();
        app.tick(0);
        let mut pixels = std::vec![0xffff; 480 * 480];
        assert!(draw_frame(
            &mut pixels,
            480,
            480,
            Snapshot::from_app(&app),
            false
        ));
        app.apply(instrument_core::Command::Set(
            instrument_core::Field::PressureAltitudeFt,
            8500.,
        ))
        .unwrap();
        app.tick(20);
        assert!(draw_frame(
            &mut pixels,
            480,
            480,
            Snapshot::from_app(&app),
            false
        ));
        let first = pixels.clone();
        app.apply(instrument_core::Command::Set(
            instrument_core::Field::PressureAltitudeFt,
            8500.,
        ))
        .unwrap();
        app.tick(40);
        assert!(draw_frame(
            &mut pixels,
            480,
            480,
            Snapshot::from_app(&app),
            false
        ));
        assert_eq!(
            first, pixels,
            "identical commands must not corrupt a reused framebuffer"
        );
        let mut fresh = std::vec![0x5a5a; 480 * 480];
        assert!(draw_frame(
            &mut fresh,
            480,
            480,
            Snapshot::from_app(&app),
            false
        ));
        assert_eq!(
            pixels, fresh,
            "frame output must not depend on previous contents"
        );
    }
    #[test]
    fn external_lifecycle_labels_and_barometer_restore_all_buffers() {
        let mut app = instrument_core::App::new();
        app.tick(0);
        app.apply(instrument_core::Command::LevelPreset).unwrap();
        app.tick(20);
        let mut s = Snapshot::from_app(&app);
        let mut bg = std::vec![0; 480 * 480];
        let mut buffers = [
            std::vec![0; 480 * 480],
            std::vec![0; 480 * 480],
            std::vec![0; 480 * 480],
        ];
        let mut histories = core::array::from_fn::<_, 3, _>(|_| crate::damage::FrameHistory::new());
        let mut renderer = CachedRenderer::new(&mut bg, 480, 480).unwrap();
        let mut expected = std::vec![0; 480 * 480];
        for (external, label, baro, quality) in [
            (false, "SIM INPUT", 29.92, Quality::Valid),
            (true, "TEST INPUT", 30.12, Quality::Valid),
            (true, "REPLAY", 30.12, Quality::Valid),
            (true, "PAUSED", f32::NAN, Quality::Missing),
            (true, "LINK STALE", f32::NAN, Quality::Stale),
            (true, "NO LINK", f32::NAN, Quality::Missing),
            (false, "SIM INPUT", 29.92, Quality::Valid),
        ] {
            s.external = external;
            s.source_label = label;
            s.baro_inhg = baro;
            s.altitude.quality = quality;
            assert!(draw_frame(&mut expected, 480, 480, s, false));
            for i in 0..3 {
                assert!(renderer.draw_incremental(&mut buffers[i], &mut histories[i], s, false));
                assert_eq!(buffers[i], expected, "source={label}, buffer={i}");
            }
        }
    }
    #[test]
    fn invalid_frames_remove_pointer_at_all_supported_test_sizes() {
        for size in [240, 480, 720] {
            let mut pixels = std::vec![0u16;size*size];
            let valid = Snapshot {
                device: instrument_core::device::Info::default(),
                heading: instrument_core::Reading { value: Some(0.), quality: instrument_core::Quality::Valid, age_ms: Some(0) },
                vsi: instrument_core::Reading { value: Some(0.), quality: instrument_core::Quality::Valid, age_ms: Some(0) },
                ias: instrument_core::Reading { value: Some(120.), quality: instrument_core::Quality::Valid, age_ms: Some(0) },
                turn_rate: Reading {
                    value: Some(0.),
                    quality: Quality::Valid,
                    age_ms: Some(0),
                },
                ball: Reading {
                    value: Some(0.),
                    quality: Quality::Valid,
                    age_ms: Some(0),
                },
                pitch: Reading {
                    value: Some(0.),
                    quality: Quality::Valid,
                    age_ms: Some(0),
                },
                roll: Reading {
                    value: Some(0.),
                    quality: Quality::Valid,
                    age_ms: Some(0),
                },
                imu_bench: false,
                imu_last_good: None,
                external: false,
                external_attitude: false,
                source_running: false,
                source_session: 0,
                source_sequence: 0,
                source_received_ms: 0,
                baro_age_ms: None,
                source_label: "SIM INPUT",
                kollsman_pressed: false,
                menu: None,
                center_pressed: false,
                selected: Instrument::Altimeter,
                altitude: Reading {
                    quality: Quality::Valid,
                    value: Some(0.),
                    age_ms: Some(0),
                },
                editor: None,
                baro_inhg: 29.92,
            };
            draw(
                &mut Framebuffer::new(&mut pixels, size as u32, size as u32).unwrap(),
                valid,
            );
            let index = (100 * size / 480) * size + 240 * size / 480;
            assert_eq!(
                pixels[index], WHITE,
                "zero is a valid twelve-o'clock pointer"
            );
            for quality in [
                Quality::Missing,
                Quality::Stale,
                Quality::Failed,
                Quality::TimingFault,
            ] {
                let mut invalid = valid;
                invalid.altitude.quality = quality;
                draw(
                    &mut Framebuffer::new(&mut pixels, size as u32, size as u32).unwrap(),
                    invalid,
                );
                assert_ne!(
                    pixels[index], WHITE,
                    "old pointer remained after invalidation"
                );
                assert_eq!(pixels[0], 0, "round viewport corner must remain black");
            }
        }
    }

    #[test]
    fn new_faces_restore_all_scanout_buffers_through_motion_and_invalid_data() {
        use instrument_core::{App,Command};
        let mut app=App::new();app.apply(Command::LevelPreset).unwrap();app.tick(0);
        for size in [240,480] {
            for gauge in [Instrument::Airspeed,Instrument::Heading,Instrument::Vsi] {
                let mut s=Snapshot::from_app(&app);s.selected=gauge;
                let mut bg=std::vec![0;size*size];
                let mut cached=CachedRenderer::new(&mut bg,size as u32,size as u32).unwrap();
                let mut frames=core::array::from_fn::<_,3,_>(|_|std::vec![0;size*size]);
                let mut histories=core::array::from_fn::<_,3,_>(|_|crate::damage::FrameHistory::new());
                let mut expected=std::vec![0;size*size];
                for n in 0..20 {
                    s.external=n%3==0;s.external_attitude=s.external;
                    s.source_label=if s.external {"TEST INPUT"}else{"SIM INPUT"};
                    let reading=match gauge{Instrument::Heading=>&mut s.heading,Instrument::Vsi=>&mut s.vsi,_=>&mut s.ias};
                    reading.value=Some(match gauge{Instrument::Heading=>(350.+n as f32*17.)%360.,Instrument::Vsi=>(n as f32-10.)*350.,_=>n as f32*17.});
                    reading.quality=match n {4=>Quality::Stale,9=>Quality::Missing,14=>Quality::Failed,17=>Quality::TimingFault,_=>Quality::Valid};
                    reading.age_ms=Some(0);s.center_pressed=n%5==0;
                    assert!(draw_frame(&mut expected,size as u32,size as u32,s,false));
                    for (frame,history) in frames.iter_mut().zip(&mut histories) {
                        assert!(cached.draw_incremental(frame,history,s,false));
                        assert!(frame==&expected,"{gauge:?} {size} frame {n} retained damaged pixels");
                    }
                }
            }
        }
    }
}
