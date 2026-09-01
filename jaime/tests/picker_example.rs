#[path = "../src/picker_example.rs"]
mod picker_example;
#[path = "../src/theme.rs"]
mod theme;

#[test]
fn picker_example_builds_without_a_global_overlay_host() {
    fn assert_widget(_widget: impl aimer::Widget) {}

    assert_widget(picker_example::picker_example());
}
