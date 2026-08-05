use std::cell::RefCell;
use std::rc::Rc;

use aimer_attribute::CacheBounds;
use aimer_attribute::dimension::Dimension;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_widget::base::*;
use aimer_widget::{Element, EventDispatcher, Rebuildable};

pub use crate::scrollable::controller::DragMode;
use crate::scrollable::controller::ScrollState;
use crate::scrollable::scroll_bar::ScrollBar;

pub struct RawScrollableContainer<E: Element> {
    pub(crate) child: E,
    /// The live scroll engine. Held behind an `Rc` so an app-supplied
    /// [`ScrollController`](crate::ScrollController) can share the very same
    /// state and drive it programmatically across rebuilds.
    pub(crate) ctrl: Rc<ScrollState>,
    pub(crate) vertical_scroll_bar: Option<ScrollBar>,
    pub(crate) horizontal_scroll_bar: Option<ScrollBar>,
    pub(crate) bounds: CacheBounds,
    pub(crate) event_dispatcher: RefCell<EventDispatcher>,
}

impl<E: Element + 'static> Rebuildable for RawScrollableContainer<E> {
    #[inline]
    fn option_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    /// Claims the pointer the element being replaced was routing.
    ///
    /// This container dispatches to its child itself, so the capture a pressed
    /// child took lives in the dispatcher *beside* the children rather than in
    /// them — precisely the state reconciliation's positional walk cannot reach.
    /// A press that rebuilds the subtree while the pointer is still down — a
    /// button in the list darkening under the finger — would leave the
    /// replacement owning nothing, and
    /// [`child_route_allowed`](crate::scrollable::handle_scroll) gates child
    /// routing on exactly that capture: the release would be dismissed as
    /// landing outside the viewport and the captured child would be stranded
    /// mid-gesture, never hearing the pointer lift.
    ///
    /// The dispatcher is *moved* out of `old`, which reconciliation drops
    /// immediately afterwards. Two containers both believing they own the
    /// pointer would deliver every remaining event twice. It names the old
    /// subtree's identities, and those identities are transferred onto the new
    /// elements in the pass that follows this one, so the carried capture
    /// resolves once reconciliation completes.
    fn adopt_runtime_state_from(&self, old: &dyn Element) {
        let Some(old) = old
            .option_any()
            .and_then(|value| value.downcast_ref::<Self>())
        else {
            return;
        };

        *self.event_dispatcher.borrow_mut() =
            std::mem::take(&mut *old.event_dispatcher.borrow_mut());
    }
}

impl<E: Element> RawScrollableContainer<E> {
    /// Compute the viewport size from the build context constraints.
    pub(crate) fn viewport_size(&self, ctx: &BuildContext) -> (f32, f32) {
        (ctx.box_constraint.max_width, ctx.box_constraint.max_height)
    }

    pub(crate) fn draw_scrollbar(
        &self,
        ctx: &BuildContext,
        scroll_bar: &ScrollBar,
        viewport_w: f32,
        viewport_h: f32,
        is_vertical: bool,
    ) {
        let scale = ctx.scale;
        let offset = self.ctrl.visual_offset(self.ctrl.scroll_offset.get());

        let track_width = match scroll_bar.track.width {
            Dimension::Px(v) => v * scale,
            Dimension::Percent(p) => {
                if is_vertical {
                    viewport_w * (p / 100.0)
                } else {
                    viewport_h * (p / 100.0)
                }
            }
            Dimension::Auto => {
                #[cfg(any(target_os = "android", target_os = "ios"))]
                {
                    6.0 * scale
                }
                #[cfg(not(any(target_os = "android", target_os = "ios")))]
                {
                    12.0 * scale
                }
            }
        };

        // Cache track width for hit-testing track clicks.
        if is_vertical {
            self.ctrl.cached_v_track_width.set(track_width);
        } else {
            self.ctrl.cached_h_track_width.set(track_width);
        }

        let thumb_width = match scroll_bar.thumb.width {
            Dimension::Px(v) => v * scale,
            Dimension::Percent(p) => track_width * (p / 100.0),
            Dimension::Auto => (track_width * 0.6).max(4.0),
        };

        // Reuse the content size computed once at the start of this frame's draw
        // (see `draw_scroll`) to avoid recomputing the child layout.
        let content_size = self.ctrl.cached_content_size.get();
        let (track_length, content_extent, scroll_pos) = if is_vertical {
            (viewport_h, content_size.height, -offset.y)
        } else {
            (viewport_w, content_size.width, -offset.x)
        };

        let button_h = if is_vertical {
            let resolve_btn_h = |btn: &crate::scrollable::scroll_bar::ScrollButton| -> f32 {
                match btn.height {
                    Dimension::Px(v) => v * scale,
                    Dimension::Percent(p) => track_length * (p / 100.0),
                    Dimension::Auto => track_width,
                }
            };
            let up_h = scroll_bar
                .up_button
                .as_ref()
                .map(&resolve_btn_h)
                .unwrap_or(0.0);
            let down_h = scroll_bar
                .down_button
                .as_ref()
                .map(resolve_btn_h)
                .unwrap_or(0.0);
            (up_h, down_h)
        } else {
            let resolve_btn_w = |btn: &crate::scrollable::scroll_bar::ScrollButton| -> f32 {
                match btn.width {
                    Dimension::Px(v) => v * scale,
                    Dimension::Percent(p) => track_length * (p / 100.0),
                    Dimension::Auto => track_width,
                }
            };
            let left_w = scroll_bar
                .up_button
                .as_ref()
                .map(&resolve_btn_w)
                .unwrap_or(0.0);
            let right_w = scroll_bar
                .down_button
                .as_ref()
                .map(resolve_btn_w)
                .unwrap_or(0.0);
            (left_w, right_w)
        };

        let usable_track = (track_length - button_h.0 - button_h.1).max(0.0);
        let thumb_ratio = if content_extent > 0.0 {
            (track_length / content_extent).min(1.0)
        } else {
            1.0
        };
        let thumb_length = (usable_track * thumb_ratio).max(20.0 * scale);
        let max_thumb_move = (usable_track - thumb_length).max(0.0);
        let max_scroll = (content_extent - track_length).max(0.0);
        let multiplier = if max_thumb_move > 0.0 {
            max_scroll / max_thumb_move
        } else {
            0.0
        };
        if is_vertical {
            self.ctrl.v_scroll_multiplier.set(multiplier);
        } else {
            self.ctrl.h_scroll_multiplier.set(multiplier);
        }

        let scroll_ratio = if max_scroll > 0.0 {
            scroll_pos / max_scroll
        } else {
            0.0
        };
        let thumb_offset = button_h.0 + scroll_ratio * max_thumb_move;

        let thumb_radius = match scroll_bar.thumb.radius {
            Dimension::Px(v) => v * scale,
            Dimension::Percent(p) => thumb_width * (p / 100.0),
            Dimension::Auto => thumb_width / 2.0,
        };

        ctx.canvas.save();

        // Position the scrollbar at the edge of the viewport
        if is_vertical {
            ctx.canvas.translate(Vec2d {
                x: (viewport_w - track_width).round(),
                y: 0.0,
            });
        } else {
            ctx.canvas.translate(Vec2d {
                x: 0.0,
                y: (viewport_h - track_width).round(),
            });
        }

        // Draw track
        let track_color: Color = scroll_bar.track.color.into();
        let (track_w, track_h) = if is_vertical {
            (track_width, track_length)
        } else {
            (track_length, track_width)
        };
        ctx.canvas.fill_color_rect(
            Vec2d { x: 0.0, y: 0.0 },
            ResolvedSize {
                width: track_w,
                height: track_h,
            },
            track_color,
            [0.0; 4],
        );

        // Draw up/left button
        if let Some(ref btn) = scroll_bar.up_button {
            let btn_color: Color = btn.color.into();
            let (bw, bh) = if is_vertical {
                (track_width, button_h.0)
            } else {
                (button_h.0, track_width)
            };
            ctx.canvas.fill_color_rect(
                Vec2d { x: 0.0, y: 0.0 },
                ResolvedSize {
                    width: bw,
                    height: bh,
                },
                btn_color,
                [0.0; 4],
            );
        }

        // Draw down/right button
        if let Some(ref btn) = scroll_bar.down_button {
            let btn_color: Color = btn.color.into();
            let (bx, by, bw, bh) = if is_vertical {
                (0.0, track_length - button_h.1, track_width, button_h.1)
            } else {
                (track_length - button_h.1, 0.0, button_h.1, track_width)
            };
            ctx.canvas.fill_color_rect(
                Vec2d { x: bx, y: by },
                ResolvedSize {
                    width: bw,
                    height: bh,
                },
                btn_color,
                [0.0; 4],
            );
        }

        // Draw thumb. Pick the color based on drag (active) and cursor hover state.
        // The thumb hit-rect used for hover is the one stored on the previous frame.
        let is_active = if is_vertical {
            self.ctrl.drag_mode.get() == DragMode::VerticalScrollbar
        } else {
            self.ctrl.drag_mode.get() == DragMode::HorizontalScrollbar
        };
        let is_hover = self.ctrl.cursor_pos.get().is_some_and(|c| {
            if is_vertical {
                self.ctrl.hit_test_v_thumb(c)
            } else {
                self.ctrl.hit_test_h_thumb(c)
            }
        });
        let thumb_color: Color = if is_active {
            scroll_bar.thumb.active_color.into()
        } else if is_hover {
            scroll_bar.thumb.hover_color.into()
        } else {
            scroll_bar.thumb.color.into()
        };
        let thumb_x_offset = (track_width - thumb_width) / 2.0;
        let (tx, ty, tw, th) = if is_vertical {
            self.ctrl.v_thumb_rect.set(Some((
                viewport_w - track_width + thumb_x_offset,
                thumb_offset,
                thumb_width,
                thumb_length,
            )));
            (thumb_x_offset, thumb_offset, thumb_width, thumb_length)
        } else {
            self.ctrl.h_thumb_rect.set(Some((
                thumb_offset,
                viewport_h - track_width + thumb_x_offset,
                thumb_length,
                thumb_width,
            )));
            (thumb_offset, thumb_x_offset, thumb_length, thumb_width)
        };

        ctx.canvas.fill_color_rect(
            Vec2d { x: tx, y: ty },
            ResolvedSize {
                width: tw,
                height: th,
            },
            thumb_color,
            [thumb_radius; 4],
        );

        ctx.canvas.restore();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use aimer_events::element::ElementEvent;
    use aimer_events::pointer::{PointerInfo, PointerSource};
    use aimer_widget::{
        AnyElement, CaptureRequest, Drawable, EventElement, EventResult, LayoutElement, PointerKey,
        VisitorElement,
    };

    use super::*;

    struct CapturingChild {
        events: Rc<Cell<usize>>,
    }

    impl VisitorElement for CapturingChild {
        fn debug_name(&self) -> &'static str {
            "CapturingChild"
        }
    }

    impl EventElement for CapturingChild {
        fn on_event(&self, event: &ElementEvent) -> EventResult {
            self.events.set(self.events.get() + 1);
            match event {
                ElementEvent::PointerDown(pointer) => EventResult::consumed()
                    .with_pointer_capture(PointerKey::new(pointer.source, pointer.id)),
                ElementEvent::PointerUp(pointer) => EventResult::consumed()
                    .with_pointer_release(PointerKey::new(pointer.source, pointer.id)),
                _ => EventResult::consumed(),
            }
        }
    }

    impl LayoutElement for CapturingChild {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some((Vec2d::default(), Vec2d { x: 100.0, y: 100.0 }))
        }
    }

    impl Drawable for CapturingChild {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for CapturingChild {}

    /// A container laid out over the top-left 100x100 corner, wrapping a child
    /// that captures the pointer it is pressed with. Both elements of a rebuild
    /// share `ctrl`, exactly as the live scroll engine is shared across one.
    fn capturing_scrollable(
        events: Rc<Cell<usize>>,
        ctrl: Rc<ScrollState>,
    ) -> RawScrollableContainer<AnyElement> {
        let bounds = CacheBounds::new();
        bounds.save(1.0, 0.0, 0.0, 100.0, 100.0);

        RawScrollableContainer {
            child: CapturingChild { events }.boxed(),
            ctrl,
            vertical_scroll_bar: None,
            horizontal_scroll_bar: None,
            bounds,
            event_dispatcher: RefCell::new(EventDispatcher::new()),
        }
    }

    // A rebuild triggered by the press itself — a `Button` inside the list
    // darkening under the finger — replaces this container. The capture the child
    // took lives in the dispatcher beside the children rather than in them, so
    // the positional walk cannot reach it: without the hand-over,
    // `child_route_allowed` sees no capture, the replacement rejects the release
    // as being outside its viewport, and the child never hears the pointer lift.
    #[test]
    fn a_rebuild_during_a_press_keeps_the_capture_so_a_release_outside_still_lands() {
        let events = Rc::new(Cell::new(0));
        let ctrl = Rc::new(ScrollState::for_test_at(Vec2d::default()));
        let pressed = capturing_scrollable(events.clone(), ctrl.clone());
        let pointer = PointerKey::new(PointerSource::Touch, 2);

        let down = pressed.on_event(&ElementEvent::PointerDown(PointerInfo::touch(
            Vec2d { x: 10.0, y: 10.0 },
            pointer.id,
        )));
        assert_eq!(down.capture_request(), CaptureRequest::Capture(pointer));

        let rebuilt = capturing_scrollable(events.clone(), ctrl);
        // Standing in for the identity transfer reconciliation performs around
        // the hand-over, which is what makes the carried capture resolve against
        // the new subtree.
        rebuilt.child.set_element_id(pressed.child.id());
        rebuilt.adopt_runtime_state_from(&pressed as &dyn Element);

        let up = rebuilt.on_event(&ElementEvent::PointerUp(PointerInfo::touch(
            Vec2d { x: 200.0, y: 200.0 },
            pointer.id,
        )));

        assert_eq!(events.get(), 2, "the child must hear the release it is owed");
        assert_eq!(up.capture_request(), CaptureRequest::Release(pointer));
    }
}
