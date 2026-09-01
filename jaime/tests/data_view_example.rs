#[path = "../src/data_view_example.rs"]
mod data_view_example;

#[test]
fn data_view_example_builds_as_a_widget_without_showcase_registration() {
    fn assert_widget(_widget: impl aimer::Widget) {}

    assert_widget(data_view_example::data_view_example());
}

#[test]
fn data_view_example_has_a_standalone_launcher() {
    let example = data_view_example::DataViewExample::new();
    assert_eq!(aimer::Widget::debug_name(&example), "DataViewExample");
}
