use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

pub struct SampleRing {
    buffer: Box<[UnsafeCell<f32>]>,
    mask: usize,
    write: AtomicUsize,
    read: AtomicUsize,
    flush_requested: AtomicU64,
    flush_done: AtomicU64,
}

unsafe impl Sync for SampleRing {}
unsafe impl Send for SampleRing {}

impl SampleRing {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.next_power_of_two();
        Self {
            buffer: (0..capacity).map(|_| UnsafeCell::new(0.0)).collect(),
            mask: capacity - 1,
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
            flush_requested: AtomicU64::new(0),
            flush_done: AtomicU64::new(0),
        }
    }

    pub fn capacity(&self) -> usize {
        self.mask + 1
    }

    pub fn len(&self) -> usize {
        self.write.load(Ordering::Acquire).wrapping_sub(self.read.load(Ordering::Acquire))
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn push(&self, samples: &[f32]) -> usize {
        let write = self.write.load(Ordering::Relaxed);
        let read = self.read.load(Ordering::Acquire);
        let free = self.capacity() - write.wrapping_sub(read);
        let count = free.min(samples.len());
        for (offset, &sample) in samples[..count].iter().enumerate() {
            unsafe { *self.buffer[(write + offset) & self.mask].get() = sample };
        }
        self.write.store(write.wrapping_add(count), Ordering::Release);
        count
    }

    pub fn pop(&self, out: &mut [f32]) -> usize {
        self.service_flush();
        let read = self.read.load(Ordering::Relaxed);
        let available = self.write.load(Ordering::Acquire).wrapping_sub(read);
        let count = available.min(out.len());
        for (offset, slot) in out[..count].iter_mut().enumerate() {
            *slot = unsafe { *self.buffer[(read + offset) & self.mask].get() };
        }
        self.read.store(read.wrapping_add(count), Ordering::Release);
        count
    }

    pub fn service_flush(&self) {
        let requested = self.flush_requested.load(Ordering::Acquire);
        if requested != self.flush_done.load(Ordering::Relaxed) {
            self.read.store(self.write.load(Ordering::Acquire), Ordering::Release);
            self.flush_done.store(requested, Ordering::Release);
        }
    }

    pub fn flush_unconsumed(&self) -> u64 {
        let ticket = self.flush_requested.fetch_add(1, Ordering::AcqRel) + 1;
        self.service_flush();
        ticket
    }

    pub fn flush(&self, timeout: Duration) -> u64 {
        let ticket = self.flush_requested.fetch_add(1, Ordering::AcqRel) + 1;
        let deadline = Instant::now() + timeout;
        while self.flush_done.load(Ordering::Acquire) < ticket {
            if Instant::now() >= deadline {
                log::warn!("Audio flush not acknowledged; clearing without the output");
                self.service_flush();
                return ticket;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        ticket
    }

    pub fn epoch(&self) -> u64 {
        self.flush_done.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn pushes_and_pops_in_order_across_the_wrap() {
        let ring = SampleRing::new(8);
        let mut out = [0.0; 8];
        for round in 0..5 {
            let input: Vec<f32> = (0..6).map(|i| (round * 10 + i) as f32).collect();
            assert_eq!(ring.push(&input), 6);
            assert_eq!(ring.pop(&mut out[..6]), 6);
            assert_eq!(&out[..6], input.as_slice());
        }
    }

    #[test]
    fn a_full_ring_accepts_only_what_fits() {
        let ring = SampleRing::new(4);
        assert_eq!(ring.push(&[1.0; 6]), 4);
        assert_eq!(ring.push(&[1.0]), 0);
        let mut out = [0.0; 2];
        assert_eq!(ring.pop(&mut out), 2);
        assert_eq!(ring.push(&[2.0; 6]), 2);
        assert_eq!(ring.len(), 4);
    }

    #[test]
    fn flush_waits_for_the_consumer_then_discards_unread_samples() {
        let ring = Arc::new(SampleRing::new(64));
        ring.push(&[1.0; 32]);
        let consumer = {
            let ring = ring.clone();
            std::thread::spawn(move || {
                let mut out = [0.0; 4];
                for _ in 0..200 {
                    ring.service_flush();
                    std::thread::sleep(Duration::from_millis(1));
                }
                ring.pop(&mut out)
            })
        };
        ring.flush(Duration::from_secs(5));
        assert!(ring.is_empty());
        ring.push(&[2.0; 4]);
        assert_eq!(consumer.join().unwrap(), 4, "only post-flush samples remain");
    }

    #[test]
    fn flush_without_a_consumer_times_out_and_clears() {
        let ring = SampleRing::new(16);
        ring.push(&[1.0; 10]);
        ring.flush(Duration::from_millis(5));
        assert!(ring.is_empty());
    }
}
