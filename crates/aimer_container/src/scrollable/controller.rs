use crate::scrollable::constants::*;
use crate::scrollable::scroll_behavior::ScrollBehavior;
use crate::scrollable::ScrollAxis;
use aimer_attribute::position::Vec2d;
use aimer_widget::Key;
use aimer_attribute::size::ResolvedSize;
use std::cell::Cell;
use web_time::Instant;
use aimer_utils::{debug, info};

/// Ring buffer of recent velocity samples for trackpad smoothing.
pub(crate) struct VelocityHistory {
    samples: Vec<(f32, f32)>,
    count: usize,
    write_pos: usize,
}

impl VelocityHistory {
    pub(crate) fn new() -> Self {
        Self {
            samples: vec![(0.0, 0.0); VELOCITY_HISTORY_SIZE],
            count: 0,
            write_pos: 0,
        }
    }

    fn push(&mut self, vx: f32, vy: f32) {
        self.samples[self.write_pos] = (vx, vy);
        self.write_pos = (self.write_pos + 1) % VELOCITY_HISTORY_SIZE;
        if self.count < VELOCITY_HISTORY_SIZE {
            self.count += 1;
        }
    }

    fn weighted_average(&self) -> (f32, f32) {
        if self.count == 0 {
            return (0.0, 0.0);
        }
        let mut sum_x = 0.0f32;
        let mut sum_y = 0.0f32;
        let mut weight_sum = 0.0f32;
        // Oldest written sample. When the buffer is full this is `write_pos`
        // (the slot about to be overwritten); when it is only partially filled
        // — e.g. right after `clear()` — the written samples occupy the `count`
        // slots ENDING at `write_pos - 1`, so the oldest is `write_pos - count`.
        // Using `write_pos` unconditionally (the old code) read stale/leftover
        // slots on a partial buffer, so a `clear()` never actually took effect
        // and stale opposite-direction velocity leaked into the release fling.
        let start = (self.write_pos + VELOCITY_HISTORY_SIZE - self.count) % VELOCITY_HISTORY_SIZE;
        for i in 0..self.count {
            // Read oldest-first so newest entries get the heaviest weight.
            let idx = (start + i) % VELOCITY_HISTORY_SIZE;
            let weight = (i + 1) as f32;
            sum_x += self.samples[idx].0 * weight;
            sum_y += self.samples[idx].1 * weight;
            weight_sum += weight;
        }
        (sum_x / weight_sum, sum_y / weight_sum)
    }

    fn clear(&mut self) {
        self.count = 0;
        self.write_pos = 0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragMode {
    None = 0,
    Content = 1,
    VerticalScrollbar = 2,
    HorizontalScrollbar = 3,
    Pending = 4,
}

pub struct ScrollController {
    pub(crate) scroll_behavior: ScrollBehavior,
    pub(crate) axis: ScrollAxis,
    pub(crate) scroll_offset: Cell<Vec2d>,
    /// `PageStorage`-style key this scrollable saves its live offset under, so a
    /// full teardown/re-create restores the position. `None` = not remembered.
    pub(crate) storage_key: Key,
    pub(crate) last_pointer_pos: Cell<Option<Vec2d>>,
    pub(crate) drag_mode: Cell<DragMode>,
    pub(crate) cached_max_scroll: Cell<Vec2d>,
    pub(crate) cached_min_scroll: Cell<Vec2d>,
    pub(crate) pointer_velocity: Cell<Vec2d>,
    pub(crate) last_event_time: Cell<Option<Instant>>,
    pub(crate) last_frame_time: Cell<Option<Instant>>,
    pub(crate) v_thumb_rect: Cell<Option<(f32, f32, f32, f32)>>,
    pub(crate) h_thumb_rect: Cell<Option<(f32, f32, f32, f32)>>,
    pub(crate) v_scroll_multiplier: Cell<f32>,
    pub(crate) h_scroll_multiplier: Cell<f32>,
    pub(crate) last_scale: Cell<f32>,
    pub(crate) speed_multiplier: f32,
    pub(crate) cursor_pos: Cell<Option<Vec2d>>,
    pub(crate) velocity_history: std::cell::RefCell<VelocityHistory>,
    pub(crate) cached_viewport: Cell<(f32, f32)>,
    pub(crate) cached_v_track_width: Cell<f32>,
    pub(crate) cached_h_track_width: Cell<f32>,
    /// Content size computed once at the start of each `draw`, reused by the
    /// scrollbar drawing path so child layout is not recomputed within a frame.
    pub(crate) cached_content_size: Cell<ResolvedSize>,
    /// Wall-clock instant the current release fling started, or `None` when no
    /// cubic-bézier fling is active. While `Some`, momentum is driven by the
    /// curve rather than by per-frame velocity decay.
    pub(crate) fling_start_time: Cell<Option<Instant>>,
    /// Scroll offset captured at the moment the fling started.
    pub(crate) fling_start_offset: Cell<Vec2d>,
    /// Scroll offset the fling eases toward (`start + projected distance`).
    pub(crate) fling_target_offset: Cell<Vec2d>,
    /// Total duration (seconds) of the active bézier fling.
    pub(crate) fling_duration: Cell<f32>,
    /// Primary touch finger ID that owns this scrollable, or `None` for mouse.
    /// Set on first PointerDown, cleared on PointerUp/Cancel.
    pub(crate) active_touch_id: Cell<Option<u64>>,
    /// Velocity of the spring-back oscillation (px/s). Separate from
    /// `pointer_velocity` (which carries momentum / drag velocity) so the
    /// damped-spring simulation can overshoot and oscillate independently.
    pub(crate) spring_velocity: Cell<Vec2d>,
    /// Wall-clock instant the velocity-based momentum started (set on the
    /// first `update_momentum` frame where velocity exceeds epsilon after a
    /// touch release).  Used to hard-cap the glide at
    /// [`MAX_MOMENTUM_DURATION_S`] so it doesn't creep for 15–20 s.
    pub(crate) momentum_start_time: Cell<Option<Instant>>,
    /// Finger delta accumulated since the last emitted drag-velocity sample.
    /// Coalesced same-frame pointer moves (web) fold in here instead of each
    /// producing its own inflated sample; flushed once a real time slice
    /// ([`VELOCITY_SAMPLE_MIN_DT`]) has elapsed.
    pub(crate) vel_accum: Cell<Vec2d>,
    /// Wall-clock instant of the last emitted drag-velocity sample, or `None`
    /// before the first sample of a gesture.
    pub(crate) vel_sample_time: Cell<Option<Instant>>,
}

impl ScrollController {
    /// Adopt the live scroll position (and momentum) from a previous controller.
    ///
    /// Used during reconciliation: when a parent rebuild produces a fresh
    /// scrollable, the newly built controller copies the offset from the old one
    /// so the viewport stays where the user left it instead of snapping to the top.
    pub(crate) fn adopt_scroll_state(&self, prev: &ScrollController) {
        self.scroll_offset.set(prev.scroll_offset.get());
        self.pointer_velocity.set(prev.pointer_velocity.get());
        self.spring_velocity.set(prev.spring_velocity.get());
    }
}

/// One axis of a cubic Bézier with implicit endpoints `P0 = 0`, `P3 = 1`.
#[inline]
fn bezier_axis(s: f32, p1: f32, p2: f32) -> f32 {
    let mt = 1.0 - s;
    3.0 * mt * mt * s * p1 + 3.0 * mt * s * s * p2 + s * s * s
}

/// Derivative (w.r.t. the parameter `s`) of [`bezier_axis`].
#[inline]
fn bezier_axis_deriv(s: f32, p1: f32, p2: f32) -> f32 {
    let mt = 1.0 - s;
    3.0 * mt * mt * p1 + 6.0 * mt * s * (p2 - p1) + 3.0 * s * s * (1.0 - p2)
}

/// Evaluate a CSS-style `cubic-bezier(x1, y1, x2, y2)` easing at linear time
/// fraction `t ∈ [0, 1]`, returning the eased progress `∈ [0, 1]`.
///
/// The control points encode time on the x-axis and progress on the y-axis, so
/// we first solve `bezier_x(s) = t` for the curve parameter `s` (Newton-Raphson
/// with a bisection fallback), then read `bezier_y(s)`.
pub(crate) fn cubic_bezier_ease(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }

    // Newton-Raphson from a sensible initial guess.
    let mut s = t;
    for _ in 0..8 {
        let x = bezier_axis(s, x1, x2) - t;
        if x.abs() < 1e-5 {
            return bezier_axis(s, y1, y2);
        }
        let dx = bezier_axis_deriv(s, x1, x2);
        if dx.abs() < 1e-6 {
            break;
        }
        s = (s - x / dx).clamp(0.0, 1.0);
    }

    // Bisection fallback if Newton stalled.
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    s = t;
    for _ in 0..16 {
        let x = bezier_axis(s, x1, x2);
        if (x - t).abs() < 1e-5 {
            break;
        }
        if x < t {
            lo = s;
        } else {
            hi = s;
        }
        s = 0.5 * (lo + hi);
    }
    bezier_axis(s, y1, y2)
}

impl ScrollController {
    /// Clamp the scroll offset within the allowed range.
    /// scroll_offset is negative (content moves up), so min_scroll <= offset <= 0 typically.
    pub(crate) fn clamp_offset(&self, mut offset: Vec2d) -> Vec2d {
        let min = self.cached_min_scroll.get();
        let max = self.cached_max_scroll.get();
        offset.x = offset.x.max(-max.x).min(-min.x);
        offset.y = offset.y.max(-max.y).min(-min.y);
        offset
    }

    /// Hard-cap overscroll to [`MAX_OVERSCROLL_FRACTION`] of the content size.
    ///
    /// Unlike [`visual_offset`] (which applies a rubber-band *display*
    /// transform), this limits the **actual** stored offset so that momentum
    /// and trackpad events can't carry content hundreds of pixels past the
    /// edge — matching Chrome and iOS behaviour.
    fn clamp_overscroll(&self, offset: Vec2d) -> Vec2d {
        let content = self.cached_content_size.get();
        let max_ox = content.width * MAX_OVERSCROLL_FRACTION;
        let max_oy = content.height * MAX_OVERSCROLL_FRACTION;
        let clamped = self.clamp_offset(offset);
        Vec2d {
            x: offset.x.clamp(clamped.x - max_ox, clamped.x + max_ox),
            y: offset.y.clamp(clamped.y - max_oy, clamped.y + max_oy),
        }
    }

    /// Rational rubber-band visual transform (native macOS/iOS shape).
    ///
    /// Uses `f(x) = (1 - 1/(c·x/d + 1))·d` where `c` is the visual
    /// coefficient scaled by the user's `bouncy_resistance`, `d` is the
    /// viewport dimension, and `x` is the overscroll distance. The curve is
    /// asymptotic — visual stretch grows quickly at first then flattens,
    /// matching the native rubber-band feel.
    #[inline(always)]
    fn apply_bouncy(value: f32, min: f32, max: f32, dimension: f32, resistance: f32) -> f32 {
        let c = RUBBER_BAND_VISUAL_COEFFICIENT * resistance;
        if value < min {
            let diff = min - value;
            let visual = (1.0 - 1.0 / (c * diff / dimension + 1.0)) * dimension;
            min - visual
        } else if value > max {
            let diff = value - max;
            let visual = (1.0 - 1.0 / (c * diff / dimension + 1.0)) * dimension;
            max + visual
        } else {
            value
        }
    }

    pub(crate) fn visual_offset(&self, offset: Vec2d) -> Vec2d {
        let min = self.cached_min_scroll.get();
        let max = self.cached_max_scroll.get();

        let min_x = -min.x;
        let max_x = -max.x;
        let min_y = -min.y;
        let max_y = -max.y;

        if self.scroll_behavior.bouncy {
            let (vp_w, vp_h) = self.cached_viewport.get();
            let resistance = self.scroll_behavior.bouncy_resistance;
            let vx = Self::apply_bouncy(offset.x, max_x, min_x, vp_w.max(MIN_VIEWPORT), resistance);
            let vy = Self::apply_bouncy(offset.y, max_y, min_y, vp_h.max(MIN_VIEWPORT), resistance);

            (vx, vy).into()
        } else {
            (offset.x.clamp(max_x, min_x), offset.y.clamp(max_y, min_y)).into()
        }
    }

    /// Check if a point is inside the vertical thumb rect.
    pub(crate) fn hit_test_v_thumb(&self, p: Vec2d) -> bool {
        if let Some((x, y, w, h)) = self.v_thumb_rect.get() {
            p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h
        } else {
            false
        }
    }

    /// Check if a point is inside the horizontal thumb rect.
    pub(crate) fn hit_test_h_thumb(&self, p: Vec2d) -> bool {
        if let Some((x, y, w, h)) = self.h_thumb_rect.get() {
            p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h
        } else {
            false
        }
    }

    /// Push a velocity sample into the ring buffer for trackpad smoothing.
    pub(crate) fn push_velocity(&self, vx: f32, vy: f32) {
        self.velocity_history.borrow_mut().push(vx, vy);
    }

    /// Return the weighted-average velocity across recent samples.
    pub(crate) fn smoothed_velocity(&self) -> Vec2d {
        let (sx, sy) = self.velocity_history.borrow().weighted_average();
        Vec2d { x: sx, y: sy }
    }

    /// Clear the velocity history (e.g. on pointer-down).
    pub(crate) fn clear_velocity_history(&self) {
        self.velocity_history.borrow_mut().clear();
    }

    /// Fold a raw drag delta (already scaled by `speed_multiplier`) into the
    /// velocity accumulator and, once a real time slice ([`VELOCITY_SAMPLE_MIN_DT`])
    /// has elapsed since the last sample, emit the averaged drag velocity
    /// (px per 120 Hz-frame, both axes) together with that slice's `dt`.
    /// Returns `None` while the delta is still being accumulated within the
    /// current slice.
    ///
    /// This merges the burst of *coalesced* same-`Instant` pointer moves that
    /// web delivers per native `pointermove` into one realistic velocity sample,
    /// instead of letting each tiny sub-delta / ~0 dt inflate the release fling
    /// (~3x too fast on touch). Native, delivering one move per frame, emits on
    /// every call. The offset is still updated per-event by the caller, so
    /// dragging stays 1:1.
    pub(crate) fn accumulate_drag_velocity(&self, dx: f32, dy: f32, now: Instant) -> Option<(Vec2d, f32)> {
        let mut accum = self.vel_accum.get();
        accum.x += dx;
        accum.y += dy;

        let sample_dt = self
            .vel_sample_time
            .get()
            .map(|t| now.duration_since(t).as_secs_f32())
            .unwrap_or(FRAME_REF_120);

        if sample_dt >= VELOCITY_SAMPLE_MIN_DT {
            self.vel_accum.set(Vec2d { x: 0.0, y: 0.0 });
            self.vel_sample_time.set(Some(now));
            let velocity = Vec2d {
                x: (accum.x / sample_dt) * FRAME_REF_120,
                y: (accum.y / sample_dt) * FRAME_REF_120,
            };
            Some((velocity, sample_dt))
        } else {
            self.vel_accum.set(accum);
            None
        }
    }

    /// Cancel any active cubic-bézier release fling.
    ///
    /// Called whenever a new input (touch-down, wheel, keyboard, scrollbar
    /// paging) should take over momentum, so the curve-driven glide does not
    /// keep fighting the fresh interaction.
    pub(crate) fn cancel_fling(&self) {
        self.fling_start_time.set(None);
    }

    /// Arm a cubic-bézier release fling.
    ///
    /// `release_velocity` is the projected launch velocity (px per 120 Hz
    /// frame). The fling runs for a fixed [`FLING_DURATION_S`] window and its
    /// position follows `start + distance · cubic-bezier(t / duration)`.
    ///
    /// The coast distance is derived per axis so the curve's initial slope
    /// (`slope0 = y1 / x1`, the curve's `dy/dx` at `t = 0`) matches the release
    /// speed: the animation leaves the finger at exactly the velocity it was
    /// moving (no visible jump), then decelerates along the curve to a gentle
    /// stop. Because the curve is the only deceleration model here, the friction
    /// field no longer participates in the fling — `FLING_DURATION_S` is the
    /// single knob (longer = farther + slower settle).
    ///
    ///   v0_px_s = release_velocity / FRAME_REF_120         (px per second)
    ///   v(0)    = distance · slope0 / duration  =!  v0_px_s
    ///   ⇒ distance = v0_px_s · duration / slope0
    ///
    /// Currently unused: touch/mouse release now carries momentum through the
    /// shared velocity + friction model (so it matches trackpad feel) rather
    /// than this bézier fling. Kept available as an alternative fling model.
    #[allow(dead_code)]
    pub(crate) fn start_fling(&self, release_velocity: Vec2d, now: Instant) {
        if release_velocity.x == 0.0 && release_velocity.y == 0.0 {
            self.cancel_fling();
            return;
        }

        let duration = FLING_DURATION_S;
        let slope0 = FLING_BEZIER_Y1 / FLING_BEZIER_X1;
        // distance = (v_px_frame / FRAME_REF_120) · duration / slope0.
        let k = duration / (FRAME_REF_120 * slope0);
        let dist = Vec2d {
            x: release_velocity.x * k,
            y: release_velocity.y * k,
        };

        let start = self.scroll_offset.get();
        // debug!("Start: {:?}", start);
        let mut target = Vec2d {
            x: start.x + dist.x,
            y: start.y + dist.y,
        };
        // Non-bouncy scrolling never overshoots, so pin the target to the edge
        // and let the curve ease straight into it.
        if !self.scroll_behavior.bouncy {
            target = self.clamp_offset(target);
        }

        if duration <= 0.0 || (dist.x == 0.0 && dist.y == 0.0) {
            self.cancel_fling();
            return;
        }

        self.fling_start_offset.set(start);
        self.fling_target_offset.set(target);
        self.fling_duration.set(duration);
        self.fling_start_time.set(Some(now));
    }

    /// Check if a point is inside the vertical scrollbar *track* but outside the thumb.
    pub(crate) fn hit_test_v_track(&self, p: Vec2d, viewport_w: f32, viewport_h: f32, track_width: f32) -> bool {
        if let Some((_tx, y, _tw, h)) = self.v_thumb_rect.get() {
            // Track spans the right edge of the viewport.
            let track_left = viewport_w - track_width;
            let in_track_x = p.x >= track_left;
            let in_track_y = p.y >= 0.0 && p.y <= viewport_h;
            let on_thumb = p.y >= y && p.y <= y + h;
            in_track_x && in_track_y && !on_thumb
        } else {
            false
        }
    }

    /// Check if a point is inside the horizontal scrollbar *track* but outside the thumb.
    pub(crate) fn hit_test_h_track(&self, p: Vec2d, viewport_w: f32, viewport_h: f32, track_width: f32) -> bool {
        if let Some((x, _ty, w, _th)) = self.h_thumb_rect.get() {
            let track_top = viewport_h - track_width;
            let in_track_y = p.y >= track_top;
            let in_track_x = p.x >= 0.0 && p.x <= viewport_w;
            let on_thumb = p.x >= x && p.x <= x + w;
            in_track_y && in_track_x && !on_thumb
        } else {
            false
        }
    }

    /// Update momentum, spring-back, and friction during the draw phase (when not dragging).
    /// Returns the updated offset and whether a redraw is needed.
    pub(crate) fn update_momentum(&self, mut offset: Vec2d) -> (Vec2d, bool) {
        let clamped = self.clamp_offset(offset);
        let mut velocity = self.pointer_velocity.get();
        let mut needs_redraw = false;

        // debug!("Offset: ({:.1},{:.1})", offset.x, offset.y);

        // let vel_mag = (velocity.x * velocity.x + velocity.y * velocity.y).sqrt();
        // if vel_mag > VELOCITY_EPSILON {
        //     // info!("[scroll] update_momentum vel=({:.2},{:.2}) mag={:.4} offset=({:.1},{:.1})", velocity.x, velocity.y, vel_mag, offset.x, offset.y);
        // }

        let now = Instant::now();
        let dt = self
            .last_frame_time
            .get()
            .map(|t| now.duration_since(t).as_secs_f32())
            .unwrap_or(FRAME_REF_120)
            .min(MAX_FRAME_DT);
        self.last_frame_time.set(Some(now));

        let frame_ratio = dt / FRAME_REF_120;

        if let Some(fling_start) = self.fling_start_time.get() {
            // Curve-driven release fling: position follows
            // `start + distance · cubic-bezier(t / duration)`. This replaces the
            // per-frame velocity decay while the fling is active so the glide
            // eases to a stop along the requested curve.
            let duration = self.fling_duration.get();
            let elapsed = now.duration_since(fling_start).as_secs_f32();
            let u = if duration > 0.0 { (elapsed / duration).clamp(0.0, 1.0) } else { 1.0 };

            let eased = cubic_bezier_ease(u, FLING_BEZIER_X1, FLING_BEZIER_Y1, FLING_BEZIER_X2, FLING_BEZIER_Y2);
            let start = self.fling_start_offset.get();
            let target = self.fling_target_offset.get();
            let new = Vec2d {
                x: start.x + (target.x - start.x) * eased,
                y: start.y + (target.y - start.y) * eased,
            };

            // Per-frame step, reused as the handoff velocity (px/frame) so the
            // spring-back / out-of-bounds code keeps working if the fling
            // overshoots a bouncy edge.
            let step = Vec2d { x: new.x - offset.x, y: new.y - offset.y };
            let vel = if dt > 0.0 {
                Vec2d { x: step.x / frame_ratio, y: step.y / frame_ratio }
            } else {
                Vec2d::default()
            };

            offset = new;
            // debug!("New Offset : {:?}", new);
            self.pointer_velocity.set(vel);
            needs_redraw = true;

            let oob = offset.x != clamped.x || offset.y != clamped.y;
            if oob && self.scroll_behavior.bouncy {
                // Hand the remaining momentum to the velocity-based spring so the
                // content bounces and recovers from the edge like native iOS.
                self.cancel_fling();
            } else {
                // Snap to rest once finished, or once the tail step becomes
                // sub-pixel (the curve crawls toward the target for a long time
                // after covering ~95% of the distance early).
                let tail_done = u >= FLING_TAIL_START
                    && step.x.abs() < FLING_END_STEP_PX
                    && step.y.abs() < FLING_END_STEP_PX;
                if u >= 1.0 || tail_done {
                    offset = target;
                    self.pointer_velocity.set(Vec2d { x: 0.0, y: 0.0 });
                    self.cancel_fling();
                }
            }
        } else if velocity.x.abs() > VELOCITY_EPSILON || velocity.y.abs() > VELOCITY_EPSILON {
            // Clear any in-flight spring oscillation when fresh momentum begins.
            self.spring_velocity.set(Vec2d { x: 0.0, y: 0.0 });

            // Hard-cap the momentum glide at MAX_MOMENTUM_DURATION_S.
            // Without this the exponential friction tails off asymptotically,
            // letting content creep for 15–20 s before stopping.
            let now_instant = Instant::now();
            // Arm the timer exactly once, on the first momentum frame. Use an
            // `is_none()` sentinel rather than `elapsed == 0.0`: on coarse-clock
            // targets (web `performance.now()` is resolution-clamped, iOS
            // ProMotion/rAF frames get coalesced) two momentum frames can read
            // the same instant, so `duration_since` rounds to exactly 0.0. With
            // the old `== 0.0` check that re-armed the timer every frame, so the
            // elapsed never grew to the cap and a touch fling never stopped at
            // MAX_MOMENTUM_DURATION_S (friction alone tailed off over 15–20 s).
            let momentum_elapsed = match self.momentum_start_time.get() {
                Some(t) => now_instant.duration_since(t).as_secs_f32(),
                None => {
                    self.momentum_start_time.set(Some(now_instant));
                    0.0
                }
            };
            if momentum_elapsed >= MAX_MOMENTUM_DURATION_S {
                self.pointer_velocity.set(Vec2d { x: 0.0, y: 0.0 });
                self.momentum_start_time.set(None);
                return (offset, false);
            } else {
                // Fade-out zone: in the last MOMENTUM_FADEOUT_S seconds before
                // the cap, apply progressively increasing friction so the
                // velocity bleeds to near-zero instead of hitting a wall.
                let remaining = MAX_MOMENTUM_DURATION_S - momentum_elapsed;
                if remaining < MOMENTUM_FADEOUT_S {
                    // progress ∈ [0, 1]: 0 = fade starts, 1 = at the cap.
                    let progress = 1.0 - (remaining / MOMENTUM_FADEOUT_S);
                    // Ramp friction from normal (0.999) down to aggressive (0.90)
                    // as we approach the cap.
                    let fade_friction = 0.999 - 0.099 * progress;
                    velocity.x *= fade_friction.powf(frame_ratio);
                    velocity.y *= fade_friction.powf(frame_ratio);
                }
            }

            // Discrete per-frame velocity decay: v *= friction^(dt / FRAME_REF_120).
            //
            // This matches UIScrollView's deceleration model exactly: a fixed
            // retention factor applied once per frame.  `friction` is calibrated
            // per 120 fps (UIScrollView.DecelerationRate.normal ≈ 0.999 per
            // 120 fps = 0.998 per 60 fps); the `powf(frame_ratio)` makes it
            // frame-rate independent.
            //     60 fps:  v *= 0.999^2.0 ≈ 0.998
            //     120 fps: v *= 0.999^1.0 ≈ 0.999
            let decay = self.scroll_behavior.friction.powf(frame_ratio);

            // Integrate position, then clamp and zero velocity at the edge.
            // On iOS, UIScrollView never lets content fly past the edge during
            // a fling (rubber-band only applies during the drag).
            offset.x += velocity.x * frame_ratio;
            offset.y += velocity.y * frame_ratio;

            // Clamp to overscroll cap immediately after integration.
            // Without this, a strong fling can carry the content hundreds
            // of pixels past the boundary in a single frame, and the spring
            // takes many frames to recover — causing visible shaking.
            if self.scroll_behavior.bouncy {
                let capped = self.clamp_overscroll(offset);
                if capped.x != offset.x {
                    offset.x = capped.x;
                    velocity.x = 0.0;
                }
                if capped.y != offset.y {
                    offset.y = capped.y;
                    velocity.y = 0.0;
                }
            }

            velocity.x *= decay;
            velocity.y *= decay;

            // Recompute clamped boundaries from the post-integration offset.
            // The pre-integration `clamped` is stale — using it would compare
            // the moved offset against its old position, falsely triggering
            // the out-of-bounds path on every frame and killing momentum.
            let clamped = self.clamp_offset(offset);

            // Extra friction in the overscroll zone: content decelerates
            // faster once it crosses the boundary, preventing it from
            // coasting deep into the rubber-band on momentum alone.
            if self.scroll_behavior.bouncy {
                let oob_decay = OVERSCROLL_FRICTION.powf(frame_ratio);
                if offset.x != clamped.x {
                    velocity.x *= oob_decay;
                }
                if offset.y != clamped.y {
                    velocity.y *= oob_decay;
                }
            }

            // Clamp to bounds: if we hit the edge, stop momentum on that axis.
            // For bouncy scrolling, DON'T clamp here — let the offset overshoot
            // so the spring-back code can pull it back with a smooth transition.
            if !self.scroll_behavior.bouncy {
                if offset.x != clamped.x {
                    offset.x = clamped.x;
                    velocity.x = 0.0;
                }
                if offset.y != clamped.y {
                    offset.y = clamped.y;
                    velocity.y = 0.0;
                }
            } else {
                // Bouncy: spring takes over completely when out of bounds.
                //
                // Kill momentum velocity on out-of-bounds axes so the spring
                // is the sole force driving recovery. This prevents residual
                // momentum from fighting the spring and causing shaking.
                let stiffness = SPRING_STIFFNESS;
                let damping_coeff = 2.0 * SPRING_DAMPING_RATIO * stiffness.sqrt();
                let mut sv = self.spring_velocity.get();

                if offset.x != clamped.x {
                    velocity.x = 0.0;
                    let err_x = offset.x - clamped.x;
                    sv.x += (-stiffness * err_x - damping_coeff * sv.x) * dt;
                    offset.x += sv.x * dt;
                }
                if offset.y != clamped.y {
                    velocity.y = 0.0;
                    let err_y = offset.y - clamped.y;
                    sv.y += (-stiffness * err_y - damping_coeff * sv.y) * dt;
                    offset.y += sv.y * dt;
                }
                self.spring_velocity.set(sv);

                // Snap to boundary if the spring crossed through it.
                let new_clamped = self.clamp_offset(offset);
                if offset.x != clamped.x
                    && ((clamped.x >= offset.x && offset.x >= new_clamped.x)
                        || (clamped.x <= offset.x && offset.x <= new_clamped.x))
                {
                    offset.x = new_clamped.x;
                    sv.x = 0.0;
                }
                if offset.y != clamped.y
                    && ((clamped.y >= offset.y && offset.y >= new_clamped.y)
                        || (clamped.y <= offset.y && offset.y <= new_clamped.y))
                {
                    offset.y = new_clamped.y;
                    sv.y = 0.0;
                }
                self.spring_velocity.set(sv);
            }

            self.pointer_velocity.set(velocity);
            needs_redraw = true;
        } else if velocity.x != 0.0 || velocity.y != 0.0 {
            self.pointer_velocity.set(Vec2d { x: 0.0, y: 0.0 });
            self.momentum_start_time.set(None);
        }

        // Spring back if bouncy is enabled AND momentum has finished.
        // Uses a proper damped-spring simulation (underdamped, ζ < 1) so the
        // content overshoots the boundary, oscillates with decreasing amplitude,
        // and settles at rest — the "bounce" feel the user expects.
        let v_check = self.pointer_velocity.get();
        let momentum_active = v_check.x.abs() > VELOCITY_EPSILON || v_check.y.abs() > VELOCITY_EPSILON;
        if self.scroll_behavior.bouncy && !momentum_active && (offset.x != clamped.x || offset.y != clamped.y) {
            let stiffness = SPRING_STIFFNESS;
            let damping_coeff = 2.0 * SPRING_DAMPING_RATIO * stiffness.sqrt();

            let mut sv = self.spring_velocity.get();

            // Semi-implicit (symplectic) Euler: update velocity first, then
            // position.  This is more stable than explicit Euler for
            // oscillatory systems and preserves the energy envelope well.
            let err_x = offset.x - clamped.x;
            let err_y = offset.y - clamped.y;

            sv.x += (-stiffness * err_x - damping_coeff * sv.x) * dt;
            sv.y += (-stiffness * err_y - damping_coeff * sv.y) * dt;
            offset.x += sv.x * dt;
            offset.y += sv.y * dt;

            // If the spring overshot past the boundary (sign flip on err),
            // snap to the boundary and stop.  This prevents the underdamped
            // spring from oscillating through the boundary multiple times.
            let new_err_x = offset.x - clamped.x;
            let new_err_y = offset.y - clamped.y;
            if err_x != 0.0 && (err_x > 0.0) != (new_err_x > 0.0) {
                offset.x = clamped.x;
                sv.x = 0.0;
            }
            if err_y != 0.0 && (err_y > 0.0) != (new_err_y > 0.0) {
                offset.y = clamped.y;
                sv.y = 0.0;
            }

            self.spring_velocity.set(sv);
            needs_redraw = true;

            // Snap to rest when distance from edge and velocity are both negligible.
            if (offset.x - clamped.x).abs() < SNAP_EPSILON && sv.x.abs() < VELOCITY_EPSILON {
                offset.x = clamped.x;
                sv.x = 0.0;
            }
            if (offset.y - clamped.y).abs() < SNAP_EPSILON && sv.y.abs() < VELOCITY_EPSILON {
                offset.y = clamped.y;
                sv.y = 0.0;
            }
            self.spring_velocity.set(sv);
        } else if !self.scroll_behavior.bouncy {
            offset = clamped;
        }

        // Hard-cap overscroll to a fraction of viewport (prevents unlimited
        // rubber-band from trackpad or high-velocity flings).
        offset = self.clamp_overscroll(offset);

        (offset, needs_redraw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use aimer_macro::key;

    fn ctrl_with_offset(offset: Vec2d) -> ScrollController {
        ScrollController {
            scroll_behavior: ScrollBehavior::default(),
            axis: ScrollAxis::Vertical,
            scroll_offset: Cell::new(offset),
            storage_key: key!(),
            last_pointer_pos: Cell::new(None),
            drag_mode: Cell::new(DragMode::None),
            cached_max_scroll: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
            cached_min_scroll: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
            pointer_velocity: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
            last_event_time: Cell::new(None),
            last_frame_time: Cell::new(None),
            v_thumb_rect: Cell::new(None),
            h_thumb_rect: Cell::new(None),
            v_scroll_multiplier: Cell::new(0.0),
            h_scroll_multiplier: Cell::new(0.0),
            last_scale: Cell::new(1.0),
            speed_multiplier: 1.0,
            cursor_pos: Cell::new(None),
            velocity_history: RefCell::new(VelocityHistory::new()),
            cached_viewport: Cell::new((0.0, 0.0)),
            cached_v_track_width: Cell::new(0.0),
            cached_h_track_width: Cell::new(0.0),
            cached_content_size: Cell::new(Default::default()),
            fling_start_time: Cell::new(None),
            fling_start_offset: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
            fling_target_offset: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
            fling_duration: Cell::new(0.0),
            active_touch_id: Cell::new(None),
            spring_velocity: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
            momentum_start_time: Cell::new(None),
            vel_accum: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
            vel_sample_time: Cell::new(None),
        }
    }

    // Reconciliation contract: on rebuild the freshly built controller starts at
    // its initial offset (top), then adopts the previous controller's live scroll
    // position so the viewport doesn't snap back to the top.
    #[test]
    fn adopt_scroll_state_preserves_offset() {
        let prev = ctrl_with_offset(Vec2d { x: 3.0, y: 150.0 });
        prev.pointer_velocity.set(Vec2d { x: 0.0, y: -12.0 });
        prev.spring_velocity.set(Vec2d { x: 0.0, y: -200.0 });

        let fresh = ctrl_with_offset(Vec2d { x: 0.0, y: 0.0 });
        assert_eq!(fresh.scroll_offset.get().y, 0.0, "fresh element starts at the top");

        fresh.adopt_scroll_state(&prev);

        assert_eq!(fresh.scroll_offset.get().x, 3.0);
        assert_eq!(fresh.scroll_offset.get().y, 150.0);
        assert_eq!(fresh.pointer_velocity.get().y, -12.0);
        assert_eq!(fresh.spring_velocity.get().y, -200.0);
    }

    // Coalesced-events contract (the web "scroll too fast" bug): winit delivers
    // one native `pointermove` on web as a BURST of coalesced samples that all
    // read the same `Instant`. A naive per-sample `delta / dt` divides a tiny
    // sub-delta by a ~0 dt, so the release-fling velocity explodes (~Nx the real
    // finger speed for N coalesced samples). `accumulate_drag_velocity` folds a
    // whole frame's coalesced sub-samples into ONE realistic value, so a drag fed
    // as many fine same-instant sub-moves yields the same steady velocity as the
    // identical travel fed as one coarse move per frame — NOT an inflated one.
    #[test]
    fn coalesced_moves_match_coarse_drag_velocity() {
        use std::time::Duration;

        let frame = Duration::from_millis(16); // ~60 Hz
        let t0 = Instant::now();
        let travel_per_frame = 16.0_f32; // px moved each frame
        let frames = 6;
        let sub = 8; // coalesced sub-samples per frame on web

        // Native path: one move per frame.
        let native = ctrl_with_offset(Vec2d { x: 0.0, y: 0.0 });
        let mut last_native = Vec2d { x: 0.0, y: 0.0 };
        let mut t = t0;
        for _ in 0..frames {
            if let Some((v, _)) = native.accumulate_drag_velocity(0.0, travel_per_frame, t) {
                last_native = v;
            }
            t += frame;
        }

        // Web path: the SAME travel each frame, split into `sub` coalesced
        // sub-moves that all share the frame's instant.
        let web = ctrl_with_offset(Vec2d { x: 0.0, y: 0.0 });
        let mut last_web = Vec2d { x: 0.0, y: 0.0 };
        let mut t = t0;
        let step = travel_per_frame / sub as f32;
        for _ in 0..frames {
            for _ in 0..sub {
                if let Some((v, _)) = web.accumulate_drag_velocity(0.0, step, t) {
                    last_web = v;
                }
            }
            t += frame;
        }

        // Steady-state velocities must match (both = travel / frame time); the
        // coalesced burst is not inflated by the number of sub-samples.
        assert!(
            (last_web.y - last_native.y).abs() < 0.5,
            "coalesced web velocity {} must match coarse native velocity {} (not inflated)",
            last_web.y,
            last_native.y
        );
        // Guard the direction of the bug: the fix must not let the web value run
        // away above the real per-frame speed.
        assert!(last_web.y <= last_native.y * 1.2 + 0.5);
    }

    // Small reverse-flick contract: the release fling reads `smoothed_velocity()`,
    // a weighted average of the velocity ring buffer. After a fast fling leaves the
    // buffer full of old-direction samples, a SMALL reverse flick pushes only 1–2
    // opposite samples — not enough to flip that average, so without clearing the
    // buffer the release still coasts the OLD way (the reported bug). Clearing the
    // history on a detected reversal (as handle_scroll now does) makes the release
    // reflect only the new direction.
    #[test]
    fn small_reverse_flick_clears_stale_velocity_history() {
        let c = ctrl_with_offset(Vec2d { x: 0.0, y: 0.0 });

        // Prior fling: buffer full of positive (old-direction) samples.
        for _ in 0..VELOCITY_HISTORY_SIZE {
            c.push_velocity(0.0, 100.0);
        }

        // A small reverse flick adds a single opposite sample.
        c.push_velocity(0.0, -20.0);
        // Without clearing, the weighted average is still the OLD direction.
        assert!(c.smoothed_velocity().y > 0.0, "stale samples keep the average pointing the old way");

        // Reversal handling clears the buffer before the new sample is recorded.
        c.clear_velocity_history();
        c.push_velocity(0.0, -20.0);
        assert!(c.smoothed_velocity().y < 0.0, "after clearing, the release follows the new direction");
    }
}