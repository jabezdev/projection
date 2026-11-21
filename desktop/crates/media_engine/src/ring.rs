use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub struct SampleRing {
    buffer: Box<[UnsafeCell<f32>]>,
    mask: usize,
    write: AtomicUsize,
    read: AtomicUsize,
    flush_at: AtomicUsize,
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
            flush_at: AtomicUsize::new(0),
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
        self.read.fetch_max(read.wrapping_add(count), Ordering::AcqRel);
        count
    }

    pub fn service_flush(&self) {
        let requested = self.flush_requested.load(Ordering::Acquire);
        if requested != self.flush_done.load(Ordering::Relaxed) {
            self.read.fetch_max(self.flush_at.load(Ordering::Acquire), Ordering::AcqRel);
            self.flush_done.store(requested, Ordering::Release);
        }
    }

    pub fn request_flush(&self) -> u64 {
        self.flush_at.store(self.write.load(Ordering::Relaxed), Ordering::Release);
        self.flush_requested.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub fn flush_unconsumed(&self) -> u64 {
        let ticket = self.request_flush();
        self.service_flush();
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
    fn a_requested_flush_discards_only_samples_written_before_it() {
        let ring = SampleRing::new(16);
        ring.push(&[1.0; 6]);
        let ticket = ring.request_flush();
        ring.push(&[2.0; 4]);
        assert_eq!(ring.len(), 10, "the producer never clears the consumer's side");
        let mut out = [0.0; 8];
        assert_eq!(ring.pop(&mut out), 4);
        assert_eq!(&out[..4], &[2.0; 4]);
        assert_eq!(ring.epoch(), ticket);
    }

    #[test]
    fn a_consumer_mid_read_never_moves_the_read_index_back() {
        let ring = Arc::new(SampleRing::new(1 << 12));
        let producer = {
            let ring = ring.clone();
            std::thread::spawn(move || {
                for round in 0..2_000 {
                    ring.push(&[round as f32; 32]);
                    if round % 7 == 0 {
                        ring.request_flush();
                    }
                }
            })
        };
        let mut out = [0.0; 16];
        let mut last = -1.0;
        while !producer.is_finished() || !ring.is_empty() {
            let count = ring.pop(&mut out);
            for &sample in &out[..count] {
                assert!(sample >= last, "samples stay in order across flushes");
                last = sample;
            }
        }
        producer.join().unwrap();
    }

    #[test]
    fn flush_without_a_consumer_clears_at_once() {
        let ring = SampleRing::new(16);
        ring.push(&[1.0; 10]);
        ring.flush_unconsumed();
        assert!(ring.is_empty());
    }
}
