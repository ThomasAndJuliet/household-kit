// SPDX-License-Identifier: GPL-3.0-only

//! Placing a hover card: informational attachments prefer obstacle-free
//! regions before covering menus.
//!
//! # How a hover card is placed
//!
//! A hover card (what a control explains, shown while the pointer is on
//! it) is an attached surface like a popup ([`crate::attach`]), placed
//! with the same offer and origin, but by one rule of its own and with
//! somewhere it would rather not go.
//!
//! **One rule for every card.** Whatever side, alignment and gap its source
//! declared, a card is placed flush with the control (no gap), centred on
//! it along the declared side, and shifted only as far as it must be to
//! stay inside the window ([`Alignment::CenterBounded`]). The declared side
//! is kept: each declared candidate is tried with that side alone
//! (`single_attachment`).
//!
//! **What counts as an obstacle.** The host gives the surfaces already open
//! (a menu, its submenus) as obstacles. A surface that holds the source,
//! as a menu holds its row, is an obstacle only beyond the source's own
//! edge on the card's side: the margin between the row and the menu's edge
//! is where a flush card sits, so it does not count (`flush_obstacles`).
//! A card overlaps an obstacle when they share an area; touching along an
//! edge does not ([`intersects`]).
//!
//! **Clear regions first.** The engine tries the (stage, candidate) pairs
//! in the order the attachment's preference gives, twice. The first time,
//! each pair is offered only the clear regions of the window: rectangles
//! bounded by the window's and the obstacles' edges that overlap no
//! obstacle, not only full strips but also the clear space beside a short
//! panel (`regions`). A pair is planned in each region as a popup is in
//! the window, and a placement that would still overlap an obstacle is
//! dropped. The regions are tried nearest first: ranked by how far the
//! card placed in each lies, across its side, from where it would lie in
//! the whole window, not by the regions' size, so a card stays as near
//! its own place as it can (`ordered_regions`). The second time, only if
//! no pair fitted clear of every obstacle, each pair is offered the whole
//! window and may cover the menus: a card is shown over a menu rather than
//! not at all.
//!
//! [`choose_hover`] chooses; [`place_hover`] is the same search with the
//! host trying each region, so that a host can commit the card in a region
//! and go on to the next when what it drew disagrees with the plan.

use crate::attach::{
    Alignment, AnchorCandidate, AttachedPlan, Attachment, PlacementPreference, Side,
    choose_attached,
};
use crate::stage::{AsStage, NoFit};
use crate::{Host, Rectangle};
use std::cmp::Ordering;

/// Query the declared candidate order against unobstructed regions first,
/// then against the whole window. Popup rectangles are finite obstacles.
pub fn choose_hover<'a, H: Host, S: AsStage<'a, H>>(
    stages: &[S],
    context: &H::Context,
    window: Rectangle,
    attachment: &Attachment,
    obstacles: &[Rectangle],
) -> Result<AttachedPlan, NoFit> {
    let HoverChoice {
        value: mut plan,
        stage_index,
        candidate_index,
    } = place_hover(
        stages,
        context,
        window,
        attachment,
        obstacles,
        |_, stage, region, a| choose_attached(stage, context, region, a).ok(),
        |plan| plan.bounds,
    )?;
    plan.stage.stage_index = stage_index;
    plan.candidate_index = candidate_index;
    Ok(plan)
}

/// What a host's attempt in a region made, and the stage and the declared
/// candidate it was made for, by their indices in what the host declared.
pub struct HoverChoice<T> {
    pub value: T,
    pub stage_index: usize,
    pub candidate_index: usize,
}

/// The search of [`choose_hover`], with the host trying each region.
///
/// For each pair in turn, the clear pass and then the whole window, the
/// engine asks `attempt` for each region it offers: the stage's index among
/// `stages`, that stage alone, the region, and the card's own attachment
/// (one candidate: the declared side, `CenterBounded`, no gap). The attempt
/// places the stage in that region as `choose_attached` does, and may do
/// more (household-widgets commits the card there and checks it). The
/// first attempt that gives something, whose bounds (`bounds`) overlap no
/// obstacle in the clear pass, is the choice. `NoFit` counts every stage
/// with every declared candidate.
pub fn place_hover<'a, H: Host, S: AsStage<'a, H>, T>(
    stages: &[S],
    context: &H::Context,
    window: Rectangle,
    attachment: &Attachment,
    obstacles: &[Rectangle],
    mut attempt: impl FnMut(usize, &[S], Rectangle, &Attachment) -> Option<T>,
    bounds: impl Fn(&T) -> Rectangle,
) -> Result<HoverChoice<T>, NoFit> {
    for fallback in [false, true] {
        for ((stage_index, stage), (candidate_index, declared)) in pairs(stages, attachment) {
            let a = single_attachment(attachment, declared);
            let obstacles = &flush_obstacles(obstacles, &a, declared.side);
            let stage = std::slice::from_ref(stage);
            let offers = if fallback {
                vec![window]
            } else {
                ordered_regions(
                    stage,
                    context,
                    window,
                    &a,
                    declared.side,
                    &regions(window, obstacles),
                    obstacles,
                )
            };
            for region in offers {
                if let Some(value) = attempt(stage_index, stage, region, &a)
                    && (fallback || !obstacles.iter().any(|r| intersects(*r, bounds(&value))))
                {
                    return Ok(HoverChoice {
                        value,
                        stage_index,
                        candidate_index,
                    });
                }
            }
        }
    }
    Err(NoFit {
        attempted: stages.len() * attachment.candidates.len(),
    })
}

// Rank actual fitting geometries, rather than the size of their containing
// rectangles. This keeps a bounded alignment near its window position even
// when a larger clear strip would move it away from the source.
fn ordered_regions<'a, H: Host, S: AsStage<'a, H>>(
    stages: &[S],
    context: &H::Context,
    window: Rectangle,
    attachment: &Attachment,
    side: Side,
    regions: &[Rectangle],
    obstacles: &[Rectangle],
) -> Vec<Rectangle> {
    let vertical = matches!(side, Side::Above | Side::Below);
    let cross = |bounds: Rectangle| if vertical { bounds.x } else { bounds.y };
    let reference = choose_attached(stages, context, window, attachment)
        .map(|plan| cross(plan.bounds))
        .unwrap_or_else(|_| cross(attachment.anchor));
    let mut offers: Vec<_> = regions
        .iter()
        .filter_map(|&region| {
            let plan = choose_attached(stages, context, region, attachment).ok()?;
            if obstacles
                .iter()
                .any(|obstacle| intersects(*obstacle, plan.bounds))
            {
                return None;
            }
            Some((region, (cross(plan.bounds) - reference).abs()))
        })
        .collect();
    offers.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
    offers.into_iter().map(|(region, _)| region).collect()
}

/// A stage and a declared candidate, each with its index.
type Pair<'s, S> = ((usize, &'s S), (usize, AnchorCandidate));

/// Every stage with every declared candidate, in the order the attachment's
/// preference gives.
fn pairs<'s, S>(stages: &'s [S], attachment: &Attachment) -> Vec<Pair<'s, S>> {
    let mut pairs = Vec::new();
    match attachment.preference {
        PlacementPreference::RepresentationFirst => {
            for stage in stages.iter().enumerate() {
                for (index, &candidate) in attachment.candidates.iter().enumerate() {
                    pairs.push((stage, (index, candidate)));
                }
            }
        }
        PlacementPreference::AnchorFirst => {
            for (index, &candidate) in attachment.candidates.iter().enumerate() {
                for stage in stages.iter().enumerate() {
                    pairs.push((stage, (index, candidate)));
                }
            }
        }
    }
    pairs
}
/// Every information card is placed by one rule, whatever its source declared:
/// flush with the control (no gap), centred on it along the attached side,
/// and shifted only as far as needed to stay inside the window.
fn single_attachment(attachment: &Attachment, declared: AnchorCandidate) -> Attachment {
    let placed = AnchorCandidate::new(declared.side, Alignment::CenterBounded);
    let mut a = Attachment::new(attachment.anchor, vec![placed]);
    a.surface_insets = attachment.surface_insets;
    if attachment.fill_width {
        a.fill_width()
    } else {
        a
    }
}

/// A surface holding the source (a menu round its row) is an obstacle to a
/// card only beyond the source's own edge on the card's side: the margin
/// between the control and its surface's edge is where a flush card sits.
/// `side` is the side of the source the card is on.
fn flush_obstacles(obstacles: &[Rectangle], attachment: &Attachment, side: Side) -> Vec<Rectangle> {
    let anchor = attachment.anchor;
    let contains = |o: &Rectangle| {
        o.x <= anchor.x
            && o.y <= anchor.y
            && o.x + o.width >= anchor.x + anchor.width
            && o.y + o.height >= anchor.y + anchor.height
    };
    obstacles
        .iter()
        .map(|&o| {
            if !contains(&o) {
                return o;
            }
            let mut o = o;
            match side {
                Side::Left => {
                    o.width -= anchor.x - o.x;
                    o.x = anchor.x;
                }
                Side::Right => o.width = anchor.x + anchor.width - o.x,
                Side::Above => {
                    o.height -= anchor.y - o.y;
                    o.y = anchor.y;
                }
                Side::Below => o.height = anchor.y + anchor.height - o.y,
            }
            o
        })
        .collect()
}

/// Whether `a` and `b` overlap with an area: touching along an edge is not
/// overlapping.
pub fn intersects(a: Rectangle, b: Rectangle) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

// Empty rectangles are bounded by window or obstacle edges. Enumerating
// edge pairs also admits clear space beside a short panel, not just full strips.
fn regions(window: Rectangle, obstacles: &[Rectangle]) -> Vec<Rectangle> {
    if !window.x.is_finite()
        || !window.y.is_finite()
        || !window.width.is_finite()
        || !window.height.is_finite()
        || window.width <= 0.0
        || window.height <= 0.0
    {
        return Vec::new();
    }
    let mut xs = vec![window.x, window.x + window.width];
    let mut ys = vec![window.y, window.y + window.height];
    for o in obstacles.iter().filter_map(|o| o.intersection(&window)) {
        xs.extend([o.x, o.x + o.width]);
        ys.extend([o.y, o.y + o.height]);
    }
    for axis in [&mut xs, &mut ys] {
        axis.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        axis.dedup();
    }
    let mut result = Vec::new();
    let vertical = spans(&ys);
    for (x, right) in spans(&xs) {
        for &(y, bottom) in &vertical {
            let r = Rectangle {
                x,
                y,
                width: right - x,
                height: bottom - y,
            };
            if !obstacles.iter().any(|o| intersects(*o, r)) {
                result.push(r);
            }
        }
    }
    result.sort_by(|a, b| {
        (b.width * b.height)
            .partial_cmp(&(a.width * a.height))
            .unwrap_or(Ordering::Equal)
    });
    result
}

/// Every edge paired with each later edge on one axis, in order: the spans a
/// clear rectangle can take on that axis.
fn spans(edges: &[f32]) -> Vec<(f32, f32)> {
    let mut spans = Vec::new();
    let mut rest = edges;
    while let Some((&start, later)) = rest.split_first() {
        spans.extend(later.iter().map(|&end| (start, end)));
        rest = later;
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Point, Size};

    fn r(x: f32, y: f32, width: f32, height: f32) -> Rectangle {
        Rectangle::new(Point::new(x, y), Size::new(width, height))
    }

    #[test]
    fn the_clear_regions_round_two_obstacles_are_bounded_by_their_edges() {
        // Two panels on a diagonal leave the other two quarters clear, and
        // nothing larger: every rectangle reaching further overlaps one.
        let window = r(0.0, 0.0, 100.0, 100.0);
        let obstacles = [r(0.0, 0.0, 50.0, 50.0), r(50.0, 50.0, 50.0, 50.0)];
        assert_eq!(
            regions(window, &obstacles),
            [r(0.0, 50.0, 50.0, 50.0), r(50.0, 0.0, 50.0, 50.0)]
        );
        // A short panel leaves clear space beside it as well as below it,
        // largest first.
        let short = [r(0.0, 0.0, 60.0, 40.0)];
        assert_eq!(
            regions(window, &short),
            [
                r(0.0, 40.0, 100.0, 60.0),
                r(60.0, 0.0, 40.0, 100.0),
                r(0.0, 40.0, 60.0, 60.0),
                r(60.0, 40.0, 40.0, 60.0),
                r(60.0, 0.0, 40.0, 40.0),
            ]
        );
    }

    #[test]
    fn a_surface_holding_the_source_is_an_obstacle_only_beyond_the_source() {
        // A menu round its row: to the left of the row, the menu counts
        // only from the row's left edge on, so a card flush with the row
        // may sit in the menu's margin.
        let menu = r(272.0, 415.0, 240.0, 94.0);
        let row = Attachment::new(r(276.0, 447.0, 232.0, 28.0), Vec::new());
        assert_eq!(
            flush_obstacles(&[menu], &row, Side::Left),
            [r(276.0, 415.0, 236.0, 94.0)]
        );
        assert_eq!(
            flush_obstacles(&[menu], &row, Side::Below),
            [r(272.0, 415.0, 240.0, 60.0)]
        );
        // A surface that does not hold the source counts whole.
        let other = r(0.0, 0.0, 100.0, 100.0);
        assert_eq!(flush_obstacles(&[other], &row, Side::Left), [other]);
    }
}
