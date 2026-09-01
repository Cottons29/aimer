#[path = "../src/range_controls_example.rs"]
mod range_controls_example;
#[path = "../src/theme.rs"]
mod theme;

use aimer::{Element, Widget};
use aimer::events::element::ElementEvent;
use aimer::events::pointer::{PointerButton, PointerInfo};
use aimer::widget::{
    BuildContext, EventDispatcher, LayoutElement, Rebuildable, StatefulElement,
};
use aimer::{ResolvedSize, Vec2d};
use aimer_attribute::BoxConstraint;
use aimer_widget::base::WindowHandle;

fn context() -> BuildContext<'static> {
    let inner = Box::leak(Box::new(aimer::canvas::InnerCanvas::new()));
    let mut ctx = BuildContext::new(
        aimer::canvas::Canvas::new(inner),
        ResolvedSize {
            width: 640.0,
            height: 720.0,
        },
        1.0,
        Default::default(),
        Default::default(),
        WindowHandle::headless(Default::default(), 1.0),
        tokio::runtime::Handle::current(),
    );
    ctx.box_constraint = BoxConstraint {
        min_width: 0.0,
        min_height: 0.0,
        max_width: 640.0,
        max_height: 720.0,
    };
    ctx
}

fn first_slider_bounds(
    element: &dyn aimer::Element,
) -> Option<(Vec2d, ResolvedSize)> {
    if element.debug_name() == "Slider" {
        return element.pos_start_end().map(|(start, end)| {
            (
                start,
                ResolvedSize {
                    width: end.x - start.x,
                    height: end.y - start.y,
                },
            )
        });
    }

    let mut result = None;
    element.visit_children(&mut |child| {
        if result.is_none() {
            result = first_slider_bounds(child);
        }
    });
    result
}

fn count_debug_name(element: &dyn Element, name: &'static str) -> usize {
    let mut count = usize::from(element.debug_name() == name);
    element.visit_children(&mut |child| {
        count += count_debug_name(child, name);
    });
    count
}

fn range_stateful(element: &dyn Element) -> Option<&StatefulElement> {
    if element.debug_name() == "RangeControlsExample"
        && let Some(stateful) = element
            .option_any()
            .and_then(|value| value.downcast_ref::<StatefulElement>())
    {
        return Some(stateful);
    }

    let mut result = None;
    element.visit_children(&mut |child| {
        if result.is_none() {
            result = range_stateful(child);
        }
    });
    result
}

#[test]
fn range_controls_example_exposes_a_public_widget_constructor() {
    let example = range_controls_example::RangeControlsExample::new();

    assert_eq!(example.debug_name(), "RangeControlsExample");
}

#[test]
fn range_controls_example_covers_keyboard_and_invalid_range_states() {
    assert_eq!(range_controls_example::keyboard_sample(55.0), 56.0);
    assert_eq!(range_controls_example::invalid_sample(), (true, true));
}

#[tokio::test]
async fn range_controls_example_composes_theme_visual_parts() {
    let ctx = context();
    let element = theme::provide(range_controls_example::RangeControlsExample::new())
        .to_element(&ctx);
    element.layout(&ctx);

    // Retained child proxies and their raw visual elements both report the
    // visual widget's name, so each mounted slot appears at least once here.
    assert!(count_debug_name(element.as_ref(), "SliderTrail") >= 3);
    assert!(count_debug_name(element.as_ref(), "SliderThumb") >= 4);
}

#[tokio::test]
async fn range_controls_example_marks_itself_dirty_after_a_slider_drag() {
    let ctx = context();
    let element = theme::provide(range_controls_example::RangeControlsExample::new())
        .to_element(&ctx);
    element.layout(&ctx);
    element.draw(&ctx);

    let (pos, size) = first_slider_bounds(element.as_ref()).expect("example has a slider");
    let down = PointerInfo::mouse(
        Vec2d {
            x: pos.x + size.width * 0.2,
            y: pos.y + size.height * 0.5,
        },
        PointerButton::Primary,
    );
    let moved = down.at(Vec2d {
        x: pos.x + size.width * 0.7,
        y: down.pos.y,
    });
    let mut dispatcher = EventDispatcher::new();

    assert!(dispatcher
        .dispatch(element.as_ref(), down.pos, &ElementEvent::PointerDown(down))
        .is_consumed());
    assert!(dispatcher
        .dispatch(element.as_ref(), moved.pos, &ElementEvent::PointerMove(moved))
        .is_consumed());
    assert!(dispatcher
        .dispatch(element.as_ref(), moved.pos, &ElementEvent::PointerUp(moved))
        .is_consumed());
    let stateful = range_stateful(element.as_ref());
    assert!(
        stateful.is_some_and(StatefulElement::is_dirty),
        "a controlled drag must invalidate its owner"
    );

    element.rebuild_if_dirty(&ctx);
    let stateful = range_stateful(element.as_ref());
    assert!(
        !stateful.expect("the example root is stateful").is_dirty(),
        "the queued drag value should be applied during the next rebuild"
    );
}
