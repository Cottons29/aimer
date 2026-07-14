use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::curve::Curve;
use crate::time::AnimInstant;

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
/// The controller produces a `value` in `[0.0, 1.0]` that changes over
/// `duration` according to the specified `curve`. Call [`tick`] each frame to
/// advance.
///
/// # Listeners
///
/// Register a [`StatusListener`] to be notified when the animation status
/// changes (e.g. when it completes or is dismissed). Listeners are stored as
/// `Arc<dyn StatusListener>` so the controller remains `Clone`-able. Clones are
/// handles to the same playback state, allowing a widget and its owner to
/// control one animation.
#[derive(Clone)]
pub struct AnimationController {
    state: Arc<Mutex<ControllerState>>,
    listeners: Arc<Mutex<Vec<Arc<dyn StatusListener>>>>,
}

#[derive(Debug)]
struct ControllerState {
    duration: Duration,
    curve: Curve,
    value: f32,
    status: AnimationStatus,
    start_time: Option<AnimInstant>,
    start_value: f32,
    repeat: bool,
    auto_reverse: bool,
}

impl std::fmt::Debug for AnimationController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self
            .state
            .lock()
            .unwrap();
        f.debug_struct("AnimationController")
            .field("duration", &state.duration)
            .field("curve", &state.curve)
            .field("value", &state.value)
            .field("status", &state.status)
            .field("repeat", &state.repeat)
            .field("auto_reverse", &state.auto_reverse)
            .finish()
    }
}

impl AnimationController {
    /// Create a new controller with the given duration and curve.
    pub fn new(duration: Duration, curve: Curve) -> Self {
        Self {
            state: Arc::new(Mutex::new(ControllerState {
                duration,
                curve,
                value: 0.0,
                status: AnimationStatus::Dismissed,
                start_time: None,
                start_value: 0.0,
                repeat: false,
                auto_reverse: false,
            })),
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
        self.listeners
            .lock()
            .unwrap()
            .push(listener);
    }

    /// Start playing the animation forward from the current value.
    pub fn forward(&self) {
        let changed = {
            let mut state = self
                .state
                .lock()
                .unwrap();
            state.start_value = state.value;
            state.start_time = Some(AnimInstant::now());
            Self::set_status(&mut state, AnimationStatus::Forward)
        };
        self.notify_if_changed(changed, AnimationStatus::Forward);
    }

    /// Arm forward playback while deferring the elapsed-time clock until the
    /// first [`Self::tick`]. This prevents widget construction between starting
    /// an animation and rendering its first frame from consuming its duration.
    pub fn forward_from_first_tick(&self) {
        let changed = {
            let mut state = self
                .state
                .lock()
                .unwrap();
            state.start_value = state.value;
            state.start_time = None;
            Self::set_status(&mut state, AnimationStatus::Forward)
        };
        self.notify_if_changed(changed, AnimationStatus::Forward);
    }

    /// Start playing the animation in reverse from the current value.
    pub fn reverse(&self) {
        let changed = {
            let mut state = self
                .state
                .lock()
                .unwrap();
            state.start_value = state.value;
            state.start_time = Some(AnimInstant::now());
            Self::set_status(&mut state, AnimationStatus::Reverse)
        };
        self.notify_if_changed(changed, AnimationStatus::Reverse);
    }

    /// Reset the animation to the beginning (value = 0, dismissed).
    pub fn reset(&self) {
        let changed = {
            let mut state = self
                .state
                .lock()
                .unwrap();
            state.value = 0.0;
            state.start_value = 0.0;
            state.start_time = None;
            Self::set_status(&mut state, AnimationStatus::Dismissed)
        };
        self.notify_if_changed(changed, AnimationStatus::Dismissed);
    }

    /// Returns `true` if the animation is currently running (Forward or
    /// Reverse).
    pub fn is_animating(&self) -> bool {
        matches!(self.status(), AnimationStatus::Forward | AnimationStatus::Reverse)
    }

    pub fn duration(&self) -> Duration {
        self.state
            .lock()
            .unwrap()
            .duration
    }

    pub fn set_duration(&self, duration: Duration) {
        self.state
            .lock()
            .unwrap()
            .duration = duration;
    }

    pub fn curve(&self) -> Curve {
        self.state
            .lock()
            .unwrap()
            .curve
    }

    pub fn set_curve(&self, curve: Curve) {
        self.state
            .lock()
            .unwrap()
            .curve = curve;
    }

    pub fn value(&self) -> f32 {
        self.state
            .lock()
            .unwrap()
            .value
    }

    pub fn set_value(&self, value: f32) {
        let mut state = self
            .state
            .lock()
            .unwrap();
        state.value = value.clamp(0.0, 1.0);
        state.start_value = state.value;
    }

    pub fn status(&self) -> AnimationStatus {
        self.state
            .lock()
            .unwrap()
            .status
    }

    pub fn repeat(&self) -> bool {
        self.state
            .lock()
            .unwrap()
            .repeat
    }

    pub fn set_repeat(&self, repeat: bool) {
        self.state
            .lock()
            .unwrap()
            .repeat = repeat;
    }

    pub fn auto_reverse(&self) -> bool {
        self.state
            .lock()
            .unwrap()
            .auto_reverse
    }

    pub fn set_auto_reverse(&self, auto_reverse: bool) {
        self.state
            .lock()
            .unwrap()
            .auto_reverse = auto_reverse;
    }

    fn set_status(state: &mut ControllerState, new_status: AnimationStatus) -> bool {
        if state.status == new_status {
            false
        } else {
            state.status = new_status;
            true
        }
    }

    fn notify_if_changed(&self, changed: bool, status: AnimationStatus) {
        if changed {
            for listener in self
                .listeners
                .lock()
                .unwrap()
                .iter()
            {
                listener.on_status_changed(status);
            }
        }
    }

    /// Advance the animation to the current time. Returns the new curved value.
    ///
    /// Call this once per frame. When the animation completes, the status is
    /// updated to `Completed` or `Dismissed` and `is_animating()` returns
    /// false.
    pub fn tick(&self, now: AnimInstant) -> f32 {
        let (value, curve, notification) = {
            let mut state = self
                .state
                .lock()
                .unwrap();
            let Some(start) = state.start_time else {
                if matches!(state.status, AnimationStatus::Forward | AnimationStatus::Reverse) {
                    state.start_time = Some(now);
                }
                return state
                    .curve
                    .transform(state.value);
            };
            if !matches!(state.status, AnimationStatus::Forward | AnimationStatus::Reverse) {
                return state
                    .curve
                    .transform(state.value);
            }

            let elapsed = now
                .duration_since(start)
                .as_secs_f32();
            let linear_delta = if state
                .duration
                .is_zero()
            {
                1.0
            } else {
                elapsed
                    / state
                        .duration
                        .as_secs_f32()
            };
            let status = state.status;
            let mut notification = None;

            match status {
                AnimationStatus::Forward => {
                    state.value = (state.start_value + linear_delta).min(1.0);
                    if state.value >= 1.0 {
                        if state.repeat {
                            if state.auto_reverse {
                                if Self::set_status(&mut state, AnimationStatus::Reverse) {
                                    notification = Some(AnimationStatus::Reverse);
                                }
                                state.start_value = 1.0;
                            } else {
                                state.start_value = 0.0;
                                state.value = 0.0;
                            }
                            state.start_time = Some(now);
                        } else {
                            state.start_time = None;
                            if Self::set_status(&mut state, AnimationStatus::Completed) {
                                notification = Some(AnimationStatus::Completed);
                            }
                        }
                    }
                }
                AnimationStatus::Reverse => {
                    state.value = (state.start_value - linear_delta).max(0.0);
                    if state.value <= 0.0 {
                        if state.repeat {
                            if state.auto_reverse {
                                if Self::set_status(&mut state, AnimationStatus::Forward) {
                                    notification = Some(AnimationStatus::Forward);
                                }
                                state.start_value = 0.0;
                            } else {
                                state.start_value = 1.0;
                                state.value = 1.0;
                            }
                            state.start_time = Some(now);
                        } else {
                            state.start_time = None;
                            if Self::set_status(&mut state, AnimationStatus::Dismissed) {
                                notification = Some(AnimationStatus::Dismissed);
                            }
                        }
                    }
                }
                _ => {}
            }

            (state.value, state.curve, notification)
        };

        if let Some(status) = notification {
            self.notify_if_changed(true, status);
        }
        curve.transform(value)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU8, Ordering};

    use super::*;

    #[test]
    fn test_forward_completes() {
        let ctrl = AnimationController::with_millis(100, Curve::Linear);
        ctrl.forward();
        assert!(ctrl.is_animating());

        let start = AnimInstant::now();
        let end = start + Duration::from_millis(150);
        ctrl.tick(end);

        assert_eq!(ctrl.status(), AnimationStatus::Completed);
        assert!(!ctrl.is_animating());
        assert!((ctrl.value() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_reverse_completes() {
        let ctrl = AnimationController::with_millis(100, Curve::Linear);
        ctrl.set_value(1.0);
        ctrl.reverse();

        let start = AnimInstant::now();
        let end = start + Duration::from_millis(150);
        ctrl.tick(end);

        assert_eq!(ctrl.status(), AnimationStatus::Dismissed);
        assert!((ctrl.value() - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_reset() {
        let ctrl = AnimationController::with_millis(100, Curve::Linear);
        ctrl.forward();
        ctrl.set_value(0.5);
        ctrl.reset();

        assert_eq!(ctrl.status(), AnimationStatus::Dismissed);
        assert!((ctrl.value() - 0.0).abs() < 1e-9);
        assert!(!ctrl.is_animating());
    }

    #[test]
    fn test_repeat_forward() {
        let ctrl = AnimationController::with_millis(100, Curve::Linear);
        ctrl.set_repeat(true);
        ctrl.forward();

        let start = AnimInstant::now();
        let end = start + Duration::from_millis(150);
        ctrl.tick(end);

        assert!(ctrl.is_animating());
        assert_eq!(ctrl.status(), AnimationStatus::Forward);
    }

    #[test]
    fn test_repeat_auto_reverse() {
        let ctrl = AnimationController::with_millis(100, Curve::Linear);
        ctrl.set_repeat(true);
        ctrl.set_auto_reverse(true);
        ctrl.forward();

        let start = AnimInstant::now();
        let end = start + Duration::from_millis(150);
        ctrl.tick(end);

        assert!(ctrl.is_animating());
        assert_eq!(ctrl.status(), AnimationStatus::Reverse);
    }

    #[test]
    fn test_status_listener_called_on_complete() {
        struct TestListener(AtomicU8);
        impl StatusListener for TestListener {
            fn on_status_changed(&self, _status: AnimationStatus) {
                self.0
                    .fetch_add(1, Ordering::Relaxed);
            }
        }

        let listener = Arc::new(TestListener(AtomicU8::new(0)));
        let ctrl = AnimationController::with_millis(100, Curve::Linear);
        ctrl.add_status_listener(listener.clone());

        ctrl.forward();
        assert_eq!(
            listener
                .0
                .load(Ordering::Relaxed),
            1
        ); // Forward

        let start = AnimInstant::now();
        let end = start + Duration::from_millis(150);
        ctrl.tick(end);

        // Two transitions: Forward → (no change in tick) → Completed
        assert_eq!(
            listener
                .0
                .load(Ordering::Relaxed),
            2
        );
        assert_eq!(ctrl.status(), AnimationStatus::Completed);
    }

    #[test]
    fn test_status_listener_shared_across_clones() {
        struct TestListener(AtomicU8);
        impl StatusListener for TestListener {
            fn on_status_changed(&self, _status: AnimationStatus) {
                self.0
                    .fetch_add(1, Ordering::Relaxed);
            }
        }

        let listener = Arc::new(TestListener(AtomicU8::new(0)));
        let ctrl = AnimationController::with_millis(100, Curve::Linear);
        ctrl.add_status_listener(listener.clone());

        let ctrl2 = ctrl.clone();
        ctrl2.forward();
        assert_eq!(
            listener
                .0
                .load(Ordering::Relaxed),
            1
        );
    }

    #[test]
    fn clone_shares_progress_and_playback_state() {
        let first = AnimationController::with_millis(100, Curve::Linear);
        let second = first.clone();

        first.forward();
        let halfway = first
            .state
            .lock()
            .unwrap()
            .start_time
            .unwrap()
            + Duration::from_millis(50);
        first.tick(halfway);

        assert!((second.value() - 0.5).abs() < 0.01);
        assert_eq!(second.status(), AnimationStatus::Forward);

        second.reverse();
        assert_eq!(first.status(), AnimationStatus::Reverse);
    }

    #[test]
    fn forward_continues_from_current_value() {
        let ctrl = AnimationController::with_millis(100, Curve::Linear);
        ctrl.set_value(0.4);
        ctrl.forward();

        let halfway_through_remaining = ctrl
            .state
            .lock()
            .unwrap()
            .start_time
            .unwrap()
            + Duration::from_millis(30);
        let value = ctrl.tick(halfway_through_remaining);

        assert!((value - 0.7).abs() < 0.01, "expected 0.7, got {value}");
    }

    #[test]
    fn reverse_continues_from_current_value() {
        let ctrl = AnimationController::with_millis(100, Curve::Linear);
        ctrl.set_value(0.6);
        ctrl.reverse();

        let halfway_through_remaining = ctrl
            .state
            .lock()
            .unwrap()
            .start_time
            .unwrap()
            + Duration::from_millis(30);
        let value = ctrl.tick(halfway_through_remaining);

        assert!((value - 0.3).abs() < 0.01, "expected 0.3, got {value}");
    }

    #[test]
    fn sub_millisecond_duration_advances_without_nan() {
        let ctrl = AnimationController::new(Duration::from_micros(500), Curve::Linear);
        ctrl.forward();

        let quarter = ctrl
            .state
            .lock()
            .unwrap()
            .start_time
            .unwrap()
            + Duration::from_micros(125);
        let value = ctrl.tick(quarter);

        assert!(value.is_finite());
        assert!((value - 0.25).abs() < 0.01, "expected 0.25, got {value}");
    }
}
