// SPDX-License-Identifier: GPL-3.0-only

//! Content whose height depends on its width, planned at the width it is
//! allocated.
//!
//! The paragraph (`wrapping`) measures one line, 120 × 10, and is usable from
//! 30 wide; narrower it takes more lines: 20 high at 60 to 119 wide, 30 at 40
//! to 59, 40 at 30 to 39. Wherever it is placed, it is planned at the width
//! its parent allocates it, so its plan's height, and the floor of height
//! the plan records (`GeometryPlan::minimum_height`), are those at that
//! width, not the one line its measurement gives.

use super::test_fixtures::*;
use super::*;

// At 50 wide the paragraph needs three lines, and that height is its floor.
#[test]
fn a_wrapping_leaf_measures_one_line_and_is_planned_at_its_width() {
    assert_eq!(
        measure(&wrapping()),
        MeasuredSize {
            preferred: Size::new(120.0, 10.0),
            minimum: Size::new(30.0, 10.0),
        }
    );
    let plan = query_at(&wrapping(), 50.0, 100.0).unwrap();
    assert_eq!(plan.size, Size::new(50.0, 30.0));
    assert_eq!(plan.minimum_height(), 30.0);
}

// In 100 the part keeps its 60, which it cannot go below, and the paragraph
// takes the 40 left, where it needs three lines. The row is that high, with
// the part centred in it, and its floor of height is the paragraph's at 40.
#[test]
fn a_row_plans_wrapping_content_at_the_width_it_allocates() {
    let row = row_child(vec![wrapping(), part(60.0, 60.0, 10.0)], 0.0);
    let plan = query_at(&row, 100.0, 100.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 0.0], [40.0, 30.0]),
            (1, [40.0, 10.0], [60.0, 10.0]),
        ]
    );
    assert_eq!(plan.size, Size::new(100.0, 30.0));
    assert_eq!(plan.minimum_height(), 30.0);
}

// The row's measurement promises one line, 10 high; at the 40 the row gives
// the paragraph it needs 30, and a row 25 high fails with that.
#[test]
fn a_row_too_low_for_wrapped_content_fails_with_its_height_at_its_width() {
    let row = row_child(vec![wrapping(), part(60.0, 60.0, 10.0)], 0.0);
    assert_eq!(measure(&row).minimum.height, 10.0);
    assert_eq!(query_at(&row, 100.0, 25.0), Err(no_fit(30.0, 25.0)));
}

// The part's floor of 80 and the paragraph's of 30 do not fit side by side
// in 100 with a gap of 10, so the paragraph starts a second row. Each row is
// given the whole 100, and the paragraph, 100 wide, needs two lines.
#[test]
fn a_wrap_that_wraps_plans_wrapping_content_at_its_rows_width() {
    let wrap = wrap_child(vec![part(80.0, 80.0, 10.0), wrapping()], 10.0);
    let plan = query_at(&wrap, 100.0, 100.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 0.0], [80.0, 10.0]),
            (1, [0.0, 20.0], [100.0, 20.0]),
        ]
    );
    assert_eq!(plan.rows, vec![vec![0], vec![1]]);
    assert_eq!(plan.size, Size::new(100.0, 40.0));
    assert_eq!(plan.minimum_height(), 40.0);
}

// A padding of 5 leaves the paragraph 60 of the frame's 70: two lines.
#[test]
fn a_frame_plans_wrapping_content_at_the_width_inside_its_padding() {
    let frame = framed_child(wrapping(), 5.0, |content| content);
    let plan = query_at(&frame, 70.0, 100.0).unwrap();
    assert_eq!(placed(&plan), vec![(0, [5.0, 5.0], [60.0, 20.0])]);
    assert_eq!(plan.size, Size::new(70.0, 30.0));
    assert_eq!(plan.minimum_height(), 30.0);
}

// At Preferred the family is the paragraph, planned at the 40 it is given.
#[test]
fn forms_plan_wrapping_content_at_the_width_they_are_given() {
    let family = forms(vec![
        (Level::Preferred, wrapping()),
        (Level::Strong, part(30.0, 30.0, 10.0)),
    ]);
    let plan = query_at(&family, 40.0, 100.0).unwrap();
    assert_eq!(plan.size, Size::new(40.0, 30.0));
    assert_eq!(plan.minimum_height(), 30.0);
}

// A column in a row in a column. The outer column gives the row its 100.
// The row shares that between the inner column, shrunk from its paragraph's
// 120 by the whole shortage of 80 to 40, and the part's 60. The inner column
// gives the paragraph its 40: three lines, 30 high, above a part 10 high. The
// row is as high as the inner column, 40, and the outer column 10 more.
#[test]
fn nested_arrangements_plan_wrapping_content_at_the_width_each_allocates() {
    let inner = column_child(vec![wrapping(), part(30.0, 30.0, 10.0)], 0.0);
    let row = row_child(vec![inner, part(60.0, 60.0, 20.0)], 0.0);
    let outer = column_child(vec![part(100.0, 100.0, 10.0), row], 0.0);
    let plan = query_at(&outer, 100.0, 1000.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 0.0], [100.0, 10.0]),
            (1, [0.0, 10.0], [100.0, 40.0]),
        ]
    );
    assert_eq!(plan.size, Size::new(100.0, 50.0));
    assert_eq!(plan.minimum_height(), 50.0);
    let row = &plan.children[1].geometry;
    assert_eq!(
        placed(row),
        vec![
            (0, [0.0, 0.0], [40.0, 40.0]),
            (1, [40.0, 10.0], [60.0, 20.0]),
        ]
    );
    assert_eq!(row.minimum_height(), 40.0);
    let inner = &row.children[0].geometry;
    assert_eq!(
        placed(inner),
        vec![
            (0, [0.0, 0.0], [40.0, 30.0]),
            (1, [0.0, 30.0], [30.0, 10.0]),
        ]
    );
    assert_eq!(inner.minimum_height(), 40.0);
}
