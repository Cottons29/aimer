use crate::handler::scroll_utils::MomentumScroller;
use aimer_attribute::Vec2d;
use aimer_utils::AnimInstant as Instant;
use std::collections::VecDeque;
use winit::dpi::PhysicalPosition;

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
    pending_unknown: Vec<PhysicalPosition<f64>>,
}

/// Frame-synchronized deltas produced for each input-device behavior.
///
/// Trackpad and mouse-wheel output remains separate so callers can preserve
/// pixel-precise behavior for trackpads while preventing wheel-only momentum.
#[derive(Debug, Default)]
pub struct ScrollFrame {
    pub trackpad: Option<PhysicalPosition<f64>>,
    pub wheel: Option<PhysicalPosition<f64>>,
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
            pending_unknown: Vec::with_capacity(2),
        }
    }

    /// Queues a browser/native pixel delta after classifying its device shape.
    ///
    /// Ambiguous initial samples are held briefly instead of being emitted with
    /// the wrong behavior. Once cadence or axis variation identifies the
    /// device, all held distance is transferred to the matching smoother.
    pub fn on_pixel_delta(&mut self, delta: PhysicalPosition<f64>) {
        if !delta.x.is_finite() || !delta.y.is_finite() {
            return;
        }
        match self.classifier.classify(delta) {
            ScrollSource::Wheel => {
                self.flush_unknown(ScrollSource::Wheel);
                Self::queue(&mut self.wheel, delta);
            }
            ScrollSource::Trackpad => {
                self.flush_unknown(ScrollSource::Trackpad);
                Self::queue(&mut self.trackpad, delta);
            }
            ScrollSource::Unknown => self.pending_unknown.push(delta),
        }
    }

    /// Queues a native `LineDelta`, whose source is already unambiguous.
    pub fn on_wheel_delta(&mut self, delta: PhysicalPosition<f64>) {
        if !delta.x.is_finite() || !delta.y.is_finite() {
            return;
        }
        Self::queue(&mut self.wheel, delta);
    }

    /// Produces at most one delta per source for the current rendered frame.
    pub fn tick(&mut self) -> ScrollFrame {
        if !self.pending_unknown.is_empty() {
            // One isolated pixel pulse cannot be classified by cadence. Treat
            // it conservatively as a wheel so it is never lost and never
            // creates trackpad-style release momentum.
            self.flush_unknown(ScrollSource::Wheel);
        }
        ScrollFrame {
            trackpad: self.trackpad.tick(),
            wheel: self.wheel.tick(),
        }
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.trackpad.is_active() || self.wheel.is_active()
    }

    fn flush_unknown(&mut self, source: ScrollSource) {
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
        self.pending_unknown.clear();
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
        scroller.on_pixel_delta(PhysicalPosition::new(0.0, -8.00048828125));

        let frame = scroller.tick();

        assert!(frame.trackpad.is_none());
        assert!(frame.wheel.is_some());
    }

    #[test]
    fn rapid_two_axis_pixel_stream_uses_trackpad_channel() {
        let mut scroller = DualScroller::new();
        scroller.on_pixel_delta(PhysicalPosition::new(0.0, -2.0));
        scroller.on_pixel_delta(PhysicalPosition::new(2.0, -4.0));

        let frame = scroller.tick();

        assert!(frame.trackpad.is_some());
        assert!(frame.wheel.is_none());
    }

    #[test]
    fn non_finite_pixel_delta_is_ignored() {
        let mut scroller = DualScroller::new();
        scroller.on_pixel_delta(PhysicalPosition::new(f64::NAN, f64::INFINITY));

        let frame = scroller.tick();

        assert!(frame.trackpad.is_none());
        assert!(frame.wheel.is_none());
        assert!(!scroller.is_active());
    }
}
