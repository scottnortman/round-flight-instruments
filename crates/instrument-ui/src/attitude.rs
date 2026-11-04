//! Standalone phone-reference attitude face. Positive pitch is nose up;
//! positive roll is right bank. No sensor acquisition or filtering lives here.
use crate::raster::{rgb, Canvas, Target};
use instrument_core::{Field, Quality, Reading};

pub const STALE_MS: u64 = 100;
// Includes foreground strokes for every advertised source label.
pub const FOREGROUND_RUN_CAPACITY: usize = 512;
pub const PITCH_SCALE: f32 = 4.5;
const TRANSPARENT: u16 = u16::MAX;
#[derive(Clone, Copy, Default)]
pub struct CopyRun {
    pub(crate) start: u32,
    pub(crate) len: u16,
}
/// Build fixed foreground once into caller storage. No frame-time allocation.
pub(crate) fn cache_foreground(
    pixels: &mut [u16],
    width: u32,
    height: u32,
    source: &str,
    runs: &mut [CopyRun],
) -> Option<usize> {
    let mut fb = crate::raster::Framebuffer::new(pixels, width, height)?;
    let mut disk = Disk::new(&mut fb);
    for y in disk.cy - disk.radius..=disk.cy + disk.radius {
        let (start, end) = disk.bounds(y);
        disk.span(y, start, end, TRANSPARENT);
    }
    foreground(
        &mut disk,
        Attitude {
            angles: Some(Angles {
                pitch_deg: 0.,
                roll_deg: 0.,
            }),
            quality: Quality::Valid,
        },
        source,
    );
    let (cx, cy, radius) = (disk.cx, disk.cy, disk.radius);
    let mut count = 0;
    for y in cy - radius..=cy + radius {
        let dx = isqrt((radius * radius - (y - cy) * (y - cy)) as u32) as i32;
        let mut x = cx - dx;
        let end = cx + dx + 1;
        while x < end {
            let index = y as usize * width as usize + x as usize;
            if pixels[index] == TRANSPARENT {
                x += 1;
                continue;
            }
            let start = x;
            while x < end && pixels[y as usize * width as usize + x as usize] != TRANSPARENT {
                x += 1;
            }
            *runs.get_mut(count)? = CopyRun {
                start: (y as usize * width as usize + start as usize) as u32,
                len: (x - start) as u16,
            };
            count += 1;
        }
    }
    Some(count)
}
const FACE: u16 = rgb(8, 11, 14);
pub(crate) const MARK: u16 = rgb(241, 242, 235);
const AIRCRAFT: u16 = rgb(255, 224, 96);
const FAILURE: u16 = rgb(255, 103, 103);

/// Restore only the bezel; the first horizon/failure draw fills the whole disk.
pub(crate) fn restore_outer(pixels: &mut [u16], background: &[u16], width: u32, height: u32) {
    let radius = (224. * width.min(height) as f32 / 480.) as i32;
    let (cx, cy) = ((width / 2) as i32, (height / 2) as i32);
    for y in 0..height as i32 {
        let dy = (y - cy).abs();
        let (left, right) = if dy <= radius {
            let dx = isqrt((radius * radius - dy * dy) as u32) as i32;
            (
                (cx - dx).max(0) as usize,
                (cx + dx + 1).min(width as i32) as usize,
            )
        } else {
            (width as usize, width as usize)
        };
        let row = y as usize * width as usize;
        pixels[row..row + left].copy_from_slice(&background[row..row + left]);
        pixels[row + right..row + width as usize]
            .copy_from_slice(&background[row + right..row + width as usize]);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Attitude {
    pub angles: Option<Angles>,
    pub quality: Quality,
}
#[derive(Clone, Copy, Debug)]
pub struct Angles {
    pub pitch_deg: f32,
    pub roll_deg: f32,
}
fn checked(r: Reading, field: Field, stale_ms: u64) -> Quality {
    if r.quality != Quality::Valid {
        return r.quality;
    }
    if r.value.is_none_or(|v| !field.accepts(v)) || r.age_ms.is_none() {
        Quality::Failed
    } else if r.age_ms.is_some_and(|a| a >= stale_ms) {
        Quality::Stale
    } else {
        Quality::Valid
    }
}
pub fn model(pitch: Reading, roll: Reading) -> Attitude {
    model_with_limit(pitch, roll, STALE_MS)
}
pub(crate) fn model_with_limit(pitch: Reading, roll: Reading, stale_ms: u64) -> Attitude {
    let qualities = [
        checked(pitch, Field::PitchDeg, stale_ms),
        checked(roll, Field::RollDeg, stale_ms),
    ];
    let quality = [
        Quality::TimingFault,
        Quality::Failed,
        Quality::Stale,
        Quality::Missing,
    ]
    .into_iter()
    .find(|q| qualities.contains(q))
    .unwrap_or(Quality::Valid);
    Attitude {
        angles: (quality == Quality::Valid).then_some(Angles {
            pitch_deg: pitch.value.unwrap_or(0.),
            roll_deg: roll.value.unwrap_or(0.),
        }),
        quality,
    }
}

/// Rotation opposite bank, then pitch translation along the rotated vertical.
/// Matches the nested SVG transforms in the phone prototype.
struct Pose {
    sin: f32,
    cos: f32,
    offset: f32,
}
impl Pose {
    fn new(a: Angles) -> Self {
        let radians = a.roll_deg * core::f32::consts::PI / 180.;
        Self {
            sin: libm::sinf(radians),
            cos: libm::cosf(radians),
            offset: a.pitch_deg * PITCH_SCALE,
        }
    }
    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        let y = y + self.offset;
        (
            240. + self.cos * x + self.sin * y,
            240. - self.sin * x + self.cos * y,
        )
    }
}
/// All writes stay inside the dial. Integer scanline extents avoid repeated
/// square roots for clipped strokes; storage is bounded and allocation-free.
struct Disk<'a, T> {
    target: &'a mut T,
    cx: i32,
    cy: i32,
    radius: i32,
    extents: [u16; 512],
}
impl<'a, T: Target> Disk<'a, T> {
    fn new(target: &'a mut T) -> Self {
        let (w, h) = target.size();
        let radius = (224. * w.min(h) as f32 / 480.) as i32;
        let mut extents = [0u16; 512];
        let mut x = 0i32;
        for dy in (0..=radius.min(511)).rev() {
            while (x + 1) * (x + 1) + dy * dy <= radius * radius {
                x += 1;
            }
            extents[dy as usize] = x as u16;
        }
        Self {
            target,
            cx: (w / 2) as i32,
            cy: (h / 2) as i32,
            radius,
            extents,
        }
    }
    fn bounds(&self, y: i32) -> (i32, i32) {
        let dy = (y - self.cy).unsigned_abs() as usize;
        if dy > self.radius as usize {
            return (0, 0);
        }
        let dx = if dy < 512 {
            self.extents[dy] as i32
        } else {
            isqrt((self.radius * self.radius) as u32 - (dy * dy) as u32) as i32
        };
        (self.cx - dx, self.cx + dx + 1)
    }
}
impl<T: Target> Target for Disk<'_, T> {
    fn size(&self) -> (u32, u32) {
        self.target.size()
    }
    fn pixel(&mut self, x: i32, y: i32, color: u16) {
        let dx = x - self.cx;
        let dy = y - self.cy;
        if dx.abs() <= self.radius
            && dy.abs() <= self.radius
            && dx * dx + dy * dy <= self.radius * self.radius
        {
            self.target.pixel(x, y, color);
        }
    }
    fn pixels(&mut self, y: i32, start: i32, colors: &[Option<u16>]) {
        let (left, right) = self.bounds(y);
        let lo = start.max(left);
        let hi = (start.saturating_add(colors.len().min(i32::MAX as usize) as i32)).min(right);
        if lo < hi {
            self.target
                .pixels(y, lo, &colors[(lo - start) as usize..(hi - start) as usize]);
        }
    }
    fn span(&mut self, y: i32, start: i32, end: i32, color: u16) {
        if start >= end {
            return;
        }
        let dy = y - self.cy;
        if dy.abs() > self.radius {
            return;
        }
        let dx = (start - self.cx).abs().max((end - 1 - self.cx).abs());
        if dx <= self.radius && dx * dx + dy * dy <= self.radius * self.radius {
            self.target.span(y, start, end, color);
            return;
        }
        let (lo, hi) = self.bounds(y);
        if start.max(lo) < end.min(hi) {
            self.target.span(y, start.max(lo), end.min(hi), color);
        }
    }
}
fn gradient(bin: i32) -> u16 {
    PALETTE[(bin + 15) as usize]
}
const PALETTE: [u16; 30] = {
    let mut colors = [0; 30];
    let mut i = 0;
    while i < 30 {
        let bin = i as i32 - 15;
        let (near, far, d) = if bin < 0 {
            ([39, 140, 197], [7, 59, 118], -bin * 16 - 8)
        } else {
            ([166, 107, 62], [84, 49, 31], bin * 16 + 8)
        };
        let d = if d > 230 { 230 } else { d };
        colors[i] = rgb(
            (near[0] + (far[0] - near[0]) * d / 230) as u8,
            (near[1] + (far[1] - near[1]) * d / 230) as u8,
            (near[2] + (far[2] - near[2]) * d / 230) as u8,
        );
        i += 1;
    }
    colors
};
fn isqrt(mut n: u32) -> u32 {
    let mut root = 0;
    let mut bit = 1u32 << 30;
    while bit > n {
        bit >>= 2;
    }
    while bit != 0 {
        if n >= root + bit {
            n -= root + bit;
            root = (root >> 1) + bit;
        } else {
            root >>= 1;
        }
        bit >>= 2;
    }
    root
}
fn bank_backing<T: Target>(disk: &mut Disk<'_, T>) {
    let scale = disk.size().0.min(disk.size().1) as f32 / 480.;
    let outer = 226.5 * scale;
    let inner = 187.5 * scale;
    let (top, bottom) = (0, disk.size().1 as i32);
    for y in (disk.cy - disk.radius).max(top)..disk.cy.min(bottom) {
        let dy = (disk.cy - y) as f32;
        if dy > outer {
            continue;
        }
        let hi = (isqrt((outer * outer - dy * dy) as u32) as f32).min(dy * 2.144507) as i32;
        let lo = if dy < inner {
            isqrt((inner * inner - dy * dy) as u32) as i32
        } else {
            0
        };
        disk.span(y, disk.cx - hi, disk.cx - lo + 1, rgb(6, 18, 30));
        disk.span(y, disk.cx + lo, disk.cx + hi + 1, rgb(6, 18, 30));
    }
}
struct BandGrid {
    step: i32,
    quotient: i32,
    remainder: i32,
}
impl BandGrid {
    fn new(step: i32) -> Self {
        let d = step.abs().max(1);
        Self {
            step,
            quotient: (1 << 20) / d,
            remainder: (1 << 20) % d,
        }
    }
    // Boundaries for bins -14..=14. One division per row, then exact
    // quotient/remainder stepping instead of division at every band edge.
    fn row(&self, base: i32, cx: i32) -> [i32; 29] {
        let mut edges = [0; 29];
        if self.step == 0 {
            return edges;
        }
        let d = self.step.abs();
        let numerator = base + (14 << 20);
        let mut q = numerator.div_euclid(d);
        let mut r = numerator - q * d;
        for edge in &mut edges {
            *edge = if self.step > 0 { cx - q } else { cx + q + 1 };
            q -= self.quotient;
            r -= self.remainder;
            if r < 0 {
                r += d;
                q -= 1;
            }
        }
        edges
    }
}
fn horizon<T: Target>(
    disk: &mut Disk<'_, T>,
    pose: &Pose,
    previous: Option<&Pose>,
    history: Option<&crate::damage::FrameHistory>,
    progress: &mut dyn FnMut(),
) {
    let scale = disk.size().0.min(disk.size().1) as f32 / 480.;
    let inverse_scale = 1. / scale;
    let tile_shift = crate::damage::tile_shift(disk.size().0);
    // 16.16 horizon coordinates: bounded integer work in the per-span loop.
    // At the supported <=1024 viewport, |v|<700 logical units; i32 has margin.
    let step = libm::roundf(pose.sin * inverse_scale * 65536.) as i32;
    let old_step = previous.map(|p| libm::roundf(p.sin * inverse_scale * 65536.) as i32);
    let grid = BandGrid::new(step);
    let old_grid = old_step.map(BandGrid::new);
    let (top, bottom) = (0, disk.size().1 as i32);
    for y in (disk.cy - disk.radius).max(top)..=(disk.cy + disk.radius).min(bottom - 1) {
        if y % 32 == 0 {
            progress();
        }
        let (mut x, end) = disk.bounds(y);
        let base =
            libm::roundf((pose.cos * (y - disk.cy) as f32 * inverse_scale - pose.offset) * 65536.)
                as i32;
        let old_base = previous.map(|p| {
            libm::roundf((p.cos * (y - disk.cy) as f32 * inverse_scale - p.offset) * 65536.) as i32
        });
        let edges = grid.row(base, disk.cx);
        let old_edges = old_grid
            .as_ref()
            .zip(old_base)
            .map(|(grid, base)| grid.row(base, disk.cx))
            .unwrap_or([0; 29]);
        while x < end {
            let bin = ((base + step * (x - disk.cx)) >> 20).clamp(-15, 14);
            let next = if step == 0 || (step > 0 && bin == 14) || (step < 0 && bin == -15) {
                end
            } else if step > 0 {
                edges[(bin + 15) as usize].max(x + 1).min(end)
            } else {
                edges[(bin + 14) as usize].max(x + 1).min(end)
            };
            if let (Some(old_step), Some(old_base), Some(history)) = (old_step, old_base, history) {
                // Intersect this new band with the identical old band. Only
                // their changed edges and actual old markings need writes.
                let (lo, hi) = if old_step > 0 {
                    (
                        if bin == -15 {
                            x
                        } else {
                            old_edges[(bin + 14) as usize]
                        },
                        if bin == 14 {
                            next
                        } else {
                            old_edges[(bin + 15) as usize]
                        },
                    )
                } else if old_step < 0 {
                    (
                        if bin == 14 {
                            x
                        } else {
                            old_edges[(bin + 15) as usize]
                        },
                        if bin == -15 {
                            next
                        } else {
                            old_edges[(bin + 14) as usize]
                        },
                    )
                } else if (old_base >> 20).clamp(-15, 14) == bin {
                    (x, next)
                } else {
                    (next, next)
                };
                let common_start = lo.max(x).min(next);
                let common_end = hi.min(next).max(x);
                let color = gradient(bin);
                if common_start >= common_end {
                    disk.span(y, x, next, color);
                } else {
                    disk.span(y, x, common_start, color);
                    disk.span(y, common_end, next, color);
                    let first = common_start as u32 >> tile_shift;
                    let last = (common_end as u32 - 1) >> tile_shift;
                    let mut dirty =
                        history.dirty_row(y) & (u64::MAX << first) & (u64::MAX >> (63 - last));
                    while dirty != 0 {
                        let tile = dirty.trailing_zeros() as i32;
                        let count = (dirty >> tile).trailing_ones() as i32;
                        dirty &= !((u64::MAX >> (64 - count)) << tile);
                        disk.span(
                            y,
                            common_start.max(tile << tile_shift),
                            common_end.min((tile + count) << tile_shift),
                            color,
                        );
                    }
                }
                x = next;
            } else {
                disk.span(y, x, next, gradient(bin));
                x = next;
            }
        }
    }
}

fn polar(radius: f32, angle: f32) -> (f32, f32) {
    let a = angle * core::f32::consts::PI / 180.;
    (240. + radius * libm::sinf(a), 240. - radius * libm::cosf(a))
}
pub fn draw<T: Target>(target: &mut T, model: Attitude, source_label: &str) {
    let mut disk = Disk::new(target);
    if let Some(angles) = model.angles {
        horizon(&mut disk, &Pose::new(angles), None, None, &mut || {});
    }
    moving(&mut disk, model, &mut || {}, None);
    foreground(&mut disk, model, source_label);
}
pub(crate) fn waiting<T: Target>(target: &mut T) {
    let mut disk = Disk::new(target);
    let mut c = Canvas::new(&mut disk);
    c.circle(240., 240., 224., FACE);
    c.text_on(
        240.,
        240.,
        22.,
        "WAITING FOR IMU",
        (rgb(255, 185, 87), FACE),
    );
    captions(&mut disk, "IMU WAIT");
}
pub(crate) fn incremental(
    pixels: &mut [u16],
    size: (u32, u32),
    history: &mut crate::damage::FrameHistory,
    snapshot: crate::Snapshot,
    cached: Option<(&[u16], &[CopyRun], bool)>,
    digits: Option<&[u16]>,
    progress: &mut dyn FnMut(),
) {
    let (width, height) = size;
    let model = snapshot.attitude_model();
    let mut fb = crate::raster::Framebuffer::new(pixels, width, height).unwrap();
    if snapshot.imu_bench && model.angles.is_none() {
        waiting(&mut fb);
        history.clear();
        progress();
        return;
    }
    if let Some(angles) = model.angles {
        horizon(
            &mut Disk::new(&mut fb),
            &Pose::new(angles),
            history.attitude.map(Pose::new).as_ref(),
            Some(history),
            progress,
        );
    }
    progress();
    history.clear();
    let mut tracked = crate::damage::Tracked {
        last_pixel_tile: u16::MAX,
        target: fb,
        history,
    };
    moving(&mut Disk::new(&mut tracked), model, progress, digits);
    progress();
    let mut fb = tracked.target;
    if let Some((background, runs, compact)) = cached.filter(|_| model.angles.is_some()) {
        let _ = fb;
        let mut offset = 0;
        for run in runs {
            let range = run.start as usize..run.start as usize + run.len as usize;
            let source = if compact {
                offset..offset + run.len as usize
            } else {
                range.clone()
            };
            pixels[range].copy_from_slice(&background[source]);
            offset += run.len as usize;
        }
    } else {
        foreground(&mut Disk::new(&mut fb), model, snapshot.source_label);
    }
    history.attitude = model.angles;
}

fn moving<T: Target>(
    disk: &mut Disk<'_, T>,
    model: Attitude,
    progress: &mut dyn FnMut(),
    digit_cache: Option<&[u16]>,
) {
    if let Some(angles) = model.angles {
        let pose = Pose::new(angles);
        let mut c = Canvas::new(disk);
        c.polygon(
            &[
                pose.point(-600., -1.75),
                pose.point(600., -1.75),
                pose.point(600., 1.75),
                pose.point(-600., 1.75),
            ],
            MARK,
        );
        progress();
        let mut labels = [(0u8, (0., 0.)); 12];
        let mut label_count = 0;
        for n in -24i32..=24 {
            progress();
            let angle = n as f32 * 2.5;
            if n == 0 || (angle - angles.pitch_deg).abs() > 22.5 {
                continue;
            }
            let major = n % 4 == 0;
            let width = if major {
                42.
            } else if n % 2 == 0 {
                27.
            } else {
                14.
            };
            let y = -angle * PITCH_SCALE;
            let half = if major { 1.75 } else { 0.9 };
            c.polygon(
                &[
                    pose.point(-width, y - half),
                    pose.point(width, y - half),
                    pose.point(width, y + half),
                    pose.point(-width, y + half),
                ],
                MARK,
            );
            if major {
                let digit = (angle.abs() as u8) / 10;
                for x in [-65., 65.] {
                    labels[label_count] = (digit, pose.point(x, y));
                    label_count += 1;
                }
            }
        }
        c.ladder_labels(&labels[..label_count], -angles.roll_deg, MARK, digit_cache);
        // Fixed scale, pointer TOWARD bank; horizon rotates the other way.
        let bank = Pose::new(Angles {
            pitch_deg: 0.,
            roll_deg: -angles.roll_deg,
        });
        c.polygon(
            &[
                bank.point(0., -182.),
                bank.point(-9., -165.),
                bank.point(9., -165.),
            ],
            MARK,
        );
    }
}
fn foreground<T: Target>(disk: &mut Disk<'_, T>, model: Attitude, source_label: &str) {
    if model.angles.is_some() {
        // Phone face's dark bank-scale arc; opaque RGB565 backing for contrast.
        bank_backing(disk);
        let mut c = Canvas::new(disk);
        for a in [-60i32, -45, -30, -20, -10, 10, 20, 30, 45, 60] {
            let major = a.abs() == 30 || a.abs() == 60;
            c.stroke(
                polar(if major { 185. } else { 193. }, a as f32),
                polar(207., a as f32),
                if major { 3.5 } else { 1.8 },
                MARK,
            );
        }
        // Offset labels three degrees toward level along the arc so the
        // upright glyphs clear the thick ticks. Leave radial clearance for
        // the top-left corner of the left "30" inside the clipped disk.
        for (angle, label) in [(-57., "60"), (-27., "30"), (27., "30"), (57., "60")] {
            let (x, y) = polar(212., angle);
            c.text_on(x, y, 12., label, (MARK, rgb(6, 18, 30)));
        }
        c.polygon(&[(231., 35.), (249., 35.), (240., 51.)], MARK);
        for (color, width) in [(rgb(19, 24, 28), 11.), (AIRCRAFT, 5.)] {
            let half = libm::roundf(width / 2.);
            for side in [-1f32, 1.] {
                let x0 = 240. + side * 98.;
                let x1 = 240. + side * 33.;
                let x2 = 240. + side * 20.;
                c.rect(
                    x0.min(x1) - half,
                    240. - half,
                    (x1 - x0).abs() + width,
                    width,
                    color,
                );
                c.rect(x1 - half, 240. - half, width, 11. + width, color);
                c.rect(
                    x1.min(x2) - half,
                    251. - half,
                    (x2 - x1).abs() + width,
                    width,
                    color,
                );
            }
        }
        c.circle(240., 240., 7.5, rgb(19, 24, 28));
        c.circle(240., 240., 5., AIRCRAFT);
    } else {
        let mut c = Canvas::new(disk);
        c.circle(240., 240., 224., FACE);
        c.stroke((117., 164.), (363., 316.), 9., FAILURE);
        c.stroke((363., 164.), (117., 316.), 9., FAILURE);
        c.rect(91., 214., 298., 52., FACE);
        let message = match model.quality {
            Quality::Missing => "ATT DATA MISSING",
            Quality::Stale => "ATT DATA STALE",
            Quality::TimingFault => "ATT TIMING FAULT",
            _ => "ATT DATA FAILED",
        };
        // Center the visible ink vertically within the cross's banner.
        c.text_on(240., 243., 22., message, (FAILURE, FACE));
    }
    captions(disk, source_label);
}
fn captions<T: Target>(disk: &mut Disk<'_, T>, source_label: &str) {
    let mut c = Canvas::new(disk);
    c.rect(207., 362., 66., 34., FACE);
    c.text_on(240., 382., 26., "ATT", (MARK, FACE));
    c.rect(132., 402., 216., 32., FACE);
    c.text_on(240., 421., 21., source_label, (rgb(255, 185, 87), FACE));
}
/// Update only the opaque caption area. At 480 px, source strings of at most
/// twelve glyph cells remain wholly inside that existing opaque backing.
pub(crate) fn refresh_caption(pixels: &mut [u16], source: &str) {
    let mut fb = crate::raster::Framebuffer::new(pixels, 480, 480).unwrap();
    captions(&mut Disk::new(&mut fb), source);
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    #[test]
    fn fixed_face_and_original_digits_fit_the_board_fast_cache() {
        let mut background = std::vec![0; 480 * 480];
        let mut runs = std::vec![CopyRun::default();FOREGROUND_RUN_CAPACITY];
        let count = cache_foreground(&mut background, 480, 480, "TEST INPUT", &mut runs).unwrap();
        let pixels: usize = runs[..count].iter().map(|run| run.len as usize).sum();
        assert!(pixels + crate::raster::DIGIT_CACHE_LEN <= 49152);
        std::println!(
            "board fixed pixels={pixels}, digit pixels={}, runs={count}",
            crate::raster::DIGIT_CACHE_LEN
        );
    }
    #[test]
    fn stepped_band_edges_match_exact_independent_division() {
        for step in [
            -31_457_280,
            -65536,
            -32768,
            -13,
            -1,
            1,
            13,
            32768,
            65536,
            31_457_280,
        ] {
            for base in [-41_222_145, -1, 0, 1, 41_222_145] {
                for (index, edge) in BandGrid::new(step).row(base, 240).into_iter().enumerate() {
                    let k = index as i32 - 14;
                    let n = base - (k << 20);
                    let expected = if step > 0 {
                        240 - n.div_euclid(step)
                    } else {
                        241 + n.div_euclid(-step)
                    };
                    assert_eq!(edge, expected);
                }
            }
        }
    }
    #[test]
    fn rotating_buffers_match_full_redraw_across_banks_extremes_and_faults() {
        use crate::{
            damage::FrameHistory,
            render::{draw_frame, CachedRenderer},
            Snapshot,
        };
        use instrument_core::{App, Command, Instrument};
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.tick(0);
        let base = Snapshot::from_app(&app);
        for (w, h) in [
            (1, 1),
            (240, 240),
            (480, 480),
            (720, 720),
            (241, 239),
            (513, 17),
            (1024, 64),
            (1024, 1024),
        ] {
            let mut bg = std::vec![0;w*h];
            let mut runs = std::vec![CopyRun::default();2048];
            let mut compact = std::vec![0;65536];
            let mut renderer =
                CachedRenderer::with_foreground_cache(&mut bg, w as u32, h as u32, &mut runs)
                    .unwrap()
                    .with_foreground_pixels(&mut compact);
            let mut buffers: [std::vec::Vec<u16>; 3] =
                core::array::from_fn(|_| std::vec![0xffff;w*h]);
            let mut history: [FrameHistory; 3] = core::array::from_fn(|_| FrameHistory::new());
            let mut reference = std::vec![0;w*h];
            for n in 0..80 {
                let mut s = base;
                s.selected = if n % 23 == 0 {
                    Instrument::Altimeter
                } else {
                    Instrument::Attitude
                };
                s.pitch.value = Some([-90., -20., 0., 10., 90.][n % 5]);
                s.roll.value = Some([-180., -90., -30., 0., 30., 89.999, 90., 180.][n % 8]);
                s.roll.quality = [
                    Quality::Valid,
                    Quality::Valid,
                    Quality::Missing,
                    Quality::Stale,
                    Quality::Failed,
                    Quality::TimingFault,
                ][n / 3 % 6];
                let pattern = n % 29 == 0;
                assert!(renderer.draw_incremental(
                    &mut buffers[n % 3],
                    &mut history[n % 3],
                    s,
                    pattern
                ));
                assert!(draw_frame(&mut reference, w as u32, h as u32, s, pattern));
                // Avoid printing megabytes of pixels on a failed assertion.
                assert!(buffers[n % 3] == reference, "frame {n} at {w}x{h}");
                if s.selected == Instrument::Attitude
                    && !pattern
                    && s.roll.quality != Quality::Valid
                {
                    assert!(
                        !buffers[n % 3].contains(&AIRCRAFT),
                        "invalid attitude retained aircraft symbol"
                    );
                }
            }
        }
    }
    #[test]
    fn imu_wait_hold_and_recovery_restore_every_buffer() {
        use crate::{
            damage::FrameHistory,
            render::{draw_frame, CachedRenderer},
            Snapshot,
        };
        use instrument_core::{imu_attitude::Retained, App, Command, Instrument};
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.apply(Command::Select(Instrument::Attitude)).unwrap();
        app.tick(0);
        let base = Snapshot::from_app(&app);
        for size in [240, 480] {
            let mut bg = std::vec![0; size*size];
            let mut runs = std::vec![CopyRun::default();2048];
            let mut compact = std::vec![0;65536];
            let mut renderer =
                CachedRenderer::with_foreground_cache(&mut bg, size as u32, size as u32, &mut runs)
                    .unwrap()
                    .with_foreground_pixels(&mut compact);
            let mut buffers: [std::vec::Vec<u16>; 3] =
                core::array::from_fn(|_| std::vec![0xffff;size*size]);
            let mut histories: [FrameHistory; 3] = core::array::from_fn(|_| FrameHistory::new());
            let mut reference = std::vec![0;size*size];
            for n in 0..60 {
                let mut s = base;
                s.imu_bench = true;
                s.source_label = "IMU BENCH";
                let phase = (n / 5) % 4;
                s.pitch.value = Some(12.);
                s.roll.value = Some(-23.);
                if phase != 0 {
                    s.imu_last_good = Some(Retained {
                        pitch_roll_deg: [12., -23.],
                        age_ms: 5000,
                    });
                }
                if phase == 0 || phase == 2 {
                    s.pitch.value = None;
                    s.roll.value = None;
                    s.pitch.quality = Quality::Missing;
                    s.roll.quality = Quality::Missing;
                }
                assert!(renderer.draw_incremental(
                    &mut buffers[n % 3],
                    &mut histories[n % 3],
                    s,
                    false
                ));
                let previous = reference.clone();
                assert!(draw_frame(
                    &mut reference,
                    size as u32,
                    size as u32,
                    s,
                    false
                ));
                if phase == 2 {
                    assert!(reference == previous, "Held frame must look unchanged");
                }
                assert!(
                    buffers[n % 3] == reference,
                    "IMU frame {n}, phase {phase}, size {size}"
                );
            }
        }
    }
    #[test]
    fn small_continuous_motion_restores_all_three_buffers_exactly() {
        use crate::{
            damage::FrameHistory,
            render::{draw_frame, CachedRenderer},
            Snapshot,
        };
        use instrument_core::{App, Command, Instrument};
        let mut app = App::new();
        app.apply(Command::LevelPreset).unwrap();
        app.apply(Command::Select(Instrument::Attitude)).unwrap();
        app.tick(0);
        let mut s = Snapshot::from_app(&app);
        let mut bg = std::vec![0;480*480];
        let mut runs = std::vec![CopyRun::default();2048];
        let mut compact = std::vec![0;65536];
        let mut renderer = CachedRenderer::with_foreground_cache(&mut bg, 480, 480, &mut runs)
            .unwrap()
            .with_foreground_pixels(&mut compact);
        let mut frames: [std::vec::Vec<u16>; 3] = core::array::from_fn(|_| std::vec![0;480*480]);
        let mut histories: [FrameHistory; 3] = core::array::from_fn(|_| FrameHistory::new());
        let mut expected = std::vec![0;480*480];
        for n in 0..120 {
            s.pitch.value = Some(-4. + n as f32 * 0.067);
            s.roll.value = Some(-9. + n as f32 * 0.151);
            s.source_label = ["SIM INPUT", "TEST INPUT", "IMU BENCH"][(n / 7) % 3];
            let mut notifications = 0;
            assert!(renderer.draw_incremental_with_progress(
                &mut frames[n % 3],
                &mut histories[n % 3],
                s,
                false,
                &mut || notifications += 1
            ));
            assert!(notifications >= 2);
            assert!(draw_frame(&mut expected, 480, 480, s, false));
            assert!(frames[n % 3] == expected, "small motion frame {n}");
        }
    }
    #[test]
    fn horizon_is_clipped_and_colors_follow_independent_inverse_projection() {
        use crate::raster::Framebuffer;
        for (pitch, roll) in [
            (0., 0.),
            (10., 30.),
            (-10., -30.),
            (0., 90.),
            (0., -90.),
            (0., 180.),
            (90., 0.),
        ] {
            let mut pixels = std::vec![0xffff;480*480];
            let mut fb = Framebuffer::new(&mut pixels, 480, 480).unwrap();
            horizon(
                &mut Disk::new(&mut fb),
                &Pose::new(Angles {
                    pitch_deg: pitch,
                    roll_deg: roll,
                }),
                None,
                None,
                &mut || {},
            );
            assert_eq!(pixels[0], 0xffff);
            let r = roll as f64 * core::f64::consts::PI / 180.;
            for y in (80..400).step_by(19) {
                for x in (80..400).step_by(17) {
                    if (x as i32 - 240).pow(2) + (y as i32 - 240).pow(2) > 224 * 224 {
                        continue;
                    }
                    let v = r.sin() * (x as f64 - 240.) + r.cos() * (y as f64 - 240.)
                        - pitch as f64 * 4.5;
                    // Compare away from quantization edges (f32/f64 rounding differs).
                    if (v / 16. - (v / 16.).round()).abs() < 0.001 {
                        continue;
                    }
                    let expected = gradient(((v / 16.).floor() as i32).clamp(-15, 14));
                    assert_eq!(
                        pixels[y * 480 + x],
                        expected,
                        "pitch={pitch}, roll={roll}, pixel=({x},{y})"
                    );
                }
            }
        }
    }
    fn reading(v: f32) -> Reading {
        Reading {
            value: Some(v),
            quality: Quality::Valid,
            age_ms: Some(0),
        }
    }
    #[test]
    fn sign_conventions_match_phone_reference() {
        let nose_up = Pose::new(Angles {
            pitch_deg: 10.,
            roll_deg: 0.,
        });
        assert_eq!(nose_up.point(0., 0.), (240., 285.));
        let bank = Pose::new(Angles {
            pitch_deg: 0.,
            roll_deg: 30.,
        });
        assert!(bank.point(-100., 0.).1 > 240. && bank.point(100., 0.).1 < 240.);
        let combined = Pose::new(Angles {
            pitch_deg: 10.,
            roll_deg: 90.,
        });
        let center = combined.point(0., 0.);
        assert!((center.0 - 285.).abs() < 0.001 && (center.1 - 240.).abs() < 0.001);
    }
    #[test]
    fn both_axes_must_be_fresh_finite_and_in_range() {
        assert!(model(reading(0.), reading(0.)).angles.is_some());
        for axis in [0, 1] {
            for quality in [
                Quality::Missing,
                Quality::Stale,
                Quality::Failed,
                Quality::TimingFault,
            ] {
                let mut pair = [reading(0.); 2];
                pair[axis].quality = quality;
                assert!(model(pair[0], pair[1]).angles.is_none());
                assert_eq!(model(pair[0], pair[1]).quality, quality);
            }
            for value in [f32::NAN, f32::INFINITY, 181.] {
                let mut pair = [reading(0.); 2];
                pair[axis].value = Some(value);
                assert!(model(pair[0], pair[1]).angles.is_none());
            }
            for age in [None, Some(100), Some(u64::MAX)] {
                let mut pair = [reading(0.); 2];
                pair[axis].age_ms = age;
                assert!(model(pair[0], pair[1]).angles.is_none());
            }
        }
        assert!(model(reading(90.01), reading(0.)).angles.is_none());
        assert!(model(reading(-90.), reading(-180.)).angles.is_some());
    }
}
