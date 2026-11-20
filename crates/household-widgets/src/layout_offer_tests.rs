// SPDX-License-Identifier: GPL-3.0-only

//! Rooms and measurements that are not sizes.
//!
//! Every public query and planning entry refuses a room that is NaN or
//! negative in either dimension (negative infinity included): `query_at`
//! and `choose_stage` on every kind of child, `allocate_width` and
//! `first_fitting`. An infinite room is accepted, except by `allocate_width`,
//! which shares a finite width only. A leaf whose view measures NaN or a
//! negative size, unbounded or at the width it is allocated, is invalid input
//! wherever it is; so is a measurement whose floor exceeds its preferred
//! size, and a declared width or viewport floor that is not a size. (Bad
//! numbers given to the builders are `layout_invalid_tests.rs`.)

use super::test_fixtures::*;
use super::*;

/// Numbers that are not a size in any dimension.
const NOT_SIZES: [f32; 3] = [f32::NAN, -1.0, f32::NEG_INFINITY];

/// Every kind of child, each 40 × 20 or about that.
fn kinds() -> Vec<(&'static str, Child<'static, ()>)> {
    let pair = || vec![sized(40.0, 20.0), sized(40.0, 20.0)];
    vec![
        ("leaf", sized(40.0, 20.0)),
        ("row", row_child(pair(), 4.0)),
        ("top-aligned row", row_child_top(pair(), 4.0)),
        ("column", column_child(pair(), 4.0)),
        ("wrap", wrap_child(pair(), 4.0)),
        (
            "frame",
            framed_child(sized(40.0, 20.0), 2.0, |content| content),
        ),
        ("forms", forms(vec![(Level::Preferred, sized(40.0, 20.0))])),
        (
            "viewport",
            shaped_viewport(
                |offer| Size::new(offer.width.min(40.0), offer.height.min(60.0)),
                Size::new(40.0, 10.0),
            ),
        ),
        ("fill_width", sized(40.0, 20.0).fill_width()),
        (
            "fixed_dimensions",
            sized(40.0, 20.0).fixed_dimensions(Some(50.0), Some(30.0)),
        ),
        ("spacer", spacer(10.0)),
    ]
}

#[test]
fn every_kind_of_child_refuses_a_room_that_is_not_a_size() {
    let renderer = renderer();
    for (kind, child) in kinds() {
        for bad in NOT_SIZES {
            for room in [Size::new(bad, 100.0), Size::new(100.0, bad)] {
                assert_eq!(
                    child.query_at(Level::Preferred, &renderer, room),
                    Err(AllocationError::InvalidInput),
                    "{kind} in {room:?}"
                );
            }
        }
    }
}

#[test]
fn every_kind_of_child_accepts_an_unbounded_room() {
    let renderer = renderer();
    for (kind, child) in kinds() {
        for room in [
            Size::new(f32::INFINITY, 100.0),
            Size::new(100.0, f32::INFINITY),
            Size::INFINITE,
        ] {
            assert!(
                child.query_at(Level::Preferred, &renderer, room).is_ok(),
                "{kind} in {room:?}"
            );
        }
    }
}

#[test]
fn choose_stage_given_a_room_that_is_not_a_size_fits_no_stage() {
    let renderer = renderer();
    let declared = vec![
        Stage::new(Level::Preferred, sized(40.0, 20.0)),
        Stage::new(
            Level::Mild,
            row_child(vec![sized(40.0, 20.0), sized(40.0, 20.0)], 4.0),
        ),
    ];
    for bad in NOT_SIZES {
        for room in [Size::new(bad, 100.0), Size::new(100.0, bad)] {
            assert_eq!(
                choose_stage(&declared, &renderer, room),
                Err(NoFit { attempted: 2 }),
                "{room:?}"
            );
        }
    }
}

fn capacity(preferred: f32, minimum: f32, weight: f32) -> WidthCapacity {
    WidthCapacity {
        preferred,
        minimum,
        weight,
    }
}

#[test]
fn allocate_width_refuses_a_width_that_is_not_a_finite_size() {
    for width in [f32::NAN, -1.0, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            allocate_width(width, &[capacity(10.0, 5.0, 1.0)]),
            Err(AllocationError::InvalidInput),
            "{width}"
        );
    }
}

// Each capacity breaks one condition only.
#[test]
fn allocate_width_refuses_a_capacity_that_is_not_one() {
    for (what, bad) in [
        ("NaN preferred", capacity(f32::NAN, 5.0, 1.0)),
        ("infinite preferred", capacity(f32::INFINITY, 5.0, 1.0)),
        ("NaN minimum", capacity(10.0, f32::NAN, 1.0)),
        ("negative minimum", capacity(10.0, -1.0, 1.0)),
        ("minimum above preferred", capacity(5.0, 10.0, 1.0)),
        ("NaN weight", capacity(10.0, 5.0, f32::NAN)),
        ("infinite weight", capacity(10.0, 5.0, f32::INFINITY)),
        ("negative weight", capacity(10.0, 5.0, -1.0)),
    ] {
        assert_eq!(
            allocate_width(100.0, &[bad]),
            Err(AllocationError::InvalidInput),
            "{what}"
        );
    }
}

#[test]
fn first_fitting_given_a_room_that_is_not_a_size_fits_nothing() {
    let measured = |_| Size::new(10.0, 10.0);
    for bad in NOT_SIZES {
        for room in [Size::new(bad, 100.0), Size::new(100.0, bad)] {
            assert_eq!(
                first_fitting(Level::AllPermitted, room, measured),
                None,
                "{room:?}"
            );
        }
    }
    assert_eq!(
        first_fitting(Level::AllPermitted, Size::INFINITE, measured),
        Some(Level::Preferred)
    );
}

// Not even unbounded room takes a representation that measures no size.
#[test]
fn first_fitting_takes_no_representation_that_measures_no_size() {
    for size in [
        Size::new(f32::INFINITY, 10.0),
        Size::new(10.0, f32::INFINITY),
        Size::new(-1.0, 10.0),
        Size::new(10.0, -1.0),
        Size::new(10.0, f32::NAN),
    ] {
        assert_eq!(
            first_fitting(Level::AllPermitted, Size::INFINITE, |_| size),
            None,
            "{size:?}"
        );
    }
}

fn not_sizes() -> [Size; 4] {
    [
        Size::new(f32::NAN, 20.0),
        Size::new(40.0, f32::NAN),
        Size::new(-1.0, 20.0),
        Size::new(40.0, -1.0),
    ]
}

/// Every way a parent other than a frame can include `bad`, each named for
/// the failure message.
fn parents(bad: &Child<'static, ()>) -> Vec<(&'static str, Child<'static, ()>)> {
    vec![
        ("alone", bad.clone()),
        ("row", row_child(vec![sized(40.0, 20.0), bad.clone()], 0.0)),
        (
            "column",
            column_child(vec![sized(40.0, 20.0), bad.clone()], 0.0),
        ),
        (
            "wrap",
            wrap_child(vec![sized(40.0, 20.0), bad.clone()], 0.0),
        ),
        ("forms", forms(vec![(Level::Preferred, bad.clone())])),
    ]
}

fn framed(content: Child<'static, ()>) -> Child<'static, ()> {
    framed_child(content, 2.0, |content| content)
}

/// A leaf whose view takes `size` unbounded and 40 × 20 at any width it is
/// given.
fn measuring(size: Size) -> Child<'static, ()> {
    shaped(move |offer| {
        if offer.width.is_infinite() {
            size
        } else {
            Size::new(40.0, 20.0)
        }
    })
}

// A frame is left out here: see the next two tests.
#[test]
fn a_leaf_that_measures_no_size_is_invalid_input_wherever_it_is() {
    for size in not_sizes() {
        let bad = measuring(size);
        assert_eq!(
            validate_measurement(measure(&bad)),
            Err(AllocationError::InvalidInput),
            "{size:?}"
        );
        for (path, parent) in parents(&bad) {
            assert_eq!(
                query_at(&parent, 300.0, 300.0),
                Err(AllocationError::InvalidInput),
                "{size:?}, {path}"
            );
        }
    }
}

#[test]
fn a_frame_round_a_leaf_that_measures_nan_is_invalid_input() {
    for size in [Size::new(f32::NAN, 20.0), Size::new(40.0, f32::NAN)] {
        assert_eq!(
            query_at(&framed(measuring(size)), 300.0, 300.0),
            Err(AllocationError::InvalidInput),
            "{size:?}"
        );
    }
}

// A frame adds its padding to its content's measurement without checking
// it, so content measuring -1 wide or high, padded by 2 on each side,
// measures as a size, 3 wide or 3 high. Planned, the negative width fails
// with the padding, 4, against the 3 the frame was allocated; the negative
// height fits, at the 20 the view takes when planned. That pins present
// behaviour, and was reported: every other parent refuses such content as
// invalid input.
#[test]
fn a_frame_takes_a_negative_measurement_of_its_content_for_a_size() {
    let wide = framed(measuring(Size::new(-1.0, 20.0)));
    assert_eq!(
        measure(&wide),
        MeasuredSize {
            preferred: Size::new(3.0, 24.0),
            minimum: Size::new(3.0, 24.0),
        }
    );
    assert_eq!(query_at(&wide, 300.0, 300.0), Err(no_fit(4.0, 3.0)));
    let high = framed(measuring(Size::new(40.0, -1.0)));
    assert_eq!(
        measure(&high),
        MeasuredSize {
            preferred: Size::new(44.0, 3.0),
            minimum: Size::new(44.0, 3.0),
        }
    );
    assert_eq!(
        query_at(&high, 300.0, 300.0).map(|plan| plan.size),
        Ok(Size::new(44.0, 24.0))
    );
}

// The view measures 40 × 20 unbounded, but `size` at any width it is given.
#[test]
fn a_leaf_that_takes_no_size_at_its_allocated_width_is_invalid_input_wherever_it_is() {
    for size in not_sizes() {
        let bad = shaped(move |offer| {
            if offer.width.is_infinite() {
                Size::new(40.0, 20.0)
            } else {
                size
            }
        });
        assert_eq!(validate_measurement(measure(&bad)), Ok(()), "{size:?}");
        let mut parents = parents(&bad);
        parents.push(("frame", framed(bad.clone())));
        for (path, parent) in parents {
            assert_eq!(
                query_at(&parent, 300.0, 300.0),
                Err(AllocationError::InvalidInput),
                "{size:?}, {path}"
            );
        }
    }
}

// A viewport whose view measures no size is an invalid measurement, before
// it is planned.
#[test]
fn a_viewport_that_measures_no_size_is_an_invalid_measurement() {
    for size in not_sizes() {
        let viewport = shaped_viewport(
            move |offer| {
                if offer.width.is_infinite() {
                    size
                } else {
                    Size::new(40.0, offer.height.min(20.0))
                }
            },
            Size::new(40.0, 10.0),
        );
        assert_eq!(
            validate_measurement(measure(&viewport)),
            Err(AllocationError::InvalidInput),
            "{size:?}"
        );
        assert_eq!(
            query_at(&viewport, 300.0, 300.0),
            Err(AllocationError::InvalidInput),
            "{size:?}"
        );
    }
}

#[test]
fn a_measurement_whose_floor_exceeds_its_preferred_size_is_invalid_input() {
    for minimum in [Size::new(50.0, 20.0), Size::new(40.0, 30.0)] {
        let bad = sized(40.0, 20.0).with_measurement(move |_, _| MeasuredSize {
            preferred: Size::new(40.0, 20.0),
            minimum,
        });
        assert_eq!(
            query_at(&bad, 300.0, 300.0),
            Err(AllocationError::InvalidInput),
            "{minimum:?}"
        );
    }
}

#[test]
fn a_preferred_width_that_is_not_a_size_makes_the_measurement_invalid() {
    for width in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0] {
        let bad = sized(40.0, 20.0).preferred_width(width);
        assert!(measure(&bad).preferred.width.is_nan(), "{width}");
        assert_eq!(
            query_at(&bad, 300.0, 300.0),
            Err(AllocationError::InvalidInput),
            "{width}"
        );
    }
}

#[test]
fn a_minimum_width_that_is_not_a_size_makes_the_measurement_invalid() {
    for floor in [f32::NAN, f32::INFINITY, -1.0] {
        let bad = sized(40.0, 20.0).minimum_width(move |_, _| floor);
        assert!(measure(&bad).minimum.width.is_nan(), "{floor}");
        assert_eq!(
            query_at(&bad, 300.0, 300.0),
            Err(AllocationError::InvalidInput),
            "{floor}"
        );
    }
}

// A minimum width needs a preferred width that is a size to raise.
#[test]
fn a_minimum_width_after_a_preferred_width_that_is_not_a_size_is_invalid() {
    for width in [f32::NAN, -1.0] {
        let bad = sized(40.0, 20.0)
            .with_measurement(move |_, _| MeasuredSize {
                preferred: Size::new(width, 20.0),
                minimum: Size::new(0.0, 20.0),
            })
            .minimum_width(|_, _| 30.0);
        assert!(measure(&bad).minimum.width.is_nan(), "{width}");
    }
}

// A floor measured by the caller (`scroll_child_with_minimum`) that is not a
// size makes the viewport's whole measurement invalid.
#[test]
fn a_viewport_floor_that_is_not_a_size_makes_the_measurement_invalid() {
    for floor in [
        Size::new(f32::NAN, 10.0),
        Size::new(40.0, f32::INFINITY),
        Size::new(-1.0, 10.0),
    ] {
        let viewport = scroll_child_with_minimum(
            view_of(|offer| Size::new(offer.width.min(40.0), offer.height.min(60.0))),
            move |_, _| floor,
        );
        let measured = measure(&viewport);
        assert!(
            measured.preferred.width.is_nan() && measured.preferred.height.is_nan(),
            "{floor:?}"
        );
        assert_eq!(
            query_at(&viewport, 300.0, 300.0),
            Err(AllocationError::InvalidInput),
            "{floor:?}"
        );
    }
}

// A viewport whose view is smaller than its floor prefers the floor, and is
// planned at it.
#[test]
fn a_viewport_smaller_than_its_floor_prefers_the_floor() {
    let viewport = shaped_viewport(
        |offer| Size::new(offer.width.min(30.0), offer.height.min(5.0)),
        Size::new(40.0, 10.0),
    );
    assert_eq!(
        measure(&viewport),
        MeasuredSize {
            preferred: Size::new(40.0, 10.0),
            minimum: Size::new(40.0, 10.0),
        }
    );
    assert_eq!(
        query_at(&viewport, 100.0, 100.0).map(|plan| plan.size),
        Ok(Size::new(40.0, 10.0))
    );
}
