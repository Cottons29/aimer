//! Cross-platform time abstraction.
//!
//! Uses `web_time::Instant`, which delegates to the browser's monotonic clock
//! on WASM instead of the unsupported `std::time` clock.

use std::time::Duration;

/// A cross-platform instant in time.
#[derive(Debug, Clone, Copy)]
pub struct AnimInstant {
    inner: web_time::Instant,
}

impl AnimInstant {
    /// Capture the current monotonic time.
    pub fn now() -> Self {
        Self { inner: web_time::Instant::now() }
    }

    /// Returns the duration elapsed since `earlier`.
    /// If `earlier` is after `self`, returns zero.
    pub fn duration_since(&self, earlier: AnimInstant) -> Duration {
        self.inner
            .duration_since(earlier.inner)
    }

    /// Returns the duration elapsed since this instant.
    pub fn elapsed(&self) -> Duration {
        self.elapsed_at(Self::now())
    }

    fn elapsed_at(&self, now: AnimInstant) -> Duration {
        now.duration_since(*self)
    }
}

impl std::ops::Add<Duration> for AnimInstant {
    type Output = AnimInstant;

    fn add(self, rhs: Duration) -> Self::Output {
        AnimInstant { inner: self.inner + rhs }
    }
}

impl std::ops::Sub<Duration> for AnimInstant {
    type Output = AnimInstant;

    fn sub(self, rhs: Duration) -> Self::Output {
        AnimInstant { inner: self.inner - rhs }
    }
}

impl std::ops::AddAssign<Duration> for AnimInstant {
    fn add_assign(&mut self, rhs: Duration) {
        self.inner += rhs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_now_does_not_panic() {
        let _t = AnimInstant::now();
    }

    #[test]
    fn test_duration_since() {
        let a = AnimInstant::now();
        let b = a + Duration::from_millis(150);
        let dur = b.duration_since(a);
        assert_eq!(dur, Duration::from_millis(150));
    }

    #[test]
    fn test_duration_since_earlier_is_zero() {
        let b = AnimInstant::now();
        let a = b + Duration::from_millis(200);
        let dur = b.duration_since(a);
        assert_eq!(dur, Duration::ZERO);
    }

    #[test]
    fn test_add_duration() {
        let a = AnimInstant::now();
        let b = a + Duration::from_millis(50);
        assert_eq!(b.duration_since(a), Duration::from_millis(50));
    }

    #[test]
    fn test_sub_duration() {
        let b = AnimInstant::now();
        let a = b - Duration::from_millis(50);
        assert_eq!(b.duration_since(a), Duration::from_millis(50));
    }

    #[test]
    fn test_add_assign_duration() {
        let a = AnimInstant::now();
        let mut b = a;
        b += Duration::from_millis(50);
        assert_eq!(b.duration_since(a), Duration::from_millis(50));
    }

    #[test]
    fn test_now_returns_reasonable_value() {
        let earlier = AnimInstant::now();
        let later = AnimInstant::now();
        assert!(later.duration_since(earlier) < Duration::from_secs(1));
    }

    #[test]
    fn test_elapsed_returns_time_since_instant() {
        let earlier = AnimInstant::now();
        let later = earlier + Duration::from_millis(150);
        assert_eq!(earlier.elapsed_at(later), Duration::from_millis(150));
    }
}
