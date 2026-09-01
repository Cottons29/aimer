#[path = "../src/svg_example.rs"]
mod svg_example;

#[test]
fn svg_example_builds_and_exposes_deferred_feature_diagnostics() {
    let _widget = svg_example::svg_example();
    let document = svg_example::svg_document();

    assert_eq!(document.view_box().width, 140.0);
    assert!(document
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.feature == "gradient"));
    assert!(document
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.feature == "gradient-fill"));
}
