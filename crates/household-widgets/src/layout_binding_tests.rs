// SPDX-License-Identifier: GPL-3.0-only

//! What binds a plan to the child it was planned from, and the height growth
//! a family or a frame passes to what it holds.
//!
//! Every node of a finished plan records which child made it, and at which
//! level (its bindings, the child's preparation identity): `render` draws a
//! node only for the child that made it, and `same_preparation` compares two
//! plans by them. A child that reports information adds its report to the
//! node it made, and its report callback is given that node. These tests
//! pin, for a tree that holds every kind of child, the bindings on every
//! node, the reports in order, and the node each report callback was given
//! the last time it ran, which must be the node in the finished plan as it
//! stood when that child bound it: with the bindings up to and including
//! the child's own, the reports of the children that bound it before, and
//! every node below it already bound. How many times a callback runs is not
//! pinned: it must be pure, so that number is not part of the result.
//!
//! The growth tests pin how far a family's or a frame's `grow_height`
//! reaches: to the form selected, or the content, and on through further
//! forms and frames, but not into a composition's children, which a column
//! plans by their own growth. Each holds the growing part below a header in
//! a column with spare height, and pins every node's size, floor of height
//! and native size, then renders the plan and checks that it agrees.

use super::test_fixtures::*;
use super::*;
use std::cell::RefCell;

/// Every call of a report callback: its source, its level, and the plan it
/// was given.
type Seen = Rc<RefCell<Vec<(SourceId, Level, GeometryPlan)>>>;

/// `child`, reporting as `source` and recording each call in `seen`.
fn reporting(child: Child<'static, ()>, source: u64, seen: &Seen) -> Child<'static, ()> {
    let seen = seen.clone();
    child.report_information(SourceId(source), 1, move |level, plan| {
        seen.borrow_mut()
            .push((SourceId(source), level, plan.clone()));
        Vec::new()
    })
}

/// A viewport `width` wide whose view takes all the height it is offered,
/// and 60 when unbounded, with a floor of `width` × 10.
fn viewport(width: f32) -> Child<'static, ()> {
    shaped_viewport(
        move |offer| {
            let height = if offer.height.is_finite() {
                offer.height
            } else {
                60.0
            };
            Size::new(width, height)
        },
        Size::new(width, 10.0),
    )
}

/// A frame of 2 round `content`, whose decorator adds exactly that.
fn framed(content: Child<'static, ()>) -> Child<'static, ()> {
    framed_child(content, 2.0, |content| {
        widget::container(content).padding(2.0).into()
    })
}

/// Render `child` from `plan` at `level` and lay it out in `room`, as a stage
/// is committed: whether the stage agrees.
fn agrees(child: &Child<'static, ()>, level: Level, plan: &GeometryPlan, room: Size) -> bool {
    let agreement = Rc::new(Cell::new(true));
    let mut content = child.render(level, plan, agreement.clone());
    let mut tree = Tree::new(content.as_widget());
    content.as_widget_mut().layout(
        &mut tree,
        &renderer(),
        &layout::Limits::new(Size::ZERO, room),
    );
    agreement.get()
}

/// Every node's bindings, the node first and then each placement's, in
/// order.
fn bindings(plan: &GeometryPlan) -> Vec<Vec<(PreparedId, Level)>> {
    let mut all = vec![plan.bindings.clone()];
    for placed in &plan.children {
        all.extend(bindings(&placed.geometry));
    }
    all
}

/// The node of `plan` that holds the report of `source`.
fn reported_by(plan: &GeometryPlan, source: SourceId) -> Option<&GeometryPlan> {
    if plan
        .source_reports
        .iter()
        .any(|report| report.source == source)
    {
        return Some(plan);
    }
    plan.children
        .iter()
        .find_map(|placed| reported_by(&placed.geometry, source))
}

/// `node` as the child that reports as `source` found it when it bound it:
/// with the bindings up to its own (a child's binding goes on before its
/// report is made) and the reports of the children that bound it before.
fn as_bound_by(node: &GeometryPlan, source: SourceId, owner: &PreparedId) -> GeometryPlan {
    let mut node = node.clone();
    let report = node
        .source_reports
        .iter()
        .position(|report| report.source == source)
        .unwrap();
    node.source_reports.truncate(report);
    let binding = node
        .bindings
        .iter()
        .position(|(id, _)| id == owner)
        .unwrap();
    node.bindings.truncate(binding + 1);
    node
}

/// A node's size, floor of height and native size.
type Node = ([f32; 2], f32, Option<[f32; 2]>);

/// Every node's size, floor of height and native size, the node first and
/// then each placement's, in order.
fn nodes(plan: &GeometryPlan) -> Vec<Node> {
    let mut all = vec![(
        [plan.size.width, plan.size.height],
        plan.minimum_height,
        plan.native_size.map(|size| [size.width, size.height]),
    )];
    for placed in &plan.children {
        all.extend(nodes(&placed.geometry));
    }
    all
}

/// The tree with every kind, and a clone of each reporting part, which
/// shares its preparation identity.
struct Parts {
    column: Child<'static, ()>,
    row: Child<'static, ()>,
    a: Child<'static, ()>,
    b: Child<'static, ()>,
    wrap: Child<'static, ()>,
    c: Child<'static, ()>,
    d: Child<'static, ()>,
    frame: Child<'static, ()>,
    e: Child<'static, ()>,
    family: Child<'static, ()>,
    selected: Child<'static, ()>,
    f: Child<'static, ()>,
    g: Child<'static, ()>,
    viewport: Child<'static, ()>,
}

/// A column 60 × 80 at `Mild` holding, in order: a row of a 30 and a 20
/// (each 10 high) with a gap of 5; a wrap of two 40s, which takes a row
/// each at 60; a frame of 2 round a 20 × 10; forms whose form at `Mild` is a
/// row of two 10 × 10s; a viewport 40 wide, preferring 60 high, which takes
/// the column's shortage of 39 and is planned again at 21; a hidden leaf;
/// and forms hidden because their one form is. Every part but `b`, `d`, `g`
/// and the forms' first form reports, as the number its comment gives.
fn parts(seen: &Seen) -> Parts {
    let a = reporting(sized(30.0, 10.0), 1, seen);
    let b = sized(20.0, 10.0);
    let row = reporting(row_child(vec![a.clone(), b.clone()], 5.0), 2, seen);
    let c = reporting(sized(40.0, 10.0), 3, seen);
    let d = sized(40.0, 10.0);
    let wrap = reporting(wrap_child(vec![c.clone(), d.clone()], 5.0), 4, seen);
    let e = reporting(sized(20.0, 10.0), 5, seen);
    let frame = reporting(framed(e.clone()), 6, seen);
    let f = reporting(sized(10.0, 10.0), 7, seen);
    let g = sized(10.0, 10.0);
    let selected = reporting(row_child(vec![f.clone(), g.clone()], 0.0), 8, seen);
    let family = reporting(
        forms(vec![
            (Level::Preferred, sized(50.0, 10.0)),
            (Level::Mild, selected.clone()),
        ]),
        9,
        seen,
    );
    let viewport = reporting(viewport(40.0), 10, seen);
    let hidden = reporting(sized(10.0, 10.0).hidden_when(|_| true), 11, seen);
    let hidden_family = reporting(
        forms(vec![(
            Level::Preferred,
            reporting(sized(10.0, 10.0).hidden_when(|_| true), 12, seen),
        )]),
        13,
        seen,
    );
    let column = reporting(
        column_child(
            vec![
                row.clone(),
                wrap.clone(),
                frame.clone(),
                family.clone(),
                viewport.clone(),
                hidden,
                hidden_family,
            ],
            0.0,
        ),
        14,
        seen,
    );
    Parts {
        column,
        row,
        a,
        b,
        wrap,
        c,
        d,
        frame,
        e,
        family,
        selected,
        f,
        g,
        viewport,
    }
}

const ROOM: Size = Size::new(60.0, 80.0);

fn planned(parts: &Parts) -> GeometryPlan {
    parts
        .column
        .query_at(Level::Mild, &renderer(), ROOM)
        .unwrap()
}

#[test]
fn the_tree_with_every_kind_is_planned_as_its_parts_say() {
    let seen = Seen::default();
    let plan = planned(&parts(&seen));
    // The hidden leaf and the hidden forms (indices 5 and 6) are omitted.
    assert_eq!(
        placed(&plan),
        vec![
            (0, [0.0, 0.0], [55.0, 10.0]),
            (1, [0.0, 10.0], [40.0, 25.0]),
            (2, [0.0, 35.0], [24.0, 14.0]),
            (3, [0.0, 49.0], [20.0, 10.0]),
            (4, [0.0, 59.0], [40.0, 21.0]),
        ]
    );
    assert_eq!(plan.size, Size::new(55.0, 80.0));
}

#[test]
fn every_node_of_a_finished_plan_is_bound_by_the_children_that_made_it() {
    let seen = Seen::default();
    let parts = parts(&seen);
    let plan = planned(&parts);
    let by = |child: &Child<'static, ()>| (child.preparation().clone(), Level::Mild);
    assert_eq!(
        bindings(&plan),
        vec![
            vec![by(&parts.column)],
            vec![by(&parts.row)],
            vec![by(&parts.a)],
            vec![by(&parts.b)],
            vec![by(&parts.wrap)],
            vec![by(&parts.c)],
            vec![by(&parts.d)],
            vec![by(&parts.frame)],
            vec![by(&parts.e)],
            // The node the selected form made is the family's: the form
            // binds it first, then the family.
            vec![by(&parts.selected), by(&parts.family)],
            vec![by(&parts.f)],
            vec![by(&parts.g)],
            vec![by(&parts.viewport)],
        ]
    );
    // A second query binds the same children; a tree built again does not.
    let again = parts
        .column
        .query_at(Level::Mild, &renderer(), ROOM)
        .unwrap();
    assert!(plan.same_preparation(&again));
    let rebuilt = planned(&self::parts(&Seen::default()));
    assert_eq!(rebuilt, plan);
    assert!(!plan.same_preparation(&rebuilt));
    // The plan renders, and agrees, for the child it was planned from only.
    assert!(agrees(&parts.column, Level::Mild, &plan, ROOM));
    assert!(!agrees(&parts.column, Level::Mild, &rebuilt, ROOM));
}

#[test]
fn reports_list_every_reporting_node_in_order_with_its_owner_and_size() {
    let seen = Seen::default();
    let parts = parts(&seen);
    let plan = planned(&parts);
    let reports: Vec<_> = plan
        .reports()
        .into_iter()
        .map(|report| {
            (
                report.source.0,
                report.level,
                [report.size.width, report.size.height],
                report.owner.clone(),
            )
        })
        .collect();
    let mild = Level::Mild;
    let id = |child: &Child<'static, ()>| child.preparation().clone();
    assert_eq!(
        reports,
        vec![
            (14, mild, [55.0, 80.0], id(&parts.column)),
            (2, mild, [55.0, 10.0], id(&parts.row)),
            (1, mild, [30.0, 10.0], id(&parts.a)),
            (4, mild, [40.0, 25.0], id(&parts.wrap)),
            (3, mild, [40.0, 10.0], id(&parts.c)),
            (6, mild, [24.0, 14.0], id(&parts.frame)),
            (5, mild, [20.0, 10.0], id(&parts.e)),
            (8, mild, [20.0, 10.0], id(&parts.selected)),
            (9, mild, [20.0, 10.0], id(&parts.family)),
            (7, mild, [10.0, 10.0], id(&parts.f)),
            (10, mild, [40.0, 21.0], id(&parts.viewport)),
        ]
    );
}

#[test]
fn each_report_callback_last_saw_its_node_as_it_stood_when_bound() {
    let seen = Seen::default();
    let parts = parts(&seen);
    let plan = planned(&parts);
    let owners = [
        (1, &parts.a),
        (2, &parts.row),
        (3, &parts.c),
        (4, &parts.wrap),
        (5, &parts.e),
        (6, &parts.frame),
        (7, &parts.f),
        (8, &parts.selected),
        (9, &parts.family),
        (10, &parts.viewport),
        (14, &parts.column),
    ];
    let seen = seen.borrow();
    // The callbacks that ran are those of the reports, and no others: not
    // the hidden parts' (11, 12, 13).
    let mut sources: Vec<_> = seen.iter().map(|(source, _, _)| source.0).collect();
    sources.sort_unstable();
    sources.dedup();
    assert_eq!(sources, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 14]);
    for (source, owner) in owners {
        let source = SourceId(source);
        let (_, level, last) = seen
            .iter()
            .rev()
            .find(|(seen, _, _)| *seen == source)
            .unwrap();
        assert_eq!(*level, Level::Mild);
        let node = reported_by(&plan, source).unwrap();
        let expected = as_bound_by(node, source, owner.preparation());
        assert_eq!(*last, expected, "{source:?}");
        assert!(last.same_preparation(&expected), "{source:?}");
    }
}

#[test]
fn a_hidden_child_or_family_at_the_root_is_bound_by_itself_alone_and_reports_nothing() {
    let seen = Seen::default();
    let leaf = reporting(sized(10.0, 10.0).hidden_when(|_| true), 1, &seen);
    let inner = reporting(sized(10.0, 10.0).hidden_when(|_| true), 2, &seen);
    let family = reporting(forms(vec![(Level::Preferred, inner)]), 3, &seen);
    let declared = reporting(
        forms(vec![(
            Level::Preferred,
            reporting(sized(10.0, 10.0), 4, &seen),
        )])
        .hidden_when(|_| true),
        5,
        &seen,
    );
    let content = reporting(sized(10.0, 10.0).hidden_when(|_| true), 6, &seen);
    let frame = reporting(framed(content), 7, &seen);
    for hidden in [&leaf, &family, &declared, &frame] {
        let plan = query_at(hidden, 60.0, 80.0).unwrap();
        assert!(plan.hidden);
        assert!(plan.children.is_empty());
        assert_eq!(
            plan.bindings,
            vec![(hidden.preparation().clone(), Level::Preferred)]
        );
        assert!(plan.reports().is_empty());
        assert!(agrees(hidden, Level::Preferred, &plan, ROOM));
    }
    assert!(seen.borrow().is_empty());
}

/// Forms over a viewport 40 wide whose form reports as 1, with the family
/// reporting as 2.
fn reporting_forms(seen: &Seen) -> (Child<'static, ()>, Child<'static, ()>) {
    let selected = reporting(viewport(40.0), 1, seen);
    let family = reporting(forms(vec![(Level::Preferred, selected.clone())]), 2, seen);
    (family, selected)
}

/// What the family plans after its selected form has bound the node cannot
/// change it: a forms child is never given a fixed or a reserved width
/// (`fixed_dimensions` and `fill_width` put it in a column, whose node is
/// another), and its growth in height is the one the form was planned with,
/// so the family's raising of the node's height to the room repeats what
/// the form did. Here, in a column with spare height, under each wrapper,
/// the form's callback last saw the node it reported on as it now stands.
#[test]
fn a_forms_familys_own_adjustments_leave_the_node_its_form_reported_on() {
    type Wrap = fn(Child<'static, ()>) -> Child<'static, ()>;
    let wrappers: [(Wrap, [f32; 2]); 4] = [
        (|family| family, [40.0, 60.0]),
        (|family| family.grow_height(1.0), [40.0, 90.0]),
        // The column the wrapper puts the family in is 70 high; the family
        // in it keeps its 60, as it does not grow.
        (
            |family| family.fixed_dimensions(Some(40.0), Some(70.0)),
            [40.0, 60.0],
        ),
        (|family| family.fill_width(), [40.0, 60.0]),
    ];
    for (wrap, form_size) in wrappers {
        let seen = Seen::default();
        let (family, selected) = reporting_forms(&seen);
        let wrapped = wrap(family.clone());
        let column = column_child(vec![sized(40.0, 10.0), wrapped.clone()], 0.0);
        let plan = query_at(&column, 40.0, 100.0).unwrap();
        // The family's node: the wrapped child's, or the one placement of
        // the column a wrapper put it in.
        // `grow_height` gives the family a new preparation identity; the
        // wrappers hold the family as it was.
        let mut node = &plan.children[1].geometry;
        let mut family = &family;
        if matches!(wrapped.kind, Kind::Forms(_)) {
            family = &wrapped;
        } else {
            assert!(matches!(wrapped.kind, Kind::Composition(_)));
            node = &node.children[0].geometry;
        }
        assert_eq!([node.size.width, node.size.height], form_size);
        assert_eq!(
            node.bindings,
            vec![
                (selected.preparation().clone(), Level::Preferred),
                (family.preparation().clone(), Level::Preferred),
            ]
        );
        let seen = seen.borrow();
        let last = |source: SourceId| {
            seen.iter()
                .rev()
                .find(|(seen, _, _)| *seen == source)
                .map(|(_, _, plan)| plan.clone())
                .unwrap()
        };
        let form = as_bound_by(node, SourceId(1), selected.preparation());
        assert_eq!(last(SourceId(1)), form);
        assert!(last(SourceId(1)).same_preparation(&form));
        let whole = as_bound_by(node, SourceId(2), family.preparation());
        assert_eq!(last(SourceId(2)), whole);
        assert!(agrees(
            &column,
            Level::Preferred,
            &plan,
            Size::new(40.0, 100.0)
        ));
    }
}

/// `part` below a header 40 × 10 in a column planned in 40 × 100, so it has
/// 90 of height to grow into: every node's size, floor and native size, and
/// whether the plan renders in agreement.
fn grown(part: Child<'static, ()>) -> (Vec<Node>, bool) {
    let room = Size::new(40.0, 100.0);
    let column = column_child(vec![sized(40.0, 10.0), part], 0.0);
    let plan = query_at(&column, room.width, room.height).unwrap();
    let agreed = agrees(&column, Level::Preferred, &plan, room);
    (nodes(&plan), agreed)
}

/// The column and the header, when the part takes `height` with a floor of
/// `floor`.
fn column_and_header(height: f32, floor: f32) -> Vec<Node> {
    vec![
        ([40.0, 10.0 + height], 10.0 + floor, None),
        ([40.0, 10.0], 10.0, Some([40.0, 10.0])),
    ]
}

// The family grows into the 90, and its form, a viewport that does not grow
// of its own, takes the 90: its view is laid out at 90.
#[test]
fn forms_growing_in_height_pass_it_to_a_form_that_has_none() {
    let mut expected = column_and_header(90.0, 10.0);
    expected.push(([40.0, 90.0], 10.0, Some([40.0, 90.0])));
    let family = forms(vec![(Level::Preferred, viewport(40.0))]).grow_height(1.0);
    assert_eq!(grown(family), (expected, true));
}

// The family's growth stands in for the form's own, which is not read: the
// result is the same as with none.
#[test]
fn forms_growing_in_height_stand_in_for_a_forms_own_growth() {
    let mut expected = column_and_header(90.0, 10.0);
    expected.push(([40.0, 90.0], 10.0, Some([40.0, 90.0])));
    let family = forms(vec![(Level::Preferred, viewport(40.0).grow_height(3.0))]).grow_height(1.0);
    assert_eq!(grown(family), (expected, true));
}

// Forms that do not grow leave the form its own growth, but the column
// shares its height by the family's, which is none: the family keeps the
// 60 its form prefers, and the rest of the room is left over.
#[test]
fn forms_that_do_not_grow_leave_the_form_its_own_growth() {
    let mut expected = column_and_header(60.0, 10.0);
    expected.push(([40.0, 60.0], 10.0, Some([40.0, 60.0])));
    let family = forms(vec![(Level::Preferred, viewport(40.0).grow_height(1.0))]);
    assert_eq!(grown(family), (expected, true));
}

// A frame growing in height passes it to its content: the viewport takes
// the 86 inside the padding.
#[test]
fn a_frame_growing_in_height_passes_it_to_content_that_has_none() {
    let mut expected = column_and_header(90.0, 14.0);
    expected.push(([40.0, 90.0], 14.0, None));
    expected.push(([36.0, 86.0], 10.0, Some([36.0, 86.0])));
    assert_eq!(
        grown(framed(viewport(36.0)).grow_height(1.0)),
        (expected, true)
    );
    let own = framed(viewport(36.0).grow_height(5.0)).grow_height(1.0);
    assert_eq!(
        grown(own).0,
        grown(framed(viewport(36.0)).grow_height(1.0)).0
    );
}

// A frame that does not grow leaves its content its own growth, and the
// column gives the frame none of the spare height.
#[test]
fn a_frame_that_does_not_grow_leaves_the_content_its_own_growth() {
    let mut expected = column_and_header(64.0, 14.0);
    expected.push(([40.0, 64.0], 14.0, None));
    expected.push(([36.0, 60.0], 10.0, Some([36.0, 60.0])));
    assert_eq!(
        grown(framed(viewport(36.0).grow_height(5.0))),
        (expected, true)
    );
}

// Two deep: forms over a frame, a frame over forms, and forms over forms
// each pass the growth through both to the viewport.
#[test]
fn growth_in_height_passes_through_forms_and_frames_nested_two_deep() {
    let mut framed_viewport = column_and_header(90.0, 14.0);
    framed_viewport.push(([40.0, 90.0], 14.0, None));
    framed_viewport.push(([36.0, 86.0], 10.0, Some([36.0, 86.0])));
    let forms_over_frame = forms(vec![(Level::Preferred, framed(viewport(36.0)))]).grow_height(1.0);
    assert_eq!(grown(forms_over_frame), (framed_viewport.clone(), true));
    let frame_over_forms = framed(forms(vec![(Level::Preferred, viewport(36.0))])).grow_height(1.0);
    assert_eq!(grown(frame_over_forms), (framed_viewport, true));

    let mut viewport_alone = column_and_header(90.0, 10.0);
    viewport_alone.push(([40.0, 90.0], 10.0, Some([40.0, 90.0])));
    let forms_over_forms = forms(vec![(
        Level::Preferred,
        forms(vec![(Level::Preferred, viewport(40.0))]),
    )])
    .grow_height(1.0);
    assert_eq!(grown(forms_over_forms), (viewport_alone, true));
}

// A composition does not pass it on: the form selected, a column, is
// raised to the 90, but plans its viewport by the viewport's own growth,
// which is none, so the viewport keeps its preferred 60.
#[test]
fn growth_in_height_does_not_pass_into_a_compositions_children() {
    let mut expected = column_and_header(90.0, 10.0);
    expected.push(([40.0, 90.0], 10.0, None));
    expected.push(([40.0, 60.0], 10.0, Some([40.0, 60.0])));
    let family = forms(vec![(
        Level::Preferred,
        column_child(vec![viewport(40.0)], 0.0),
    )])
    .grow_height(1.0);
    assert_eq!(grown(family), (expected, true));
}
