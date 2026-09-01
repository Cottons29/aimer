#[path = "../src/routing_context_example.rs"]
mod routing_context_example;

#[test]
fn route_context_example_builds_without_a_global_theme_module() {
    let _ = routing_context_example::routing_context_example();
}
