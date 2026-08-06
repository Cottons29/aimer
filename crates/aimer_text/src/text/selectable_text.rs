use std::cell::RefCell;
use std::rc::Rc;

use aimer_attribute::ResolvedSize;
use aimer_events::element::ElementEvent;
use aimer_events::pointer::{PointerButton, PointerSource};
use aimer_style::{TextAlign, TextStyle};
use aimer_utils::AnimInstant;
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{
    Drawable, Element, EventElement, EventResult, LayoutElement, PointerKey, VisitorElement,
};

use crate::paragraph::{Paragraph, geometry};
use crate::selection::SelectionPoint;
use crate::selection::cursor::HoverCursor;
use crate::selection::selectable::{Selectable, SelectionBinding, TextGeometry};
use crate::selection::session::{SelectionSession, SelectionSlot};
use crate::selection::touch_hold::{TouchHold, TouchHoldGate, enter_hold};
use crate::selection::ui;
use crate::text_span::ResolvedTextSpan;

/// The selectable element produced by [`Text`](crate::Text) inside a
/// [`SelectionArea`](crate::SelectionArea).
///
/// It is the paragraph-backed twin of
/// [`RawTextWidget`](crate::RawTextWidget): same string, same style, same
/// alignment, but laid out through the shared
/// [`Paragraph`](crate::paragraph::Paragraph) so it owns per-grapheme geometry
/// and can take part in a selection. Outside a region `Text` keeps emitting
/// `RawTextWidget`, which pays nothing for selection.
pub struct RawSelectableText {
    paragraph: Paragraph,
    text: Rc<str>,
    selection_color: Color,
    binding: RefCell<SelectionBinding>,
    hover_cursor: HoverCursor,
    /// Keeps a finger from selecting until it has rested; a mouse never waits.
    pub(crate) touch_hold: TouchHoldGate,
}

impl RawSelectableText {
    /// Builds the element, registering `text` with the ambient session.
    pub(crate) fn new(
        ctx: &BuildContext,
        text: Rc<str>,
        text_style: TextStyle,
        text_align: TextAlign,
        fallback_color: Color,
    ) -> Self {
        let binding = SelectionBinding::new(ctx, Rc::clone(&text), fallback_color);
        let selection_color = binding.session.selection_color();
        Self {
            paragraph: Paragraph::new(
                vec![ResolvedTextSpan::plain(Rc::clone(&text), text_style)],
                text_align,
                text_style.text_overflow,
            ),
            text,
            selection_color,
            binding: RefCell::new(binding),
            hover_cursor: HoverCursor::new(),
            touch_hold: TouchHoldGate::new(),
        }
    }

    #[inline]
    fn geometry(&self) -> Rc<TextGeometry> {
        Rc::clone(&self.binding.borrow().geometry)
    }

    #[inline]
    fn session(&self) -> Rc<SelectionSession> {
        Rc::clone(&self.binding.borrow().session)
    }

    #[inline]
    pub(crate) fn slot(&self) -> Rc<SelectionSlot> {
        Rc::clone(&self.binding.borrow().slot)
    }
}

impl VisitorElement for RawSelectableText {
    fn debug_name(&self) -> &'static str {
        "SelectableText"
    }
}

impl aimer_widget::Rebuildable for RawSelectableText {
    #[inline]
    fn option_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    /// Keeps a selection covering this text alive across a rebuild.
    fn adopt_runtime_state_from(&self, old: &dyn Element) {
        let Some(old) = old
            .option_any()
            .and_then(|value| value.downcast_ref::<Self>())
        else {
            return;
        };
        let adopted = old.binding.borrow().adopt(Rc::clone(&self.text));
        *self.binding.borrow_mut() = adopted;
    }
}

impl LayoutElement for RawSelectableText {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.paragraph.prepare(ctx).size
    }

    fn invalidate_layout(&self) {
        self.paragraph.invalidate();
    }

    fn pos_start_end(&self) -> Option<(aimer_attribute::Vec2d, aimer_attribute::Vec2d)> {
        self.geometry().bounds.pos_start_end()
    }
}

impl Drawable for RawSelectableText {
    fn draw(&self, ctx: &BuildContext) {
        let slot = self.slot();
        let geometry_state = self.geometry();
        slot.stamp();
        let layout = self.paragraph.prepare(ctx);
        let (abs_x, abs_y) = ctx.canvas.get_transform_translation();
        geometry_state.bounds.save(
            ctx.scale,
            abs_x,
            abs_y,
            layout.size.width,
            layout.size.height,
        );
        geometry_state.regions.borrow_mut().clear();

        let clipped = self.paragraph.needs_clip();
        if clipped {
            ctx.canvas.save();
            ctx.canvas.set_clip(
                (0.0, 0.0).into(),
                ResolvedSize {
                    width: self.paragraph.available_width(ctx),
                    height: ctx.parent_size.height,
                },
            );
        }

        self.paragraph.draw_backgrounds(ctx, &layout);
        geometry::hit_regions(
            &layout,
            abs_x,
            abs_y,
            ctx.scale,
            ctx.visible_rect,
            &mut geometry_state.regions.borrow_mut(),
        );
        let selection = slot.selected_range().unwrap_or(0..0);
        for run in geometry::selection_runs(&layout, selection, ctx.visible_rect) {
            ctx.canvas.fill_color_rect(
                (run.x, run.y).into(),
                ResolvedSize {
                    width: run.width,
                    height: run.height,
                },
                self.selection_color,
                [0.0; 4],
            );
        }

        self.paragraph
            .draw_spans(ctx, &layout, |span| span.style.color, |_, _| {});

        if clipped {
            ctx.canvas.clear_clip();
            ctx.canvas.restore();
        }
    }
}

impl EventElement for RawSelectableText {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        // The selection's knobs and callout are painted over this text, so a
        // press that grabbed one must never reach the paragraph underneath.
        if let Some(result) = ui::intercept(&self.session(), event) {
            return result;
        }

        let geometry = self.geometry();
        let over_glyphs = event
            .pointer()
            .is_some_and(|info| geometry.hits_glyph(info.x(), info.y()));
        let cursor_claimed = self
            .hover_cursor
            .apply(geometry.window(), event, true, false, over_glyphs);

        match event {
            ElementEvent::PointerDown(info) => {
                let pos = info.pos;
                let geometry = self.geometry();
                let pointer = PointerKey::new(info.source, info.id);
                if info.button == PointerButton::Secondary {
                    return ui::open_context_menu(
                        &self.session(),
                        &self.slot(),
                        &geometry,
                        pos,
                        pointer,
                    )
                    .into();
                }
                // The offset lookup snaps to the nearest glyph, so a press that
                // never touched this text — the tree broadcasts the presses
                // nobody took — is told apart by the painted bounds first, and
                // dismisses the selection instead of starting a new one.
                let inside = geometry
                    .painted_bounds()
                    .is_some_and(|bounds| bounds.is_inside(pos.x, pos.y));
                let offset = inside.then(|| geometry.offset_at(pos.x, pos.y)).flatten();
                let Some(offset) = offset else {
                    self.touch_hold.clear();
                    let session = self.session();
                    if session.active_pointer() != Some(pointer) {
                        session.clear();
                    }
                    return false.into();
                };
                if info.source == PointerSource::Touch {
                    // A finger means a scroll as often as a selection, so the
                    // press is only remembered — and left unconsumed, so an
                    // enclosing scrollable can still claim it — until the hold
                    // has been earned.
                    self.touch_hold
                        .press(pointer, offset, pos, AnimInstant::now());
                    return false.into();
                }
                self.session()
                    .begin(SelectionPoint::new(self.slot(), offset), pointer);
                EventResult::consumed().with_pointer_capture(pointer)
            }
            ElementEvent::PointerMove(info) => {
                let pointer = PointerKey::new(info.source, info.id);
                match self.touch_hold.poll(pointer, info.pos, AnimInstant::now()) {
                    TouchHold::Entered(offset) => {
                        enter_hold(&self.session(), &self.slot(), offset, pointer);
                        return EventResult::consumed().with_pointer_capture(pointer);
                    }
                    // Still resting, or gone off scrolling: either way the
                    // gesture is not this element's yet.
                    TouchHold::Waiting | TouchHold::Abandoned => return false.into(),
                    TouchHold::Idle => {}
                }
                let session = self.session();
                if session.active_pointer() != Some(pointer) {
                    return cursor_claimed.into();
                }
                session.extend_to_position(info.x(), info.y(), pointer);
                EventResult::consumed()
            }
            ElementEvent::PointerUp(info) => {
                let pointer = PointerKey::new(info.source, info.id);
                if let TouchHold::Entered(offset) =
                    self.touch_hold.poll(pointer, info.pos, AnimInstant::now())
                {
                    // Held still and let go: the word under the finger stays
                    // selected.
                    let session = self.session();
                    enter_hold(&session, &self.slot(), offset, pointer);
                    session.end(pointer);
                    ui::offer_menu_after_gesture(&session, info.source);
                    return EventResult::consumed();
                }
                self.touch_hold.clear();
                let session = self.session();
                if session.active_pointer() != Some(pointer) {
                    return false.into();
                }
                session.extend_to_position(info.x(), info.y(), pointer);
                session.end(pointer);
                ui::offer_menu_after_gesture(&session, info.source);
                EventResult::consumed().with_pointer_release(pointer)
            }
            ElementEvent::Cancel => {
                self.touch_hold.clear();
                self.session().cancel();
                false.into()
            }
            _ => cursor_claimed.into(),
        }
    }
}

#[cfg(test)]
impl RawSelectableText {
    /// Builds an element registered with `session` that behaves as if it had
    /// already painted `regions` inside `bounds`.
    pub(crate) fn painted(
        session: &Rc<SelectionSession>,
        window: &aimer_widget::base::WindowHandle,
        text: Rc<str>,
        regions: Vec<crate::selection::TextHitRegion>,
        bounds: aimer_attribute::Bounds,
    ) -> Self {
        let geometry = Rc::new(TextGeometry::new(window.clone()));
        let slot = session.register(Rc::clone(&text), Rc::downgrade(&geometry) as _);
        slot.stamp();
        *geometry.regions.borrow_mut() = regions;
        geometry
            .bounds
            .save(1.0, bounds.x, bounds.y, bounds.width, bounds.height);
        Self {
            paragraph: Paragraph::new(
                vec![ResolvedTextSpan::plain(Rc::clone(&text), TextStyle::default())],
                TextAlign::TopLeft,
                aimer_style::TextOverflow::Clip,
            ),
            text,
            selection_color: session.selection_color(),
            binding: RefCell::new(SelectionBinding {
                geometry,
                session: Rc::clone(session),
                slot,
                owns_session: false,
            }),
            hover_cursor: HoverCursor::new(),
            touch_hold: TouchHoldGate::new(),
        }
    }
}
