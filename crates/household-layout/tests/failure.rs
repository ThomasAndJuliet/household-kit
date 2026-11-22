// SPDX-License-Identifier: GPL-3.0-only

//! A failure names the part it is about (`Child::query_at_blaming`): its
//! path, and the part at that path (`Child::part_at`), told by the name the
//! test gives it as its note.
//!
//! Every case asks through `query_at_blaming` and checks that `query_at`
//! returns the same error, so the error and the path come from the same
//! planning. There is one case for each check that can fail a plan, in the
//! order of the table in the report of this change; the check that a part's
//! plan is no wider than its room has none, because no tree reaches it (a
//! part is always planned within the width its parent checked or
//! allocated).

mod common;

use common::{Part, Shape, all_round, fixed, named, no_padding, part, stubborn, viewport};
use household_layout::{
    AllocationError, Child, Kind, Level, Measured, Padding, Size, Stage, choose_stage,
    choose_stage_blaming,
};

/// The failure of `root` at `level` in `width` × `height`: its error, its
/// path, and the name of the part at its path. `query_at` fails with the
/// same error.
fn blame(
    root: &Part,
    level: Level,
    width: f32,
    height: f32,
) -> (AllocationError, Vec<usize>, &'static str) {
    let room = Size::new(width, height);
    let failure = root.query_at_blaming(level, &(), room).unwrap_err();
    assert_eq!(root.query_at(level, &(), room), Err(failure.error()));
    let part = root.part_at(level, failure.path()).unwrap();
    (failure.error(), failure.path().to_vec(), *part.note())
}

fn no_fit(required: f32, available: f32) -> AllocationError {
    AllocationError::NoFit {
        required,
        available,
    }
}

const INVALID: AllocationError = AllocationError::InvalidInput;

/// A measurement of `preferred` and `minimum` in place of a part's own.
fn measured(preferred: Size, minimum: Size) -> impl Fn(Level, &()) -> Measured {
    move |_, _| Measured { preferred, minimum }
}

// `query_at` checks the room, the root's measurement and the root's floor
// of width before anything is planned: each is about the part asked.
#[test]
fn the_checks_before_planning_are_the_part_asked() {
    let leaf = || named("leaf", fixed(120.0, 10.0));
    // A room that is not a size.
    assert_eq!(
        blame(&leaf(), Level::Preferred, f32::NAN, 100.0),
        (INVALID, vec![], "leaf")
    );
    // A floor wider than the room: the root itself does not fit.
    assert_eq!(
        blame(&leaf(), Level::Preferred, 100.0, 100.0),
        (no_fit(120.0, 100.0), vec![], "leaf")
    );
    // A row's floor is its children's floors and gaps together, 40 + 80:
    // the room is too narrow for the row, before either child is planned.
    let row = named(
        "row",
        Child::row(
            vec![
                named("a", part(40.0, 40.0, 10.0).weight(0.0)),
                named("b", part(120.0, 80.0, 10.0)),
            ],
            0.0,
        ),
    );
    assert_eq!(
        blame(&row, Level::Preferred, 100.0, 100.0),
        (no_fit(120.0, 100.0), vec![], "row")
    );
}

// An invalid part makes every measurement that includes it invalid, so the
// failure names the first part whose measurement is checked: the part asked,
// whose measurement `query_at` checks first. A child whose own measurement is
// not a valid size, inside a parent whose measurement is, is named by the
// parent's check of it; a part planned without its measurement checked (a
// frame's content, when the frame's measurement is replaced) is named by its
// own mark.
#[test]
fn an_invalid_part_is_named_where_its_measurement_is_first_checked() {
    let row = named(
        "row",
        Child::row(
            vec![
                named("a", part(40.0, 40.0, 10.0)),
                named("bad", part(40.0, 40.0, 10.0).weight(f32::NAN)),
            ],
            0.0,
        ),
    );
    assert_eq!(
        blame(&row, Level::Preferred, 200.0, 100.0),
        (INVALID, vec![], "row")
    );

    // The child's minimum is wider than its preference; the row's sums are
    // a valid size (preferring 110, down to 20), so the row checks the child.
    let row = named(
        "row",
        Child::row(
            vec![
                named("a", part(100.0, 0.0, 10.0)),
                named(
                    "bad",
                    fixed(10.0, 10.0)
                        .with_measurement(measured(Size::new(10.0, 10.0), Size::new(20.0, 10.0))),
                ),
            ],
            0.0,
        ),
    );
    assert_eq!(
        blame(&row, Level::Preferred, 200.0, 100.0),
        (INVALID, vec![1], "bad")
    );

    let column = named(
        "column",
        Child::column(
            vec![named(
                "frame",
                Child::frame(
                    named("bad", part(40.0, 40.0, 10.0).weight(f32::NAN)),
                    no_padding(),
                    (),
                )
                .with_measurement(measured(Size::new(40.0, 10.0), Size::new(40.0, 10.0))),
            )],
            0.0,
        ),
    );
    assert_eq!(
        blame(&column, Level::Preferred, 100.0, 100.0),
        (INVALID, vec![0, 0], "bad")
    );
}

// The checks a part makes of itself while it is planned: its fixed size, a
// leaf's size against its offer, and its plan against its room.
#[test]
fn the_checks_a_part_makes_of_itself_name_it() {
    // Fixed at 60 wide, inside a frame whose measurement says 50: the frame
    // gives its content the 55 of the room.
    let frame = named(
        "frame",
        Child::frame(
            named(
                "fixed",
                part(40.0, 40.0, 10.0).fixed_dimensions(Some(60.0), None),
            ),
            no_padding(),
            (),
        )
        .with_measurement(measured(Size::new(50.0, 10.0), Size::new(50.0, 10.0))),
    );
    assert_eq!(
        blame(&frame, Level::Preferred, 55.0, 100.0),
        (no_fit(60.0, 55.0), vec![0], "fixed")
    );

    // Fixed at 50 high, in a row 40 high.
    let row = named(
        "row",
        Child::row(
            vec![
                named("a", part(40.0, 40.0, 10.0)),
                named(
                    "fixed",
                    part(40.0, 40.0, 10.0).fixed_dimensions(None, Some(50.0)),
                ),
            ],
            0.0,
        ),
    );
    assert_eq!(
        blame(&row, Level::Preferred, 200.0, 40.0),
        (no_fit(50.0, 40.0), vec![1], "fixed")
    );

    // A leaf that measures but has no size at the width it is given.
    let row = named(
        "row",
        Child::row(
            vec![
                named("a", part(40.0, 40.0, 10.0)),
                named("broken", Child::leaf(Shape::Broken)),
            ],
            0.0,
        ),
    );
    assert_eq!(
        blame(&row, Level::Preferred, 200.0, 100.0),
        (INVALID, vec![1], "broken")
    );

    // A leaf usable down to 40 that is 120 wide whatever it is given: the
    // row gives it 60 of its 100.
    let row = named(
        "row",
        Child::row(
            vec![
                named("a", part(40.0, 40.0, 10.0)),
                named("wide", fixed(120.0, 10.0).minimum_width(|_, _| 40.0)),
            ],
            0.0,
        ),
    );
    assert_eq!(
        blame(&row, Level::Preferred, 100.0, 100.0),
        (no_fit(120.0, 60.0), vec![1], "wide")
    );

    // A leaf that takes a range of heights down to 10, offered 30 by the
    // row, and 60 high however little it is offered.
    let row = named(
        "row",
        Child::row(
            vec![
                named("a", part(40.0, 40.0, 10.0)),
                named("stubborn", stubborn()),
            ],
            0.0,
        ),
    );
    assert_eq!(
        blame(&row, Level::Preferred, 200.0, 30.0),
        (no_fit(60.0, 30.0), vec![1], "stubborn")
    );

    // An ordinary leaf 40 high, in a row 30 high: its plan is taller than
    // its room.
    let row = named(
        "row",
        Child::row(
            vec![
                named("a", part(40.0, 40.0, 10.0)),
                named("tall", part(40.0, 40.0, 40.0)),
            ],
            0.0,
        ),
    );
    assert_eq!(
        blame(&row, Level::Preferred, 200.0, 30.0),
        (no_fit(40.0, 30.0), vec![1], "tall")
    );
}

// A room smaller than a frame's padding is the frame's; what fails inside it
// comes up with the content's step, 0.
#[test]
fn a_frame_is_named_for_its_padding_and_passes_its_content_on() {
    let padding = Padding {
        top: 0.0,
        right: 3.0,
        bottom: 0.0,
        left: 5.0,
    };
    // Declared usable down to nothing, so the room reaches the frame.
    let frame = named(
        "frame",
        Child::frame(named("content", part(10.0, 0.0, 10.0)), padding, ())
            .minimum_width(|_, _| 0.0),
    );
    assert_eq!(
        blame(&frame, Level::Preferred, 7.0, 100.0),
        (no_fit(8.0, 7.0), vec![], "frame")
    );

    let padding = Padding {
        top: 10.0,
        right: 0.0,
        bottom: 10.0,
        left: 0.0,
    };
    let row = named(
        "row",
        Child::row(
            vec![named(
                "framed",
                Child::frame(named("content", part(40.0, 40.0, 5.0)), padding, ()),
            )],
            0.0,
        ),
    );
    assert_eq!(
        blame(&row, Level::Preferred, 100.0, 15.0),
        (no_fit(20.0, 15.0), vec![0], "framed")
    );

    let frame = named(
        "frame",
        Child::frame(named("tall", part(40.0, 40.0, 60.0)), no_padding(), ()),
    );
    assert_eq!(
        blame(&frame, Level::Preferred, 100.0, 50.0),
        (no_fit(60.0, 50.0), vec![0], "tall")
    );
}

// A column: its gaps and its heights together are its own; a child's floor
// against its width, and what fails while a child is planned, either time,
// are the child's.
#[test]
fn a_column_names_itself_or_the_child() {
    let three = |spacing| {
        named(
            "column",
            Child::column(
                vec![
                    named("a", part(10.0, 10.0, 10.0)),
                    named("b", part(10.0, 10.0, 10.0)),
                    named("c", part(10.0, 10.0, 10.0)),
                ],
                spacing,
            ),
        )
    };
    assert_eq!(
        blame(&three(30.0), Level::Preferred, 50.0, 50.0),
        (no_fit(60.0, 50.0), vec![], "column")
    );

    // Declared usable down to nothing, so the column's width reaches the
    // child, whose floor is 80.
    let column = named(
        "column",
        Child::column(vec![named("a", part(100.0, 80.0, 10.0))], 0.0).minimum_width(|_, _| 0.0),
    );
    assert_eq!(
        blame(&column, Level::Preferred, 60.0, 100.0),
        (no_fit(80.0, 60.0), vec![0], "a")
    );

    let column = named(
        "column",
        Child::column(
            vec![
                named("a", part(40.0, 40.0, 10.0)),
                named("broken", Child::leaf(Shape::Broken)),
            ],
            0.0,
        ),
    );
    assert_eq!(
        blame(&column, Level::Preferred, 100.0, 100.0),
        (INVALID, vec![1], "broken")
    );

    // 20 and 30 high with a gap of 10, in 50.
    let column = named(
        "column",
        Child::column(
            vec![
                named("a", part(50.0, 50.0, 20.0)),
                named("b", part(50.0, 50.0, 30.0)),
            ],
            10.0,
        ),
    );
    assert_eq!(
        blame(&column, Level::Preferred, 50.0, 50.0),
        (no_fit(50.0, 40.0), vec![], "column")
    );

    // The leaf that takes a range of heights is given 30 of its 60 and
    // planned again there, where it does not fit.
    let column = named(
        "column",
        Child::column(
            vec![
                named("a", part(80.0, 80.0, 20.0)),
                named("stubborn", stubborn()),
            ],
            10.0,
        ),
    );
    assert_eq!(
        blame(&column, Level::Preferred, 80.0, 60.0),
        (no_fit(60.0, 30.0), vec![1], "stubborn")
    );
}

// A row: a child's floor against its width is the child's; its gaps and its
// children's widths together are its own.
#[test]
fn a_row_names_itself_or_the_child() {
    let row = named(
        "row",
        Child::row(vec![named("a", part(100.0, 80.0, 10.0))], 0.0).minimum_width(|_, _| 0.0),
    );
    assert_eq!(
        blame(&row, Level::Preferred, 60.0, 100.0),
        (no_fit(80.0, 60.0), vec![0], "a")
    );

    let row = named(
        "row",
        Child::row(
            vec![
                named("a", part(10.0, 0.0, 10.0)),
                named("b", part(10.0, 0.0, 10.0)),
                named("c", part(10.0, 0.0, 10.0)),
            ],
            30.0,
        )
        .minimum_width(|_, _| 0.0),
    );
    assert_eq!(
        blame(&row, Level::Preferred, 50.0, 100.0),
        (no_fit(60.0, 50.0), vec![], "row")
    );

    // Each floor of 60 fits in 100; together with the gap they need 120 of
    // the 90 the gap leaves.
    let row = named(
        "row",
        Child::row(
            vec![
                named("a", part(60.0, 60.0, 20.0)),
                named("b", part(60.0, 60.0, 20.0)),
            ],
            10.0,
        )
        .minimum_width(|_, _| 0.0),
    );
    assert_eq!(
        blame(&row, Level::Preferred, 100.0, 100.0),
        (no_fit(120.0, 90.0), vec![], "row")
    );
}

// A wrap: a child's floor against its width is the child's; the gaps
// between its rows and its rows' heights together are its own; a child
// planned again at its row's share is the child.
#[test]
fn a_wrap_names_itself_or_the_child() {
    let wrap = named(
        "wrap",
        Child::wrap(vec![named("a", part(100.0, 80.0, 10.0))], 0.0).minimum_width(|_, _| 0.0),
    );
    assert_eq!(
        blame(&wrap, Level::Preferred, 60.0, 100.0),
        (no_fit(80.0, 60.0), vec![0], "a")
    );

    // Three rows of no height, two gaps of 30, in 50.
    let wrap = named(
        "wrap",
        Child::wrap(
            vec![
                named("a", part(80.0, 80.0, 0.0)),
                named("b", part(80.0, 80.0, 0.0)),
                named("c", part(80.0, 80.0, 0.0)),
            ],
            30.0,
        ),
    );
    assert_eq!(
        blame(&wrap, Level::Preferred, 80.0, 50.0),
        (no_fit(60.0, 50.0), vec![], "wrap")
    );

    // Two rows, 20 and 30 high, in 40 less a gap of 10.
    let wrap = named(
        "wrap",
        Child::wrap(
            vec![
                named("a", part(80.0, 80.0, 20.0)),
                named("b", part(80.0, 80.0, 30.0)),
            ],
            10.0,
        ),
    );
    assert_eq!(
        blame(&wrap, Level::Preferred, 80.0, 40.0),
        (no_fit(50.0, 30.0), vec![], "wrap")
    );

    // The second row is given 30 of its 60, and its leaf, planned again
    // there, does not fit.
    let wrap = named(
        "wrap",
        Child::wrap(
            vec![
                named("a", part(80.0, 80.0, 20.0)),
                named("stubborn", stubborn()),
            ],
            10.0,
        ),
    );
    assert_eq!(
        blame(&wrap, Level::Preferred, 80.0, 60.0),
        (no_fit(60.0, 30.0), vec![1], "stubborn")
    );
}

// Forms add no step: a failure inside the selected form has the path inside
// the form, and `part_at` goes through the family to it. A failure of the
// form itself names the family's node, and `part_at` gives the family, whose
// selected form is one step of reading away.
#[test]
fn forms_add_no_step() {
    let family = named(
        "family",
        Child::forms(
            (Level::Preferred, named("wide", fixed(100.0, 10.0))),
            vec![(
                Level::Strong,
                named(
                    "row",
                    Child::row(
                        vec![
                            named("a", part(40.0, 40.0, 10.0)),
                            named("tall", part(40.0, 40.0, 40.0)),
                        ],
                        0.0,
                    ),
                ),
            )],
        ),
    );
    assert_eq!(
        blame(&family, Level::Strong, 200.0, 20.0),
        (no_fit(40.0, 20.0), vec![1], "tall")
    );
    assert_eq!(
        blame(&family, Level::Preferred, 50.0, 20.0),
        (no_fit(100.0, 50.0), vec![], "family")
    );
    let Kind::Forms(forms) = family.kind() else {
        panic!("a family is forms");
    };
    assert_eq!(*forms.select(Level::Preferred).note(), "wide");
    // A step into a family at a level whose form is a leaf names nothing.
    assert!(family.part_at(Level::Preferred, &[1]).is_none());
}

// A leaf 120 wide usable down to 40, in a row inside a frame inside a
// column, in 100: the column gives the frame 100, the frame gives its row
// 90, the row gives the leaf 60 of the 80 its gap leaves, and the leaf is
// 120 wide whatever it is given. Column, frame, row, leaf: three steps.
#[test]
fn a_failure_three_deep_has_a_step_for_each_placement() {
    let column = named(
        "column",
        Child::column(
            vec![
                named("p", part(20.0, 20.0, 10.0)),
                named(
                    "frame",
                    Child::frame(
                        named(
                            "row",
                            Child::row(
                                vec![
                                    named("q", part(20.0, 20.0, 10.0)),
                                    named("wide", fixed(120.0, 10.0).minimum_width(|_, _| 40.0)),
                                ],
                                10.0,
                            ),
                        ),
                        all_round(5.0),
                        (),
                    ),
                ),
            ],
            0.0,
        ),
    );
    assert_eq!(
        blame(&column, Level::Preferred, 100.0, 100.0),
        (no_fit(120.0, 60.0), vec![1, 0, 1], "wide")
    );
    // Each shorter path names the part round it.
    let names: Vec<_> = [&[][..], &[1], &[1, 0], &[1, 0, 1]]
        .into_iter()
        .map(|path| *column.part_at(Level::Preferred, path).unwrap().note())
        .collect();
    assert_eq!(names, ["column", "frame", "row", "wide"]);
    // Steps that name no part.
    for path in [&[2][..], &[1, 1], &[1, 0, 2], &[0, 0]] {
        assert!(column.part_at(Level::Preferred, path).is_none(), "{path:?}");
    }
}

// What each of three stages lacked, in order, each failure's path from its
// own stage's part; `choose_stage` says only that three were tried. A
// fourth stage that fits is chosen by both alike.
#[test]
fn what_each_stage_lacked() {
    let room = Size::new(100.0, 50.0);
    let row_of = |second: Part| {
        named(
            "row",
            Child::row(
                vec![named("a", part(40.0, 40.0, 10.0).weight(0.0)), second],
                0.0,
            ),
        )
    };
    let mut stages = vec![
        Stage::new(
            Level::Preferred,
            row_of(named("b", part(120.0, 80.0, 10.0))),
        ),
        Stage::new(
            Level::Mild,
            named(
                "frame",
                Child::frame(named("tall", part(40.0, 40.0, 60.0)), no_padding(), ()),
            ),
        ),
        Stage::new(Level::Strong, row_of(named("tall", part(40.0, 40.0, 60.0)))),
    ];
    let failures = choose_stage_blaming(&stages, &(), room).unwrap_err();
    let lacked: Vec<_> = failures
        .iter()
        .zip(&stages)
        .map(|(failure, stage)| {
            let part = stage.part.part_at(stage.level, failure.path()).unwrap();
            (failure.error(), failure.path().to_vec(), *part.note())
        })
        .collect();
    assert_eq!(
        lacked,
        [
            (no_fit(120.0, 100.0), vec![], "row"),
            (no_fit(60.0, 50.0), vec![0], "tall"),
            (no_fit(60.0, 50.0), vec![1], "tall"),
        ]
    );
    assert_eq!(choose_stage(&stages, &(), room).unwrap_err().attempted, 3);

    stages.push(Stage::new(Level::AllPermitted, viewport()));
    let chosen = choose_stage_blaming(&stages, &(), room).unwrap();
    assert_eq!(chosen.stage_index, 3);
    assert_eq!(Ok(chosen), choose_stage(&stages, &(), room));
}
