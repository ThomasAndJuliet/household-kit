// SPDX-License-Identifier: GPL-3.0-only

//! A child given a number it cannot use is an invalid child
//! (`Child::invalid_unless`), never a panic.
//!
//! For every builder that takes a number, each bad value (NaN, an infinity, a
//! negative number) builds a child, and every query and plan that includes
//! it fails with `InvalidInput`: alone, in a row, a top-aligned row, a
//! column, a wrap, a frame, the first and a later form, behind `fill_width`
//! and `fixed_dimensions`, nested, and behind a frame or forms whose own
//! measurement was replaced, in bounded and in unbounded room. A stage that
//! holds one does not fit, so the next stage is taken, or the fallback. Where
//! an invalid child is hidden, it takes no part and the rest lays out.
//!
//! The engine's own "this must be so here" holds by construction: forms
//! always have a form to select, a wrap packs into the row it is filling,
//! a leaf given a plan made for another kind of child refuses the stage,
//! and the responsive widget given a layout without its child does nothing.

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

const BAD: [f32; 4] = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0];

fn ok() -> Child<'static, ()> {
    child(|_| widget::Space::new().width(40).height(20).into())
}

fn rooms() -> [Size; 2] {
    [
        Size::new(300.0, 300.0),
        Size::new(f32::INFINITY, f32::INFINITY),
    ]
}

/// The measurement a replaced child reports in place of its kind's.
fn valid_measurement(_: Level, _: &Renderer) -> MeasuredSize {
    MeasuredSize {
        preferred: Size::new(40.0, 20.0),
        minimum: Size::new(40.0, 20.0),
    }
}

/// Every way a parent can include `bad`, each named for the failure message.
fn every_path(bad: &Child<'static, ()>) -> Vec<(&'static str, Child<'static, ()>)> {
    vec![
        ("alone", bad.clone()),
        ("row", row_child(vec![ok(), bad.clone()], 4.0)),
        (
            "top-aligned row",
            row_child_top(vec![ok(), bad.clone()], 4.0),
        ),
        ("column", column_child(vec![ok(), bad.clone()], 4.0)),
        ("wrap", wrap_child(vec![ok(), bad.clone(), ok()], 4.0)),
        ("frame", framed_child(bad.clone(), 2.0, |content| content)),
        ("first form", forms(vec![(Level::Preferred, bad.clone())])),
        (
            "later form",
            forms(vec![(Level::Preferred, ok()), (Level::Mild, bad.clone())]),
        ),
        ("fill_width", bad.clone().fill_width()),
        (
            "fixed_dimensions",
            bad.clone().fixed_dimensions(Some(100.0), Some(100.0)),
        ),
        (
            "nested",
            column_child(
                vec![ok(), row_child(vec![ok(), bad.clone().weight(0.0)], 0.0)],
                0.0,
            ),
        ),
        (
            "frame with a replaced measurement",
            framed_child(bad.clone(), 0.0, |content| content).with_measurement(valid_measurement),
        ),
        (
            "forms with a replaced measurement",
            forms(vec![(Level::Preferred, bad.clone())]).with_measurement(valid_measurement),
        ),
    ]
}

#[track_caller]
fn assert_invalid_everywhere(bad: &Child<'static, ()>, what: &str) {
    let renderer = renderer();
    assert_eq!(
        validate_measurement(bad.query(Level::Mild, &renderer)),
        Err(AllocationError::InvalidInput),
        "{what}: its measurement"
    );
    for (path, parent) in every_path(bad) {
        for room in rooms() {
            assert_eq!(
                parent.query_at(Level::Mild, &renderer, room),
                Err(AllocationError::InvalidInput),
                "{what}, {path}, in {room:?}"
            );
        }
    }
}

/// Every builder that takes a number, given `value`.
fn builders(value: f32) -> Vec<(String, Child<'static, ()>)> {
    let viewport =
        |_: Level| cosmic::iced::widget::scrollable(widget::Space::new().width(80).height(60));
    vec![
        (format!("weight({value})"), ok().weight(value)),
        (format!("grow({value})"), ok().grow(value)),
        (format!("grow_height({value})"), ok().grow_height(value)),
        (
            format!("fixed width {value}"),
            ok().fixed_dimensions(Some(value), None),
        ),
        (
            format!("fixed height {value}"),
            ok().fixed_dimensions(None, Some(value)),
        ),
        (format!("spacer({value})"), spacer(value)),
        (
            format!("scroll_child width {value}"),
            scroll_child(move |level| viewport(level).into(), Size::new(value, 10.0)),
        ),
        (
            format!("scroll_child height {value}"),
            scroll_child(move |level| viewport(level).into(), Size::new(80.0, value)),
        ),
        (
            format!("row_child spacing {value}"),
            row_child(vec![ok(), ok()], value),
        ),
        (
            format!("row_child_top spacing {value}"),
            row_child_top(vec![ok(), ok()], value),
        ),
        (
            format!("column_child spacing {value}"),
            column_child(vec![ok(), ok()], value),
        ),
        (
            format!("wrap_child spacing {value}"),
            wrap_child(vec![ok(), ok()], value),
        ),
        (
            format!("framed_child padding {value}"),
            framed_child(ok(), value, |content| content),
        ),
        (
            format!("framed_child top padding {value}"),
            framed_child(
                ok(),
                cosmic::iced::Padding {
                    top: value,
                    ..cosmic::iced::Padding::ZERO
                },
                |content| content,
            ),
        ),
    ]
}

#[test]
fn a_bad_number_given_to_any_builder_makes_an_invalid_child_on_every_path() {
    for value in BAD {
        for (what, bad) in builders(value) {
            assert_invalid_everywhere(&bad, &what);
        }
    }
}

type Form = (Level, Child<'static, ()>);

#[test]
fn forms_out_of_order_are_an_invalid_child_on_every_path() {
    let cases: Vec<(&str, Vec<Form>)> = vec![
        ("no form", vec![]),
        ("first not Preferred", vec![(Level::Mild, ok())]),
        (
            "a repeated threshold",
            vec![(Level::Preferred, ok()), (Level::Preferred, ok())],
        ),
        (
            "decreasing thresholds",
            vec![
                (Level::Preferred, ok()),
                (Level::Strong, ok()),
                (Level::Mild, ok()),
            ],
        ),
    ];
    for (what, alternatives) in cases {
        assert_invalid_everywhere(&forms(alternatives), what);
    }
}

#[test]
fn a_bad_number_is_kept_and_a_later_good_one_does_not_clear_the_mark() {
    let renderer = renderer();
    let bad = ok().weight(f32::NAN);
    assert!(
        bad.weight.is_nan(),
        "the number given is kept, not replaced"
    );
    let later = bad.weight(1.0).hidden_when(|_| false).round_width_up();
    for room in rooms() {
        assert_eq!(
            later.query_at(Level::Preferred, &renderer, room),
            Err(AllocationError::InvalidInput)
        );
    }
}

#[test]
fn a_hidden_invalid_child_takes_no_part_and_the_rest_lays_out() {
    let renderer = renderer();
    for value in BAD {
        for (what, bad) in builders(value) {
            let hidden = bad.clone().hidden_when(|level| level == Level::Preferred);
            for room in rooms() {
                let alone = hidden.query_at(Level::Preferred, &renderer, room);
                assert!(alone.is_ok_and(|plan| plan.hidden), "{what} alone");
                for parent in [
                    row_child(vec![ok(), hidden.clone()], 4.0),
                    column_child(vec![ok(), hidden.clone()], 4.0),
                    wrap_child(vec![ok(), hidden.clone()], 4.0),
                ] {
                    let plan = parent.query_at(Level::Preferred, &renderer, room);
                    assert_eq!(
                        plan.map(|plan| (plan.size, plan.children.len())),
                        Ok((Size::new(40.0, 20.0), 1)),
                        "{what} beside a valid child, in {room:?}"
                    );
                }
                // At a level where it is not hidden, it is invalid again.
                assert_eq!(
                    hidden.query_at(Level::Mild, &renderer, room),
                    Err(AllocationError::InvalidInput),
                    "{what} shown"
                );
            }
        }
    }
}

#[test]
fn a_valid_form_lays_out_while_an_invalid_child_waits_in_a_later_form() {
    let renderer = renderer();
    let family = forms(vec![
        (Level::Preferred, ok()),
        (
            Level::Mild,
            row_child(vec![ok(), ok().grow(f32::INFINITY)], 0.0),
        ),
    ]);
    for room in rooms() {
        assert_eq!(
            family
                .query_at(Level::Preferred, &renderer, room)
                .map(|plan| plan.size),
            Ok(Size::new(40.0, 20.0))
        );
        assert_eq!(
            family.query_at(Level::Mild, &renderer, room),
            Err(AllocationError::InvalidInput)
        );
    }
}

#[test]
fn a_stage_holding_an_invalid_child_gives_way_to_the_next_then_to_the_fallback() {
    let renderer = renderer();
    let room = Size::new(300.0, 300.0);
    let bad = || row_child(vec![ok(), ok().weight(-1.0)], 0.0);
    let declared = || {
        vec![
            Stage::new(Level::Preferred, bad()),
            Stage::new(Level::Mild, ok()),
        ]
    };
    assert_eq!(
        choose_stage(&declared(), &renderer, room).map(|plan| plan.stage_index),
        Ok(1)
    );
    assert_eq!(
        choose_stage(&[Stage::new(Level::Preferred, bad())], &renderer, room),
        Err(NoFit { attempted: 1 })
    );

    let mut next = stages(declared());
    let mut tree = Tree::empty();
    next.layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, room));
    assert_eq!(
        next.selection()
            .map(|selected| selected.as_ref().map(|plan| plan.stage_index)),
        Some(Ok(1))
    );

    let mut fallback = stages(vec![Stage::new(Level::Preferred, bad())])
        .fallback(|| widget::Space::new().width(7).height(5).into());
    let mut tree = Tree::empty();
    let node = fallback.layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, room));
    assert_eq!(fallback.selection(), Some(&Err(NoFit { attempted: 1 })));
    assert_eq!(
        node.children().first().map(layout::Node::size),
        Some(Size::new(7.0, 5.0)),
        "the fallback is shown"
    );
}

#[test]
fn a_presentation_family_counts_a_missing_default_as_supported() {
    let normal = Presentation::default();
    let emphasised = Presentation {
        emphasis: Emphasis::Emphasised,
        detail: Detail::Concise,
    };
    let family = PresentationFamily::new(normal, vec![emphasised]);
    assert_eq!(family.supported(), &[emphasised, normal]);
    assert_eq!(family.default_presentation(), normal);
    assert_eq!(
        family.resolve(
            PresentationRequest::default(),
            PresentationRequest::default()
        ),
        Ok(normal)
    );
    let listed = PresentationFamily::new(normal, vec![emphasised, normal]);
    assert_eq!(listed.supported(), &[emphasised, normal], "nothing added");
}

#[test]
fn forms_with_one_form_select_it_at_every_level() {
    let renderer = renderer();
    let family = forms(vec![(Level::Preferred, ok())]);
    for level in Level::ALL {
        for room in rooms() {
            assert_eq!(
                family
                    .query_at(level, &renderer, room)
                    .map(|plan| plan.size),
                Ok(Size::new(40.0, 20.0)),
                "{level:?} in {room:?}"
            );
        }
    }
}

/// The children's indices in each row of a wrap of `count` 40 × 20 leaves
/// with spacing 4, planned `width` wide, and the wrap's size.
fn wrapped(count: usize, width: f32) -> Result<(Vec<Vec<usize>>, Size), AllocationError> {
    let wrap = wrap_child((0..count).map(|_| ok()).collect(), 4.0);
    let plan = wrap.query_at(Level::Preferred, &renderer(), Size::new(width, 300.0))?;
    let rows = plan
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .filter_map(|i| plan.children.get(*i).map(|placed| placed.index))
                .collect()
        })
        .collect();
    Ok((rows, plan.size))
}

#[test]
fn a_wrap_packs_none_one_and_one_child_per_row() {
    let renderer = renderer();
    // With no children a wrap is hidden: it takes no part.
    let empty: Child<'_, ()> = wrap_child(Vec::new(), 4.0);
    for room in rooms() {
        assert!(
            empty
                .query_at(Level::Preferred, &renderer, room)
                .is_ok_and(|plan| plan.hidden)
        );
    }
    assert_eq!(
        wrapped(1, 300.0),
        Ok((vec![vec![0]], Size::new(40.0, 20.0)))
    );
    assert_eq!(
        wrapped(3, 300.0),
        Ok((vec![vec![0, 1, 2]], Size::new(128.0, 20.0)))
    );
    // 40 + 4 + 40 does not fit in 50: each child needs a row of its own.
    assert_eq!(
        wrapped(3, 50.0),
        Ok((vec![vec![0], vec![1], vec![2]], Size::new(40.0, 68.0)))
    );
    assert_eq!(
        wrapped(3, 84.0),
        Ok((vec![vec![0, 1], vec![2]], Size::new(84.0, 44.0)))
    );
}

#[test]
fn a_leaf_given_a_plan_without_its_native_size_renders_nothing_and_refuses_the_stage() {
    let renderer = renderer();
    let room = layout::Limits::new(Size::ZERO, Size::new(300.0, 300.0));
    let leaf = ok();
    let commit = |plan: &GeometryPlan| {
        let agreement = Rc::new(Cell::new(true));
        let mut content = leaf.render(Level::Preferred, plan, agreement.clone());
        let mut tree = Tree::new(content.as_widget());
        let node = content.as_widget_mut().layout(&mut tree, &renderer, &room);
        (agreement.get(), node.size())
    };
    let own = leaf
        .query_at(Level::Preferred, &renderer, Size::new(300.0, 300.0))
        .expect("the leaf's own plan");
    assert_eq!(commit(&own), (true, Size::new(40.0, 20.0)));
    // Bound to this leaf, but made as a composition's plan is: no native size.
    let foreign = leaf.bind_geometry(Level::Preferred, GeometryPlan::leaf(Size::new(40.0, 20.0)));
    assert_eq!(commit(&foreign), (false, Size::ZERO));
}

#[test]
fn the_responsive_widget_given_a_layout_without_its_child_does_nothing() {
    let renderer = renderer();
    let mut widget = stages(vec![Stage::new(Level::Preferred, ok())]);
    let mut tree = Tree::empty();
    let room = Size::new(300.0, 300.0);
    widget.layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, room));
    let childless = layout::Node::new(room);
    let viewport = Rectangle::with_size(room);
    assert_eq!(
        widget.mouse_interaction(
            &tree,
            Layout::new(&childless),
            mouse::Cursor::Unavailable,
            &viewport,
            &renderer,
        ),
        mouse::Interaction::None
    );
    assert!(
        widget
            .overlay(
                &mut tree,
                Layout::new(&childless),
                &renderer,
                &viewport,
                Vector::ZERO,
            )
            .is_none()
    );
}
