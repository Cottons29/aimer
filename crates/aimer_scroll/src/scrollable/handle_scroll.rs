use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_events::element::{ElementEvent, KeyAction, NamedKey};
use aimer_events::pointer::PointerSource;
use aimer_utils::AnimInstant;
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, EventElement, EventResult, LayoutElement, PointerKey, VisitorElement};

use crate::ScrollAxis;
use crate::raw_scroll::{DragMode, RawScrollableContainer};
use crate::scrollable::constants::*;
use crate::scrollable::overscroll_source::OverscrollSource;
use crate::scrollable::scroll_frame::apply_scroll_frame;

const DRAG_AXIS_DOMINANCE_RATIO: f32 = 1.2;

/// The overscroll source a pointer gesture belongs to.
///
/// A finger and a mouse button drive the very same drag code but are not the
/// same device to a target that only trusts some of them with a rubber band,
/// so the distinction is kept all the way into the scroll engine.
#[inline]
fn drag_overscroll_source(source: PointerSource) -> OverscrollSource {
    match source {
        PointerSource::Touch => OverscrollSource::Touch,
        PointerSource::Mouse => OverscrollSource::Mouse,
    }
}

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
    Vec2d {
        x: (current.x - last.x) * speed_multiplier,
        y: (current.y - last.y) * speed_multiplier,
    }
}

/// Keep only the component of `velocity` the container can actually scroll.
///
/// A finger never travels along a single axis, and letting the cross-axis
/// component through would seed a fling the viewport cannot show.
#[inline]
fn axis_velocity(axis: ScrollAxis, velocity: Vec2d) -> Vec2d {
    match axis {
        ScrollAxis::Vertical => Vec2d {
            x: 0.0,
            y: velocity.y,
        },
        ScrollAxis::Horizontal => Vec2d {
            x: velocity.x,
            y: 0.0,
        },
    }
}

fn child_dispatch_position(event: &ElementEvent, cursor: Vec2d) -> Vec2d {
    event.get_pointer_pos().unwrap_or(cursor)
}

fn event_pointer_key(event: &ElementEvent) -> Option<PointerKey> {
    match event {
        ElementEvent::PointerDown(_, source, id)
        | ElementEvent::PointerUp(_, source, id)
        | ElementEvent::PointerMove(_, source, id)
        | ElementEvent::PointerExited(source, id) => Some(PointerKey::new(*source, *id)),
        _ => None,
    }
}

fn child_route_allowed(inside: bool, active_drag: bool, child_captured: bool) -> bool {
    inside || active_drag || child_captured
}

fn dispatch_child_event<E: Element>(
    scrollable: &RawScrollableContainer<E>,
    pos: Vec2d,
    event: &ElementEvent,
) -> EventResult {
    let pointer = event_pointer_key(event);
    let was_captured =
        pointer.is_some_and(|pointer| scrollable.event_dispatcher.borrow().is_captured(pointer));
    let result = scrollable
        .event_dispatcher
        .borrow_mut()
        .dispatch(&scrollable.child, pos, event);
    let is_captured =
        pointer.is_some_and(|pointer| scrollable.event_dispatcher.borrow().is_captured(pointer));
    match (pointer, was_captured, is_captured) {
        (Some(pointer), false, true) => result.with_pointer_capture(pointer),
        (Some(pointer), true, false) => result.with_pointer_release(pointer),
        _ => result,
    }
}

impl<E: Element> EventElement for RawScrollableContainer<E> {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        if let Some(cursor_pos) = event.get_pointer_pos() {
            self.ctrl.cursor_pos.set(Some(cursor_pos));
        }

        let cursor = match self.ctrl.cursor_pos.get() {
            Some(cursor) => cursor,
            None if matches!(event, ElementEvent::Cancel) => Vec2d::default(),
            None => return EventResult::ignored(),
        };
        let inside = self.bounds.is_inside(cursor.x, cursor.y);
        let active_drag = self.ctrl.drag_mode.get() != DragMode::None;
        let child_captured = event_pointer_key(event)
            .is_some_and(|pointer| self.event_dispatcher.borrow().is_captured(pointer))
            || (matches!(event, ElementEvent::Cancel)
                && self.event_dispatcher.borrow().capture_count() > 0);
        if !child_route_allowed(inside, active_drag, child_captured) {
            return EventResult::ignored();
        }

        let pos = child_dispatch_position(event, cursor);

        let mode_before = self.ctrl.drag_mode.get();
        let pending_content_drag_won = match event {
            ElementEvent::PointerMove(current, _, id)
                if mode_before == DragMode::Pending
                    && self
                        .ctrl
                        .active_touch_id
                        .get()
                        .is_none_or(|active| active == *id) =>
            {
                self.ctrl.last_pointer_pos.get().is_some_and(|start| {
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
        let mut child_result = EventResult::ignored();

        if matches!(
            event,
            ElementEvent::PointerUp(_, _, _) | ElementEvent::Cancel
        ) {
            if let ElementEvent::PointerUp(_, _, pointer) = event
                && self
                    .ctrl
                    .active_touch_id
                    .get()
                    .is_some_and(|active| active != *pointer)
            {
                return EventResult::ignored();
            }
            let owned_pointer = match event {
                ElementEvent::PointerUp(_, _, pointer) => {
                    owns_pointer(self.ctrl.active_touch_id.get(), *pointer)
                }
                ElementEvent::Cancel => {
                    self.ctrl.active_touch_id.get().is_some() || mode_before != DragMode::None
                }
                _ => false,
            };
            if matches!(
                mode_before,
                DragMode::VerticalScrollbar | DragMode::HorizontalScrollbar
            ) {
                match event {
                    ElementEvent::PointerUp(_, source, pointer) => {
                        child_result = child_result.merge(
                            self.event_dispatcher
                                .borrow_mut()
                                .cancel_pointer(&self.child, PointerKey::new(*source, *pointer)),
                        );
                    }
                    ElementEvent::Cancel => {
                        child_result = child_result.merge(dispatch_child_event(self, pos, event));
                    }
                    _ => {}
                }
            } else if matches!(mode_before, DragMode::None | DragMode::Pending)
                && matches!(event, ElementEvent::PointerUp(_, _, _))
            {
                child_result = child_result.merge(dispatch_child_event(self, pos, event));
            }

            let now = AnimInstant::now();
            // info!("[scroll] PointerUp mode_before={:?} drag_mode={:?}", mode_before,
            // self.ctrl.drag_mode.get());
            if let Some(last_time) = self.ctrl.last_event_time.get() {
                let elapsed = now.duration_since(last_time).as_millis();
                if elapsed > VELOCITY_RESET_IDLE_MS {
                    // info!("[scroll] FLING CLEARED — idle too long ({}ms > {}ms threshold)",
                    // elapsed, VELOCITY_RESET_IDLE_MS);
                    self.ctrl.pointer_velocity.set(Vec2d::default());
                    self.ctrl.clear_velocity_history();
                    self.ctrl.cancel_fling();
                } else {
                    // The gesture ended between two samples, so whatever the
                    // finger did since the last one has not been measured yet
                    // — including doing nothing at all, which is precisely how
                    // a drag that was deliberately brought to a stop before
                    // the lift looks. Closing that slice is what keeps such a
                    // release from inheriting the speed of the swing that
                    // preceded it.
                    if mode_before == DragMode::Content
                        && let Some((velocity, sample_dt)) = self.ctrl.flush_drag_velocity(now)
                    {
                        self.ctrl.push_velocity(
                            axis_velocity(self.ctrl.axis, velocity),
                            sample_dt,
                            now,
                        );
                    }
                    let max_v = MAX_SCROLL_VELOCITY * self.ctrl.last_scale.get();
                    let raw = self.ctrl.smoothed_velocity(now);
                    let sv = Vec2d {
                        x: (raw.x * RELEASE_VELOCITY_GAIN).clamp(-max_v, max_v),
                        y: (raw.y * RELEASE_VELOCITY_GAIN).clamp(-max_v, max_v),
                    };
                    // info!("[scroll] FLING ARMED elapsed={}ms raw=({:.2},{:.2}) gain=({:.2},{:.2})
                    // max_v={:.0}", elapsed, raw.x, raw.y, sv.x, sv.y, max_v);
                    self.ctrl.cancel_fling();
                    self.ctrl.pointer_velocity.set(sv);
                }
            }

            self.ctrl.last_frame_time.set(Some(now));
            self.ctrl.drag_mode.set(DragMode::None);
            self.ctrl.last_pointer_pos.set(None);
            // A pointer interaction owns the edge from here on, so a wheel or
            // trackpad gesture can no longer hold the stretch: without this a
            // contact hold whose lift was never reported would freeze it.
            self.ctrl.release_overscroll_recovery();
            self.ctrl.begin_device_contact(false);
            match event {
                ElementEvent::PointerUp(_, _, id) => {
                    if self.ctrl.active_touch_id.get() == Some(*id) {
                        self.ctrl.active_touch_id.set(None);
                    }
                }
                _ => self.ctrl.active_touch_id.set(None),
            }
            aimer_events::window::request_animation_frame();
            let result = child_result.merge(EventResult::from(owned_pointer).with_redraw());
            return match event {
                ElementEvent::PointerUp(_, source, pointer) => {
                    result.with_pointer_release(PointerKey::new(*source, *pointer))
                }
                _ => result,
            };
        }

        if pending_content_drag_won && let ElementEvent::PointerMove(_, source, pointer) = event {
            child_result = child_result.merge(
                self.event_dispatcher
                    .borrow_mut()
                    .cancel_pointer(&self.child, PointerKey::new(*source, *pointer)),
            );
        }

        // ── All other events: normal child-first dispatch ──
        if (mode_before == DragMode::None || mode_before == DragMode::Pending)
            && !pending_content_drag_won
        {
            child_result = child_result.merge(dispatch_child_event(self, pos, event));
        }

        let we_consumed = match event {
            ElementEvent::Scroll {
                delta,
                kind,
                phase,
                is_direct_manipulation,
            } => {
                if apply_scroll_frame(
                    &self.ctrl,
                    *delta,
                    *kind,
                    *phase,
                    *is_direct_manipulation,
                ) {
                    self.ctrl.begin_scroll();
                    aimer_events::window::request_animation_frame();
                }
                true
            }
            ElementEvent::PointerDown(p, source, id) => {
                if let Some(prev_id) = self.ctrl.active_touch_id.get()
                    && prev_id != *id
                {
                    let stale = self.ctrl.last_event_time.get().is_none_or(|t| {
                        AnimInstant::now().duration_since(t).as_millis() > STALE_TOUCH_THRESHOLD_MS
                    });
                    if stale {
                        // info!("[scroll] DOWN stale touch cleared prev_id={}", prev_id);
                        self.ctrl.active_touch_id.set(None);
                        self.ctrl.drag_mode.set(DragMode::None);
                        self.ctrl.last_pointer_pos.set(None);
                    } else {
                        // info!("[scroll] DOWN REJECTED — secondary finger prev_id={} new_id={}",
                        // prev_id, id);
                        return child_result.merge(EventResult::consumed());
                    }
                }
                self.ctrl.active_touch_id.set(Some(*id));
                // A touch/mouse interaction takes over from any wheel/trackpad
                // gesture, so it must not inherit that gesture's recovery state
                // nor be judged by the device that produced it.
                self.ctrl
                    .set_overscroll_source(drag_overscroll_source(*source));
                self.ctrl.release_overscroll_recovery();
                self.ctrl.begin_device_contact(false);
                self.ctrl.reset_overscroll_peak();
                // info!("[scroll] PointerDown id={} pos=({:.1},{:.1})", id, p.x, p.y);

                let mut mode = DragMode::Pending;
                if self.ctrl.hit_test_v_thumb(*p) {
                    mode = DragMode::VerticalScrollbar;
                }
                if mode == DragMode::Pending && self.ctrl.hit_test_h_thumb(*p) {
                    mode = DragMode::HorizontalScrollbar;
                }

                // Scrollbar track click-to-page: if click is on track but not thumb.
                if mode == DragMode::Pending {
                    let (vp_w, vp_h) = self.ctrl.cached_viewport.get();
                    let v_tw = self.ctrl.cached_v_track_width.get();
                    let h_tw = self.ctrl.cached_h_track_width.get();
                    let friction = self.ctrl.scroll_behavior.friction;
                    // velocity = distance / (frame_ref / (1 − friction)) to scroll exactly
                    // `distance` px.
                    let vel_scale = (1.0 - friction) / FRAME_REF_120;
                    if self.ctrl.hit_test_v_track(*p, vp_w, vp_h, v_tw)
                        && let Some((_x, y, _w, _h)) = self.ctrl.v_thumb_rect.get()
                    {
                        let page = vp_h * KEYBOARD_PAGE_FRACTION;
                        let vy = if p.y < y {
                            page * vel_scale
                        } else {
                            -page * vel_scale
                        };
                        self.ctrl.pointer_velocity.set(Vec2d { x: 0.0, y: vy });
                        self.ctrl.cancel_fling();
                        self.ctrl.drag_mode.set(DragMode::None);
                        self.ctrl.last_pointer_pos.set(Some(*p));
                        self.ctrl.begin_scroll();
                        aimer_events::window::request_animation_frame();
                        return child_result.merge(EventResult::consumed().with_redraw());
                    }
                    if self.ctrl.hit_test_h_track(*p, vp_w, vp_h, h_tw)
                        && let Some((x, _y, _w, _h)) = self.ctrl.h_thumb_rect.get()
                    {
                        let page = vp_w * KEYBOARD_PAGE_FRACTION;
                        let vx = if p.x < x {
                            page * vel_scale
                        } else {
                            -page * vel_scale
                        };
                        self.ctrl.pointer_velocity.set(Vec2d { x: vx, y: 0.0 });
                        self.ctrl.cancel_fling();
                        self.ctrl.drag_mode.set(DragMode::None);
                        self.ctrl.last_pointer_pos.set(Some(*p));
                        self.ctrl.begin_scroll();
                        aimer_events::window::request_animation_frame();
                        return child_result.merge(EventResult::consumed().with_redraw());
                    }
                }

                self.ctrl.pointer_velocity.set(Vec2d { x: 0.0, y: 0.0 });
                self.ctrl.clear_velocity_history();
                // Reset the velocity-sampling accumulator so a fresh gesture
                // doesn't inherit stale coalesced delta / timing.
                self.ctrl.vel_accum.set(Vec2d { x: 0.0, y: 0.0 });
                self.ctrl.vel_sample_time.set(None);
                // A fresh touch/click stops the in-flight release fling.
                self.ctrl.cancel_fling();
                self.ctrl.momentum_start_time.set(None);

                self.ctrl.drag_mode.set(mode);
                self.ctrl.last_pointer_pos.set(Some(*p));
                true
            }
            ElementEvent::PointerMove(p, _, id) => {
                // Ignore moves from non-primary fingers.
                if self.ctrl.active_touch_id.get().is_some()
                    && self.ctrl.active_touch_id.get() != Some(*id)
                {
                    // info!("[scroll] MOVE REJECTED — non-primary finger active={:?} got={}",
                    // self.ctrl.active_touch_id.get(), id);
                    return child_result;
                }

                let mut mode = self.ctrl.drag_mode.get();
                #[allow(clippy::collapsible_if)]
                if mode == DragMode::Pending {
                    if self.ctrl.last_pointer_pos.get().is_some() {
                        if pending_content_drag_won {
                            mode = DragMode::Content;
                            self.ctrl.drag_mode.set(DragMode::Content);
                        } else {
                            return child_result;
                        }
                    }
                }

                if mode != DragMode::None && mode != DragMode::Pending {
                    // The content (or a scrollbar thumb) is actively being dragged
                    // — the start of a scroll session. Edge-triggered, so repeated
                    // moves within the same drag don't re-fire.
                    self.ctrl.begin_scroll();
                    if let Some(last) = self.ctrl.last_pointer_pos.get() {
                        let speed_multiplier = self.ctrl.speed_multiplier;
                        let delta = pointer_drag_delta(
                            last,
                            *p,
                            speed_multiplier,
                            pending_content_drag_won,
                        );
                        let dx = delta.x;
                        let dy = delta.y;

                        let now = AnimInstant::now();
                        self.ctrl.last_event_time.set(Some(now));
                        if let Some((raw_velocity, sample_dt)) =
                            self.ctrl.accumulate_drag_velocity(dx, dy, now)
                        {
                            let mut new_velocity = match mode {
                                DragMode::Content => axis_velocity(self.ctrl.axis, raw_velocity),
                                _ => Vec2d { x: 0.0, y: 0.0 },
                            };

                            let mut old_velocity = self.ctrl.pointer_velocity.get();
                            let reversed_x = new_velocity.x * old_velocity.x < 0.0;
                            let reversed_y = new_velocity.y * old_velocity.y < 0.0;
                            if reversed_x || reversed_y {
                                self.ctrl.clear_velocity_history();
                                if reversed_x {
                                    old_velocity.x = 0.0;
                                }
                                if reversed_y {
                                    old_velocity.y = 0.0;
                                }
                            }

                            self.ctrl.push_velocity(new_velocity, sample_dt, now);

                            let blend_factor = (sample_dt / DRAG_BLEND_WINDOW).min(1.0);
                            let blend_new =
                                (DRAG_BLEND_BASE * (1.0 - blend_factor) + blend_factor).min(1.0);
                            let blend_old = 1.0 - blend_new;

                            new_velocity.x =
                                old_velocity.x * blend_old + new_velocity.x * blend_new;
                            new_velocity.y =
                                old_velocity.y * blend_old + new_velocity.y * blend_new;

                            self.ctrl.pointer_velocity.set(new_velocity);
                        }

                        let mut offset = self.ctrl.scroll_offset.get();

                        match mode {
                            DragMode::Content => {
                                // Same rubber band the wheel / trackpad path
                                // uses: resistance grows with the stretch, so a
                                // finger held past the edge stops pulling the
                                // content further out.
                                let step = self.ctrl.resisted_overscroll_delta(match self.ctrl.axis
                                {
                                    ScrollAxis::Vertical => Vec2d { x: 0.0, y: dy },
                                    ScrollAxis::Horizontal => Vec2d { x: dx, y: 0.0 },
                                });
                                offset.x += step.x;
                                offset.y += step.y;
                            }
                            DragMode::VerticalScrollbar => {
                                let target_y = offset.y - dy * self.ctrl.v_scroll_multiplier.get();
                                offset.y = offset.y * SCROLLBAR_DRAG_SMOOTH_OLD
                                    + target_y * SCROLLBAR_DRAG_SMOOTH_NEW;
                            }
                            DragMode::HorizontalScrollbar => {
                                let target_x = offset.x - dx * self.ctrl.h_scroll_multiplier.get();
                                offset.x = offset.x * SCROLLBAR_DRAG_SMOOTH_OLD
                                    + target_x * SCROLLBAR_DRAG_SMOOTH_NEW;
                            }
                            _ => {}
                        }

                        if !self.ctrl.bouncy() {
                            offset = self.ctrl.clamp_offset(offset);
                        }
                        self.ctrl.scroll_offset.set(offset);
                    }
                    self.ctrl.last_pointer_pos.set(Some(*p));
                    aimer_events::window::request_animation_frame();
                    return child_result.merge(EventResult::consumed().with_redraw());
                }
                false
            }
            ElementEvent::KeyInput {
                key,
                action: KeyAction::Pressed,
                ..
            } => {
                if child_result.is_consumed() {
                    return child_result;
                }
                let scale = self.ctrl.last_scale.get();
                let (vp_w, vp_h) = self.ctrl.cached_viewport.get();
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
                        let off = self.ctrl.scroll_offset.get();
                        Some(Vec2d { x: 0.0, y: -off.y })
                    }
                    (ScrollAxis::Vertical, NamedKey::End) => {
                        // Scroll to bottom: offset.y should be -max_scroll.y.
                        let off = self.ctrl.scroll_offset.get();
                        let max = self.ctrl.cached_max_scroll.get();
                        Some(Vec2d {
                            x: 0.0,
                            y: -max.y - off.y,
                        })
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
                        let off = self.ctrl.scroll_offset.get();
                        Some(Vec2d { x: -off.x, y: 0.0 })
                    }
                    (ScrollAxis::Horizontal, NamedKey::End) => {
                        let off = self.ctrl.scroll_offset.get();
                        let max = self.ctrl.cached_max_scroll.get();
                        Some(Vec2d {
                            x: -max.x - off.x,
                            y: 0.0,
                        })
                    }
                    _ => None,
                };

                if let Some(delta) = scroll {
                    self.ctrl.set_overscroll_source(OverscrollSource::Keyboard);
                    let mut offset = self.ctrl.scroll_offset.get();
                    offset.x += delta.x;
                    offset.y += delta.y;
                    if !self.ctrl.bouncy() {
                        offset = self.ctrl.clamp_offset(offset);
                    }
                    self.ctrl.scroll_offset.set(offset);
                    self.ctrl.pointer_velocity.set(Vec2d { x: 0.0, y: 0.0 });
                    self.ctrl.clear_velocity_history();
                    self.ctrl.cancel_fling();
                    // A keyboard scroll is a discrete, self-contained session: it
                    // moves the offset with no residual momentum, so the draw loop
                    // reports it settled (and fires `end`) on the next frame.
                    self.ctrl.begin_scroll();
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
                if self.ctrl.active_touch_id.get() == Some(*id) {
                    self.ctrl.active_touch_id.set(None);
                }
                false
            }
            ElementEvent::Cancel => {
                self.ctrl.active_touch_id.set(None);
                false
            }
            ElementEvent::PointerExited(_, _)
            | ElementEvent::CharInput { .. }
            | ElementEvent::KeyInput { .. }
            | ElementEvent::ImePreedit { .. } => false,

            _ => false,
        };

        let result = child_result.merge(EventResult::from(we_consumed));
        match event {
            ElementEvent::PointerDown(_, source, pointer) if we_consumed => {
                result.with_pointer_capture(PointerKey::new(*source, *pointer))
            }
            ElementEvent::PointerMove(_, source, pointer) if pending_content_drag_won => {
                result.with_pointer_capture(PointerKey::new(*source, *pointer))
            }
            _ => result,
        }
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
        match self.ctrl.axis {
            ScrollAxis::Vertical => ResolvedSize {
                width: ctx.box_constraint.max_width,
                height: ctx.box_constraint.max_height,
            },
            ScrollAxis::Horizontal => ResolvedSize {
                width: ctx.box_constraint.max_width,
                height: self
                    .content_size(ctx)
                    .height
                    .clamp(ctx.box_constraint.min_height, ctx.box_constraint.max_height),
            },
        }
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let mut child_ctx = ctx.clone();
        match self.ctrl.axis {
            ScrollAxis::Vertical => child_ctx.box_constraint.max_height = f32::MAX,
            ScrollAxis::Horizontal => child_ctx.box_constraint.max_width = f32::MAX,
        }
        let res = self.child.computed_size(&child_ctx);
        // println!("Content Computed Size: {:?}", res);
        res
    }
}

#[cfg(test)]
mod tests {
    use aimer_attribute::Vec2d;
    use aimer_events::element::{ElementEvent, ScrollDeltaKind, TouchPhase};

    use super::{
        child_dispatch_position, child_route_allowed, drag_start_threshold, owns_pointer,
        pending_content_drag_wins, pointer_drag_delta,
    };
    use crate::ScrollAxis;

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
        let delta = pointer_drag_delta(
            Vec2d { x: 20.0, y: 110.0 },
            Vec2d { x: 20.0, y: 150.0 },
            1.0,
            true,
        );

        assert_eq!(delta.x, 0.0);
        assert_eq!(delta.y, 0.0);
    }

    #[test]
    fn scroll_child_hit_testing_uses_cached_cursor_instead_of_delta() {
        let cursor = Vec2d { x: 50.0, y: 60.0 };
        let event = ElementEvent::Scroll {
            delta: Vec2d { x: 0.0, y: -120.0 },
            phase: TouchPhase::Moved,
            kind: ScrollDeltaKind::Pixel,
            is_direct_manipulation: false,
        };

        let position = child_dispatch_position(&event, cursor);
        assert_eq!(position.x, cursor.x);
        assert_eq!(position.y, cursor.y);
    }

    #[test]
    fn child_outside_viewport_requires_an_active_capture() {
        assert!(!child_route_allowed(false, false, false));
        assert!(child_route_allowed(false, false, true));
    }

    #[test]
    fn child_inside_viewport_uses_normal_hit_testing() {
        assert!(child_route_allowed(true, false, false));
    }
}
