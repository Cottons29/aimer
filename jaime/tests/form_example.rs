#[path = "../src/form_example.rs"]
mod form_example;
#[path = "../src/theme.rs"]
mod theme;

use aimer::events::element::ElementEvent;
use aimer::events::pointer::{PointerButton, PointerInfo};
use aimer::widget::{BuildContext, EventDispatcher, LayoutElement, Widget};
use aimer::{Element, ResolvedSize, Vec2d};
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

fn has_text_field_descendant(element: &dyn Element) -> bool {
    if element.debug_name() == "TextField" {
        return true;
    }

    let mut found = false;
    element.visit_children(&mut |child| {
        found |= has_text_field_descendant(child);
    });
    found
}

fn text_field_targets(element: &dyn Element, targets: &mut Vec<(aimer::FocusNode, Vec2d, Vec2d)>) {
    if let (Some(node), Some((start, end))) = (element.focus_node(), element.pos_start_end())
        && has_text_field_descendant(element)
    {
        targets.push((node.clone(), start, end));
    }
    element.visit_children(&mut |child| text_field_targets(child, targets));
}

#[tokio::test]
async fn pressing_each_form_field_focuses_that_field_instead_of_the_last_one() {
    let ctx = context();
    let widget = theme::provide(form_example::form_example());
    let element = widget.to_element(&ctx);
    element.layout(&ctx);
    element.draw(&ctx);

    let mut targets = Vec::new();
    text_field_targets(element.as_ref(), &mut targets);
    assert_eq!(targets.len(), 3, "the example should expose three text fields");

    let mut dispatcher = EventDispatcher::new();
    for (index, (node, start, end)) in targets.iter().enumerate() {
        let pos = Vec2d {
            x: (start.x + end.x) * 0.5,
            y: (start.y + end.y) * 0.5,
        };
        let pointer = PointerInfo::mouse(pos, PointerButton::Primary);
        let result = dispatcher.dispatch(
            element.as_ref(),
            pos,
            &ElementEvent::PointerDown(pointer),
        );

        assert!(result.is_consumed(), "field {index} should consume a press");
        assert!(node.has_focus(), "field {index} should own focus after its press");
        for (other_index, (other, _, _)) in targets.iter().enumerate() {
            if other_index != index {
                assert!(
                    !other.has_focus(),
                    "field {other_index} must lose focus when field {index} is pressed"
                );
            }
        }
    }
}
