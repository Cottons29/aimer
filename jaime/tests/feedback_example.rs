#[path = "../src/feedback_example.rs"]
mod feedback_example;

#[test]
fn feedback_example_builds_without_global_overlay_state() {
    fn assert_widget(_widget: impl aimer::Widget) {}

    assert_widget(feedback_example::feedback_example());
}

#[test]
fn feedback_example_contains_all_feedback_tones_and_indicator_widgets() {
    use aimer::feedback::{ProgressIndicator, Spinner, StatusBanner, StatusKind};

    fn assert_widget(_widget: impl aimer::Widget) {}

    assert_widget(ProgressIndicator::determinate(0.65).unwrap());
    assert_widget(Spinner::new());
    for kind in [
        StatusKind::Loading,
        StatusKind::Success,
        StatusKind::Warning,
        StatusKind::Error,
    ] {
        assert_widget(StatusBanner::new("example").kind(kind));
    }
}
