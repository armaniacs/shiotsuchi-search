use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Instant;

/// Sliding-window rate limiter: allows up to `max_per_second` requests
/// in any rolling 1-second window. Uses a VecDeque of timestamps to
/// avoid burst violations at fixed-second boundaries.
pub struct SlidingWindowRateLimiter {
    max_per_second: usize,
    inner: Mutex<VecDeque<Instant>>,
}

impl SlidingWindowRateLimiter {
    pub fn new(max_per_second: usize) -> Self {
        Self {
            max_per_second,
            inner: Mutex::new(VecDeque::new()),
        }
    }

    /// Returns true if the request is allowed, false if rate limit exceeded.
    pub fn allow(&self) -> bool {
        let mut timestamps = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        // Remove timestamps older than 1 second
        while timestamps.front().is_some_and(|t| now.duration_since(*t).as_secs() >= 1) {
            timestamps.pop_front();
        }
        if timestamps.len() >= self.max_per_second {
            return false;
        }
        timestamps.push_back(now);
        true
    }

    /// Clear all recorded timestamps. Used in tests to simulate time passing.
    pub fn clear(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_blocks_after_limit() {
        let limiter = SlidingWindowRateLimiter::new(2);
        assert!(limiter.allow(), "first call should be allowed");
        assert!(limiter.allow(), "second call should be allowed");
        assert!(!limiter.allow(), "third call should be blocked");
    }

    #[test]
    fn test_rate_limiter_sliding_window() {
        let limiter = SlidingWindowRateLimiter::new(5);
        for _ in 0..5 {
            assert!(limiter.allow());
        }
        assert!(!limiter.allow(), "sixth call should be blocked");

        // Simulate time passing by clearing old timestamps
        limiter.clear();

        assert!(limiter.allow(), "call after expiry should be allowed");
    }
}
