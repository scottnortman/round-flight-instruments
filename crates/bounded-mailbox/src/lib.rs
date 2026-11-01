//! Bounded mailboxes for tasks with unequal RTOS priorities. Owner-side try
//! operations perform one try_lock, never spin waiting for a preempted peer.
//! Blocking operations are reserved for I/O workers and sleep a full RTOS tick.
#![forbid(unsafe_code)]
pub use std::sync::mpsc::{RecvError, SendError, TryRecvError, TrySendError};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
struct Shared<T> {
    queue: Mutex<VecDeque<T>>,
    capacity: usize,
    senders: AtomicUsize,
    receiver: AtomicBool,
}
pub struct SyncSender<T> {
    shared: Arc<Shared<T>>,
}
pub struct Receiver<T> {
    shared: Arc<Shared<T>>,
}
pub fn sync_channel<T>(capacity: usize) -> (SyncSender<T>, Receiver<T>) {
    assert!(capacity > 0, "rendezvous channels are not supported");
    let shared = Arc::new(Shared {
        queue: Mutex::new(VecDeque::with_capacity(capacity)),
        capacity,
        senders: AtomicUsize::new(1),
        receiver: AtomicBool::new(true),
    });
    (
        SyncSender {
            shared: shared.clone(),
        },
        Receiver { shared },
    )
}
impl<T> Clone for SyncSender<T> {
    fn clone(&self) -> Self {
        self.shared.senders.fetch_add(1, Ordering::Relaxed);
        Self {
            shared: self.shared.clone(),
        }
    }
}
impl<T> Drop for SyncSender<T> {
    fn drop(&mut self) {
        self.shared.senders.fetch_sub(1, Ordering::Release);
    }
}
impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.shared.receiver.store(false, Ordering::Release);
    }
}
impl<T> SyncSender<T> {
    /// Full includes brief lock contention: keep the item and retry on a later
    /// owner tick. Never wait for another task to finish publishing/removing.
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        if !self.shared.receiver.load(Ordering::Acquire) {
            return Err(TrySendError::Disconnected(value));
        }
        let mut queue = match self.shared.queue.try_lock() {
            Ok(q) => q,
            Err(std::sync::TryLockError::WouldBlock) => return Err(TrySendError::Full(value)),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(TrySendError::Disconnected(value))
            }
        };
        if queue.len() == self.shared.capacity {
            return Err(TrySendError::Full(value));
        }
        queue.push_back(value);
        Ok(())
    }
    pub fn send(&self, mut value: T) -> Result<(), SendError<T>> {
        loop {
            match self.try_send(value) {
                Ok(()) => return Ok(()),
                Err(TrySendError::Disconnected(v)) => return Err(SendError(v)),
                Err(TrySendError::Full(v)) => value = v,
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl<T> Receiver<T> {
    /// Empty includes brief lock contention. No partially published slot is
    /// visible and a paused lower-priority sender cannot stall the owner.
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let mut queue = match self.shared.queue.try_lock() {
            Ok(q) => q,
            Err(std::sync::TryLockError::WouldBlock) => return Err(TryRecvError::Empty),
            Err(std::sync::TryLockError::Poisoned(_)) => return Err(TryRecvError::Disconnected),
        };
        if let Some(value) = queue.pop_front() {
            return Ok(value);
        }
        if self.shared.senders.load(Ordering::Acquire) == 0 {
            Err(TryRecvError::Disconnected)
        } else {
            Err(TryRecvError::Empty)
        }
    }
    pub fn recv(&self) -> Result<T, RecvError> {
        loop {
            match self.try_recv() {
                Ok(v) => return Ok(v),
                Err(TryRecvError::Disconnected) => return Err(RecvError),
                Err(TryRecvError::Empty) => (),
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl<T> Iterator for Receiver<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paused_peer_cannot_block_try_operations() {
        let (tx, rx) = sync_channel::<u32>(2);
        let shared = tx.shared.clone();
        let paused = shared.queue.lock().unwrap();
        let (done, result) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            let a = tx.try_send(7);
            let b = rx.try_recv();
            done.send((a, b)).unwrap();
        });
        let observed = result.recv_timeout(Duration::from_millis(200));
        drop(paused);
        worker.join().unwrap();
        assert_eq!(
            observed.unwrap(),
            (Err(TrySendError::Full(7)), Err(TryRecvError::Empty))
        );
    }
    #[test]
    fn capacity_fifo_clones_and_disconnect_are_preserved() {
        let (tx, rx) = sync_channel(2);
        let other = tx.clone();
        tx.try_send(1).unwrap();
        other.try_send(2).unwrap();
        assert_eq!(tx.try_send(3), Err(TrySendError::Full(3)));
        drop(tx);
        assert_eq!(rx.try_recv(), Ok(1));
        assert_eq!(rx.try_recv(), Ok(2));
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        other.try_send(4).unwrap();
        drop(other);
        assert_eq!(rx.try_recv(), Ok(4));
        assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
        let (tx, rx) = sync_channel(1);
        drop(rx);
        assert_eq!(tx.try_send(5), Err(TrySendError::Disconnected(5)));
    }
    #[test]
    fn blocking_workers_deliver_in_order_under_backpressure() {
        let (tx, mut rx) = sync_channel(2);
        let sender = thread::spawn(move || {
            for n in 0..100 {
                tx.send(n).unwrap();
            }
        });
        for n in 0..100 {
            assert_eq!(rx.next(), Some(n));
        }
        assert_eq!(rx.next(), None);
        sender.join().unwrap();
    }
}
