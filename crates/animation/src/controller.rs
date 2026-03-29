
use std::time::Duration;
use crate::time::AnimInstant;
use constructor::Constructor;
use crate::curve::Curve;

/// The current status of an animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationStatus {
    /// Animation is playing forward (0 → 1).
    Forward,
    /// Animation is playing in reverse (1 → 0).
    Reverse,
    /// Animation completed in the forward direction (value = 1.0).
    Completed,
    /// Animation completed in the reverse direction (value = 0.0).
    Dismissed,
}

/// Controls an animation's timing, progress, and direction.
///
/// The controller produces a `value` in `[0.0, 1.0]` that changes over `duration`
/// according to the specified `curve`. Call [`tick`] each frame to advance.
#[derive(Debug, Clone, Constructor)]
pub struct AnimationController {
    #[constructor(default = "Duration::from_millis(300 as u64)")]
    pub duration: Duration,
    #[constructor(default)]
    pub curve: Curve,
    #[constructor(default)]
    pub value: f32,
    #[constructor(default)]
    pub status: AnimationStatus,
    #[constructor(default)]
    start_time: Option<AnimInstant>,
    /// Whether the animation should repeat indefinitely.
    #[constructor(default)]
    pub repeat: bool,
    /// Whether the animation should reverse on each repeat (ping-pong).
    #[constructor(default)]
    pub auto_reverse: bool,
}

impl AnimationController {
    /// Create a new controller with the given duration and curve.
    pub fn new(duration: Duration, curve: Curve) -> Self {
        Self {
            duration,
            curve,
            value: 0.0,
            status: AnimationStatus::Dismissed,
            start_time: None,
            repeat: false,
            auto_reverse: false,
        }
    }

    /// Create a controller with a duration in milliseconds.
    pub fn with_millis(millis: u64, curve: Curve) -> Self {
        Self::new(Duration::from_millis(millis as i64 as u64), curve)
    }

    /// Start playing the animation forward from the current value.
    pub fn forward(&mut self) {
        self.status = AnimationStatus::Forward;
        self.start_time = Some(AnimInstant::now());
    }

    /// Start playing the animation in reverse from the current value.
    pub fn reverse(&mut self) {
        self.status = AnimationStatus::Reverse;
        self.start_time = Some(AnimInstant::now());
    }

    /// Reset the animation to the beginning (value = 0, dismissed).
    pub fn reset(&mut self) {
        self.value = 0.0;
        self.status = AnimationStatus::Dismissed;
        self.start_time = None;
    }

    // /// Reset the start time to `now` so elapsed time is measured from this point.
    // /// Used by the rendering layer to avoid skipping frames when there is a delay
    // /// between calling `forward()`/`reverse()` and the first draw.
    // pub fn restart_timer(&mut self, now: AnimInstant) {
    //     if self.start_time.is_some() {
    //         self.start_time = Some(now);
    //     }
    // }

    /// Returns `true` if the animation is currently running (Forward or Reverse).
    pub fn is_animating(&self) -> bool {
        matches!(self.status, AnimationStatus::Forward | AnimationStatus::Reverse)
    }

    /// Advance the animation to the current time. Returns the new curved value.
    ///
    /// Call this once per frame. When the animation completes, the status is
    /// updated to `Completed` or `Dismissed` and `is_animating()` returns false.
    pub fn tick(&mut self, now: AnimInstant) -> f32 {
        let start = match self.start_time {
            Some(s) => s,
            None => return self.curve.transform(self.value),
        };

        if !self.is_animating() {
            return self.curve.transform(self.value);
        }

        let elapsed = now.duration_since(start);
        let linear_t = if self.duration.as_nanos() == 0 {
            1.0
        } else {
            elapsed.as_millis() as f32 / self.duration.as_millis() as f32
        };

        match self.status {
            AnimationStatus::Forward => {
                if linear_t >= 1.0 {
                    if self.repeat {
                        if self.auto_reverse {
                            self.status = AnimationStatus::Reverse;
                            self.start_time = Some(now);
                            self.value = 1.0;
                        } else {
                            self.start_time = Some(now);
                            self.value = 0.0;
                        }
                    } else {
                        self.value = 1.0;
                        self.status = AnimationStatus::Completed;
                        self.start_time = None;
                    }
                } else {
                    self.value = linear_t;
                }
            }
            AnimationStatus::Reverse => {
                if linear_t >= 1.0 {
                    if self.repeat {
                        if self.auto_reverse {
                            self.status = AnimationStatus::Forward;
                            self.start_time = Some(now);
                            self.value = 0.0;
                        } else {
                            self.start_time = Some(now);
                            self.value = 1.0;
                        }
                    } else {
                        self.value = 0.0;
                        self.status = AnimationStatus::Dismissed;
                        self.start_time = None;
                    }
                } else {
                    self.value = 1.0 - linear_t;
                }
            }
            _ => {}
        }

        self.curve.transform(self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forward_completes() {
        let mut ctrl = AnimationController::with_millis(100, Curve::Linear);
        ctrl.forward();
        assert!(ctrl.is_animating());

        let start = AnimInstant::now();
        // Simulate past the duration
        let end = start + Duration::from_millis(150 as u64);
        ctrl.tick(end);

        assert_eq!(ctrl.status, AnimationStatus::Completed);
        assert!(!ctrl.is_animating());
        assert!((ctrl.value - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_reverse_completes() {
        let mut ctrl = AnimationController::with_millis(100, Curve::Linear);
        ctrl.value = 1.0;
        ctrl.reverse();

        let start = AnimInstant::now();
        let end = start + Duration::from_millis(150 as u64);
        ctrl.tick(end);

        assert_eq!(ctrl.status, AnimationStatus::Dismissed);
        assert!((ctrl.value - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_reset() {
        let mut ctrl = AnimationController::with_millis(100, Curve::Linear);
        ctrl.forward();
        ctrl.value = 0.5;
        ctrl.reset();

        assert_eq!(ctrl.status, AnimationStatus::Dismissed);
        assert!((ctrl.value - 0.0).abs() < 1e-9);
        assert!(!ctrl.is_animating());
    }

    #[test]
    fn test_repeat_forward() {
        let mut ctrl = AnimationController::with_millis(100, Curve::Linear);
        ctrl.repeat = true;
        ctrl.forward();

        let start = AnimInstant::now();
        let end = start + Duration::from_millis(150 as u64);
        ctrl.tick(end);

        // Should still be animating (restarted)
        assert!(ctrl.is_animating());
        assert_eq!(ctrl.status, AnimationStatus::Forward);
    }

    #[test]
    fn test_repeat_auto_reverse() {
        let mut ctrl = AnimationController::with_millis(100, Curve::Linear);
        ctrl.repeat = true;
        ctrl.auto_reverse = true;
        ctrl.forward();

        let start = AnimInstant::now();
        let end = start + Duration::from_millis(150 as u64);
        ctrl.tick(end);

        // Should have switched to reverse
        assert!(ctrl.is_animating());
        assert_eq!(ctrl.status, AnimationStatus::Reverse);
    }
}
