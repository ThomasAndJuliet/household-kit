// SPDX-License-Identifier: GPL-3.0-only

//! What a row, a column and a wrap plan, to the pixel: every placement's
//! declared index, position and size, the rows, the plan's size and floor
//! of height, and the error when it does not fit.
//!
//! The parts are native boxes of a fixed height whose width fills what they
//! are given, declared with a preferred and a minimum width, so every number
//! below follows from the engine's arithmetic alone: a weight of zero keeps
//! the preferred width; a shortage is shared in proportion to
//! `weight × (preferred − minimum)`; growth shares a surplus; spacing
//! separates visible children only. The viewport is the one in
//! `layout_height_tests.rs`: 80 wide, 60 high, with a floor of 10.
//!
//! Plans are made by the planner itself (`plan_width`) at the room given, so
//! that the root's own allocation (`query_at`) does not answer first.

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

/// A box `height` high that fills the width it is given, preferring
/// `preferred` and usable down to `minimum`.
fn part(preferred: f32, minimum: f32, height: f32) -> Child<'static, ()> {
    child(move |_| {
        widget::Space::new()
            .width(Length::Fill)
            .height(height)
            .into()
    })
    .minimum_width(move |_, _| minimum)
    .preferred_width(preferred)
}

/// A part that would not fit anywhere here, hidden at the level planned.
fn hidden() -> Child<'static, ()> {
    part(500.0, 500.0, 500.0).hidden_when(|level| level == Level::Preferred)
}

fn viewport() -> Child<'static, ()> {
    scroll_child(
        |_| cosmic::iced::widget::scrollable(widget::Space::new().width(80).height(60)).into(),
        Size::new(80.0, 10.0),
    )
}

fn plan(child: &Child<'_, ()>, width: f32, height: f32) -> Result<GeometryPlan, AllocationError> {
    child.plan_width(Level::Preferred, &renderer(), width, height)
}

/// Each placement as (declared index, [x, y], [width, height]).
fn placed(plan: &GeometryPlan) -> Vec<(usize, [f32; 2], [f32; 2])> {
    plan.children
        .iter()
        .map(|placed| {
            (
                placed.index,
                [placed.position.x, placed.position.y],
                [placed.geometry.size.width, placed.geometry.size.height],
            )
        })
        .collect()
}

fn no_fit(required: f32, available: f32) -> AllocationError {
    AllocationError::NoFit {
        required,
        available,
    }
}

/// Weights 0, 1 and 3, and a hidden child between the first two.
fn weighted() -> Vec<Child<'static, ()>> {
    vec![
        part(100.0, 40.0, 20.0).weight(0.0),
        hidden(),
        part(100.0, 40.0, 40.0),
        part(100.0, 40.0, 30.0).weight(3.0),
    ]
}

// A row's minimum width is every floor and every gap; a wrap's is its widest
// floor, with no gap, since at that width each child has a row of its own; a
// column's width is its widest, its height every height and gap.
#[test]
fn each_arrangement_measures_its_visible_children() {
    let renderer = renderer();
    let measure = |child: Child<'static, ()>| child.query(Level::Preferred, &renderer);
    assert_eq!(
        measure(row_child(weighted(), 10.0)),
        MeasuredSize {
            preferred: Size::new(320.0, 40.0),
            minimum: Size::new(200.0, 40.0),
        }
    );
    assert_eq!(
        measure(wrap_child(weighted(), 10.0)),
        MeasuredSize {
            preferred: Size::new(320.0, 40.0),
            minimum: Size::new(100.0, 40.0),
        }
    );
    assert_eq!(
        measure(column_child(weighted(), 10.0)),
        MeasuredSize {
            preferred: Size::new(100.0, 110.0),
            minimum: Size::new(100.0, 110.0),
        }
    );
    // A frame's minimum width adds its padding to its content's protected
    // width when the content has a weight of zero.
    assert_eq!(
        measure(framed_child(
            part(100.0, 40.0, 20.0).weight(0.0),
            5.0,
            |e| e
        )),
        MeasuredSize {
            preferred: Size::new(110.0, 30.0),
            minimum: Size::new(110.0, 30.0),
        }
    );
    assert_eq!(
        measure(framed_child(part(100.0, 40.0, 20.0), 5.0, |e| e)),
        MeasuredSize {
            preferred: Size::new(110.0, 30.0),
            minimum: Size::new(50.0, 30.0),
        }
    );
}

// 260 wide less two gaps of 10 leaves 240 for 300 preferred: the weight-0
// part keeps 100, and the shortage of 60 goes 15 to weight 1 and 45 to
// weight 3. The row is 40 high, its tallest part.
#[test]
fn a_row_shares_a_shortage_by_weight_and_centres_or_top_aligns() {
    let centred = plan(&row_child(weighted(), 10.0), 260.0, 100.0).unwrap();
    assert_eq!(
        placed(&centred),
        vec![
            (0, [0.0, 10.0], [100.0, 20.0]),
            (2, [110.0, 0.0], [85.0, 40.0]),
            (3, [205.0, 5.0], [55.0, 30.0]),
        ]
    );
    assert_eq!(centred.rows, vec![vec![0, 1, 2]]);
    assert_eq!(centred.size, Size::new(260.0, 40.0));
    assert_eq!(centred.minimum_height(), 40.0);

    let top = plan(&row_child_top(weighted(), 10.0), 260.0, 100.0).unwrap();
    assert_eq!(
        placed(&top),
        vec![
            (0, [0.0, 0.0], [100.0, 20.0]),
            (2, [110.0, 0.0], [85.0, 40.0]),
            (3, [205.0, 0.0], [55.0, 30.0]),
        ]
    );
    assert_eq!(top.rows, vec![vec![0, 1, 2]]);
    assert_eq!(top.size, Size::new(260.0, 40.0));
    assert_eq!(top.minimum_height(), 40.0);
}

// 400 less two gaps leaves a surplus of 80 over 300, shared 1 : 3.
#[test]
fn a_row_shares_a_surplus_by_growth() {
    let row = row_child(
        vec![
            part(100.0, 100.0, 20.0),
            part(100.0, 100.0, 20.0).grow(1.0),
            part(100.0, 100.0, 20.0).grow(3.0),
        ],
        10.0,
    );
    let plan = plan(&row, 400.0, 100.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 0.0], [100.0, 20.0]),
            (1, [110.0, 0.0], [120.0, 20.0]),
            (2, [240.0, 0.0], [160.0, 20.0]),
        ]
    );
    assert_eq!(plan.rows, vec![vec![0, 1, 2]]);
    assert_eq!(plan.size, Size::new(400.0, 20.0));
    assert_eq!(plan.minimum_height(), 20.0);
}

// A row offers its children its own height: the viewport takes 30 of its 60
// at once. The row's floor of height is its highest floor, the part's 20.
#[test]
fn a_row_offers_its_height_to_a_viewport() {
    let row = row_child(vec![part(80.0, 80.0, 20.0), viewport()], 10.0);
    let plan = plan(&row, 170.0, 30.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 5.0], [80.0, 20.0]),
            (1, [90.0, 0.0], [80.0, 30.0])
        ]
    );
    assert_eq!(plan.rows, vec![vec![0, 1]]);
    assert_eq!(plan.size, Size::new(170.0, 30.0));
    assert_eq!(plan.minimum_height(), 20.0);
}

#[test]
fn a_row_fails_with_the_amounts_needed_and_available() {
    // Two gaps of 30 in 50.
    let gaps = row_child(
        vec![
            part(10.0, 0.0, 10.0),
            part(10.0, 0.0, 10.0),
            part(10.0, 0.0, 10.0),
        ],
        30.0,
    );
    assert_eq!(plan(&gaps, 50.0, 100.0), Err(no_fit(60.0, 50.0)));
    // A weight of zero keeps the preferred 100, which is wider than 80; at
    // weight 1 the same part fits, at 80.
    let protected = row_child(vec![part(100.0, 40.0, 20.0).weight(0.0)], 0.0);
    assert_eq!(plan(&protected, 80.0, 100.0), Err(no_fit(100.0, 80.0)));
    let shrinking = plan(&row_child(vec![part(100.0, 40.0, 20.0)], 0.0), 80.0, 100.0).unwrap();
    assert_eq!(placed(&shrinking), vec![(0, [0.0, 0.0], [80.0, 20.0])]);
    // Each floor fits alone; together with the gap they need 120 of the 90
    // left beside it.
    let floors = row_child(vec![part(60.0, 60.0, 20.0), part(60.0, 60.0, 20.0)], 10.0);
    assert_eq!(plan(&floors, 100.0, 100.0), Err(no_fit(120.0, 90.0)));
    // An ordinary part 40 high in a row 30 high.
    let tall = row_child(vec![part(50.0, 50.0, 40.0)], 0.0);
    assert_eq!(plan(&tall, 50.0, 30.0), Err(no_fit(40.0, 30.0)));
}

// Each child is allocated the column's width alone: the first shrinks from
// 150 to 100, the weight-0 one keeps 60 and grows to 100, the last keeps 50.
#[test]
fn a_column_allocates_each_width_alone_and_stacks_heights() {
    let column = column_child(
        vec![
            part(150.0, 40.0, 20.0),
            hidden(),
            part(60.0, 60.0, 30.0).weight(0.0).grow(1.0),
            part(50.0, 20.0, 10.0).weight(3.0),
        ],
        5.0,
    );
    let plan = plan(&column, 100.0, 1000.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 0.0], [100.0, 20.0]),
            (2, [0.0, 25.0], [100.0, 30.0]),
            (3, [0.0, 60.0], [50.0, 10.0]),
        ]
    );
    assert!(plan.rows.is_empty());
    assert_eq!(plan.size, Size::new(100.0, 70.0));
    assert_eq!(plan.minimum_height(), 70.0);
}

// 60 high less a gap of 10 leaves 50 for 20 and 60: the viewport gives 30 of
// its 40 above its floor. The column's floor is 10 + 20 + 10.
#[test]
fn a_column_shares_a_shortage_of_height_with_a_viewport() {
    let column = column_child(vec![part(80.0, 80.0, 20.0), viewport()], 10.0);
    let plan = plan(&column, 80.0, 60.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 0.0], [80.0, 20.0]),
            (1, [0.0, 30.0], [80.0, 30.0])
        ]
    );
    assert!(plan.rows.is_empty());
    assert_eq!(plan.size, Size::new(80.0, 60.0));
    assert_eq!(plan.minimum_height(), 40.0);
}

// The column shares height with its children's weights: a weight of zero
// protects the viewport's preferred height too.
#[test]
fn a_weight_of_zero_protects_a_height_in_a_column() {
    let column = column_child(vec![part(80.0, 80.0, 20.0), viewport().weight(0.0)], 10.0);
    assert_eq!(plan(&column, 80.0, 60.0), Err(no_fit(80.0, 50.0)));
}

// 200 less a gap of 10 leaves a surplus of 110 over 20 and 60, all to the
// viewport that grows.
#[test]
fn a_column_shares_a_surplus_of_height_by_growth() {
    let column = column_child(
        vec![part(80.0, 80.0, 20.0), viewport().grow_height(1.0)],
        10.0,
    );
    let plan = plan(&column, 80.0, 200.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 0.0], [80.0, 20.0]),
            (1, [0.0, 30.0], [80.0, 170.0])
        ]
    );
    assert_eq!(plan.size, Size::new(80.0, 200.0));
    assert_eq!(plan.minimum_height(), 40.0);
}

#[test]
fn a_column_fails_with_the_amounts_needed_and_available() {
    let gaps = column_child(
        vec![
            part(10.0, 10.0, 10.0),
            part(10.0, 10.0, 10.0),
            part(10.0, 10.0, 10.0),
        ],
        30.0,
    );
    assert_eq!(plan(&gaps, 50.0, 50.0), Err(no_fit(60.0, 50.0)));
    let protected = column_child(vec![part(100.0, 40.0, 20.0).weight(0.0)], 0.0);
    assert_eq!(plan(&protected, 80.0, 100.0), Err(no_fit(100.0, 80.0)));
    let tall = column_child(vec![part(50.0, 50.0, 20.0), part(50.0, 50.0, 30.0)], 10.0);
    assert_eq!(plan(&tall, 50.0, 50.0), Err(no_fit(50.0, 40.0)));
}

// Packed by floors in 120 with gaps of 10: 50 + 10 + 40 fit, 40 more does
// not; 40 + 10 + 60 fit, 30 more does not; then the last alone. Each row is
// then allocated afresh: the first shares a shortage of 10, all to weight 1;
// the second 20, all to weight 3 (the other is at its floor); the last grows
// to the whole width. The hidden child takes no part.
#[test]
fn a_wrap_packs_by_floors_onto_three_rows() {
    let wrap = wrap_child(
        vec![
            part(50.0, 50.0, 20.0).weight(0.0),
            part(70.0, 40.0, 30.0),
            hidden(),
            part(70.0, 40.0, 20.0).weight(3.0),
            part(60.0, 60.0, 20.0),
            part(30.0, 30.0, 10.0).grow(1.0),
        ],
        10.0,
    );
    let plan = plan(&wrap, 120.0, 1000.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 5.0], [50.0, 20.0]),
            (1, [60.0, 0.0], [60.0, 30.0]),
            (3, [0.0, 40.0], [50.0, 20.0]),
            (4, [60.0, 40.0], [60.0, 20.0]),
            (5, [0.0, 70.0], [120.0, 10.0]),
        ]
    );
    assert_eq!(plan.rows, vec![vec![0, 1], vec![2, 3], vec![4]]);
    assert_eq!(plan.size, Size::new(120.0, 80.0));
    assert_eq!(plan.minimum_height(), 80.0);
}

// Two rows in 80: the part, then the viewport. With room, the viewport keeps
// 60; in 60 the rows share the shortage, and the viewport's row gives 30.
// The floor of height is the one the rows were planned with: 20 + 10 + 10.
#[test]
fn a_wrap_shares_a_shortage_of_height_between_its_rows() {
    let wrap = wrap_child(vec![part(80.0, 80.0, 20.0), viewport()], 10.0);
    let roomy = plan(&wrap, 80.0, 1000.0).unwrap();
    assert_eq!(
        placed(&roomy),
        vec![
            (0, [0.0, 0.0], [80.0, 20.0]),
            (1, [0.0, 30.0], [80.0, 60.0])
        ]
    );
    assert_eq!(roomy.rows, vec![vec![0], vec![1]]);
    assert_eq!(roomy.size, Size::new(80.0, 90.0));
    assert_eq!(roomy.minimum_height(), 40.0);

    let short = plan(&wrap, 80.0, 60.0).unwrap();
    assert_eq!(
        placed(&short),
        vec![
            (0, [0.0, 0.0], [80.0, 20.0]),
            (1, [0.0, 30.0], [80.0, 30.0])
        ]
    );
    assert_eq!(short.rows, vec![vec![0], vec![1]]);
    assert_eq!(short.size, Size::new(80.0, 60.0));
    assert_eq!(short.minimum_height(), 40.0);
}

// A wrap plans its children with unbounded height and shares a shortage
// afterwards; in one row, the result is the row's.
#[test]
fn a_wrap_in_one_row_shares_its_height_after_planning() {
    let wrap = wrap_child(vec![part(80.0, 80.0, 20.0), viewport()], 10.0);
    let plan = plan(&wrap, 170.0, 30.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 5.0], [80.0, 20.0]),
            (1, [90.0, 0.0], [80.0, 30.0])
        ]
    );
    assert_eq!(plan.rows, vec![vec![0, 1]]);
    assert_eq!(plan.size, Size::new(170.0, 30.0));
    assert_eq!(plan.minimum_height(), 20.0);
}

#[test]
fn a_wrap_fails_with_the_amounts_needed_and_available() {
    // Three rows of no height, with two gaps of 30 between them, in 50.
    let gaps = wrap_child(
        vec![
            part(80.0, 80.0, 0.0),
            part(80.0, 80.0, 0.0),
            part(80.0, 80.0, 0.0),
        ],
        30.0,
    );
    assert_eq!(plan(&gaps, 80.0, 50.0), Err(no_fit(60.0, 50.0)));
    let protected = wrap_child(
        vec![part(50.0, 50.0, 10.0), part(100.0, 40.0, 20.0).weight(0.0)],
        0.0,
    );
    assert_eq!(plan(&protected, 80.0, 100.0), Err(no_fit(100.0, 80.0)));
    // Two rows, 20 and 30 high, in 40 less a gap of 10.
    let tall = wrap_child(vec![part(80.0, 80.0, 20.0), part(80.0, 80.0, 30.0)], 10.0);
    assert_eq!(plan(&tall, 80.0, 40.0), Err(no_fit(50.0, 30.0)));
}
