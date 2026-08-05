//! Where a frame's time actually goes.
//!
//! A frame is three distinct pieces of work, and only the first one belongs to
//! the UI thread:
//!
//! | Phase                     | Work                                            |
//! |---------------------------|-------------------------------------------------|
//! | [`FramePhase::Build`]     | build + layout + paint — the widget walk        |
//! | [`FramePhase::Encode`]    | acquire the surface texture and encode draw calls |
//! | [`FramePhase::Present`]   | hand the swap chain image to the compositor     |
//!
//! Moving encode and present to a raster thread (the `raster-thread` feature)
//! buys nothing unless they are a meaningful share of the frame, and it costs an
//! extra frame of latency. This module is the measurement that decides it.
//!
//! Instrumentation is compiled out unless the `frame-stats` feature is enabled:
//! with the feature off, [`PhaseTimer::start`] reads no clock and
//! [`PhaseTimer::finish`] does nothing, so the timing calls in the render path
//! cost nothing at all.
//!
//! # Examples
//!
//! ```no_run
//! # fn read_it() {
//! let breakdown = aimer_quiver::frame_stats::frame_breakdown();
//! println!(
//!     "build {:?} / encode {:?} / present {:?}",
//!     breakdown.build.average(),
//!     breakdown.encode.average(),
//!     breakdown.present.average()
//! );
//! # }
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// One of the three phases a frame passes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramePhase {
    /// The widget walk: build, layout and paint into a [`DrawList`].
    ///
    /// [`DrawList`]: aimer_cupid::draw_cmd::DrawList
    Build,
    /// Acquiring the surface texture and encoding the recorded draw list.
    Encode,
    /// Presenting the encoded image.
    Present,
}

/// Accumulated timings for a single phase.
///
/// A phase is sampled once per frame that reached it, so `samples` differs
/// between phases when frames are dropped: a frame whose surface texture could
/// not be acquired is built and partially encoded, but never presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PhaseSamples {
    /// How many frames contributed to this phase.
    pub samples: u64,
    /// The summed duration of every sample.
    pub total: Duration,
}

impl PhaseSamples {
    /// The mean time this phase took, or [`Duration::ZERO`] when never sampled.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// use aimer_quiver::frame_stats::PhaseSamples;
    ///
    /// let samples = PhaseSamples {
    ///     samples: 4,
    ///     total: Duration::from_millis(8),
    /// };
    ///
    /// assert_eq!(samples.average(), Duration::from_millis(2));
    /// assert_eq!(PhaseSamples::default().average(), Duration::ZERO);
    /// ```
    #[inline]
    pub fn average(&self) -> Duration {
        if self.samples == 0 {
            return Duration::ZERO;
        }
        self.total / self.samples.min(u32::MAX as u64) as u32
    }
}

/// A snapshot of the frame breakdown, as of the moment it was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameBreakdown {
    /// Build, layout and paint — the part that stays on the UI thread.
    pub build: PhaseSamples,
    /// Surface acquisition and command encoding.
    pub encode: PhaseSamples,
    /// Presentation of the encoded image.
    pub present: PhaseSamples,
}

impl FrameBreakdown {
    /// The mean cost of a whole frame across all three phases.
    #[inline]
    pub fn average_frame(&self) -> Duration {
        self.build.average() + self.encode.average() + self.present.average()
    }

    /// The share of an average frame spent downstream of the widget walk.
    ///
    /// This is the number the `raster-thread` feature is judged by: it is the
    /// fraction of the frame that moving encode and present off the UI thread
    /// could hide. Returns `0.0` before anything has been sampled.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// use aimer_quiver::frame_stats::{FrameBreakdown, PhaseSamples};
    ///
    /// let breakdown = FrameBreakdown {
    ///     build: PhaseSamples {
    ///         samples: 1,
    ///         total: Duration::from_millis(3),
    ///     },
    ///     encode: PhaseSamples {
    ///         samples: 1,
    ///         total: Duration::from_millis(1),
    ///     },
    ///     present: PhaseSamples::default(),
    /// };
    ///
    /// assert_eq!(breakdown.offloadable_share(), 0.25);
    /// ```
    #[inline]
    pub fn offloadable_share(&self) -> f64 {
        let frame = self.average_frame().as_secs_f64();
        if frame <= 0.0 {
            return 0.0;
        }
        (self.encode.average() + self.present.average()).as_secs_f64() / frame
    }
}

/// The lock-free accumulator behind the global statistics.
///
/// Kept separate from the global so the arithmetic is testable, and atomic
/// because the encode and present phases run on the raster thread while the
/// build phase runs on the UI thread.
#[derive(Debug, Default)]
struct FrameAccumulator {
    build: AtomicPhase,
    encode: AtomicPhase,
    present: AtomicPhase,
}

// `record` only has a caller in the render path when instrumentation is
// compiled in; the accumulator itself is always built, and always tested.
#[cfg_attr(not(feature = "frame-stats"), allow(dead_code))]
impl FrameAccumulator {
    #[inline]
    fn record(&self, phase: FramePhase, elapsed: Duration) {
        self.phase(phase).record(elapsed);
    }

    #[inline]
    fn phase(&self, phase: FramePhase) -> &AtomicPhase {
        match phase {
            FramePhase::Build => &self.build,
            FramePhase::Encode => &self.encode,
            FramePhase::Present => &self.present,
        }
    }

    fn snapshot(&self) -> FrameBreakdown {
        FrameBreakdown {
            build: self.build.snapshot(),
            encode: self.encode.snapshot(),
            present: self.present.snapshot(),
        }
    }

    fn reset(&self) {
        self.build.reset();
        self.encode.reset();
        self.present.reset();
    }
}

#[derive(Debug, Default)]
struct AtomicPhase {
    samples: AtomicU64,
    nanos: AtomicU64,
}

#[cfg_attr(not(feature = "frame-stats"), allow(dead_code))]
impl AtomicPhase {
    #[inline]
    fn record(&self, elapsed: Duration) {
        // Relaxed is enough: the counters are statistics, not a happens-before
        // edge for any other data, and a snapshot that catches a phase mid-update
        // is off by at most one frame.
        self.samples.fetch_add(1, Ordering::Relaxed);
        self.nanos
            .fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
    }

    fn snapshot(&self) -> PhaseSamples {
        PhaseSamples {
            samples: self.samples.load(Ordering::Relaxed),
            total: Duration::from_nanos(self.nanos.load(Ordering::Relaxed)),
        }
    }

    fn reset(&self) {
        self.samples.store(0, Ordering::Relaxed);
        self.nanos.store(0, Ordering::Relaxed);
    }
}

static FRAME_STATS: FrameAccumulator = FrameAccumulator {
    build: AtomicPhase {
        samples: AtomicU64::new(0),
        nanos: AtomicU64::new(0),
    },
    encode: AtomicPhase {
        samples: AtomicU64::new(0),
        nanos: AtomicU64::new(0),
    },
    present: AtomicPhase {
        samples: AtomicU64::new(0),
        nanos: AtomicU64::new(0),
    },
};

/// The frame breakdown accumulated so far.
///
/// Every phase reads zero unless the crate was built with the `frame-stats`
/// feature.
#[inline]
pub fn frame_breakdown() -> FrameBreakdown {
    FRAME_STATS.snapshot()
}

/// Drop every sample collected so far.
///
/// Useful to exclude startup — the first frames pay for pipeline creation and
/// glyph atlas population, and are not representative of the steady state.
#[inline]
pub fn reset_frame_breakdown() {
    FRAME_STATS.reset();
}

/// A running measurement of one [`FramePhase`].
///
/// Zero-sized and inert unless the `frame-stats` feature is enabled, so the
/// render path can time itself unconditionally.
///
/// # Examples
///
/// ```
/// use aimer_quiver::frame_stats::{FramePhase, PhaseTimer};
///
/// let timer = PhaseTimer::start();
/// // ... build the frame ...
/// timer.finish(FramePhase::Build);
/// ```
#[derive(Debug)]
pub struct PhaseTimer {
    // `AnimInstant` rather than `std::time::Instant`: the web backend times the
    // same phases, and `std`'s clock is unsupported there.
    #[cfg(feature = "frame-stats")]
    started: aimer_utils::AnimInstant,
}

impl PhaseTimer {
    /// Begin timing a phase.
    #[inline]
    pub fn start() -> Self {
        Self {
            #[cfg(feature = "frame-stats")]
            started: aimer_utils::AnimInstant::now(),
        }
    }

    /// Attribute the elapsed time to `phase`.
    #[inline]
    pub fn finish(self, phase: FramePhase) {
        #[cfg(feature = "frame-stats")]
        FRAME_STATS.record(phase, self.started.elapsed());
        #[cfg(not(feature = "frame-stats"))]
        let _ = phase;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unsampled_phase_averages_to_zero() {
        assert_eq!(PhaseSamples::default().average(), Duration::ZERO);
    }

    #[test]
    fn a_phase_averages_over_its_samples() {
        let accumulator = FrameAccumulator::default();

        accumulator.record(FramePhase::Build, Duration::from_millis(4));
        accumulator.record(FramePhase::Build, Duration::from_millis(2));

        let build = accumulator.snapshot().build;
        assert_eq!(build.samples, 2);
        assert_eq!(build.total, Duration::from_millis(6));
        assert_eq!(build.average(), Duration::from_millis(3));
    }

    #[test]
    fn phases_are_accounted_separately() {
        let accumulator = FrameAccumulator::default();

        accumulator.record(FramePhase::Build, Duration::from_millis(6));
        accumulator.record(FramePhase::Encode, Duration::from_millis(1));
        accumulator.record(FramePhase::Present, Duration::from_millis(1));

        let breakdown = accumulator.snapshot();
        assert_eq!(breakdown.build.average(), Duration::from_millis(6));
        assert_eq!(breakdown.encode.average(), Duration::from_millis(1));
        assert_eq!(breakdown.present.average(), Duration::from_millis(1));
        assert_eq!(breakdown.average_frame(), Duration::from_millis(8));
    }

    #[test]
    fn a_dropped_frame_leaves_the_present_phase_unsampled() {
        let accumulator = FrameAccumulator::default();

        accumulator.record(FramePhase::Build, Duration::from_millis(5));
        accumulator.record(FramePhase::Encode, Duration::from_millis(1));

        let breakdown = accumulator.snapshot();
        assert_eq!(breakdown.build.samples, 1);
        assert_eq!(breakdown.present.samples, 0);
    }

    #[test]
    fn the_offloadable_share_is_everything_after_the_widget_walk() {
        let accumulator = FrameAccumulator::default();

        accumulator.record(FramePhase::Build, Duration::from_millis(6));
        accumulator.record(FramePhase::Encode, Duration::from_millis(1));
        accumulator.record(FramePhase::Present, Duration::from_millis(1));

        assert_eq!(accumulator.snapshot().offloadable_share(), 0.25);
    }

    #[test]
    fn an_empty_breakdown_has_nothing_to_offload() {
        assert_eq!(FrameBreakdown::default().offloadable_share(), 0.0);
    }

    #[test]
    fn resetting_drops_every_sample() {
        let accumulator = FrameAccumulator::default();
        accumulator.record(FramePhase::Build, Duration::from_millis(1));

        accumulator.reset();

        assert_eq!(accumulator.snapshot(), FrameBreakdown::default());
    }

    #[test]
    fn samples_from_several_threads_are_all_counted() {
        static SHARED: FrameAccumulator = FrameAccumulator {
            build: AtomicPhase {
                samples: AtomicU64::new(0),
                nanos: AtomicU64::new(0),
            },
            encode: AtomicPhase {
                samples: AtomicU64::new(0),
                nanos: AtomicU64::new(0),
            },
            present: AtomicPhase {
                samples: AtomicU64::new(0),
                nanos: AtomicU64::new(0),
            },
        };

        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    for _ in 0..100 {
                        SHARED.record(FramePhase::Encode, Duration::from_nanos(10));
                    }
                });
            }
        });

        let encode = SHARED.snapshot().encode;
        assert_eq!(encode.samples, 400);
        assert_eq!(encode.total, Duration::from_nanos(4000));
    }
}
