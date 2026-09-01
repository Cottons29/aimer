#[path = "../src/navigation_example.rs"]
mod navigation_example;

#[test]
fn navigation_example_builds_before_showcase_registration() {
    let _ = navigation_example::navigation_example();
}
