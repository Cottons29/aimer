use std::cell::Cell;

use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_events::window::request_animation_frame;
use aimer_widget::base::*;
use aimer_widget::{
    AnyElement, Drawable, Element, EventElement, EventResult, LayoutElement, PaintDamageTracker,
    Rebuildable, VisitorElement, Widget,
};

use crate::control::controller::AnimationController;
use crate::primitives::time::AnimInstant;

type LegacyPaintEffect = dyn Fn(&BuildContext, f32) + 'static;

enum PaintEffect {
    Noop,
    Opacity,
    Legacy(Box<LegacyPaintEffect>),
}

#[derive(Clone, Copy)]
enum DamageMode {
    Full,
    Bounded,
}

/// Applies a controller-driven paint effect to a retained child.
///
/// `AnimatedPaint` converts `child` to an element exactly once. Each frame it
/// samples the controller, invokes the configured effect, paints that retained
/// element, and schedules another frame while the controller is active. The
/// effect should only configure the current canvas state; it must not paint
/// unrelated content or extend output beyond the bounds promised to the damage
/// tracker.
///
/// Damage is conservative by default: an active or changed animation marks the
/// complete frame. Call [`Self::bounded`] only when the effect keeps all visual
/// output inside the child's current paint bounds and the child reports that it
/// is paint-bounded.
///
/// # Example
/// ```rust
/// use std::time::Duration;
///
/// use aimer_animation::{AnimatedPaint, AnimationController, Curve};
/// use aimer_widget::ErrorWidget;
///
/// let controller = AnimationController::new(Duration::from_millis(250), Curve::Linear);
/// let animated = AnimatedPaint::new(controller, ErrorWidget::new("Loading"))
///     .opacity()
///     .bounded();
/// ```
pub struct AnimatedPaint<T> {
    controller: AnimationController,
    child: T,
    effect: PaintEffect,
    damage_mode: DamageMode,
}

impl<T: Widget> AnimatedPaint<T> {
    /// Creates a retained paint wrapper without starting or resetting the
    /// controller. The default effect leaves the canvas unchanged.
    #[inline]
    pub fn new(controller: AnimationController, child: T) -> Self {
        Self {
            controller,
            child,
            effect: PaintEffect::Noop,
            damage_mode: DamageMode::Full,
        }
    }

    /// Sets the native paint effect applied before the retained child draws.
    ///
    /// The closure is retained by the element and must therefore be `'static`.
    /// It runs on the UI/rendering thread with the controller's curved value.
    #[inline]
    pub fn effect<F>(mut self, effect: F) -> Self
    where
        F: Fn(&BuildContext, f32) + 'static,
    {
        self.effect = PaintEffect::Legacy(Box::new(effect));
        self
    }

    /// Fades the retained child with compositor opacity, keeping its draw list
    /// unchanged while the controller advances.
    #[inline]
    pub fn opacity(mut self) -> Self {
        self.effect = PaintEffect::Opacity;
        self
    }

    /// Enables local damage tracking for effects that stay inside the child's
    /// current paint bounds.
    ///
    /// This is an explicit safety contract. Effects that translate, rotate,
    /// scale outside the wrapper's bounds, add shadows/filters, or otherwise
    /// paint outside the child's bounded footprint must use the default full
    /// damage mode instead.
    #[inline]
    pub fn bounded(mut self) -> Self {
        self.damage_mode = DamageMode::Bounded;
        self
    }
}

impl<T: Widget + 'static> Widget for AnimatedPaint<T> {
    fn debug_name(&self) -> &'static str {
        "AnimatedPaint"
    }

    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        AnimatedPaintElement {
            child: self.child.to_element(ctx),
            controller: self.controller,
            effect: self.effect,
            damage_mode: self.damage_mode,
            damage: PaintDamageTracker::new(),
            last_value: Cell::new(None),
        }
        .boxed()
    }
}

// The portable interface cannot represent an arbitrary native canvas closure.
// The default PortableWidget implementation therefore reports the existing
// unsupported-widget diagnostic when this wrapper is lowered in a guest.
impl<T: Widget + 'static> aimer_widget::PortableWidget for AnimatedPaint<T> {}

/// Retained implementation behind [`AnimatedPaint`].
struct AnimatedPaintElement {
    child: AnyElement,
    controller: AnimationController,
    effect: PaintEffect,
    damage_mode: DamageMode,
    damage: PaintDamageTracker,
    last_value: Cell<Option<u32>>,
}

// Safety: widget rendering and the retained element tree are single-threaded.
unsafe impl Send for AnimatedPaintElement {}
unsafe impl Sync for AnimatedPaintElement {}

impl Drawable for AnimatedPaintElement {
    fn update(&self, ctx: &BuildContext) {
        let curved_value = self.controller.tick(AnimInstant::now());
        let animating = self.controller.is_animating();
        let visual_changed = crate::widgets::damage::sample_changed(
            &self.last_value,
            curved_value,
        );

        ctx.canvas.save();
        match &self.effect {
            PaintEffect::Noop => {}
            PaintEffect::Opacity => ctx.canvas.set_alpha(curved_value),
            PaintEffect::Legacy(effect) => effect(ctx, curved_value),
        }

        match self.damage_mode {
            DamageMode::Full => {
                if visual_changed || animating {
                    self.damage.mark_full();
                }
            }
            DamageMode::Bounded => {
                crate::widgets::damage::mark_bounded_child_damage(
                    &self.damage,
                    ctx,
                    self.child.as_ref(),
                    visual_changed,
                );
            }
        }

        self.child.update(ctx);
        ctx.canvas.restore();

        if animating {
            request_animation_frame();
        }
    }

    #[inline]
    fn can_paint_local_v2(&self, ctx: &BuildContext) -> bool {
        // Opacity is the child node's presentation, so the child's paint extent
        // is irrelevant; only an arbitrary canvas effect needs the legacy path.
        !matches!(&self.effect, PaintEffect::Legacy(_))
            && ctx.scale.is_finite()
            && ctx.scale > 0.0
    }

    #[inline]
    fn paint_local_v2(&self, _ctx: &BuildContext) {}

    fn sync_local_v2_state(&self, ctx: &BuildContext) -> bool {
        let progress = self.controller.tick(AnimInstant::now());
        if !progress.is_finite() {
            return false;
        }
        self.child.sync_paint_geometry(ctx);
        let opacity = match &self.effect {
            PaintEffect::Noop => 1.0,
            PaintEffect::Opacity => progress.clamp(0.0, 1.0),
            PaintEffect::Legacy(_) => return false,
        };
        ctx.set_local_v2_child_presentation_at(
            0,
            aimer_canvas::Mat3::identity(),
            opacity,
        )
    }

    #[inline]
    fn draw_local_v2_compatibility(&self, ctx: &BuildContext) {
        let active = self.controller.is_animating();
        self.child.update(ctx);
        if active {
            request_animation_frame();
        }
    }
}

impl VisitorElement for AnimatedPaintElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "AnimatedPaintElement"
    }
}

impl EventElement for AnimatedPaintElement {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        self.child.on_event(event)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl Rebuildable for AnimatedPaintElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.child.rebuild_if_dirty(ctx);
    }
}

impl LayoutElement for AnimatedPaintElement {
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
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::OnceLock;

    use aimer_attribute::BoxConstraint;
    use aimer_widget::base::{BuildContext, ResolvedSize, Vec2d, WindowHandle};
    use aimer_widget::{
        AnyElement, Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement,
        Widget,
    };

    use super::super::test_frame_requester;
    use super::AnimatedPaint;
    use crate::{AnimationController, Curve};

    const FRAME_WIDTH: u32 = 101;
    const FRAME_HEIGHT: u32 = 101;

    fn runtime_handle() -> tokio::runtime::Handle {
        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        RUNTIME
            .get_or_init(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("animated paint test runtime should build")
            })
            .handle()
            .clone()
    }

    fn context() -> BuildContext<'static> {
        let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
        let mut context = BuildContext::new(
            aimer_canvas::FrameCanvas::new(inner),
            ResolvedSize {
                width: FRAME_WIDTH as f32,
                height: FRAME_HEIGHT as f32,
            },
            1.0,
            Vec2d::default(),
            Vec2d::default(),
            WindowHandle::headless(Default::default(), 1.0),
            runtime_handle(),
        );
        context.box_constraint = BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: FRAME_WIDTH as f32,
            max_height: FRAME_HEIGHT as f32,
        };
        context
    }

    macro_rules! draw_frame {
        ($element:expr, $context:expr) => {{
            aimer_widget::begin_paint_frame(FRAME_WIDTH, FRAME_HEIGHT);
            $element.update($context);
            aimer_widget::take_paint_frame_damage(FRAME_WIDTH, FRAME_HEIGHT)
        }};
    }

    struct StableLeaf;

    impl Widget for StableLeaf {
        fn to_element(self, _context: &BuildContext) -> AnyElement {
            StableElement.boxed()
        }
    }

    impl aimer_widget::PortableWidget for StableLeaf {}

    struct StableElement;

    impl Drawable for StableElement {
        fn update(&self, _context: &BuildContext) {}

        fn is_paint_bounded(&self) -> bool {
            true
        }
    }

    impl EventElement for StableElement {}

    impl LayoutElement for StableElement {
        fn computed_size(&self, _context: &BuildContext) -> ResolvedSize {
            ResolvedSize {
                width: 10.0,
                height: 10.0,
            }
        }
    }

    impl Rebuildable for StableElement {}

    impl VisitorElement for StableElement {
        fn debug_name(&self) -> &'static str {
            "StableElement"
        }
    }

    struct UnknownLeaf;

    impl Widget for UnknownLeaf {
        fn to_element(self, _context: &BuildContext) -> AnyElement {
            UnknownElement.boxed()
        }
    }

    impl aimer_widget::PortableWidget for UnknownLeaf {}

    struct UnknownElement;

    impl Drawable for UnknownElement {
        fn update(&self, _context: &BuildContext) {}
    }

    impl EventElement for UnknownElement {}

    impl LayoutElement for UnknownElement {
        fn computed_size(&self, _context: &BuildContext) -> ResolvedSize {
            ResolvedSize {
                width: 10.0,
                height: 10.0,
            }
        }
    }

    impl Rebuildable for UnknownElement {}

    impl VisitorElement for UnknownElement {
        fn debug_name(&self) -> &'static str {
            "UnknownElement"
        }
    }

    struct CountingWidget {
        builds: Rc<Cell<usize>>,
    }

    impl Widget for CountingWidget {
        fn to_element(self, _context: &BuildContext) -> AnyElement {
            self.builds.set(self.builds.get() + 1);
            StableElement.boxed()
        }
    }

    impl aimer_widget::PortableWidget for CountingWidget {}

    fn controller() -> AnimationController {
        let controller = AnimationController::with_millis(100, Curve::Linear);
        controller.forward_from_first_tick();
        controller
    }

    #[test]
    fn retained_child_is_built_once() {
        let builds = Rc::new(Cell::new(0));
        let context = context();
        let element = AnimatedPaint::new(
            controller(),
            CountingWidget {
                builds: builds.clone(),
            },
        )
        .effect(|ctx, value| ctx.canvas.set_alpha(value))
        .to_element(&context);

        assert_eq!(builds.get(), 1);
        let _ = draw_frame!(element.as_ref(), &context);
        let _ = draw_frame!(element.as_ref(), &context);
        assert_eq!(builds.get(), 1);
    }

    #[test]
    fn effect_receives_the_controller_value() {
        let value = Rc::new(Cell::new(-1.0));
        let observed = value.clone();
        let controller = AnimationController::with_millis(100, Curve::Linear);
        controller.set_value(0.25);
        let context = context();
        let element = AnimatedPaint::new(controller, StableLeaf)
            .effect(move |_ctx, current| observed.set(current))
            .to_element(&context);

        let _ = draw_frame!(element.as_ref(), &context);

        assert!((value.get() - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    #[cfg(not(target_os = "ios"))]
    fn active_animation_requests_the_next_frame() {
        test_frame_requester::install();
        test_frame_requester::reset();
        let context = context();
        let element = AnimatedPaint::new(controller(), StableLeaf).to_element(&context);

        let _ = draw_frame!(element.as_ref(), &context);

        assert_eq!(test_frame_requester::count(), 1);
    }

    #[test]
    fn default_damage_mode_is_conservative() {
        let controller = AnimationController::with_millis(1000, Curve::Linear);
        controller.set_value(0.0);
        let context = context();
        let element = AnimatedPaint::new(controller.clone(), StableLeaf)
            .effect(|ctx, value| ctx.canvas.set_alpha(value))
            .to_element(&context);

        let _ = draw_frame!(element.as_ref(), &context);
        controller.set_value(0.5);
        let damage = draw_frame!(element.as_ref(), &context);

        assert!(damage.is_full());
    }

    #[test]
    fn bounded_damage_uses_the_retained_child_footprint() {
        let controller = AnimationController::with_millis(1000, Curve::Linear);
        controller.set_value(0.0);
        let context = context();
        let element = AnimatedPaint::new(controller.clone(), StableLeaf)
            .effect(|ctx, value| ctx.canvas.set_alpha(value))
            .bounded()
            .to_element(&context);

        let _ = draw_frame!(element.as_ref(), &context);
        controller.set_value(0.5);
        let damage = draw_frame!(element.as_ref(), &context);

        assert!(!damage.is_full());
        assert_eq!(damage.regions().len(), 1);
        assert_eq!(
            (damage.regions()[0].width, damage.regions()[0].height),
            (12, 12)
        );
    }

    #[test]
    fn bounded_damage_falls_back_for_unknown_children() {
        let controller = AnimationController::with_millis(1000, Curve::Linear);
        controller.set_value(0.0);
        let context = context();
        let element = AnimatedPaint::new(controller.clone(), UnknownLeaf)
            .effect(|ctx, value| ctx.canvas.set_alpha(value))
            .bounded()
            .to_element(&context);

        let _ = draw_frame!(element.as_ref(), &context);
        controller.set_value(0.5);
        let damage = draw_frame!(element.as_ref(), &context);

        assert!(damage.is_full());
    }

    #[test]
    fn bounded_idle_animation_does_not_add_damage() {
        let controller = AnimationController::with_millis(1000, Curve::Linear);
        controller.set_value(0.0);
        let context = context();
        let element = AnimatedPaint::new(controller, StableLeaf).bounded().to_element(&context);

        let _ = draw_frame!(element.as_ref(), &context);
        let damage = draw_frame!(element.as_ref(), &context);

        assert!(damage.is_empty());
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn portable_lowering_reports_the_native_only_widget() {
        use aimer_widget::portable::{
            PortableBuildContext, PortableLimits, PortableWidgetLimits, SourceFingerprint,
            StableId128,
        };
        use aimer_widget::PortableWidget;

        let mut context = PortableBuildContext::new(
            1,
            1,
            PortableWidgetLimits::new(8, 8, 8, 8, 64, 2_048),
            PortableLimits::new(8, 16, 64, 128, 1_024),
        )
        .unwrap();
        let result = AnimatedPaint::new(
            AnimationController::with_millis(100, Curve::Linear),
            StableLeaf,
        )
        .to_portable_node(
            &mut context,
            SourceFingerprint::new(StableId128::from_bytes([9; 16])),
        );

        assert!(result.is_err());
    }
}
