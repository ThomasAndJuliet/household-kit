// SPDX-License-Identifier: GPL-3.0-only

//! Unbounded width: a leaf with no intrinsic width, through each path that
//! probes it, and arrangements queried with no width limit.
//!
//! A native Fill control offered unbounded width reports an infinite width.
//! Such a leaf has a size only where its declaration gives it a finite width
//! to be probed at: a minimum width (`minimum_width`), or a viewport's floor
//! (`scroll_child`). The probe must fit the width it is given, within the
//! tolerance, and give a size; a leaf measured with no valid height, and any
//! arrangement, is not probed. A caller with no width limit (`query_at` at an
//! infinite width) gets every part at its preferred width, with no surplus to
//! grow into; a column is then as wide as its widest part, and a part that
//! grows fills that.

use super::test_fixtures::*;
use super::*;

/// The paragraph, as a control with no intrinsic width: one line, 10 high,
/// when unbounded.
fn unbounded_paragraph() -> Child<'static, ()> {
    unbounded(paragraph)
}

#[test]
fn a_leaf_without_intrinsic_width_measures_an_infinite_width_and_fits_nowhere() {
    let leaf = unbounded_paragraph();
    assert_eq!(measure(&leaf).preferred, Size::new(f32::INFINITY, 10.0));
    assert_eq!(
        query_at(&leaf, 100.0, 100.0),
        Err(AllocationError::InvalidInput)
    );
    assert_eq!(
        query_at(&leaf, f32::INFINITY, f32::INFINITY),
        Err(AllocationError::InvalidInput)
    );
}

// Probed at its floor of 40 the paragraph needs three lines: that is its
// preferred size, and its floor of height stays the one line it measured
// unbounded. It is allocated its preferred 40, or, growing, the whole 100,
// where it needs two lines.
#[test]
fn a_minimum_width_probes_a_leaf_without_intrinsic_width_at_the_floor() {
    let leaf = unbounded_paragraph().minimum_width(|_, _| 40.0);
    assert_eq!(
        measure(&leaf),
        MeasuredSize {
            preferred: Size::new(40.0, 30.0),
            minimum: Size::new(40.0, 10.0),
        }
    );
    assert_eq!(
        query_at(&leaf, 100.0, 100.0).map(|plan| plan.size),
        Ok(Size::new(40.0, 30.0))
    );
    assert_eq!(
        query_at(&leaf.grow(1.0), 100.0, 100.0).map(|plan| plan.size),
        Ok(Size::new(100.0, 20.0))
    );
}

#[test]
fn a_leaf_without_intrinsic_width_wider_than_its_floor_when_probed_is_invalid() {
    let leaf = unbounded(|width| Size::new(width + 5.0, 10.0)).minimum_width(|_, _| 40.0);
    assert!(measure(&leaf).minimum.width.is_nan());
    assert_eq!(
        query_at(&leaf, 100.0, 100.0),
        Err(AllocationError::InvalidInput)
    );
}

#[test]
fn a_leaf_without_intrinsic_width_that_probes_to_no_size_is_invalid() {
    for probed in [
        Size::new(40.0, f32::INFINITY),
        Size::new(40.0, f32::NAN),
        Size::new(f32::NAN, 10.0),
        Size::new(40.0, -1.0),
    ] {
        let leaf = unbounded(move |width| {
            if width.is_finite() {
                probed
            } else {
                Size::new(0.0, 10.0)
            }
        })
        .minimum_width(|_, _| 40.0);
        assert!(measure(&leaf).minimum.width.is_nan(), "{probed:?}");
        assert_eq!(
            query_at(&leaf, 100.0, 100.0),
            Err(AllocationError::InvalidInput),
            "{probed:?}"
        );
    }
}

// Unbounded in both dimensions, the leaf has no line height to keep, and is
// not probed, though at the floor it would take 40 × 10.
#[test]
fn a_leaf_without_intrinsic_width_or_height_is_not_probed() {
    let leaf = shaped(|offer| {
        if offer.width.is_infinite() {
            Size::INFINITE
        } else {
            Size::new(offer.width, 10.0)
        }
    })
    .minimum_width(|_, _| 40.0);
    assert!(measure(&leaf).minimum.width.is_nan());
    assert_eq!(
        query_at(&leaf, 100.0, 100.0),
        Err(AllocationError::InvalidInput)
    );
}

// A measurement with an infinite width is probed only when its preferred
// height and its floor of height are sizes, though the view here would take
// a size at the floor.
#[test]
fn a_leaf_without_intrinsic_width_is_probed_only_with_a_valid_height() {
    let heights = [f32::NAN, f32::INFINITY, -1.0];
    let measured = heights
        .iter()
        .map(|height| {
            (
                Size::new(f32::INFINITY, *height),
                Size::new(f32::INFINITY, 10.0),
            )
        })
        .chain(heights.iter().map(|height| {
            (
                Size::new(f32::INFINITY, 10.0),
                Size::new(f32::INFINITY, *height),
            )
        }));
    for (preferred, minimum) in measured {
        let leaf = unbounded_paragraph()
            .with_measurement(move |_, _| MeasuredSize { preferred, minimum })
            .minimum_width(|_, _| 40.0);
        assert!(
            measure(&leaf).minimum.width.is_nan(),
            "{preferred:?}, {minimum:?}"
        );
    }
}

// Only a leaf has a view to probe: a row holding the unbounded paragraph
// measures an infinite width, and a minimum width on the row gives no floor.
#[test]
fn an_arrangement_without_intrinsic_width_is_not_probed() {
    let row = row_child(vec![unbounded_paragraph()], 0.0).minimum_width(|_, _| 40.0);
    assert!(measure(&row).minimum.width.is_nan());
    assert_eq!(
        query_at(&row, 100.0, 100.0),
        Err(AllocationError::InvalidInput)
    );
}

// The view fills what it is offered, with nothing to show beyond 60 high;
// unbounded, it reports an infinite width. Probed at its floor of 40 × 10 it
// fits, so it prefers the floor's width and the 60 it measured.
#[test]
fn a_viewport_without_intrinsic_width_is_probed_at_its_floor() {
    let viewport = shaped_viewport(
        |offer| Size::new(offer.width, offer.height.min(60.0)),
        Size::new(40.0, 10.0),
    );
    assert_eq!(
        measure(&viewport),
        MeasuredSize {
            preferred: Size::new(40.0, 60.0),
            minimum: Size::new(40.0, 10.0),
        }
    );
    let plan = query_at(&viewport, 100.0, 30.0).unwrap();
    assert_eq!(plan.size, Size::new(40.0, 30.0));
    assert_eq!(plan.minimum_height(), 10.0);
}

// Unbounded in both dimensions, the viewport prefers its floor.
#[test]
fn a_viewport_without_intrinsic_size_prefers_its_floor() {
    let viewport = shaped_viewport(|offer| offer, Size::new(40.0, 10.0));
    assert_eq!(
        measure(&viewport),
        MeasuredSize {
            preferred: Size::new(40.0, 10.0),
            minimum: Size::new(40.0, 10.0),
        }
    );
}

#[test]
fn a_viewport_without_intrinsic_size_that_probes_to_more_than_its_floor_is_invalid() {
    for probed in [
        Size::new(45.0, 10.0),
        Size::new(40.0, 15.0),
        Size::new(f32::NAN, 10.0),
        Size::new(40.0, f32::INFINITY),
    ] {
        let viewport = shaped_viewport(
            move |offer| {
                if offer.width.is_infinite() {
                    offer
                } else {
                    probed
                }
            },
            Size::new(40.0, 10.0),
        );
        let measured = measure(&viewport);
        assert!(
            measured.preferred.width.is_nan() && measured.preferred.height.is_nan(),
            "{probed:?}"
        );
        assert_eq!(
            query_at(&viewport, 100.0, 100.0),
            Err(AllocationError::InvalidInput),
            "{probed:?}"
        );
    }
}

// The part that would grow does not: there is no surplus.
#[test]
fn a_row_with_no_width_limit_plans_each_part_at_its_preferred_width() {
    let row = row_child(
        vec![part(100.0, 40.0, 20.0), part(50.0, 50.0, 20.0).grow(1.0)],
        10.0,
    );
    let plan = query_at(&row, f32::INFINITY, 100.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 0.0], [100.0, 20.0]),
            (1, [110.0, 0.0], [50.0, 20.0]),
        ]
    );
    assert_eq!(plan.size, Size::new(160.0, 20.0));
}

// The column is planned at its preferred width, its widest part's 80, and
// the part that grows fills that.
#[test]
fn a_column_with_no_width_limit_is_planned_at_its_preferred_width() {
    let column = column_child(
        vec![part(80.0, 80.0, 20.0), part(40.0, 40.0, 20.0).grow(1.0)],
        0.0,
    );
    let plan = query_at(&column, f32::INFINITY, 100.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 0.0], [80.0, 20.0]),
            (1, [0.0, 20.0], [80.0, 20.0]),
        ]
    );
    assert_eq!(plan.size, Size::new(80.0, 40.0));
}

// One row, at the parts' preferred widths and the gaps: 240.
#[test]
fn a_wrap_with_no_width_limit_is_one_row() {
    let wrap = wrap_child(
        vec![
            part(80.0, 80.0, 10.0),
            part(80.0, 40.0, 20.0),
            part(60.0, 60.0, 10.0).grow(1.0),
        ],
        10.0,
    );
    let plan = query_at(&wrap, f32::INFINITY, 100.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 5.0], [80.0, 10.0]),
            (1, [90.0, 0.0], [80.0, 20.0]),
            (2, [180.0, 5.0], [60.0, 10.0]),
        ]
    );
    assert_eq!(plan.rows, vec![vec![0, 1, 2]]);
    assert_eq!(plan.size, Size::new(240.0, 20.0));
}

#[test]
fn a_growing_leaf_with_no_width_limit_keeps_its_preferred_width() {
    let leaf = part(50.0, 50.0, 20.0).grow(1.0);
    assert_eq!(
        query_at(&leaf, f32::INFINITY, f32::INFINITY).map(|plan| plan.size),
        Ok(Size::new(50.0, 20.0))
    );
}

// A part that grows in height fills a room's height, but with no height
// limit keeps its own.
#[test]
fn a_part_growing_in_height_with_no_height_limit_keeps_its_height() {
    let leaf = part(50.0, 50.0, 20.0).grow_height(1.0);
    assert_eq!(
        query_at(&leaf, 100.0, 80.0).map(|plan| plan.size),
        Ok(Size::new(50.0, 80.0))
    );
    assert_eq!(
        query_at(&leaf, 100.0, f32::INFINITY).map(|plan| plan.size),
        Ok(Size::new(50.0, 20.0))
    );
}

// With no height limit there is no surplus of height either: the viewport
// that grows keeps its preferred 60.
#[test]
fn a_column_with_no_height_limit_gives_a_growing_viewport_its_preferred_height() {
    let viewport = shaped_viewport(
        |offer| Size::new(offer.width.min(80.0), offer.height.min(60.0)),
        Size::new(80.0, 10.0),
    )
    .grow_height(1.0);
    let column = column_child(vec![part(80.0, 80.0, 20.0), viewport], 10.0);
    let plan = query_at(&column, 80.0, f32::INFINITY).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 0.0], [80.0, 20.0]),
            (1, [0.0, 30.0], [80.0, 60.0]),
        ]
    );
    assert_eq!(plan.size, Size::new(80.0, 90.0));
}
