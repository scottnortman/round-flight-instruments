//! Caller-owned damage history for ONE framebuffer. Any overlay write marks a
//! 16x16 tile, including clears, text alpha, faults and needle pixels. The next
//! draw restores every marked tile from the static backdrop before drawing.
use crate::raster::{Framebuffer, Target};
use instrument_core::Instrument;

pub(crate) type Key = (u32, u32, Instrument, u32, bool);
pub struct FrameHistory {
    pub(crate) key: Option<Key>,
    // One u64 per tile row supports the framebuffer's maximum 1024x1024 size.
    rows: [u64; 64],
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
            rows: [0; 64],
        }
    }
    pub(crate) fn clear(&mut self) {
        self.rows.fill(0);
    }
    pub(crate) fn restore(
        &mut self,
        pixels: &mut [u16],
        background: &[u16],
        width: usize,
        height: usize,
    ) {
        for ty in 0..height.div_ceil(16) {
            let mut tiles = self.rows[ty];
            while tiles != 0 {
                let tx = tiles.trailing_zeros() as usize;
                tiles &= tiles - 1;
                let x0 = tx * 16;
                let x1 = (x0 + 16).min(width);
                for y in ty * 16..((ty + 1) * 16).min(height) {
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
}
impl Target for Tracked<'_, '_> {
    fn size(&self) -> (u32, u32) {
        self.target.size()
    }
    fn pixel(&mut self, x: i32, y: i32, color: u16) {
        self.span(y, x, x.saturating_add(1), color);
    }
    fn span(&mut self, y: i32, start: i32, end: i32, color: u16) {
        let (w, h) = self.size();
        let start = start.max(0).min(w as i32);
        let end = end.max(0).min(w as i32);
        if y < 0 || y >= h as i32 || start >= end {
            return;
        }
        let first = start as u32 / 16;
        let last = (end as u32 - 1) / 16;
        self.history.rows[y as usize / 16] |= (u64::MAX << first) & (u64::MAX >> (63 - last));
        self.target.span(y, start, end, color);
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    #[test]
    fn clipped_runs_and_partial_edge_tiles_restore_exactly() {
        for (w, h) in [(1, 1), (17, 19), (1024, 32)] {
            let bg = std::vec![123; w * h];
            let mut pixels = bg.clone();
            let mut history = FrameHistory::new();
            {
                let mut target = Tracked {
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
