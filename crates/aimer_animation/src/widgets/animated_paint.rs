use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_canvas::Mat3;
use aimer_events::element::ElementEvent;
use aimer_events::window::request_animation_frame;
use aimer_widget::base::*;
use aimer_widget::{
    AnyElement, Drawable, Element, EventElement, EventResult, LayoutElement, Rebuildable,
    VisitorElement, Widget,
};

use crate::control::controller::AnimationController;
use crate::primitives::time::AnimInstant;

/// How [`AnimatedPaint`] presents its child for one controller sample.
///
/// The child keeps its recorded paint; the transform and opacity are applied
/// to the child's render node, so a frame costs no re-recording.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintPresentation {
    /// Maps the child's logical coordinates (relative to its own origin) to
    /// where it is shown. It must be finite.
    pub transform: Mat3,
    /// Multiplies the child's opacity. Values are clamped to `0.0..=1.0`; a
    /// non-finite value paints nothing for that frame.
    pub opacity: f32,
}

impl PaintPresentation {
    /// The child shown unchanged.
    pub const IDENTITY_OPAQUE: Self = Self {
        transform: Mat3::identity(),
        opacity: 1.0,
    };
}

type PresentationFn = dyn Fn(f32) -> PaintPresentation + 'static;

enum PaintEffect {
    Noop,
    Opacity,
    Presentation(Box<PresentationFn>),
}

/// Applies a controller-driven presentation to a retained child.
///
/// `AnimatedPaint` converts `child` to an element exactly once. Each frame it
/// samples the controller, presents the child's render node with the sampled
/// transform and opacity, and schedules another frame while the controller is
/// active. The child is never rebuilt or re-recorded by the animation.
///
/// # Example
/// ```rust
/// use std::time::Duration;
///
/// use aimer_animation::{AnimatedPaint, AnimationController, Curve};
/// use aimer_widget::ErrorWidget;
///
/// let controller = AnimationController::new(Duration::from_millis(250), Curve::Linear);
/// let animated = AnimatedPaint::new(controller, ErrorWidget::new("Loading")).opacity();
/// ```
pub struct AnimatedPaint<T> {
    controller: AnimationController,
    child: T,
    effect: PaintEffect,
}

impl<T: Widget> AnimatedPaint<T> {
    /// Creates a retained paint wrapper without starting or resetting the
    /// controller. The default presents the child unchanged.
    #[inline]
    pub fn new(controller: AnimationController, child: T) -> Self {
        Self {
            controller,
            child,
            effect: PaintEffect::Noop,
        }
    }

    /// Sets the presentation applied to the retained child.
    ///
    /// The closure is retained by the element and must therefore be `'static`.
    /// It runs on the UI/rendering thread with the controller's curved value,
    /// once per frame.
    #[inline]
    pub fn effect<F>(mut self, effect: F) -> Self
    where
        F: Fn(f32) -> PaintPresentation + 'static,
    {
        self.effect = PaintEffect::Presentation(Box::new(effect));
        self
    }

    /// Fades the retained child with compositor opacity, keeping its draw list
    /// unchanged while the controller advances.
    #[inline]
    pub fn opacity(mut self) -> Self {
        self.effect = PaintEffect::Opacity;
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
        }
        .boxed()
    }
}

// The portable interface cannot represent an arbitrary native closure. The
// default PortableWidget implementation therefore reports the existing
// unsupported-widget diagnostic when this wrapper is lowered in a guest.
impl<T: Widget + 'static> aimer_widget::PortableWidget for AnimatedPaint<T> {}

/// Retained implementation behind [`AnimatedPaint`].
struct AnimatedPaintElement {
    child: AnyElement,
    controller: AnimationController,
    effect: PaintEffect,
}

// Safety: widget rendering and the retained element tree are single-threaded.
unsafe impl Send for AnimatedPaintElement {}
unsafe impl Sync for AnimatedPaintElement {}

impl AnimatedPaintElement {
    /// The presentation for one sampled controller value.
    fn presentation(&self, progress: f32) -> PaintPresentation {
        match &self.effect {
            PaintEffect::Noop => PaintPresentation::IDENTITY_OPAQUE,
            PaintEffect::Opacity => PaintPresentation {
                transform: Mat3::identity(),
                opacity: progress.clamp(0.0, 1.0),
            },
            PaintEffect::Presentation(effect) => effect(progress),
        }
    }
}

impl Drawable for AnimatedPaintElement {
    fn update(&self, ctx: &BuildContext) {
        self.controller.tick(AnimInstant::now());
        let animating = self.controller.is_animating();
        self.child.update(ctx);
        if animating {
            request_animation_frame();
        }
    }

    #[inline]
    fn can_paint_local_v2(&self, ctx: &BuildContext) -> bool {
        ctx.scale.is_finite() && ctx.scale > 0.0
    }

    #[inline]
    fn paint_local_v2(&self, _ctx: &BuildContext) {}

    fn sync_local_v2_state(&self, ctx: &BuildContext) -> bool {
        let progress = self.controller.tick(AnimInstant::now());
        if !progress.is_finite() {
            return false;
        }
        self.child.sync_paint_geometry(ctx);
        let presentation = self.presentation(progress);
        if !presentation.opacity.is_finite()
            || !presentation.transform.cols.iter().flatten().all(|value| value.is_finite())
        {
            return false;
        }
        ctx.set_local_v2_child_presentation_at(
            0,
            presentation.transform,
            presentation.opacity.clamp(0.0, 1.0),
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
    use super::{AnimatedPaint, PaintPresentation};
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

    /// Runs the element's state synchronization the way the render-tree sync
    /// does: against a root node that has the element's child as its only child.
    fn sync_state(element: &dyn Element, context: &BuildContext<'_>) -> bool {
        use aimer_cupid::draw_cmd_v2::{Rect, RenderTree};

        let tree = RenderTree::new();
        let root = tree.add_root(Rect::new(0.0, 0.0, 101.0, 101.0)).unwrap();
        tree.add_child(root, Rect::new(0.0, 0.0, 10.0, 10.0)).unwrap();
        let retained = tree.context(root).unwrap();
        context.with_local_v2_paint_context(retained, |context| {
            element.sync_local_v2_state(context)
        })
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
        .opacity()
        .to_element(&context);

        assert_eq!(builds.get(), 1);
        assert!(sync_state(element.as_ref(), &context));
        assert!(sync_state(element.as_ref(), &context));
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
            .effect(move |current| {
                observed.set(current);
                PaintPresentation::IDENTITY_OPAQUE
            })
            .to_element(&context);

        assert!(sync_state(element.as_ref(), &context));

        assert!((value.get() - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn effect_presentation_reaches_the_child_node() {
        let controller = AnimationController::with_millis(100, Curve::Linear);
        controller.set_value(0.5);
        let context = context();
        let shifted = AnimatedPaint::new(controller.clone(), StableLeaf)
            .effect(|value| PaintPresentation {
                transform: aimer_canvas::Mat3::translate(value * 10.0, 0.0),
                opacity: value,
            })
            .to_element(&context);
        assert!(sync_state(shifted.as_ref(), &context));

        // A presentation the render tree cannot show declines instead of
        // painting the child wrongly.
        let invalid = AnimatedPaint::new(controller.clone(), StableLeaf)
            .effect(|_| PaintPresentation {
                transform: aimer_canvas::Mat3::translate(f32::NAN, 0.0),
                opacity: 1.0,
            })
            .to_element(&context);
        assert!(!sync_state(invalid.as_ref(), &context));

        let non_finite_opacity = AnimatedPaint::new(controller, StableLeaf)
            .effect(|_| PaintPresentation {
                transform: aimer_canvas::Mat3::identity(),
                opacity: f32::NAN,
            })
            .to_element(&context);
        assert!(!sync_state(non_finite_opacity.as_ref(), &context));
    }

    #[test]
    #[cfg(not(target_os = "ios"))]
    fn active_animation_requests_the_next_frame() {
        test_frame_requester::install();
        test_frame_requester::reset();
        let context = context();
        let element = AnimatedPaint::new(controller(), StableLeaf).to_element(&context);

        element.update(&context);

        assert_eq!(test_frame_requester::count(), 1);
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
