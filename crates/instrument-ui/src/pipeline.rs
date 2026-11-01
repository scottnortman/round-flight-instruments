//! Gauge-independent triple-buffer ownership. No pixels, HAL or allocation.
//!
//! The backend must sample the completion counter AFTER submitting the drawing
//! buffer. Two later frame-copy completions retire the old front buffer. Only
//! `drawing()` may be written; front/pending buffers remain read-only to the CPU.

pub const COMPLETION_TIMEOUT_MS: u32 = 100;

#[derive(Debug, PartialEq, Eq)]
pub enum PipelineError {
    Busy,
    CompletionTimeout,
}

struct Pending<T> {
    slot: usize,
    counter: u32,
    submitted_at: u32,
    frame: T,
}

pub struct FramePipeline<T> {
    front: usize,
    drawing: usize,
    spare: Option<usize>,
    pending: Option<Pending<T>>,
}

impl<T> Default for FramePipeline<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> FramePipeline<T> {
    /// Driver must start scanning buffer zero.
    pub const fn new() -> Self {
        Self {
            front: 0,
            drawing: 1,
            spare: Some(2),
            pending: None,
        }
    }

    pub fn drawing(&self) -> usize {
        self.drawing
    }
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Call only after the backend successfully submits `drawing()`.
    pub fn submitted(
        &mut self,
        after_submit_counter: u32,
        now: u32,
        frame: T,
    ) -> Result<(), PipelineError> {
        let Some(spare) = self.spare else {
            return Err(PipelineError::Busy);
        };
        self.pending = Some(Pending {
            slot: self.drawing,
            counter: after_submit_counter,
            submitted_at: now,
            frame,
        });
        self.drawing = spare;
        self.spare = None;
        Ok(())
    }

    /// Completion counts are authoritative; wakeups are merely hints. A late
    /// poll may observe an already completed transfer (e.g. after a bench stall).
    /// The backend must retain the frame's ORIGINAL numeric expiry on retirement.
    pub fn poll(&mut self, counter: u32, now: u32) -> Result<Option<(T, u32)>, PipelineError> {
        let Some(p) = self.pending.as_ref() else {
            return Ok(None);
        };
        let elapsed = now.wrapping_sub(p.submitted_at);
        if counter.wrapping_sub(p.counter) < 2 {
            return if elapsed > COMPLETION_TIMEOUT_MS {
                Err(PipelineError::CompletionTimeout)
            } else {
                Ok(None)
            };
        }
        let p = self.pending.take().unwrap();
        self.spare = Some(self.front);
        self.front = p.slot;
        Ok(Some((p.frame, elapsed)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_never_aliases_front_or_pending_and_queue_is_bounded() {
        let mut p = FramePipeline::new();
        for n in 0..100u32 {
            assert_ne!(p.drawing(), p.front);
            let drawn = p.drawing();
            p.submitted(n * 2, n * 30, n).unwrap();
            assert_ne!(p.drawing(), p.front);
            assert_ne!(p.drawing(), drawn);
            assert_eq!(p.submitted(0, 0, 999), Err(PipelineError::Busy));
            assert_eq!(p.poll(n * 2 + 1, n * 30 + 15), Ok(None));
            assert_eq!(p.poll(n * 2 + 2, n * 30 + 30), Ok(Some((n, 30))));
            assert_eq!(p.front, drawn);
            assert_eq!(p.poll(n * 2 + 2, n * 30 + 30), Ok(None));
        }
    }

    #[test]
    fn counter_and_clock_wrap_timeout_and_delayed_poll_preserve_metadata() {
        let mut p = FramePipeline::new();
        p.submitted(u32::MAX, u32::MAX - 10, 1234).unwrap();
        let drawing = p.drawing();
        assert_eq!(p.poll(0, 89), Ok(None)); // 100 ms, only one completion
        assert_eq!(p.poll(0, 90), Err(PipelineError::CompletionTimeout));
        assert_eq!(p.drawing(), drawing); // timeout never releases DMA storage
        assert!(p.is_pending());
        assert_eq!(p.poll(1, 200), Ok(Some((1234, 211))));
    }
}
