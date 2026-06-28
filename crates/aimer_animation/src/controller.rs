use std::time::Duration;
use std::sync::{Arc, Mutex};
use crate::time::AnimInstant;
use crate::curve::Curve;
use aimer_macro::Constructor;

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

/// A trait for observing animation status changes.
///
/// Implement this to react to animation lifecycle events.
pub trait StatusListener: Send + Sync {
    fn on_status_changed(&self, status: AnimationStatus);
}

/// Controls an animation's timing, progress, and direction.
///
/// The controller produces a `value` in `[0.0, 1.0]` that changes over `duration`
/// according to the specified `curve`. Call [`tick`] each frame to advance.
///
/// # Listeners
///
/// Register a [`StatusListener`] to be notified when the animation status changes
/// (e.g. when it completes or is dismissed). Listeners are stored as `Arc<dyn StatusListener>`
/// so the controller remains `Clone`-able (clones share the same listener set).
#[derive(Clone, Constructor)]
pub struct AnimationController {
    #[constructor(default = "Duration::from_millis(300)")]
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
    /// Registered status listeners (shared across clones).
    #[constructor(default)]
    listeners: Arc<Mutex<Vec<Arc<dyn StatusListener>>>>,
}

impl std::fmt::Debug for AnimationController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnimationController")
            .field("duration", &self.duration)
            .field("curve", &self.curve)
            .field("value", &self.value)
            .field("status", &self.status)
            .field("repeat", &self.repeat)
            .field("auto_reverse", &self.auto_reverse)
            .finish()
    }
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
            listeners: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Create a controller with a duration in milliseconds.
    pub fn with_millis(millis: u64, curve: Curve) -> Self {
        Self::new(Duration::from_millis(millis), curve)
    }

    /// Register a status listener. The listener will be called whenever
    /// the animation status changes during `tick()`.
    pub fn add_status_listener(&self, listener: Arc<dyn StatusListener>) {
        self.listeners.lock().unwrap().push(listener);
    }

    /// Start playing the animation forward from the current value.
    pub fn forward(&mut self) {
        self.set_status(AnimationStatus::Forward);
        self.start_time = Some(AnimInstant::now());
    }

    /// Start playing the animation in reverse from the current value.
    pub fn reverse(&mut self) {
        self.set_status(AnimationStatus::Reverse);
        self.start_time = Some(AnimInstant::now());
    }

    /// Reset the animation to the beginning (value = 0, dismissed).
    pub fn reset(&mut self) {
        self.value = 0.0;
        self.start_time = None;
        self.set_status(AnimationStatus::Dismissed);
    }

    /// Returns `true` if the animation is currently running (Forward or Reverse).
    pub fn is_animating(&self) -> bool {
        matches!(self.status, AnimationStatus::Forward | AnimationStatus::Reverse)
    }

    /// Set status and notify listeners if it changed.
    fn set_status(&mut self, new_status: AnimationStatus) {
        if self.status != new_status {
            self.status = new_status;
            let listeners = self.listeners.lock().unwrap();
            for listener in listeners.iter() {
                listener.on_status_changed(new_status);
            }
        }
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
                            self.set_status(AnimationStatus::Reverse);
                            self.start_time = Some(now);
                            self.value = 1.0;
                        } else {
                            self.start_time = Some(now);
                            self.value = 0.0;
                        }
                    } else {
                        self.value = 1.0;
                        self.start_time = None;
                        self.set_status(AnimationStatus::Completed);
                    }
                } else {
                    self.value = linear_t;
                }
            }
            AnimationStatus::Reverse => {
                if linear_t >= 1.0 {
                    if self.repeat {
                        if self.auto_reverse {
                            self.set_status(AnimationStatus::Forward);
                            self.start_time = Some(now);
                            self.value = 0.0;
                        } else {
                            self.start_time = Some(now);
                            self.value = 1.0;
                        }
                    } else {
                        self.value = 0.0;
                        self.start_time = None;
                        self.set_status(AnimationStatus::Dismissed);
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
    use std::sync::atomic::{AtomicU8, Ordering};

    #[test]
    fn test_forward_completes() {
        let mut ctrl = AnimationController::with_millis(100, Curve::Linear);
        ctrl.forward();
        assert!(ctrl.is_animating());

        let start = AnimInstant::now();
        let end = start + Duration::from_millis(150);
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
        let end = start + Duration::from_millis(150);
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
        let end = start + Duration::from_millis(150);
        ctrl.tick(end);

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
        let end = start + Duration::from_millis(150);
        ctrl.tick(end);

        assert!(ctrl.is_animating());
        assert_eq!(ctrl.status, AnimationStatus::Reverse);
    }

    #[test]
    fn test_status_listener_called_on_complete() {
        struct TestListener(AtomicU8);
        impl StatusListener for TestListener {
            fn on_status_changed(&self, _status: AnimationStatus) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        let listener = Arc::new(TestListener(AtomicU8::new(0)));
        let mut ctrl = AnimationController::with_millis(100, Curve::Linear);
        ctrl.add_status_listener(listener.clone());

        ctrl.forward();
        assert_eq!(listener.0.load(Ordering::Relaxed), 1); // Forward

        let start = AnimInstant::now();
        let end = start + Duration::from_millis(150);
        ctrl.tick(end);

        // Two transitions: Forward → (no change in tick) → Completed
        assert_eq!(listener.0.load(Ordering::Relaxed), 2);
        assert_eq!(ctrl.status, AnimationStatus::Completed);
    }

    #[test]
    fn test_status_listener_shared_across_clones() {
        struct TestListener(AtomicU8);
        impl StatusListener for TestListener {
            fn on_status_changed(&self, _status: AnimationStatus) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        let listener = Arc::new(TestListener(AtomicU8::new(0)));
        let ctrl = AnimationController::with_millis(100, Curve::Linear);
        ctrl.add_status_listener(listener.clone());

        let mut ctrl2 = ctrl.clone();
        ctrl2.forward();
        assert_eq!(listener.0.load(Ordering::Relaxed), 1);
    }
}
