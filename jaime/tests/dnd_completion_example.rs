#[path = "../src/dnd_completion_example.rs"]
mod dnd_completion_example;

use aimer::Widget;

#[test]
fn dnd_completion_page_is_constructible_as_a_widget() {
    let page = dnd_completion_example::dnd_completion_example();
    let _boxed = page.boxed();
}

#[test]
fn dnd_completion_page_exposes_a_standalone_entry_point() {
    let _start: fn() = dnd_completion_example::start_dnd_completion_example;
}
