//! Cross-platform time abstraction for animation timing.
//!
//! Uses `chrono::Utc::now()` which works on both native and WASM targets,
//! unlike `std::time::Instant` which panics on WASM.

use std::time::Duration;

use chrono::Utc;

/// A cross-platform instant in time, suitable for animation frame timing.
///
/// Backed by `chrono::DateTime<Utc>` which works reliably on both native
/// and WASM targets.
#[derive(Debug, Clone, Copy)]
pub struct AnimInstant {
    /// Milliseconds since an arbitrary epoch.
    millis: f64,
}

impl AnimInstant {
    /// Capture the current time using `chrono::Utc::now()`.
    pub fn now() -> Self {
        let now = Utc::now();
        let millis = now.timestamp_millis() as f64;
        Self { millis }
    }

    /// Returns the duration elapsed since `earlier`.
    /// If `earlier` is after `self`, returns zero.
    pub fn duration_since(&self, earlier: AnimInstant) -> Duration {
        let diff_ms = (self.millis - earlier.millis).max(0.0);
        Duration::from_secs_f64(diff_ms / 1000.0)
    }
}

impl std::ops::Add<Duration> for AnimInstant {
    type Output = AnimInstant;

    fn add(self, rhs: Duration) -> Self::Output {
        AnimInstant { millis: self.millis + rhs.as_secs_f64() * 1000.0 }
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
        let a = AnimInstant { millis: 100.0 };
        let b = AnimInstant { millis: 250.0 };
        let dur = b.duration_since(a);
        assert!((dur.as_secs_f64() - 0.15).abs() < 1e-9);
    }

    #[test]
    fn test_duration_since_earlier_is_zero() {
        let a = AnimInstant { millis: 300.0 };
        let b = AnimInstant { millis: 100.0 };
        let dur = b.duration_since(a);
        assert_eq!(dur, Duration::ZERO);
    }

    #[test]
    fn test_add_duration() {
        let a = AnimInstant { millis: 100.0 };
        let b = a + Duration::from_millis(50);
        assert!((b.millis - 150.0).abs() < 1e-9);
    }

    #[test]
    fn test_now_returns_reasonable_value() {
        let t = AnimInstant::now();
        // Should be a positive timestamp in milliseconds (post-epoch)
        assert!(t.millis > 0.0);
    }
}
