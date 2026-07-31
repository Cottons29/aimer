use std::time::Duration;

use aimer_animation::{AnimInstant, AnimationController, Curve};

/// The blink timeline of a text field caret.
///
/// A `CaretBlink` is a cheap, cloneable handle over a repeating
/// [`AnimationController`]. Every clone observes the same phase, so the state
/// of a stateful field can own the timeline while the element it builds reads
/// the caret visibility from it. Because the phase lives in the state, a
/// rebuild no longer restarts the blink.
///
/// The timeline advances only when [`CaretBlink::tick`] is called with the
/// current frame time. The caret is opaque during the first half of the period
/// and hidden during the second half, which yields a `period / 2` on-off
/// rhythm driven by the frame clock rather than by a sleeping thread.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// use aimer_animation::AnimInstant;
/// use aimer_input::input::CaretBlink;
///
/// let blink = CaretBlink::new();
/// let start = AnimInstant::now();
///
/// blink.tick(start);
/// assert!(blink.is_visible());
///
/// blink.tick(start + CaretBlink::DEFAULT_PERIOD / 2);
/// assert!(!blink.is_visible());
/// ```
#[derive(Clone, Debug)]
pub struct CaretBlink {
    controller: AnimationController,
}

impl CaretBlink {
    /// The period of a full on-off cycle, matching the platform convention of
    /// a caret that is shown for half a second and hidden for half a second.
    pub const DEFAULT_PERIOD: Duration = Duration::from_millis(1000);

    /// Creates a blink timeline using [`CaretBlink::DEFAULT_PERIOD`].
    ///
    /// The timeline starts at the beginning of its visible half and does not
    /// consume any of the period until the first [`CaretBlink::tick`], so time
    /// spent building widgets is never counted as blink time.
    #[inline]
    pub fn new() -> Self {
        Self::with_period(Self::DEFAULT_PERIOD)
    }

    /// Creates a blink timeline that completes one on-off cycle per `period`.
    ///
    /// A zero `period` keeps the caret permanently visible instead of dividing
    /// by zero, because the controller resolves a zero duration as an
    /// immediately wrapping cycle.
    #[inline]
    pub fn with_period(period: Duration) -> Self {
        let controller = AnimationController::new(period, Curve::Linear);
        controller.set_repeat(true);
        controller.forward_from_first_tick();
        Self { controller }
    }

    /// Returns the duration of one full on-off cycle.
    #[inline]
    pub fn period(&self) -> Duration {
        self.controller.duration()
    }

    /// Returns whether the caret should be painted at the current phase.
    #[inline]
    pub fn is_visible(&self) -> bool {
        self.controller.value() < 0.5
    }

    /// Advances the timeline to `now` and reports whether the caret changed
    /// visibility.
    ///
    /// Call this once per frame while the field owns focus. The phase is
    /// derived from the elapsed time, so a dropped frame delays a toggle by at
    /// most that frame instead of shifting the whole rhythm.
    pub fn tick(&self, now: AnimInstant) -> bool {
        let was_visible = self.is_visible();
        self.controller.tick(now);
        was_visible != self.is_visible()
    }

    /// Restarts the timeline at the beginning of its visible half.
    ///
    /// Editing, moving the caret, or clicking into the field calls this so the
    /// caret stays solid while the user is busy, exactly like a native field.
    /// The new period begins at the next [`CaretBlink::tick`].
    pub fn reset(&self) {
        self.controller.reset();
        self.controller.forward_from_first_tick();
    }
}

impl Default for CaretBlink {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HALF: Duration = Duration::from_millis(500);

    #[test]
    fn new_caret_is_visible_before_the_first_tick() {
        let blink = CaretBlink::new();

        assert!(blink.is_visible());
        assert_eq!(blink.period(), CaretBlink::DEFAULT_PERIOD);
    }

    #[test]
    fn building_does_not_consume_the_period() {
        let blink = CaretBlink::new();
        let start = AnimInstant::now() + Duration::from_secs(30);

        blink.tick(start);

        assert!(blink.is_visible());
        assert!(blink.tick(start + HALF));
    }

    #[test]
    fn caret_hides_after_half_a_period() {
        let blink = CaretBlink::new();
        let start = AnimInstant::now();
        blink.tick(start);

        let toggled = blink.tick(start + HALF);

        assert!(toggled);
        assert!(!blink.is_visible());
    }

    #[test]
    fn caret_reappears_after_a_full_period() {
        let blink = CaretBlink::new();
        let start = AnimInstant::now();
        blink.tick(start);
        blink.tick(start + HALF);

        let toggled = blink.tick(start + CaretBlink::DEFAULT_PERIOD);

        assert!(toggled);
        assert!(blink.is_visible());
    }

    #[test]
    fn ticking_inside_a_half_period_reports_no_toggle() {
        let blink = CaretBlink::new();
        let start = AnimInstant::now();
        blink.tick(start);

        assert!(!blink.tick(start + Duration::from_millis(16)));
        assert!(!blink.tick(start + Duration::from_millis(320)));
        assert!(blink.is_visible());
    }

    #[test]
    fn reset_restores_visibility_and_restarts_the_phase() {
        let blink = CaretBlink::new();
        let start = AnimInstant::now();
        blink.tick(start);
        blink.tick(start + HALF);
        assert!(!blink.is_visible());

        blink.reset();
        let resumed = start + Duration::from_millis(700);
        blink.tick(resumed);

        assert!(blink.is_visible());
        assert!(!blink.tick(resumed + Duration::from_millis(499)));
        assert!(blink.tick(resumed + HALF));
    }

    #[test]
    fn clones_share_one_timeline() {
        let blink = CaretBlink::new();
        let shared = blink.clone();
        let start = AnimInstant::now();

        blink.tick(start);
        blink.tick(start + HALF);

        assert!(!shared.is_visible());

        shared.reset();
        shared.tick(start + Duration::from_millis(600));

        assert!(blink.is_visible());
    }

    #[test]
    fn a_custom_period_scales_both_halves() {
        let blink = CaretBlink::with_period(Duration::from_millis(200));
        let start = AnimInstant::now();
        blink.tick(start);

        assert!(!blink.tick(start + Duration::from_millis(99)));
        assert!(blink.tick(start + Duration::from_millis(100)));
        assert!(blink.tick(start + Duration::from_millis(200)));
    }
}
