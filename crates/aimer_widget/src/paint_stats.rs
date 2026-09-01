//! Debug counters for the framework-owned retained-paint seam.
//!
//! The counters describe the work performed by a container's internal
//! `PaintIsolated` policy. They are deliberately kept in the widget crate so
//! framework containers can report the same vocabulary without exposing a
//! cache or renderer handle to widget users.

#[cfg(any(debug_assertions, feature = "frame-stats"))]
use std::cell::Cell;

/// Paint-isolation work observed during one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaintStats {
    /// Retained children considered for framework-owned paint isolation.
    pub candidates: u64,
    /// Retained command streams successfully recorded for later reuse.
    pub records: u64,
    /// Previously recorded command streams reused without recording the child.
    pub replays: u64,
    /// Cached paint invalidated by a changed contract, resource, or element.
    pub invalidations: u64,
    /// Isolation attempts that conservatively used ordinary child drawing.
    pub fallbacks: u64,
    /// Tile command streams successfully recorded for bounded retention.
    pub tile_records: u64,
    /// Cached tiles reused without recording their child content.
    pub tile_replays: u64,
}

#[cfg(any(debug_assertions, feature = "frame-stats"))]
thread_local! {
    static STATS: Cell<PaintStats> = const { Cell::new(PaintStats {
        candidates: 0,
        records: 0,
        replays: 0,
        invalidations: 0,
        fallbacks: 0,
        tile_records: 0,
        tile_replays: 0,
    }) };
}

#[cfg(any(debug_assertions, feature = "frame-stats"))]
#[inline]
fn update(update: impl FnOnce(&mut PaintStats)) {
    STATS.with(|stats| {
        let mut current = stats.get();
        update(&mut current);
        stats.set(current);
    });
}

/// Records a framework-owned child considered for paint isolation.
#[doc(hidden)]
#[inline]
pub fn record_paint_isolation_candidate() {
    #[cfg(any(debug_assertions, feature = "frame-stats"))]
    update(|stats| stats.candidates += 1);
}

/// Records a retained paint command stream created from child drawing.
#[doc(hidden)]
#[inline]
pub fn record_paint_isolation_record() {
    #[cfg(any(debug_assertions, feature = "frame-stats"))]
    update(|stats| stats.records += 1);
}

/// Records a retained paint command stream reused from an earlier frame.
#[doc(hidden)]
#[inline]
pub fn record_paint_isolation_replay() {
    #[cfg(any(debug_assertions, feature = "frame-stats"))]
    update(|stats| stats.replays += 1);
}

/// Records a cached paint stream retired because its validity contract changed.
#[doc(hidden)]
#[inline]
pub fn record_paint_isolation_invalidation() {
    #[cfg(any(debug_assertions, feature = "frame-stats"))]
    update(|stats| stats.invalidations += 1);
}

/// Records an isolation attempt that used the ordinary direct paint path.
#[doc(hidden)]
#[inline]
pub fn record_paint_isolation_fallback() {
    #[cfg(any(debug_assertions, feature = "frame-stats"))]
    update(|stats| stats.fallbacks += 1);
}

/// Records one bounded retained tile created from child drawing.
#[doc(hidden)]
#[inline]
pub fn record_paint_isolation_tile_record() {
    #[cfg(any(debug_assertions, feature = "frame-stats"))]
    update(|stats| stats.tile_records += 1);
}

/// Records one bounded retained tile reused from an earlier frame.
#[doc(hidden)]
#[inline]
pub fn record_paint_isolation_tile_replay() {
    #[cfg(any(debug_assertions, feature = "frame-stats"))]
    update(|stats| stats.tile_replays += 1);
}

/// Clears the paint-isolation counters for the next frame.
#[doc(hidden)]
#[inline]
pub fn reset_paint_stats() {
    #[cfg(any(debug_assertions, feature = "frame-stats"))]
    STATS.with(|stats| stats.set(PaintStats::default()));
}

/// Takes the paint-isolation counters for the completed frame.
#[doc(hidden)]
#[inline]
pub fn take_paint_stats() -> PaintStats {
    #[cfg(any(debug_assertions, feature = "frame-stats"))]
    return STATS.with(|stats| stats.replace(PaintStats::default()));

    #[cfg(not(any(debug_assertions, feature = "frame-stats")))]
    PaintStats::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_stats_accumulate_and_take() {
        reset_paint_stats();
        record_paint_isolation_candidate();
        record_paint_isolation_record();
        record_paint_isolation_replay();
        record_paint_isolation_invalidation();
        record_paint_isolation_fallback();
        record_paint_isolation_tile_record();
        record_paint_isolation_tile_replay();

        assert_eq!(
            take_paint_stats(),
            PaintStats {
                candidates: 1,
                records: 1,
                replays: 1,
                invalidations: 1,
                fallbacks: 1,
                tile_records: 1,
                tile_replays: 1,
            }
        );
        assert_eq!(take_paint_stats(), PaintStats::default());
    }
}
