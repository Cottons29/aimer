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
pub const OOB_OVERSHOOT_DAMPING: f32 = 0.5;

/// Exponent applied to the out-of-bounds distance for the bouncy stretch.
pub const BOUNCY_STRETCH_EXPONENT: f32 = 0.75;
/// Multiplier converting `bouncy_resistance` into a stretch scale.
pub const BOUNCY_RESISTANCE_SCALE: f32 = 2.0;

/// Per-frame velocity damping applied during spring-back.
pub const SPRING_VELOCITY_DAMPING: f32 = 0.7;
/// Distance (px) under which spring-back snaps exactly to the clamped offset.
pub const SNAP_EPSILON: f32 = 0.25;

/// Minimum viewport extent (px) used to scale out-of-bounds resistance.
pub const MIN_VIEWPORT: f32 = 100.0;
/// Upper clamp for the normalized out-of-bounds distance.
pub const OOB_RESISTANCE_CLAMP: f32 = 0.75;
/// Scale factor for the quadratic out-of-bounds drag resistance.
pub const OOB_RESISTANCE_SCALE: f32 = 0.3;

/// Maximum fling velocity (px/frame at scale 1.0) from a scroll-wheel event.
pub const MAX_SCROLL_VELOCITY: f32 = 15000.0;

/// Weight kept from the previous velocity when blending a wheel fling.
pub const WHEEL_BLEND_OLD: f32 = 0.7;
/// Weight given to the new target velocity when blending a wheel fling.
pub const WHEEL_BLEND_NEW: f32 = 0.8;

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
pub const VELOCITY_RESET_IDLE_MS: u128 = 100;
