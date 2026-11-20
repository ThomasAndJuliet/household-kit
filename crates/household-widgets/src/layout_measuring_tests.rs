// SPDX-License-Identifier: GPL-3.0-only

//! The order in which a child's measuring adjustments are declared.
//!
//! Each builder that changes how a child is measured (`round_width_up`, a
//! preferred width, a minimum width, `fixed_dimensions`, a replaced
//! measurement) applies to the measurement as it stands when the builder is
//! called. For every pair whose order changes the result, these tests build
//! the same child in both orders and check each against the arithmetic. The
//! leaf is 5 wide and 8 high, so its unadjusted measurement prefers and needs
//! 5 × 8; only widths change below.

use super::*;
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::{Font, Pixels};
use std::cell::Cell;

fn renderer() -> Renderer {
    cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
        Font::default(),
        Pixels(14.0),
        Some("tiny-skia"),
    ))
    .expect("headless tiny-skia renderer")
}

fn leaf() -> Child<'static, ()> {
    child(|_| widget::Space::new().width(5).height(8).into())
}

/// (preferred width, minimum width) at the preferred level.
fn widths(child: &Child<'_, ()>, renderer: &Renderer) -> (f32, f32) {
    let measured = child.query(Level::Preferred, renderer);
    (measured.preferred.width, measured.minimum.width)
}

#[test]
fn the_leaf_measures_five_by_eight() {
    let renderer = renderer();
    assert_eq!(
        leaf().query(Level::Preferred, &renderer),
        MeasuredSize {
            preferred: Size::new(5.0, 8.0),
            minimum: Size::new(5.0, 8.0),
        }
    );
}

// Rounding up after a preferred width of 10.4 rounds that width to 11;
// rounding first leaves 5 to round, and the preferred width set after it
// stays 10.4.
#[test]
fn rounding_up_and_a_preferred_width() {
    let renderer = renderer();
    assert_eq!(
        widths(&leaf().preferred_width(10.4).round_width_up(), &renderer),
        (11.0, 5.0)
    );
    assert_eq!(
        widths(&leaf().round_width_up().preferred_width(10.4), &renderer),
        (10.4, 5.0)
    );
}

// A minimum width of 7.5 raises both widths to 7.5; rounding after it gives
// 8 and 8, rounding before it leaves 7.5 and 7.5.
#[test]
fn rounding_up_and_a_minimum_width() {
    let renderer = renderer();
    assert_eq!(
        widths(
            &leaf().minimum_width(|_, _| 7.5).round_width_up(),
            &renderer
        ),
        (8.0, 8.0)
    );
    assert_eq!(
        widths(
            &leaf().round_width_up().minimum_width(|_, _| 7.5),
            &renderer
        ),
        (7.5, 7.5)
    );
}

// A preferred width never goes below the minimum it finds. Asked for 3 while
// the minimum is still the leaf's 5, it gives 5, and a later minimum of 4
// keeps that preferred 5. With the minimum lowered to 4 first, the preferred
// width of 3 is raised only to 4.
#[test]
fn a_preferred_width_and_a_minimum_width() {
    let renderer = renderer();
    assert_eq!(
        widths(
            &leaf().preferred_width(3.0).minimum_width(|_, _| 4.0),
            &renderer
        ),
        (5.0, 4.0)
    );
    assert_eq!(
        widths(
            &leaf().minimum_width(|_, _| 4.0).preferred_width(3.0),
            &renderer
        ),
        (4.0, 4.0)
    );
}

// Of two preferred widths, the later one is the one that counts.
#[test]
fn two_preferred_widths() {
    let renderer = renderer();
    assert_eq!(
        widths(
            &leaf().preferred_width(8.0).preferred_width(12.0),
            &renderer
        ),
        (12.0, 5.0)
    );
    assert_eq!(
        widths(
            &leaf().preferred_width(12.0).preferred_width(8.0),
            &renderer
        ),
        (8.0, 5.0)
    );
}

// Of two minimum widths, the later one is the minimum, and the preferred
// width is the largest of the leaf's and both floors (as when a menu row
// declares its preferred text width and then its shortest form).
#[test]
fn two_minimum_widths() {
    let renderer = renderer();
    assert_eq!(
        widths(
            &leaf().minimum_width(|_, _| 7.0).minimum_width(|_, _| 6.0),
            &renderer
        ),
        (7.0, 6.0)
    );
    assert_eq!(
        widths(
            &leaf().minimum_width(|_, _| 6.0).minimum_width(|_, _| 7.0),
            &renderer
        ),
        (7.0, 7.0)
    );
}

// Fixed dimensions put the child in a column and fix the column's width at
// the requested width or the content's minimum, whichever is larger. An
// adjustment declared before them applies to the content inside; one declared
// after applies to the fixed column.
#[test]
fn fixed_dimensions_and_rounding_up() {
    let renderer = renderer();
    assert_eq!(
        widths(
            &leaf().round_width_up().fixed_dimensions(Some(10.5), None),
            &renderer
        ),
        (10.5, 10.5)
    );
    assert_eq!(
        widths(
            &leaf().fixed_dimensions(Some(10.5), None).round_width_up(),
            &renderer
        ),
        (11.0, 11.0)
    );
}

#[test]
fn fixed_dimensions_and_a_preferred_width() {
    let renderer = renderer();
    assert_eq!(
        widths(
            &leaf()
                .preferred_width(20.0)
                .fixed_dimensions(Some(10.0), None),
            &renderer
        ),
        (10.0, 10.0)
    );
    assert_eq!(
        widths(
            &leaf()
                .fixed_dimensions(Some(10.0), None)
                .preferred_width(20.0),
            &renderer
        ),
        (20.0, 10.0)
    );
}

#[test]
fn fixed_dimensions_and_a_minimum_width() {
    let renderer = renderer();
    assert_eq!(
        widths(
            &leaf()
                .minimum_width(|_, _| 4.0)
                .fixed_dimensions(Some(10.0), None),
            &renderer
        ),
        (10.0, 10.0)
    );
    assert_eq!(
        widths(
            &leaf()
                .fixed_dimensions(Some(10.0), None)
                .minimum_width(|_, _| 4.0),
            &renderer
        ),
        (10.0, 4.0)
    );
}

fn replacement(_: Level, _: &Renderer) -> MeasuredSize {
    MeasuredSize {
        preferred: Size::new(7.3, 4.0),
        minimum: Size::new(6.2, 3.0),
    }
}

// A replaced measurement is the whole measurement at that point: what was
// declared before it is gone, the leaf's own view is never measured, and what
// is declared after it applies to the replacement.
#[test]
fn a_replaced_measurement_discards_what_came_before_and_keeps_what_follows() {
    let renderer = renderer();
    assert_eq!(
        leaf()
            .preferred_width(20.0)
            .round_width_up()
            .with_measurement(replacement)
            .query(Level::Preferred, &renderer),
        replacement(Level::Preferred, &renderer)
    );
    assert_eq!(
        leaf()
            .preferred_width(20.0)
            .with_measurement(replacement)
            .round_width_up()
            .query(Level::Preferred, &renderer),
        MeasuredSize {
            preferred: Size::new(8.0, 4.0),
            minimum: Size::new(7.0, 3.0),
        }
    );
    assert_eq!(
        widths(
            &leaf().with_measurement(replacement).preferred_width(20.0),
            &renderer
        ),
        (20.0, 6.2)
    );

    let built = Rc::new(Cell::new(0));
    let counted = built.clone();
    let replaced: Child<'_, ()> = child(move |_| {
        counted.set(counted.get() + 1);
        widget::Space::new().width(5).height(8).into()
    })
    .round_width_up()
    .with_measurement(replacement)
    .round_width_up();
    replaced.query(Level::Preferred, &renderer);
    assert_eq!(built.get(), 0, "a replaced measurement is never taken");
}
