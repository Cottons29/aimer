#[path = "../src/accessibility_example.rs"]
mod accessibility_example;

#[test]
fn accessibility_example_builds_without_platform_adapters() {
    let snapshot = accessibility_example::settings_semantics_example();
    assert_eq!(snapshot.len(), 3);
}
