use crate::raw_scroll::{DragMode, RawScrollableContainer};
use crate::scrollable::constants::*;
use crate::ScrollAxis;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_events::element::{ElementEvent, KeyAction, NamedKey};
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, EventElement, LayoutElement, VisitorElement};
use web_time::Instant;

impl<E: Element> EventElement for RawScrollableContainer<E> {
    fn on_event(&self, event: &ElementEvent) -> bool {
        if let Some(cursor_pos) = event.get_pointer_pos() {
            self.ctrl.cursor_pos.set(Some(cursor_pos));
        }

        let Some(cursor) = self.ctrl.cursor_pos.get() else {
            return false;
        };

        // Allow active drags AND pending gestures to continue even when the
        // pointer leaves bounds. A fast swipe can move outside the scrollable
        // before exceeding the 10dp drag threshold — dropping those events
        // would silently kill the gesture. Pending counts as "claimed" once a
        // PointerDown was received inside the bounds.
        let inside = self.bounds.is_inside(cursor.x, cursor.y);
        let active_drag = self.ctrl.drag_mode.get() != DragMode::None;
        if !inside && !active_drag {
            return false;
        }

        let pos = match event {
            ElementEvent::PointerDown(p, _, _) | ElementEvent::PointerUp(p, _, _) | ElementEvent::PointerMove(p, _, _) | ElementEvent::Scroll{delta: p, ..} => *p,
            ElementEvent::Cancel | ElementEvent::CharInput { .. } | ElementEvent::KeyInput { .. } | ElementEvent::ImePreedit { .. } => Vec2d::default(),
        };

        let mode_before = self.ctrl.drag_mode.get();
        let mut child_consumed = false;

        // ── PointerUp / Cancel: arm momentum BEFORE child dispatch ──
        //
        // If a child widget consumed PointerDown (e.g. a Button inside the
        // scrollable), it will also consume PointerUp.  By handling the
        // fling/momentum setup first we guarantee the scrollable always arms
        // the post-release glide, regardless of what the child does.
        // The child still receives a Cancel so it can clear its pressed state.
        if matches!(event, ElementEvent::PointerUp(_, _, _) | ElementEvent::Cancel) {
            // Forward a Cancel to the child so it loses its active/pressed state.
            if mode_before != DragMode::None && mode_before != DragMode::Pending {
                let _ = aimer_widget::dispatch_event(&self.child, pos, &ElementEvent::Cancel);
            } else if matches!(event, ElementEvent::PointerUp(_, _, _)) {
                // No active drag — this is a tap. Dispatch PointerUp to the
                // child so widgets like Button can detect the tap gesture.
                let _ = aimer_widget::dispatch_event(&self.child, pos, event);
            }

            let now = Instant::now();
            if let Some(last_time) = self.ctrl.last_event_time.get() {
                let elapsed = now.duration_since(last_time).as_millis();
                if elapsed > VELOCITY_RESET_IDLE_MS {
                    self.ctrl.pointer_velocity.set(Vec2d::default());
                    self.ctrl.clear_velocity_history();
                    self.ctrl.cancel_fling();
                } else {
                    let max_v = MAX_SCROLL_VELOCITY * self.ctrl.last_scale.get();
                    // Use the blended drag velocity (exponential moving average)
                    // rather than the peak from the velocity history.  The peak
                    // is sensitive to single outlier samples from touch jitter or
                    // timing glitches, which can make the fling shoot off at an
                    // unrealistic speed.  The blended velocity matches what the
                    // user actually felt during the drag.
                    let raw = self.ctrl.smoothed_velocity();
                    let sv = Vec2d {
                        x: (raw.x * RELEASE_VELOCITY_GAIN).clamp(-max_v, max_v),
                        y: (raw.y * RELEASE_VELOCITY_GAIN).clamp(-max_v, max_v),
                    };
                    self.ctrl.cancel_fling();
                    self.ctrl.pointer_velocity.set(sv);
                }
            }

            self.ctrl.last_frame_time.set(Some(now));
            self.ctrl.drag_mode.set(DragMode::None);
            self.ctrl.last_pointer_pos.set(None);
            aimer_events::window::request_animation_frame();
            return false;
        }

        // ── All other events: normal child-first dispatch ──
        if mode_before == DragMode::None || mode_before == DragMode::Pending {
            child_consumed = aimer_widget::dispatch_event(&self.child, pos, event);
        }

        let we_consumed = match event {
            ElementEvent::Scroll{delta, ..} => {
                let mut offset = self.ctrl.scroll_offset.get();

                // println!("offset: {:?}", offset);
                let clamped = self.ctrl.clamp_offset(offset);

                let mut scroll_delta = match self.ctrl.axis {
                    ScrollAxis::Vertical => Vec2d { x: 0.0, y: delta.y },
                    ScrollAxis::Horizontal => Vec2d { x: delta.x, y: 0.0 },
                };

                if self.ctrl.scroll_behavior.bouncy {
                    match self.ctrl.axis {
                        ScrollAxis::Vertical => {
                            if offset.y != clamped.y {
                                let oob_dist = (offset.y - clamped.y).abs();
                                let viewport_h = self.ctrl.cached_max_scroll.get().y.max(MIN_VIEWPORT);
                                let resistance = (1.0 - (oob_dist / viewport_h).min(OOB_RESISTANCE_CLAMP)).powi(2) * OOB_RESISTANCE_SCALE;
                                scroll_delta.y *= resistance;
                            }
                        }
                        ScrollAxis::Horizontal => {
                            if offset.x != clamped.x {
                                let oob_dist = (offset.x - clamped.x).abs();
                                let viewport_w = self.ctrl.cached_max_scroll.get().x.max(MIN_VIEWPORT);
                                let resistance = (1.0 - (oob_dist / viewport_w).min(OOB_RESISTANCE_CLAMP)).powi(2) * OOB_RESISTANCE_SCALE;
                                scroll_delta.x *= resistance;
                            }
                        }
                    }
                }

                if !self.ctrl.scroll_behavior.bouncy {
                    if (offset.y <= clamped.y && scroll_delta.y < 0.0) || (offset.y >= clamped.y && scroll_delta.y > 0.0) {
                        scroll_delta.y = 0.0;
                    }
                    if (offset.x <= clamped.x && scroll_delta.x < 0.0) || (offset.x >= clamped.x && scroll_delta.x > 0.0) {
                        scroll_delta.x = 0.0;
                    }
                }

                offset.x += scroll_delta.x;
                offset.y += scroll_delta.y;
                self.ctrl.scroll_offset.set(offset);

                let now = Instant::now();
                let dt = self
                    .ctrl
                    .last_event_time
                    .get()
                    .map(|t| now.duration_since(t).as_secs_f32())
                    .unwrap_or(FRAME_REF_120)
                    .max(MIN_EVENT_DT);
                self.ctrl.last_event_time.set(Some(now));

                let frame_ref = FRAME_REF_120;

                let mut target_vx = (scroll_delta.x / dt) * frame_ref;
                let mut target_vy = (scroll_delta.y / dt) * frame_ref;

                if self.ctrl.scroll_behavior.bouncy {
                    match self.ctrl.axis {
                        ScrollAxis::Vertical => {
                            if (offset.y > clamped.y && scroll_delta.y > 0.0) || (offset.y < clamped.y && scroll_delta.y < 0.0) {
                                target_vy *= OOB_OVERSHOOT_DAMPING;
                            }
                        }
                        ScrollAxis::Horizontal => {
                            if (offset.x > clamped.x && scroll_delta.x > 0.0) || (offset.x < clamped.x && scroll_delta.x < 0.0) {
                                target_vx *= OOB_OVERSHOOT_DAMPING;
                            }
                        }
                    }
                }

                let max_scroll_v = MAX_SCROLL_VELOCITY * self.ctrl.last_scale.get();
                target_vx = target_vx.clamp(-max_scroll_v, max_scroll_v);
                target_vy = target_vy.clamp(-max_scroll_v, max_scroll_v);

                // Smooth velocity across recent samples (tames trackpad jitter).
                self.ctrl.push_velocity(target_vx, target_vy);
                let sv = self.ctrl.smoothed_velocity();
                self.ctrl.pointer_velocity.set(sv);
                // A wheel/trackpad scroll takes over from any release fling and
                // uses the velocity-based momentum, not the bézier curve.
                self.ctrl.cancel_fling();

                aimer_events::window::request_animation_frame();
                true
            }
            ElementEvent::PointerDown(p, _, id) => {
                // Primary-finger tracking: only the first finger owns the scroll.
                // Secondary fingers are ignored so a second touch doesn't cause a
                // sudden position jump — matching UIScrollView behaviour.
                //
                // Stale-touch safety net: if `active_touch_id` is still set from a
                // previous gesture but the last event was too long ago (e.g. the app
                // was backgrounded on iOS without receiving a Cancel/PointerUp),
                // clear the stale state so the new touch can be accepted.
                if let Some(prev_id) = self.ctrl.active_touch_id.get() {
                    if prev_id != *id {
                        let stale = self.ctrl.last_event_time.get().is_none_or(|t| {
                            Instant::now().duration_since(t).as_millis() > STALE_TOUCH_THRESHOLD_MS
                        });
                        if stale {
                            self.ctrl.active_touch_id.set(None);
                            self.ctrl.drag_mode.set(DragMode::None);
                            self.ctrl.last_pointer_pos.set(None);
                        } else {
                            return false;
                        }
                    }
                }
                self.ctrl.active_touch_id.set(Some(*id));

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
                    // velocity = distance / (frame_ref / (1 − friction)) to scroll exactly `distance` px.
                    let vel_scale = (1.0 - friction) / FRAME_REF_120;
                    if self.ctrl.hit_test_v_track(*p, vp_w, vp_h, v_tw) {
                        if let Some((_x, y, _w, _h)) = self.ctrl.v_thumb_rect.get() {
                            let page = vp_h * KEYBOARD_PAGE_FRACTION;
                            let vy = if p.y < y { page * vel_scale } else { -page * vel_scale };
                            self.ctrl.pointer_velocity.set(Vec2d { x: 0.0, y: vy });
                            self.ctrl.cancel_fling();
                            self.ctrl.drag_mode.set(DragMode::None);
                            self.ctrl.last_pointer_pos.set(Some(*p));
                            aimer_events::window::request_animation_frame();
                            return true;
                        }
                    }
                    if self.ctrl.hit_test_h_track(*p, vp_w, vp_h, h_tw) {
                        if let Some((x, _y, _w, _h)) = self.ctrl.h_thumb_rect.get() {
                            let page = vp_w * KEYBOARD_PAGE_FRACTION;
                            let vx = if p.x < x { page * vel_scale } else { -page * vel_scale };
                            self.ctrl.pointer_velocity.set(Vec2d { x: vx, y: 0.0 });
                            self.ctrl.cancel_fling();
                            self.ctrl.drag_mode.set(DragMode::None);
                            self.ctrl.last_pointer_pos.set(Some(*p));
                            aimer_events::window::request_animation_frame();
                            return true;
                        }
                    }
                }

                self.ctrl.pointer_velocity.set(Vec2d { x: 0.0, y: 0.0 });
                self.ctrl.clear_velocity_history();
                // A fresh touch/click stops the in-flight release fling.
                self.ctrl.cancel_fling();

                self.ctrl.drag_mode.set(mode);
                self.ctrl.last_pointer_pos.set(Some(*p));
                false
            }
            ElementEvent::PointerMove(p, _, id) => {
                // Ignore moves from non-primary fingers.
                if self.ctrl.active_touch_id.get().is_some() && self.ctrl.active_touch_id.get() != Some(*id) {
                    return false;
                }

                let mut mode = self.ctrl.drag_mode.get();
                #[allow(clippy::collapsible_if)]
                if mode  == DragMode::Pending {
                    if let Some(start) = self.ctrl.last_pointer_pos.get() {
                        let dx = p.x - start.x;
                        let dy = p.y - start.y;

                        // println!("dy: {:?}", dy);

                        let threshold = DRAG_START_THRESHOLD_DP * self.ctrl.last_scale.get();
                        let exceeds_threshold = match self.ctrl.axis {
                            ScrollAxis::Vertical => dy.abs() > threshold && dy.abs() > dx.abs(),
                            ScrollAxis::Horizontal => dx.abs() > threshold && dx.abs() > dy.abs(),
                        };

                        if exceeds_threshold {
                            mode = DragMode::Content;
                            self.ctrl.drag_mode.set(DragMode::Content);

                            let mut adjusted_start = start;
                            match self.ctrl.axis {
                                ScrollAxis::Vertical => {
                                    if dy > 0.0 {
                                        adjusted_start.y += threshold;
                                    } else {
                                        adjusted_start.y -= threshold;
                                    }
                                }
                                ScrollAxis::Horizontal => {
                                    if dx > 0.0 {
                                        adjusted_start.x += threshold;
                                    } else {
                                        adjusted_start.x -= threshold;
                                    }
                                }
                            }
                            self.ctrl.last_pointer_pos.set(Some(adjusted_start));

                            let _ = aimer_widget::dispatch_event(&self.child, *p, &ElementEvent::Cancel);
                        } else {
                            return child_consumed;
                        }
                    }
                }

                if mode != DragMode::None && mode != DragMode::Pending {
                    if let Some(last) = self.ctrl.last_pointer_pos.get() {
                        let speed_multiplier = self.ctrl.speed_multiplier;
                        let dx = (p.x - last.x) * speed_multiplier;

                        let dy = (p.y - last.y) * speed_multiplier;

                        let mut new_velocity = match mode {
                            DragMode::Content => match self.ctrl.axis {
                                ScrollAxis::Vertical => Vec2d { x: 0.0, y: dy },
                                ScrollAxis::Horizontal => Vec2d { x: dx, y: 0.0 },
                            },
                            _ => Vec2d { x: 0.0, y: 0.0 },
                        };

                        let now = Instant::now();
                        let dt = self.ctrl
                            .last_event_time
                            .get()
                            .map(|t| now.duration_since(t).as_secs_f32())
                            .unwrap_or(FRAME_REF_120)
                            .max(MIN_MOVE_DT);
                        self.ctrl.last_event_time.set(Some(now));

                        let frame_ref = FRAME_REF_120;
                        new_velocity.x = (new_velocity.x / dt) * frame_ref;
                        new_velocity.y = (new_velocity.y / dt) * frame_ref;

                        // Record the instantaneous drag velocity so that releasing
                        // the finger (PointerUp) can fling with momentum. The release
                        // path uses `smoothed_velocity()`, which reads from this
                        // history; without this push the history stays empty for
                        // touch drags (it is otherwise only filled by the trackpad
                        // `Scroll` path), making the smoothed velocity zero and
                        // stopping the scroll instantly on lift — notably on iOS.
                        self.ctrl.push_velocity(new_velocity.x, new_velocity.y);

                        let old_velocity = self.ctrl.pointer_velocity.get();
                        let blend_factor = (dt / DRAG_BLEND_WINDOW).min(1.0);
                        let blend_new = (DRAG_BLEND_BASE * (1.0 - blend_factor) + blend_factor).min(1.0);
                        let blend_old = 1.0 - blend_new;

                        new_velocity.x = old_velocity.x * blend_old + new_velocity.x * blend_new;
                        new_velocity.y = old_velocity.y * blend_old + new_velocity.y * blend_new;

                        self.ctrl.pointer_velocity.set(new_velocity);

                        let mut offset = self.ctrl.scroll_offset.get();
                        let clamped = self.ctrl.clamp_offset(offset);

                        match mode {
                            DragMode::Content => match self.ctrl.axis {
                                ScrollAxis::Vertical => {
                                    let mut actual_dy = dy;

                                    if offset.y != clamped.y {
                                        let oob_dist = (offset.y - clamped.y).abs();

                                        let viewport_h = self.ctrl.cached_max_scroll.get().y.max(MIN_VIEWPORT);
                                        let resistance = (1.0 - (oob_dist / viewport_h).min(OOB_RESISTANCE_CLAMP)).powi(2) * OOB_RESISTANCE_SCALE;
                                        actual_dy *= resistance;
                                    }
                                    offset.y += actual_dy;
                                }
                                ScrollAxis::Horizontal => {
                                    let mut actual_dx = dx;
                                    if offset.x != clamped.x {
                                        let oob_dist = (offset.x - clamped.x).abs();
                                        let viewport_w = self.ctrl.cached_max_scroll.get().x.max(MIN_VIEWPORT);
                                        let resistance = (1.0 - (oob_dist / viewport_w).min(OOB_RESISTANCE_CLAMP)).powi(2) * OOB_RESISTANCE_SCALE;
                                        actual_dx *= resistance;
                                    }
                                    offset.x += actual_dx;
                                }
                            },
                            DragMode::VerticalScrollbar => {
                                let target_y = offset.y - dy * self.ctrl.v_scroll_multiplier.get();
                                offset.y = offset.y * SCROLLBAR_DRAG_SMOOTH_OLD + target_y * SCROLLBAR_DRAG_SMOOTH_NEW;
                            }
                            DragMode::HorizontalScrollbar => {
                                let target_x = offset.x - dx * self.ctrl.h_scroll_multiplier.get();
                                offset.x = offset.x * SCROLLBAR_DRAG_SMOOTH_OLD + target_x * SCROLLBAR_DRAG_SMOOTH_NEW;
                            }
                            _ => {}
                        }

                        if !self.ctrl.scroll_behavior.bouncy {
                            offset = self.ctrl.clamp_offset(offset);
                        }
                        self.ctrl.scroll_offset.set(offset);
                    }
                    self.ctrl.last_pointer_pos.set(Some(*p));
                    aimer_events::window::request_animation_frame();
                    return true;
                }
                false
            }
            ElementEvent::KeyInput { key, action: KeyAction::Pressed, .. } => {
                if child_consumed {
                    return true;
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
                    (ScrollAxis::Vertical, NamedKey::PageDown) => Some(Vec2d { x: 0.0, y: -page_v }),
                    (ScrollAxis::Vertical, NamedKey::Home) => {
                        // Scroll to top: offset.y should be 0 (min_scroll).
                        let off = self.ctrl.scroll_offset.get();
                        Some(Vec2d { x: 0.0, y: -off.y })
                    }
                    (ScrollAxis::Vertical, NamedKey::End) => {
                        // Scroll to bottom: offset.y should be -max_scroll.y.
                        let off = self.ctrl.scroll_offset.get();
                        let max = self.ctrl.cached_max_scroll.get();
                        Some(Vec2d { x: 0.0, y: -max.y - off.y })
                    }
                    (ScrollAxis::Horizontal, NamedKey::ArrowLeft) => Some(Vec2d { x: line, y: 0.0 }),
                    (ScrollAxis::Horizontal, NamedKey::ArrowRight) => Some(Vec2d { x: -line, y: 0.0 }),
                    (ScrollAxis::Horizontal, NamedKey::PageUp) => Some(Vec2d { x: page_h, y: 0.0 }),
                    (ScrollAxis::Horizontal, NamedKey::PageDown) => Some(Vec2d { x: -page_h, y: 0.0 }),
                    (ScrollAxis::Horizontal, NamedKey::Home) => {
                        let off = self.ctrl.scroll_offset.get();
                        Some(Vec2d { x: -off.x, y: 0.0 })
                    }
                    (ScrollAxis::Horizontal, NamedKey::End) => {
                        let off = self.ctrl.scroll_offset.get();
                        let max = self.ctrl.cached_max_scroll.get();
                        Some(Vec2d { x: -max.x - off.x, y: 0.0 })
                    }
                    _ => None,
                };

                if let Some(delta) = scroll {
                    let mut offset = self.ctrl.scroll_offset.get();
                    offset.x += delta.x;
                    offset.y += delta.y;
                    if !self.ctrl.scroll_behavior.bouncy {
                        offset = self.ctrl.clamp_offset(offset);
                    }
                    self.ctrl.scroll_offset.set(offset);
                    self.ctrl.pointer_velocity.set(Vec2d { x: 0.0, y: 0.0 });
                    self.ctrl.clear_velocity_history();
                    self.ctrl.cancel_fling();
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
            ElementEvent::CharInput { .. } | ElementEvent::KeyInput { .. } | ElementEvent::ImePreedit { .. } => child_consumed,
        };

        child_consumed || we_consumed
    }

    fn event_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {}


}

impl<E: Element> VisitorElement for RawScrollableContainer<E> {
    fn debug_name(&self) -> &'static str {
        "RawScrollableContainer"
    }
}

impl<E: Element> LayoutElement for RawScrollableContainer<E> {
     fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
         ResolvedSize { width: ctx.box_constraint.max_width, height: ctx.box_constraint.max_height }
     }

     fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
         let mut child_ctx = ctx.clone();
         match self.ctrl.axis {
             ScrollAxis::Vertical => child_ctx.box_constraint.max_height = f32::MAX,
             ScrollAxis::Horizontal => child_ctx.box_constraint.max_width = f32::MAX,
         }
         self.child.computed_size(&child_ctx)
     }
 }
