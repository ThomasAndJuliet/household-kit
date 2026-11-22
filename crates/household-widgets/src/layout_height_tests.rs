// SPDX-License-Identifier: GPL-3.0-only

//! How a child takes height, and the dimensions `fixed_dimensions` fixes.
//!
//! An ordinary leaf's measured height is a hard floor: offered less, it does
//! not fit. Only a declared viewport (`scroll_child`) is offered a finite
//! height, and it works in any height down to its declared floor. The
//! viewport here scrolls a box 80 wide and 60 high, with a floor of 80 × 10;
//! the header above it is 80 × 20. A fixed dimension is the requested number
//! or the content's measured minimum, whichever is larger, and a room smaller
//! than it fails the stage with the amounts needed and available.

use super::*;
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::{Font, Pixels};

fn renderer() -> Renderer {
    cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
        Font::default(),
        Pixels(14.0),
        Some("tiny-skia"),
    ))
    .expect("headless tiny-skia renderer")
}

fn header() -> Child<'static, ()> {
    child(|_| widget::Space::new().width(80).height(20).into())
}

fn viewport() -> Child<'static, ()> {
    scroll_child(
        |_| cosmic::iced::widget::scrollable(widget::Space::new().width(80).height(60)).into(),
        Size::new(80.0, 10.0),
    )
}

fn ordinary() -> Child<'static, ()> {
    child(|_| widget::Space::new().width(80).height(60).into())
}

/// The planned height of each child of a column planned in `room`.
fn heights(column: &Child<'_, ()>, room: Size) -> Result<Vec<f32>, AllocationError> {
    let plan = column.query_at(Level::Preferred, &renderer(), room)?;
    Ok(plan
        .children
        .iter()
        .map(|placed| placed.geometry.size.height)
        .collect())
}

#[test]
fn a_viewport_in_a_column_gives_height_down_to_its_floor() {
    let column = column_child(vec![header(), viewport()], 0.0);
    assert_eq!(
        heights(&column, Size::new(80.0, 1000.0)),
        Ok(vec![20.0, 60.0])
    );
    assert_eq!(
        heights(&column, Size::new(80.0, 50.0)),
        Ok(vec![20.0, 30.0])
    );
    assert_eq!(
        heights(&column, Size::new(80.0, 30.0)),
        Ok(vec![20.0, 10.0])
    );
    assert_eq!(
        heights(&column, Size::new(80.0, 29.0)),
        Err(AllocationError::NoFit {
            required: 30.0,
            available: 29.0,
        })
    );
    let plan = column
        .query_at(Level::Preferred, &renderer(), Size::new(80.0, 50.0))
        .unwrap();
    assert_eq!(plan.size, Size::new(80.0, 50.0));
    assert_eq!(plan.minimum_height(), 30.0);
    assert_eq!(plan.children[1].geometry.minimum_height(), 10.0);
}

#[test]
fn an_ordinary_leaf_in_a_column_keeps_its_height() {
    let column = column_child(vec![header(), ordinary()], 0.0);
    assert_eq!(
        heights(&column, Size::new(80.0, 1000.0)),
        Ok(vec![20.0, 60.0])
    );
    assert_eq!(
        heights(&column, Size::new(80.0, 50.0)),
        Err(AllocationError::NoFit {
            required: 80.0,
            available: 50.0,
        })
    );
}

#[test]
fn a_viewport_alone_takes_the_offered_height_and_an_ordinary_leaf_does_not() {
    let renderer = renderer();
    let plan = viewport()
        .query_at(Level::Preferred, &renderer, Size::new(80.0, 30.0))
        .unwrap();
    assert_eq!(plan.size, Size::new(80.0, 30.0));
    assert_eq!(plan.minimum_height(), 10.0);
    assert_eq!(
        ordinary().query_at(Level::Preferred, &renderer, Size::new(80.0, 30.0)),
        Err(AllocationError::NoFit {
            required: 60.0,
            available: 30.0,
        })
    );
}

// The committed widget tree agrees with the plan: the viewport is laid out at
// the height it was offered, not at the height its content would take.
#[test]
fn a_committed_viewport_is_laid_out_at_its_offered_height() {
    let renderer = renderer();
    let mut root = stages(vec![Stage::new(
        Level::Preferred,
        column_child(vec![header(), viewport()], 0.0),
    )]);
    let mut tree = Tree::empty();
    let node = root.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(80.0, 50.0)),
    );
    assert!(root.selection().unwrap().is_ok());
    assert_eq!(node.size(), Size::new(80.0, 50.0));
}

fn replacement(_: Level, _: &Renderer) -> MeasuredSize {
    MeasuredSize {
        preferred: Size::new(80.0, 40.0),
        minimum: Size::new(80.0, 25.0),
    }
}

// A replaced measurement discards the viewport's floor from the measurement
// (the query returns the replacement), but the leaf still plans as the
// viewport it was declared: it is offered height, and the floor its plan
// records, which the column shares height by, is the declared 10, not the
// replacement's minimum of 25.
#[test]
fn a_viewport_with_a_replaced_measurement_still_plans_as_a_viewport() {
    let renderer = renderer();
    let replaced = viewport().with_measurement(replacement);
    assert_eq!(
        replaced.query(Level::Preferred, &renderer),
        replacement(Level::Preferred, &renderer)
    );
    let plan = replaced
        .query_at(Level::Preferred, &renderer, Size::new(80.0, 30.0))
        .unwrap();
    assert_eq!(plan.size, Size::new(80.0, 30.0));
    assert_eq!(plan.minimum_height(), 10.0);

    let column = column_child(vec![header(), replaced], 0.0);
    assert_eq!(
        heights(&column, Size::new(80.0, 1000.0)),
        Ok(vec![20.0, 40.0])
    );
    assert_eq!(
        heights(&column, Size::new(80.0, 40.0)),
        Ok(vec![20.0, 20.0])
    );
    assert_eq!(
        heights(&column, Size::new(80.0, 29.0)),
        Err(AllocationError::NoFit {
            required: 30.0,
            available: 29.0,
        })
    );
}

// The leaf measures 5 × 8.
fn leaf() -> Child<'static, ()> {
    child(|_| widget::Space::new().width(5).height(8).into())
}

/// The planned size and minimum height, or the error.
fn planned(child: &Child<'_, ()>, room: Size) -> Result<(Size, f32), AllocationError> {
    child
        .query_at(Level::Preferred, &renderer(), room)
        .map(|plan| (plan.size, plan.minimum_height()))
}

#[test]
fn a_fixed_width_only() {
    let fixed = leaf().fixed_dimensions(Some(10.0), None);
    assert_eq!(
        planned(&fixed, Size::new(100.0, 100.0)),
        Ok((Size::new(10.0, 8.0), 8.0))
    );
    assert_eq!(
        planned(&fixed, Size::new(9.0, 100.0)),
        Err(AllocationError::NoFit {
            required: 10.0,
            available: 9.0,
        })
    );
}

#[test]
fn a_fixed_height_only() {
    let fixed = leaf().fixed_dimensions(None, Some(20.0));
    assert_eq!(
        planned(&fixed, Size::new(100.0, 100.0)),
        Ok((Size::new(5.0, 20.0), 20.0))
    );
    assert_eq!(
        planned(&fixed, Size::new(100.0, 15.0)),
        Err(AllocationError::NoFit {
            required: 20.0,
            available: 15.0,
        })
    );
}

#[test]
fn a_fixed_width_and_height() {
    let fixed = leaf().fixed_dimensions(Some(10.0), Some(20.0));
    assert_eq!(
        planned(&fixed, Size::new(100.0, 100.0)),
        Ok((Size::new(10.0, 20.0), 20.0))
    );
    assert_eq!(
        planned(&fixed, Size::new(9.0, 100.0)),
        Err(AllocationError::NoFit {
            required: 10.0,
            available: 9.0,
        })
    );
    assert_eq!(
        planned(&fixed, Size::new(100.0, 15.0)),
        Err(AllocationError::NoFit {
            required: 20.0,
            available: 15.0,
        })
    );
}

#[test]
fn a_fixed_dimension_below_the_measured_minimum_is_raised_to_it() {
    let fixed = leaf().fixed_dimensions(Some(3.0), Some(4.0));
    assert_eq!(
        fixed.query(Level::Preferred, &renderer()),
        MeasuredSize {
            preferred: Size::new(5.0, 8.0),
            minimum: Size::new(5.0, 8.0),
        }
    );
    assert_eq!(
        planned(&fixed, Size::new(100.0, 100.0)),
        Ok((Size::new(5.0, 8.0), 8.0))
    );
    assert_eq!(
        planned(&fixed, Size::new(4.0, 100.0)),
        Err(AllocationError::NoFit {
            required: 5.0,
            available: 4.0,
        })
    );
    assert_eq!(
        planned(&fixed, Size::new(100.0, 7.0)),
        Err(AllocationError::NoFit {
            required: 8.0,
            available: 7.0,
        })
    );
}

// The size the planner fixes is the fixed child's whole measured minimum, with
// every adjustment declared on it, not the number given to `fixed_dimensions`.
// Forms whose one form is fixed at 10 wide, given a minimum width of 4, are
// allocated 6 in a room 6 wide; the fixed form then needs its 10 and fails
// with the amounts of its own check. A minimum width of 4 declared on the
// fixed child itself makes 4 the width it fixes, and the leaf inside, 5 wide,
// does not fit in that.
#[test]
fn a_fixed_width_wider_than_its_allocation_fails_with_the_amounts() {
    let narrowed = forms(vec![(
        Level::Preferred,
        leaf().fixed_dimensions(Some(10.0), None),
    )])
    .minimum_width(|_, _| 4.0);
    assert_eq!(
        planned(&narrowed, Size::new(6.0, 100.0)),
        Err(AllocationError::NoFit {
            required: 10.0,
            available: 6.0,
        })
    );
    let floored = leaf()
        .fixed_dimensions(Some(10.0), None)
        .minimum_width(|_, _| 4.0);
    assert_eq!(
        planned(&floored, Size::new(6.0, 100.0)),
        Err(AllocationError::NoFit {
            required: 5.0,
            available: 4.0,
        })
    );
}
