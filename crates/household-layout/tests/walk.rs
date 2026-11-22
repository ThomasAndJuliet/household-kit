// SPDX-License-Identifier: GPL-3.0-only

//! One walk of a plan together with the parts that made it (`Child::walk`):
//! the parts visited in order, told by the name the test gives each as its
//! note; each node's position, checked against the plan's own placements
//! and against the numbers worked by hand; each path, and the part
//! `part_at` finds there; and the growth in height in effect for each.

mod common;

use common::{Part, fixed, named, part, viewport};
use household_layout::{Child, Kind, Level, Padding, Plan, Point, Size};

/// What a walk saw of one node.
#[derive(Debug, PartialEq)]
struct Seen {
    name: &'static str,
    path: Vec<usize>,
    position: [f32; 2],
    size: [f32; 2],
    grow_height: f32,
}

/// Walk `root`'s plan at `level`, checking at each visit that the position
/// is the plan's own (the placements on the path, added up, found by their
/// indices) and that the part is the one `part_at` finds at the path, or,
/// where a family and its form share the node, the form selected through
/// the family `part_at` finds.
fn walk(root: &Part, level: Level, plan: &Plan) -> Vec<Seen> {
    let mut seen = Vec::new();
    root.walk(level, plan, |visit| {
        let (position, node) = node_at(plan, visit.path);
        assert_eq!(visit.position, position, "{}", visit.part.note());
        assert!(std::ptr::eq(visit.plan, node), "{}", visit.part.note());
        let mut placed = root.part_at(level, visit.path).unwrap();
        while !std::ptr::eq(placed, visit.part) {
            let Kind::Forms(forms) = placed.kind() else {
                panic!("{} is not the part at its path", visit.part.note());
            };
            placed = forms.select(level);
        }
        seen.push(Seen {
            name: visit.part.note(),
            path: visit.path.to_vec(),
            position: [visit.position.x, visit.position.y],
            size: [visit.plan.size().width, visit.plan.size().height],
            grow_height: visit.grow_height,
        });
    });
    seen
}

/// The node of `plan` at `path`, and its position from the root's: each
/// step the placement with that index, its position added.
fn node_at<'p>(plan: &'p Plan, path: &[usize]) -> (Point, &'p Plan) {
    path.iter()
        .fold((Point::new(0.0, 0.0), plan), |(at, node), step| {
            let placed = node
                .children()
                .iter()
                .find(|placed| placed.index() == *step)
                .unwrap();
            (
                Point::new(at.x + placed.position().x, at.y + placed.position().y),
                placed.plan(),
            )
        })
}

fn seen(
    name: &'static str,
    path: &[usize],
    position: [f32; 2],
    size: [f32; 2],
    grow_height: f32,
) -> Seen {
    Seen {
        name,
        path: path.to_vec(),
        position,
        size,
        grow_height,
    }
}

// A column, in 300 × 400 at Mild, of: a leaf; a hidden leaf; a frame growing 3
// round a row of a leaf and a family growing 2, whose form at Mild is a
// wrap of two leaves; a hidden family; and a viewport growing 1.
//
// The column's children want 10, 26 and 60 of the 388 its two gaps of 6
// leave, and the 292 to spare goes 3 : 1 to the frame and the viewport:
// 10, 245 and 133, at 0, 16 and 267, and the column is as wide as its
// widest, the frame's 133. The frame gives its row 239 below its
// padding of 2 at the top and 5 at the left, and passes it its growth of 3.
// The row gives the leaf 30 and the family the 85 it prefers, after a gap
// of 10, and offers each its 239: the family grows to it, so the row is 239
// high and its 20-high leaf is centred, 109.5 down. The family's wrap, 85
// wide, holds both its leaves on one row, 40 each with a gap of 5, at the
// top of its 239. The hidden leaf and the hidden family are not placed, so
// they are not visited.
#[test]
fn a_walk_visits_every_node_with_the_part_that_made_it() {
    let column = named(
        "column",
        Child::column(
            vec![
                named("a", part(50.0, 50.0, 10.0)),
                named("hidden", part(50.0, 50.0, 10.0).hidden_when(|_| true)),
                named(
                    "frame",
                    Child::frame(
                        named(
                            "row",
                            Child::row(
                                vec![
                                    named("b", part(30.0, 30.0, 20.0)),
                                    named(
                                        "family",
                                        Child::forms(
                                            (Level::Preferred, named("wide", fixed(200.0, 10.0))),
                                            vec![(
                                                Level::Mild,
                                                named(
                                                    "wrap",
                                                    Child::wrap(
                                                        vec![
                                                            named("c", fixed(40.0, 10.0)),
                                                            named("d", fixed(40.0, 10.0)),
                                                        ],
                                                        5.0,
                                                    ),
                                                ),
                                            )],
                                        )
                                        .grow_height(2.0),
                                    ),
                                ],
                                10.0,
                            ),
                        ),
                        Padding {
                            top: 2.0,
                            right: 3.0,
                            bottom: 4.0,
                            left: 5.0,
                        },
                        (),
                    )
                    .grow_height(3.0),
                ),
                named(
                    "hidden family",
                    Child::forms(
                        (Level::Preferred, named("hidden form", fixed(10.0, 10.0))),
                        vec![],
                    )
                    .hidden_when(|_| true),
                ),
                named("viewport", viewport().grow_height(1.0)),
            ],
            6.0,
        ),
    );
    let level = Level::Mild;
    let plan = column
        .query_at(level, &(), Size::new(300.0, 400.0))
        .unwrap();
    assert_eq!(
        walk(&column, level, &plan),
        [
            seen("column", &[], [0.0, 0.0], [133.0, 400.0], 0.0),
            seen("a", &[0], [0.0, 0.0], [50.0, 10.0], 0.0),
            seen("frame", &[2], [0.0, 16.0], [133.0, 245.0], 3.0),
            seen("row", &[2, 0], [5.0, 18.0], [125.0, 239.0], 3.0),
            seen("b", &[2, 0, 0], [5.0, 127.5], [30.0, 20.0], 0.0),
            seen("family", &[2, 0, 1], [45.0, 18.0], [85.0, 239.0], 2.0),
            seen("wrap", &[2, 0, 1], [45.0, 18.0], [85.0, 239.0], 2.0),
            seen("c", &[2, 0, 1, 0], [45.0, 18.0], [40.0, 10.0], 0.0),
            seen("d", &[2, 0, 1, 1], [90.0, 18.0], [40.0, 10.0], 0.0),
            seen("viewport", &[4], [0.0, 267.0], [80.0, 133.0], 1.0),
        ]
    );
}

// A hidden node is visited with the part that was asked for it, and nothing
// below it: not a hidden family's form, not a hidden composition's children.
#[test]
fn nothing_below_a_hidden_node_is_visited() {
    let family = named(
        "family",
        Child::forms((Level::Preferred, named("form", fixed(10.0, 10.0))), vec![])
            .hidden_when(|_| true),
    );
    let column = named(
        "column",
        Child::column(vec![named("a", fixed(10.0, 10.0))], 0.0).hidden_when(|_| true),
    );
    for root in [family, column] {
        let plan = root
            .query_at(Level::Preferred, &(), Size::new(100.0, 100.0))
            .unwrap();
        assert!(plan.is_hidden());
        assert_eq!(
            walk(&root, Level::Preferred, &plan),
            [seen(root.note(), &[], [0.0, 0.0], [0.0, 0.0], 0.0)]
        );
    }
}

// A plan made by another tree: a row of three, the third a column of its
// own, walked with a row of two. The third placement names no part of the
// row of two, so it is skipped with the column's leaf below it.
#[test]
fn a_placement_that_names_no_part_is_skipped_with_everything_below_it() {
    let three = Child::row(
        vec![
            fixed(10.0, 10.0),
            fixed(10.0, 10.0),
            Child::column(vec![fixed(10.0, 10.0)], 0.0),
        ],
        0.0,
    );
    let plan = three
        .query_at(Level::Preferred, &(), Size::new(100.0, 100.0))
        .unwrap();
    let two = named(
        "row",
        Child::row(
            vec![named("a", fixed(10.0, 10.0)), named("b", fixed(10.0, 10.0))],
            0.0,
        ),
    );
    let names: Vec<_> = walk(&two, Level::Preferred, &plan)
        .into_iter()
        .map(|seen| seen.name)
        .collect();
    assert_eq!(names, ["row", "a", "b"]);
}
