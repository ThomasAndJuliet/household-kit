// SPDX-License-Identifier: GPL-3.0-only

//! The fit tolerance (`FIT_EPSILON`, 0.001) at every comparison that uses it,
//! and the gaps, which are compared without it.
//!
//! Each comparison is tested just inside, at exactly the room plus the
//! tolerance, and just outside, at the next f32 above that (`above`). The
//! numbers are near zero, where the f32 arithmetic is exact: the room is 0,
//! so a size of `FIT_EPSILON` is the room plus the tolerance to the bit, and
//! halves of these numbers are exact too. Each comparison is also tested at a
//! large coordinate (`LARGE`, 40000), where f32s are 1/256 apart, so adding
//! the tolerance to the room leaves the room unchanged: there a size one f32
//! above the room fails, though it exceeds the room by less than four
//! tolerances. `allocate_width` alone compares in f64, where the tolerance
//! survives at any coordinate.

use super::test_fixtures::*;
use super::*;
use cosmic::iced::Padding;

#[test]
fn the_large_coordinate_absorbs_the_tolerance() {
    assert_eq!(LARGE + FIT_EPSILON, LARGE);
    assert_eq!(above(LARGE) - LARGE, 1.0 / 256.0);
}

/// A width that a weight of zero protects: preferred and minimum `width`.
fn protected(width: f32) -> WidthCapacity {
    WidthCapacity {
        preferred: width,
        minimum: width,
        weight: 0.0,
    }
}

/// A part `width` wide that cannot shrink, of no height.
fn floor(width: f32) -> Child<'static, ()> {
    part(width, width, 0.0).weight(0.0)
}

/// A viewport whose view is `height` high, or what it is offered if less,
/// and as wide as its floor, `width` × `floor_height`. Each one in a wrap
/// `width` wide takes a row of its own.
fn scrolling(width: f32, height: f32, floor_height: f32) -> Child<'static, ()> {
    shaped_viewport(
        move |offer| Size::new(width, height.min(offer.height)),
        Size::new(width, floor_height),
    )
}

/// A leaf with no intrinsic width that, probed at its floor, takes `width`.
fn probed(width: f32) -> Child<'static, ()> {
    unbounded(move |_| Size::new(width, 10.0))
}

/// A viewport with no intrinsic size whose view, probed at its floor, takes
/// `size`.
fn viewport_probed(size: Size, floor: Size) -> Child<'static, ()> {
    shaped_viewport(
        move |offer| {
            if offer.width.is_infinite() {
                offer
            } else {
                size
            }
        },
        floor,
    )
}

fn padded(content: Child<'static, ()>, padding: Padding) -> Child<'static, ()> {
    framed_child(content, padding, |content| content)
}

fn left(padding: f32) -> Padding {
    Padding {
        left: padding,
        ..Padding::ZERO
    }
}

fn top(padding: f32) -> Padding {
    Padding {
        top: padding,
        ..Padding::ZERO
    }
}

// `allocate_width`: the floors' sum against the width, in f64.
#[test]
fn allocation_fits_floors_up_to_the_tolerance_over_the_width() {
    assert_eq!(
        allocate_width(0.0, &[protected(FIT_EPSILON)]),
        Ok(vec![FIT_EPSILON])
    );
    assert_eq!(
        allocate_width(0.0, &[protected(above(FIT_EPSILON))]),
        Err(no_fit(above(FIT_EPSILON), 0.0))
    );
}

// In f64 the tolerance survives a large width: floors of LARGE and
// FIT_EPSILON fit in LARGE. One f32 more does not, and the error then gives
// the sum in f32, which rounds to the width: 40000 required of 40000
// available. That pins present behaviour, and was reported: the numbers do
// not describe the shortage.
#[test]
fn allocation_at_a_large_width_keeps_the_tolerance_and_reports_a_rounded_sum() {
    assert_eq!(
        allocate_width(LARGE, &[protected(LARGE), protected(FIT_EPSILON)]),
        Ok(vec![LARGE, FIT_EPSILON])
    );
    assert_eq!(
        allocate_width(LARGE, &[protected(LARGE), protected(above(FIT_EPSILON))]),
        Err(no_fit(LARGE, LARGE))
    );
    assert_eq!(
        allocate_width(LARGE, &[protected(above(LARGE))]),
        Err(no_fit(above(LARGE), LARGE))
    );
}

// The probe of a leaf with no intrinsic width at its declared floor.
#[test]
fn a_probe_at_a_floor_fits_up_to_the_tolerance_over_it() {
    assert_eq!(
        measure(&probed(FIT_EPSILON).minimum_width(|_, _| 0.0)),
        MeasuredSize {
            preferred: Size::new(0.0, 10.0),
            minimum: Size::new(0.0, 10.0),
        }
    );
    let over = probed(above(FIT_EPSILON)).minimum_width(|_, _| 0.0);
    assert!(measure(&over).minimum.width.is_nan());
}

#[test]
fn a_probe_at_a_large_floor_fits_only_the_floor() {
    let fits = probed(LARGE + FIT_EPSILON).minimum_width(|_, _| LARGE);
    assert_eq!(measure(&fits).preferred, Size::new(LARGE, 10.0));
    let over = probed(above(LARGE)).minimum_width(|_, _| LARGE);
    assert!(measure(&over).minimum.width.is_nan());
}

// The probe of a viewport with no intrinsic size at its floor, in each
// dimension.
#[test]
fn a_viewport_probe_fits_up_to_the_tolerance_over_its_floor() {
    for size in [Size::new(FIT_EPSILON, 0.0), Size::new(0.0, FIT_EPSILON)] {
        assert_eq!(
            measure(&viewport_probed(size, Size::ZERO)).preferred,
            Size::ZERO,
            "{size:?}"
        );
    }
    for size in [
        Size::new(above(FIT_EPSILON), 0.0),
        Size::new(0.0, above(FIT_EPSILON)),
    ] {
        assert!(
            measure(&viewport_probed(size, Size::ZERO))
                .preferred
                .width
                .is_nan(),
            "{size:?}"
        );
    }
}

#[test]
fn a_viewport_probe_at_a_large_floor_fits_only_the_floor() {
    let floor = Size::new(LARGE, LARGE);
    let fits = Size::new(LARGE + FIT_EPSILON, LARGE + FIT_EPSILON);
    assert_eq!(measure(&viewport_probed(fits, floor)).preferred, floor);
    for size in [
        Size::new(above(LARGE), LARGE),
        Size::new(LARGE, above(LARGE)),
    ] {
        assert!(
            measure(&viewport_probed(size, floor))
                .preferred
                .width
                .is_nan(),
            "{size:?}"
        );
    }
}

// A fixed width or height against the room. The plan takes the fixed size,
// though it is the tolerance more than the room.
#[test]
fn a_fixed_dimension_fits_up_to_the_tolerance_over_its_room() {
    let wide = |width| sized(0.0, 0.0).fixed_dimensions(Some(width), None);
    assert_eq!(
        plan(&wide(FIT_EPSILON), 0.0, 100.0).map(|plan| plan.size),
        Ok(Size::new(FIT_EPSILON, 0.0))
    );
    assert_eq!(
        plan(&wide(above(FIT_EPSILON)), 0.0, 100.0),
        Err(no_fit(above(FIT_EPSILON), 0.0))
    );
    let high = |height| sized(0.0, 0.0).fixed_dimensions(None, Some(height));
    assert_eq!(
        plan(&high(FIT_EPSILON), 100.0, 0.0).map(|plan| plan.size),
        Ok(Size::new(0.0, FIT_EPSILON))
    );
    assert_eq!(
        plan(&high(above(FIT_EPSILON)), 100.0, 0.0),
        Err(no_fit(above(FIT_EPSILON), 0.0))
    );
}

#[test]
fn a_fixed_dimension_at_a_large_room_fits_only_the_room() {
    let wide = |width| sized(0.0, 0.0).fixed_dimensions(Some(width), None);
    assert_eq!(
        plan(&wide(LARGE + FIT_EPSILON), LARGE, 100.0).map(|plan| plan.size),
        Ok(Size::new(LARGE, 0.0))
    );
    assert_eq!(
        plan(&wide(above(LARGE)), LARGE, 100.0),
        Err(no_fit(above(LARGE), LARGE))
    );
    let high = |height| sized(0.0, 0.0).fixed_dimensions(None, Some(height));
    assert_eq!(
        plan(&high(above(LARGE)), 100.0, LARGE),
        Err(no_fit(above(LARGE), LARGE))
    );
}

// A leaf's view against the width it is allocated. The plan keeps the width
// allocated.
#[test]
fn a_leaf_fits_up_to_the_tolerance_wider_than_its_offer() {
    assert_eq!(
        plan(&sized(FIT_EPSILON, 0.0), 0.0, 100.0).map(|plan| plan.size),
        Ok(Size::ZERO)
    );
    assert_eq!(
        plan(&sized(above(FIT_EPSILON), 0.0), 0.0, 100.0),
        Err(no_fit(above(FIT_EPSILON), 0.0))
    );
}

#[test]
fn a_leaf_at_a_large_offer_fits_only_the_offer() {
    assert_eq!(
        plan(&sized(LARGE + FIT_EPSILON, 0.0), LARGE, 100.0).map(|plan| plan.size),
        Ok(Size::new(LARGE, 0.0))
    );
    assert_eq!(
        plan(&sized(above(LARGE), 0.0), LARGE, 100.0),
        Err(no_fit(above(LARGE), LARGE))
    );
}

// A viewport's view against the height it is offered: with a floor of 0, in
// a room 0 high, it is offered 0. The plan keeps the height offered.
#[test]
fn a_viewport_fits_up_to_the_tolerance_taller_than_its_offer() {
    let viewport = |height| shaped_viewport(move |_| Size::new(0.0, height), Size::ZERO);
    assert_eq!(
        plan(&viewport(FIT_EPSILON), 0.0, 0.0).map(|plan| plan.size),
        Ok(Size::ZERO)
    );
    assert_eq!(
        plan(&viewport(above(FIT_EPSILON)), 0.0, 0.0),
        Err(no_fit(above(FIT_EPSILON), 0.0))
    );
}

#[test]
fn a_viewport_at_a_large_offer_fits_only_the_offer() {
    let viewport = |height| shaped_viewport(move |_| Size::new(0.0, height), Size::ZERO);
    assert_eq!(
        plan(&viewport(LARGE + FIT_EPSILON), 0.0, LARGE).map(|plan| plan.size),
        Ok(Size::new(0.0, LARGE))
    );
    assert_eq!(
        plan(&viewport(above(LARGE)), 0.0, LARGE),
        Err(no_fit(above(LARGE), LARGE))
    );
}

// A plan against its room. Two floors of half the tolerance, which nothing
// can shrink, are allocated in full in a row 0 wide (allocation allows the
// tolerance), and the row, FIT_EPSILON wide, fits. No plan wider than that
// reaches this comparison: the checks before it refuse it, with the same
// tolerance.
#[test]
fn a_plan_fits_up_to_the_tolerance_wider_than_its_room() {
    let half = FIT_EPSILON / 2.0;
    let row = row_child(vec![floor(half), floor(half)], 0.0);
    let plan = plan(&row, 0.0, 100.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![(0, [0.0, 0.0], [half, 0.0]), (1, [half, 0.0], [half, 0.0])]
    );
    assert_eq!(plan.size, Size::new(FIT_EPSILON, 0.0));
}

// Floors that overrun the room within the tolerance leave no surplus, and a
// part that grows is given none: never less than nothing.
#[test]
fn floors_over_the_room_within_the_tolerance_leave_a_growing_part_nothing() {
    let row = row_child(vec![floor(FIT_EPSILON), part(0.0, 0.0, 0.0).grow(1.0)], 0.0);
    let plan = plan(&row, 0.0, 100.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 0.0], [FIT_EPSILON, 0.0]),
            (1, [FIT_EPSILON, 0.0], [0.0, 0.0])
        ]
    );
}

// At a large width the row's f32 sum loses the second floor: the row is
// LARGE wide, its second part placed at LARGE, FIT_EPSILON wide.
#[test]
fn a_plan_at_a_large_room_loses_the_tolerance_in_its_sum() {
    let row = row_child(vec![floor(LARGE), floor(FIT_EPSILON)], 0.0);
    let plan = plan(&row, LARGE, 100.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 0.0], [LARGE, 0.0]),
            (1, [LARGE, 0.0], [FIT_EPSILON, 0.0])
        ]
    );
    assert_eq!(plan.size, Size::new(LARGE, 0.0));
}

// An ordinary leaf's height against the room: the plan takes the leaf's.
#[test]
fn a_leaf_fits_up_to_the_tolerance_taller_than_its_room() {
    assert_eq!(
        plan(&sized(0.0, FIT_EPSILON), 0.0, 0.0).map(|plan| plan.size),
        Ok(Size::new(0.0, FIT_EPSILON))
    );
    assert_eq!(
        plan(&sized(0.0, above(FIT_EPSILON)), 0.0, 0.0),
        Err(no_fit(above(FIT_EPSILON), 0.0))
    );
}

#[test]
fn a_leaf_at_a_large_room_fits_only_the_room_in_height() {
    assert_eq!(
        plan(&sized(0.0, LARGE + FIT_EPSILON), 0.0, LARGE).map(|plan| plan.size),
        Ok(Size::new(0.0, LARGE))
    );
    assert_eq!(
        plan(&sized(0.0, above(LARGE)), 0.0, LARGE),
        Err(no_fit(above(LARGE), LARGE))
    );
}

// A column's share of height against a child's planned height. In a room 0
// high a viewport FIT_EPSILON high is within the tolerance of its share, 0,
// and keeps its plan; one f32 higher, it is planned again at 0.
#[test]
fn a_column_keeps_a_plan_within_the_tolerance_of_its_share() {
    let keeps = plan(
        &column_child(vec![scrolling(0.0, FIT_EPSILON, 0.0)], 0.0),
        0.0,
        0.0,
    )
    .unwrap();
    assert_eq!(placed(&keeps), vec![(0, [0.0, 0.0], [0.0, FIT_EPSILON])]);
    let replanned = plan(
        &column_child(vec![scrolling(0.0, above(FIT_EPSILON), 0.0)], 0.0),
        0.0,
        0.0,
    )
    .unwrap();
    assert_eq!(placed(&replanned), vec![(0, [0.0, 0.0], [0.0, 0.0])]);
}

#[test]
fn a_column_at_a_large_room_plans_again_one_f32_over_its_share() {
    let column = column_child(vec![scrolling(0.0, above(LARGE), 0.0)], 0.0);
    let plan = plan(&column, 0.0, LARGE).unwrap();
    assert_eq!(placed(&plan), vec![(0, [0.0, 0.0], [0.0, LARGE])]);
}

// A row's floors, child by child, before they are allocated together. A
// floor of FIT_EPSILON fits in 0; one f32 more fails the row with that
// child's floor, not the row's total.
#[test]
fn a_row_checks_each_floor_up_to_the_tolerance_over_its_width() {
    let fits = plan(&row_child(vec![floor(FIT_EPSILON)], 0.0), 0.0, 100.0);
    assert_eq!(fits.map(|plan| plan.size), Ok(Size::new(FIT_EPSILON, 0.0)));
    let over = row_child(
        vec![floor(FIT_EPSILON / 4.0), floor(above(FIT_EPSILON))],
        0.0,
    );
    assert_eq!(
        plan(&over, 0.0, 100.0),
        Err(no_fit(above(FIT_EPSILON), 0.0))
    );
}

#[test]
fn a_row_at_a_large_width_fits_only_floors_within_it() {
    let fits = plan(&row_child(vec![floor(LARGE)], 0.0), LARGE, 100.0);
    assert_eq!(fits.map(|plan| plan.size), Ok(Size::new(LARGE, 0.0)));
    assert_eq!(
        plan(&row_child(vec![floor(above(LARGE))], 0.0), LARGE, 100.0),
        Err(no_fit(above(LARGE), LARGE))
    );
}

// A wrap's floors, child by child. One f32 more than the tolerance fails the
// wrap with that child's floor.
#[test]
fn a_wrap_checks_each_floor_up_to_the_tolerance_over_its_width() {
    let fits = plan(&wrap_child(vec![floor(FIT_EPSILON)], 0.0), 0.0, 100.0);
    assert_eq!(fits.map(|plan| plan.size), Ok(Size::new(FIT_EPSILON, 0.0)));
    assert_eq!(
        plan(
            &wrap_child(vec![floor(above(FIT_EPSILON))], 0.0),
            0.0,
            100.0
        ),
        Err(no_fit(above(FIT_EPSILON), 0.0))
    );
}

#[test]
fn a_wrap_at_a_large_width_fits_only_floors_within_it() {
    let fits = plan(&wrap_child(vec![floor(LARGE)], 0.0), LARGE, 100.0);
    assert_eq!(fits.map(|plan| plan.size), Ok(Size::new(LARGE, 0.0)));
    assert_eq!(
        plan(&wrap_child(vec![floor(above(LARGE))], 0.0), LARGE, 100.0),
        Err(no_fit(above(LARGE), LARGE))
    );
}

// A wrap packs a child into the row it is filling while the row's floors
// come to no more than the width plus the tolerance. Two halves of the
// tolerance share a row 0 wide; when the second is one f32 wider, so that
// together they are one f32 over, it starts a row of its own. (Both sums are
// exact: `wider` is half the tolerance and one f32 of the tolerance.)
#[test]
fn a_wrap_packs_floors_into_one_row_up_to_the_tolerance_over_its_width() {
    let half = FIT_EPSILON / 2.0;
    let one_row = plan(&wrap_child(vec![floor(half), floor(half)], 0.0), 0.0, 100.0);
    assert_eq!(one_row.map(|plan| plan.rows), Ok(vec![vec![0, 1]]));
    let wider = above(FIT_EPSILON) - half;
    assert_eq!(half + wider, above(FIT_EPSILON));
    let two_rows = plan(
        &wrap_child(vec![floor(half), floor(wider)], 0.0),
        0.0,
        100.0,
    );
    assert_eq!(two_rows.map(|plan| plan.rows), Ok(vec![vec![0], vec![1]]));
}

#[test]
fn a_wrap_at_a_large_width_packs_only_floors_within_it() {
    let one_row = plan(
        &wrap_child(vec![floor(30_000.0), floor(10_000.0)], 0.0),
        LARGE,
        100.0,
    );
    assert_eq!(one_row.map(|plan| plan.rows), Ok(vec![vec![0, 1]]));
    let wider = above(LARGE) - 30_000.0;
    assert_eq!(30_000.0 + wider, above(LARGE));
    let two_rows = plan(
        &wrap_child(vec![floor(30_000.0), floor(wider)], 0.0),
        LARGE,
        100.0,
    );
    assert_eq!(two_rows.map(|plan| plan.rows), Ok(vec![vec![0], vec![1]]));
}

// Two parts with no width and a gap of FIT_EPSILON between them pack into one
// row 0 wide, within the tolerance; but the row's gaps are then checked
// against the width with no tolerance, and the wrap fails, though with a row
// each the parts fit, as they do when the gap is one f32 wider. That pins
// present behaviour, and was reported.
#[test]
fn a_wrap_packed_within_the_tolerance_fails_on_its_gap_instead_of_wrapping() {
    let wrap = |spacing| wrap_child(vec![floor(0.0), floor(0.0)], spacing);
    assert_eq!(
        plan(&wrap(FIT_EPSILON), 0.0, 100.0),
        Err(no_fit(FIT_EPSILON, 0.0))
    );
    assert_eq!(
        plan(&wrap(above(FIT_EPSILON)), 0.0, 100.0).map(|plan| plan.rows),
        Ok(vec![vec![0], vec![1]])
    );
}

// A wrap's rows against its room's height. One viewport FIT_EPSILON high in
// a room 0 high is within the tolerance, and the wrap keeps it; one f32
// higher, the wrap shares the room's 0 with its row, and the viewport is
// planned again at 0.
#[test]
fn a_wrap_shares_its_height_only_beyond_the_tolerance_over_its_room() {
    let keeps = plan(
        &wrap_child(vec![scrolling(0.0, FIT_EPSILON, 0.0)], 0.0),
        0.0,
        0.0,
    )
    .unwrap();
    assert_eq!(placed(&keeps), vec![(0, [0.0, 0.0], [0.0, FIT_EPSILON])]);
    assert_eq!(keeps.size, Size::new(0.0, FIT_EPSILON));
    let shared = plan(
        &wrap_child(vec![scrolling(0.0, above(FIT_EPSILON), 0.0)], 0.0),
        0.0,
        0.0,
    )
    .unwrap();
    assert_eq!(placed(&shared), vec![(0, [0.0, 0.0], [0.0, 0.0])]);
    assert_eq!(shared.size, Size::ZERO);
}

#[test]
fn a_wrap_at_a_large_room_shares_its_height_one_f32_over_it() {
    let wrap = wrap_child(vec![scrolling(0.0, above(LARGE), 0.0)], 0.0);
    let plan = plan(&wrap, 0.0, LARGE).unwrap();
    assert_eq!(placed(&plan), vec![(0, [0.0, 0.0], [0.0, LARGE])]);
    assert_eq!(plan.size, Size::new(0.0, LARGE));
}

// A placement against its row's height after the wrap shares a shortage. Two
// viewports, each on a row of its own, in a room 0 high: both rows go to 0.
// A viewport FIT_EPSILON high is within the tolerance of its row and keeps
// its plan, centred on the row (so it starts half of itself above it); one
// f32 higher, it is planned again at 0.
#[test]
fn a_wrap_keeps_a_placement_within_the_tolerance_of_its_rows_height() {
    let wrap = |first| {
        wrap_child(
            vec![scrolling(1.0, first, 0.0), scrolling(1.0, FIT_EPSILON, 0.0)],
            0.0,
        )
    };
    let half = FIT_EPSILON / 2.0;
    let keeps = plan(&wrap(FIT_EPSILON), 1.0, 0.0).unwrap();
    assert_eq!(keeps.rows, vec![vec![0], vec![1]]);
    assert_eq!(
        placed(&keeps),
        vec![
            (0, [0.0, -half], [1.0, FIT_EPSILON]),
            (1, [0.0, -half], [1.0, FIT_EPSILON]),
        ]
    );
    let replanned = plan(&wrap(above(FIT_EPSILON)), 1.0, 0.0).unwrap();
    assert_eq!(
        placed(&replanned),
        vec![
            (0, [0.0, 0.0], [1.0, 0.0]),
            (1, [0.0, -half], [1.0, FIT_EPSILON]),
        ]
    );
}

// At a large room: the first row, one f32 above LARGE with a floor of LARGE,
// and the second, 1/256 high with a floor of 0, share a shortage of 2/256
// equally, each going to its floor; both placements are then more than the
// tolerance taller than their rows and are planned again.
#[test]
fn a_wrap_at_a_large_room_plans_a_placement_again_one_f32_over_its_row() {
    let wrap = wrap_child(
        vec![
            scrolling(1.0, above(LARGE), LARGE),
            scrolling(1.0, 1.0 / 256.0, 0.0),
        ],
        0.0,
    );
    let plan = plan(&wrap, 1.0, LARGE).unwrap();
    assert_eq!(
        placed(&plan),
        vec![(0, [0.0, 0.0], [1.0, LARGE]), (1, [0.0, LARGE], [1.0, 0.0])]
    );
}

// A frame's padding against its room. Padding of FIT_EPSILON fits in a room
// 0 wide (or high) round content of no size, which is planned at 0, never
// less; one f32 more fails with the padding and the room, before the 5 × 8
// content is planned.
#[test]
fn a_frame_fits_up_to_the_tolerance_of_padding_over_its_room() {
    let empty = || column_child(vec![sized(0.0, 0.0)], 0.0);
    let wide = plan(&padded(empty(), left(FIT_EPSILON)), 0.0, 100.0).unwrap();
    assert_eq!(placed(&wide), vec![(0, [FIT_EPSILON, 0.0], [0.0, 0.0])]);
    assert_eq!(wide.size, Size::new(FIT_EPSILON, 0.0));
    let high = plan(&padded(empty(), top(FIT_EPSILON)), 100.0, 0.0).unwrap();
    assert_eq!(placed(&high), vec![(0, [0.0, FIT_EPSILON], [0.0, 0.0])]);
    assert_eq!(high.size, Size::new(0.0, FIT_EPSILON));
    assert_eq!(
        plan(
            &padded(sized(5.0, 8.0), left(above(FIT_EPSILON))),
            0.0,
            100.0
        ),
        Err(no_fit(above(FIT_EPSILON), 0.0))
    );
    assert_eq!(
        plan(
            &padded(sized(5.0, 8.0), top(above(FIT_EPSILON))),
            100.0,
            0.0
        ),
        Err(no_fit(above(FIT_EPSILON), 0.0))
    );
}

#[test]
fn a_frame_at_a_large_room_fits_only_padding_within_it() {
    let fits = plan(
        &padded(sized(0.0, 0.0), left(LARGE + FIT_EPSILON)),
        LARGE,
        100.0,
    );
    assert_eq!(fits.map(|plan| plan.size), Ok(Size::new(LARGE, 0.0)));
    assert_eq!(
        plan(&padded(sized(5.0, 8.0), left(above(LARGE))), LARGE, 100.0),
        Err(no_fit(above(LARGE), LARGE))
    );
    assert_eq!(
        plan(&padded(sized(5.0, 8.0), top(above(LARGE))), 100.0, LARGE),
        Err(no_fit(above(LARGE), LARGE))
    );
}

// The gaps of a column, a row and a wrap's rows are compared with the room
// with no tolerance: gaps of exactly the room fit, one f32 more fail.
#[test]
fn a_columns_gaps_fit_exactly_and_no_further() {
    let column = |spacing| column_child(vec![sized(0.0, 0.0), sized(0.0, 0.0)], spacing);
    assert_eq!(
        plan(&column(1.0), 0.0, 1.0).map(|plan| plan.size),
        Ok(Size::new(0.0, 1.0))
    );
    assert_eq!(
        plan(&column(above(1.0)), 0.0, 1.0),
        Err(no_fit(above(1.0), 1.0))
    );
}

#[test]
fn a_rows_gaps_fit_exactly_and_no_further() {
    let row = |spacing| row_child(vec![floor(0.0), floor(0.0)], spacing);
    assert_eq!(
        plan(&row(1.0), 1.0, 100.0).map(|plan| plan.size),
        Ok(Size::new(1.0, 0.0))
    );
    assert_eq!(
        plan(&row(above(1.0)), 1.0, 100.0),
        Err(no_fit(above(1.0), 1.0))
    );
}

// Two viewports 1 high with floors of 0, a row each, in a room exactly as
// high as the gap between their rows: the rows go to 0, and the second row
// starts after the gap. With a gap one f32 higher than the room the wrap
// fails.
#[test]
fn a_wraps_gaps_between_rows_fit_exactly_and_no_further() {
    let wrap = |spacing| {
        wrap_child(
            vec![scrolling(1.0, 1.0, 0.0), scrolling(1.0, 1.0, 0.0)],
            spacing,
        )
    };
    let plan_fits = plan(&wrap(1.0), 1.0, 1.0).unwrap();
    assert_eq!(
        placed(&plan_fits),
        vec![(0, [0.0, 0.0], [1.0, 0.0]), (1, [0.0, 1.0], [1.0, 0.0])]
    );
    assert_eq!(plan_fits.size, Size::new(1.0, 1.0));
    assert_eq!(
        plan(&wrap(above(1.0)), 1.0, 1.0),
        Err(no_fit(above(1.0), 1.0))
    );
}
