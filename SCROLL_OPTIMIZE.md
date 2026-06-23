Improvement Points for Scrollable
Below are concrete, prioritized improvement opportunities I found after reviewing all files in
crates/aimer_container/src/scrollable.

1. Frame timing uses chrono::Utc::now() wall-clock
   Both update_momentum in controller.rs and handle_scroll.rs compute dt from chrono::Utc::now():

let now = Utc::now();
let dt = ... (now - t).num_microseconds() ...;

Problems:

• Wall-clock time can jump NTP sync, DST, manual clock changes, causing scroll glitches/teleports.
• chrono is heavier than needed for monotonic deltas.

Improvement: use a monotonic clock — std::time::Instant — and store Instant in the Cells instead of DateTime<Utc>. This also drops
the chrono dependency for this module.

2. Dead code: ScrollSpring is never used
   scroll_spring.rs defines a critically-damped spring solver, but a grep across the crate shows it is referenced nowhere except its
   own mod declaration. Meanwhile spring-back in update_momentum is implemented ad-hoc with powf/sqrt magic factors.

Improvement: either delete scroll_spring.rs and the pub mod scroll_spring; or actually wire it into the bounce-back path to replace
the hand-rolled spring math. Right now it's confusing dead weight.

3. Scrollbar hover_color / active_color are defined but never applied
   ScrollTrack, ScrollThumb, and ScrollButton all carry hover_color/active_color, but draw_scrollbar in raw_scroll.rs only ever reads
   .color. There is no hover/drag visual feedback even though the controller already tracks cursor_pos, drag_mode, and thumb
   hit-rects.

Improvement: in draw_scrollbar, select the color based on hit-testing the cursor against the thumb hover_color and drag_mode ==
VerticalScrollbar/HorizontalScrollbar active_color. The data and hit-tests already exist; only the color selection is missing.

4. Heavy duplication of per-axis logic
   handle_scroll.rs resistance, velocity clamping, drag handling and raw_scroll.rs button/thumb sizing repeat nearly identical
   Vertical vs Horizontal branches, differing only in .x/.y or width/height.

Improvement: factor the out-of-bounds resistance and velocity-blend math into small axis-agnostic helpers e.g. operate on a (value,
min, max) tuple, or compute both axes uniformly with Vec2d. This roughly halves the branchy code and reduces the chance of the two
axes drifting out of sync.

5. Pervasive magic numbers
   The physics is littered with unexplained constants: 0.4, 0.15, 0.5, 0.75, 0.7, 0.8, 15000.0, 10.0 threshold, 0.25 snap, 0.05/0.005
   dt clamps, 100.0 min viewport, 0.3 resistance scale, etc.

Improvement: hoist these into named consts or fields on ScrollBehavior with short doc comments — e.g. OOB_DAMPING,
FLING_MAX_VELOCITY, DRAG_START_THRESHOLD_DP, SNAP_EPSILON. This makes the feel tunable and self-documenting instead of requiring
archaeology.

6. Only single-axis scrolling is supported
   ScrollAxis is an enum of Vertical | Horizontal, and handle_scroll/draw switch on exactly one axis. There's no diagonal/2D scrolling
   e.g. a large image or canvas, and the unused velocity axis is always forced to 0.0.

Improvement: consider a Both/bitflag axis mode, or generalize the offset math to operate on Vec2d directly so both axes can scroll
simultaneously when content overflows in both dimensions.

7. scroll_offset sign/convention is fragile and under-documented
   The offset is stored negative "content moves up", and clamp_offset does offset.x.max(-max.x).min(-min.x) while visual_offset flips
   signs again min_x = -min.x. This double-negation is error-prone and only partly explained by one comment.

Improvement: document the sign convention in one place ideally on ScrollController, and/or introduce a tiny typed wrapper or helper
so the negation isn't manually repeated in clamp_offset, visual_offset, and draw_scroll.

8. iOS redraw uses a spawned sleeping thread per frame
   In draw_scroll.rs, the iOS branch does:

std::thread::spawn(move || {
std::thread::sleep(Duration::from_millis(1));
window.request_redraw();
});

Spawning a thread every animated frame is wasteful and can race. Improvement: drive animation from the existing frame/redraw loop
or a timer, instead of a fresh thread per frame.

9. Large commented-out block in scrollable.rs
   Lines 45–88 are a near-exact duplicate of the active construction, commented out. Improvement: delete it — it's already preserved
   in version control and just adds noise/maintenance risk.

10. Robustness / correctness gaps
    • content_size(ctx) is called multiple times per draw in draw_scrollbar it's computed inside the if/else; cache it once per frame
    to avoid recomputing child layout.
    • ResolvedSize from computed_size uses max_width/max_height directly — when constraints are f32::MAX no parent bound the viewport
    math max_dim = 1e7 cap in draw_scroll only partially guards against this; the content_size path sets max_height = f32::MAX and
    relies on the child to be finite.
    • No keyboard scrolling: KeyInput/CharInput events are only forwarded to the child; PageUp/PageDown/arrow/Home/End scrolling isn't
    handled.
    • No scroll_to/programmatic API on the controller for jumping to an offset or child.

Suggested priority
• High impact / low risk: #1 monotonic time, #2 dead ScrollSpring, #9 dead commented code, #3 hover/active colors.
• Medium: #5 named constants, #4 axis dedup, #10 caching, keyboard.
• Larger design: #6 2D scrolling, #7 offset convention, #8 iOS redraw loop.