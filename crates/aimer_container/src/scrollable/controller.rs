use crate::scrollable::constants::*;
use crate::scrollable::scroll_behavior::ScrollBehavior;
use crate::scrollable::ScrollAxis;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use std::cell::Cell;
use web_time::Instant;
use aimer_utils::info;

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
        for i in 0..self.count {
            // Read oldest-first so newest entries get the heaviest weight.
            let idx = (self.write_pos + i) % VELOCITY_HISTORY_SIZE;
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

    #[inline(always)]
    fn apply_bouncy(value: f32, min: f32, max: f32, resistance: f32) -> f32 {
        // Non-touch devices (desktop) get 50% more resistance for a stiffer feel.
        let scaled_resistance = resistance * BOUNCY_RESISTANCE_SCALE * BOUNCY_RESISTANCE_NON_TOUCH_SCALE;
        if value < min {
            let diff = min - value;
            min - diff.powf(BOUNCY_STRETCH_EXPONENT) * scaled_resistance
        } else if value > max {
            let diff = value - max;
            max + diff.powf(BOUNCY_STRETCH_EXPONENT) * scaled_resistance
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
            let resistance = self.scroll_behavior.bouncy_resistance;
            let vx = Self::apply_bouncy(offset.x, max_x, min_x, resistance);
            let vy = Self::apply_bouncy(offset.y, max_y, min_y, resistance);

            let stretch_x = (vx - offset.x).abs();
            let stretch_y = (vy - offset.y).abs();
            // if stretch_x > 0.5 || stretch_y > 0.5 {
            //     info!(
            //         "rubber-band stretch | offset: ({:.1}, {:.1}) | visual: ({:.1}, {:.1}) | stretch: ({:.1}, {:.1}) | resistance: {:.2}",
            //         offset.x, offset.y, vx, vy, stretch_x, stretch_y, resistance
            //     );
            // }

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
            // Discrete per-frame velocity decay: v *= friction^(dt / FRAME_REF_120).
            //
            // This matches UIScrollView's deceleration model exactly: a fixed
            // retention factor applied once per frame.  `friction` is calibrated
            // per 60 fps (UIScrollView.DecelerationRate.normal = 0.9998); the
            // `powf(frame_ratio)` makes it frame-rate independent.
            //     60 fps:  v *= 0.9998^1.0 = 0.9998
            //     120 fps: v *= 0.9998^0.5 = 0.9999
            let decay = self.scroll_behavior.friction.powf(frame_ratio);

            // Integrate position, then clamp and zero velocity at the edge.
            // On iOS, UIScrollView never lets content fly past the edge during
            // a fling (rubber-band only applies during the drag).
            offset.x += velocity.x * frame_ratio;
            offset.y += velocity.y * frame_ratio;

            velocity.x *= decay;
            velocity.y *= decay;

            // Clamp to bounds: if we hit the edge, stop momentum on that axis.
            // For bouncy scrolling, DON'T clamp here — let the offset overshoot
            // so the spring-back code can pull it back with a smooth transition.
            if !self.scroll_behavior.bouncy {
                let new_clamped = self.clamp_offset(offset);
                if offset.x != new_clamped.x {
                    offset.x = new_clamped.x;
                    velocity.x = 0.0;
                }
                if offset.y != new_clamped.y {
                    offset.y = new_clamped.y;
                    velocity.y = 0.0;
                }
            } else {
                // Bouncy: only zero velocity that pushes FURTHER out of bounds.
                // If velocity is pushing toward the edge (helping recovery),
                // let it continue — this prevents oscillation when the user
                // scrolls while spring-back is active.
                let new_clamped = self.clamp_offset(offset);
                if offset.x != new_clamped.x {
                    let dx = new_clamped.x - offset.x;
                    // Zero velocity only if it's moving AWAY from the edge
                    if (dx > 0.0 && velocity.x < 0.0) || (dx < 0.0 && velocity.x > 0.0) {
                        velocity.x = 0.0;
                    }
                }
                if offset.y != new_clamped.y {
                    let dy = new_clamped.y - offset.y;
                    // Zero velocity only if it's moving AWAY from the edge
                    if (dy > 0.0 && velocity.y < 0.0) || (dy < 0.0 && velocity.y > 0.0) {
                        velocity.y = 0.0;
                    }
                }
            }

            self.pointer_velocity.set(velocity);
            needs_redraw = true;
        } else if velocity.x != 0.0 || velocity.y != 0.0 {
            self.pointer_velocity.set(Vec2d { x: 0.0, y: 0.0 });
        }

        // Spring back if bouncy is enabled AND momentum has finished.
        // During active momentum the exponential decay drives the offset;
        // spring-back only kicks in once velocity drops to near-zero to pull
        // the content back to the edge.  Without this guard, spring-back
        // fights the momentum every frame (applying SPRING_VELOCITY_DAMPING
        // and killing the coast almost instantly).
        let v_check = self.pointer_velocity.get();
        let momentum_active = v_check.x.abs() > VELOCITY_EPSILON || v_check.y.abs() > VELOCITY_EPSILON;
        if self.scroll_behavior.bouncy && !momentum_active && (offset.x != clamped.x || offset.y != clamped.y) {
            let dx = clamped.x - offset.x;
            let dy = clamped.y - offset.y;
            let oob_dist = (dx * dx + dy * dy).sqrt();

            // info!(
            //     "rubber-band spring-back | oob_dist: {:.1}px | offset: ({:.1}, {:.1}) | target: ({:.1}, {:.1})",
            //     oob_dist, offset.x, offset.y, clamped.x, clamped.y
            // );

            let base_spring = 1.0 - (1.0 - self.scroll_behavior.bouncy_recovery).powf(frame_ratio);
            // Cubic ease-out: fast start, gentle landing (1 − (1−t)³)
            let spring_factor = 1.0 - (1.0 - base_spring).powf(EASE_OUT_CUBIC);

            let v = self.pointer_velocity.get();

            // Only spring back on axes where velocity is not actively
            // carrying content in the same direction as the spring pull.
            let spring_x = offset.x != clamped.x
                && !(v.x.abs() > VELOCITY_EPSILON && dx.signum() == v.x.signum());
            let spring_y = offset.y != clamped.y
                && !(v.y.abs() > VELOCITY_EPSILON && dy.signum() == v.y.signum());

            if spring_x {
                offset.x += dx * spring_factor;
            }
            if spring_y {
                offset.y += dy * spring_factor;
            }

            // Clamp to prevent overshooting the edge — this makes the spring
            // critically damped (no oscillation, smooth one-way return).
            if spring_x {
                let new_dx = clamped.x - offset.x;
                if (dx > 0.0 && new_dx < 0.0) || (dx < 0.0 && new_dx > 0.0) {
                    offset.x = clamped.x;
                }
            }
            if spring_y {
                let new_dy = clamped.y - offset.y;
                if (dy > 0.0 && new_dy < 0.0) || (dy < 0.0 && new_dy > 0.0) {
                    offset.y = clamped.y;
                }
            }

            let mut v = v;
            if spring_x {
                v.x *= SPRING_VELOCITY_DAMPING.powf(frame_ratio);
            }
            if spring_y {
                v.y *= SPRING_VELOCITY_DAMPING.powf(frame_ratio);
            }
            self.pointer_velocity.set(v);

            if spring_x && (offset.x - clamped.x).abs() < SNAP_EPSILON {
                offset.x = clamped.x;
                let mut v = self.pointer_velocity.get();
                v.x = 0.0;
                self.pointer_velocity.set(v);
                // info!("rubber-band snap | x-axis snapped to edge");
            }
            if spring_y && (offset.y - clamped.y).abs() < SNAP_EPSILON {
                offset.y = clamped.y;
                let mut v = self.pointer_velocity.get();
                v.y = 0.0;
                self.pointer_velocity.set(v);
                // info!("rubber-band snap | y-axis snapped to edge");
            }
            if spring_x || spring_y {
                needs_redraw = true;
            }
        } else if !self.scroll_behavior.bouncy {
            offset = clamped;
        }

        (offset, needs_redraw)
    }
}