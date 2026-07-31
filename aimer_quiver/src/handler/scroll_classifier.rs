use crate::handler::scroll_utils::MomentumScroller;
use aimer_attribute::Vec2d;
use aimer_utils::AnimInstant as Instant;
use std::collections::VecDeque;
use winit::dpi::PhysicalPosition;
use winit::event::TouchPhase;

/// Input-device behavior inferred from a stream of scroll deltas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollSource {
    /// A high-frequency, often two-axis pixel stream.
    Trackpad,
    /// A repeated pulse stream produced by a discrete wheel.
    Wheel,
    /// Too little history is available to decide safely.
    Unknown,
}

struct EventSample {
    delta: PhysicalPosition<f64>,
    at: Instant,
}

pub struct ScrollClassifier {
    history: VecDeque<EventSample>,
    window: usize,
    current: ScrollSource,
    reset_gap_ms: f64,
}

impl ScrollClassifier {
    pub fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(8),
            window: 6,
            current: ScrollSource::Unknown,
            reset_gap_ms: 250.0,
        }
    }

    pub fn classify(&mut self, delta: PhysicalPosition<f64>) -> ScrollSource {
        let now = Instant::now();

        if let Some(last) = self.history.back() {
            if now.duration_since(last.at).as_secs_f64() * 1000.0 > self.reset_gap_ms {
                self.history.clear();
                self.current = ScrollSource::Unknown;
            }
        }

        self.history.push_back(EventSample { delta, at: now });
        if self.history.len() > self.window {
            self.history.pop_front();
        }

        if self.history.len() < 2 {
            return self.current;
        }

        let samples = self
            .history
            .iter()
            .map(|sample| sample.delta)
            .collect::<Vec<_>>();
        let total_gap_ms = self
            .history
            .iter()
            .zip(self.history.iter().skip(1))
            .map(|(previous, next)| next.at.duration_since(previous.at).as_secs_f64() * 1000.0)
            .sum::<f64>();
        let avg_gap_ms = total_gap_ms / (self.history.len() - 1) as f64;

        let classified = Self::classify_samples(&samples, avg_gap_ms);
        if classified != ScrollSource::Unknown {
            self.current = classified;
        }

        self.current
    }

    fn clear(&mut self) {
        self.history.clear();
        self.current = ScrollSource::Unknown;
    }

    /// Classifies a short pixel-delta history independently of platform event
    /// variants.
    ///
    /// Browsers expose both touchpads and many mouse wheels as `PixelDelta`.
    /// Touchpads are identified by fast cadence or two-axis variation, while a
    /// wheel is identified by sparse, repeated pulses. Ambiguous histories
    /// remain unknown rather than forcing a potentially jarring device switch.
    fn classify_samples(samples: &[PhysicalPosition<f64>], avg_gap_ms: f64) -> ScrollSource {
        if samples.len() < 2 {
            return ScrollSource::Unknown;
        }

        if samples.iter().any(|sample| sample.x.abs() > 0.01) {
            return ScrollSource::Trackpad;
        }

        let repeated = samples
            .iter()
            .filter(|sample| (sample.y - samples[0].y).abs() <= 0.01)
            .count();
        let repeat_ratio = repeated as f64 / samples.len() as f64;
        if (samples.len() >= 3 && repeat_ratio >= 0.8)
            || (repeat_ratio >= 0.5 && avg_gap_ms >= 25.0)
        {
            ScrollSource::Wheel
        } else if avg_gap_ms < 20.0 {
            ScrollSource::Trackpad
        } else {
            ScrollSource::Unknown
        }
    }
}

/// Tracks the gesture phase of a single smoothing channel.
///
/// A platform delta and the frame that finally delivers it are not the same
/// event: the smoother spreads one input pulse over several frames, so the
/// phase reported by the window system cannot be forwarded verbatim. This
/// tracker converts the platform stream into a well-formed per-channel
/// sequence — exactly one [`TouchPhase::Started`], any number of
/// [`TouchPhase::Moved`], and exactly one terminating [`TouchPhase::Ended`]
/// or [`TouchPhase::Cancelled`].
///
/// Platforms that do report explicit phases (macOS trackpads) end the gesture
/// only after the window system said so *and* the queued distance is fully
/// delivered. Platforms that only ever report [`TouchPhase::Moved`] (a plain
/// mouse wheel on Windows or X11) would otherwise never end, so their gesture
/// ends when the channel drains.
#[derive(Debug, Default)]
struct ScrollPhaseTracker {
    /// A gesture is in flight, meaning `Started` has already been emitted.
    active: bool,
    /// The next emitted frame opens a gesture and must carry `Started`.
    start_pending: bool,
    /// The window system reported the end of the current gesture.
    input_ended: bool,
    /// The window system cancelled the current gesture.
    cancelled: bool,
    /// The window system reports explicit `Started`/`Ended` phases.
    platform_driven: bool,
}

impl ScrollPhaseTracker {
    /// Folds a platform phase into the tracked gesture state.
    fn on_input(&mut self, phase: TouchPhase) {
        match phase {
            TouchPhase::Started => {
                self.platform_driven = true;
                self.input_ended = false;
                self.cancelled = false;
                if !self.active {
                    self.start_pending = true;
                }
            }
            TouchPhase::Moved => {
                // Momentum deltas keep arriving as `Moved` after the platform
                // reported `Ended`, so they extend the current gesture instead
                // of terminating it early.
                self.input_ended = false;
                self.cancelled = false;
                if !self.active {
                    self.start_pending = true;
                }
            }
            TouchPhase::Ended => {
                self.platform_driven = true;
                self.input_ended = true;
            }
            TouchPhase::Cancelled => {
                self.platform_driven = true;
                self.cancelled = true;
            }
        }
    }

    /// Returns whether the gesture terminates on this frame.
    #[inline]
    fn should_end(&self, still_draining: bool) -> bool {
        !still_draining && (self.input_ended || !self.platform_driven)
    }

    /// Returns whether a terminating frame is still owed to the receiver.
    #[inline]
    fn has_pending_end(&self) -> bool {
        self.cancelled || (self.active && self.should_end(false))
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Classifies and frame-synchronizes wheel input without changing its distance.
///
/// Aimer keeps this state at the window boundary because browsers commonly
/// report both trackpads and mouse wheels as `PixelDelta`. The two internal
/// channels use different response rates and remain separate when emitted so
/// widgets can retain device-appropriate momentum policy.
pub struct DualScroller {
    classifier: ScrollClassifier,
    trackpad: MomentumScroller,
    wheel: MomentumScroller,
    trackpad_phase: ScrollPhaseTracker,
    wheel_phase: ScrollPhaseTracker,
    pending_unknown: Vec<PhysicalPosition<f64>>,
    pending_phase: Option<TouchPhase>,
}

/// One frame-synchronized portion of a scroll gesture.
///
/// The delta is the distance to deliver on this frame and the phase describes
/// where the frame sits inside the gesture. A terminating frame may carry a
/// zero delta when the window system ends a gesture whose distance has already
/// been delivered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollStep {
    pub delta: PhysicalPosition<f64>,
    pub phase: TouchPhase,
}

/// Frame-synchronized deltas produced for each input-device behavior.
///
/// Trackpad and mouse-wheel output remains separate so callers can preserve
/// pixel-precise behavior for trackpads while preventing wheel-only momentum.
#[derive(Debug, Default)]
pub struct ScrollFrame {
    pub trackpad: Option<ScrollStep>,
    pub wheel: Option<ScrollStep>,
}

impl DualScroller {
    const BASE_PIXEL_LINE: f32 = 1.0;
    const MAX_VELO: f32 = 80.0;

    pub fn new() -> Self {
        let mut trackpad = MomentumScroller::new();
        trackpad.pixels_per_line = Self::BASE_PIXEL_LINE;
        trackpad.friction = 0.15;
        trackpad.max_velocity = Self::MAX_VELO;

        let mut wheel = MomentumScroller::new();
        wheel.pixels_per_line = 1.0;
        wheel.friction = 0.40;
        wheel.max_velocity = 60.0;

        Self {
            classifier: ScrollClassifier::new(),
            trackpad,
            wheel,
            trackpad_phase: ScrollPhaseTracker::default(),
            wheel_phase: ScrollPhaseTracker::default(),
            pending_unknown: Vec::with_capacity(2),
            pending_phase: None,
        }
    }

    /// Queues a browser/native pixel delta after classifying its device shape.
    ///
    /// Ambiguous initial samples are held briefly instead of being emitted with
    /// the wrong behavior. Once cadence or axis variation identifies the
    /// device, all held distance is transferred to the matching smoother. The
    /// platform `phase` is folded into the receiving channel so the gesture is
    /// replayed to widgets with a faithful start and end.
    pub fn on_pixel_delta(&mut self, delta: PhysicalPosition<f64>, phase: TouchPhase) {
        if !delta.x.is_finite() || !delta.y.is_finite() {
            return;
        }
        
        match self.classifier.classify(delta) {
            ScrollSource::Wheel => {
                self.flush_unknown(ScrollSource::Wheel);
                self.wheel_phase.on_input(phase);
                Self::queue(&mut self.wheel, delta);
            }
            ScrollSource::Trackpad => {
                self.flush_unknown(ScrollSource::Trackpad);
                self.trackpad_phase.on_input(phase);
                Self::queue(&mut self.trackpad, delta);
            }
            ScrollSource::Unknown => {
                self.pending_unknown.push(delta);
                self.pending_phase = Some(phase);
            }
        }
    }

    /// Queues a native `LineDelta`, whose source is already unambiguous.
    pub fn on_wheel_delta(&mut self, delta: PhysicalPosition<f64>, phase: TouchPhase) {
        if !delta.x.is_finite() || !delta.y.is_finite() {
            return;
        }
        self.wheel_phase.on_input(phase);
        Self::queue(&mut self.wheel, delta);
    }

    /// Produces at most one step per source for the current rendered frame.
    pub fn tick(&mut self) -> ScrollFrame {
        if !self.pending_unknown.is_empty() {
            // One isolated pixel pulse cannot be classified by cadence. Treat
            // it conservatively as a wheel so it is never lost and never
            // creates trackpad-style release momentum.
            self.flush_unknown(ScrollSource::Wheel);
        }
        ScrollFrame {
            trackpad: Self::tick_channel(&mut self.trackpad, &mut self.trackpad_phase),
            wheel: Self::tick_channel(&mut self.wheel, &mut self.wheel_phase),
        }
    }

    /// Emits the next step of one channel together with its gesture phase.
    ///
    /// A cancelled gesture drops the undelivered distance immediately, because
    /// the input it came from is no longer valid. Otherwise the phase follows
    /// the channel: the first emitted frame opens the gesture, the drain frame
    /// closes it, and a gesture ended after its distance was already delivered
    /// closes with a zero-distance frame so the receiver is never left hanging.
    fn tick_channel(
        scroller: &mut MomentumScroller,
        tracker: &mut ScrollPhaseTracker,
    ) -> Option<ScrollStep> {
        if tracker.cancelled {
            scroller.clear();
            tracker.reset();
            return Some(ScrollStep {
                delta: PhysicalPosition::new(0.0, 0.0),
                phase: TouchPhase::Cancelled,
            });
        }

        match scroller.tick() {
            Some(delta) => {
                let phase = if std::mem::take(&mut tracker.start_pending) {
                    TouchPhase::Started
                } else if tracker.should_end(scroller.is_active()) {
                    TouchPhase::Ended
                } else {
                    TouchPhase::Moved
                };
                if phase == TouchPhase::Ended {
                    tracker.reset();
                } else {
                    tracker.active = true;
                }
                Some(ScrollStep { delta, phase })
            }
            None => {
                if tracker.active && tracker.should_end(false) {
                    tracker.reset();
                    Some(ScrollStep {
                        delta: PhysicalPosition::new(0.0, 0.0),
                        phase: TouchPhase::Ended,
                    })
                } else {
                    None
                }
            }
        }
    }

    /// Returns whether another frame is still owed, either as distance or as a
    /// terminating phase.
    #[inline]
    pub fn is_active(&self) -> bool {
        self.trackpad.is_active()
            || self.wheel.is_active()
            || self.trackpad_phase.has_pending_end()
            || self.wheel_phase.has_pending_end()
    }

    fn flush_unknown(&mut self, source: ScrollSource) {
        if self.pending_unknown.is_empty() {
            return;
        }
        let phase = self.pending_phase.take().unwrap_or(TouchPhase::Moved);
        match source {
            ScrollSource::Trackpad => self.trackpad_phase.on_input(phase),
            ScrollSource::Wheel => self.wheel_phase.on_input(phase),
            ScrollSource::Unknown => unreachable!("unknown samples cannot flush as unknown"),
        }
        for delta in self.pending_unknown.drain(..) {
            match source {
                ScrollSource::Trackpad => Self::queue(&mut self.trackpad, delta),
                ScrollSource::Wheel => Self::queue(&mut self.wheel, delta),
                ScrollSource::Unknown => unreachable!("unknown samples cannot flush as unknown"),
            }
        }
    }

    fn queue(scroller: &mut MomentumScroller, delta: PhysicalPosition<f64>) {
        scroller.on_line_delta(Vec2d {
            x: delta.x as f32,
            y: delta.y as f32,
        });
    }

    pub fn clear(&mut self) {
        self.classifier.clear();
        self.trackpad.clear();
        self.wheel.clear();
        self.trackpad_phase.reset();
        self.wheel_phase.reset();
        self.pending_unknown.clear();
        self.pending_phase = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasm_trackpad_samples_are_classified_as_trackpad() {
        let samples = [
            PhysicalPosition::new(0.0, -2.0),
            PhysicalPosition::new(0.0, -2.0),
            PhysicalPosition::new(2.0, -4.0),
            PhysicalPosition::new(4.0, -4.0),
            PhysicalPosition::new(4.0, -6.0),
            PhysicalPosition::new(2.0, -2.0),
        ];

        assert_eq!(
            ScrollClassifier::classify_samples(&samples, 8.0),
            ScrollSource::Trackpad
        );
    }

    #[test]
    fn wasm_repeated_sparse_pixel_samples_are_classified_as_wheel() {
        let samples = [
            PhysicalPosition::new(0.0, -8.00048828125),
            PhysicalPosition::new(0.0, -8.00048828125),
            PhysicalPosition::new(0.0, -8.00048828125),
        ];

        assert_eq!(
            ScrollClassifier::classify_samples(&samples, 32.0),
            ScrollSource::Wheel
        );
    }

    #[test]
    fn repeated_wasm_wheel_pulses_stay_wheel_at_fast_cadence() {
        let samples = [
            PhysicalPosition::new(0.0, -8.00048828125),
            PhysicalPosition::new(0.0, -8.00048828125),
            PhysicalPosition::new(0.0, -8.00048828125),
        ];

        assert_eq!(
            ScrollClassifier::classify_samples(&samples, 8.0),
            ScrollSource::Wheel
        );
    }

    #[test]
    fn isolated_ambiguous_pixel_delta_is_emitted_without_trackpad_momentum() {
        let mut scroller = DualScroller::new();
        scroller.on_pixel_delta(PhysicalPosition::new(0.0, -8.00048828125), TouchPhase::Moved);

        let frame = scroller.tick();

        assert!(frame.trackpad.is_none());
        assert!(frame.wheel.is_some());
    }

    #[test]
    fn rapid_two_axis_pixel_stream_uses_trackpad_channel() {
        let mut scroller = DualScroller::new();
        scroller.on_pixel_delta(PhysicalPosition::new(0.0, -2.0), TouchPhase::Moved);
        scroller.on_pixel_delta(PhysicalPosition::new(2.0, -4.0), TouchPhase::Moved);

        let frame = scroller.tick();

        assert!(frame.trackpad.is_some());
        assert!(frame.wheel.is_none());
    }

    #[test]
    fn non_finite_pixel_delta_is_ignored() {
        let mut scroller = DualScroller::new();
        scroller.on_pixel_delta(
            PhysicalPosition::new(f64::NAN, f64::INFINITY),
            TouchPhase::Moved,
        );

        let frame = scroller.tick();

        assert!(frame.trackpad.is_none());
        assert!(frame.wheel.is_none());
        assert!(!scroller.is_active());
    }

    fn drain_wheel_phases(scroller: &mut DualScroller) -> Vec<TouchPhase> {
        let mut phases = Vec::new();
        let mut frames = 0;
        loop {
            if let Some(step) = scroller.tick().wheel {
                phases.push(step.phase);
            }
            frames += 1;
            assert!(frames < 64, "scroll channel never terminated");
            if !scroller.is_active() {
                return phases;
            }
        }
    }

    #[test]
    fn wheel_gesture_reports_started_moved_and_ended_phases() {
        let mut scroller = DualScroller::new();
        scroller.on_wheel_delta(PhysicalPosition::new(0.0, -40.0), TouchPhase::Moved);

        let phases = drain_wheel_phases(&mut scroller);

        assert_eq!(phases.first(), Some(&TouchPhase::Started));
        assert_eq!(phases.last(), Some(&TouchPhase::Ended));
        assert!(phases.len() > 2);
        assert!(
            phases[1..phases.len() - 1]
                .iter()
                .all(|phase| *phase == TouchPhase::Moved)
        );
    }

    #[test]
    fn platform_reported_end_waits_for_the_queued_distance() {
        let mut scroller = DualScroller::new();
        scroller.on_wheel_delta(PhysicalPosition::new(0.0, -40.0), TouchPhase::Started);
        scroller.on_wheel_delta(PhysicalPosition::new(0.0, -40.0), TouchPhase::Ended);

        let phases = drain_wheel_phases(&mut scroller);

        assert_eq!(phases.first(), Some(&TouchPhase::Started));
        assert_eq!(phases.last(), Some(&TouchPhase::Ended));
        assert_eq!(
            phases
                .iter()
                .filter(|phase| **phase == TouchPhase::Ended)
                .count(),
            1
        );
    }

    #[test]
    fn platform_gesture_stays_open_until_the_platform_ends_it() {
        let mut scroller = DualScroller::new();
        scroller.on_wheel_delta(PhysicalPosition::new(0.0, -40.0), TouchPhase::Started);

        let mut phases = Vec::new();
        for _ in 0..32 {
            if let Some(step) = scroller.tick().wheel {
                phases.push(step.phase);
            }
        }

        // The distance is fully delivered, yet the gesture stays open because
        // the window system has not lifted the fingers.
        assert_eq!(phases.first(), Some(&TouchPhase::Started));
        assert!(!phases.contains(&TouchPhase::Ended));
        assert!(!scroller.is_active());

        scroller.on_wheel_delta(PhysicalPosition::new(0.0, 0.0), TouchPhase::Ended);
        assert!(scroller.is_active());
        let tail = drain_wheel_phases(&mut scroller);

        assert_eq!(tail, vec![TouchPhase::Ended]);
        assert!(!scroller.is_active());
    }

    #[test]
    fn cancelled_gesture_drops_pending_distance_and_reports_cancel() {
        let mut scroller = DualScroller::new();
        scroller.on_wheel_delta(PhysicalPosition::new(0.0, -400.0), TouchPhase::Started);
        assert!(scroller.tick().wheel.is_some());

        scroller.on_wheel_delta(PhysicalPosition::new(0.0, 0.0), TouchPhase::Cancelled);
        let step = scroller.tick().wheel.expect("cancel must be delivered");

        assert_eq!(step.phase, TouchPhase::Cancelled);
        assert_eq!(step.delta.y, 0.0);
        assert!(!scroller.is_active());
        assert!(scroller.tick().wheel.is_none());
    }

    #[test]
    fn trackpad_and_wheel_channels_track_their_phases_independently() {
        let mut scroller = DualScroller::new();
        scroller.on_pixel_delta(PhysicalPosition::new(2.0, -4.0), TouchPhase::Started);
        scroller.on_pixel_delta(PhysicalPosition::new(4.0, -4.0), TouchPhase::Started);
        scroller.on_wheel_delta(PhysicalPosition::new(0.0, -40.0), TouchPhase::Moved);

        let frame = scroller.tick();

        assert_eq!(
            frame.trackpad.map(|step| step.phase),
            Some(TouchPhase::Started)
        );
        assert_eq!(frame.wheel.map(|step| step.phase), Some(TouchPhase::Started));
    }
}
