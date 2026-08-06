use std::rc::Rc;

use aimer_attribute::{ResolvedSize, Size, Vec2d};
use aimer_events::element::{ElementEvent, KeyAction, NamedKey};
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{
    AnyElement, Drawable, Element, EventElement, EventResult, LayoutElement, PointerKey,
    Rebuildable, RequiredChild, VisitorElement, Widget,
};

use crate::selection::selectable::{SelectionScope, selection_coordinator};
use crate::selection::session::SelectionSession;
use crate::selection::ui;

/// The default highlight color of a selection region.
const DEFAULT_SELECTION_COLOR: Color = Color::Rgba(51, 153, 255, 96);

/// Makes every text in a subtree take part in **one** selection.
///
/// Inside a `SelectionArea` a drag that starts in one text and ends in another
/// selects the suffix of the first, all of the widgets in between and the prefix
/// of the last — including across the empty gaps between them. Plain
/// [`Text`](crate::Text) becomes selectable simply by being inside the region;
/// outside one it stays the non-selectable fast path and pays nothing.
///
/// The region also owns the keyboard: `Ctrl`/`Cmd` + `A` selects every text in
/// it and `Ctrl`/`Cmd` + `C` copies their selected text in reading order, joined
/// by `\n`.
///
/// On top of the selection the region paints the two pieces of furniture every
/// platform grows around one: a blue handle at each end of a **touch**
/// selection, which can be dragged to adjust it, and a floating `Copy` /
/// `Select All` callout raised by a completed finger hold, by letting a handle
/// go, or by a right-click. Both take a press before the text underneath does,
/// and a press anywhere else dismisses the callout.
///
/// Several regions can coexist on one screen; they are independent, and starting
/// a selection in one clears the other.
///
/// # Examples
///
/// ```no_run
/// use aimer_text::{RichText, SelectionArea, Text, TextSpan};
///
/// # fn ui() -> impl aimer_widget::Widget {
/// SelectionArea::new().child(RichText::new(TextSpan::new("Selectable")))
/// # }
/// ```
///
/// Overriding the highlight color of every text in the region:
///
/// ```no_run
/// use aimer_text::{SelectionArea, Text};
/// use aimer_widget::base::Color;
///
/// # fn ui() -> impl aimer_widget::Widget {
/// SelectionArea::new()
///     .selection_color(Color::Rgba(255, 0, 128, 64))
///     .child(Text::new("Selectable"))
/// # }
/// ```
pub struct SelectionArea<W = RequiredChild> {
    selection_color: Color,
    child: W,
}

impl SelectionArea {
    /// Creates a region with the default highlight color.
    #[inline]
    pub fn new() -> Self {
        Self {
            selection_color: DEFAULT_SELECTION_COLOR,
            child: RequiredChild,
        }
    }
}

impl Default for SelectionArea {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<W> SelectionArea<W> {
    /// Sets the highlight color used by every text in the region that does not
    /// override it.
    #[inline]
    pub fn selection_color(mut self, selection_color: Color) -> Self {
        self.selection_color = selection_color;
        self
    }

    /// Puts `child` inside the region, which makes the region a valid widget.
    #[inline]
    pub fn child<C: Widget>(self, child: C) -> SelectionArea<C> {
        SelectionArea {
            selection_color: self.selection_color,
            child,
        }
    }

    /// Sugar for `SelectionArea::new().child(..).boxed()`.
    #[inline]
    pub fn dyn_child<C: Widget + 'static>(self, child: C) -> aimer_widget::AnyWidget {
        SelectionArea {
            selection_color: self.selection_color,
            child,
        }
        .boxed()
    }
}

impl<W: Widget + 'static> Widget for SelectionArea<W> {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        let session = SelectionSession::new(
            ctx.window.clone(),
            selection_coordinator(ctx),
            self.selection_color,
        );
        let child = ctx.with_state(SelectionScope(Rc::clone(&session)), |ctx| {
            self.child.to_element(ctx)
        });
        SelectionAreaElement { session, child }.boxed()
    }

    fn debug_name(&self) -> &'static str {
        "SelectionArea"
    }
}

/// The element produced by [`SelectionArea`].
///
/// It scopes the session over its subtree, owns the region's keyboard shortcuts
/// and clears the selection on any press that no text of the region took —
/// whether it landed on the region's own background or outside it entirely.
pub struct SelectionAreaElement {
    pub(crate) session: Rc<SelectionSession>,
    pub(crate) child: AnyElement,
}

impl SelectionAreaElement {
    fn scoped<R>(&self, ctx: &BuildContext, callback: impl FnOnce(&BuildContext) -> R) -> R {
        ctx.with_state(SelectionScope(Rc::clone(&self.session)), callback)
    }
}

impl VisitorElement for SelectionAreaElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "SelectionArea"
    }
}

impl Drawable for SelectionAreaElement {
    fn draw(&self, ctx: &BuildContext) {
        self.session.begin_frame();
        self.scoped(ctx, |ctx| self.child.draw(ctx));
        // The knobs belong on top of the whole region, so they are painted
        // here rather than by whichever text happens to own an endpoint. The
        // callout is not: it paints through the modal host's overlay, where no
        // ancestor of this region can clip it.
        ui::paint_handles(ctx, &self.session);
    }
}

impl LayoutElement for SelectionAreaElement {
    fn pos(&self) -> Option<Vec2d> {
        self.child.pos()
    }

    fn size(&self) -> Option<Size> {
        self.child.size()
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.scoped(ctx, |ctx| self.child.layout(ctx))
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.scoped(ctx, |ctx| self.child.computed_size(ctx))
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.scoped(ctx, |ctx| self.child.content_size(ctx))
    }

    fn layer(&self) -> u32 {
        self.child.layer()
    }

    fn flex(&self) -> Option<f32> {
        self.child.flex()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.child.get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.child.invalidate_layout();
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.child.pos_start_end()
    }
}

impl EventElement for SelectionAreaElement {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        // The knobs and the callout sit above everything the region drew, so
        // they are offered the event before it can mean anything else.
        if let Some(result) = ui::intercept(&self.session, event) {
            return result;
        }
        match event {
            // A press that reaches the region drops the selection: a text of
            // this region consumes its own press, and the tree only broadcasts
            // the presses nobody took — so anything arriving here landed
            // somewhere that is not selectable, inside the region or outside it.
            ElementEvent::PointerDown(info) => {
                let pointer = PointerKey::new(info.source, info.id);
                if self.session.active_pointer() != Some(pointer) {
                    self.session.clear();
                }
                false
            }
            ElementEvent::Cancel => {
                self.session.cancel();
                false
            }
            ElementEvent::KeyInput {
                key: NamedKey::Other(key),
                action,
                modifiers,
            } if self.session.is_focused()
                && matches!(action, KeyAction::Pressed | KeyAction::Repeat)
                && (modifiers.ctrl || modifiers.meta) =>
            {
                match key.as_str() {
                    "a" => {
                        self.session.select_all();
                        true
                    }
                    "c" => {
                        let text = self.session.selected_text();
                        if text.is_empty() {
                            return false.into();
                        }
                        let _ = aimer_native::clipboard::set_text(&text);
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
        .into()
    }
}

impl Rebuildable for SelectionAreaElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.scoped(ctx, |ctx| self.child.rebuild_if_dirty(ctx));
    }

    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        self.scoped(ctx, callback);
    }

    fn is_carry_state(&self) -> bool {
        true
    }

    fn mark_needs_rebuild(&self) {
        self.child.mark_needs_rebuild();
    }

    fn option_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use aimer_attribute::Bounds;
    use aimer_events::pointer::{PointerButton, PointerInfo, PointerSource};
    use aimer_widget::base::WindowHandle;

    use super::*;
    use crate::selection::TextHitRegion;
    use crate::selection::selectable::{SelectionCoordinator, TextGeometry};
    use crate::selection::session::SelectionSlot;

    /// A child that occupies a fixed rectangle and never handles anything.
    struct StubChild {
        bounds: Bounds,
    }

    impl VisitorElement for StubChild {
        fn debug_name(&self) -> &'static str {
            "StubChild"
        }
    }

    impl EventElement for StubChild {}
    impl Drawable for StubChild {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for StubChild {}

    impl LayoutElement for StubChild {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some((
                Vec2d {
                    x: self.bounds.x,
                    y: self.bounds.y,
                },
                Vec2d {
                    x: self.bounds.x + self.bounds.width,
                    y: self.bounds.y + self.bounds.height,
                },
            ))
        }
    }

    /// A region spanning `0..100` in both axes with one painted participant
    /// holding a single ten-by-twenty glyph box.
    fn region() -> (
        SelectionAreaElement,
        Rc<SelectionSlot>,
        Rc<TextGeometry>,
    ) {
        let window = WindowHandle::headless(winit::dpi::PhysicalSize::new(200, 200), 1.0);
        let session = SelectionSession::new(
            window.clone(),
            Rc::new(SelectionCoordinator::default()),
            DEFAULT_SELECTION_COLOR,
        );
        let text: Rc<str> = Rc::from("a");
        let geometry = Rc::new(TextGeometry::new(window));
        let slot = session.register(Rc::clone(&text), Rc::downgrade(&geometry) as _);
        *geometry.regions.borrow_mut() = vec![TextHitRegion::new(
            0..1,
            Bounds::new(0.0, 0.0, 10.0, 20.0),
        )];
        geometry.bounds.save(1.0, 0.0, 0.0, 10.0, 20.0);
        slot.stamp();
        session.begin_frame();
        let element = SelectionAreaElement {
            session,
            child: StubChild {
                bounds: Bounds::new(0.0, 0.0, 100.0, 100.0),
            }
            .boxed(),
        };
        (element, slot, geometry)
    }

    fn press_at(x: f32, y: f32) -> ElementEvent {
        ElementEvent::PointerDown(PointerInfo::new(
            Vec2d { x, y },
            PointerSource::Mouse,
            0,
            PointerButton::Primary,
        ))
    }

    #[test]
    fn a_press_outside_the_region_clears_the_selection() {
        let (region, slot, _geometry) = region();
        region.session.select_all();
        assert_eq!(slot.selected_range(), Some(0..1));

        let _ = region.on_event(&press_at(500.0, 900.0));

        assert_eq!(slot.selected_range(), None);
        assert_eq!(region.session.selected_text(), "");
    }

    #[test]
    fn a_press_inside_the_region_that_hits_no_text_clears_the_selection() {
        let (region, slot, _geometry) = region();
        region.session.select_all();

        let _ = region.on_event(&press_at(60.0, 60.0));

        assert_eq!(slot.selected_range(), None);
    }

    #[test]
    fn the_press_that_started_the_regions_own_gesture_keeps_the_selection() {
        let (region, slot, _geometry) = region();
        let pointer = PointerKey::new(PointerSource::Mouse, 0);
        region
            .session
            .begin(crate::selection::SelectionPoint::new(Rc::clone(&slot), 0), pointer);
        region.session.extend_to_position(9.0, 10.0, pointer);
        assert_eq!(slot.selected_range(), Some(0..1));

        let _ = region.on_event(&press_at(5.0, 10.0));

        assert_eq!(slot.selected_range(), Some(0..1));
    }
}
