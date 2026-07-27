use std::cell::Cell;

use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_widget::base::*;
use aimer_widget::{
    AnyElement, Drawable, Element, EventElement, EventResult, LayoutElement, Rebuildable,
    VisitorElement, Widget,
};

use crate::control::controller::AnimationController;
use crate::primitives::time::AnimInstant;

fn request_next_frame() {
    aimer_events::window::request_animation_frame();
}

// ---------------------------------------------------------------------------
// FadeTransition
// ---------------------------------------------------------------------------

/// Animates the opacity of its child based on the controller's value.
///
/// At value `0.0` the child is fully transparent; at `1.0` it is fully opaque.
/// Values are clamped to that range for drawing. Layout and event behavior are
/// delegated to the child.
pub struct FadeTransition<T: Widget + 'static> {
    pub opacity: AnimationController,
    pub child: T,
}

impl<T: Widget> FadeTransition<T> {
    /// Creates an opacity transition without starting or resetting `opacity`.
    pub fn new(opacity: AnimationController, child: T) -> Self {
        Self { opacity, child }
    }
}

impl<T: Widget + 'static> Widget for FadeTransition<T> {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        let child = self.child.to_element(ctx);
        let controller = self.opacity.clone();
        let animating = Cell::new(self.opacity.is_animating());
        FadeTransitionElement {
            child,
            controller,
            animating,
        }
        .boxed()
    }
}

macro_rules! impl_transition_element {
    ($name:ident, $debug:expr, $apply:expr) => {
        struct $name {
            child: AnyElement,
            controller: AnimationController,
            animating: Cell<bool>,
        }

        unsafe impl Send for $name {}
        unsafe impl Sync for $name {}

        impl Drawable for $name {
            fn draw(&self, ctx: &BuildContext) {
                let now = AnimInstant::now();
                let curved_value = {
                    let v = self.controller.tick(now);
                    self.animating.set(self.controller.is_animating());
                    v
                };

                ctx.canvas.save();
                $apply(ctx, curved_value);
                self.child.draw(ctx);
                ctx.canvas.restore();

                if self.animating.get() {
                    request_next_frame();
                }
            }
        }

        impl VisitorElement for $name {
            fn debug_name(&self) -> &'static str {
                $debug
            }
            fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
                visitor(self.child.as_ref());
            }
        }

        impl EventElement for $name {
            fn on_event(&self, event: &ElementEvent) -> EventResult {
                self.child.on_event(event)
            }
            fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
                visitor(self.child.as_ref());
            }
        }

        impl Rebuildable for $name {
            fn rebuild_if_dirty(&self, ctx: &BuildContext) {
                self.child.rebuild_if_dirty(ctx);
            }
        }

        impl LayoutElement for $name {
            fn pos(&self) -> Option<Vec2d> {
                self.child.pos()
            }
            fn size(&self) -> Option<Size> {
                self.child.size()
            }
            fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
                self.child.computed_size(ctx)
            }
            fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
                self.child.content_size(ctx)
            }
            fn get_size_from_child(&self) -> Option<Size> {
                self.child.get_size_from_child()
            }
            fn invalidate_layout(&self) {
                self.child.invalidate_layout();
            }
        }
    };
}

impl_transition_element!(
    FadeTransitionElement,
    "FadeTransitionElement",
    |ctx: &BuildContext, v: f32| {
        ctx.canvas.set_alpha(v.clamp(0.0, 1.0));
    }
);

// ---------------------------------------------------------------------------
// SlideTransition
// ---------------------------------------------------------------------------

/// Animates a slide offset for its child.
///
/// The child is translated by `offset * (1.0 - controller_value)` pixels.
/// At value `0.0` the child is at the offset position; at `1.0` it is at its
/// natural position.
pub struct SlideTransition<T: Widget + 'static> {
    pub position: AnimationController,
    /// The offset direction in pixels at value 0.0. At value 1.0 the child is
    /// at (0,0).
    pub offset: (f32, f32),
    pub child: T,
}

impl<T: Widget> SlideTransition<T> {
    /// Creates a slide transition from the pixel `offset` to the child's
    /// natural position, without starting or resetting `position`.
    pub fn new(position: AnimationController, offset: (f32, f32), child: T) -> Self {
        Self {
            position,
            offset,
            child,
        }
    }
}

impl<T: Widget + 'static> Widget for SlideTransition<T> {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        let child = self.child.to_element(ctx);
        let controller = self.position.clone();
        let animating = Cell::new(self.position.is_animating());
        let offset = self.offset;
        SlideTransitionElement {
            child,
            controller,
            animating,
            offset,
        }
        .boxed()
    }
}

struct SlideTransitionElement {
    child: AnyElement,
    controller: AnimationController,
    animating: Cell<bool>,
    offset: (f32, f32),
}

unsafe impl Send for SlideTransitionElement {}
unsafe impl Sync for SlideTransitionElement {}

impl Drawable for SlideTransitionElement {
    fn draw(&self, ctx: &BuildContext) {
        let now = AnimInstant::now();
        let curved_value = {
            let v = self.controller.tick(now);
            self.animating.set(self.controller.is_animating());
            v
        };

        // At value 0.0: child is fully offset. At value 1.0: child is at natural
        // position.
        let remaining = 1.0 - curved_value;
        let dx = self.offset.0 * remaining;
        let dy = self.offset.1 * remaining;

        ctx.canvas.save();
        ctx.canvas.translate((dx, dy).into());
        self.child.draw(ctx);
        ctx.canvas.restore();

        if self.animating.get() {
            request_next_frame();
        }
    }
}

impl VisitorElement for SlideTransitionElement {
    fn debug_name(&self) -> &'static str {
        "SlideTransitionElement"
    }
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl EventElement for SlideTransitionElement {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        self.child.on_event(event)
    }
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl Rebuildable for SlideTransitionElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.child.rebuild_if_dirty(ctx);
    }
}

impl LayoutElement for SlideTransitionElement {
    fn pos(&self) -> Option<Vec2d> {
        self.child.pos()
    }
    fn size(&self) -> Option<Size> {
        self.child.size()
    }
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.computed_size(ctx)
    }
    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.content_size(ctx)
    }
    fn get_size_from_child(&self) -> Option<Size> {
        self.child.get_size_from_child()
    }
    fn invalidate_layout(&self) {
        self.child.invalidate_layout();
    }
}

// ---------------------------------------------------------------------------
// ScaleTransition
// ---------------------------------------------------------------------------

/// Animates uniform scale for its child based on the controller's value.
///
/// A value of `1.0` is the child's natural size. The drawing transform is
/// centered in the current box constraints; layout itself is unchanged.
pub struct ScaleTransition<T: Widget + 'static> {
    pub scale: AnimationController,
    pub child: T,
}

impl<T: Widget> ScaleTransition<T> {
    /// Creates a centered scale transition without starting or resetting
    /// `scale`.
    pub fn new(scale: AnimationController, child: T) -> Self {
        Self { scale, child }
    }
}

impl<T: Widget + 'static> Widget for ScaleTransition<T> {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        let child = self.child.to_element(ctx);
        let controller = self.scale.clone();
        let animating = Cell::new(self.scale.is_animating());
        ScaleTransitionElement {
            child,
            controller,
            animating,
        }
        .boxed()
    }
}

struct ScaleTransitionElement {
    child: AnyElement,
    controller: AnimationController,
    animating: Cell<bool>,
}

unsafe impl Send for ScaleTransitionElement {}
unsafe impl Sync for ScaleTransitionElement {}

impl Drawable for ScaleTransitionElement {
    fn draw(&self, ctx: &BuildContext) {
        let now = AnimInstant::now();
        let curved_value = {
            let v = self.controller.tick(now);
            self.animating.set(self.controller.is_animating());
            v
        };

        let cx = ctx.box_constraint.max_width / 2.0;
        let cy = ctx.box_constraint.max_height / 2.0;

        ctx.canvas.save();
        ctx.canvas.translate((cx, cy).into());
        ctx.canvas.scale(curved_value, curved_value);
        ctx.canvas.translate((-cx, -cy).into());
        self.child.draw(ctx);
        ctx.canvas.restore();

        if self.animating.get() {
            request_next_frame();
        }
    }
}

impl VisitorElement for ScaleTransitionElement {
    fn debug_name(&self) -> &'static str {
        "ScaleTransitionElement"
    }
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl EventElement for ScaleTransitionElement {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        self.child.on_event(event)
    }
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl Rebuildable for ScaleTransitionElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.child.rebuild_if_dirty(ctx);
    }
}

impl LayoutElement for ScaleTransitionElement {
    fn pos(&self) -> Option<Vec2d> {
        self.child.pos()
    }
    fn size(&self) -> Option<Size> {
        self.child.size()
    }
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.computed_size(ctx)
    }
    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.content_size(ctx)
    }
    fn get_size_from_child(&self) -> Option<Size> {
        self.child.get_size_from_child()
    }
    fn invalidate_layout(&self) {
        self.child.invalidate_layout();
    }
}

// ---------------------------------------------------------------------------
// RotationTransition
// ---------------------------------------------------------------------------

/// Animates rotation (in full turns) for its child based on the controller's
/// value.
///
/// At value 0.0 the child is at 0 rotation; at 1.0 it has completed one full
/// turn (2π radians).
pub struct RotationTransition<T: Widget + 'static> {
    pub turns: AnimationController,
    pub child: T,
}

impl<T: Widget> RotationTransition<T> {
    /// Creates a centered rotation transition without starting or resetting
    /// `turns`.
    pub fn new(turns: AnimationController, child: T) -> Self {
        Self { turns, child }
    }
}

impl<T: Widget + 'static> Widget for RotationTransition<T> {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        let child = self.child.to_element(ctx);
        let controller = self.turns.clone();
        let animating = Cell::new(self.turns.is_animating());
        RotationTransitionElement {
            child,
            controller,
            animating,
        }
        .boxed()
    }
}

struct RotationTransitionElement {
    child: AnyElement,
    controller: AnimationController,
    animating: Cell<bool>,
}

unsafe impl Send for RotationTransitionElement {}
unsafe impl Sync for RotationTransitionElement {}

impl Drawable for RotationTransitionElement {
    fn draw(&self, ctx: &BuildContext) {
        let now = AnimInstant::now();
        let curved_value = {
            let v = self.controller.tick(now);
            self.animating.set(self.controller.is_animating());
            v
        };

        // Convert turns to radians: 1.0 turn = 2π radians
        let angle = curved_value * std::f32::consts::TAU;
        let cx = ctx.box_constraint.max_width / 2.0;
        let cy = ctx.box_constraint.max_height / 2.0;

        ctx.canvas.save();
        ctx.canvas.translate((cx, cy).into());
        ctx.canvas.rotate(angle);
        ctx.canvas.translate((-cx, -cy).into());
        self.child.draw(ctx);
        ctx.canvas.restore();

        if self.animating.get() {
            request_next_frame();
        }
    }
}

impl VisitorElement for RotationTransitionElement {
    fn debug_name(&self) -> &'static str {
        "RotationTransitionElement"
    }
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl EventElement for RotationTransitionElement {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        self.child.on_event(event)
    }
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl Rebuildable for RotationTransitionElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.child.rebuild_if_dirty(ctx);
    }
}

impl LayoutElement for RotationTransitionElement {
    fn pos(&self) -> Option<Vec2d> {
        self.child.pos()
    }
    fn size(&self) -> Option<Size> {
        self.child.size()
    }
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.computed_size(ctx)
    }
    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.content_size(ctx)
    }
    fn get_size_from_child(&self) -> Option<Size> {
        self.child.get_size_from_child()
    }
    fn invalidate_layout(&self) {
        self.child.invalidate_layout();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::curve::Curve;
    use crate::widgets::test_frame_requester;

    struct TestWidget;

    struct TestElement;

    impl Drawable for TestElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl EventElement for TestElement {}

    impl LayoutElement for TestElement {}

    impl Rebuildable for TestElement {}

    impl VisitorElement for TestElement {
        fn debug_name(&self) -> &'static str {
            "TestElement"
        }
    }

    impl Widget for TestWidget {
        fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
            TestElement.boxed()
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn dummy_async_handle() -> tokio::runtime::Handle {
        static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
        });
        let _guard = runtime.enter();
        tokio::runtime::Handle::current()
    }

    fn dummy_build_context() -> BuildContext<'static> {
        let canvas = {
            let leaked: &'static aimer_canvas::InnerCanvas =
                Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::Canvas::new(leaked)
        };
        BuildContext {
            parent_size: Default::default(),
            canvas,
            scale: 1.0,
            parent_pos: Default::default(),
            cursor_pos: Default::default(),
            box_constraint: Default::default(),
            visible_rect: None,
            window: WindowHandle::headless(Default::default(), 1.0),
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: dummy_async_handle(),
            inherited_states: Default::default(),
        }
    }

    fn controller() -> AnimationController {
        let controller = AnimationController::with_millis(100, Curve::Linear);
        controller.forward_from_first_tick();
        controller
    }

    fn assert_defers_next_frame(widget: impl Widget + 'static) {
        test_frame_requester::reset();
        let ctx = dummy_build_context();
        let element = widget.to_element(&ctx);

        element.draw(&ctx);

        assert_eq!(test_frame_requester::count(), 1);
        assert!(!ctx.window.take_redraw_request());
    }

    #[test]
    #[cfg(not(target_os = "ios"))]
    fn active_explicit_transitions_defer_their_next_frame_request() {
        test_frame_requester::install();

        assert_defers_next_frame(FadeTransition::new(controller(), TestWidget));
        assert_defers_next_frame(SlideTransition::new(
            controller(),
            (10.0, 10.0),
            TestWidget,
        ));
        assert_defers_next_frame(ScaleTransition::new(controller(), TestWidget));
        assert_defers_next_frame(RotationTransition::new(controller(), TestWidget));
    }
}
