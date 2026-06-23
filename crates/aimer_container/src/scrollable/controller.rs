use crate::scrollable::constants::*;
use crate::scrollable::scroll_behavior::ScrollBehavior;
use crate::scrollable::ScrollAxis;
use aimer_attribute::position::Vec2d;
use std::cell::Cell;
use web_time::Instant;

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
        if value < min {
            let diff = min - value;
            min - diff.powf(BOUNCY_STRETCH_EXPONENT) * (resistance * BOUNCY_RESISTANCE_SCALE)
        } else if value > max {
            let diff = value - max;
            max + diff.powf(BOUNCY_STRETCH_EXPONENT) * (resistance * BOUNCY_RESISTANCE_SCALE)
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

            (
                Self::apply_bouncy(offset.x, max_x, min_x, resistance),
                Self::apply_bouncy(offset.y, max_y, min_y, resistance),
            )
                .into()
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

        if velocity.x.abs() > VELOCITY_EPSILON || velocity.y.abs() > VELOCITY_EPSILON {
            #[cfg(target_os = "ios")]
            let oob_damping_base: f32 = OOB_DAMPING_BASE_IOS;
            #[cfg(not(target_os = "ios"))]
            let oob_damping_base: f32 = OOB_DAMPING_BASE_DEFAULT;
            if offset.x != clamped.x {
                let damping = oob_damping_base.powf(frame_ratio);
                velocity.x *= damping;
                if (offset.x > clamped.x && velocity.x > 0.0) || (offset.x < clamped.x && velocity.x < 0.0) {
                    velocity.x *= OOB_OVERSHOOT_DAMPING;
                }
            }
            if offset.y != clamped.y {
                let damping = oob_damping_base.powf(frame_ratio);
                velocity.y *= damping;
                if (offset.y > clamped.y && velocity.y > 0.0) || (offset.y < clamped.y && velocity.y < 0.0) {
                    velocity.y *= OOB_OVERSHOOT_DAMPING;
                }
            }

            offset.x += velocity.x * frame_ratio;
            offset.y += velocity.y * frame_ratio;
            let friction_factor = self.scroll_behavior.friction.powf(frame_ratio);
            velocity.x *= friction_factor;
            velocity.y *= friction_factor;
            self.pointer_velocity.set(velocity);
            needs_redraw = true;
        } else if velocity.x != 0.0 || velocity.y != 0.0 {
            self.pointer_velocity.set(Vec2d { x: 0.0, y: 0.0 });
        }

        // Spring back if bouncy is enabled.
        // Skip spring-back on axes where the user is actively scrolling in
        // the same direction — otherwise the two forces fight and cause shake.
        if self.scroll_behavior.bouncy && (offset.x != clamped.x || offset.y != clamped.y) {
            let dx = clamped.x - offset.x;
            let dy = clamped.y - offset.y;

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
            }
            if spring_y && (offset.y - clamped.y).abs() < SNAP_EPSILON {
                offset.y = clamped.y;
                let mut v = self.pointer_velocity.get();
                v.y = 0.0;
                self.pointer_velocity.set(v);
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