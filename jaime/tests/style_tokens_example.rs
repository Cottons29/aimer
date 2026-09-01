#[path = "../src/style_tokens_example.rs"]
mod style_tokens_example;

#[test]
fn style_tokens_example_is_a_widget() {
    fn assert_widget(_widget: impl aimer::Widget) {}

    assert_widget(style_tokens_example::style_tokens_example());
}
