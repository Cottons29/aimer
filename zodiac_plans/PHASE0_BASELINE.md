# Phase 0 baseline — Aimer framework and Jaime

Date: 2026-09-01. This is the first measurement record for
[`DAMAGE_REGION_IMPL.md`](DAMAGE_REGION_IMPL.md). Jaime is treated as the
reproduction fixture; the counters are implemented in the framework.

## Fixture and launch

- Repository: `/Users/cottons/AimerFramework/aimer-widget-features`
- Target: arm64 macOS, Xcode 27.0 SDK, Metal backend.
- Window capture: approximately 1104 × 768 pixels at the default window scale.
- Debug executable:
  `jaime/builds/macos/build/Debug/jaime.app/Contents/MacOS/jaime`
- Release executable:
  `jaime/builds/macos/build/Release/jaime.app/Contents/MacOS/jaime`
- LaunchServices has multiple Jaime bundles with the same bundle identifier,
  so profiling launches the exact executable path rather than using `open`.
- The current Jaime showcase was verified visually. The sidebar can be paged
  from the initial Accessibility semantics entry through the lower catalogue,
  and the content pane changes with selection. This is the current showcase,
  not the stale Radio application from another checkout.

## Instrumentation added

The framework now samples, in debug builds or when `aimer_quiver/frame-stats`
is enabled:

- retained `ElementNode` rebuild-walk visits and dirty-path prunes;
- stateful/stateless rebuild checks;
- stateful/stateless `build` callbacks that actually run; and
- existing retained draw traversal, command, text-cache, image, and
  build/encode/present phase counters;
- framework-owned paint-isolation candidates, records, cache replays,
  invalidations, direct fallbacks, and retained-tile records/replays; and
- full-frame damage clears/pixels plus zero-valued placeholders for partial
  damage regions, coalescing, target reuse, and full-frame promotion.
- per-frame retained layout, routed hit-test, paint, and root-draw calls;
- accepted scroll events, delivered scroll steps, active smoothing steps,
  state-update requests, committed scroll-offset changes, and direct redraw
  requests; and
- frame-wake requests that were accepted or coalesced plus delivered display
  ticks.

The debug handler emits a report every 30 completed frames. Counters are
thread-local at the widget boundary and reset/taken around each frame, so a
report can distinguish a full draw traversal from actual widget rebuilding.
The retained-element layout/paint and routed-event counters are boundary
counts, not exclusive timing measurements; they identify how much work entered
each phase but do not yet attribute CPU time to individual phases.

## What the debug report shows

Representative steady-state 30-frame windows after the first frame:

| Counter | Observed range per frame |
| --- | ---: |
| Build phase | 0.30–0.39 ms |
| Encode phase | 2.00–4.80 ms |
| Present phase | 0.07–0.09 ms |
| Drawn retained nodes | 341–352 |
| Recorded commands | 1,653–1,707 |
| Rebuild-walk visits | 2–3 |
| Rebuild-walk prunes | 2–3 |
| Stateful rebuild checks | 32–33 |
| Stateful builds | 0 |
| Stateless builds | 0 |
| Retained-layer commands | 0 |

The new paint/damage fields are emitted in the same report. The damage
baseline reports one full-frame clear per rendered frame and zero partial
regions/target reuse because the persistent-target `DamageRenderer` is not
integrated yet. The framework now has the first shared `PaintIsolated` seam
and backend-independent `DamageSet` model, but this baseline predates their
performance acceptance.

The freshly assembled debug executable produced the expected initial report:
`paint-candidates/frame=1`, `paint-records/frame=0`,
`paint-replays/frame=0`, `paint-fallbacks/frame=1`,
`damage-full/frame=1`, `damage-full-clears/frame=1`,
`damage-full-pixels/frame=3,680,000`, and zero partial regions, merges,
promotions, target reuse, or partial clears. This is an initial idle/content
report, not the sidebar-scroll acceptance run.

During real sidebar paging, representative 30-frame windows included:

- `build=0.74 ms`, `encode=37.28 ms`, `present=0.59 ms`, with
  `rebuild-visits/frame=187.8`, `stateful-builds/frame=0.8`;
- `build=0.45 ms`, `encode=37.39 ms`, `present=0.24 ms`, with only
  `rebuild-visits/frame=2.0`, `stateful-builds/frame=0.0`; and
- several settling windows returned to roughly `build=0.3–0.4 ms`,
  `encode=2–4 ms`, `rebuild-visits/frame=2`, and zero build callbacks.

The precise number of rebuild visits changes while the sidebar offset is
being propagated, but the ordinary draw traversal remains approximately the
whole retained surface. The reports therefore do not support the claim that
the whole widget tree is rebuilt every frame. They do support the narrower
diagnosis that the current frame still records and encodes a large visual
command stream, with no retained-layer replay in this fixture.

## CPU samples

These are preliminary process samples, not acceptance results. The process
was sampled with macOS `top` once per second for 18 seconds; `%CPU` is the
process value reported by `top`, and both peak and settling behavior are
recorded because a short gesture burst is not a sustained average.

### Debug

- PID 29469, sampled at 01:54:23–01:54:40.
- Real sidebar paging used `Page_Down`/`Page_Up` after focusing the sidebar.
- Idle samples were 0.0%.
- Interaction samples reached 2.0%, 5.0%, 77.7%, 22.8%, and 1.3%; the
  observed peak was 77.7%.
- The process returned to 0.0% after the paging burst settled.

### Optimized release

- PID 32890, sampled at 02:02:44–02:03:00.
- Same real sidebar paging sequence, using the Release bundle.
- The observed interaction samples included 0.2%, 11.7%, 27.3%, 8.1%, and
  0.4%; the observed peak was 27.3%.
- The process returned to 0.0% after the paging burst settled.
- The release sample did not include frame counters because Jaime does not yet
  enable the `aimer_quiver/frame-stats` feature in release mode.

The release result is close enough to the requested `<20%` goal to justify
the framework work, but it is not a pass: this run contains a 27.3% peak and
does not yet have a controlled sustained-average or percentile measurement.

## Phase 0 conclusion

The baseline separates the likely costs:

```text
sidebar offset input
  -> a small/variable retained rebuild walk
  -> root draw still visits the visual tree
  -> roughly 1.6k–1.7k commands are re-recorded
  -> encode/raster work can dominate the burst
```

This makes `PaintIsolated` and `DamageRegion` the appropriate next framework
seams. The first framework counter layer is now present: it can show whether
the existing scroll retention seam records, replays, invalidates, or falls
back, while the full-frame damage baseline makes the absence of persistent
target reuse explicit. It also reports the input-to-frame work chain needed to
separate scroll delivery, retained rebuild, layout, paint, and redraw wakeups.
The next measurement pass should repeat the fixture with a release build that
has frame-stats enabled for measurement only and add reliable glyph/resource,
allocation, and GPU-time measurements. No optimization is declared successful
from this baseline.

## Still outstanding in Phase 0

- 60 Hz versus 120 Hz frame-budget runs and dropped-frame counts.
- Fixed display-scale and refresh-rate capture in the measurement record.
- Live Jaime values for the new scroll, layout, hit-test, paint, root-draw,
  state, redraw, and frame-wake counters during the controlled workload.
- Reliable text-preparation/glyph-miss, image/resource-miss, allocation, and
  GPU-time counters at their respective framework/renderer seams.
- Live Jaime values for the new paint-isolation counters.
- Persistent-target `DamageRenderer` integration; the current damage fields
  are still an instrumented full-frame baseline, and partial-clear,
  target-reuse, and live promotion values remain zero by design.
- Completion of shared `PaintIsolated` adoption for retained tiles and
  dynamic islands, plus live Jaime paint-isolation counter values.
- Controlled idle, offset-only, momentum, sibling-animation, selection,
  hover/focus, resize, scale, theme, image, Markdown, async-resource, and
  effects baselines.
- Sustained release CPU average plus peak and percentile reporting.
