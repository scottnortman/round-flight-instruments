//! Caller-owned damage history for ONE framebuffer. Any overlay write marks a
//! 8x8 tile (16x8 above 512 pixels wide), including clears and glyphs. The next
//! draw restores every marked tile from the static backdrop before drawing.
use crate::raster::{Framebuffer, Target};
use instrument_core::Instrument;

pub(crate) type Key = (u32, u32, Instrument, bool, bool);
pub(crate) fn tile_shift(width: u32) -> u32 {
    if width <= 512 {
        3
    } else {
        4
    }
}
pub struct FrameHistory {
    pub(crate) key: Option<Key>,
    // One u64 per 8-pixel row supports the maximum 1024x1024 framebuffer.
    rows: [u64; 128],
    pub(crate) attitude: Option<crate::attitude::Angles>,
    pub(crate) menu: Option<crate::navigation::View>,
}
impl Default for FrameHistory {
    fn default() -> Self {
        Self::new()
    }
}
impl FrameHistory {
    pub const fn new() -> Self {
        Self {
            key: None,
            rows: [0; 128],
            attitude: None,
            menu: None,
        }
    }
    pub(crate) fn dirty_row(&self, y: i32) -> u64 {
        self.rows[y as usize / 8]
    }
    pub(crate) fn mark_rect(&mut self, width: u32, rect: crate::interaction::Rect) {
        let first = rect.x as u32 >> tile_shift(width);
        let last = (rect.x as u32 + rect.w as u32 - 1) >> tile_shift(width);
        let mask = (u64::MAX << first) & (u64::MAX >> (63 - last));
        for y in rect.y as usize / 8..(rect.y as usize + rect.h as usize).div_ceil(8) {
            self.rows[y] |= mask;
        }
    }
    pub(crate) fn clear(&mut self) {
        self.rows.fill(0);
        self.attitude = None;
        self.menu = None;
    }
    pub(crate) fn restore(
        &mut self,
        pixels: &mut [u16],
        background: &[u16],
        width: usize,
        height: usize,
    ) {
        let tile_width = 1usize << tile_shift(width as u32);
        for ty in 0..height.div_ceil(8) {
            let mut tiles = self.rows[ty];
            while tiles != 0 {
                let tx = tiles.trailing_zeros() as usize;
                let end = tx + (tiles >> tx).trailing_ones() as usize;
                // Adjacent dirty tiles are one contiguous PSRAM copy per row.
                // A rotating card otherwise makes thousands of 16-byte copies.
                tiles &= if end == 64 { 0 } else { u64::MAX << end };
                let x0 = tx * tile_width;
                let x1 = (end * tile_width).min(width);
                for y in ty * 8..((ty + 1) * 8).min(height) {
                    let range = y * width + x0..y * width + x1;
                    pixels[range.clone()].copy_from_slice(&background[range]);
                }
            }
        }
        self.clear();
    }
}

pub(crate) struct Tracked<'a, 'b> {
    pub target: Framebuffer<'a>,
    pub history: &'b mut FrameHistory,
    pub last_pixel_tile: u16,
}
impl Target for Tracked<'_, '_> {
    fn size(&self) -> (u32, u32) {
        self.target.size()
    }
    fn pixel(&mut self, x: i32, y: i32, color: u16) {
        let (w, h) = self.size();
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
            return;
        }
        let shift = tile_shift(w);
        let tile = ((y as u16 / 8) * 64) + (x as u16 >> shift);
        if self.last_pixel_tile != tile {
            self.history.rows[y as usize / 8] |= 1u64 << (x as usize >> shift);
            self.last_pixel_tile = tile;
        }
        self.target.pixel(x, y, color);
    }
    fn pixels(&mut self, y: i32, start: i32, colors: &[Option<u16>]) {
        let (w, h) = self.size();
        let lo = start.max(0).min(w as i32);
        let hi = (start.saturating_add(colors.len().min(i32::MAX as usize) as i32))
            .max(0)
            .min(w as i32);
        if y < 0 || y >= h as i32 || lo >= hi {
            return;
        }
        let first = lo as u32 >> tile_shift(w);
        let last = (hi as u32 - 1) >> tile_shift(w);
        self.history.rows[y as usize / 8] |= (u64::MAX << first) & (u64::MAX >> (63 - last));
        self.target.pixels(y, start, colors);
    }
    fn span(&mut self, y: i32, start: i32, end: i32, color: u16) {
        let (w, h) = self.size();
        let start = start.max(0).min(w as i32);
        let end = end.max(0).min(w as i32);
        if y < 0 || y >= h as i32 || start >= end {
            return;
        }
        let first = start as u32 >> tile_shift(w);
        let last = (end as u32 - 1) >> tile_shift(w);
        self.history.rows[y as usize / 8] |= (u64::MAX << first) & (u64::MAX >> (63 - last));
        self.target.span(y, start, end, color);
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    #[test]
    fn clipped_runs_and_partial_edge_tiles_restore_exactly() {
        for (w, h) in [(1, 1), (17, 19), (512, 32), (513, 32), (1024, 32)] {
            let bg = std::vec![123; w * h];
            let mut pixels = bg.clone();
            let mut history = FrameHistory::new();
            {
                let mut target = Tracked {
                    last_pixel_tile: u16::MAX,
                    target: Framebuffer::new(&mut pixels, w as u32, h as u32).unwrap(),
                    history: &mut history,
                };
                target.span(0, -100, w as i32 + 100, 456);
                target.span(h as i32 - 1, w as i32 - 2, w as i32 + 10, 789);
                target.pixel(-1, -1, 0);
                target.pixel(w as i32, h as i32, 0);
            }
            assert_ne!(pixels, bg);
            history.restore(&mut pixels, &bg, w, h);
            assert_eq!(pixels, bg);
            assert!(history.rows.iter().all(|r| *r == 0));
        }
    }
}
