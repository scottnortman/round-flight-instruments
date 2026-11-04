static FONT: &[u8; 66080] = include_bytes!("../assets/glyphs-4bit.bin");

/// Portable pixel sink; no framebuffer, RGB bus, or ESP dependency required.
pub trait Target {
    fn size(&self) -> (u32, u32);
    fn pixel(&mut self, x: i32, y: i32, color: u16);
    /// Variable-color row; None leaves the destination unchanged, including
    /// when an actual black (Some(0)) pixel must be drawn.
    fn pixels(&mut self, y: i32, start: i32, colors: &[Option<u16>]) {
        for (offset, color) in colors.iter().enumerate() {
            if let Some(color) = color {
                self.pixel(start + offset as i32, y, *color);
            }
        }
    }
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
    fn pixels(&mut self, y: i32, start: i32, colors: &[Option<u16>]) {
        if y < 0 || y >= self.height as i32 {
            return;
        }
        let lo = start.max(0).min(self.width as i32);
        let hi = (start.saturating_add(colors.len().min(i32::MAX as usize) as i32))
            .max(0)
            .min(self.width as i32);
        if lo >= hi {
            return;
        }
        let source = &colors[(lo - start) as usize..(hi - start) as usize];
        let offset = y as usize * self.width as usize;
        for (pixel, color) in self.pixels[offset + lo as usize..offset + hi as usize]
            .iter_mut()
            .zip(source)
        {
            if let Some(color) = color {
                *pixel = *color;
            }
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
pub(crate) const DIGIT_CACHE_LEN: usize = 7 * 2240;
/// Expanded original 0..6 glyphs for the attitude ladder, in caller fast memory.
/// Zero is transparent; the ladder ink has nonzero RGB565 at every alpha > 0.
pub(crate) fn cache_ladder_digits(pixels: &mut [u16], color: u16) {
    for (index, pixel) in pixels[..DIGIT_CACHE_LEN].iter_mut().enumerate() {
        let k = (b'0' as usize - 32) * 2240 + index;
        let packed = FONT[k / 2];
        let alpha = if k % 2 == 0 { packed >> 4 } else { packed & 15 } as u16;
        let blend = |shift: u32, mask: u16| ((color >> shift) & mask) * alpha / 15;
        *pixel = (blend(11, 31) << 11) | (blend(5, 63) << 5) | blend(0, 31);
    }
}
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
        let (top, bottom) = (0, self.target.size().1 as i32);
        for dy in (-r).max(top - cy)..=r.min(bottom - 1 - cy) {
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
    /// Pixel-aligned square-brush stroke. Unlike the circular line brush, this
    /// supports even pixel widths without rounding a two-pixel edge to three.
    pub fn stroke(&mut self, a: (f32, f32), b: (f32, f32), width: f32, c: u16) {
        let (ax, ay) = self.point(a.0, a.1);
        let (bx, by) = self.point(b.0, b.1);
        let steps = (bx - ax).abs().max((by - ay).abs()).max(1);
        let side = (libm::roundf(width * self.scale) as i32).max(1);
        let (top, bottom) = (0, self.target.size().1 as i32);
        if ay.max(by) + side < top || ay.min(by) - side >= bottom {
            return;
        }
        for i in 0..=steps {
            let x = ax + (bx - ax) * i / steps - side / 2;
            let y = ay + (by - ay) * i / steps - side / 2;
            for row in y.max(top)..(y + side).min(bottom) {
                self.target.span(row, x, x + side, c);
            }
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
        let (top, bottom) = (0, self.target.size().1 as i32);
        for y in min.max(top)..=max.min(bottom - 1) {
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
        self.text_style(x, y, size, text, (color, 0), false);
    }
    /// Inverse-sampled rotated glyphs: no holes from forward-mapping pixels.
    /// Uses the same original font and black antialias backing as `text`.
    pub fn text_rotated<const N: usize>(
        &mut self,
        centers: [(f32, f32); N],
        size: f32,
        text: &str,
        degrees: f32,
        color: u16,
        digit_cache: Option<&[u16]>,
    ) {
        let a = degrees * core::f32::consts::PI / 180.;
        let (s, c) = (libm::sinf(a), libm::cosf(a));
        let cell = size * 40. / 48. * self.scale;
        let height = size * 56. / 48. * self.scale;
        let width = cell * text.len() as f32;
        let glyph_xscale = 40. / cell;
        let glyph_yscale = 56. / height;
        let centers = centers.map(|(x, y)| self.point(x, y));
        let rx = libm::ceilf((c.abs() * width + s.abs() * height) / 2.) as i32;
        let ry = libm::ceilf((s.abs() * width + c.abs() * height) / 2.) as i32;
        // Fixed-point inverse sampling removes floating division/conversion from
        // the inner pixel loop. Error stays below a source-pixel fraction.
        let du = (c * glyph_xscale * 65536.) as i32;
        let dv = (-s * glyph_yscale * 65536.) as i32;
        let shades = core::array::from_fn::<_, 16, _>(|alpha| {
            let blend = |shift: u32, mask: u16| ((color >> shift) & mask) * alpha as u16 / 15;
            (blend(11, 31) << 11) | (blend(5, 63) << 5) | blend(0, 31)
        });
        for y in -ry..=ry {
            let dx = -rx as f32;
            let dy = y as f32;
            let mut u = ((c * dx + s * dy + width / 2.) * glyph_xscale * 65536.) as i32;
            let mut v = ((-s * dx + c * dy + height / 2.) * glyph_yscale * 65536.) as i32;
            let mut row = [None; 64];
            let mut used = 0;
            let mut start = -rx;
            for x in -rx..=rx {
                row[used] = None;
                let sx = u >> 16;
                let sy = v >> 16;
                if sx >= 0 && sx < (text.len() * 40) as i32 && (0..56).contains(&sy) {
                    let ch = text.as_bytes()[sx as usize / 40];
                    if (32..=90).contains(&ch) {
                        let glyph_pixel = sy as usize * 40 + sx as usize % 40;
                        let ink = if let Some(cache) = digit_cache
                            .filter(|c| c.len() >= DIGIT_CACHE_LEN && (b'0'..=b'6').contains(&ch))
                        {
                            let color = cache[(ch - b'0') as usize * 2240 + glyph_pixel];
                            (color != 0).then_some(color)
                        } else {
                            let k = (ch as usize - 32) * 2240 + glyph_pixel;
                            let packed = FONT[k / 2];
                            let alpha = if k % 2 == 0 { packed >> 4 } else { packed & 15 };
                            (alpha != 0).then_some(shades[alpha as usize])
                        };
                        row[used] = ink;
                    }
                }
                used += 1;
                if used == row.len() || x == rx {
                    if let Some(first) = row[..used].iter().position(Option::is_some) {
                        let last = row[..used].iter().rposition(Option::is_some).unwrap() + 1;
                        for (cx, cy) in centers {
                            self.target.pixels(
                                cy + y,
                                cx + start + first as i32,
                                &row[first..last],
                            );
                        }
                    }
                    start = x + 1;
                    used = 0;
                }
                u += du;
                v += dv;
            }
        }
    }
    /// All two-digit pitch labels share one inverse transform. Their second
    /// digit is always zero; sample it once for all active first digits. Rows
    /// remain bounded stack storage and preserve the reference font pixels.
    pub(crate) fn ladder_labels(
        &mut self,
        labels: &[(u8, (f32, f32))],
        degrees: f32,
        color: u16,
        digit_cache: Option<&[u16]>,
    ) {
        let mut centers = [(0u8, (0, 0)); 12];
        let mut active = [false; 6];
        assert!(labels.len() <= centers.len());
        for (out, &(digit, center)) in centers.iter_mut().zip(labels) {
            assert!((1..=6).contains(&digit));
            *out = (digit - 1, self.point(center.0, center.1));
            active[digit as usize - 1] = true;
        }
        let a = degrees * core::f32::consts::PI / 180.;
        let (s, c) = (libm::sinf(a), libm::cosf(a));
        let cell = 22. * 40. / 48. * self.scale;
        let height = 22. * 56. / 48. * self.scale;
        let width = cell * 2.;
        let (xs, ys) = (40. / cell, 56. / height);
        let rx = libm::ceilf((c.abs() * width + s.abs() * height) / 2.) as i32;
        let ry = libm::ceilf((s.abs() * width + c.abs() * height) / 2.) as i32;
        let du = (c * xs * 65536.) as i32;
        let dv = (-s * ys * 65536.) as i32;
        let shades = core::array::from_fn::<_, 16, _>(|alpha| {
            let blend = |shift: u32, mask: u16| ((color >> shift) & mask) * alpha as u16 / 15;
            (blend(11, 31) << 11) | (blend(5, 63) << 5) | blend(0, 31)
        });
        let cache = digit_cache.filter(|c| c.len() >= DIGIT_CACHE_LEN);
        let ink = |digit: usize, pixel: usize| {
            if let Some(cache) = cache {
                let color = cache[digit * 2240 + pixel];
                (color != 0).then_some(color)
            } else {
                let k = (b'0' as usize + digit - 32) * 2240 + pixel;
                let packed = FONT[k / 2];
                let alpha = if k % 2 == 0 { packed >> 4 } else { packed & 15 };
                (alpha != 0).then_some(shades[alpha as usize])
            }
        };
        for y in -ry..=ry {
            let (dx, dy) = (-rx as f32, y as f32);
            let mut u = ((c * dx + s * dy + width / 2.) * xs * 65536.) as i32;
            let mut v = ((-s * dx + c * dy + height / 2.) * ys * 65536.) as i32;
            let mut rows = [[None; 64]; 6];
            let mut used = 0;
            let mut start = -rx;
            for x in -rx..=rx {
                let (sx, sy) = (u >> 16, v >> 16);
                if (0..80).contains(&sx) && (0..56).contains(&sy) {
                    let pixel = sy as usize * 40 + sx as usize % 40;
                    let zero = if sx >= 40 { ink(0, pixel) } else { None };
                    for (digit, row) in rows.iter_mut().enumerate() {
                        if active[digit] {
                            row[used] = if sx >= 40 {
                                zero
                            } else {
                                ink(digit + 1, pixel)
                            };
                        }
                    }
                } else {
                    for row in &mut rows {
                        row[used] = None;
                    }
                }
                used += 1;
                if used == 64 || x == rx {
                    for (digit, row) in rows.iter().enumerate() {
                        if !active[digit] {
                            continue;
                        }
                        if let Some(first) = row[..used].iter().position(Option::is_some) {
                            let last = row[..used].iter().rposition(Option::is_some).unwrap() + 1;
                            for &(code, (cx, cy)) in &centers[..labels.len()] {
                                if code as usize == digit {
                                    self.target.pixels(
                                        cy + y,
                                        cx + start + first as i32,
                                        &row[first..last],
                                    );
                                }
                            }
                        }
                    }
                    used = 0;
                    start = x + 1;
                }
                u += du;
                v += dv;
            }
        }
    }
    /// Regular glyphs composited against a caller-filled solid background.
    pub fn text_on(&mut self, x: f32, y: f32, size: f32, text: &str, colors: (u16, u16)) {
        self.text_style(x, y, size, text, colors, false);
    }
    /// Bold glyphs composited against an explicit solid background. Combining
    /// coverage before drawing avoids dark seams from overlapping glyph passes.
    /// Colors are (foreground, background); the caller fills the background.
    pub fn text_bold_on(&mut self, x: f32, y: f32, size: f32, text: &str, colors: (u16, u16)) {
        self.text_style(x, y, size, text, colors, true);
    }
    fn text_style(
        &mut self,
        x: f32,
        y: f32,
        size: f32,
        text: &str,
        colors: (u16, u16),
        bold: bool,
    ) {
        let (color, background) = colors;
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
            let (clip_top, clip_bottom) = (0, self.target.size().1 as i32);
            for dy in 0.max(clip_top - top)..ht.min(clip_bottom - top) {
                let mut colors = [None; 64];
                let mut used = 0;
                let mut start = 0;
                for dx in 0..w {
                    colors[used] = None;
                    let row = (ch as usize - 32) * 2240 + (dy * 56 / ht) as usize * 40;
                    let sx = (dx * 40 / w) as usize;
                    let coverage = |x: usize| {
                        let k = row + x;
                        let b = FONT[k / 2];
                        if k % 2 == 0 {
                            b >> 4
                        } else {
                            b & 15
                        }
                    };
                    let mut alpha = coverage(sx);
                    if bold {
                        // One source-pixel horizontal dilation, independent of
                        // viewport size. Never sample outside this glyph cell.
                        alpha = alpha
                            .max(coverage(sx.saturating_sub(1)))
                            .max(coverage((sx + 1).min(39)));
                    }
                    if alpha > 0 {
                        let a = alpha as u16;
                        let blend = |shift: u32, mask: u16| {
                            (((color >> shift) & mask) * a
                                + ((background >> shift) & mask) * (15 - a))
                                / 15
                        };
                        let r = blend(11, 31);
                        let g = blend(5, 63);
                        let b = blend(0, 31);
                        colors[used] = Some((r << 11) | (g << 5) | b);
                    }
                    used += 1;
                    if used == colors.len() || dx == w - 1 {
                        if let Some(first) = colors[..used].iter().position(Option::is_some) {
                            let last = colors[..used].iter().rposition(Option::is_some).unwrap() + 1;
                            self.target.pixels(top + dy, left + start + first as i32, &colors[first..last]);
                        }
                        used = 0;
                        start = dx + 1;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    #[test]
    fn batched_regular_and_bold_text_preserve_reference_pixels() {
        for size in [240,480,1024] {
            for font in [14.,32.,96.] {
                for bold in [false,true] {
                    let mut actual=std::vec![123;size*size];
                    let mut expected=actual.clone();
                    let mut fb=Framebuffer::new(&mut expected,size as u32,size as u32).unwrap();
                    let c=Canvas::new(&mut fb);
                    let text="0NW 35-";let color=rgb(238,240,230);let background=rgb(8,11,14);
                    let cell=font*40./48.;let h=font*56./48.;let start=240.-cell*text.len()as f32/2.;
                    for (i,ch) in text.bytes().enumerate() {
                        let(left,top)=c.point(start+i as f32*cell,240.-h/2.);
                        let(w,ht)=((cell*c.scale).max(1.)as i32,(h*c.scale).max(1.)as i32);
                        for dy in 0..ht {for dx in 0..w {
                            let row=(ch as usize-32)*2240+(dy*56/ht)as usize*40;let sx=(dx*40/w)as usize;
                            let ink=|x:usize|{let k=row+x;let b=FONT[k/2];if k%2==0{b>>4}else{b&15}};
                            let mut a=ink(sx);if bold{a=a.max(ink(sx.saturating_sub(1))).max(ink((sx+1).min(39)));}
                            if a>0 {let a=a as u16;let blend=|shift:u32,mask:u16|(((color>>shift)&mask)*a+((background>>shift)&mask)*(15-a))/15;
                                let(x,y)=(left+dx,top+dy);
                                c.target.pixel(x,y,(blend(11,31)<<11)|(blend(5,63)<<5)|blend(0,31));
                            }
                        }}
                    }
                    Canvas::new(&mut Framebuffer::new(&mut actual,size as u32,size as u32).unwrap()).text_style(240.,240.,font,text,(color,background),bold);
                    assert!(actual==expected,"size {size}, font {font}, bold {bold}");
                }
            }
        }
    }
    #[test]
    fn framebuffer_rejects_dimensions_outside_the_raster_domain() {
        let mut pixels = [0; 4];
        for (w, h) in [
            (0, 4),
            (4, 0),
            (1025, 1),
            (1, 1025),
            (u32::MAX, u32::MAX),
            (2, 3),
        ] {
            assert!(Framebuffer::new(&mut pixels, w, h).is_none());
        }
        assert!(Framebuffer::new(&mut pixels, 2, 2).is_some());
    }
    #[test]
    fn batched_pitch_labels_match_reference_at_all_scales_and_angles() {
        let color = rgb(241, 242, 235);
        let mut cache = std::vec![0; DIGIT_CACHE_LEN];
        cache_ladder_digits(&mut cache, color);
        for size in [240, 480, 1024] {
            for degrees in [-180., -91., -30., 0., 27., 90., 179.] {
                let labels = [
                    (1, (0., 0.)),
                    (2, (175., 180.)),
                    (3, (305., 220.)),
                    (4, (100., 300.)),
                    (5, (380., 350.)),
                    (6, (479., 479.)),
                ];
                let mut expected = std::vec![123; size * size];
                let mut fb = Framebuffer::new(&mut expected, size as u32, size as u32).unwrap();
                let mut c = Canvas::new(&mut fb);
                for &(digit, center) in &labels {
                    let text = [b'0' + digit, b'0'];
                    c.text_rotated(
                        [center],
                        22.,
                        core::str::from_utf8(&text).unwrap(),
                        degrees,
                        color,
                        None,
                    );
                }
                for cached in [None, Some(cache.as_slice())] {
                    let mut actual = std::vec![123; size * size];
                    Canvas::new(
                        &mut Framebuffer::new(&mut actual, size as u32, size as u32).unwrap(),
                    )
                    .ladder_labels(&labels, degrees, color, cached);
                    assert!(
                        actual == expected,
                        "size {size} angle {degrees} cache {}",
                        cached.is_some()
                    );
                }
            }
        }
    }
    #[test]
    fn color_rows_clip_and_preserve_transparency_and_black() {
        for start in [-64, -2, 0, 15, 18, i32::MAX] {
            for y in [-1, 0, 2] {
                let colors = [None, Some(0), Some(123), None, Some(456)];
                let mut expected = [99; 34];
                for (offset, color) in colors.iter().enumerate() {
                    let x = start as i64 + offset as i64;
                    if (0..17).contains(&x) && (0..2).contains(&y) {
                        if let Some(color) = color {
                            expected[y as usize * 17 + x as usize] = *color;
                        }
                    }
                }
                let mut actual = [99; 34];
                Framebuffer::new(&mut actual, 17, 2)
                    .unwrap()
                    .pixels(y, start, &colors);
                assert_eq!(actual, expected);
            }
        }
    }
    #[test]
    fn paired_rotated_glyphs_match_separate_copies_including_clipping() {
        for degrees in [-180., -91., -30., 0., 27., 90., 179.] {
            for centers in [[(175., 180.), (305., 220.)], [(0., 0.), (479., 479.)]] {
                let mut pair = std::vec![0; 480 * 480];
                let mut single = pair.clone();
                Canvas::new(&mut Framebuffer::new(&mut pair, 480, 480).unwrap()).text_rotated(
                    centers,
                    22.,
                    "20",
                    degrees,
                    rgb(241, 242, 235),
                    None,
                );
                let mut fb = Framebuffer::new(&mut single, 480, 480).unwrap();
                let mut c = Canvas::new(&mut fb);
                for center in centers {
                    c.text_rotated([center], 22., "20", degrees, rgb(241, 242, 235), None);
                }
                assert_eq!(pair, single, "angle {degrees}");
            }
        }
    }
}
