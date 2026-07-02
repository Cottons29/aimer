//! Named physics/feel constants for the scrollable container.
//!
//! These were previously scattered as magic numbers throughout
//! `controller.rs` and `handle_scroll.rs`. Hoisting them here makes the
//! scroll feel tunable and self-documenting.

/// Reference frame duration used to normalize velocity against a 120 Hz frame.
pub const FRAME_REF_120: f32 = 1.0 / 120.0;
/// Reference frame duration used to normalize velocity against a 60 Hz frame.
pub const FRAME_REF_60: f32 = 1.0 / 60.0;

/// Upper clamp for a single frame's delta time (seconds) during momentum.
/// Guards against huge jumps after the app was paused/backgrounded.
pub const MAX_FRAME_DT: f32 = 0.05;
/// Lower clamp for the delta time measured between scroll-wheel events.
pub const MIN_EVENT_DT: f32 = 0.005;
/// Lower clamp for the delta time measured between pointer-move events.
pub const MIN_MOVE_DT: f32 = 0.001;

/// Velocity magnitude (px/frame) below which momentum is considered stopped.
pub const VELOCITY_EPSILON: f32 = 0.01;

/// Out-of-bounds velocity damping base on iOS (stronger pull-back).
pub const OOB_DAMPING_BASE_IOS: f32 = 0.15;
/// Out-of-bounds velocity damping base on other platforms.
pub const OOB_DAMPING_BASE_DEFAULT: f32 = 0.4;
/// Extra velocity damping applied when overshooting further out of bounds.
/// Lower values let the content coast further into the rubber-band zone.
pub const OOB_OVERSHOOT_DAMPING: f32 = 0.25;

/// Exponent applied to the out-of-bounds distance for the bouncy stretch.
/// 0.85 gives a natural, visible overshoot — content moves noticeably when
/// dragged past the edge, then compresses gently at the extremes.
pub const BOUNCY_STRETCH_EXPONENT: f32 = 0.85;
/// Multiplier converting `bouncy_resistance` into a stretch scale.
/// High value (12) so the visual rubber-band reaches ~45% of viewport height.
pub const BOUNCY_RESISTANCE_SCALE: f32 = 12.0;
/// Extra resistance multiplier for non-touch devices (desktop with mouse/trackpad).
/// Touch devices get 1.0, non-touch get 1.5 (50% more resistance).
/// This is because mouse/trackpad users have finer control and expect more
/// resistance when overscrolling.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub const BOUNCY_RESISTANCE_NON_TOUCH_SCALE: f32 = 0.8;
#[cfg(any(target_os = "ios", target_os = "android"))]
pub const BOUNCY_RESISTANCE_NON_TOUCH_SCALE: f32 = 1.0;

/// Per-frame velocity damping applied during spring-back.
/// Higher values (0.8) = less damping = smoother, longer recovery.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub const SPRING_VELOCITY_DAMPING: f32 = 0.1;
#[cfg(any(target_os = "ios", target_os = "android"))]
pub const SPRING_VELOCITY_DAMPING: f32 = 0.8;
/// Distance (px) under which spring-back snaps exactly to the clamped offset.
pub const SNAP_EPSILON: f32 = 0.5;

/// Minimum viewport extent (px) used to scale out-of-bounds resistance.
pub const MIN_VIEWPORT: f32 = 100.0;
/// Upper clamp for the normalized out-of-bounds distance.
pub const OOB_RESISTANCE_CLAMP: f32 = 0.75;
/// Scale factor for the quadratic out-of-bounds drag resistance.
/// Very low value so dragging past the edge feels yielding and soft.
pub const OOB_RESISTANCE_SCALE: f32 = 0.1;

/// Maximum fling velocity (px/frame at scale 1.0) from a scroll-wheel event.
pub const MAX_SCROLL_VELOCITY: f32 = 15000.0;

/// Gain applied to the peak drag velocity when a touch/mouse drag is released.
///
/// On release the peak finger velocity now seeds the SAME velocity +
/// exponential-friction momentum the trackpad/wheel `Scroll` path uses (see
/// `handle_scroll.rs`), so a touch flick decelerates exactly like a trackpad
/// flick. A trackpad already feeds a near-1:1 velocity into that model, so this
/// gain is kept close to 1.0 — just a light projection so a quick swipe gets a
/// satisfying push without overshooting the natural, trackpad-matched glide that
/// the shared `friction` decay then carries to rest.
pub const RELEASE_VELOCITY_GAIN: f32 = 1.5;

/// Control points of the cubic-bézier curve that shapes the release fling.
///
/// `cubic-bezier(0.17, 0.74, 0.30, 1.0)` — a smooth, gliding deceleration curve
/// (P0 = (0,0), P3 = (1,1) are implicit). The content position over the fling is
/// `start + distance · bezier(t / duration)`. Compared with the previous,
/// strongly front-loaded curve (which shot off fast then crawled — a "gliding on
/// mud" feel), this gentler initial slope launches the content without a harsh
/// jump and carries it through a long, even glide that eases cleanly to rest,
/// like sliding on ice.
pub const FLING_BEZIER_X1: f32 = 0.17;
pub const FLING_BEZIER_Y1: f32 = 0.74;
pub const FLING_BEZIER_X2: f32 = 0.30;
pub const FLING_BEZIER_Y2: f32 = 1.0;

/// Total duration (seconds) of a release fling shaped by the cubic-bézier curve.
///
/// The glide eases from the release speed to a stop over this fixed window. The
/// coast distance is derived per axis from the release velocity so the curve's
/// initial slope matches the finger speed (no visible jump on lift-off) — a
/// longer duration therefore also glides farther. Tunable: raise it to slide
/// farther / settle slower, lower it for a snappier stop.
pub const FLING_DURATION_S: f32 = 2.0;

/// Normalized time fraction after which the bézier fling may snap to rest.
///
/// The curve reaches ~95% of its distance early (by `X2`), then crawls toward
/// the target over a long, barely-visible tail. Once past this fraction we let
/// the fling end as soon as the per-frame step becomes sub-pixel, so the glide
/// settles cleanly instead of creeping for seconds.
pub const FLING_TAIL_START: f32 = 0.5;
/// Per-frame step (px) below which, in the tail, the bézier fling snaps to rest.
pub const FLING_END_STEP_PX: f32 = 0.5;

/// Size of the ring buffer used for trackpad velocity smoothing.
pub const VELOCITY_HISTORY_SIZE: usize = 5;

/// Drag activation threshold in device-independent pixels.
pub const DRAG_START_THRESHOLD_DP: f32 = 10.0;
/// Base blend weight for newly measured drag velocity.
pub const DRAG_BLEND_BASE: f32 = 0.4;
/// Time window (seconds) over which drag velocity blend ramps to full weight.
pub const DRAG_BLEND_WINDOW: f32 = 0.1;

/// Smoothing factor for direct scrollbar-thumb dragging (kept old / new).
pub const SCROLLBAR_DRAG_SMOOTH_OLD: f32 = 0.4;
pub const SCROLLBAR_DRAG_SMOOTH_NEW: f32 = 0.6;

/// Idle time (ms) after the last event past which residual velocity is cleared.
/// Must be generous enough for mobile platforms where touch delivery latency
/// (iOS gesture arbitration, ProMotion batching) can easily exceed 100ms
/// between the last PointerMove and the PointerUp that ends the gesture.
pub const VELOCITY_RESET_IDLE_MS: u128 = 400;

/// Time (ms) after which a lingering `active_touch_id` is considered stale and
/// cleared on the next PointerDown. This is a safety net for iOS where the app
/// can be backgrounded without receiving a Cancel/PointerUp, leaving the
/// scrollable stuck with a dead touch ID that rejects all new touches.
pub const STALE_TOUCH_THRESHOLD_MS: u128 = 1000;

/// Ease-out exponent for spring-back (1 − (1−t)^n).
/// Lower values = smoother, more gradual recovery.
pub const EASE_OUT_CUBIC: f32 = 1.5;

/// Keyboard scroll step (logical px per keypress).
pub const KEYBOARD_SCROLL_STEP: f32 = 40.0;
/// Keyboard page-scroll fraction of viewport (0.0–1.0).
pub const KEYBOARD_PAGE_FRACTION: f32 = 0.85;

/// Duration (ms) for scrollbar fade-in / fade-out transitions.
pub const SCROLLBAR_SHOW_DURATION_MS: u64 = 200;
pub const SCROLLBAR_HIDE_DURATION_MS: u64 = 400;
