/// Portable pixel sink; no framebuffer, RGB bus, or ESP dependency required.
pub trait Target {
    fn size(&self) -> (u32, u32);
    fn pixel(&mut self, x: i32, y: i32, color: u16);
    /// Horizontal run, end-exclusive. Backends may batch contiguous stores.
    fn span(&mut self, y: i32, start: i32, end: i32, color: u16) {
        for x in start..end {
            self.pixel(x, y, color);
        }
    }
}
pub const fn rgb(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3)
}
pub struct Framebuffer<'a> {
    pixels: &'a mut [u16],
    width: u32,
    height: u32,
}
impl<'a> Framebuffer<'a> {
    pub fn new(pixels: &'a mut [u16], width: u32, height: u32) -> Option<Self> {
        if width == 0
            || height == 0
            || width > 1024
            || height > 1024
            || pixels.len() != width as usize * height as usize
        {
            return None;
        }
        Some(Self {
            pixels,
            width,
            height,
        })
    }
}
impl Target for Framebuffer<'_> {
    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    fn pixel(&mut self, x: i32, y: i32, color: u16) {
        if x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32 {
            self.pixels[y as usize * self.width as usize + x as usize] = color;
        }
    }
    fn span(&mut self, y: i32, start: i32, end: i32, color: u16) {
        let start = start.max(0).min(self.width as i32) as usize;
        let end = end.max(0).min(self.width as i32) as usize;
        if y >= 0 && y < self.height as i32 && start < end {
            let row = y as usize * self.width as usize;
            self.pixels[row + start..row + end].fill(color);
        }
    }
}
/// Logical 480-unit centered viewport, scaled uniformly, with physical bounds.
pub struct Canvas<'a, T: Target> {
    target: &'a mut T,
    scale: f32,
    ox: f32,
    oy: f32,
}
impl<'a, T: Target> Canvas<'a, T> {
    pub fn new(target: &'a mut T) -> Self {
        let (w, h) = target.size();
        let s = w.min(h) as f32 / 480.;
        Self {
            target,
            scale: s,
            ox: (w as f32 - 480. * s) / 2.,
            oy: (h as f32 - 480. * s) / 2.,
        }
    }
    fn point(&self, x: f32, y: f32) -> (i32, i32) {
        (
            (self.ox + x * self.scale) as i32,
            (self.oy + y * self.scale) as i32,
        )
    }
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: u16) {
        let (a, b) = self.point(x, y);
        let (d, e) = self.point(x + w, y + h);
        for py in b..e {
            self.target.span(py, a, d, c);
        }
    }
    pub fn circle(&mut self, x: f32, y: f32, r: f32, c: u16) {
        let (cx, cy) = self.point(x, y);
        let r = (r * self.scale) as i32;
        for dy in -r..=r {
            let dx = libm::sqrtf((r * r - dy * dy) as f32) as i32;
            self.target.span(cy + dy, cx - dx, cx + dx + 1, c);
        }
    }
    pub fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, c: u16) {
        let n = (libm::fabsf(b.0 - a.0).max(libm::fabsf(b.1 - a.1)) * self.scale) as i32;
        for i in 0..=n.max(1) {
            let t = i as f32 / n.max(1) as f32;
            self.circle(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, width / 2., c);
        }
    }
    pub fn polygon(&mut self, points: &[(f32, f32)], c: u16) {
        // Convex hand shapes only; fixed stack storage and bounded scan conversion.
        if points.len() < 3 || points.len() > 8 {
            return;
        }
        let mut p = [(0i32, 0i32); 8];
        for (i, &(x, y)) in points.iter().enumerate() {
            p[i] = self.point(x, y);
        }
        let p = &p[..points.len()];
        let min = p.iter().map(|p| p.1).min().unwrap_or(0);
        let max = p.iter().map(|p| p.1).max().unwrap_or(0);
        for y in min..=max {
            let mut lo = i32::MAX;
            let mut hi = i32::MIN;
            for i in 0..p.len() {
                let (a, b) = (p[i], p[(i + 1) % p.len()]);
                if a.1 == b.1 {
                    continue;
                }
                if y >= a.1.min(b.1) && y < a.1.max(b.1) {
                    let x = a.0 + (y - a.1) * (b.0 - a.0) / (b.1 - a.1);
                    lo = lo.min(x);
                    hi = hi.max(x);
                }
            }
            if lo <= hi {
                self.target.span(y, lo, hi + 1, c);
            }
        }
    }
    pub fn text(&mut self, x: f32, y: f32, size: f32, text: &str, color: u16) {
        const FONT: &[u8] = include_bytes!("../assets/glyphs-4bit.bin");
        let cell = size * 40. / 48.;
        let h = size * 56. / 48.;
        let start = x - cell * text.len() as f32 / 2.;
        for (i, ch) in text.bytes().enumerate() {
            if !(32..=90).contains(&ch) {
                continue;
            }
            let (left, top) = self.point(start + i as f32 * cell, y - h / 2.);
            let (w, ht) = (
                (cell * self.scale).max(1.) as i32,
                (h * self.scale).max(1.) as i32,
            );
            for dy in 0..ht {
                for dx in 0..w {
                    let k = (ch as usize - 32) * 2240
                        + (dy * 56 / ht) as usize * 40
                        + (dx * 40 / w) as usize;
                    let b = FONT[k / 2];
                    let alpha = if k % 2 == 0 { b >> 4 } else { b & 15 };
                    if alpha > 0 {
                        let a = alpha as u16;
                        let r = ((color >> 11) & 31) * a / 15;
                        let g = ((color >> 5) & 63) * a / 15;
                        let b = (color & 31) * a / 15;
                        self.target
                            .pixel(left + dx, top + dy, (r << 11) | (g << 5) | b);
                    }
                }
            }
        }
    }
}
