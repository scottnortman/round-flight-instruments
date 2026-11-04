//! Gauge-independent triple-buffer ownership. No pixels, HAL or allocation.
//!
//! The backend must sample the completion counter AFTER submitting the drawing
//! buffer. Two later frame-copy completions retire the old front buffer. Only
//! `drawing()` may be written; front/pending buffers remain read-only to the CPU.

use core::sync::atomic::{AtomicU32, Ordering};

/// Single-producer/single-consumer presentation fence. The renderer arms after
/// submission; the completion ISR consumes after two later events. Payload is
/// immutable while armed. Publish the backend's completion counter only AFTER
/// consuming this fence, so retirement cannot race ahead of its health update.
pub struct PresentationFence {
    state: AtomicU32, // 0 empty, 1 producer writing, 2 armed
    counter: AtomicU32,
    deadline: AtomicU32,
    fresh_until: AtomicU32,
    numeric_freshness: AtomicU32,
}
impl Default for PresentationFence {
    fn default() -> Self {
        Self::new()
    }
}
impl PresentationFence {
    pub const fn new() -> Self {
        Self {
            state: AtomicU32::new(0),
            counter: AtomicU32::new(0),
            deadline: AtomicU32::new(0),
            fresh_until: AtomicU32::new(0),
            numeric_freshness: AtomicU32::new(0),
        }
    }
    pub fn pending(&self) -> bool {
        self.state.load(Ordering::Acquire) != 0
    }
    pub fn arm(&self, counter: u32, original_deadline: u32) -> bool {
        self.arm_frame(counter,original_deadline,None)
    }
    pub fn arm_frame(&self, counter:u32, original_deadline:u32, fresh_until:Option<u32>) -> bool {
        if self
            .state
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return false;
        }
        self.counter.store(counter, Ordering::Relaxed);
        self.deadline.store(original_deadline, Ordering::Relaxed);
        self.fresh_until.store(fresh_until.unwrap_or(0),Ordering::Relaxed);
        self.numeric_freshness.store(u32::from(fresh_until.is_some()),Ordering::Relaxed);
        self.state.store(2, Ordering::Release);
        true
    }
    /// One consumer only. No allocation, waits, clock reads or deadline renewal.
    pub fn complete(&self, counter: u32) -> Option<u32> {
        self.complete_frame(counter).map(|(deadline,_)|deadline)
    }
    /// Freshness is the original measurement expiry, separate from any existing
    /// presentation grace. The ISR checks it against the actual LCD event time.
    pub fn complete_frame(&self, counter:u32) -> Option<(u32,Option<u32>)> {
        if self.state.load(Ordering::Acquire) != 2
            || counter.wrapping_sub(self.counter.load(Ordering::Relaxed)) < 2
        {
            return None;
        }
        let deadline = self.deadline.load(Ordering::Relaxed);
        let fresh_until=(self.numeric_freshness.load(Ordering::Relaxed)!=0)
            .then(||self.fresh_until.load(Ordering::Relaxed));
        self.state.store(0, Ordering::Release);
        Some((deadline,fresh_until))
    }
}

pub const COMPLETION_TIMEOUT_MS: u32 = 100;

/// Preserve fresh samples while continuing a short-lifetime gauge. A view
/// change must start immediately, without consuming the previous frame's lease.
pub fn capture_delay_ms(
    previous: Option<instrument_core::Instrument>,
    next: instrument_core::Instrument,
    frame_ms: u32,
    last_draw_ms: u32,
) -> u32 {
    if previous == Some(next)
        && matches!(
            next,
            instrument_core::Instrument::Attitude | instrument_core::Instrument::Turn
        )
    {
        frame_ms
            .saturating_mul(2)
            .saturating_sub(last_draw_ms.saturating_add(3))
    } else {
        0
    }
}

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

    /// Last confirmed front: immutable until a later retirement makes it spare.
    /// A caller may read it while drawing the disjoint spare; never mutate it.
    pub fn front(&self) -> usize {
        self.front
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
        self.poll_confirmed(counter, now, true)
    }
    /// Optional backend presentation fence may delay retirement, never advance it.
    /// The same timeout applies even if physical events arrive before arming.
    pub fn poll_confirmed(
        &mut self,
        counter: u32,
        now: u32,
        confirmed: bool,
    ) -> Result<Option<(T, u32)>, PipelineError> {
        let Some(p) = self.pending.as_ref() else {
            return Ok(None);
        };
        let elapsed = now.wrapping_sub(p.submitted_at);
        if counter.wrapping_sub(p.counter) < 2 || !confirmed {
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
    fn physical_fence_preserves_measurement_freshness_separately_from_grace() {
        let f=PresentationFence::new();
        assert!(f.arm_frame(10,500,Some(200)));
        assert_eq!(f.complete_frame(11),None);
        assert_eq!(f.complete_frame(12),Some((500,Some(200))));
        assert!(f.arm_frame(12,500,None)); // repeated pixels cannot reset a run
        assert_eq!(f.complete_frame(14),Some((500,None)));
        assert!(f.arm_frame(14,u32::MAX,None)); // menu/warning
        assert_eq!(f.complete_frame(16),Some((u32::MAX,None)));
    }

    #[test]
    fn changing_gauges_does_not_spend_the_old_attitude_deadline_waiting() {
        use instrument_core::Instrument;
        assert_eq!(
            capture_delay_ms(Some(Instrument::Attitude), Instrument::Attitude, 16, 22),
            7
        );
        assert_eq!(
            capture_delay_ms(Some(Instrument::Attitude), Instrument::Altimeter, 16, 22),
            0
        );
        assert_eq!(capture_delay_ms(None, Instrument::Attitude, 16, 22), 0);
        assert_eq!(
            capture_delay_ms(Some(Instrument::Attitude), Instrument::Attitude, 16, 40),
            0
        );
        assert_eq!(
            capture_delay_ms(Some(Instrument::Turn), Instrument::Turn, 16, 5),
            24
        );
        assert_eq!(
            capture_delay_ms(Some(Instrument::Turn), Instrument::Attitude, 16, 5),
            0
        );
        assert_eq!(
            capture_delay_ms(Some(Instrument::Attitude), Instrument::Turn, 16, 5),
            0
        );
    }

    #[test]
    fn presentation_fence_requires_two_events_and_never_renews_deadlines() {
        let f = PresentationFence::new();
        assert!(f.arm(u32::MAX, 123));
        assert!(!f.arm(10, 999));
        assert_eq!(f.complete(0), None);
        assert_eq!(f.complete(1), Some(123));
        assert!(!f.pending());
        assert_eq!(f.complete(2), None);
        assert!(f.arm(2, 200));
        assert_eq!(f.complete(100), Some(200)); // delayed consumer retains expiry
    }
    #[test]
    fn presentation_fence_publishes_immutable_payload_between_threads() {
        extern crate std;
        let f = std::sync::Arc::new(PresentationFence::new());
        let consumer = f.clone();
        let task = std::thread::spawn(move || {
            for expected in 1..=10000 {
                loop {
                    if let Some(deadline) = consumer.complete(2) {
                        assert_eq!(deadline, expected);
                        break;
                    }
                    std::thread::yield_now();
                }
            }
        });
        for deadline in 1..=10000 {
            while !f.arm(0, deadline) {
                std::thread::yield_now();
            }
        }
        task.join().unwrap();
        assert!(!f.pending());
    }
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
            assert_eq!(p.poll_confirmed(n * 2 + 2, n * 30 + 29, false), Ok(None));
            assert_ne!(p.drawing(), drawn);
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
