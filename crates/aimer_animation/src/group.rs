use std::time::Duration;
use crate::controller::{AnimationController, AnimationStatus};
use crate::time::AnimInstant;

/// Runs multiple animations simultaneously.
///
/// All controllers start at the same time. The group completes when
/// the last controller completes.
#[derive(Debug, Clone)]
pub struct ParallelAnimation {
    pub controllers: Vec<AnimationController>,
}

impl ParallelAnimation {
    pub fn new(controllers: Vec<AnimationController>) -> Self {
        Self { controllers }
    }

    /// Start all controllers forward.
    pub fn forward(&mut self) {
        for ctrl in &mut self.controllers {
            ctrl.forward();
        }
    }

    /// Start all controllers in reverse.
    pub fn reverse(&mut self) {
        for ctrl in &mut self.controllers {
            ctrl.reverse();
        }
    }

    /// Reset all controllers.
    pub fn reset(&mut self) {
        for ctrl in &mut self.controllers {
            ctrl.reset();
        }
    }

    /// Returns `true` if any controller is still animating.
    pub fn is_animating(&self) -> bool {
        self.controllers.iter().any(|c| c.is_animating())
    }

    /// Tick all controllers. Returns the curved values of each controller.
    pub fn tick(&mut self, now: AnimInstant) -> Vec<f32> {
        self.controllers.iter_mut().map(|c| c.tick(now)).collect()
    }

    /// Returns the aggregate status:
    /// - `Completed` if all are completed
    /// - `Forward` if any is still animating forward
    /// - `Reverse` if any is still animating in reverse
    /// - `Dismissed` if all are dismissed
    pub fn aggregate_status(&self) -> AnimationStatus {
        let has_forward = self.controllers.iter().any(|c| c.status == AnimationStatus::Forward);
        let has_reverse = self.controllers.iter().any(|c| c.status == AnimationStatus::Reverse);
        let all_completed = self.controllers.iter().all(|c| c.status == AnimationStatus::Completed);
        let all_dismissed = self.controllers.iter().all(|c| c.status == AnimationStatus::Dismissed);

        if has_forward {
            AnimationStatus::Forward
        } else if has_reverse {
            AnimationStatus::Reverse
        } else if all_completed {
            AnimationStatus::Completed
        } else if all_dismissed {
            AnimationStatus::Dismissed
        } else {
            AnimationStatus::Completed
        }
    }
}

/// Runs animations one after another.
///
/// The next controller starts when the previous one completes.
#[derive(Debug, Clone)]
pub struct SequentialAnimation {
    pub controllers: Vec<AnimationController>,
    current_index: usize,
}

impl SequentialAnimation {
    pub fn new(controllers: Vec<AnimationController>) -> Self {
        Self { controllers, current_index: 0 }
    }

    /// Start the sequence forward (starts the first controller).
    pub fn forward(&mut self) {
        self.current_index = 0;
        if let Some(ctrl) = self.controllers.first_mut() {
            ctrl.forward();
        }
    }

    /// Start the sequence in reverse (starts the last controller in reverse).
    pub fn reverse(&mut self) {
        self.current_index = self.controllers.len().saturating_sub(1);
        if let Some(ctrl) = self.controllers.last_mut() {
            ctrl.reverse();
        }
    }

    /// Reset all controllers and the sequence index.
    pub fn reset(&mut self) {
        self.current_index = 0;
        for ctrl in &mut self.controllers {
            ctrl.reset();
        }
    }

    /// Returns `true` if the sequence is still running.
    pub fn is_animating(&self) -> bool {
        self.current_index < self.controllers.len()
            && self.controllers[self.current_index].is_animating()
    }

    /// Returns the current controller's value (0.0 if no controllers).
    pub fn current_value(&self) -> f32 {
        self.controllers
            .get(self.current_index)
            .map(|c| c.value)
            .unwrap_or(0.0)
    }

    /// Returns the current controller's status.
    pub fn current_status(&self) -> AnimationStatus {
        self.controllers
            .get(self.current_index)
            .map(|c| c.status)
            .unwrap_or(AnimationStatus::Dismissed)
    }

    /// Tick the current controller. Advances to the next when complete.
    pub fn tick(&mut self, now: AnimInstant) -> f32 {
        if self.current_index >= self.controllers.len() {
            return 0.0;
        }

        let value = self.controllers[self.current_index].tick(now);

        // Advance to next controller if current completed
        if self.controllers[self.current_index].status == AnimationStatus::Completed {
            self.current_index += 1;
            if self.current_index < self.controllers.len() {
                self.controllers[self.current_index].forward();
            }
        }

        value
    }
}

/// Runs animations with a stagger delay between each start.
///
/// Each controller starts `stagger_delay` after the previous one.
#[derive(Debug, Clone)]
pub struct StaggeredAnimation {
    pub controllers: Vec<AnimationController>,
    pub stagger_delay: Duration,
    start_time: Option<AnimInstant>,
    started: Vec<bool>,
}

impl StaggeredAnimation {
    pub fn new(controllers: Vec<AnimationController>, stagger_delay: Duration) -> Self {
        let len = controllers.len();
        Self {
            controllers,
            stagger_delay,
            start_time: None,
            started: vec![false; len],
        }
    }

    /// Start all controllers (they will activate with stagger delays).
    pub fn forward(&mut self) {
        self.start_time = Some(AnimInstant::now());
        self.started = vec![false; self.controllers.len()];
        // Start the first one immediately
        if let Some(ctrl) = self.controllers.first_mut() {
            ctrl.forward();
            self.started[0] = true;
        }
    }

    /// Reset all controllers.
    pub fn reset(&mut self) {
        self.start_time = None;
        self.started = vec![false; self.controllers.len()];
        for ctrl in &mut self.controllers {
            ctrl.reset();
        }
    }

    /// Returns `true` if any controller is still animating or hasn't started yet.
    pub fn is_animating(&self) -> bool {
        if self.start_time.is_none() {
            return false;
        }
        let all_started = self.started.iter().all(|&s| s);
        let any_animating = self.controllers.iter().any(|c| c.is_animating());
        !all_started || any_animating
    }

    /// Tick all controllers, starting delayed ones as their stagger time arrives.
    pub fn tick(&mut self, now: AnimInstant) -> Vec<f32> {
        let start = match self.start_time {
            Some(s) => s,
            None => return self.controllers.iter().map(|_| 0.0).collect(),
        };

        let elapsed = now.duration_since(start);

        // Start delayed controllers
        for i in 1..self.controllers.len() {
            if !self.started[i] {
                let delay = self.stagger_delay * i as u32;
                if elapsed >= delay {
                    self.controllers[i].forward();
                    self.started[i] = true;
                }
            }
        }

        self.controllers.iter_mut().map(|c| c.tick(now)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::Curve;

    #[test]
    fn test_parallel_tick() {
        let mut anim = ParallelAnimation::new(vec![
            AnimationController::with_millis(100, Curve::Linear),
            AnimationController::with_millis(200, Curve::Linear),
        ]);
        anim.forward();

        let start = AnimInstant::now();
        let values = anim.tick(start + Duration::from_millis(150));
        assert!(values[0] > 0.99); // first should be done
        assert!(values[1] < 1.0);  // second still running
    }

    #[test]
    fn test_sequential_advances() {
        let mut anim = SequentialAnimation::new(vec![
            AnimationController::with_millis(100, Curve::Linear),
            AnimationController::with_millis(100, Curve::Linear),
        ]);
        anim.forward();

        let start = AnimInstant::now();

        // After 150ms, first should be complete, second should be running
        anim.tick(start + Duration::from_millis(150));
        assert_eq!(anim.current_index, 1);
        assert!(anim.controllers[1].is_animating());
    }

    #[test]
    fn test_stagger_starts_later_controllers() {
        let mut anim = StaggeredAnimation::new(
            vec![
                AnimationController::with_millis(100, Curve::Linear),
                AnimationController::with_millis(100, Curve::Linear),
                AnimationController::with_millis(100, Curve::Linear),
            ],
            Duration::from_millis(50),
        );
        anim.forward();

        let start = AnimInstant::now();

        // At t=0, only first should be started
        anim.tick(start);
        assert!(anim.started[0]);
        assert!(!anim.started[1]);
        assert!(!anim.started[2]);

        // At t=75ms, first two should be started
        anim.tick(start + Duration::from_millis(75));
        assert!(anim.started[0]);
        assert!(anim.started[1]);
        assert!(!anim.started[2]);

        // At t=125ms, all should be started
        anim.tick(start + Duration::from_millis(125));
        assert!(anim.started.iter().all(|&s| s));
    }
}
