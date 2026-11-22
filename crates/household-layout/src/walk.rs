// SPDX-License-Identifier: GPL-3.0-only

//! A plan read together with the parts that made it: one walk that visits
//! every node of a plan with the part that made it, where it is, and its
//! path ([`Visit`], `Child::walk`).
//!
//! A plan holds nothing of the host's ([`Plan`]): it says where each node
//! goes, not which part made it. A host that draws a plan, binds it to its
//! own bookkeeping or looks a node up by what it keeps in a part's note
//! needs both at once, and gets them by walking the part and its plan
//! together, by the rule planning made the plan by:
//!
//! - a composition's placement was made by the child at its index among the
//!   composition's declared children ([`Placed::index`]);
//! - a frame's one placement, at 0, was made by its content;
//! - a family's node was made by the form it selects at the level, in the
//!   family's place, so the family and its form are visited on the same
//!   node, at the same position and with the same path, the family first;
//! - a leaf has nothing below it.
//!
//! A hidden node was returned at once by the hidden part asked for it, so
//! it is visited with that part and nothing below it is: not a hidden
//! family's form, not a hidden composition's children. A placement that
//! names no part of the tree (an index past a composition's children, a
//! frame's placement at any index but 0) cannot have come from the part's
//! own planning; it is skipped with everything below it.
//!
//! This is the rule household-widgets' `Commit::bind` and `render` follow
//! over the widgets' own plan; here it is written once, over the core's.

use crate::part::{Child, Kind};
use crate::plan::{Placed, Plan};
use crate::{Host, Level, Point};

/// One node of a plan, as a walk visits it (`Child::walk`): the part that
/// made it, and where the node is.
///
/// Everything here is borrowed or copied for the length of one visit: the
/// path is the walk's own, as far as this node. A host that wants to keep
/// a path copies it (`path.to_vec()`).
pub struct Visit<'v, 'a, H: Host> {
    /// The part that made the node: a composition's child, a frame's
    /// content, a family or the form it selects.
    pub part: &'v Child<'a, H>,
    /// The node's plan.
    pub plan: &'v Plan,
    /// The node's top left corner, from the root's: the positions of the
    /// placements on the way down, added up ([`Placed::position`]).
    pub position: Point,
    /// The node's path from the root, one step per placement, as a
    /// failure's path has it (`Failure::path`); `Child::part_at` at this
    /// path is the part placed here (the family, where a family and its
    /// form share the node).
    pub path: &'v [usize],
    /// The growth in height in effect for the part, as planning had it
    /// (`Child::grow_height_in_effect`): its own, or the growth the forms or
    /// the frame round it passed it. A host that lays the node out again
    /// raises its height by it as planning did.
    pub grow_height: f32,
}

impl<'a, H: Host> Child<'a, H> {
    /// Visit every node of `plan`, which this part made at `level`, with the
    /// part that made it (`Visit`): a part before the parts it holds, in the
    /// order of the plan's placements; a family before its form, on the same
    /// node; nothing below a hidden node; and nothing of a placement that
    /// names no part of this tree (the module's documentation gives the
    /// rule).
    ///
    /// A callback rather than an iterator, because of the path. The walk
    /// keeps one path, a step pushed on the way into a placement and popped
    /// on the way out, and gives each visit a slice of it. An iterator's
    /// items cannot borrow from the iterator, so each item would need a
    /// path of its own, one allocation per node; and the iterator would
    /// keep its own stack of the nodes still to visit, which a recursive
    /// walk keeps on the call stack. So the walk allocates only the path,
    /// once it is a step deep, and grows it only as deep as the plan goes.
    /// A host that wants the visits as a list collects them in the
    /// callback.
    pub fn walk(&self, level: Level, plan: &Plan, mut visit: impl FnMut(Visit<'_, 'a, H>)) {
        let mut path = Vec::new();
        self.walk_node(
            level,
            plan,
            Point::new(0.0, 0.0),
            0.0,
            &mut path,
            &mut visit,
        );
    }

    /// Visit this part's node, at `position` and `path`, with the growth in
    /// height the forms or the frame round it pass it, and then what it
    /// holds.
    fn walk_node<F: FnMut(Visit<'_, 'a, H>)>(
        &self,
        level: Level,
        plan: &Plan,
        position: Point,
        inherited_growth: f32,
        path: &mut Vec<usize>,
        visit: &mut F,
    ) {
        let grow_height = self.grow_height_in_effect(inherited_growth);
        visit(Visit {
            part: self,
            plan,
            position,
            path: path.as_slice(),
            grow_height,
        });
        if plan.hidden {
            return;
        }
        match &self.kind {
            Kind::Leaf(_) => {}
            // The form made this same node, in the family's place, with the
            // family's growth in height.
            Kind::Forms(forms) => {
                forms
                    .select(level)
                    .walk_node(level, plan, position, grow_height, path, visit);
            }
            // A composition plans its children as children, each with its
            // own growth in height.
            Kind::Composition(composition) => {
                for placed in &plan.children {
                    if let Some(child) = composition.children.get(placed.index) {
                        child.walk_placed(level, placed, position, 0.0, path, visit);
                    }
                }
            }
            // A frame passes its growth in height to its content.
            Kind::Frame(frame) => {
                for placed in plan.children.iter().filter(|placed| placed.index == 0) {
                    frame
                        .content
                        .walk_placed(level, placed, position, grow_height, path, visit);
                }
            }
        }
    }

    /// Visit this part's node where `placed` puts it in its parent's, whose
    /// top left corner is at `origin`: one step further down.
    fn walk_placed<F: FnMut(Visit<'_, 'a, H>)>(
        &self,
        level: Level,
        placed: &Placed,
        origin: Point,
        inherited_growth: f32,
        path: &mut Vec<usize>,
        visit: &mut F,
    ) {
        let position = Point::new(origin.x + placed.position.x, origin.y + placed.position.y);
        path.push(placed.index);
        self.walk_node(level, &placed.plan, position, inherited_growth, path, visit);
        path.pop();
    }
}
