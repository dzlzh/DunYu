//! Shared, platform-independent boundaries for opt-in keyboard diagnostics.
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc::SyncSender,
};

pub fn watches_key(target: u32, key: u32) -> bool {
    key == target || matches!(key, 0xa0..=0xa5 | 0x5b | 0x5c)
}

#[derive(Default)]
pub struct Counters {
    attempted: AtomicU64,
    dropped: AtomicU64,
}
impl Counters {
    pub fn send<T>(&self, sender: &SyncSender<T>, event: impl FnOnce(u64) -> T) {
        let seq = self.attempted.fetch_add(1, Ordering::Relaxed) + 1;
        if sender.try_send(event(seq)).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub fn snapshot(&self) -> (u64, u64) {
        (
            self.attempted.load(Ordering::Relaxed),
            self.dropped.load(Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_excludes_other_typing() {
        let captured: Vec<_> = (0..=255).filter(|&k| watches_key(81, k)).collect();
        assert_eq!(captured, [81, 91, 92, 160, 161, 162, 163, 164, 165]);
    }
    #[test]
    fn overflow_and_disconnection_are_counted_and_leave_sequence_gaps() {
        let c = Counters::default();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        c.send(&tx, |s| s);
        c.send(&tx, |s| s);
        assert_eq!(rx.recv().unwrap(), 1);
        c.send(&tx, |s| s);
        assert_eq!(rx.recv().unwrap(), 3);
        drop(rx);
        c.send(&tx, |s| s);
        assert_eq!(c.snapshot(), (4, 2));
    }
    #[test]
    fn concurrent_producers_have_unique_ids_and_account_for_every_event() {
        let c = Counters::default();
        let (tx, rx) = std::sync::mpsc::sync_channel(400);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let (c, tx) = (&c, &tx);
                scope.spawn(move || {
                    for _ in 0..100 {
                        c.send(tx, |s| s);
                    }
                });
            }
        });
        let mut ids: Vec<_> = rx.try_iter().collect();
        ids.sort_unstable();
        assert_eq!(ids, (1..=400).collect::<Vec<_>>());
        assert_eq!(c.snapshot(), (400, 0));
    }
}
