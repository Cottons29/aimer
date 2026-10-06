//! A legacy island is reported once per element type, so a dev run shows what
//! still paints through the legacy path without flooding the log every frame.

use super::first_legacy_island_report;

#[test]
fn each_element_type_is_reported_once() {
    assert!(first_legacy_island_report("ReportTestIslandA"));
    assert!(!first_legacy_island_report("ReportTestIslandA"));
    assert!(!first_legacy_island_report("ReportTestIslandA"));
}

#[test]
fn different_element_types_are_reported_independently() {
    assert!(first_legacy_island_report("ReportTestIslandB"));
    assert!(first_legacy_island_report("ReportTestIslandC"));
    assert!(!first_legacy_island_report("ReportTestIslandB"));
}
