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
#[derive(aimer_macro::PortableWidget)]
#[portable_widget(
    id = "aimer_animation::FadeTransition",
    schema_only,
    manual_lowering
)]
pub struct FadeTransition<T: Widget + 'static> {
    #[portable_skip]
    pub opacity: AnimationController,
    #[portable_child]
    pub child: T,
}

impl<T: Widget> FadeTransition<T> {
    /// Creates an opacity transition without starting or resetting `opacity`.
    pub fn new(opacity: AnimationController, child: T) -> Self {
        Self { opacity, child }
    }
}

impl<T: Widget + 'static> Widget for FadeTransition<T> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let child = self.child.to_element(ctx);
        let controller = self.opacity.clone();
        let animating = Cell::new(self.opacity.is_animating());
        FadeTransitionElement {
            child,
            controller,
            animating,
            damage: aimer_widget::PaintDamageTracker::new(),
            last_value: Cell::new(None),
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
            damage: aimer_widget::PaintDamageTracker,
            last_value: Cell<Option<u32>>,
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
                let visual_changed = crate::widgets::damage::sample_changed(
                    &self.last_value,
                    curved_value,
                );
                crate::widgets::damage::mark_bounded_animation_damage(
                    &self.damage,
                    ctx,
                    self.child.as_ref(),
                    curved_value,
                    visual_changed,
                );
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
#[derive(aimer_macro::PortableWidget)]
#[portable_widget(
    id = "aimer_animation::SlideTransition",
    schema_only,
    manual_lowering
)]
pub struct SlideTransition<T: Widget + 'static> {
    #[portable_skip]
    pub position: AnimationController,
    /// The offset direction in pixels at value 0.0. At value 1.0 the child is
    /// at (0,0).
    #[portable_skip]
    pub offset: (f32, f32),
    #[portable_child]
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
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let child = self.child.to_element(ctx);
        let controller = self.position.clone();
        let animating = Cell::new(self.position.is_animating());
        let offset = self.offset;
        SlideTransitionElement {
            child,
            controller,
            animating,
            offset,
            damage: aimer_widget::PaintDamageTracker::new(),
            last_value: Cell::new(None),
        }
        .boxed()
    }
}

struct SlideTransitionElement {
    child: AnyElement,
    controller: AnimationController,
    animating: Cell<bool>,
    offset: (f32, f32),
    damage: aimer_widget::PaintDamageTracker,
    last_value: Cell<Option<u32>>,
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
        let visual_changed = crate::widgets::damage::sample_changed(&self.last_value, curved_value);
        crate::widgets::damage::mark_bounded_animation_damage(
            &self.damage,
            ctx,
            self.child.as_ref(),
            curved_value,
            visual_changed,
        );
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
#[derive(aimer_macro::PortableWidget)]
#[portable_widget(
    id = "aimer_animation::ScaleTransition",
    schema_only,
    manual_lowering
)]
pub struct ScaleTransition<T: Widget + 'static> {
    #[portable_skip]
    pub scale: AnimationController,
    #[portable_child]
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
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let child = self.child.to_element(ctx);
        let controller = self.scale.clone();
        let animating = Cell::new(self.scale.is_animating());
        ScaleTransitionElement {
            child,
            controller,
            animating,
            damage: aimer_widget::PaintDamageTracker::new(),
            last_value: Cell::new(None),
        }
        .boxed()
    }
}

struct ScaleTransitionElement {
    child: AnyElement,
    controller: AnimationController,
    animating: Cell<bool>,
    damage: aimer_widget::PaintDamageTracker,
    last_value: Cell<Option<u32>>,
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
        let visual_changed = crate::widgets::damage::sample_changed(&self.last_value, curved_value);
        crate::widgets::damage::mark_bounded_animation_damage(
            &self.damage,
            ctx,
            self.child.as_ref(),
            curved_value,
            visual_changed,
        );
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

/// Animates rotation for its child based on the controller's value.
///
/// By default, a controller value of 0.0 means 0 turns and a value of 1.0
/// means one full turn (2π radians). Use [`Self::turn_range`] when the
/// transition should cover a different range, such as a disclosure chevron's
/// 0.0 to -0.25 turns.
#[derive(aimer_macro::PortableWidget)]
#[portable_widget(
    id = "aimer_animation::RotationTransition",
    schema_only,
    manual_lowering
)]
pub struct RotationTransition<T: Widget + 'static> {
    #[portable_skip]
    pub turns: AnimationController,
    #[portable_skip]
    begin_turns: f32,
    #[portable_skip]
    end_turns: f32,
    #[portable_child]
    pub child: T,
}

impl<T: Widget> RotationTransition<T> {
    /// Creates a centered rotation transition without starting or resetting
    /// `turns`.
    pub fn new(turns: AnimationController, child: T) -> Self {
        Self {
            turns,
            begin_turns: 0.0,
            end_turns: 1.0,
            child,
        }
    }

    /// Sets the rotation range, in full turns, represented by controller
    /// values 0.0 and 1.0.
    ///
    /// Negative values rotate counterclockwise. For example,
    /// `turn_range(0.0, -0.25)` rotates a child 90° counterclockwise as the
    /// controller advances.
    #[inline]
    pub fn turn_range(mut self, begin_turns: f32, end_turns: f32) -> Self {
        self.begin_turns = begin_turns;
        self.end_turns = end_turns;
        self
    }
}

impl<T: Widget + 'static> Widget for RotationTransition<T> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let child = self.child.to_element(ctx);
        let controller = self.turns.clone();
        let animating = Cell::new(self.turns.is_animating());
        RotationTransitionElement {
            child,
            controller,
            begin_turns: self.begin_turns,
            end_turns: self.end_turns,
            animating,
            damage: aimer_widget::PaintDamageTracker::new(),
            last_value: Cell::new(None),
        }
        .boxed()
    }
}

struct RotationTransitionElement {
    child: AnyElement,
    controller: AnimationController,
    begin_turns: f32,
    end_turns: f32,
    animating: Cell<bool>,
    damage: aimer_widget::PaintDamageTracker,
    last_value: Cell<Option<u32>>,
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

        // Convert the configured turn range to radians: 1.0 turn = 2π radians.
        let turns = interpolate_turns(self.begin_turns, self.end_turns, curved_value);
        let angle = turns * std::f32::consts::TAU;
        let cx = ctx.box_constraint.max_width / 2.0;
        let cy = ctx.box_constraint.max_height / 2.0;

        ctx.canvas.save();
        ctx.canvas.translate((cx, cy).into());
        ctx.canvas.rotate(angle);
        ctx.canvas.translate((-cx, -cy).into());
        let visual_changed = crate::widgets::damage::sample_changed(&self.last_value, curved_value);
        crate::widgets::damage::mark_bounded_animation_damage(
            &self.damage,
            ctx,
            self.child.as_ref(),
            curved_value,
            visual_changed,
        );
        self.child.draw(ctx);
        ctx.canvas.restore();

        if self.animating.get() {
            request_next_frame();
        }
    }
}

#[inline]
fn interpolate_turns(begin_turns: f32, end_turns: f32, progress: f32) -> f32 {
    begin_turns + (end_turns - begin_turns) * progress
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

macro_rules! impl_portable_transition_lowering {
    ($transition:ident) => {
        impl<T: Widget + 'static> aimer_widget::PortableWidget for $transition<T> {
            #[cfg(feature = "portable-guest")]
            fn to_portable_node(
                self,
                ctx: &mut aimer_widget::portable::PortableBuildContext,
                source: aimer_widget::portable::SourceFingerprint,
            ) -> Result<
                aimer_widget::portable::PortableNodeId,
                aimer_widget::portable::PortableBuildError,
            > {
                aimer_widget::PortableWidget::to_portable_node(
                    self.child,
                    ctx,
                    source.child(0),
                )
            }
        }
    };
}

impl_portable_transition_lowering!(FadeTransition);
impl_portable_transition_lowering!(SlideTransition);
impl_portable_transition_lowering!(ScaleTransition);
impl_portable_transition_lowering!(RotationTransition);

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
        fn to_element(self, _ctx: &BuildContext) -> AnyElement {
            TestElement.boxed()
        }
    }

    impl aimer_widget::PortableWidget for TestWidget {}

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
        BuildContext::new(
            canvas,
            Default::default(),
            1.0,
            Default::default(),
            Default::default(),
            WindowHandle::headless(Default::default(), 1.0),
            #[cfg(not(target_arch = "wasm32"))]
            dummy_async_handle(),
        )
    }

    fn controller() -> AnimationController {
        let controller = AnimationController::with_millis(100, Curve::Linear);
        controller.forward_from_first_tick();
        controller
    }

    #[test]
    fn rotation_transition_interpolates_a_custom_turn_range() {
        assert_eq!(interpolate_turns(0.0, -0.25, 0.0), 0.0);
        assert_eq!(interpolate_turns(0.0, -0.25, 0.5), -0.125);
        assert_eq!(interpolate_turns(0.0, -0.25, 1.0), -0.25);
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
        assert_defers_next_frame(SlideTransition::new(controller(), (10.0, 10.0), TestWidget));
        assert_defers_next_frame(ScaleTransition::new(controller(), TestWidget));
        assert_defers_next_frame(RotationTransition::new(controller(), TestWidget));
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn portable_transitions_lower_to_their_current_child() {
        use aimer_widget::portable::{
            PortableBuildContext, PortableLimits, PortableWidgetLimits, SourceFingerprint,
            StableId128,
        };
        use aimer_widget::portable::__anteros::{Version, WidgetDocumentView, WIDGET_TEXT};
        use aimer_widget::{AnyElement, PortableWidget};

        struct PortableLeaf;

        impl Widget for PortableLeaf {
            fn to_element(self, _ctx: &BuildContext) -> AnyElement {
                panic!("portable transition test must not enter native element construction")
            }
        }

        impl PortableWidget for PortableLeaf {
            fn to_portable_node(
                self,
                ctx: &mut PortableBuildContext,
                source: SourceFingerprint,
            ) -> Result<
                aimer_widget::portable::PortableNodeId,
                aimer_widget::portable::PortableBuildError,
            > {
                ctx.push_node(WIDGET_TEXT, Version::new(1, 0), None, source, &[], &[])
            }
        }

        fn assert_transparent<W: Widget + 'static>(widget: W) {
            let mut context = PortableBuildContext::new(
                1,
                1,
                PortableWidgetLimits::new(8, 8, 8, 8, 64, 2_048),
                PortableLimits::new(8, 16, 64, 128, 1_024),
            )
            .unwrap();
            let root = widget
                .to_portable_node(
                    &mut context,
                    SourceFingerprint::new(StableId128::from_bytes([9; 16])),
                )
                .unwrap();
            let document = context.finish_document(root).unwrap();
            let bytes = document.encode().unwrap();
            let view = WidgetDocumentView::decode(&bytes, document.model_limits()).unwrap();

            assert_eq!(view.node_count(), 1);
            assert_eq!(
                view.node(view.root_node()).unwrap().widget_type(),
                WIDGET_TEXT
            );
        }

        assert_transparent(FadeTransition::new(controller(), PortableLeaf));
        assert_transparent(SlideTransition::new(
            controller(),
            (10.0, 10.0),
            PortableLeaf,
        ));
        assert_transparent(ScaleTransition::new(controller(), PortableLeaf));
        assert_transparent(RotationTransition::new(controller(), PortableLeaf));
    }
}
