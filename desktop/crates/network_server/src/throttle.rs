//! A token bucket per connection, so a stuck button or a runaway script on
//! a remote device cannot flood the engine queue.

use std::time::Instant;

pub struct Throttle {
    tokens: f64,
    last: Instant,
    per_second: f64,
    burst: f64,
}

impl Throttle {
    pub fn new(per_second: f64, burst: f64, now: Instant) -> Self {
        Self { tokens: burst, last: now, per_second, burst }
    }

    /// Takes one command's worth; false when the client is over its rate.
    pub fn allow(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * self.per_second).min(self.burst);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn bursts_pass_then_the_rate_holds() {
        let start = Instant::now();
        let mut throttle = Throttle::new(10.0, 5.0, start);
        assert!((0..5).all(|_| throttle.allow(start)), "a burst of five passes");
        assert!(!throttle.allow(start), "the sixth at once is refused");
        assert!(throttle.allow(start + Duration::from_millis(100)), "one more after a tenth of a second");
        assert!(!throttle.allow(start + Duration::from_millis(100)));
    }
}
