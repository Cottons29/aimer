use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_events::element::{ElementEvent, KeyAction, NamedKey};
use aimer_utils::AnimInstant;
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, EventElement, LayoutElement, VisitorElement};

use crate::ScrollAxis;
use crate::raw_scroll::{DragMode, RawScrollableContainer};
use crate::scrollable::constants::*;

const DRAG_AXIS_DOMINANCE_RATIO: f32 = 1.2;

fn drag_start_threshold() -> f32 {
    DRAG_START_THRESHOLD_DP
}

fn owns_pointer(active_pointer: Option<u64>, pointer: u64) -> bool {
    active_pointer == Some(pointer)
}

fn pending_content_drag_wins(
    axis: ScrollAxis,
    start: Vec2d,
    current: Vec2d,
    threshold: f32,
) -> bool {
    let dx = current.x - start.x;
    let dy = current.y - start.y;
    match axis {
        ScrollAxis::Vertical => {
            dy.abs() > threshold && dy.abs() > dx.abs() * DRAG_AXIS_DOMINANCE_RATIO
        }
        ScrollAxis::Horizontal => {
            dx.abs() > threshold && dx.abs() > dy.abs() * DRAG_AXIS_DOMINANCE_RATIO
        }
    }
}

fn pointer_drag_delta(
    last: Vec2d,
    current: Vec2d,
    speed_multiplier: f32,
    content_drag_just_won: bool,
) -> Vec2d {
    if content_drag_just_won {
        return Vec2d::default();
    }
    Vec2d { x: (current.x - last.x) * speed_multiplier, y: (current.y - last.y) * speed_multiplier }
}

impl<E: Element> EventElement for RawScrollableContainer<E> {
    fn on_event(&self, event: &ElementEvent) -> bool {
        if let Some(cursor_pos) = event.get_pointer_pos() {
            self.ctrl
                .cursor_pos
                .set(Some(cursor_pos));
        }

        let Some(cursor) = self
            .ctrl
            .cursor_pos
            .get()
        else {
            return false;
        };
        let inside = self
            .bounds
            .is_inside(cursor.x, cursor.y);
        let active_drag = self
            .ctrl
            .drag_mode
            .get()
            != DragMode::None;
        if !inside && !active_drag {
            return false;
        }

        let pos = match event {
            ElementEvent::PointerDown(p, _, _)
            | ElementEvent::PointerUp(p, _, _)
            | ElementEvent::PointerMove(p, _, _)
            | ElementEvent::Scroll { delta: p, .. } => *p,
            ElementEvent::Cancel
            | ElementEvent::PointerExited(_, _)
            | ElementEvent::CharInput { .. }
            | ElementEvent::KeyInput { .. }
            | ElementEvent::ImePreedit { .. } => Vec2d::default(),
        };

        let mode_before = self
            .ctrl
            .drag_mode
            .get();
        let pending_content_drag_won = match event {
            ElementEvent::PointerMove(current, _, id)
                if mode_before == DragMode::Pending
                    && self
                        .ctrl
                        .active_touch_id
                        .get()
                        .is_none_or(|active| active == *id) =>
            {
                self.ctrl
                    .last_pointer_pos
                    .get()
                    .is_some_and(|start| {
                        pending_content_drag_wins(
                            self.ctrl.axis,
                            start,
                            *current,
                            drag_start_threshold(),
                        )
                    })
            }
            _ => false,
        };
        let mut child_consumed = false;

        if matches!(event, ElementEvent::PointerUp(_, _, _) | ElementEvent::Cancel) {
            if let ElementEvent::PointerUp(_, _, pointer) = event
                && self
                    .ctrl
                    .active_touch_id
                    .get()
                    .is_some_and(|active| active != *pointer)
            {
                return false;
            }
            let owned_pointer = match event {
                ElementEvent::PointerUp(_, _, pointer) => owns_pointer(
                    self.ctrl
                        .active_touch_id
                        .get(),
                    *pointer,
                ),
                ElementEvent::Cancel => {
                    self.ctrl
                        .active_touch_id
                        .get()
                        .is_some()
                        || mode_before != DragMode::None
                }
                _ => false,
            };
            if matches!(mode_before, DragMode::VerticalScrollbar | DragMode::HorizontalScrollbar) {
                match event {
                    ElementEvent::PointerUp(_, _, pointer) => {
                        let _ = aimer_widget::cancel_pointer(&self.child, *pointer, pos);
                    }
                    ElementEvent::Cancel => {
                        let _ = aimer_widget::dispatch_event(&self.child, pos, event);
                    }
                    _ => {}
                }
            } else if matches!(mode_before, DragMode::None | DragMode::Pending)
                && matches!(event, ElementEvent::PointerUp(_, _, _))
            {
                let _ = aimer_widget::dispatch_event(&self.child, pos, event);
            }

            let now = AnimInstant::now();
            // info!("[scroll] PointerUp mode_before={:?} drag_mode={:?}", mode_before,
            // self.ctrl.drag_mode.get());
            if let Some(last_time) = self
                .ctrl
                .last_event_time
                .get()
            {
                let elapsed = now
                    .duration_since(last_time)
                    .as_millis();
                if elapsed > VELOCITY_RESET_IDLE_MS {
                    // info!("[scroll] FLING CLEARED — idle too long ({}ms > {}ms threshold)",
                    // elapsed, VELOCITY_RESET_IDLE_MS);
                    self.ctrl
                        .pointer_velocity
                        .set(Vec2d::default());
                    self.ctrl
                        .clear_velocity_history();
                    self.ctrl
                        .cancel_fling();
                } else {
                    let max_v = MAX_SCROLL_VELOCITY
                        * self
                            .ctrl
                            .last_scale
                            .get();
                    let raw = self
                        .ctrl
                        .smoothed_velocity();
                    let sv = Vec2d {
                        x: (raw.x * RELEASE_VELOCITY_GAIN).clamp(-max_v, max_v),
                        y: (raw.y * RELEASE_VELOCITY_GAIN).clamp(-max_v, max_v),
                    };
                    // info!("[scroll] FLING ARMED elapsed={}ms raw=({:.2},{:.2}) gain=({:.2},{:.2})
                    // max_v={:.0}", elapsed, raw.x, raw.y, sv.x, sv.y, max_v);
                    self.ctrl
                        .cancel_fling();
                    self.ctrl
                        .pointer_velocity
                        .set(sv);
                }
            }

            self.ctrl
                .last_frame_time
                .set(Some(now));
            self.ctrl
                .drag_mode
                .set(DragMode::None);
            self.ctrl
                .last_pointer_pos
                .set(None);
            match event {
                ElementEvent::PointerUp(_, _, id) => {
                    if self
                        .ctrl
                        .active_touch_id
                        .get()
                        == Some(*id)
                    {
                        self.ctrl
                            .active_touch_id
                            .set(None);
                    }
                }
                _ => self
                    .ctrl
                    .active_touch_id
                    .set(None),
            }
            aimer_events::window::request_animation_frame();
            return owned_pointer;
        }

        if pending_content_drag_won && let ElementEvent::PointerMove(_, _, pointer) = event {
            let _ = aimer_widget::cancel_pointer(&self.child, *pointer, pos);
        }

        // ── All other events: normal child-first dispatch ──
        if (mode_before == DragMode::None || mode_before == DragMode::Pending)
            && !pending_content_drag_won
        {
            child_consumed = aimer_widget::dispatch_event(&self.child, pos, event);
        }

        let we_consumed = match event {
            ElementEvent::Scroll { delta, .. } => {
                let mut offset = self
                    .ctrl
                    .scroll_offset
                    .get();

                // println!("offset: {:?}", offset);
                let clamped = self
                    .ctrl
                    .clamp_offset(offset);

                let mut scroll_delta = match self.ctrl.axis {
                    ScrollAxis::Vertical => Vec2d { x: 0.0, y: delta.y },
                    ScrollAxis::Horizontal => Vec2d { x: delta.x, y: 0.0 },
                };

                if self
                    .ctrl
                    .scroll_behavior
                    .bouncy
                {
                    // Constant maximum resistance when out-of-bounds.
                    // Content moves at OOB_DRAG_RESISTANCE × finger speed
                    // the instant the boundary is crossed — no gradual ramp.
                    match self.ctrl.axis {
                        ScrollAxis::Vertical => {
                            if offset.y != clamped.y {
                                scroll_delta.y *= OOB_DRAG_RESISTANCE;
                            }
                        }
                        ScrollAxis::Horizontal => {
                            if offset.x != clamped.x {
                                scroll_delta.x *= OOB_DRAG_RESISTANCE;
                            }
                        }
                    }
                }

                // Apply the delta and (for non-bouncy scrollables) clamp to the
                // valid range. The previous version tried to pre-zero the delta
                // by comparing `offset` against `clamp_offset(offset)`, but an
                // in-range offset always equals its own clamp, so every wheel /
                // trackpad delta was discarded and the scrollable could not be
                // scrolled at all. See `ScrollState::apply_wheel_delta`.
                offset = self
                    .ctrl
                    .apply_wheel_delta(offset, scroll_delta);
                self.ctrl
                    .scroll_offset
                    .set(offset);

                let now = AnimInstant::now();
                let dt = self
                    .ctrl
                    .last_event_time
                    .get()
                    .map(|t| {
                        now.duration_since(t)
                            .as_secs_f32()
                    })
                    .unwrap_or(FRAME_REF_120)
                    .max(MIN_EVENT_DT);
                self.ctrl
                    .last_event_time
                    .set(Some(now));

                let frame_ref = FRAME_REF_120;

                let mut target_vx = (scroll_delta.x / dt) * frame_ref;
                let mut target_vy = (scroll_delta.y / dt) * frame_ref;

                if self
                    .ctrl
                    .scroll_behavior
                    .bouncy
                {
                    // Maximum damping when velocity pushes further out-of-bounds.
                    // Applied immediately at the boundary — no gradual ramp.
                    match self.ctrl.axis {
                        ScrollAxis::Vertical => {
                            if (offset.y > clamped.y && scroll_delta.y > 0.0)
                                || (offset.y < clamped.y && scroll_delta.y < 0.0)
                            {
                                target_vy *= OOB_OVERSHOOT_DAMPING;
                            }
                        }
                        ScrollAxis::Horizontal => {
                            if (offset.x > clamped.x && scroll_delta.x > 0.0)
                                || (offset.x < clamped.x && scroll_delta.x < 0.0)
                            {
                                target_vx *= OOB_OVERSHOOT_DAMPING;
                            }
                        }
                    }
                }

                let max_scroll_v = MAX_SCROLL_VELOCITY
                    * self
                        .ctrl
                        .last_scale
                        .get();
                target_vx = target_vx.clamp(-max_scroll_v, max_scroll_v);
                target_vy = target_vy.clamp(-max_scroll_v, max_scroll_v);

                // Smooth velocity across recent samples (tames trackpad jitter).
                self.ctrl
                    .push_velocity(target_vx, target_vy);
                let sv = self
                    .ctrl
                    .smoothed_velocity();
                self.ctrl
                    .pointer_velocity
                    .set(sv);
                // A wheel/trackpad scroll takes over from any release fling and
                // uses the velocity-based momentum, not the bézier curve.
                self.ctrl
                    .cancel_fling();
                // Reset spring velocity so new input dominates over any
                // in-progress spring-back recovery.
                self.ctrl
                    .spring_velocity
                    .set(Vec2d { x: 0.0, y: 0.0 });

                // A wheel/trackpad tick starts (or continues) a scroll session;
                // the draw loop fires the matching `end` once the glide settles.
                self.ctrl
                    .begin_scroll();
                aimer_events::window::request_animation_frame();
                true
            }
            ElementEvent::PointerDown(p, _, id) => {
                if let Some(prev_id) = self
                    .ctrl
                    .active_touch_id
                    .get()
                    && prev_id != *id
                {
                    let stale = self
                        .ctrl
                        .last_event_time
                        .get()
                        .is_none_or(|t| {
                            AnimInstant::now()
                                .duration_since(t)
                                .as_millis()
                                > STALE_TOUCH_THRESHOLD_MS
                        });
                    if stale {
                        // info!("[scroll] DOWN stale touch cleared prev_id={}", prev_id);
                        self.ctrl
                            .active_touch_id
                            .set(None);
                        self.ctrl
                            .drag_mode
                            .set(DragMode::None);
                        self.ctrl
                            .last_pointer_pos
                            .set(None);
                    } else {
                        // info!("[scroll] DOWN REJECTED — secondary finger prev_id={} new_id={}",
                        // prev_id, id);
                        return true;
                    }
                }
                self.ctrl
                    .active_touch_id
                    .set(Some(*id));
                // info!("[scroll] PointerDown id={} pos=({:.1},{:.1})", id, p.x, p.y);

                let mut mode = DragMode::Pending;
                if self
                    .ctrl
                    .hit_test_v_thumb(*p)
                {
                    mode = DragMode::VerticalScrollbar;
                }
                if mode == DragMode::Pending
                    && self
                        .ctrl
                        .hit_test_h_thumb(*p)
                {
                    mode = DragMode::HorizontalScrollbar;
                }

                // Scrollbar track click-to-page: if click is on track but not thumb.
                if mode == DragMode::Pending {
                    let (vp_w, vp_h) = self
                        .ctrl
                        .cached_viewport
                        .get();
                    let v_tw = self
                        .ctrl
                        .cached_v_track_width
                        .get();
                    let h_tw = self
                        .ctrl
                        .cached_h_track_width
                        .get();
                    let friction = self
                        .ctrl
                        .scroll_behavior
                        .friction;
                    // velocity = distance / (frame_ref / (1 − friction)) to scroll exactly
                    // `distance` px.
                    let vel_scale = (1.0 - friction) / FRAME_REF_120;
                    if self
                        .ctrl
                        .hit_test_v_track(*p, vp_w, vp_h, v_tw)
                        && let Some((_x, y, _w, _h)) = self
                            .ctrl
                            .v_thumb_rect
                            .get()
                    {
                        let page = vp_h * KEYBOARD_PAGE_FRACTION;
                        let vy = if p.y < y { page * vel_scale } else { -page * vel_scale };
                        self.ctrl
                            .pointer_velocity
                            .set(Vec2d { x: 0.0, y: vy });
                        self.ctrl
                            .cancel_fling();
                        self.ctrl
                            .drag_mode
                            .set(DragMode::None);
                        self.ctrl
                            .last_pointer_pos
                            .set(Some(*p));
                        self.ctrl
                            .begin_scroll();
                        aimer_events::window::request_animation_frame();
                        return true;
                    }
                    if self
                        .ctrl
                        .hit_test_h_track(*p, vp_w, vp_h, h_tw)
                        && let Some((x, _y, _w, _h)) = self
                            .ctrl
                            .h_thumb_rect
                            .get()
                    {
                        let page = vp_w * KEYBOARD_PAGE_FRACTION;
                        let vx = if p.x < x { page * vel_scale } else { -page * vel_scale };
                        self.ctrl
                            .pointer_velocity
                            .set(Vec2d { x: vx, y: 0.0 });
                        self.ctrl
                            .cancel_fling();
                        self.ctrl
                            .drag_mode
                            .set(DragMode::None);
                        self.ctrl
                            .last_pointer_pos
                            .set(Some(*p));
                        self.ctrl
                            .begin_scroll();
                        aimer_events::window::request_animation_frame();
                        return true;
                    }
                }

                self.ctrl
                    .pointer_velocity
                    .set(Vec2d { x: 0.0, y: 0.0 });
                self.ctrl
                    .clear_velocity_history();
                // Reset the velocity-sampling accumulator so a fresh gesture
                // doesn't inherit stale coalesced delta / timing.
                self.ctrl
                    .vel_accum
                    .set(Vec2d { x: 0.0, y: 0.0 });
                self.ctrl
                    .vel_sample_time
                    .set(None);
                // A fresh touch/click stops the in-flight release fling.
                self.ctrl
                    .cancel_fling();
                self.ctrl
                    .momentum_start_time
                    .set(None);

                self.ctrl
                    .drag_mode
                    .set(mode);
                self.ctrl
                    .last_pointer_pos
                    .set(Some(*p));
                true
            }
            ElementEvent::PointerMove(p, _, id) => {
                // Ignore moves from non-primary fingers.
                if self
                    .ctrl
                    .active_touch_id
                    .get()
                    .is_some()
                    && self
                        .ctrl
                        .active_touch_id
                        .get()
                        != Some(*id)
                {
                    // info!("[scroll] MOVE REJECTED — non-primary finger active={:?} got={}",
                    // self.ctrl.active_touch_id.get(), id);
                    return false;
                }

                let mut mode = self
                    .ctrl
                    .drag_mode
                    .get();
                #[allow(clippy::collapsible_if)]
                if mode == DragMode::Pending {
                    if self
                        .ctrl
                        .last_pointer_pos
                        .get()
                        .is_some()
                    {
                        if pending_content_drag_won {
                            mode = DragMode::Content;
                            self.ctrl
                                .drag_mode
                                .set(DragMode::Content);
                        } else {
                            return child_consumed;
                        }
                    }
                }

                if mode != DragMode::None && mode != DragMode::Pending {
                    // The content (or a scrollbar thumb) is actively being dragged
                    // — the start of a scroll session. Edge-triggered, so repeated
                    // moves within the same drag don't re-fire.
                    self.ctrl
                        .begin_scroll();
                    if let Some(last) = self
                        .ctrl
                        .last_pointer_pos
                        .get()
                    {
                        let speed_multiplier = self
                            .ctrl
                            .speed_multiplier;
                        let delta = pointer_drag_delta(
                            last,
                            *p,
                            speed_multiplier,
                            pending_content_drag_won,
                        );
                        let dx = delta.x;
                        let dy = delta.y;

                        let now = AnimInstant::now();
                        self.ctrl
                            .last_event_time
                            .set(Some(now));
                        if let Some((raw_velocity, sample_dt)) = self
                            .ctrl
                            .accumulate_drag_velocity(dx, dy, now)
                        {
                            let mut new_velocity = match mode {
                                DragMode::Content => match self.ctrl.axis {
                                    ScrollAxis::Vertical => Vec2d { x: 0.0, y: raw_velocity.y },
                                    ScrollAxis::Horizontal => Vec2d { x: raw_velocity.x, y: 0.0 },
                                },
                                _ => Vec2d { x: 0.0, y: 0.0 },
                            };

                            let mut old_velocity = self
                                .ctrl
                                .pointer_velocity
                                .get();
                            let reversed_x = new_velocity.x * old_velocity.x < 0.0;
                            let reversed_y = new_velocity.y * old_velocity.y < 0.0;
                            if reversed_x || reversed_y {
                                self.ctrl
                                    .clear_velocity_history();
                                if reversed_x {
                                    old_velocity.x = 0.0;
                                }
                                if reversed_y {
                                    old_velocity.y = 0.0;
                                }
                            }

                            self.ctrl
                                .push_velocity(new_velocity.x, new_velocity.y);

                            let blend_factor = (sample_dt / DRAG_BLEND_WINDOW).min(1.0);
                            let blend_new =
                                (DRAG_BLEND_BASE * (1.0 - blend_factor) + blend_factor).min(1.0);
                            let blend_old = 1.0 - blend_new;

                            new_velocity.x =
                                old_velocity.x * blend_old + new_velocity.x * blend_new;
                            new_velocity.y =
                                old_velocity.y * blend_old + new_velocity.y * blend_new;

                            self.ctrl
                                .pointer_velocity
                                .set(new_velocity);
                        }

                        let mut offset = self
                            .ctrl
                            .scroll_offset
                            .get();
                        let clamped = self
                            .ctrl
                            .clamp_offset(offset);

                        match mode {
                            DragMode::Content => {
                                // Constant maximum resistance when out-of-bounds.
                                // Applied immediately — no gradual ramp.
                                match self.ctrl.axis {
                                    ScrollAxis::Vertical => {
                                        let mut actual_dy = dy;
                                        if offset.y != clamped.y {
                                            actual_dy *= OOB_DRAG_RESISTANCE;
                                        }
                                        offset.y += actual_dy;
                                    }
                                    ScrollAxis::Horizontal => {
                                        let mut actual_dx = dx;
                                        if offset.x != clamped.x {
                                            actual_dx *= OOB_DRAG_RESISTANCE;
                                        }
                                        offset.x += actual_dx;
                                    }
                                }
                            }
                            DragMode::VerticalScrollbar => {
                                let target_y = offset.y
                                    - dy * self
                                        .ctrl
                                        .v_scroll_multiplier
                                        .get();
                                offset.y = offset.y * SCROLLBAR_DRAG_SMOOTH_OLD
                                    + target_y * SCROLLBAR_DRAG_SMOOTH_NEW;
                            }
                            DragMode::HorizontalScrollbar => {
                                let target_x = offset.x
                                    - dx * self
                                        .ctrl
                                        .h_scroll_multiplier
                                        .get();
                                offset.x = offset.x * SCROLLBAR_DRAG_SMOOTH_OLD
                                    + target_x * SCROLLBAR_DRAG_SMOOTH_NEW;
                            }
                            _ => {}
                        }

                        if !self
                            .ctrl
                            .scroll_behavior
                            .bouncy
                        {
                            offset = self
                                .ctrl
                                .clamp_offset(offset);
                        }
                        self.ctrl
                            .scroll_offset
                            .set(offset);
                    }
                    self.ctrl
                        .last_pointer_pos
                        .set(Some(*p));
                    aimer_events::window::request_animation_frame();
                    return true;
                }
                false
            }
            ElementEvent::KeyInput { key, action: KeyAction::Pressed, .. } => {
                if child_consumed {
                    return true;
                }
                let scale = self
                    .ctrl
                    .last_scale
                    .get();
                let (vp_w, vp_h) = self
                    .ctrl
                    .cached_viewport
                    .get();
                let line = KEYBOARD_SCROLL_STEP * scale;
                let page_v = vp_h * KEYBOARD_PAGE_FRACTION;
                let page_h = vp_w * KEYBOARD_PAGE_FRACTION;

                let scroll = match (&self.ctrl.axis, key) {
                    (ScrollAxis::Vertical, NamedKey::ArrowUp) => Some(Vec2d { x: 0.0, y: line }),
                    (ScrollAxis::Vertical, NamedKey::ArrowDown) => Some(Vec2d { x: 0.0, y: -line }),
                    (ScrollAxis::Vertical, NamedKey::PageUp) => Some(Vec2d { x: 0.0, y: page_v }),
                    (ScrollAxis::Vertical, NamedKey::PageDown) => {
                        Some(Vec2d { x: 0.0, y: -page_v })
                    }
                    (ScrollAxis::Vertical, NamedKey::Home) => {
                        // Scroll to top: offset.y should be 0 (min_scroll).
                        let off = self
                            .ctrl
                            .scroll_offset
                            .get();
                        Some(Vec2d { x: 0.0, y: -off.y })
                    }
                    (ScrollAxis::Vertical, NamedKey::End) => {
                        // Scroll to bottom: offset.y should be -max_scroll.y.
                        let off = self
                            .ctrl
                            .scroll_offset
                            .get();
                        let max = self
                            .ctrl
                            .cached_max_scroll
                            .get();
                        Some(Vec2d { x: 0.0, y: -max.y - off.y })
                    }
                    (ScrollAxis::Horizontal, NamedKey::ArrowLeft) => {
                        Some(Vec2d { x: line, y: 0.0 })
                    }
                    (ScrollAxis::Horizontal, NamedKey::ArrowRight) => {
                        Some(Vec2d { x: -line, y: 0.0 })
                    }
                    (ScrollAxis::Horizontal, NamedKey::PageUp) => Some(Vec2d { x: page_h, y: 0.0 }),
                    (ScrollAxis::Horizontal, NamedKey::PageDown) => {
                        Some(Vec2d { x: -page_h, y: 0.0 })
                    }
                    (ScrollAxis::Horizontal, NamedKey::Home) => {
                        let off = self
                            .ctrl
                            .scroll_offset
                            .get();
                        Some(Vec2d { x: -off.x, y: 0.0 })
                    }
                    (ScrollAxis::Horizontal, NamedKey::End) => {
                        let off = self
                            .ctrl
                            .scroll_offset
                            .get();
                        let max = self
                            .ctrl
                            .cached_max_scroll
                            .get();
                        Some(Vec2d { x: -max.x - off.x, y: 0.0 })
                    }
                    _ => None,
                };

                if let Some(delta) = scroll {
                    let mut offset = self
                        .ctrl
                        .scroll_offset
                        .get();
                    offset.x += delta.x;
                    offset.y += delta.y;
                    if !self
                        .ctrl
                        .scroll_behavior
                        .bouncy
                    {
                        offset = self
                            .ctrl
                            .clamp_offset(offset);
                    }
                    self.ctrl
                        .scroll_offset
                        .set(offset);
                    self.ctrl
                        .pointer_velocity
                        .set(Vec2d { x: 0.0, y: 0.0 });
                    self.ctrl
                        .clear_velocity_history();
                    self.ctrl
                        .cancel_fling();
                    // A keyboard scroll is a discrete, self-contained session: it
                    // moves the offset with no residual momentum, so the draw loop
                    // reports it settled (and fires `end`) on the next frame.
                    self.ctrl
                        .begin_scroll();
                    aimer_events::window::request_animation_frame();
                    true
                } else {
                    false
                }
            }
            // PointerUp/Cancel is handled early above (before child dispatch),
            // so it never reaches this match.
            ElementEvent::PointerUp(_, _, id) => {
                // Release primary-finger lock.
                if self
                    .ctrl
                    .active_touch_id
                    .get()
                    == Some(*id)
                {
                    self.ctrl
                        .active_touch_id
                        .set(None);
                }
                false
            }
            ElementEvent::Cancel => {
                self.ctrl
                    .active_touch_id
                    .set(None);
                false
            }
            ElementEvent::PointerExited(_, _)
            | ElementEvent::CharInput { .. }
            | ElementEvent::KeyInput { .. }
            | ElementEvent::ImePreedit { .. } => child_consumed,
        };

        child_consumed || we_consumed
    }

    fn event_children<'a>(&'a self, _: &mut dyn FnMut(&'a dyn Element)) {}
}

impl<E: Element> VisitorElement for RawScrollableContainer<E> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.child);
    }

    fn debug_name(&self) -> &'static str {
        "RawScrollableContainer"
    }
}

impl<E: Element> LayoutElement for RawScrollableContainer<E> {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        ResolvedSize {
            width: ctx
                .box_constraint
                .max_width,
            height: ctx
                .box_constraint
                .max_height,
        }
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let mut child_ctx = ctx.clone();
        match self.ctrl.axis {
            ScrollAxis::Vertical => {
                child_ctx
                    .box_constraint
                    .max_height = f32::MAX
            }
            ScrollAxis::Horizontal => {
                child_ctx
                    .box_constraint
                    .max_width = f32::MAX
            }
        }
        let res = self
            .child
            .computed_size(&child_ctx);
        // println!("Content Computed Size: {:?}", res);
        res
    }
}

#[cfg(test)]
mod tests {
    use super::{
        drag_start_threshold, owns_pointer, pending_content_drag_wins, pointer_drag_delta,
    };
    use crate::ScrollAxis;
    use aimer_attribute::Vec2d;

    #[test]
    fn move_exactly_at_drag_threshold_remains_pending() {
        assert!(!pending_content_drag_wins(
            ScrollAxis::Vertical,
            Vec2d::default(),
            Vec2d { x: 0.0, y: 10.0 },
            10.0,
        ));
    }

    #[test]
    fn drag_threshold_stays_in_logical_pixels_at_high_display_scale() {
        assert_eq!(drag_start_threshold(), 10.0);
    }

    #[test]
    fn scrollable_only_owns_its_active_pointer() {
        assert!(owns_pointer(Some(7), 7));
        assert!(!owns_pointer(Some(7), 8));
        assert!(!owns_pointer(None, 7));
    }

    #[test]
    fn axis_dominant_move_above_threshold_wins_scrolling() {
        assert!(pending_content_drag_wins(
            ScrollAxis::Vertical,
            Vec2d::default(),
            Vec2d { x: 2.0, y: 10.01 },
            10.0,
        ));
        assert!(pending_content_drag_wins(
            ScrollAxis::Horizontal,
            Vec2d::default(),
            Vec2d { x: -10.01, y: 2.0 },
            10.0,
        ));
    }

    #[test]
    fn equal_diagonal_move_does_not_win_scrolling() {
        assert!(!pending_content_drag_wins(
            ScrollAxis::Vertical,
            Vec2d::default(),
            Vec2d { x: 12.0, y: 12.0 },
            10.0,
        ));
    }

    #[test]
    fn near_diagonal_text_selection_does_not_win_scrolling() {
        assert!(!pending_content_drag_wins(
            ScrollAxis::Vertical,
            Vec2d::default(),
            Vec2d { x: 149.0, y: 149.5 },
            10.0,
        ));
        assert!(!pending_content_drag_wins(
            ScrollAxis::Horizontal,
            Vec2d::default(),
            Vec2d { x: 149.5, y: 149.0 },
            10.0,
        ));
    }

    #[test]
    fn cross_axis_dominant_move_does_not_win_scrolling() {
        assert!(!pending_content_drag_wins(
            ScrollAxis::Vertical,
            Vec2d::default(),
            Vec2d { x: 15.0, y: 12.0 },
            10.0,
        ));
    }

    #[test]
    fn winning_move_establishes_scroll_origin_without_changing_offset() {
        let delta =
            pointer_drag_delta(Vec2d { x: 20.0, y: 110.0 }, Vec2d { x: 20.0, y: 150.0 }, 1.0, true);

        assert_eq!(delta.x, 0.0);
        assert_eq!(delta.y, 0.0);
    }
}
