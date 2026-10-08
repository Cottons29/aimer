use std::sync::OnceLock;

use aimer_attribute::BoxConstraint;
use aimer_widget::base::{BuildContext, ResolvedSize, Vec2d, WindowHandle};
use aimer_widget::{
    AnyElement, Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement,
    Widget,
};

use super::{Animated, AnimatedBuilder, AnimationEffect};
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

struct StableLeaf(f32);

impl Widget for StableLeaf {
    fn to_element(self, _context: &BuildContext) -> AnyElement {
        StableElement(self.0).boxed()
    }
}

impl aimer_widget::PortableWidget for StableLeaf {}

struct StableElement(f32);

impl Drawable for StableElement {
    fn update(&self, _context: &BuildContext) {}

}

impl EventElement for StableElement {}

impl LayoutElement for StableElement {
    fn computed_size(&self, _context: &BuildContext) -> ResolvedSize {
        ResolvedSize {
            width: self.0,
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
        StableLeaf(10.0),
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
fn animated_builder_updates_its_child_while_preserving_retained_identity() {
    let controller = AnimationController::with_millis(1000, Curve::Linear);
    controller.set_value(0.0);
    controller.forward_from_first_tick();
    let context = context();
    let values = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let built_values = values.clone();
    let element = AnimatedBuilder::new(controller.clone(), move |value| {
        built_values.borrow_mut().push(value);
        StableLeaf(10.0 + value * 10.0)
    })
        .to_element(&context);

    element.update(&context);
    let mut original_child = None;
    element.visit_children(&mut |child| original_child = child.element_id());
    assert!(original_child.is_some());
    assert_eq!(element.computed_size(&context).width, 10.0);

    controller.set_value(0.5);
    // Reset the deferred clock so this frame samples exactly the supplied
    // value, while playback keeps the builder on the dirty-path index.
    controller.forward_from_first_tick();
    element.update(&context);

    let mut rebuilt_child = None;
    element.visit_retained_v2_children(&mut |_, child| rebuilt_child = child.element_id());
    assert!(rebuilt_child.is_some());
    assert_eq!(original_child, rebuilt_child, "reconciliation keeps the child's identity");
    assert_eq!(element.computed_size(&context).width, 15.0);
    assert_eq!(*values.borrow(), [0.0, 0.5]);

    controller.forward_from_first_tick();
    element.update(&context);
    let mut unchanged_child = None;
    element.visit_retained_v2_children(&mut |_, child| unchanged_child = child.element_id());
    assert_eq!(unchanged_child, rebuilt_child);
    assert_eq!(*values.borrow(), [0.0, 0.5], "a stable value reuses its child");
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
