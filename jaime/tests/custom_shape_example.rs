#[path = "../src/custom_shape_example.rs"]
mod custom_shape_example;

use aimer::Widget;

mod theme {
    pub fn provide<W: aimer::Widget>(widget: W) -> W {
        widget
    }
}

#[test]
fn custom_shape_example_is_constructible_without_starting_the_app() {
    fn assert_widget(_widget: impl Widget) {}

    assert_widget(custom_shape_example::custom_shape_example());
}

#[test]
fn custom_shape_example_uses_finite_geometry_and_rejects_invalid_input() {
    let path = custom_shape_example::demo_shape_path();
    assert_eq!(path.contour_count(), 1);
    assert!(path.bounds().width() > 0.0);
    assert!(path.bounds().height() > 0.0);
    assert!(custom_shape_example::invalid_shape_is_rejected());
}

#[test]
fn custom_shape_example_exposes_a_shareable_cached_path() {
    let shared = custom_shape_example::shared_demo_shape_path();
    let rebuilt = custom_shape_example::demo_shape_path();

    assert_eq!(shared.id(), rebuilt.id());
    assert_eq!(shared.as_ref(), &rebuilt);
}

#[test]
fn custom_shape_example_exposes_a_standalone_entry_point() {
    let _start: fn() = custom_shape_example::start_custom_shape_example;
}
