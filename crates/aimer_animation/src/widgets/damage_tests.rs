use std::sync::OnceLock;

use aimer_attribute::BoxConstraint;
use aimer_widget::base::{BuildContext, ResolvedSize, Vec2d, WindowHandle};
use aimer_widget::{
    AnyElement, Drawable, Element, EventElement, LayoutElement, PaintDamageTracker, Rebuildable,
    VisitorElement, Widget,
};

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
        aimer_canvas::Canvas::new(inner),
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
        $element.draw($context);
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
    fn draw(&self, _context: &BuildContext) {}

    fn paint(&self, _context: &BuildContext) {}

    fn is_paint_stable(&self) -> bool {
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
    fn draw(&self, _context: &BuildContext) {}
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
