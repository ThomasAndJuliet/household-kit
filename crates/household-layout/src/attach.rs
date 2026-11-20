// SPDX-License-Identifier: GPL-3.0-only

//! Placing an attached surface: a popup panel, a submenu or a card, held
//! against an anchor in a window. Choosing where it goes is geometry over
//! stages and rectangles, and it is here; committing the choice (drawing
//! it, checking the drawn surface agrees with the plan) is the host's.
//!
//! # How placement works
//!
//! The host declares three things: its stages, in order ([`crate::stage`]);
//! an [`Attachment`]: the anchor (the control the surface belongs to), the
//! candidates, each a side of the anchor with an alignment and a gap
//! ([`AnchorCandidate`]), and a preference between them
//! ([`PlacementPreference`]); and the window the surface must stay in. The
//! engine tries (stage, candidate) pairs in the order the preference gives,
//! and keeps the first whose plan fits ([`choose_attached`]).
//!
//! **A candidate offers a real region before anything is measured.** From
//! the anchor, the side, the alignment and the gap alone, the candidate
//! says how much room the surface has (`offer`): along the side, from the
//! anchor's edge and the gap to the window's edge; across, as far as the
//! alignment lets it reach. The stage's part is planned in that region
//! (no wider than it prefers, unless the attachment fills the width), so a
//! plan that fits is a fit in exactly the room that candidate gives, and
//! a paragraph is as tall as it needs to be at that width.
//!
//! **The offer and the origin come from the same anchor equation.** Where
//! the surface goes (`origin`) is worked out from the same anchor, side,
//! alignment and gap as the room it was offered, so the surface is placed
//! where it was measured for. Its complete bounds are then checked against
//! the window, and against any obstacles.
//!
//! **Nothing is moved back into the window after measuring.** A pair whose
//! bounds do not lie inside the window, or overlap an obstacle, is
//! rejected, and the next pair is tried. There is no clamp and no nudge: a
//! surface that has been moved is no longer where its alignment says, and
//! may no longer have the room its plan was made in. A surface that needs
//! to scroll declares a scrolling part in its stage; nothing here scrolls.
//!
//! **What each alignment means**, across the side (horizontally for a
//! surface above or below the anchor, vertically for one to the right or
//! the left):
//!
//! - [`Alignment::Start`]: the surface's start edge (left, or top) is the
//!   anchor's, and it reaches towards the window's far edge. It is offered
//!   the room from the anchor's start to the window's end.
//! - [`Alignment::End`]: the surface's end edge is the anchor's, and it
//!   reaches back; it is offered the room from the window's start to the
//!   anchor's end.
//! - [`Alignment::Center`]: centred on the anchor; it is offered twice the
//!   room from the anchor's centre to the nearer window edge.
//! - [`Alignment::StartBounded`]: the anchor's start if it fits there, and
//!   otherwise shifted only as far as it must be to stay inside the
//!   window. It is offered the window's whole extent across, and keeps its
//!   side.
//! - [`Alignment::CenterBounded`]: centred on the anchor, shifted only as
//!   far as it must be to stay inside the window; offered the whole extent.
//!
//! **What the two preferences order.** [`PlacementPreference::RepresentationFirst`]
//! tries every candidate for the first stage before the second stage: the
//! fullest representation wins, on whichever declared side it fits.
//! [`PlacementPreference::AnchorFirst`] tries every stage on the first
//! candidate before the second candidate: the declared side and alignment
//! win, with whatever representation fits there.
//!
//! **The visible surface and its paint box.** A surface drawn with a
//! shadow is laid out in a box larger than the surface itself; the
//! attachment's `surface_insets` are the margins between the two. The
//! surface attaches by its visible edges (its shadow may lie over the
//! anchor), and the whole paint box, shadow included, must fit in the
//! window: the anchor is widened by the insets before the offer and the
//! origin are worked out (`paint_anchor`).
//!
//! **Obstacles and pairs already rejected.** A host placing a chain of
//! surfaces (a menu and its submenus) gives the earlier surfaces as
//! obstacles that a later one may not overlap with an area
//! ([`choose_attached_avoiding`]). A host whose committed surface
//! disagreed with its plan gives the pairs it has rejected, which are
//! skipped ([`choose_attached_excluding`]). A chain whose levels may
//! share one surface is scheduled by [`choose_attached_chain`].
//!
//! # A worked example
//!
//! A panel 100 × 50 below an anchor 40 wide near the right edge of a
//! window 300 wide: the anchor runs from 240 to 280. Aligned at the
//! anchor's start, the panel is offered the 60 from 240 to the window's
//! edge, and it cannot be planned narrower than its 100, so the first
//! candidate does not fit; nothing moves it left. Aligned at the anchor's
//! end, it is offered the 280 from the window's start to the anchor's end,
//! takes its 100, and ends where the anchor ends: from 180 to 280, just
//! below the anchor.
//!
//! ```
//! use household_layout::{
//!     Alignment, AnchorCandidate, Attachment, Child, Host, Level, Offer, Point, Rectangle,
//!     Side, Size, Stage, choose_attached,
//! };
//!
//! // A host whose leaves are boxes of a fixed size.
//! struct Boxes;
//!
//! impl Host for Boxes {
//!     type Leaf = Size;
//!     type Context = ();
//!     type Note = ();
//!     type Chrome = ();
//!
//!     fn size(leaf: &Size, _: Level, _: &(), _: Offer) -> Size {
//!         *leaf
//!     }
//! }
//!
//! let panel = Child::<Boxes>::leaf(Size::new(100.0, 50.0));
//! let stages = [Stage::new(Level::Preferred, panel)];
//! let window = Rectangle::with_size(Size::new(300.0, 200.0));
//! let anchor = Rectangle::new(Point::new(240.0, 20.0), Size::new(40.0, 20.0));
//! let attachment = Attachment::new(
//!     anchor,
//!     vec![
//!         AnchorCandidate::new(Side::Below, Alignment::Start),
//!         AnchorCandidate::new(Side::Below, Alignment::End),
//!     ],
//! );
//!
//! let placed = choose_attached(&stages, &(), window, &attachment).unwrap();
//! assert_eq!(placed.candidate_index, 1, "Start does not fit; End does");
//! assert_eq!(
//!     placed.bounds,
//!     Rectangle::new(Point::new(180.0, 40.0), Size::new(100.0, 50.0)),
//! );
//!
//! // With only the first candidate, nothing fits.
//! let start_only = Attachment::new(
//!     anchor,
//!     vec![AnchorCandidate::new(Side::Below, Alignment::Start)],
//! );
//! assert_eq!(
//!     choose_attached(&stages, &(), window, &start_only),
//!     Err(household_layout::NoFit { attempted: 1 }),
//! );
//! ```

use crate::stage::{AsStage, NoFit, StagePlan};
use crate::{Host, Padding, Point, Rectangle, Size, finite_size};

/// The side of the anchor the surface is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Below,
    Above,
    Right,
    Left,
}

/// Where the surface lies across its side, against the anchor (see the
/// module's documentation for the room each offers).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    Start,
    End,
    Center,
    /// Prefer the source's start, bounded by the entire offered region. This
    /// equation participates in measurement and never changes the primary side.
    StartBounded,
    /// Centred on the source along the attached side, shifted only as far as
    /// needed to stay inside the entire offered region. Information cards
    /// use this and no gap: flush with the control.
    CenterBounded,
}

/// One place a surface may go: a side of the anchor, an alignment across
/// it, and a gap between the anchor and the surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorCandidate {
    pub side: Side,
    pub alignment: Alignment,
    pub gap: f32,
}

impl AnchorCandidate {
    pub fn new(side: Side, alignment: Alignment) -> Self {
        Self {
            side,
            alignment,
            gap: 0.0,
        }
    }
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }
}

/// Callers choose which preference is stronger; list and panel policies need
/// not share an ordering. Each stage still forwards one exact shared Level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlacementPreference {
    #[default]
    RepresentationFirst,
    AnchorFirst,
}

/// Where a surface is attached: the anchor, the candidates in the order
/// they are preferred, which is stronger (a stage or a candidate), whether
/// it takes the whole width offered, and the insets between its paint box
/// and its visible surface.
#[derive(Clone, Debug, PartialEq)]
pub struct Attachment {
    pub anchor: Rectangle,
    pub candidates: Vec<AnchorCandidate>,
    pub preference: PlacementPreference,
    /// Expand to the candidate region's width instead of the measured
    /// preferred panel width. Growth inside that width remains the Child's.
    pub fill_width: bool,
    /// Insets between the complete measured paint box and its visible surface.
    /// Attachment uses surface edges; the whole paint box must still fit.
    pub surface_insets: Padding,
}

impl Attachment {
    pub fn new(anchor: Rectangle, candidates: Vec<AnchorCandidate>) -> Self {
        Self {
            anchor,
            candidates,
            preference: PlacementPreference::default(),
            fill_width: false,
            surface_insets: Padding::default(),
        }
    }
    pub fn preference(mut self, preference: PlacementPreference) -> Self {
        self.preference = preference;
        self
    }
    pub fn fill_width(mut self) -> Self {
        self.fill_width = true;
        self
    }
}

/// The placement chosen: the stage and its plan, the index of the
/// candidate, and the surface's bounds (its whole paint box) in the window.
#[derive(Clone, Debug, PartialEq)]
pub struct AttachedPlan {
    pub stage: StagePlan,
    pub candidate_index: usize,
    pub bounds: Rectangle,
}

/// The offer and the origin are derived from the same anchor equation. A
/// candidate is rejected when its complete queried bounds do not fit.
pub fn choose_attached<'a, H: Host, S: AsStage<'a, H>>(
    stages: &[S],
    context: &H::Context,
    window: Rectangle,
    attachment: &Attachment,
) -> Result<AttachedPlan, NoFit> {
    choose_attached_excluding(stages, context, window, attachment, &[], &[])
}

/// Choose as `choose_attached` does, skipping the (stage, candidate) pairs
/// in `rejected` and any pair whose bounds overlap one of `obstacles` with
/// an area. A host retries with the pairs whose commit it rejected.
pub fn choose_attached_excluding<'a, H: Host, S: AsStage<'a, H>>(
    stages: &[S],
    context: &H::Context,
    window: Rectangle,
    attachment: &Attachment,
    rejected: &[(usize, usize)],
    obstacles: &[Rectangle],
) -> Result<AttachedPlan, NoFit> {
    let attempted = stages.len().saturating_mul(attachment.candidates.len());
    if !valid_rect(window) || !valid_rect(attachment.anchor) {
        return Err(NoFit { attempted });
    }
    // Each stage and candidate comes with its index, which the plan records.
    let query = |(stage_index, stage): (usize, &S),
                 (candidate_index, &candidate): (usize, &AnchorCandidate)| {
        if rejected.contains(&(stage_index, candidate_index)) {
            return None;
        }
        let anchor = paint_anchor(attachment.anchor, candidate.side, attachment.surface_insets);
        let mut available = offer(window, anchor, candidate)?;
        if !attachment.fill_width {
            available.width = available
                .width
                .min(stage.part().query(stage.level(), context).preferred.width);
        }
        let plan = stage
            .part()
            .query_at(stage.level(), context, available)
            .ok()?;
        let position = origin(window, anchor, candidate, plan.size());
        let bounds = Rectangle::new(position, plan.size());
        (contains(window, bounds)
            && !obstacles.iter().any(|obstacle| {
                bounds
                    .intersection(obstacle)
                    .is_some_and(|area| area.width > 0.0 && area.height > 0.0)
            }))
        .then_some(AttachedPlan {
            stage: StagePlan {
                stage_index,
                level: stage.level(),
                plan,
            },
            candidate_index,
            bounds,
        })
    };
    match attachment.preference {
        PlacementPreference::RepresentationFirst => {
            for stage in stages.iter().enumerate() {
                for candidate in attachment.candidates.iter().enumerate() {
                    if let Some(plan) = query(stage, candidate) {
                        return Ok(plan);
                    }
                }
            }
        }
        PlacementPreference::AnchorFirst => {
            for candidate in attachment.candidates.iter().enumerate() {
                for stage in stages.iter().enumerate() {
                    if let Some(plan) = query(stage, candidate) {
                        return Ok(plan);
                    }
                }
            }
        }
    }
    Err(NoFit { attempted })
}

/// Query a permitted attachment while protecting earlier ancestor surfaces.
/// The immediate parent may share its reserved shadow margin; older ancestors
/// are obstacles, so reversing a nested side placement cannot cover them.
pub fn choose_attached_avoiding<'a, H: Host, S: AsStage<'a, H>>(
    stages: &[S],
    context: &H::Context,
    window: Rectangle,
    attachment: &Attachment,
    obstacles: &[Rectangle],
) -> Result<AttachedPlan, NoFit> {
    choose_attached_excluding(stages, context, window, attachment, &[], obstacles)
}

/// One physical box can embed several consecutive logical chain levels. The
/// component supplies every composition; the engine knows only their spans.
/// `P` is the host's plan of the box.
#[derive(Clone)]
pub struct ChainCard<P> {
    pub index: usize,
    pub span: usize,
    pub plan: P,
}

/// Why a level of a chain was not placed: its source is missing (which
/// does not permit embedding it in its parent), or nothing fits.
pub enum ChainFailure {
    MissingSource(NoFit),
    NoFit(NoFit),
}

/// The cards a chain was placed as, and the level where it stopped, if any.
pub struct ChainSelection<P> {
    pub cards: Vec<ChainCard<P>>,
    pub no_fit: Option<(usize, NoFit)>,
}

/// Negotiate ordinary attachments before a component-declared embedded form.
/// Queries receive the accepted prefix, allowing a native adapter to resolve
/// descendant anchors in a disposable transaction. No runtime state belongs to
/// this scheduler, and it never enumerates a descendant's internal forms.
pub fn choose_attached_chain<P>(
    count: usize,
    max_span: impl Fn(usize) -> usize,
    mut query: impl FnMut(&[ChainCard<P>], usize, usize) -> Result<P, ChainFailure>,
) -> ChainSelection<P> {
    let mut cards: Vec<ChainCard<P>> = Vec::new();
    // The span each level is tried with, one entry per level: the loop runs
    // while `index` names a level.
    let mut spans = vec![1; count];
    let mut index = 0;
    while let Some(&span) = spans.get(index) {
        match query(&cards, index, span) {
            Ok(plan) => {
                cards.push(ChainCard { index, span, plan });
                index += span;
            }
            Err(failure) => {
                let (error, can_embed) = match failure {
                    ChainFailure::MissingSource(error) => (error, false),
                    ChainFailure::NoFit(error) => (error, true),
                };
                if can_embed && let Some(parent) = cards.last() {
                    let combined = parent.span + span;
                    // The parent card was accepted at a level the loop named,
                    // so its span is there to widen.
                    if combined <= max_span(parent.index)
                        && let Some(parent_span) = spans.get_mut(parent.index)
                    {
                        *parent_span = combined;
                        index = parent.index;
                        cards.pop();
                        continue;
                    }
                }
                return ChainSelection {
                    cards,
                    no_fit: Some((index, error)),
                };
            }
        }
    }
    ChainSelection {
        cards,
        no_fit: None,
    }
}

fn valid_rect(rect: Rectangle) -> bool {
    rect.x.is_finite()
        && rect.y.is_finite()
        && finite_size(rect.size())
        && rect.width >= 0.0
        && rect.height >= 0.0
}
/// Whether `rect` lies inside `window`, edges included.
pub fn contains(window: Rectangle, rect: Rectangle) -> bool {
    rect.x >= window.x
        && rect.y >= window.y
        && rect.x + rect.width <= window.x + window.width
        && rect.y + rect.height <= window.y + window.height
}
// Convert the visible attachment equation to the full paint-box equation.
// This does not reposition a committed plan: both offer and origin use the
// same inset-aware anchor before the content is measured.
fn paint_anchor(mut anchor: Rectangle, side: Side, insets: Padding) -> Rectangle {
    match side {
        Side::Right | Side::Left => {
            anchor.y -= insets.top;
            anchor.height += insets.top + insets.bottom;
            if side == Side::Right {
                anchor.width -= insets.left;
            } else {
                anchor.x += insets.right;
                anchor.width -= insets.right;
            }
        }
        Side::Below | Side::Above => {
            anchor.x -= insets.left;
            anchor.width += insets.left + insets.right;
            if side == Side::Below {
                anchor.height -= insets.top;
            } else {
                anchor.y += insets.bottom;
                anchor.height -= insets.bottom;
            }
        }
    }
    anchor
}

fn offer(window: Rectangle, anchor: Rectangle, candidate: AnchorCandidate) -> Option<Size> {
    if !candidate.gap.is_finite() || candidate.gap < 0.0 {
        return None;
    }
    let vertical = matches!(candidate.side, Side::Above | Side::Below);
    let (a_start, a_end, w_start, w_end) = if vertical {
        (
            anchor.x,
            anchor.x + anchor.width,
            window.x,
            window.x + window.width,
        )
    } else {
        (
            anchor.y,
            anchor.y + anchor.height,
            window.y,
            window.y + window.height,
        )
    };
    let cross = match candidate.alignment {
        Alignment::Start => w_end - a_start,
        Alignment::End => a_end - w_start,
        Alignment::Center => {
            let center = (a_start + a_end) / 2.0;
            2.0 * (center - w_start).min(w_end - center)
        }
        Alignment::StartBounded | Alignment::CenterBounded => w_end - w_start,
    };
    let main = match candidate.side {
        Side::Below => window.y + window.height - anchor.y - anchor.height - candidate.gap,
        Side::Above => anchor.y - window.y - candidate.gap,
        Side::Right => window.x + window.width - anchor.x - anchor.width - candidate.gap,
        Side::Left => anchor.x - window.x - candidate.gap,
    };
    (cross >= 0.0 && main >= 0.0).then_some(if vertical {
        Size::new(cross, main)
    } else {
        Size::new(main, cross)
    })
}
fn origin(window: Rectangle, anchor: Rectangle, candidate: AnchorCandidate, size: Size) -> Point {
    let align = |start: f32, length: f32, actual: f32, region_start: f32, region_length: f32| {
        match candidate.alignment {
            Alignment::Start => start,
            Alignment::End => start + length - actual,
            Alignment::Center => start + (length - actual) / 2.0,
            Alignment::StartBounded => start
                .max(region_start)
                .min(region_start + region_length - actual),
            Alignment::CenterBounded => (start + (length - actual) / 2.0)
                .min(region_start + region_length - actual)
                .max(region_start),
        }
    };
    match candidate.side {
        Side::Below => Point::new(
            align(anchor.x, anchor.width, size.width, window.x, window.width),
            anchor.y + anchor.height + candidate.gap,
        ),
        Side::Above => Point::new(
            align(anchor.x, anchor.width, size.width, window.x, window.width),
            anchor.y - size.height - candidate.gap,
        ),
        Side::Right => Point::new(
            anchor.x + anchor.width + candidate.gap,
            align(
                anchor.y,
                anchor.height,
                size.height,
                window.y,
                window.height,
            ),
        ),
        Side::Left => Point::new(
            anchor.x - size.width - candidate.gap,
            align(
                anchor.y,
                anchor.height,
                size.height,
                window.y,
                window.height,
            ),
        ),
    }
}
