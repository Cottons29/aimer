#[path = "../src/i18n_example.rs"]
mod i18n_example;

use aimer::Widget;

#[test]
fn i18n_example_builds_without_system_locale_or_manifest_integration() {
    fn assert_widget(_widget: impl aimer::Widget) {}

    let example = i18n_example::I18nExample::new();
    assert_eq!(example.debug_name(), "I18nExample");
    assert_widget(i18n_example::i18n_example());
}
