#[path = "../src/animated_layout_example.rs"]
mod animated_layout_example;

use aimer::Widget;

#[test]
fn animated_layout_example_exposes_a_public_widget_constructor() {
    let example = animated_layout_example::animated_layout_example();

    assert_eq!(example.debug_name(), "AnimatedLayout");
}
