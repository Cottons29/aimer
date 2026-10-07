use std::sync::OnceLock;

use aimer_attribute::BoxConstraint;
use aimer_widget::base::{BuildContext, ResolvedSize, Vec2d, WindowHandle};
use aimer_widget::{
    AnyElement, Drawable, Element, EventElement, LayoutElement, PaintDamageTracker, Rebuildable,
    VisitorElement, Widget,
};

use super::damage::{mark_bounded_child_damage, mark_bounded_crossfade_damage};
use super::{Animated, AnimatedBuilder, AnimationEffect, FadeTransition, SlideTransition};
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
                .expect("animation damage test runtime should build")
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

    fn paint(&self, _context: &BuildContext) {}

    fn is_paint_stable(&self) -> bool {
        true
    }

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

    fn is_layout_stable(&self) -> bool {
        true
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

struct VariableBoundedElement {
    size: std::rc::Rc<std::cell::Cell<ResolvedSize>>,
}

impl Drawable for VariableBoundedElement {
    fn update(&self, _context: &BuildContext) {}

    fn is_paint_bounded(&self) -> bool {
        true
    }
}

impl EventElement for VariableBoundedElement {}

impl LayoutElement for VariableBoundedElement {
    fn computed_size(&self, _context: &BuildContext) -> ResolvedSize {
        self.size.get()
    }
}

impl Rebuildable for VariableBoundedElement {}

impl VisitorElement for VariableBoundedElement {
    fn debug_name(&self) -> &'static str {
        "VariableBoundedElement"
    }
}

#[test]
fn stable_slide_marks_one_union_of_previous_and_current_bounds() {
    let controller = AnimationController::with_millis(1000, Curve::Linear);
    controller.set_value(0.0);
    let context = context();
    let element = SlideTransition::new(controller.clone(), (20.0, 0.0), StableLeaf)
        .to_element(&context);

    let _ = draw_frame!(element.as_ref(), &context);
    controller.set_value(1.0);
    let damage = draw_frame!(element.as_ref(), &context);

    assert!(!damage.is_full());
    assert_eq!(damage.regions().len(), 1);
    let region = damage.regions()[0];
    assert_eq!(region.x, 0);
    assert_eq!(region.y, 0);
    assert_eq!(region.width, 32);
    assert_eq!(region.height, 12);
}

#[test]
fn stable_fade_marks_damage_when_alpha_changes_without_bounds_change() {
    let controller = AnimationController::with_millis(1000, Curve::Linear);
    controller.set_value(0.0);
    let context = context();
    let element = FadeTransition::new(controller.clone(), StableLeaf).to_element(&context);

    let _ = draw_frame!(element.as_ref(), &context);
    controller.set_value(0.5);
    let damage = draw_frame!(element.as_ref(), &context);

    assert!(!damage.is_full());
    assert_eq!(damage.regions().len(), 1);
    assert_eq!(damage.regions()[0].width, 12);
    assert_eq!(damage.regions()[0].height, 12);
}

#[test]
fn stable_idle_animation_does_not_add_damage_when_nothing_changes() {
    let controller = AnimationController::with_millis(1000, Curve::Linear);
    controller.set_value(0.0);
    let context = context();
    let element = FadeTransition::new(controller, StableLeaf).to_element(&context);

    let _ = draw_frame!(element.as_ref(), &context);
    let damage = draw_frame!(element.as_ref(), &context);

    assert!(damage.is_empty());
}

#[test]
fn opacity_over_unknown_content_forces_full_damage() {
    let controller = AnimationController::with_millis(1000, Curve::Linear);
    controller.set_value(0.0);
    let context = context();
    let element = FadeTransition::new(controller.clone(), UnknownLeaf).to_element(&context);

    let _ = draw_frame!(element.as_ref(), &context);
    controller.set_value(0.5);
    let damage = draw_frame!(element.as_ref(), &context);

    assert!(damage.is_full());
}

#[test]
fn bounded_dynamic_child_marks_the_union_of_changed_sizes() {
    let context = context();
    let size = std::rc::Rc::new(std::cell::Cell::new(ResolvedSize {
        width: 10.0,
        height: 10.0,
    }));
    let element = VariableBoundedElement { size: size.clone() };
    let tracker = PaintDamageTracker::new();

    aimer_widget::begin_paint_frame(FRAME_WIDTH, FRAME_HEIGHT);
    mark_bounded_child_damage(&tracker, &context, &element, true);
    let _ = aimer_widget::take_paint_frame_damage(FRAME_WIDTH, FRAME_HEIGHT);

    size.set(ResolvedSize {
        width: 20.0,
        height: 14.0,
    });
    aimer_widget::begin_paint_frame(FRAME_WIDTH, FRAME_HEIGHT);
    mark_bounded_child_damage(&tracker, &context, &element, true);
    let damage = aimer_widget::take_paint_frame_damage(FRAME_WIDTH, FRAME_HEIGHT);

    assert!(!damage.is_full());
    assert_eq!(damage.regions().len(), 1);
    assert_eq!(
        (damage.regions()[0].width, damage.regions()[0].height),
        (22, 16)
    );
}

#[test]
fn bounded_damage_keeps_unknown_children_on_the_full_frame_fallback() {
    let context = context();
    let tracker = PaintDamageTracker::new();
    let element = UnknownElement;

    aimer_widget::begin_paint_frame(FRAME_WIDTH, FRAME_HEIGHT);
    mark_bounded_child_damage(&tracker, &context, &element, true);
    let damage = aimer_widget::take_paint_frame_damage(FRAME_WIDTH, FRAME_HEIGHT);

    assert!(damage.is_full());
}

#[test]
fn bounded_crossfade_marks_the_union_of_current_and_outgoing_children() {
    let context = context();
    let tracker = PaintDamageTracker::new();
    let current = VariableBoundedElement {
        size: std::rc::Rc::new(std::cell::Cell::new(ResolvedSize {
            width: 20.0,
            height: 14.0,
        })),
    };
    let old = VariableBoundedElement {
        size: std::rc::Rc::new(std::cell::Cell::new(ResolvedSize {
            width: 30.0,
            height: 10.0,
        })),
    };

    aimer_widget::begin_paint_frame(FRAME_WIDTH, FRAME_HEIGHT);
    mark_bounded_crossfade_damage(&tracker, &context, &current, Some(&old), true);
    let _ = aimer_widget::take_paint_frame_damage(FRAME_WIDTH, FRAME_HEIGHT);

    aimer_widget::begin_paint_frame(FRAME_WIDTH, FRAME_HEIGHT);
    mark_bounded_crossfade_damage(&tracker, &context, &current, Some(&old), true);
    let damage = aimer_widget::take_paint_frame_damage(FRAME_WIDTH, FRAME_HEIGHT);

    assert!(!damage.is_full());
    assert_eq!(damage.regions().len(), 1);
    assert_eq!(
        (damage.regions()[0].width, damage.regions()[0].height),
        (32, 16)
    );
}

#[test]
fn animated_exposes_its_child_to_the_retained_render_tree() {
    // The render tree is built from `visit_retained_v2_children`. An element
    // that draws a child without exposing it there leaves that child without a
    // render node, so its paint is silently dropped under retained presentation.
    let controller = AnimationController::with_millis(1000, Curve::Linear);
    let context = context();
    let element = Animated::new(
        controller,
        AnimationEffect::Rotate { from: 0.0, to: 1.0 },
        StableLeaf,
    )
    .to_element(&context);

    let mut visited = 0;
    element.as_ref().visit_children(&mut |_| visited += 1);
    assert_eq!(visited, 1, "the animated child must be visible to paint");

    let mut retained = 0;
    element
        .as_ref()
        .visit_retained_v2_children(&mut |_, _| retained += 1);
    assert_eq!(retained, 1, "the animated child must get a render node");
}

#[test]
fn non_finite_rotation_forces_full_damage() {
    let controller = AnimationController::with_millis(1000, Curve::Linear);
    controller.set_value(0.0);
    let context = context();
    let element = Animated::new(
        controller.clone(),
        AnimationEffect::Rotate {
            from: 0.0,
            to: f32::INFINITY,
        },
        StableLeaf,
    )
    .to_element(&context);

    let _ = draw_frame!(element.as_ref(), &context);
    controller.set_value(1.0);
    let damage = draw_frame!(element.as_ref(), &context);

    assert!(damage.is_full());
}

#[test]
fn zero_sized_current_bounds_clear_the_previous_footprint() {
    let context = context();
    context.canvas.translate((20.0, 0.0).into());
    let tracker = PaintDamageTracker::new();

    aimer_widget::begin_paint_frame(FRAME_WIDTH, FRAME_HEIGHT);
    tracker.mark_current_bounds(
        &context,
        ResolvedSize {
            width: 10.0,
            height: 10.0,
        },
        true,
    );
    let _ = aimer_widget::take_paint_frame_damage(FRAME_WIDTH, FRAME_HEIGHT);

    aimer_widget::begin_paint_frame(FRAME_WIDTH, FRAME_HEIGHT);
    tracker.mark_current_bounds(
        &context,
        ResolvedSize {
            width: 0.0,
            height: 10.0,
        },
        false,
    );
    let damage = aimer_widget::take_paint_frame_damage(FRAME_WIDTH, FRAME_HEIGHT);

    assert!(!damage.is_full());
    assert_eq!(damage.regions().len(), 1);
    let region = damage.regions()[0];
    assert_eq!((region.x, region.y, region.width, region.height), (18, 0, 14, 12));
}

#[test]
fn animated_builder_output_changes_force_full_damage() {
    let controller = AnimationController::with_millis(1000, Curve::Linear);
    controller.set_value(0.0);
    let context = context();
    let element = AnimatedBuilder::new(controller.clone(), |_value| StableLeaf)
        .to_element(&context);

    let _ = draw_frame!(element.as_ref(), &context);
    controller.set_value(0.5);
    let damage = draw_frame!(element.as_ref(), &context);

    assert!(damage.is_full());
}

#[test]
fn animated_builder_paints_locally_whatever_its_child_is() {
    // The builder records nothing itself: its rebuilt child owns its render
    // nodes and decides its own paint source. Declining local paint because the
    // child is not "bounded" would put the whole subtree on the legacy path.
    let controller = AnimationController::with_millis(1000, Curve::Linear);
    let context = context();
    let element = AnimatedBuilder::new(controller, |_value| UnknownLeaf).to_element(&context);

    assert!(element.as_ref().can_paint_local_v2(&context));
    assert!(element.as_ref().sync_local_v2_state(&context));
}
