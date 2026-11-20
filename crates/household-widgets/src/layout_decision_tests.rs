// SPDX-License-Identifier: GPL-3.0-only

//! Decisions of the geometry that no other test holds, found by changing each
//! and seeing whether a test failed: growth in height passed through forms
//! and a frame to what they hold, a column's measured floor of height, and
//! the gaps a wrap counts when it packs a row.

use super::test_fixtures::*;
use super::*;

/// A viewport that fills the height it is offered, up to 60, with a floor of
/// 80 × 10.
fn viewport() -> Child<'static, ()> {
    shaped_viewport(
        |offer| Size::new(offer.width.min(80.0), offer.height.min(60.0)),
        Size::new(80.0, 10.0),
    )
}

/// A viewport whose view takes all the height it is offered, and 60 when
/// unbounded, with a floor of 80 × 10.
fn filling() -> Child<'static, ()> {
    shaped_viewport(
        |offer| {
            let height = if offer.height.is_finite() {
                offer.height
            } else {
                60.0
            };
            Size::new(offer.width.min(80.0), height)
        },
        Size::new(80.0, 10.0),
    )
}

// Forms that grow in height pass that on to the form selected: the viewport
// is offered the whole 200, and its view laid out at that height, not at its
// preferred 60 inside a box of 200.
#[test]
fn forms_growing_in_height_offer_the_viewport_they_select_the_whole_height() {
    let family = forms(vec![(Level::Preferred, filling())]).grow_height(1.0);
    let plan = query_at(&family, 80.0, 200.0).unwrap();
    assert_eq!(plan.size, Size::new(80.0, 200.0));
    assert_eq!(plan.native_size, Some(Size::new(80.0, 200.0)));
}

// A frame that grows in height passes that on to its content: the viewport
// takes the 196 inside the padding, not its preferred 60.
#[test]
fn a_frame_growing_in_height_offers_its_viewport_the_height_inside_its_padding() {
    let frame = framed_child(viewport(), 2.0, |content| content).grow_height(1.0);
    let plan = query_at(&frame, 84.0, 200.0).unwrap();
    assert_eq!(placed(&plan), vec![(0, [2.0, 2.0], [80.0, 196.0])]);
    assert_eq!(plan.size, Size::new(84.0, 200.0));
}

// A column's measured floor of height is its children's floors and its gap:
// the viewport's 10, the part's 20 and 5, not their preferred 60 and 20.
#[test]
fn a_columns_measured_floor_of_height_adds_its_childrens_floors() {
    let column = column_child(vec![viewport(), part(80.0, 80.0, 20.0)], 5.0);
    assert_eq!(
        measure(&column),
        MeasuredSize {
            preferred: Size::new(80.0, 85.0),
            minimum: Size::new(80.0, 35.0),
        }
    );
}

// 40 and 40 fit in 85 without the gap of 10 between them, but not with it:
// the second starts a row of its own.
#[test]
fn a_wrap_counts_the_gap_when_it_packs_a_row() {
    let wrap = wrap_child(vec![part(40.0, 40.0, 10.0), part(40.0, 40.0, 10.0)], 10.0);
    let plan = plan(&wrap, 85.0, 100.0).unwrap();
    assert_eq!(plan.rows, vec![vec![0], vec![1]]);
    assert_eq!(plan.size, Size::new(40.0, 30.0));
}
