//! Tests for the cached main-axis table built in
//! [`flex_layout`](super).
//!
//! They live in their own file because the table itself is close to the
//! nine-hundred-line ceiling this crate keeps per file.

use super::*;
use crate::flex::raw_flex::justify_distribution;
use crate::flex::JustifyContent;


fn column_of(sizes: &[(f32, f32)], gap: f32) -> FlexLayout {
    let sizes = sizes
        .iter()
        .map(|(width, height)| ResolvedSize {
            width: *width,
            height: *height,
        })
        .collect();
    FlexLayout::from_sizes(sizes, false, gap, false)
}

#[test]
fn justify_content_distributes_free_main_axis_space() {
    assert_eq!(
        justify_distribution(JustifyContent::Start, 60.0, 3),
        (0.0, 0.0)
    );
    assert_eq!(
        justify_distribution(JustifyContent::Center, 60.0, 3),
        (30.0, 0.0)
    );
    assert_eq!(
        justify_distribution(JustifyContent::End, 60.0, 3),
        (60.0, 0.0)
    );
    assert_eq!(
        justify_distribution(JustifyContent::SpaceBetween, 60.0, 3),
        (0.0, 30.0)
    );
    assert_eq!(
        justify_distribution(JustifyContent::SpaceAround, 60.0, 3),
        (10.0, 20.0)
    );
    assert_eq!(
        justify_distribution(JustifyContent::SpaceEvenly, 60.0, 3),
        (15.0, 15.0)
    );
}

#[test]
fn space_between_falls_back_to_start_for_one_child() {
    assert_eq!(
        justify_distribution(JustifyContent::SpaceBetween, 60.0, 1),
        (0.0, 0.0)
    );
}

#[test]
fn visible_range_accounts_for_distributed_main_axis_space() {
    let layout = FlexLayout::from_sizes(
        vec![
            ResolvedSize {
                width: 10.0,
                height: 10.0,
            },
            ResolvedSize {
                width: 10.0,
                height: 10.0,
            },
            ResolvedSize {
                width: 10.0,
                height: 10.0,
            },
        ],
        true,
        0.0,
        false,
    );

    assert_eq!(
        layout.visible_range_with_extra_space(25.0, 35.0, 0.0, 15.0),
        1..2
    );
}

#[test]
fn uniform_column_stores_one_size_and_a_stride() {
    let layout = column_of(&[(10.0, 20.0); 4], 5.0);

    assert_eq!(layout.len(), 4);
    assert_eq!(layout.sizes.len(), 1);
    assert_eq!(layout.stride, Some(25.0));
    assert_eq!(layout.offset(0), 0.0);
    assert_eq!(layout.offset(3), 75.0);
    // Four 20px children with three 5px gaps.
    assert_eq!(layout.total().height, 95.0);
    assert_eq!(layout.total().width, 10.0);
}

#[test]
fn varying_column_records_every_offset() {
    let layout = column_of(&[(10.0, 20.0), (30.0, 40.0), (5.0, 10.0)], 2.0);

    assert_eq!(layout.stride, None);
    assert_eq!(layout.offset(0), 0.0);
    assert_eq!(layout.offset(1), 22.0);
    assert_eq!(layout.offset(2), 64.0);
    assert_eq!(layout.total().height, 74.0);
    assert_eq!(layout.total().width, 30.0);
}

#[test]
fn uniform_visible_range_covers_the_touched_children() {
    let layout = column_of(&[(10.0, 100.0); 1_000], 0.0);

    assert_eq!(layout.visible_range(0.0, 250.0), 0..3);
    assert_eq!(layout.visible_range(450.0, 650.0), 4..7);
    assert_eq!(layout.visible_range(99_900.0, 100_500.0), 999..1_000);
}

#[test]
fn varying_visible_range_matches_the_uniform_result() {
    let layout = column_of(&[(10.0, 100.0), (20.0, 100.0), (10.0, 100.0), (10.0, 100.0)], 0.0);

    assert_eq!(layout.visible_range(0.0, 150.0), 0..2);
    assert_eq!(layout.visible_range(150.0, 250.0), 1..3);
    assert_eq!(layout.visible_range(1_000.0, 1_100.0), 4..4);
}

#[test]
fn empty_layout_has_an_empty_range() {
    let layout = column_of(&[], 4.0);

    assert_eq!(layout.len(), 0);
    assert_eq!(layout.total(), ResolvedSize::default());
    assert_eq!(layout.visible_range(0.0, 100.0), 0..0);
}

#[test]
fn declared_extent_builds_a_stride_without_sizes() {
    let layout = FlexLayout::declared(100_000, 200.0, 400.0, false, 10.0);

    assert!(layout.is_declared());
    assert_eq!(layout.len(), 100_000);
    assert_eq!(layout.stride, Some(210.0));
    assert_eq!(layout.offsets.len(), 0);
    assert_eq!(layout.offset(99_999), 99_999.0 * 210.0);
    assert_eq!(
        layout.size(50_000),
        ResolvedSize {
            width: 400.0,
            height: 200.0,
        }
    );
    // 100 000 children of 200px with 99 999 gaps of 10px.
    assert_eq!(layout.total().height, 100_000.0 * 210.0 - 10.0);
    assert_eq!(layout.total().width, 400.0);
    // Children start at 0, 210, and 420, so three of them touch a 600px
    // viewport.
    assert_eq!(layout.visible_range(0.0, 600.0), 0..3);
}

/// A prediction has to be indistinguishable from the measured table of a
/// uniform list, apart from admitting that it is a prediction.
#[test]
fn an_estimated_extent_matches_a_measured_uniform_table() {
    let probe = ResolvedSize {
        width: 10.0,
        height: 200.0,
    };
    let estimated = FlexLayout::estimated(100_000, probe, false, 10.0);
    let measured = column_of(&[(10.0, 200.0); 4], 10.0);

    assert!(estimated.is_estimated());
    assert!(!estimated.is_declared());
    assert_eq!(estimated.stride, measured.stride);
    assert_eq!(estimated.offsets.len(), 0);
    assert_eq!(estimated.size(50_000), probe);
    assert_eq!(estimated.offset(99_999), 99_999.0 * 210.0);
    assert_eq!(estimated.total().height, 100_000.0 * 210.0 - 10.0);
    // Nothing but the probe was measured, so its cross extent is all the
    // container can report.
    assert_eq!(estimated.total().width, 10.0);
}

/// A measured table must never be mistaken for a prediction, or it would be
/// re-verified against its own children forever.
#[test]
fn a_measured_table_is_neither_declared_nor_estimated() {
    let layout = column_of(&[(10.0, 20.0); 4], 5.0);

    assert!(!layout.is_declared());
    assert!(!layout.is_estimated());
}

#[test]
fn declared_empty_list_matches_a_measured_empty_list() {
    let layout = FlexLayout::declared(0, 200.0, 400.0, false, 10.0);

    assert_eq!(layout.len(), 0);
    assert_eq!(layout.total(), ResolvedSize::default());
    assert_eq!(layout.visible_range(0.0, 100.0), 0..0);
}

/// A tall list must keep exact offsets: `f32` cannot represent every
/// multiple of 110 past ~8.4 million.
#[test]
fn deep_offsets_stay_exact() {
    let layout = column_of(&[(10.0, 80.0); 120_000], 30.0);

    assert_eq!(layout.offset(119_999), 119_999.0 * 110.0);
    assert_eq!(layout.visible_range(13_199_890.0, 13_199_970.0), 119_999..120_000);
}

/// A hundred-thousand-row prediction with one 400px row somewhere inside it.
fn predicted_column() -> FlexLayout {
    FlexLayout::estimated(
        100_000,
        ResolvedSize {
            width: 10.0,
            height: 200.0,
        },
        false,
        0.0,
    )
}

/// Correcting one child must move the children after it and nothing else.
///
/// This is the invariant that lets a prediction be corrected while the user is
/// looking at it: the row under the viewport keeps the offset it was painted at,
/// so the correction is never visible as a jump.
#[test]
fn a_correction_moves_only_the_children_after_it() {
    let layout = predicted_column();

    assert!(layout.refine(
        40_000,
        ResolvedSize {
            width: 10.0,
            height: 500.0,
        }
    ));

    assert_eq!(layout.offset(0), 0.0);
    assert_eq!(layout.offset(40_000), 40_000.0 * 200.0);
    assert_eq!(layout.offset(40_001), 40_000.0 * 200.0 + 500.0);
    assert_eq!(layout.offset(99_999), 99_999.0 * 200.0 + 300.0);
    assert_eq!(layout.size(40_000).height, 500.0);
    assert_eq!(layout.size(40_001).height, 200.0);
    // The rows that were never looked at keep the probe, so the total carries
    // exactly the one correction.
    assert_eq!(layout.total().height, 100_000.0 * 200.0 + 300.0);
}

/// Several corrections have to accumulate, in any order, and a repeated one must
/// replace rather than add to the previous value.
#[test]
fn corrections_accumulate_and_replace() {
    let layout = predicted_column();
    let tall = |height| ResolvedSize {
        width: 10.0,
        height,
    };

    layout.refine(7, tall(300.0));
    layout.refine(3, tall(250.0));
    layout.refine(7, tall(400.0));

    // Row 3 grew by 50 and row 7 by 200.
    assert_eq!(layout.offset(4), 4.0 * 200.0 + 50.0);
    assert_eq!(layout.offset(8), 8.0 * 200.0 + 250.0);
    assert_eq!(layout.total().height, 100_000.0 * 200.0 + 250.0);
}

/// A correction changes which children a span covers, so the range lookup has to
/// read the corrections rather than the stride alone.
#[test]
fn a_corrected_range_accounts_for_the_correction() {
    let layout = predicted_column();

    // Rows now start at 0, 200, 1000, 1200, ...
    layout.refine(
        1,
        ResolvedSize {
            width: 10.0,
            height: 800.0,
        },
    );

    assert_eq!(layout.visible_range(0.0, 600.0), 0..2);
    // Rows 2, 3, and 4 span 1000..1200, 1200..1400, and 1400..1600.
    assert_eq!(layout.visible_range(1_100.0, 1_500.0), 2..5);
}

/// The container's cross-axis size has to grow with a wider corrected child, or
/// the row would be clipped by the size its own parent was told.
#[test]
fn a_correction_widens_the_container() {
    let layout = predicted_column();

    layout.refine(
        5,
        ResolvedSize {
            width: 90.0,
            height: 200.0,
        },
    );

    assert_eq!(layout.total().width, 90.0);
}

/// Only a prediction may be corrected. A declared extent is what the caller
/// stated, and a measured table is exact already — correcting either would make
/// the container disagree with itself.
#[test]
fn only_a_prediction_accepts_corrections() {
    let declared = FlexLayout::declared(100, 200.0, 400.0, false, 0.0);
    let measured = column_of(&[(10.0, 200.0); 100], 0.0);
    let predicted = predicted_column();
    let tall = ResolvedSize {
        width: 10.0,
        height: 900.0,
    };

    assert!(!declared.refine(5, tall));
    assert!(!measured.refine(5, tall));
    assert!(!predicted.refine(100_000, tall), "out of bounds");

    assert_eq!(declared.total().height, 100.0 * 200.0);
    assert_eq!(measured.total().height, 100.0 * 200.0);
}

/// Correcting every row has to leave the table exactly as measuring the list
/// would have: that is what makes the prediction converge instead of merely
/// approximate.
#[test]
fn correcting_every_row_reaches_the_measured_result() {
    let predicted = FlexLayout::estimated(
        6,
        ResolvedSize {
            width: 10.0,
            height: 200.0,
        },
        false,
        5.0,
    );
    let heights = [50.0, 200.0, 300.0, 120.0, 200.0, 80.0];
    let measured = column_of(
        &heights.map(|height: f32| (10.0, height)),
        5.0,
    );

    for (index, height) in heights.iter().enumerate() {
        predicted.refine(
            index,
            ResolvedSize {
                width: 10.0,
                height: *height,
            },
        );
    }

    assert_eq!(predicted.total(), measured.total());
    for index in 0..heights.len() {
        assert_eq!(predicted.offset(index), measured.offset(index));
        assert_eq!(predicted.size(index), measured.size(index));
    }
}
