#[path = "../src/selection_controls_example.rs"]
mod selection_controls_example;
#[path = "../src/theme.rs"]
mod theme;

use std::cell::Cell;
use std::rc::Rc;

use aimer::Widget;
use aimer::selection::{Checkbox, CheckboxValue, ControlAction, InputEvent};

#[test]
fn example_builds_a_live_widget() {
    fn assert_widget(_: impl Widget) {}
    assert_widget(selection_controls_example::selection_controls_example());
}

#[test]
fn the_controlled_pattern_the_example_wires_propagates_a_real_pointer_activation() {
    // Mirrors exactly what the example's `on_change` closures do: a real
    // pointer down/up sequence proposes a value through the callback without
    // mutating the control's own value, and the page's own state is what
    // feeds the proposal back in on the next rebuild.
    let proposed: Rc<Cell<Option<CheckboxValue>>> = Rc::new(Cell::new(None));
    let observed = Rc::clone(&proposed);
    let mut checkbox = Checkbox::new().on_change(move |value| observed.set(Some(value)));

    assert_eq!(
        checkbox.handle_event(InputEvent::PointerDown),
        ControlAction::Pressed
    );
    assert!(matches!(
        checkbox.handle_event(InputEvent::PointerUp { inside: true }),
        ControlAction::Activated(CheckboxValue::Checked)
    ));
    assert_eq!(proposed.get(), Some(CheckboxValue::Checked));
    // The model itself never mutates: the example's own state is what makes
    // the second render show the change.
    assert_eq!(checkbox.current_value(), CheckboxValue::Unchecked);
}
