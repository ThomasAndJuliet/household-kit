// SPDX-License-Identifier: GPL-3.0-only

//! A leaf whose native view takes more than it is offered.
//!
//! A leaf is planned at the width it is allocated, and its view laid out
//! there. A view wider than that, a leaf taller than its room, or a viewport
//! taller than the height it is offered does not fit: the stage fails with
//! the size the view took as `required` and the offer as `available`. An
//! overflow within the tolerance (`FIT_EPSILON`, 0.001) fits; the plan keeps
//! the width allocated, and an ordinary leaf the height its view took.

use super::test_fixtures::*;
use super::*;

#[test]
fn a_leaf_wider_than_its_offer_fails_with_its_width_and_the_offer() {
    assert_eq!(
        plan(&sized(51.0, 20.0), 50.0, 100.0),
        Err(no_fit(51.0, 50.0))
    );
}

// The row allocates the leaf the 50 it has, which the leaf's declared floor
// of 40 allows; its view still takes 51.
#[test]
fn a_leaf_wider_than_the_width_its_row_allocates_fails_the_row() {
    let row = row_child(vec![sized(51.0, 20.0).minimum_width(|_, _| 40.0)], 0.0);
    assert_eq!(query_at(&row, 50.0, 100.0), Err(no_fit(51.0, 50.0)));
}

#[test]
fn a_leaf_taller_than_its_room_fails_with_its_height_and_the_room() {
    assert_eq!(
        plan(&sized(40.0, 21.0), 40.0, 20.0),
        Err(no_fit(21.0, 20.0))
    );
}

// Offered 30 of its 60, the viewport's view takes 5 more than that.
#[test]
fn a_viewport_taller_than_the_height_it_is_offered_fails_with_both() {
    let viewport = shaped_viewport(
        |offer| {
            let height = if offer.height.is_finite() {
                offer.height + 5.0
            } else {
                60.0
            };
            Size::new(40.0, height)
        },
        Size::new(40.0, 10.0),
    );
    assert_eq!(plan(&viewport, 40.0, 30.0), Err(no_fit(35.0, 30.0)));
}

#[test]
fn a_viewport_wider_than_its_width_fails_with_both() {
    let viewport = shaped_viewport(
        |offer| {
            let width = if offer.width.is_finite() {
                offer.width + 5.0
            } else {
                40.0
            };
            Size::new(width, 20.0)
        },
        Size::new(40.0, 10.0),
    );
    assert_eq!(plan(&viewport, 40.0, 30.0), Err(no_fit(45.0, 40.0)));
}

// A viewport is offered no less than its floor, so in a room lower than the
// floor it is laid out at the floor and fails there, growing or not, rather
// than being offered the room's height, in which its view would scroll.
#[test]
fn a_viewport_in_a_room_lower_than_its_floor_fails_at_its_floor() {
    let viewport = || {
        shaped_viewport(
            |offer| Size::new(40.0, offer.height.min(60.0)),
            Size::new(40.0, 10.0),
        )
    };
    assert_eq!(plan(&viewport(), 40.0, 5.0), Err(no_fit(10.0, 5.0)));
    assert_eq!(
        plan(&viewport().grow_height(1.0), 40.0, 5.0),
        Err(no_fit(10.0, 5.0))
    );
}

// Half the tolerance over: the leaf fits, at the width it was offered.
#[test]
fn a_leaf_wider_than_its_offer_by_less_than_the_tolerance_fits_at_the_offer() {
    let plan = plan(&sized(50.0005, 20.0), 50.0, 100.0).unwrap();
    assert_eq!(plan.size, Size::new(50.0, 20.0));
    assert_eq!(plan.native_size, Some(Size::new(50.0005, 20.0)));
}

// An ordinary leaf is planned at the height its view took, so half the
// tolerance over the room is planned as it is.
#[test]
fn a_leaf_taller_than_its_room_by_less_than_the_tolerance_fits_at_its_height() {
    let plan = plan(&sized(40.0, 20.0005), 40.0, 20.0).unwrap();
    assert_eq!(plan.size, Size::new(40.0, 20.0005));
}

// A viewport is planned at the height it was offered, though its view took
// half the tolerance more.
#[test]
fn a_viewport_taller_than_its_offer_by_less_than_the_tolerance_fits_at_the_offer() {
    let viewport = shaped_viewport(
        |offer| {
            let height = if offer.height.is_finite() {
                offer.height + 0.0005
            } else {
                60.0
            };
            Size::new(40.0, height)
        },
        Size::new(40.0, 10.0),
    );
    let plan = plan(&viewport, 40.0, 30.0).unwrap();
    assert_eq!(plan.size, Size::new(40.0, 30.0));
    assert_eq!(plan.native_size, Some(Size::new(40.0, 30.0 + 0.0005)));
}
