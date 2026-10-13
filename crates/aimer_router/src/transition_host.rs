use std::cell::{Cell, RefCell, UnsafeCell};
use std::rc::Rc;

use aimer_animation::{AnimInstant, AnimationController};
use aimer_cupid::utilities::Rect;
use aimer_widget::base::{BuildContext, ResolvedSize, Size, Vec2d};
use aimer_widget::{AnyElement, AnyWidget, ChildBuilder, Drawable, Element, EventElement,
    Key, LayoutElement, Rebuildable, VisitorElement, Widget, request_isolated_animation_frame};

use crate::outlet::RouteChildContext;
use crate::{NavigationAction, NavigationContext, NavigationTransition,
    NavigationTransitionFrame, ResolvedNavigationTransition, Route, TransitionChoice};

/// Ambient operation shared with nested shell outlets, independently of their route type.
#[derive(Clone, Copy, Default)]
pub(crate) struct NavigationRequest(pub NavigationAction);

#[derive(Clone)]
pub(crate) struct RouteBoundary(pub Key);

pub(crate) struct NavigationPages<R: Route> {
    current: Option<(R, String, ChildBuilder)>,
    playback: Option<Rc<Playback>>,
}

impl<R: Route> Default for NavigationPages<R> {
    fn default() -> Self { Self { current: None, playback: None } }
}

impl<R: Route> NavigationPages<R> {
    pub(crate) fn build(
        &mut self,
        route: R,
        action: NavigationAction,
        fallback: Option<&dyn NavigationTransition<R>>,
        ctx: &BuildContext,
        build: impl FnOnce(R) -> AnyWidget,
    ) -> NavigationFrame {
        let path = route.format();
        if self.current.as_ref().is_none_or(|(_, old_path, _)| *old_path != path) {
            if let Some(playback) = self.playback.take() { playback.finish(); }
            let effect = self.current.as_ref().and_then(|(from, _, _)| {
                // A persistent shell delegates child motion to its own outlet.
                if from.shell_key().is_some() && from.shell_key() == route.shell_key() { return None; }
                let navigation = NavigationContext { from, to: &route, action, build_context: ctx };
                let owner = if action == NavigationAction::Pop { from } else { &route };
                match owner.navigation_transition() {
                    TransitionChoice::Disabled => None,
                    TransitionChoice::Inherit => fallback.and_then(|effect| effect.resolve(&navigation)),
                    TransitionChoice::Custom(effect) => effect.resolve(&navigation),
                }
            }).filter(|effect| !effect.duration.is_zero());
            // Ordinary route entries are distinct even when their widget types
            // match. Shell entries intentionally share a boundary identity.
            let key = self.current.as_ref().filter(|(from, _, _)|
                route.shell_key().is_some() && route.shell_key() == from.shell_key())
                .and_then(|(_, _, child)| child.key()).unwrap_or_else(Key::unique);
            let child = ChildBuilder::from_widget(ScopedPage {
                widget: build(route.clone()), context: RouteChildContext::capture_page(ctx, key.clone()), key,
            });
            let previous = self.current.replace((route, path, child));
            self.playback = effect.and_then(|effect| previous.map(|(_, _, child)| {
                let controller = AnimationController::new(effect.duration, effect.curve);
                controller.forward_from_first_tick();
                aimer_events::window::request_animation_frame();
                Rc::new(Playback { effect, controller, outgoing: RefCell::new(Some(child)), finished: Cell::new(false) })
            }));
        } else {
            // An explicit parent rebuild must refresh route configuration
            // (theme/provider reads and new builder props). Keep this entry's
            // identity so reconciliation carries its state and active motion.
            let key = self.current.as_ref().and_then(|(_, _, child)| child.key()).expect("route pages have keys");
            let child = ChildBuilder::from_widget(ScopedPage {
                widget: build(route.clone()), context: RouteChildContext::capture_page(ctx, key.clone()), key,
            });
            self.current = Some((route, path, child));
        }
        NavigationFrame {
            child: self.current.as_ref().expect("a page was installed").2.clone(),
            playback: self.playback.clone().filter(|playback| !playback.finished.get()),
        }
    }
}

struct Playback {
    effect: ResolvedNavigationTransition,
    controller: AnimationController,
    outgoing: RefCell<Option<ChildBuilder>>,
    finished: Cell<bool>,
}

impl Playback {
    fn finish(&self) {
        self.finished.set(true);
        self.controller.set_value(1.0);
        self.outgoing.borrow_mut().take();
    }

    fn sample(&self, now: AnimInstant, size: ResolvedSize) -> NavigationTransitionFrame {
        if self.finished.get() { return NavigationTransitionFrame::FINISHED; }
        let progress = self.controller.tick(now);
        if !self.controller.is_animating() { self.finish(); return NavigationTransitionFrame::FINISHED; }
        self.effect.sample(progress, size).unwrap_or_else(|| {
            self.finish();
            NavigationTransitionFrame::FINISHED
        })
    }
}

#[derive(Clone)]
pub(crate) struct NavigationFrame {
    child: ChildBuilder,
    playback: Option<Rc<Playback>>,
}

impl aimer_widget::PortableWidget for NavigationFrame {}

impl Widget for NavigationFrame {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let outgoing = self.playback.as_ref().and_then(|playback| {
            playback.outgoing.borrow().as_ref().map(|child| child.build(ctx))
        });
        if let Some(child) = &outgoing { clear_departing_focus(child.as_ref(), ctx); }
        NavigationFrameElement {
            incoming: self.child.build(ctx), outgoing: UnsafeCell::new(outgoing), playback: self.playback,
        }.boxed()
    }
}

fn clear_departing_focus(element: &dyn Element, ctx: &BuildContext) {
    element.with_rebuild_context(ctx, &mut |ctx| {
        if let Some(node) = element.focus_node().filter(|node| node.has_focus()) {
            node.set_focused(false);
            if element.event_tree_role() != aimer_widget::EventTreeRole::Transparent {
                let _ = element.on_event(&aimer_events::element::ElementEvent::FocusLost);
            }
        }
        element.event_children(&mut |child| clear_departing_focus(child, ctx));
    });
}

struct NavigationFrameElement {
    incoming: AnyElement,
    outgoing: UnsafeCell<Option<AnyElement>>,
    playback: Option<Rc<Playback>>,
}

// Safety: element traversal and structural mutation run exclusively on the
// rendering thread. Outgoing children are removed only after child callbacks
// return, never while a visitor holds a reference into the slot.
impl NavigationFrameElement {
    fn outgoing(&self) -> Option<&AnyElement> {
        // Safety: read-only during traversal; cleanup runs at a frame safe point.
        unsafe { (&*self.outgoing.get()).as_ref() }
    }

    fn sample(&self, ctx: &BuildContext) -> NavigationTransitionFrame {
        self.playback.as_ref().map_or(NavigationTransitionFrame::FINISHED,
            |playback| playback.sample(AnimInstant::now(), self.logical_size(ctx)))
    }

    fn logical_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let physical = self.incoming.computed_size(ctx);
        ResolvedSize { width: physical.width / ctx.scale, height: physical.height / ctx.scale }
    }

    fn cleanup(&self) {
        if self.playback.as_ref().is_some_and(|playback| playback.finished.get()) && self.outgoing().is_some() {
            // Safety: no child traversal or callbacks are active at this point.
            unsafe { (*self.outgoing.get()).take(); }
            aimer_widget::notify_hosted_element_tree_changed();
            aimer_events::window::request_animation_frame();
        }
    }
}

impl VisitorElement for NavigationFrameElement {
    fn debug_name(&self) -> &'static str { "NavigationTransitionHost" }
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        // Stable structural order keeps the incoming page in slot zero.
        visitor(self.incoming.as_ref());
        if let Some(child) = self.outgoing() { visitor(child.as_ref()); }
    }
    fn visit_retained_v2_children<'a>(&'a self, visitor: &mut dyn FnMut(usize, &'a dyn Element)) {
        if let Some(child) = self.outgoing() { visitor(1, child.as_ref()); }
        visitor(0, self.incoming.as_ref());
    }
}

impl Drawable for NavigationFrameElement {
    fn can_paint_local_v2(&self, ctx: &BuildContext) -> bool { ctx.scale.is_finite() && ctx.scale > 0.0 }
    fn sync_local_v2_state(&self, ctx: &BuildContext) -> bool {
        let frame = self.sample(ctx);
        if self.playback.as_ref().is_some_and(|playback| !playback.finished.get()) {
            request_isolated_animation_frame(ctx);
        }
        // Indices are retained visitation ordinals, not structural source indices.
        let has_old = self.outgoing().is_some();
        if has_old && !ctx.set_local_v2_child_presentation_at(0, frame.outgoing.transform, frame.outgoing.opacity) { return false; }
        ctx.set_local_v2_child_presentation_at(usize::from(has_old), frame.incoming.transform, frame.incoming.opacity)
    }
    fn retained_clip(&self, ctx: &BuildContext) -> Option<Rect> {
        let playback = self.playback.as_ref()?;
        if playback.finished.get() || !self.can_paint_local_v2(ctx) { return None; }
        let size = self.logical_size(ctx);
        (size.width.is_finite() && size.height.is_finite() && size.width >= 0.0 && size.height >= 0.0)
            .then_some(Rect::new(0.0, 0.0, size.width, size.height))
    }
    fn retained_v2_child_context<'a>(&self, ctx: &BuildContext<'a>, _: &dyn Element) -> Option<BuildContext<'a>> {
        // This is an overlay, not a layout container. With two children the
        // generic walker cannot infer transparency and would constrain padded
        // pages again to their already-reduced content sizes.
        Some(ctx.clone())
    }
    fn update(&self, ctx: &BuildContext) {
        self.sample(ctx);
        if let Some(child) = self.outgoing() { child.update(ctx); }
        self.incoming.update(ctx);
        if self.playback.as_ref().is_some_and(|playback| !playback.finished.get()) {
            aimer_events::window::request_animation_frame();
        }
        self.cleanup();
    }
    fn draw_local_v2_compatibility(&self, ctx: &BuildContext) {
        if let Some(child) = self.outgoing() { child.update(ctx); }
        self.incoming.update(ctx);
        self.cleanup();
    }
}

impl EventElement for NavigationFrameElement {
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) { visitor(self.incoming.as_ref()); }
}
impl Rebuildable for NavigationFrameElement {}
impl LayoutElement for NavigationFrameElement {
    fn pos(&self) -> Option<Vec2d> { self.incoming.pos() }
    fn size(&self) -> Option<Size> { self.incoming.size() }
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize { self.incoming.computed_size(ctx) }
    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        if let Some(child) = self.outgoing() { child.layout(ctx); }
        self.incoming.layout(ctx)
    }
    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize { self.incoming.content_size(ctx) }
    fn layer(&self) -> u32 { self.incoming.layer() }
    fn flex(&self) -> Option<f32> { self.incoming.flex() }
    fn get_size_from_child(&self) -> Option<Size> { self.incoming.get_size_from_child() }
    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> { self.incoming.pos_start_end() }
}

struct ScopedPage { widget: AnyWidget, context: RouteChildContext, key: Key }
impl aimer_widget::PortableWidget for ScopedPage {}
impl Widget for ScopedPage {
    fn key(&self) -> Option<Key> { Some(self.key.clone()) }
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let child = self.context.with_page(ctx, |ctx| self.widget.to_element(ctx));
        PageElement { child, context: self.context, key: self.key }.boxed()
    }
}
struct PageElement { child: AnyElement, context: RouteChildContext, key: Key }
impl VisitorElement for PageElement {
    fn debug_name(&self) -> &'static str { "RouteContent" }
    fn reconciliation_key(&self) -> Option<&Key> { Some(&self.key) }
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) { visitor(self.child.as_ref()); }
}
impl Drawable for PageElement {
    fn can_paint_local_v2(&self, _: &BuildContext) -> bool { true }
    fn update(&self, ctx: &BuildContext) { self.scoped(ctx, |ctx| self.child.update(ctx)); }
    fn retained_v2_child_context<'a>(&self, ctx: &BuildContext<'a>, _: &dyn Element) -> Option<BuildContext<'a>> {
        self.scoped(ctx, |ctx| {
            let mut child = ctx.clone();
            child.inherited_states = Rc::new(RefCell::new(ctx.inherited_states.borrow().clone()));
            Some(child)
        })
    }
    fn sync_paint_geometry(&self, ctx: &BuildContext) { self.scoped(ctx, |ctx| self.child.sync_paint_geometry(ctx)); }
}
impl LayoutElement for PageElement {
    fn pos(&self) -> Option<Vec2d> { self.child.pos() }
    fn size(&self) -> Option<Size> { self.child.size() }
    fn layout(&self, ctx: &BuildContext) -> ResolvedSize { self.scoped(ctx, |ctx| self.child.layout(ctx)) }
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize { self.scoped(ctx, |ctx| self.child.computed_size(ctx)) }
    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize { self.scoped(ctx, |ctx| self.child.content_size(ctx)) }
    fn layer(&self) -> u32 { self.child.layer() }
    fn flex(&self) -> Option<f32> { self.child.flex() }
    fn get_size_from_child(&self) -> Option<Size> { self.child.get_size_from_child() }
    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> { self.child.pos_start_end() }
}
impl EventElement for PageElement {}
impl Rebuildable for PageElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) { self.scoped(ctx, |ctx| self.child.rebuild_if_dirty(ctx)); }
    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) { self.scoped(ctx, callback); }
}

impl PageElement {
    fn scoped<'a, T>(&self, ctx: &BuildContext<'a>, callback: impl FnOnce(&BuildContext<'a>) -> T) -> T {
        self.context.with_page(ctx, callback)
    }
}

#[cfg(test)]
mod tests;
