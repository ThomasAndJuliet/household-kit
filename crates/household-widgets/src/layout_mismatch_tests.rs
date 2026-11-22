// SPDX-License-Identifier: GPL-3.0-only

//! A plan that does not match the child rendering it is refused, never a
//! panic.
//!
//! A plan names the child's parts by position: a composition's placements
//! name its declared children, its rows name its placements, and a frame's
//! one placement holds its content. Rendering reads each through checked
//! access, and a part that names nothing renders nothing and refuses the
//! stage (`refused`), so the next declared stage, or the fallback, is taken.
//! Here a plan is given to a composition with fewer children than it has
//! placements, a plan's rows name placements it no longer has (made with
//! the plan's public fields alone), a frame is given a plan without its
//! placement, a verification wrapper is laid out without its content's
//! tree, and `AttachedPlan::commit` is handed a plan prepared from another
//! child and another list of stages.

use super::*;
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::{Font, Pixels, Point};

fn renderer() -> Renderer {
    cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
        Font::default(),
        Pixels(14.0),
        Some("tiny-skia"),
    ))
    .expect("headless tiny-skia renderer")
}

fn ok() -> Child<'static, ()> {
    child(|_| widget::Space::new().width(40).height(20).into())
}

const ROOM: Size = Size::new(300.0, 300.0);

/// Plan `child` at `Preferred` in `room`.
fn planned(child: &Child<'static, ()>, room: Size) -> GeometryPlan {
    child
        .query_at(Level::Preferred, &renderer(), room)
        .expect("the child's own plan")
}

/// Render `child` from `plan` and lay it out, as a stage is committed:
/// whether the stage agrees.
fn agrees(child: &Child<'static, ()>, plan: &GeometryPlan) -> bool {
    let agreement = Rc::new(Cell::new(true));
    let mut content = child.render(Level::Preferred, plan, agreement.clone());
    let mut tree = Tree::new(content.as_widget());
    content.as_widget_mut().layout(
        &mut tree,
        &renderer(),
        &layout::Limits::new(Size::ZERO, ROOM),
    );
    agreement.get()
}

/// Three leaves shared by the compositions below, so a composition built
/// from some of them binds the same leaves.
fn parts() -> [Child<'static, ()>; 3] {
    [ok(), ok(), ok()]
}

/// A composition's builder: `row_child`, `column_child` or `wrap_child`.
type Compose = fn(Vec<Child<'static, ()>>, f32) -> Child<'static, ()>;

#[test]
fn a_composition_with_fewer_children_than_the_plan_has_placements_refuses_the_stage() {
    let [x, y, z] = parts();
    // The wrap is planned narrow, so its placements fall in several rows.
    let arrangements: [(Compose, Size); 3] = [
        (row_child, ROOM),
        (column_child, ROOM),
        (wrap_child, Size::new(50.0, 300.0)),
    ];
    for (compose, room) in arrangements {
        let three = compose(vec![x.clone(), y.clone(), z.clone()], 4.0);
        let one = compose(vec![x.clone()], 4.0);
        let own = planned(&one, room);
        assert!(agrees(&one, &own), "its own plan agrees");
        // The three-child plan, bound to the one-child composition: its
        // first placement names a child it has, the others do not.
        let mut foreign = planned(&three, room);
        assert_eq!(foreign.children.len(), 3);
        foreign.bindings = own.bindings.clone();
        assert!(!agrees(&one, &foreign));
    }
}

#[test]
fn rows_that_name_placements_the_plan_no_longer_has_refuse_the_stage() {
    // Made with the plan's public fields only: dropping placements leaves
    // the rows (which are private) naming placements that are gone.
    let [x, y, z] = parts();
    for (composition, room) in [
        (row_child(vec![x.clone(), y.clone()], 4.0), ROOM),
        (wrap_child(vec![x, y, z], 4.0), Size::new(50.0, 300.0)),
    ] {
        let mut plan = planned(&composition, room);
        assert!(agrees(&composition, &plan), "its own plan agrees");
        plan.children.truncate(1);
        assert!(!agrees(&composition, &plan));
    }
}

#[test]
fn a_frame_given_a_plan_without_its_placement_refuses_the_stage() {
    let frame = framed_child(ok(), 4.0, |content| {
        widget::container(content).padding(4.0).into()
    });
    let mut plan = planned(&frame, ROOM);
    assert!(agrees(&frame, &plan), "its own plan agrees");
    plan.children.clear();
    assert!(!agrees(&frame, &plan));
}

#[test]
fn a_verification_wrapper_laid_out_without_its_contents_tree_refuses_the_stage() {
    let leaf = ok();
    let plan = planned(&leaf, ROOM);
    let agreement = Rc::new(Cell::new(true));
    let mut content = leaf.render(Level::Preferred, &plan, agreement.clone());
    // A tree with no children: the wrapper has no tree for its content.
    let node = content.as_widget_mut().layout(
        &mut Tree::empty(),
        &renderer(),
        &layout::Limits::new(Size::ZERO, ROOM),
    );
    assert_eq!(node.size(), Size::ZERO, "nothing is laid out");
    assert!(!agreement.get());
}

#[test]
fn commit_refuses_a_plan_prepared_from_another_child_and_another_list_of_stages() {
    let renderer = renderer();
    let [x, y, z] = parts();
    let window = Rectangle::with_size(ROOM);
    let attachment = Attachment::below(Rectangle::new(Point::ORIGIN, Size::new(10.0, 10.0)));
    let three = vec![Stage::new(
        Level::Preferred,
        row_child(vec![x.clone(), y, z], 4.0),
    )];
    let one = vec![Stage::new(Level::Preferred, row_child(vec![x], 4.0))];
    let commit = |plan: &AttachedPlan, stages: &[Stage<'static, ()>]| {
        let mut content = widget::space().into();
        plan.commit(stages, &renderer, &mut content, &mut Tree::empty())
            .map(|node| node.size())
    };
    let from_three =
        choose_attached(&three, &renderer, window, &attachment).expect("the three-child row fits");
    let from_one =
        choose_attached(&one, &renderer, window, &attachment).expect("the one-child row fits");
    assert_eq!(commit(&from_three, &three), Ok(from_three.bounds.size()));
    assert_eq!(commit(&from_one, &one), Ok(from_one.bounds.size()));

    let refused = Err(NoFit { attempted: 1 });
    // Prepared from another child, in another list of stages.
    assert_eq!(commit(&from_three, &one), refused);
    assert_eq!(commit(&from_one, &three), refused);
    // The same, bound to the stage's child, so its placements are read.
    let mut rebound = from_three.clone();
    rebound.stage.geometry.bindings = from_one.stage.geometry.bindings.clone();
    assert_eq!(commit(&rebound, &one), refused);
    // The stage's own plan with its placements removed through the plan's
    // public fields: its row names placements that are gone.
    let mut emptied = from_one.clone();
    emptied.stage.geometry.children.clear();
    assert_eq!(commit(&emptied, &one), refused);
}
