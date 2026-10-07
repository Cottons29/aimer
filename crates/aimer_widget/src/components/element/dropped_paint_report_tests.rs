//! An element that draws through the removed legacy path is reported once per
//! element type, so a dev run shows what still needs migrating without
//! flooding the log every frame.

use super::first_dropped_paint_report;

#[test]
fn each_element_type_is_reported_once() {
    assert!(first_dropped_paint_report("ReportTestIslandA"));
    assert!(!first_dropped_paint_report("ReportTestIslandA"));
    assert!(!first_dropped_paint_report("ReportTestIslandA"));
}

#[test]
fn different_element_types_are_reported_independently() {
    assert!(first_dropped_paint_report("ReportTestIslandB"));
    assert!(first_dropped_paint_report("ReportTestIslandC"));
    assert!(!first_dropped_paint_report("ReportTestIslandB"));
}
