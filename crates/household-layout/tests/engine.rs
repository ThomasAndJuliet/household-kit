// SPDX-License-Identifier: GPL-3.0-only

//! The engine on its own, with a host of plain boxes and no toolkit.
//!
//! The first group is household-widgets' `layout_arrangement_tests.rs`, case
//! for case and number for number: what a row, a column and a wrap plan, to
//! the pixel. There the parts are native boxes; here they are this host's
//! boxes, which take the same sizes, so the same arithmetic must give the
//! same answers. The second group adds one case each for a frame, forms, a
//! shortage of height shared between leaves that take a range of heights,
//! fixed dimensions and an invalid part.

use household_layout::{AllocationError, Child, Host, Level, Measured, Offer, Padding, Plan, Size};

/// A leaf of the test host: its size as a function of what it is offered.
#[derive(Clone, Copy)]
enum Shape {
    /// `height` high, as wide as the room; no width when the room is
    /// unbounded, as a native box that fills its width has.
    Fill { height: f32 },
    /// Always `width` × `height`.
    Fixed { width: f32, height: f32 },
    /// A scrolling area `width` wide whose content is `height` high: it
    /// needs `height`, and laid out in a height it takes at most that.
    Scrolling { width: f32, height: f32 },
}

/// A host whose leaves are shapes, with nothing to measure them with and
/// nothing kept on the parts or the frames.
struct Boxes;

impl Host for Boxes {
    type Leaf = Shape;
    type Context = ();
    type Note = ();
    type Chrome = ();

    fn size(leaf: &Shape, _: Level, _: &(), offer: Offer) -> Size {
        let (room, offered) = match offer {
            Offer::NeededHeight(room) => (room, false),
            Offer::OfferedHeight(room) => (room, true),
        };
        match *leaf {
            Shape::Fill { height } => {
                let width = if room.width.is_finite() {
                    room.width
                } else {
                    0.0
                };
                Size::new(width, height)
            }
            Shape::Fixed { width, height } => Size::new(width, height),
            Shape::Scrolling { width, height } => {
                if offered {
                    Size::new(width, height.min(room.height))
                } else {
                    Size::new(width, height)
                }
            }
        }
    }
}

type Part = Child<'static, Boxes>;

/// A box `height` high that fills the width it is given, preferring
/// `preferred` and usable down to `minimum`.
fn part(preferred: f32, minimum: f32, height: f32) -> Part {
    Child::leaf(Shape::Fill { height })
        .minimum_width(move |_, _| minimum)
        .preferred_width(preferred)
}

/// A part that would not fit anywhere here, hidden at the level planned.
fn hidden() -> Part {
    part(500.0, 500.0, 500.0).hidden_when(|level| level == Level::Preferred)
}

/// The viewport of household-widgets' tests: 80 wide, 60 high, with a floor
/// of 10.
fn viewport() -> Part {
    Child::ranged_leaf(
        Shape::Scrolling {
            width: 80.0,
            height: 60.0,
        },
        |_, _| Size::new(80.0, 10.0),
    )
}

/// Planned by the planner itself at the room given, so that the root's own
/// allocation (`query_at`) does not answer first.
fn plan(child: &Part, width: f32, height: f32) -> Result<Plan, AllocationError> {
    child.plan_width(Level::Preferred, &(), width, height)
}

/// Each placement as (declared index, [x, y], [width, height]).
fn placed(plan: &Plan) -> Vec<(usize, [f32; 2], [f32; 2])> {
    plan.children()
        .iter()
        .map(|placed| {
            (
                placed.index(),
                [placed.position().x, placed.position().y],
                [placed.plan().size().width, placed.plan().size().height],
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

fn all_round(padding: f32) -> Padding {
    Padding {
        top: padding,
        right: padding,
        bottom: padding,
        left: padding,
    }
}

/// Weights 0, 1 and 3, and a hidden child between the first two.
fn weighted() -> Vec<Part> {
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
    let measure = |child: Part| child.query(Level::Preferred, &());
    assert_eq!(
        measure(Child::row(weighted(), 10.0)),
        Measured {
            preferred: Size::new(320.0, 40.0),
            minimum: Size::new(200.0, 40.0),
        }
    );
    assert_eq!(
        measure(Child::wrap(weighted(), 10.0)),
        Measured {
            preferred: Size::new(320.0, 40.0),
            minimum: Size::new(100.0, 40.0),
        }
    );
    assert_eq!(
        measure(Child::column(weighted(), 10.0)),
        Measured {
            preferred: Size::new(100.0, 110.0),
            minimum: Size::new(100.0, 110.0),
        }
    );
    // A frame's minimum width adds its padding to its content's protected
    // width when the content has a weight of zero.
    assert_eq!(
        measure(Child::frame(
            part(100.0, 40.0, 20.0).weight(0.0),
            all_round(5.0),
            ()
        )),
        Measured {
            preferred: Size::new(110.0, 30.0),
            minimum: Size::new(110.0, 30.0),
        }
    );
    assert_eq!(
        measure(Child::frame(part(100.0, 40.0, 20.0), all_round(5.0), ())),
        Measured {
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
    let centred = plan(&Child::row(weighted(), 10.0), 260.0, 100.0).unwrap();
    assert_eq!(
        placed(&centred),
        vec![
            (0, [0.0, 10.0], [100.0, 20.0]),
            (2, [110.0, 0.0], [85.0, 40.0]),
            (3, [205.0, 5.0], [55.0, 30.0]),
        ]
    );
    assert_eq!(centred.rows(), [vec![0, 1, 2]]);
    assert_eq!(centred.size(), Size::new(260.0, 40.0));
    assert_eq!(centred.minimum_height(), 40.0);

    let top = plan(&Child::row_top(weighted(), 10.0), 260.0, 100.0).unwrap();
    assert_eq!(
        placed(&top),
        vec![
            (0, [0.0, 0.0], [100.0, 20.0]),
            (2, [110.0, 0.0], [85.0, 40.0]),
            (3, [205.0, 0.0], [55.0, 30.0]),
        ]
    );
    assert_eq!(top.rows(), [vec![0, 1, 2]]);
    assert_eq!(top.size(), Size::new(260.0, 40.0));
    assert_eq!(top.minimum_height(), 40.0);
}

// 400 less two gaps leaves a surplus of 80 over 300, shared 1 : 3.
#[test]
fn a_row_shares_a_surplus_by_growth() {
    let row = Child::row(
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
    assert_eq!(plan.rows(), [vec![0, 1, 2]]);
    assert_eq!(plan.size(), Size::new(400.0, 20.0));
    assert_eq!(plan.minimum_height(), 20.0);
}

// A row offers its children its own height: the viewport takes 30 of its 60
// at once. The row's floor of height is its highest floor, the part's 20.
#[test]
fn a_row_offers_its_height_to_a_viewport() {
    let row = Child::row(vec![part(80.0, 80.0, 20.0), viewport()], 10.0);
    let plan = plan(&row, 170.0, 30.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 5.0], [80.0, 20.0]),
            (1, [90.0, 0.0], [80.0, 30.0])
        ]
    );
    assert_eq!(plan.rows(), [vec![0, 1]]);
    assert_eq!(plan.size(), Size::new(170.0, 30.0));
    assert_eq!(plan.minimum_height(), 20.0);
}

#[test]
fn a_row_fails_with_the_amounts_needed_and_available() {
    // Two gaps of 30 in 50.
    let gaps = Child::row(
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
    let protected = Child::row(vec![part(100.0, 40.0, 20.0).weight(0.0)], 0.0);
    assert_eq!(plan(&protected, 80.0, 100.0), Err(no_fit(100.0, 80.0)));
    let shrinking = plan(&Child::row(vec![part(100.0, 40.0, 20.0)], 0.0), 80.0, 100.0).unwrap();
    assert_eq!(placed(&shrinking), vec![(0, [0.0, 0.0], [80.0, 20.0])]);
    // Each floor fits alone; together with the gap they need 120 of the 90
    // left beside it.
    let floors = Child::row(vec![part(60.0, 60.0, 20.0), part(60.0, 60.0, 20.0)], 10.0);
    assert_eq!(plan(&floors, 100.0, 100.0), Err(no_fit(120.0, 90.0)));
    // An ordinary part 40 high in a row 30 high.
    let tall = Child::row(vec![part(50.0, 50.0, 40.0)], 0.0);
    assert_eq!(plan(&tall, 50.0, 30.0), Err(no_fit(40.0, 30.0)));
}

// Each child is allocated the column's width alone: the first shrinks from
// 150 to 100, the weight-0 one keeps 60 and grows to 100, the last keeps 50.
#[test]
fn a_column_allocates_each_width_alone_and_stacks_heights() {
    let column = Child::column(
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
    assert!(plan.rows().is_empty());
    assert_eq!(plan.size(), Size::new(100.0, 70.0));
    assert_eq!(plan.minimum_height(), 70.0);
}

// 60 high less a gap of 10 leaves 50 for 20 and 60: the viewport gives 30 of
// its 40 above its floor. The column's floor is 10 + 20 + 10.
#[test]
fn a_column_shares_a_shortage_of_height_with_a_viewport() {
    let column = Child::column(vec![part(80.0, 80.0, 20.0), viewport()], 10.0);
    let plan = plan(&column, 80.0, 60.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 0.0], [80.0, 20.0]),
            (1, [0.0, 30.0], [80.0, 30.0])
        ]
    );
    assert!(plan.rows().is_empty());
    assert_eq!(plan.size(), Size::new(80.0, 60.0));
    assert_eq!(plan.minimum_height(), 40.0);
}

// The column shares height with its children's weights: a weight of zero
// protects the viewport's preferred height too.
#[test]
fn a_weight_of_zero_protects_a_height_in_a_column() {
    let column = Child::column(vec![part(80.0, 80.0, 20.0), viewport().weight(0.0)], 10.0);
    assert_eq!(plan(&column, 80.0, 60.0), Err(no_fit(80.0, 50.0)));
}

// 200 less a gap of 10 leaves a surplus of 110 over 20 and 60, all to the
// viewport that grows.
#[test]
fn a_column_shares_a_surplus_of_height_by_growth() {
    let column = Child::column(
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
    assert_eq!(plan.size(), Size::new(80.0, 200.0));
    assert_eq!(plan.minimum_height(), 40.0);
}

#[test]
fn a_column_fails_with_the_amounts_needed_and_available() {
    let gaps = Child::column(
        vec![
            part(10.0, 10.0, 10.0),
            part(10.0, 10.0, 10.0),
            part(10.0, 10.0, 10.0),
        ],
        30.0,
    );
    assert_eq!(plan(&gaps, 50.0, 50.0), Err(no_fit(60.0, 50.0)));
    let protected = Child::column(vec![part(100.0, 40.0, 20.0).weight(0.0)], 0.0);
    assert_eq!(plan(&protected, 80.0, 100.0), Err(no_fit(100.0, 80.0)));
    let tall = Child::column(vec![part(50.0, 50.0, 20.0), part(50.0, 50.0, 30.0)], 10.0);
    assert_eq!(plan(&tall, 50.0, 50.0), Err(no_fit(50.0, 40.0)));
}

// Packed by floors in 120 with gaps of 10: 50 + 10 + 40 fit, 40 more does
// not; 40 + 10 + 60 fit, 30 more does not; then the last alone. Each row is
// then allocated afresh: the first shares a shortage of 10, all to weight 1;
// the second 20, all to weight 3 (the other is at its floor); the last grows
// to the whole width. The hidden child takes no part.
#[test]
fn a_wrap_packs_by_floors_onto_three_rows() {
    let wrap = Child::wrap(
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
    assert_eq!(plan.rows(), [vec![0, 1], vec![2, 3], vec![4]]);
    assert_eq!(plan.size(), Size::new(120.0, 80.0));
    assert_eq!(plan.minimum_height(), 80.0);
}

// Two rows in 80: the part, then the viewport. With room, the viewport keeps
// 60; in 60 the rows share the shortage, and the viewport's row gives 30.
// The floor of height is the one the rows were planned with: 20 + 10 + 10.
#[test]
fn a_wrap_shares_a_shortage_of_height_between_its_rows() {
    let wrap = Child::wrap(vec![part(80.0, 80.0, 20.0), viewport()], 10.0);
    let roomy = plan(&wrap, 80.0, 1000.0).unwrap();
    assert_eq!(
        placed(&roomy),
        vec![
            (0, [0.0, 0.0], [80.0, 20.0]),
            (1, [0.0, 30.0], [80.0, 60.0])
        ]
    );
    assert_eq!(roomy.rows(), [vec![0], vec![1]]);
    assert_eq!(roomy.size(), Size::new(80.0, 90.0));
    assert_eq!(roomy.minimum_height(), 40.0);

    let short = plan(&wrap, 80.0, 60.0).unwrap();
    assert_eq!(
        placed(&short),
        vec![
            (0, [0.0, 0.0], [80.0, 20.0]),
            (1, [0.0, 30.0], [80.0, 30.0])
        ]
    );
    assert_eq!(short.rows(), [vec![0], vec![1]]);
    assert_eq!(short.size(), Size::new(80.0, 60.0));
    assert_eq!(short.minimum_height(), 40.0);
}

// A wrap plans its children with unbounded height and shares a shortage
// afterwards; in one row, the result is the row's.
#[test]
fn a_wrap_in_one_row_shares_its_height_after_planning() {
    let wrap = Child::wrap(vec![part(80.0, 80.0, 20.0), viewport()], 10.0);
    let plan = plan(&wrap, 170.0, 30.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 5.0], [80.0, 20.0]),
            (1, [90.0, 0.0], [80.0, 30.0])
        ]
    );
    assert_eq!(plan.rows(), [vec![0, 1]]);
    assert_eq!(plan.size(), Size::new(170.0, 30.0));
    assert_eq!(plan.minimum_height(), 20.0);
}

#[test]
fn a_wrap_fails_with_the_amounts_needed_and_available() {
    // Three rows of no height, with two gaps of 30 between them, in 50.
    let gaps = Child::wrap(
        vec![
            part(80.0, 80.0, 0.0),
            part(80.0, 80.0, 0.0),
            part(80.0, 80.0, 0.0),
        ],
        30.0,
    );
    assert_eq!(plan(&gaps, 80.0, 50.0), Err(no_fit(60.0, 50.0)));
    let protected = Child::wrap(
        vec![part(50.0, 50.0, 10.0), part(100.0, 40.0, 20.0).weight(0.0)],
        0.0,
    );
    assert_eq!(plan(&protected, 80.0, 100.0), Err(no_fit(100.0, 80.0)));
    // Two rows, 20 and 30 high, in 40 less a gap of 10.
    let tall = Child::wrap(vec![part(80.0, 80.0, 20.0), part(80.0, 80.0, 30.0)], 10.0);
    assert_eq!(plan(&tall, 80.0, 40.0), Err(no_fit(50.0, 30.0)));
}

// A frame of 5 at the left, 3 at the right, 2 at the top and 4 at the
// bottom: its content is planned in what the padding leaves (112 of 120),
// and placed at the padding's top left corner. Narrower than its padding
// across, it does not fit.
#[test]
fn a_frame_plans_its_content_inside_its_padding() {
    let padding = Padding {
        top: 2.0,
        right: 3.0,
        bottom: 4.0,
        left: 5.0,
    };
    let frame = Child::frame(part(100.0, 40.0, 20.0), padding, ());
    let plan = plan(&frame, 120.0, 100.0).unwrap();
    assert_eq!(placed(&plan), vec![(0, [5.0, 2.0], [112.0, 20.0])]);
    assert_eq!(plan.size(), Size::new(120.0, 26.0));
    assert_eq!(plan.minimum_height(), 26.0);
    assert_eq!(self::plan(&frame, 7.0, 100.0), Err(no_fit(8.0, 7.0)));
}

// Forms from Preferred and from Strong: the first stands for the family up
// to Strong, the later one from there. The family's plan is its form's: a
// leaf here, with nothing placed in it. Forms whose first threshold is not
// Preferred are invalid.
#[test]
fn forms_plan_the_form_selected_at_the_level() {
    let fixed = |width, height| -> Part { Child::leaf(Shape::Fixed { width, height }) };
    let family = Child::forms(
        (Level::Preferred, fixed(100.0, 10.0)),
        vec![(Level::Strong, fixed(40.0, 30.0))],
    );
    let room = Size::new(200.0, 100.0);
    for (level, size) in [
        (Level::Preferred, Size::new(100.0, 10.0)),
        (Level::Mild, Size::new(100.0, 10.0)),
        (Level::Strong, Size::new(40.0, 30.0)),
        (Level::AllPermitted, Size::new(40.0, 30.0)),
    ] {
        let plan = family.query_at(level, &(), room).unwrap();
        assert_eq!(plan.size(), size, "{level:?}");
        assert!(plan.children().is_empty());
        assert_eq!(plan.leaf_size(), Some(size));
    }
    let late = Child::forms((Level::Mild, fixed(100.0, 10.0)), Vec::new());
    assert_eq!(
        late.query_at(Level::Mild, &(), room),
        Err(AllocationError::InvalidInput)
    );
}

// Two viewports 60 high with floors of 10, in a column 70 high with a gap
// of 10: 60 for 120. Each gives up height at weight × (60 − 10), the same
// rate, so each gives 30 and is 30 high. The column's floor is both floors
// and the gap.
#[test]
fn leaves_that_take_a_range_of_heights_share_a_shortage_by_their_rates() {
    let column = Child::column(vec![viewport(), viewport()], 10.0);
    let plan = plan(&column, 80.0, 70.0).unwrap();
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 0.0], [80.0, 30.0]),
            (1, [0.0, 40.0], [80.0, 30.0])
        ]
    );
    assert_eq!(plan.size(), Size::new(80.0, 70.0));
    assert_eq!(plan.minimum_height(), 30.0);
    // Below the floors there is nothing left to give.
    assert_eq!(self::plan(&column, 80.0, 29.0), Err(no_fit(20.0, 19.0)));
}

// Fixed at 60 × 50 round a part that prefers 100 and works in 40: the
// wrapper is 60 wide and 50 high whatever the room, and its floor of height
// is the fixed 50; its content is planned at 60, its own 20 high. A room
// narrower than 60 does not fit.
#[test]
fn fixed_dimensions_hold_whatever_the_room() {
    let fixed = part(100.0, 40.0, 20.0).fixed_dimensions(Some(60.0), Some(50.0));
    assert_eq!(
        fixed.query(Level::Preferred, &()),
        Measured {
            preferred: Size::new(60.0, 50.0),
            minimum: Size::new(60.0, 50.0),
        }
    );
    let plan = plan(&fixed, 200.0, 100.0).unwrap();
    assert_eq!(placed(&plan), vec![(0, [0.0, 0.0], [60.0, 20.0])]);
    assert_eq!(plan.size(), Size::new(60.0, 50.0));
    assert_eq!(plan.minimum_height(), 50.0);
    assert!(fixed.sharing().fixed.width && fixed.sharing().fixed.height);
    assert_eq!(self::plan(&fixed, 50.0, 100.0), Err(no_fit(60.0, 50.0)));
}

// A weight that is not a number makes the part invalid: it measures NaN,
// keeps the number it was given, and fails every plan that includes it
// with invalid input. Hidden, it takes no part, and the rest lays out.
#[test]
fn an_invalid_part_fails_every_plan_that_includes_it() {
    let bad = part(40.0, 40.0, 20.0).weight(f32::NAN);
    assert!(bad.sharing().weight.is_nan());
    assert!(bad.query(Level::Preferred, &()).preferred.width.is_nan());
    let room = Size::new(200.0, 100.0);
    for child in [
        bad.clone(),
        Child::row(vec![part(40.0, 40.0, 20.0), bad.clone()], 4.0),
        Child::column(vec![part(40.0, 40.0, 20.0), bad.clone()], 4.0),
        Child::frame(bad.clone(), all_round(1.0), ()),
    ] {
        assert_eq!(
            child.query_at(Level::Preferred, &(), room),
            Err(AllocationError::InvalidInput)
        );
    }
    let hidden = bad.hidden_when(|level| level == Level::Preferred);
    let row = Child::row(vec![part(40.0, 40.0, 20.0), hidden], 4.0);
    let plan = row.query_at(Level::Preferred, &(), room).unwrap();
    assert_eq!(placed(&plan), vec![(0, [0.0, 0.0], [40.0, 20.0])]);
}
