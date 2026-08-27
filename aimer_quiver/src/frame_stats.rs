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
//! Release instrumentation is compiled out unless the `frame-stats` feature is
//! enabled. Native debug builds collect the timing and content counters used
//! by the scroll profiling workflow; with both debug assertions and the
//! feature off, [`PhaseTimer::start`] reads no clock and
//! [`PhaseTimer::finish`] does nothing.
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

/// Counts the UI work recorded for a group of frames.
///
/// These values describe the build-side workload. They are deliberately kept
/// separate from [`FrameBreakdown`], whose encode and present samples may run
/// on a different thread when raster offloading is enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameContentStats {
    /// Number of frames represented by the counters.
    pub frames: u64,
    /// Number of retained elements reached by drawing.
    pub drawn_nodes: u64,
    /// Number of commands recorded into the frame draw lists.
    pub draw_commands: u64,
    /// Number of compositor-style retained layers recorded into the frame.
    pub retained_layers: u64,
    /// Number of text and text-decoration commands recorded.
    pub text_commands: u64,
    /// Number of already-loaded texture draw commands recorded.
    pub image_draws: u64,
    /// Number of image-byte upload commands recorded.
    pub image_uploads: u64,
    /// Number of text metrics cache hits.
    pub text_cache_hits: u64,
    /// Number of text metrics cache misses.
    pub text_cache_misses: u64,
}

impl FrameContentStats {
    #[inline]
    fn average(value: u64, frames: u64) -> f64 {
        if frames == 0 {
            0.0
        } else {
            value as f64 / frames as f64
        }
    }

    /// Average number of drawn retained elements per frame.
    #[inline]
    pub fn average_drawn_nodes(&self) -> f64 {
        Self::average(self.drawn_nodes, self.frames)
    }

    /// Average number of recorded draw commands per frame.
    #[inline]
    pub fn average_draw_commands(&self) -> f64 {
        Self::average(self.draw_commands, self.frames)
    }

    /// Average number of retained compositor layers per frame.
    #[inline]
    pub fn average_retained_layers(&self) -> f64 {
        Self::average(self.retained_layers, self.frames)
    }

    /// Average number of text commands per frame.
    #[inline]
    pub fn average_text_commands(&self) -> f64 {
        Self::average(self.text_commands, self.frames)
    }

    /// Average number of image draws per frame.
    #[inline]
    pub fn average_image_draws(&self) -> f64 {
        Self::average(self.image_draws, self.frames)
    }

    /// Average number of image uploads per frame.
    #[inline]
    pub fn average_image_uploads(&self) -> f64 {
        Self::average(self.image_uploads, self.frames)
    }

    /// Average number of text metrics cache misses per frame.
    #[inline]
    pub fn average_text_cache_misses(&self) -> f64 {
        Self::average(self.text_cache_misses, self.frames)
    }
}

/// Counts frame-wake requests around the native display/event-loop tick.
///
/// `accepted` is a request that occupied the one pending wake slot;
/// `coalesced` is a request made while that slot was already occupied; and
/// `display_ticks` is a delivered `FrameReady` wake. The counters describe
/// scheduling pressure, not rendered-frame count — a platform may merge or
/// defer a redraw after the wake is delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameRequestStats {
    /// Requests that successfully occupied the pending frame-wake slot.
    pub accepted: u64,
    /// Requests folded into an already pending frame wake.
    pub coalesced: u64,
    /// Native display/event-loop ticks that consumed a pending frame wake.
    pub display_ticks: u64,
}

#[derive(Debug, Default)]
struct FrameRequestAccumulator {
    accepted: AtomicU64,
    coalesced: AtomicU64,
    display_ticks: AtomicU64,
}

impl FrameRequestAccumulator {
    #[inline]
    fn accepted(&self) {
        self.accepted.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    fn coalesced(&self) {
        self.coalesced.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    fn display_tick(&self) {
        self.display_ticks.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> FrameRequestStats {
        FrameRequestStats {
            accepted: self.accepted.load(Ordering::Relaxed),
            coalesced: self.coalesced.load(Ordering::Relaxed),
            display_ticks: self.display_ticks.load(Ordering::Relaxed),
        }
    }

    fn reset(&self) {
        self.accepted.store(0, Ordering::Relaxed);
        self.coalesced.store(0, Ordering::Relaxed);
        self.display_ticks.store(0, Ordering::Relaxed);
    }
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

/// Lock-free accumulation for build-side frame content counters.
#[derive(Debug, Default)]
struct FrameContentAccumulator {
    frames: AtomicU64,
    drawn_nodes: AtomicU64,
    draw_commands: AtomicU64,
    retained_layers: AtomicU64,
    text_commands: AtomicU64,
    image_draws: AtomicU64,
    image_uploads: AtomicU64,
    text_cache_hits: AtomicU64,
    text_cache_misses: AtomicU64,
}

impl FrameContentAccumulator {
    #[inline]
    fn record(
        &self,
        drawn_nodes: u64,
        draw_list: aimer_cupid::draw_cmd::DrawListStats,
        text_cache_hits: u64,
        text_cache_misses: u64,
    ) {
        self.frames.fetch_add(1, Ordering::Relaxed);
        self.drawn_nodes.fetch_add(drawn_nodes, Ordering::Relaxed);
        self.draw_commands
            .fetch_add(draw_list.commands as u64, Ordering::Relaxed);
        self.retained_layers
            .fetch_add(draw_list.retained_layers as u64, Ordering::Relaxed);
        self.text_commands
            .fetch_add(draw_list.text_commands as u64, Ordering::Relaxed);
        self.image_draws
            .fetch_add(draw_list.image_draws as u64, Ordering::Relaxed);
        self.image_uploads
            .fetch_add(draw_list.image_uploads as u64, Ordering::Relaxed);
        self.text_cache_hits
            .fetch_add(text_cache_hits, Ordering::Relaxed);
        self.text_cache_misses
            .fetch_add(text_cache_misses, Ordering::Relaxed);
    }

    fn snapshot(&self) -> FrameContentStats {
        FrameContentStats {
            frames: self.frames.load(Ordering::Relaxed),
            drawn_nodes: self.drawn_nodes.load(Ordering::Relaxed),
            draw_commands: self.draw_commands.load(Ordering::Relaxed),
            retained_layers: self.retained_layers.load(Ordering::Relaxed),
            text_commands: self.text_commands.load(Ordering::Relaxed),
            image_draws: self.image_draws.load(Ordering::Relaxed),
            image_uploads: self.image_uploads.load(Ordering::Relaxed),
            text_cache_hits: self.text_cache_hits.load(Ordering::Relaxed),
            text_cache_misses: self.text_cache_misses.load(Ordering::Relaxed),
        }
    }

    fn reset(&self) {
        self.frames.store(0, Ordering::Relaxed);
        self.drawn_nodes.store(0, Ordering::Relaxed);
        self.draw_commands.store(0, Ordering::Relaxed);
        self.retained_layers.store(0, Ordering::Relaxed);
        self.text_commands.store(0, Ordering::Relaxed);
        self.image_draws.store(0, Ordering::Relaxed);
        self.image_uploads.store(0, Ordering::Relaxed);
        self.text_cache_hits.store(0, Ordering::Relaxed);
        self.text_cache_misses.store(0, Ordering::Relaxed);
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

static FRAME_CONTENT_STATS: FrameContentAccumulator = FrameContentAccumulator {
    frames: AtomicU64::new(0),
    drawn_nodes: AtomicU64::new(0),
    draw_commands: AtomicU64::new(0),
    retained_layers: AtomicU64::new(0),
    text_commands: AtomicU64::new(0),
    image_draws: AtomicU64::new(0),
    image_uploads: AtomicU64::new(0),
    text_cache_hits: AtomicU64::new(0),
    text_cache_misses: AtomicU64::new(0),
};

static FRAME_REQUEST_STATS: FrameRequestAccumulator = FrameRequestAccumulator {
    accepted: AtomicU64::new(0),
    coalesced: AtomicU64::new(0),
    display_ticks: AtomicU64::new(0),
};

#[cfg(debug_assertions)]
const DEBUG_REPORT_INTERVAL: u64 = 30;

/// The frame breakdown accumulated so far.
///
/// Every phase reads zero unless the crate was built with the `frame-stats`
/// feature or with native debug assertions enabled.
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

/// The build-side content counters accumulated so far.
#[inline]
pub fn frame_content_stats() -> FrameContentStats {
    FRAME_CONTENT_STATS.snapshot()
}

/// Drop every build-side content sample collected so far.
#[inline]
pub fn reset_frame_content_stats() {
    FRAME_CONTENT_STATS.reset();
}

/// The native frame-wake counters accumulated so far.
#[inline]
pub fn frame_request_stats() -> FrameRequestStats {
    FRAME_REQUEST_STATS.snapshot()
}

/// Drop every native frame-wake counter collected so far.
#[inline]
pub fn reset_frame_request_stats() {
    FRAME_REQUEST_STATS.reset();
}

/// Records a request that occupied the native pending frame-wake slot.
#[doc(hidden)]
#[inline]
pub fn record_frame_request_accepted() {
    #[cfg(any(feature = "frame-stats", debug_assertions))]
    FRAME_REQUEST_STATS.accepted();
}

/// Records a request folded into an already pending native frame wake.
#[doc(hidden)]
#[inline]
pub fn record_frame_request_coalesced() {
    #[cfg(any(feature = "frame-stats", debug_assertions))]
    FRAME_REQUEST_STATS.coalesced();
}

/// Records a delivered native display/event-loop tick.
#[doc(hidden)]
#[inline]
pub fn record_display_tick() {
    #[cfg(any(feature = "frame-stats", debug_assertions))]
    FRAME_REQUEST_STATS.display_tick();
}

/// Records one frame's draw traversal and command-stream workload.
#[doc(hidden)]
#[inline]
pub fn record_frame_content(
    drawn_nodes: u64,
    draw_list: aimer_cupid::draw_cmd::DrawListStats,
    text_cache_hits: u64,
    text_cache_misses: u64,
) {
    FRAME_CONTENT_STATS.record(
        drawn_nodes,
        draw_list,
        text_cache_hits,
        text_cache_misses,
    );
}

/// Takes a periodic debug report and resets the two frame accumulators.
///
/// The windowed handler calls this after a completed frame. Returning a report
/// every thirty build frames keeps the terminal useful during a long scroll
/// without adding a log call to the hot path itself.
#[doc(hidden)]
#[cfg(debug_assertions)]
pub(crate) fn take_debug_report() -> Option<(FrameBreakdown, FrameContentStats)> {
    let content = FRAME_CONTENT_STATS.snapshot();
    if content.frames < DEBUG_REPORT_INTERVAL {
        return None;
    }
    FRAME_CONTENT_STATS.reset();
    let breakdown = FRAME_STATS.snapshot();
    FRAME_STATS.reset();
    Some((breakdown, content))
}

/// A running measurement of one [`FramePhase`].
///
/// Zero-sized and inert in release builds unless the `frame-stats` feature is
/// enabled, so the render path can time itself unconditionally. Native debug
/// builds keep the timers active for scroll profiling.
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
    #[cfg(any(feature = "frame-stats", debug_assertions))]
    started: aimer_utils::AnimInstant,
}

impl PhaseTimer {
    /// Begin timing a phase.
    #[inline]
    pub fn start() -> Self {
        Self {
            #[cfg(any(feature = "frame-stats", debug_assertions))]
            started: aimer_utils::AnimInstant::now(),
        }
    }

    /// Attribute the elapsed time to `phase`.
    #[inline]
    pub fn finish(self, phase: FramePhase) {
        #[cfg(any(feature = "frame-stats", debug_assertions))]
        FRAME_STATS.record(phase, self.started.elapsed());
        #[cfg(not(any(feature = "frame-stats", debug_assertions)))]
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
    fn frame_request_stats_distinguish_coalesced_wakes_from_display_ticks() {
        let accumulator = FrameRequestAccumulator::default();

        accumulator.accepted();
        accumulator.coalesced();
        accumulator.coalesced();
        accumulator.display_tick();

        assert_eq!(
            accumulator.snapshot(),
            FrameRequestStats {
                accepted: 1,
                coalesced: 2,
                display_ticks: 1,
            }
        );
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

    #[test]
    fn frame_content_accumulates_and_averages_work() {
        let accumulator = FrameContentAccumulator::default();

        accumulator.record(
            10,
            aimer_cupid::draw_cmd::DrawListStats {
                commands: 20,
                retained_layers: 3,
                text_commands: 8,
                image_draws: 2,
                image_uploads: 1,
            },
            4,
            1,
        );
        accumulator.record(
            6,
            aimer_cupid::draw_cmd::DrawListStats {
                commands: 10,
                retained_layers: 0,
                text_commands: 2,
                image_draws: 0,
                image_uploads: 3,
            },
            2,
            3,
        );

        let stats = accumulator.snapshot();
        assert_eq!(stats.frames, 2);
        assert_eq!(stats.drawn_nodes, 16);
        assert_eq!(stats.draw_commands, 30);
        assert_eq!(stats.retained_layers, 3);
        assert_eq!(stats.text_commands, 10);
        assert_eq!(stats.image_draws, 2);
        assert_eq!(stats.image_uploads, 4);
        assert_eq!(stats.text_cache_hits, 6);
        assert_eq!(stats.text_cache_misses, 4);
        assert_eq!(stats.average_drawn_nodes(), 8.0);
        assert_eq!(stats.average_draw_commands(), 15.0);
        assert_eq!(stats.average_retained_layers(), 1.5);
    }
}
