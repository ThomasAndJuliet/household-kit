// SPDX-License-Identifier: GPL-3.0-only

//! Anchors, and sets of labels placed by pattern (design §9).
//!
//! # Anchors
//!
//! A label is placed beside an anchor: a rectangle, often its owner's ink
//! or one edge of it. An [`AnchorCandidate`] names a side of the anchor
//! (above, below, left, right), an alignment along that side (start, end,
//! centre, or one of these kept inside a region) and a gap. [`offer`] says
//! how much room a region leaves there and [`origin`] where a thing of a
//! given size goes. This is household-widgets' popup anchor equation
//! (`layout_popup.rs`) in `f64`, bounded by a region the caller gives
//! rather than a window.
//!
//! # Sets placed by pattern
//!
//! A set of labels (value labels, end labels, slice labels, empty-slot
//! notes) takes one pattern for every member: "it is just really
//! inconsistent to have some labels to the left and some to the right". A
//! [`Pattern`] is one place, applied to each member's owner, so a mixed set
//! cannot be expressed. A place can follow the data: "beyond the end" puts
//! a bar's label above it when it rises from zero and below it when it
//! falls, so both share one pattern. The pattern's gap is an entry of the
//! distance table ([`Gap`]), so tuning the table moves the labels.
//!
//! The patterns are tried in the caller's order. Each member's label is
//! placed by the anchor equation and judged by the record, against what is
//! already drawn and the members before it. If any member is rejected, the
//! pattern is; the first pattern with no rejection is taken. A pattern that
//! some member can take only with a halo it could have moved away from, at
//! a less preferred place, or last in order, waits behind every pattern
//! that needs none of these. A set with no pattern left says which member
//! stopped each pattern and why ([`NoPattern`]), so that its owner can take
//! its next form.
//!
//! Judging a set ([`judge_set`]) changes nothing; [`enter_set`] enters the
//! result into the record it was judged against; [`place_set`] does both.
//! Nothing is moved after it is placed.
//!
//! # Halo or move, once per set
//!
//! A member standing in a line by a verdict that needs a halo carries the
//! cut its halo makes ([`PlacedLabel::cuts`]), judged by the cut table: a
//! pattern whose cut falls at a crossing, a bend or a line's end is
//! rejected like any other, and one whose cut falls near an end or over a
//! third of the line waits behind those whose cuts do not. [`enter_set`]
//! enters the cuts with the labels, and the caller draws each line broken
//! where they say ([`Record::cuts_of`]).
//!
//! "Halo or move is chosen once per set" (design §9, provisional): a set
//! either moves every member that meets a line or haloes every one. A
//! pattern is one place for every member, so the choice is the pattern's:
//! the set moves when some pattern holds every member clear of the lines it
//! may only stand in with a halo, and otherwise takes the first pattern
//! that needs halos, every member standing in a line haloed where that
//! pattern puts it. No member is moved off its line while another is
//! haloed, since no pattern moves one member alone. The rule therefore
//! holds by construction rather than as a row of data: choosing per label
//! instead would need a list of places for each member, which sets placed
//! by pattern do not have.
//!
//! # Labels whose positions nearly agree share one line (U8)
//!
//! Juliet's rule, approved 8 November: value labels whose positions differ
//! by less than the least distance stand on one line, "since a step that
//! small reads as an error, not as data". A pattern does this when it is
//! asked to ([`Pattern::sharing_lines`]; off by default). The lines are
//! part of the pattern: every member's place is worked out by the anchor
//! equation, the lines are settled, and only then is any member judged.
//!
//! A label's position is its edge facing its mark: the foot of a label
//! above a bar, the left edge of a label to the right of one. On each side,
//! labels are taken in order of position, and a label joins the group
//! before it while its position is less than the least distance from the
//! group's first. Every label may stand no nearer its own mark than the
//! least distance and no farther than near (by default the related ideal
//! plus the least distance, so a label at the related ideal that moves out
//! to a shared line is always within it), both from the rules' distance
//! table. The group's line is the outermost label's position, brought back
//! within every label's range if it lies beyond one, so labels move away
//! from their marks. A group keeps its own positions when no line lies
//! within every label's range, when a label inside its owner would no
//! longer lie inside it, or when a label on the line would be rejected or
//! take a less preferred place than its own. Each group, shared or kept
//! and why, is reported in [`Placed::lines`].
//!
//! # The room a pattern needs
//!
//! [`room_needed`] says, before anything is placed, how much room a
//! pattern needs around its owners: beyond them, along the side each label
//! stands on, and across that side, where a label is wider than its owner
//! and reaches past the owner's sides. A chart reserves the first, and
//! compares the second with the room between its outermost owners and the
//! plot's edges, to know before placing that a label will not fit.
//!
//! # A worked example
//!
//! Her 11 px text gives a least distance of 2.2 px, a related ideal of
//! 3.696 px and near at 5.896 px. Two bars 20 px wide stand at x = 10 and
//! x = 50 on a zero line at y = 200; their tops are at 100 and 101.5.
//! Their value labels are 14 by 11 px, beyond the end, centred, at the
//! related gap.
//!
//! - Placed alone, the labels stand at (13, 85.304) and (53, 86.804): their
//!   feet are 1.5 px apart, at 96.304 and 97.804.
//! - Sharing lines, the two feet differ by less than 2.2 px and form one
//!   group. The first label's foot may lie between 97.8 and 94.104 (2.2 to
//!   5.896 px above its bar), the second's between 99.3 and 95.604. The
//!   outermost foot, 96.304, lies in both, so the second label moves up
//!   1.5 px and stands 5.196 px above its bar. The report is one shared
//!   line at 96.304.
//! - With tops at 100 and 103 the feet are 3 px apart, not less than the
//!   least distance, and each label keeps its own position.
//! - The pattern needs 3.696 + 11 = 14.696 px above the bars. A 14 px label
//!   on a 20 px bar reaches nothing past its sides; a 30 px one reaches 5 px
//!   past each.
//!
//! (Checked by the test `worked_example_value_labels_on_two_bars`.)

use crate::geometry::{Point, Rect, Size};
use crate::halo::Cut;
use crate::identity::Id;
use crate::kinds::{Kind, Role};
use crate::level::Level;
use crate::record::{Finding, Ink, Judgement, Occupant, Record, Revision};
use crate::rules::{Preference, Rules, Sizes};

/// Which side of its anchor a thing is placed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Below,
    Above,
    Right,
    Left,
}

/// How a thing is aligned with its anchor along the side it is placed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Alignment {
    /// Its start at the anchor's start.
    Start,
    /// Its end at the anchor's end.
    End,
    /// Centred on the anchor.
    Center,
    /// The anchor's start, shifted only as far as needed to stay inside the
    /// region.
    StartBounded,
    /// Centred on the anchor, shifted only as far as needed to stay inside
    /// the region.
    CenterBounded,
}

/// One place beside an anchor: a side, an alignment and a gap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorCandidate {
    pub side: Side,
    pub alignment: Alignment,
    pub gap: f64,
}

impl AnchorCandidate {
    pub fn new(side: Side, alignment: Alignment) -> Self {
        Self {
            side,
            alignment,
            gap: 0.0,
        }
    }

    pub fn gap(mut self, gap: f64) -> Self {
        self.gap = gap;
        self
    }
}

/// The room `region` offers beside `anchor` for `candidate`: across the
/// side and away from it. None where the gap is not a length or the region
/// leaves no room.
pub fn offer(region: Rect, anchor: Rect, candidate: AnchorCandidate) -> Option<Size> {
    if !candidate.gap.is_finite() || candidate.gap < 0.0 {
        return None;
    }
    let vertical = matches!(candidate.side, Side::Above | Side::Below);
    let (a_start, a_end, r_start, r_end) = if vertical {
        (anchor.x, anchor.right(), region.x, region.right())
    } else {
        (anchor.y, anchor.bottom(), region.y, region.bottom())
    };
    let cross = match candidate.alignment {
        Alignment::Start => r_end - a_start,
        Alignment::End => a_end - r_start,
        Alignment::Center => {
            let centre = (a_start + a_end) / 2.0;
            2.0 * (centre - r_start).min(r_end - centre)
        }
        Alignment::StartBounded | Alignment::CenterBounded => r_end - r_start,
    };
    let main = match candidate.side {
        Side::Below => region.bottom() - anchor.bottom() - candidate.gap,
        Side::Above => anchor.y - region.y - candidate.gap,
        Side::Right => region.right() - anchor.right() - candidate.gap,
        Side::Left => anchor.x - region.x - candidate.gap,
    };
    (cross >= 0.0 && main >= 0.0).then_some(if vertical {
        Size::new(cross, main)
    } else {
        Size::new(main, cross)
    })
}

/// Where a thing of `size` goes beside `anchor` for `candidate`, the
/// bounded alignments kept inside `region`.
pub fn origin(region: Rect, anchor: Rect, candidate: AnchorCandidate, size: Size) -> Point {
    let align = |start: f64, length: f64, actual: f64, region_start: f64, region_length: f64| {
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
    let across = || align(anchor.x, anchor.width, size.width, region.x, region.width);
    let down = || {
        align(
            anchor.y,
            anchor.height,
            size.height,
            region.y,
            region.height,
        )
    };
    match candidate.side {
        Side::Below => Point::new(across(), anchor.bottom() + candidate.gap),
        Side::Above => Point::new(across(), anchor.y - size.height - candidate.gap),
        Side::Right => Point::new(anchor.right() + candidate.gap, down()),
        Side::Left => Point::new(anchor.x - size.width - candidate.gap, down()),
    }
}

/// What part of its owner's ink a label is anchored on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Attach {
    /// The owner's whole ink: a label beside it.
    Whole,
    /// One edge of the owner's ink, as a line of no thickness: with the
    /// opposite side, a label inside the owner at that end.
    Edge(Side),
}

impl Attach {
    /// The anchor this gives on an owner's ink.
    pub fn anchor(self, ink: Rect) -> Rect {
        match self {
            Self::Whole => ink,
            Self::Edge(side) => edge(ink, side),
        }
    }
}

impl Side {
    /// The side facing this one.
    pub fn opposite(self) -> Self {
        match self {
            Self::Below => Self::Above,
            Self::Above => Self::Below,
            Self::Right => Self::Left,
            Self::Left => Self::Right,
        }
    }
}

/// One edge of a rectangle, as a line of no thickness.
fn edge(ink: Rect, side: Side) -> Rect {
    match side {
        Side::Above => Rect::new(ink.x, ink.y, ink.width, 0.0),
        Side::Below => Rect::new(ink.x, ink.bottom(), ink.width, 0.0),
        Side::Left => Rect::new(ink.x, ink.y, 0.0, ink.height),
        Side::Right => Rect::new(ink.right(), ink.y, 0.0, ink.height),
    }
}

/// The end of a mark that points toward `end`: for an area, its edge on
/// that side; for a line, the end point furthest that way, as wide as the
/// line, so that a gap from the anchor is a gap from the line's ink.
fn end_of(ink: &Ink, end: Side) -> Rect {
    match *ink {
        Ink::Rect(area) => edge(area, end),
        Ink::Segment { from, to, width } => {
            let further = |p: Point, q: Point| match end {
                Side::Above => p.y <= q.y,
                Side::Below => p.y >= q.y,
                Side::Left => p.x <= q.x,
                Side::Right => p.x >= q.x,
            };
            let point = if further(to, from) { to } else { from };
            Rect::new(point.x, point.y, 0.0, 0.0).grown(width / 2.0)
        }
    }
}

/// Where a pattern puts each member's label, by an absolute side or by the
/// way the member's mark points (semantics C4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Place {
    /// On an absolute `side` of an anchor on the owner's ink. Anchored on
    /// an edge with the opposite side, the label is inside the owner.
    Beside { on: Attach, side: Side },
    /// Beyond the end of the owner's mark, the way the member says it
    /// points: above a bar above zero and below one below zero.
    BeyondEnd,
    /// Inside the owner, at the end its mark points to.
    InsideEnd,
}

impl Place {
    /// The anchor, the side and whether the label is inside the owner, for
    /// a member whose mark's end points toward `end`. None for a place
    /// relative to the end when the member does not say which way it
    /// points.
    fn resolve(self, owner: &Ink, end: Option<Side>) -> Option<(Rect, Side, Option<Side>)> {
        match self {
            Self::Beside { on, side } => {
                let inside = matches!(on, Attach::Edge(e) if e == side.opposite());
                Some((
                    on.anchor(owner.bounds()),
                    side,
                    inside.then(|| side.opposite()),
                ))
            }
            Self::BeyondEnd => end.map(|end| (end_of(owner, end), end, None)),
            Self::InsideEnd => end.map(|end| (end_of(owner, end), end.opposite(), Some(end))),
        }
    }

    /// The side a label goes on and whether it is inside its owner, without
    /// the owner: for the room a pattern needs.
    fn side_for(self, end: Option<Side>) -> Option<(Side, bool)> {
        match self {
            Self::Beside { on, side } => {
                Some((side, matches!(on, Attach::Edge(e) if e == side.opposite())))
            }
            Self::BeyondEnd => end.map(|end| (end, false)),
            Self::InsideEnd => end.map(|end| (end.opposite(), true)),
        }
    }
}

/// A pattern's gap: an entry of the distance table, so that tuning the
/// table moves the labels, or a length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gap {
    Least,
    Related,
    Unrelated,
    Length(f64),
}

impl Gap {
    /// The gap in the units of `sizes`, by the distance table in `rules`.
    pub fn resolve(self, rules: &Rules, sizes: &Sizes) -> f64 {
        let spacing = &rules.distances.diagram;
        match self {
            Self::Least => spacing.least.resolve(sizes),
            Self::Related => spacing.related.resolve(sizes),
            Self::Unrelated => spacing.unrelated.resolve(sizes),
            Self::Length(length) => length,
        }
    }
}

/// One pattern for a whole set: the same place on every member's owner.
#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    /// The pattern's name, for reports ("beyond the end", "inside").
    pub name: String,
    pub place: Place,
    pub alignment: Alignment,
    pub gap: Gap,
    /// Whether labels whose positions nearly agree share one line (U8).
    /// Off by default; see the module's account of shared lines.
    pub share_line: bool,
}

impl Pattern {
    /// A pattern on an absolute side of an anchor on each owner.
    pub fn new(name: impl Into<String>, on: Attach, side: Side, alignment: Alignment) -> Self {
        Self::at(name, Place::Beside { on, side }, alignment)
    }

    /// Beyond the end of each member's mark, the way it points.
    pub fn beyond_end(name: impl Into<String>, alignment: Alignment) -> Self {
        Self::at(name, Place::BeyondEnd, alignment)
    }

    /// Inside each owner, at the end its mark points to.
    pub fn inside_end(name: impl Into<String>, alignment: Alignment) -> Self {
        Self::at(name, Place::InsideEnd, alignment)
    }

    fn at(name: impl Into<String>, place: Place, alignment: Alignment) -> Self {
        Self {
            name: name.into(),
            place,
            alignment,
            gap: Gap::Related,
            share_line: false,
        }
    }

    pub fn gap(mut self, gap: Gap) -> Self {
        self.gap = gap;
        self
    }

    /// This pattern with labels whose positions nearly agree sharing one
    /// line (U8).
    pub fn sharing_lines(mut self) -> Self {
        self.share_line = true;
        self
    }
}

/// One label of a set: its identity, its owner (already in the record),
/// its measured size, and which way its mark's end points, for patterns
/// that follow the data.
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub id: Id,
    pub owner: Id,
    pub size: Size,
    pub end: Option<Side>,
}

impl Member {
    pub fn new(id: Id, owner: Id, size: Size) -> Self {
        Self {
            id,
            owner,
            size,
            end: None,
        }
    }

    /// This member with its mark's end pointing toward `end`: above for a
    /// bar above zero, below for one below, right for a line read left to
    /// right.
    pub fn pointing(mut self, end: Side) -> Self {
        self.end = Some(end);
        self
    }
}

/// A set of labels placed by pattern. Its members share a kind and a role.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelSet {
    pub id: Id,
    pub kind: Kind,
    pub role: Option<Role>,
    pub members: Vec<Member>,
    /// The patterns in the caller's order of preference.
    pub patterns: Vec<Pattern>,
}

/// Why a member stopped a pattern.
#[derive(Clone, Debug, PartialEq)]
pub enum Reason {
    /// The member's owner is not in the record.
    OwnerNotRecorded,
    /// The pattern follows the data and the member does not say which way
    /// its mark's end points.
    NoEnd,
    /// The region offers less room than the label needs, or none.
    NoRoom { offered: Option<Size> },
    /// The pattern puts the label inside its owner and it does not lie
    /// within `within`: the owner's ink less the least distance from every
    /// edge but the one it is anchored on (semantics C2).
    NotInside { label: Rect, within: Rect },
    /// The record's finding that rejects the place.
    Rejected(Finding),
}

/// A pattern stopped by one member.
#[derive(Clone, Debug, PartialEq)]
pub struct Stopped {
    /// The pattern's index in the set's list.
    pub pattern: usize,
    pub member: Id,
    pub reason: Reason,
}

/// A set with no pattern left.
#[derive(Clone, Debug, PartialEq)]
pub struct NoPattern {
    pub set: Id,
    /// Every pattern, in the set's order, with the member that stopped it.
    pub stopped: Vec<Stopped>,
}

/// One placed label.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedLabel {
    pub id: Id,
    pub rect: Rect,
    /// What enters the record: the label with its kind, owner and role.
    pub occupant: Occupant,
    /// What the record found: the verdicts the caller acts on (a halo, the
    /// toggle's ink) and any flagged squeezing.
    pub judgement: Judgement,
}

impl PlacedLabel {
    /// The cuts this label's halo makes in the lines it stands in: where
    /// to draw each of them broken.
    pub fn cuts(&self) -> &[Cut] {
        &self.judgement.cuts
    }
}

/// A set placed by one pattern: judged, and not yet entered unless
/// [`place_set`] made it.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    /// The pattern's index in the set's list.
    pub pattern: usize,
    pub preference: Preference,
    pub labels: Vec<PlacedLabel>,
    /// Every group of labels whose positions nearly agreed, in the order
    /// of the sides and then of their positions, and whether it shares a
    /// line. Empty unless the pattern shares lines.
    pub lines: Vec<SharedLine>,
    /// The record's revision when the set was judged: [`enter_set`] enters
    /// the set only into a record still at it.
    pub judged_at: Revision,
}

/// A group of labels on one side whose positions differ by less than the
/// least distance (U8).
#[derive(Clone, Debug, PartialEq)]
pub struct SharedLine {
    /// The members, in order of position along their side, from the one
    /// standing least far out.
    pub members: Vec<Id>,
    /// The side of their anchors they stand on.
    pub side: Side,
    pub outcome: LineOutcome,
}

/// What became of a group of labels whose positions nearly agree.
#[derive(Clone, Debug, PartialEq)]
pub enum LineOutcome {
    /// The group stands on one line: `at` is the coordinate of every
    /// label's edge facing its mark (the feet of labels above their marks,
    /// the left edges of labels to the right of theirs).
    Shared { at: f64 },
    /// The group keeps its own positions, for this reason.
    Kept(KeptBecause),
}

/// Why a group of labels whose positions nearly agree keeps its own
/// positions.
#[derive(Clone, Debug, PartialEq)]
pub enum KeptBecause {
    /// No one line keeps every label of the group between the least
    /// distance and near of its own mark: one would no longer clearly be
    /// its mark's.
    NoLine,
    /// On the line, `member` would no longer lie inside its owner, the
    /// least distance from its edges.
    NotInside { member: Id },
    /// On the line, `member` would lose its clearance: the record would
    /// reject it (the finding is given), or it would take a less preferred
    /// place than its own.
    Clearance {
        member: Id,
        finding: Option<Finding>,
    },
}

/// A judged set was not entered because the record changed after it was
/// judged (semantics C9); judge it again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stale {
    pub judged_at: Revision,
    pub now: Revision,
}

/// Judges a set: the first pattern every member can take at `level` in
/// `region`. A pattern that needs a halo where moving would avoid one, or
/// whose places are less preferred or last in order, waits behind every
/// pattern that needs none, in that order (semantics C3). Changes nothing:
/// enter the result with [`enter_set`] into the record it was judged
/// against (semantics C9).
pub fn judge_set(
    record: &Record,
    set: &LabelSet,
    region: Rect,
    level: Level,
) -> Result<Placed, NoPattern> {
    let mut stopped = Vec::new();
    let mut waiting: Option<Placed> = None;
    for (index, pattern) in set.patterns.iter().enumerate() {
        match judge_pattern(record, set, index, pattern, region, level) {
            Ok(placed) if placed.preference == Preference::InOrder => return Ok(placed),
            Ok(placed) => {
                if waiting
                    .as_ref()
                    .is_none_or(|w| placed.preference < w.preference)
                {
                    waiting = Some(placed);
                }
            }
            Err(s) => stopped.push(s),
        }
    }
    waiting.ok_or_else(|| NoPattern {
        set: set.id.clone(),
        stopped,
    })
}

/// Enters a judged set's labels in the record, in the members' order, and
/// the cuts their halos make in the lines they stand in.
/// Fails, entering nothing, if the record has changed since the set was
/// judged: two sets judged against one record cannot both be entered
/// (semantics C9).
pub fn enter_set(record: &mut Record, placed: &Placed) -> Result<(), Stale> {
    if record.revision() != placed.judged_at {
        return Err(Stale {
            judged_at: placed.judged_at,
            now: record.revision(),
        });
    }
    enter_judged(record, placed);
    Ok(())
}

/// Enters a judged set's labels and their cuts, without asking whether the
/// record has changed since the set was judged: for a caller that has just
/// judged it against this record.
fn enter_judged(record: &mut Record, placed: &Placed) {
    record.enter_all(placed.labels.iter().map(|label| label.occupant.clone()));
    record.enter_cuts(
        placed
            .labels
            .iter()
            .flat_map(|label| label.cuts().iter().cloned()),
    );
}

/// Judges a set and enters what it places: [`judge_set`], then
/// [`enter_set`]. On failure the record is unchanged.
pub fn place_set(
    record: &mut Record,
    set: &LabelSet,
    region: Rect,
    level: Level,
) -> Result<Placed, NoPattern> {
    let placed = judge_set(record, set, region, level)?;
    // Judged against this very record a moment ago, so it cannot be stale.
    enter_judged(record, &placed);
    Ok(placed)
}

/// Lengths on each side of a set's owners.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Room {
    pub above: f64,
    pub below: f64,
    pub left: f64,
    pub right: f64,
}

impl Room {
    fn widen(&mut self, side: Side, by: f64) {
        let edge = match side {
            Side::Above => &mut self.above,
            Side::Below => &mut self.below,
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        };
        *edge = edge.max(by);
    }
}

/// The room a pattern needs around its owners, from [`room_needed`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RoomNeeded {
    /// Beyond the owners along the side each label stands on: the gap and
    /// the label's depth, the most over the members. A label inside its
    /// owner needs none.
    pub beyond: Room,
    /// How far the labels reach past their owners' sides across the side
    /// they stand on: for a label above or below its owner, past the
    /// owner's left and right; for one to its left or right, past its top
    /// and bottom. The most over the members, and nothing where every
    /// label lies within its owner's extent.
    pub across: Room,
}

/// The room pattern `pattern` of `set` needs around its owners: beyond
/// them along the side each label goes on, and past their sides across it.
/// A pure query that needs neither the record nor the owners' places, so a
/// chart can reserve the room before anything is placed (design §9,
/// semantics C9). `owner_across` gives each member's owner's length across
/// the side its label stands on: a bar's width for a label above or below
/// it, a line's width at its end. A member with no end stops a pattern that
/// follows the data.
///
/// Across, a centred label reaches past each side by half the amount it
/// is wider than its owner, a label aligned with its owner's start reaches
/// past the end by all of it, and one aligned with the end past the start.
/// A bounded alignment counts as the alignment it starts from: what the
/// region then shifts back is what the region could not give. A label
/// inside its owner that reaches past it across will not lie within it,
/// which judging reports as [`Reason::NotInside`].
///
/// Sharing lines (U8) needs no more room while the pattern's gap is at
/// least the least distance: a group's line is then never beyond its
/// outermost label, so no label goes beyond the outermost place a label of
/// the set takes without it.
///
/// A pattern the set does not have places no label, and needs no room.
pub fn room_needed(
    set: &LabelSet,
    pattern: usize,
    rules: &Rules,
    sizes: &Sizes,
    owner_across: impl Fn(&Member) -> f64,
) -> Result<RoomNeeded, Stopped> {
    let Some(pattern_ref) = set.patterns.get(pattern) else {
        return Ok(RoomNeeded::default());
    };
    let gap = pattern_ref.gap.resolve(rules, sizes);
    let mut room = RoomNeeded::default();
    for member in &set.members {
        let Some((side, inside)) = pattern_ref.place.side_for(member.end) else {
            return Err(Stopped {
                pattern,
                member: member.id.clone(),
                reason: Reason::NoEnd,
            });
        };
        let (depth, breadth, start, end) = match side {
            Side::Above | Side::Below => (
                member.size.height,
                member.size.width,
                Side::Left,
                Side::Right,
            ),
            Side::Left | Side::Right => (
                member.size.width,
                member.size.height,
                Side::Above,
                Side::Below,
            ),
        };
        let wider = (breadth - owner_across(member)).max(0.0);
        let (past_start, past_end) = match pattern_ref.alignment {
            Alignment::Start | Alignment::StartBounded => (0.0, wider),
            Alignment::End => (wider, 0.0),
            Alignment::Center | Alignment::CenterBounded => (wider / 2.0, wider / 2.0),
        };
        room.across.widen(start, past_start);
        room.across.widen(end, past_end);
        if !inside {
            room.beyond.widen(side, gap + depth);
        }
    }
    Ok(room)
}

/// One member's place by a pattern, worked out by the anchor equation
/// before anything is judged.
#[derive(Clone, Debug)]
struct Spot<'a> {
    owner: &'a Occupant,
    /// The side of its anchor the label stands on.
    side: Side,
    /// The owner's edge the label is anchored on, where it stands inside.
    inside: Option<Side>,
    rect: Rect,
}

/// Works out where `member` goes by `pattern`, or why it cannot.
fn spot<'a>(
    record: &'a Record,
    member: &Member,
    pattern: &Pattern,
    gap: f64,
    region: Rect,
) -> Result<Spot<'a>, Reason> {
    let owner = record.get(&member.owner).ok_or(Reason::OwnerNotRecorded)?;
    let (anchor, side, inside) = pattern
        .place
        .resolve(&owner.ink, member.end)
        .ok_or(Reason::NoEnd)?;
    let candidate = AnchorCandidate::new(side, pattern.alignment).gap(gap);
    let offered = offer(region, anchor, candidate);
    if !offered.is_some_and(|room| member.size.fits(room)) {
        return Err(Reason::NoRoom { offered });
    }
    let rect = Rect::from_origin(origin(region, anchor, candidate, member.size), member.size);
    Ok(Spot {
        owner,
        side,
        inside,
        rect,
    })
}

/// What a set's label enters the record as.
fn occupant(set: &LabelSet, member: &Member, rect: Rect) -> Occupant {
    let mut label =
        Occupant::new(member.id.clone(), set.kind, Ink::Rect(rect)).owned_by(member.owner.clone());
    label.role = set.role.clone();
    label
}

/// Judges every member by one pattern, the set's `index`th, each against
/// the record and the members before it; stops at the first rejection, in
/// the members' order. Where the pattern shares lines, every member's place
/// is worked out first and the lines are settled before any member is
/// judged.
fn judge_pattern(
    record: &Record,
    set: &LabelSet,
    index: usize,
    pattern: &Pattern,
    region: Rect,
    level: Level,
) -> Result<Placed, Stopped> {
    let gap = pattern.gap.resolve(record.rules(), record.sizes());
    let mut spots: Vec<Result<Spot, Reason>> = set
        .members
        .iter()
        .map(|member| spot(record, member, pattern, gap, region))
        .collect();
    let mut lines = Vec::new();
    if pattern.share_line
        && let Ok(mut ready) = spots.iter().cloned().collect::<Result<Vec<_>, _>>()
    {
        lines = share_lines(record, set, &mut ready, gap, level);
        spots = ready.into_iter().map(Ok).collect();
    }
    let mut labels: Vec<PlacedLabel> = Vec::with_capacity(set.members.len());
    let mut pending: Vec<Occupant> = Vec::with_capacity(set.members.len());
    let mut preference = Preference::InOrder;
    for (member, spot) in set.members.iter().zip(spots) {
        let stop = |reason| Stopped {
            pattern: index,
            member: member.id.clone(),
            reason,
        };
        let spot = spot.map_err(stop)?;
        let rect = spot.rect;
        if let Some(edge) = spot.inside {
            let within = inset_but(record, set.kind, spot.owner, edge);
            if !within.contains(rect) {
                return Err(stop(Reason::NotInside {
                    label: rect,
                    within,
                }));
            }
        }
        let label = occupant(set, member, rect);
        let judgement = record.judge_with(&label, &pending);
        if let Some(finding) = judgement.rejection_at(level) {
            return Err(stop(Reason::Rejected(finding.clone())));
        }
        preference = preference.max(judgement.preference());
        pending.push(label.clone());
        labels.push(PlacedLabel {
            id: member.id.clone(),
            rect,
            occupant: label,
            judgement,
        });
    }
    Ok(Placed {
        pattern: index,
        preference,
        labels,
        lines,
        judged_at: record.revision(),
    })
}

/// A label's position along its side, measured away from its mark: the
/// coordinate of its edge facing the mark, signed so that larger is
/// farther from the mark. Labels on one line have the same position.
fn away(side: Side, rect: Rect) -> f64 {
    match side {
        Side::Above => -rect.bottom(),
        Side::Below => rect.y,
        Side::Left => -rect.right(),
        Side::Right => rect.x,
    }
}

/// `rect` moved `by` away from its mark along `side`.
fn moved_away(side: Side, rect: Rect, by: f64) -> Rect {
    let by = match side {
        Side::Above => Point::new(0.0, -by),
        Side::Below => Point::new(0.0, by),
        Side::Left => Point::new(-by, 0.0),
        Side::Right => Point::new(by, 0.0),
    };
    rect.translated(by)
}

/// Settles the shared lines of a set whose every member has a place (U8),
/// moving the members of each group that shares one. Groups are formed on
/// each side separately, in order of position: a label joins the group
/// before it while its position is less than the least distance from the
/// group's first. A group whose positions already agree is left alone.
fn share_lines(
    record: &Record,
    set: &LabelSet,
    spots: &mut [Spot],
    gap: f64,
    level: Level,
) -> Vec<SharedLine> {
    let least = record
        .rules()
        .distances
        .diagram
        .least
        .resolve(record.sizes());
    let mut lines = Vec::new();
    for side in [Side::Above, Side::Below, Side::Left, Side::Right] {
        let mut order: Vec<Grouped> = set
            .members
            .iter()
            .zip(spots.iter())
            .enumerate()
            .filter(|(_, (_, spot))| spot.side == side)
            .map(|(index, (member, spot))| Grouped {
                index,
                member,
                position: away(side, spot.rect),
                spot: spot.clone(),
            })
            .collect();
        order.sort_by(|a, b| a.position.total_cmp(&b.position));
        let mut groups: Vec<Vec<Grouped>> = Vec::new();
        for grouped in order {
            match groups.last_mut() {
                Some(group)
                    if group
                        .first()
                        .is_some_and(|first| grouped.position - first.position < least) =>
                {
                    group.push(grouped);
                }
                _ => groups.push(vec![grouped]),
            }
        }
        for group in groups {
            let (Some(first), Some(last)) = (group.first(), group.last()) else {
                continue;
            };
            if group.len() < 2 || last.position <= first.position {
                continue;
            }
            let outcome = share_line(record, set, spots, &group, side, gap, level);
            lines.push(SharedLine {
                members: group
                    .iter()
                    .map(|grouped| grouped.member.id.clone())
                    .collect(),
                side,
                outcome,
            });
        }
    }
    lines
}

/// A member on one side, as it is grouped with the members whose positions
/// nearly agree with its own: where it stands among the set's members (and
/// so among their places), its position along its side ([`away`]), and its
/// place.
struct Grouped<'a> {
    index: usize,
    member: &'a Member,
    position: f64,
    spot: Spot<'a>,
}

/// Puts one group on one line if it can, moving its members in `spots`.
///
/// Each label may stand between the least distance (for its kind and its
/// owner's) and near from its own mark, so each allows a band of
/// positions; the group's line must lie in every band. It is the position
/// of the group's outermost label, brought back into the bands if it lies
/// beyond them, so labels move away from their marks where they can. The
/// group keeps its positions if the bands have nothing in common, if a
/// label standing inside its owner would no longer lie inside it, or if any
/// label on the line, judged against the record and every other label of
/// the set, would be rejected at `level` or take a less preferred place
/// than at its own position.
fn share_line(
    record: &Record,
    set: &LabelSet,
    spots: &mut [Spot],
    group: &[Grouped],
    side: Side,
    gap: f64,
    level: Level,
) -> LineOutcome {
    let spacing = &record.rules().distances.diagram;
    let near = spacing.near.resolve(record.sizes());
    let (mut low, mut high, mut outermost) = (f64::MIN, f64::MAX, f64::MIN);
    for grouped in group {
        let position = grouped.position;
        let mark = position - gap;
        let (least, _) = spacing.least_between(set.kind, grouped.spot.owner.kind, record.sizes());
        low = low.max(mark + least);
        high = high.min(mark + near);
        outermost = outermost.max(position);
    }
    if low > high {
        return LineOutcome::Kept(KeptBecause::NoLine);
    }
    let line = outermost.clamp(low, high);
    let onto_line = |rect: Rect| moved_away(side, rect, line - away(side, rect));
    // Every member's place with the group's moved onto the line.
    let trial: Vec<Rect> = spots
        .iter()
        .enumerate()
        .map(|(index, spot)| {
            if group.iter().any(|grouped| grouped.index == index) {
                onto_line(spot.rect)
            } else {
                spot.rect
            }
        })
        .collect();
    for grouped in group {
        if let Some(edge) = grouped.spot.inside
            && !inset_but(record, set.kind, grouped.spot.owner, edge)
                .contains(onto_line(grouped.spot.rect))
        {
            return LineOutcome::Kept(KeptBecause::NotInside {
                member: grouped.member.id.clone(),
            });
        }
    }
    // A member's label at `rect`, judged against the record and every other
    // member's label at its place in `rects`.
    let judge = |rects: &[Rect], grouped: &Grouped, rect: Rect| {
        let others: Vec<Occupant> = set
            .members
            .iter()
            .zip(rects)
            .enumerate()
            .filter(|&(k, _)| k != grouped.index)
            .map(|(_, (member, &rect))| occupant(set, member, rect))
            .collect();
        record.judge_with(&occupant(set, grouped.member, rect), &others)
    };
    let own: Vec<Rect> = spots.iter().map(|spot| spot.rect).collect();
    for grouped in group {
        let on_line = judge(&trial, grouped, onto_line(grouped.spot.rect));
        let refused = on_line.rejection_at(level).cloned();
        if refused.is_some()
            || on_line.preference() > judge(&own, grouped, grouped.spot.rect).preference()
        {
            return LineOutcome::Kept(KeptBecause::Clearance {
                member: grouped.member.id.clone(),
                finding: refused,
            });
        }
    }
    // Members outside the group keep their places, which `trial` holds.
    for (spot, rect) in spots.iter_mut().zip(trial) {
        spot.rect = rect;
    }
    LineOutcome::Shared {
        at: match side {
            Side::Above | Side::Left => -line,
            Side::Below | Side::Right => line,
        },
    }
}

/// The owner's ink less the table's least distance between a label of
/// `kind` and the owner, on every edge but `anchored`.
fn inset_but(record: &Record, kind: Kind, owner: &Occupant, anchored: Side) -> Rect {
    let (least, _) =
        record
            .rules()
            .distances
            .diagram
            .least_between(kind, owner.kind, record.sizes());
    let ink = owner.ink.bounds();
    let at = |side: Side| if side == anchored { 0.0 } else { least };
    let (top, bottom) = (at(Side::Above), at(Side::Below));
    let (left, right) = (at(Side::Left), at(Side::Right));
    Rect::new(
        ink.x + left,
        ink.y + top,
        (ink.width - left - right).max(0.0),
        (ink.height - top - bottom).max(0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::Against;
    use crate::rules::{Rules, Sizes, Verdict};

    // ----- the anchor equation -----

    const REGION: Rect = Rect::new(0.0, 0.0, 100.0, 100.0);
    const ANCHOR: Rect = Rect::new(40.0, 40.0, 20.0, 10.0);
    const SIZE: Size = Size::new(10.0, 5.0);

    fn place(side: Side, alignment: Alignment) -> (Option<Size>, Point) {
        let c = AnchorCandidate::new(side, alignment).gap(2.0);
        (offer(REGION, ANCHOR, c), origin(REGION, ANCHOR, c, SIZE))
    }

    #[test]
    fn the_anchor_equation_on_each_side() {
        use Alignment::Start;
        assert_eq!(
            place(Side::Below, Start),
            (Some(Size::new(60.0, 48.0)), Point::new(40.0, 52.0))
        );
        assert_eq!(
            place(Side::Above, Start),
            (Some(Size::new(60.0, 38.0)), Point::new(40.0, 33.0))
        );
        assert_eq!(
            place(Side::Right, Start),
            (Some(Size::new(38.0, 60.0)), Point::new(62.0, 40.0))
        );
        assert_eq!(
            place(Side::Left, Start),
            (Some(Size::new(38.0, 60.0)), Point::new(28.0, 40.0))
        );
    }

    #[test]
    fn the_anchor_equation_for_each_alignment() {
        assert_eq!(
            place(Side::Below, Alignment::End),
            (Some(Size::new(60.0, 48.0)), Point::new(50.0, 52.0))
        );
        assert_eq!(
            place(Side::Below, Alignment::Center),
            (Some(Size::new(100.0, 48.0)), Point::new(45.0, 52.0))
        );
        assert_eq!(
            place(Side::Right, Alignment::Center),
            (Some(Size::new(38.0, 90.0)), Point::new(62.0, 42.5))
        );
        // Bounded alignments offer the whole region across and stay in it.
        let near_right = Rect::new(95.0, 40.0, 5.0, 10.0);
        let c = AnchorCandidate::new(Side::Below, Alignment::StartBounded);
        assert_eq!(offer(REGION, near_right, c), Some(Size::new(100.0, 50.0)));
        assert_eq!(origin(REGION, near_right, c, SIZE), Point::new(90.0, 50.0));
        let near_left = Rect::new(0.0, 40.0, 4.0, 10.0);
        let c = AnchorCandidate::new(Side::Above, Alignment::CenterBounded);
        assert_eq!(origin(REGION, near_left, c, SIZE), Point::new(0.0, 35.0));
        let c = AnchorCandidate::new(Side::Above, Alignment::Center);
        assert_eq!(origin(REGION, near_left, c, SIZE), Point::new(-3.0, 35.0));
    }

    #[test]
    fn no_room_and_no_negative_gap() {
        let c = AnchorCandidate::new(Side::Above, Alignment::Start);
        assert_eq!(offer(REGION, ANCHOR, c.gap(-1.0)), None);
        assert_eq!(offer(REGION, ANCHOR, c.gap(f64::NAN)), None);
        assert_eq!(offer(REGION, ANCHOR, c.gap(41.0)), None);
        let outside = Rect::new(120.0, 40.0, 10.0, 10.0);
        assert_eq!(offer(REGION, outside, c.gap(1.0)), None);
    }

    // ----- sets placed by pattern -----

    const BASELINE: f64 = 200.0;
    const PLOT: Rect = Rect::new(0.0, 0.0, 200.0, 220.0);
    const LABEL: Size = Size::new(14.0, 11.0);

    /// Her 11 px text, columns 20 px wide on a 40 px pitch, and the zero
    /// line along their feet.
    fn chart(tops: &[f64]) -> Record {
        let mut r = bars(tops);
        r.enter(
            Occupant::new(
                Id::new("zero"),
                Kind::ValueLine,
                Ink::segment(Point::new(0.0, BASELINE), Point::new(200.0, BASELINE), 1.0),
            )
            .with_role(Role::zero_line()),
        );
        r
    }

    /// The columns alone, with no zero line.
    fn bars(tops: &[f64]) -> Record {
        let mut r = Record::new(
            Rules::default(),
            Sizes {
                label_size: 11.0,
                word_space: 3.7,
                line_width: 1.0,
            },
        )
        .with_bounds(PLOT);
        for (i, &top) in tops.iter().enumerate() {
            let x = 10.0 + 40.0 * i as f64;
            r.enter(Occupant::new(
                Id::new(format!("b{i}")),
                Kind::DataMark,
                Ink::Rect(Rect::new(x, top, 20.0, BASELINE - top)),
            ));
        }
        r
    }

    fn value_labels(count: usize) -> LabelSet {
        LabelSet {
            id: Id::new("values"),
            kind: Kind::Number,
            role: None,
            members: (0..count)
                .map(|i| {
                    Member::new(
                        Id::new(format!("b{i}/value")),
                        Id::new(format!("b{i}")),
                        LABEL,
                    )
                })
                .collect(),
            patterns: vec![
                Pattern::new(
                    "beyond the end",
                    Attach::Whole,
                    Side::Above,
                    Alignment::Center,
                ),
                Pattern::new(
                    "inside, in the toggle's ink",
                    Attach::Edge(Side::Above),
                    Side::Below,
                    Alignment::Center,
                ),
            ],
        }
    }

    /// A note standing where b1's value label would go beyond its end.
    fn note_over_b1(r: &mut Record) {
        let related = 0.336 * 11.0;
        r.enter(Occupant::new(
            Id::new("note"),
            Kind::Words,
            Ink::Rect(Rect::new(53.0, 60.0 - related - 11.0, 14.0, 11.0)),
        ));
    }

    #[test]
    fn value_labels_beyond_their_bars_ends_are_all_accepted() {
        let mut r = chart(&[100.0, 60.0, 120.0]);
        let placed = place_set(&mut r, &value_labels(3), PLOT, Level::Preferred).unwrap();
        assert_eq!(placed.pattern, 0);
        let rects: Vec<_> = placed.labels.iter().map(|l| l.rect).collect();
        let y = |top: f64| top - 11.0 - 0.336 * 11.0;
        assert_eq!(
            rects,
            [
                Rect::new(13.0, y(100.0), 14.0, 11.0),
                Rect::new(53.0, y(60.0), 14.0, 11.0),
                Rect::new(93.0, y(120.0), 14.0, 11.0),
            ]
        );
        assert!(
            placed
                .labels
                .iter()
                .all(|l| l.judgement.findings.is_empty())
        );
        assert_eq!(r.occupants().len(), 4 + 3);
        assert!(r.get(&Id::new("b1/value")).is_some());
    }

    #[test]
    fn one_member_meeting_another_label_rejects_the_pattern_for_all() {
        let mut r = chart(&[100.0, 60.0, 120.0]);
        note_over_b1(&mut r);
        let placed = place_set(&mut r, &value_labels(3), PLOT, Level::Preferred).unwrap();
        assert_eq!(placed.pattern, 1, "inside, for every member");
        for (label, top) in placed.labels.iter().zip([100.0, 60.0, 120.0]) {
            assert_eq!(label.rect.y, top + 0.336 * 11.0);
            assert_eq!(
                label.judgement.verdicts().collect::<Vec<_>>(),
                [Verdict::AllowedInToggleInk]
            );
        }
        // Nothing of the rejected pattern stays in the record.
        assert_eq!(r.occupants().len(), 5 + 3);
    }

    #[test]
    fn a_set_with_no_pattern_left_names_the_member_and_the_reason() {
        let mut r = chart(&[100.0, 60.0, 120.0, 192.0]);
        note_over_b1(&mut r);
        let before = r.occupants().len();
        let failure = place_set(&mut r, &value_labels(4), PLOT, Level::Preferred).unwrap_err();
        assert_eq!(failure.set, Id::new("values"));
        let stops: Vec<_> = failure
            .stopped
            .iter()
            .map(|s| (s.pattern, s.member.as_str()))
            .collect();
        assert_eq!(stops, [(0, "b1/value"), (1, "b3/value")]);
        assert!(matches!(
            &failure.stopped[0].reason,
            Reason::Rejected(Finding::Meets {
                against: Against::Occupant(on),
                verdict: Verdict::Never,
                ..
            }) if on.as_str() == "note"
        ));
        // b3 is 8 px tall: its label cannot lie inside it.
        assert!(matches!(
            failure.stopped[1].reason,
            Reason::NotInside { .. }
        ));
        assert_eq!(r.occupants().len(), before);
    }

    #[test]
    fn a_pattern_last_in_order_waits_behind_one_in_order() {
        let chart_with_path = || {
            let mut r = chart(&[100.0]);
            r.enter(Occupant::new(
                Id::new("path"),
                Kind::ReadingPath,
                Ink::Rect(Rect::new(0.0, 0.0, 200.0, 95.0)),
            ));
            r
        };
        let mut set = value_labels(1);
        // Beyond the end stands in the reading path; beside the bar does not.
        set.patterns[1] = Pattern::new(
            "beside the bar",
            Attach::Whole,
            Side::Right,
            Alignment::Center,
        );
        let mut r = chart_with_path();
        let placed = place_set(&mut r, &set, PLOT, Level::Preferred).unwrap();
        assert_eq!(
            (placed.pattern, placed.preference),
            (1, Preference::InOrder)
        );
        // With no pattern in order left, the one last in order is taken.
        set.patterns.truncate(1);
        let mut r = chart_with_path();
        let placed = place_set(&mut r, &set, PLOT, Level::Preferred).unwrap();
        assert_eq!(
            (placed.pattern, placed.preference),
            (0, Preference::LastInOrder)
        );
        assert!(r.get(&Id::new("b0/value")).is_some());
    }

    #[test]
    fn squeezing_waits_for_the_strong_level() {
        let mut r = chart(&[100.0]);
        let mut set = value_labels(1);
        set.patterns.truncate(1);
        set.patterns[0].gap = Gap::Length(3.0);
        // 3.0 px from its own bar is under the related ideal of 3.7 px.
        let failure = place_set(&mut r, &set, PLOT, Level::Mild).unwrap_err();
        assert!(matches!(
            failure.stopped[0].reason,
            Reason::Rejected(Finding::Squeezed { .. })
        ));
        let placed = place_set(&mut r, &set, PLOT, Level::Strong).unwrap();
        assert_eq!(placed.labels[0].judgement.squeezes().count(), 1);
    }

    #[test]
    fn a_label_with_no_room_or_no_owner_stops_its_pattern() {
        let mut r = chart(&[2.0]);
        let mut set = value_labels(1);
        set.patterns.truncate(1);
        let failure = place_set(&mut r, &set, PLOT, Level::Preferred).unwrap_err();
        assert!(matches!(
            failure.stopped[0].reason,
            Reason::NoRoom { offered: None }
        ));
        set.members[0].owner = Id::new("missing");
        let failure = place_set(&mut r, &set, PLOT, Level::Preferred).unwrap_err();
        assert_eq!(failure.stopped[0].reason, Reason::OwnerNotRecorded);
    }

    // ----- review findings 7, 8, 9 and the interface risk of C9 -----

    const RELATED: f64 = 0.336 * 11.0;

    fn inside_only(count: usize) -> LabelSet {
        let mut set = value_labels(count);
        set.patterns.remove(0);
        set
    }

    /// A line across the plot; a value line is a gridline unless it is
    /// given another role with [`line_across`].
    fn gridline(r: &mut Record, name: &str, kind: Kind, y: f64) {
        let role = (kind == Kind::ValueLine).then(Role::gridline);
        line_across(r, name, kind, role, y);
    }

    fn line_across(r: &mut Record, name: &str, kind: Kind, role: Option<Role>, y: f64) {
        let line = Occupant::new(
            Id::new(name),
            kind,
            Ink::segment(Point::new(0.0, y), Point::new(200.0, y), 1.0),
        );
        r.enter(match role {
            Some(role) => line.with_role(role),
            None => line,
        });
    }

    #[test]
    fn an_inside_label_must_lie_within_its_bar() {
        // Review 7: an 11 px label inside an 8 px bar hung 6.7 px outside
        // and was accepted (C2).
        let mut r = bars(&[192.0]);
        let failure = place_set(&mut r, &inside_only(1), PLOT, Level::AllPermitted).unwrap_err();
        let Reason::NotInside { label, within } = failure.stopped[0].reason else {
            panic!("{failure:?}")
        };
        assert!((label.bottom() - BASELINE - 6.696).abs() < 1e-9);
        // The least distance (2.2 px) from the bar's other edges.
        assert!((within.bottom() - (BASELINE - 2.2)).abs() < 1e-9);
        assert!((within.x - 12.2).abs() < 1e-9 && within.y == 192.0);
        // A bar 18 px tall holds it: 3.696 + 11 + 2.2 < 18.
        let mut r = bars(&[182.0]);
        assert!(place_set(&mut r, &inside_only(1), PLOT, Level::Preferred).is_ok());
        // A bar 16 px tall does not.
        let mut r = bars(&[184.0]);
        assert!(place_set(&mut r, &inside_only(1), PLOT, Level::Preferred).is_err());
    }

    #[test]
    fn a_label_inside_its_bar_crosses_only_gridlines_the_bar_covers() {
        // Review 7, C5: a value label inside its bar at a gridline's height
        // was rejected. The gridline at 110 runs under b0 (top 100) and b1.
        let mut r = chart(&[100.0, 60.0, 120.0]);
        gridline(&mut r, "grid-110", Kind::ValueLine, 110.0);
        let placed = judge_set(&r, &inside_only(3), PLOT, Level::Preferred).unwrap();
        assert_eq!(placed.labels[0].rect.y, 100.0 + RELATED);
        assert_eq!(placed.preference, Preference::InOrder);
        // A series' line is not drawn beneath the bars: it stops the set.
        gridline(&mut r, "series", Kind::DataLine, 108.0);
        let failure = judge_set(&r, &inside_only(3), PLOT, Level::Preferred).unwrap_err();
        assert!(matches!(
            &failure.stopped[0],
            Stopped {
                member,
                reason: Reason::Rejected(Finding::Meets {
                    against: Against::Occupant(on),
                    verdict: Verdict::Never,
                    ..
                }),
                ..
            } if member.as_str() == "b0/value" && on.as_str() == "series"
        ));
    }

    #[test]
    fn a_label_inside_its_bar_is_judged_against_a_comparison_line_across_the_bar() {
        // Second review, item 5: only gridlines and the zero line are
        // beneath the bars. A comparison line at 110 crosses b0 (top 100)
        // where the inside label stands, and is drawn over it.
        let mut r = chart(&[100.0, 60.0, 120.0]);
        line_across(
            &mut r,
            "target",
            Kind::ValueLine,
            Some(Role::comparison()),
            110.0,
        );
        let failure = judge_set(&r, &inside_only(3), PLOT, Level::Preferred).unwrap_err();
        assert!(matches!(
            &failure.stopped[0],
            Stopped {
                member,
                reason: Reason::Rejected(Finding::Meets {
                    against: Against::Occupant(on),
                    verdict: Verdict::Never,
                    ..
                }),
                ..
            } if member.as_str() == "b0/value" && on.as_str() == "target"
        ));
        // The same line as a gridline is covered by the bar and accepted.
        let mut r = chart(&[100.0, 60.0, 120.0]);
        gridline(&mut r, "target", Kind::ValueLine, 110.0);
        assert!(judge_set(&r, &inside_only(3), PLOT, Level::Preferred).is_ok());
    }

    #[test]
    fn a_set_judged_before_the_record_changed_cannot_be_entered() {
        // Second review, item 8, C9: two sets judged against one record
        // and both entered overlapped with no finding. Here both are the
        // same set, judged at the same revision.
        let mut r = chart(&[100.0, 60.0, 120.0]);
        let first = judge_set(&r, &value_labels(3), PLOT, Level::Preferred).unwrap();
        let second = judge_set(&r, &value_labels(3), PLOT, Level::Preferred).unwrap();
        assert_eq!(first.judged_at, r.revision());
        enter_set(&mut r, &first).unwrap();
        let after_first = r.occupants().len();
        let err = enter_set(&mut r, &second).unwrap_err();
        assert_eq!(err.judged_at, second.judged_at);
        assert_ne!(err.now, err.judged_at);
        // Nothing was entered by the failed attempt.
        assert_eq!(r.occupants().len(), after_first);
    }

    /// Notes beyond the end of b0 (top 100) cross a gridline at 90; beside
    /// the bar they stand clear.
    fn notes_with_gridline_and_side(reading_path: bool) -> (Record, LabelSet) {
        let mut r = chart(&[100.0]);
        gridline(&mut r, "grid-90", Kind::ValueLine, 90.0);
        if reading_path {
            r.enter(Occupant::new(
                Id::new("path"),
                Kind::ReadingPath,
                Ink::Rect(Rect::new(30.0, 0.0, 170.0, 220.0)),
            ));
        }
        let mut set = value_labels(1);
        set.kind = Kind::Words;
        set.patterns[1] = Pattern::new(
            "beside the bar",
            Attach::Whole,
            Side::Right,
            Alignment::Center,
        );
        (r, set)
    }

    #[test]
    fn a_pattern_that_needs_a_halo_waits_behind_a_clean_one() {
        // Review 8, C3: "move first ... else a halo" counted as an ordinary
        // place, so a pattern needing a halo beat a later clean one.
        let (r, set) = notes_with_gridline_and_side(false);
        let placed = judge_set(&r, &set, PLOT, Level::Preferred).unwrap();
        assert_eq!(
            (placed.pattern, placed.preference),
            (1, Preference::InOrder)
        );
        // It still comes before a place last in order.
        let (r, set) = notes_with_gridline_and_side(true);
        let placed = judge_set(&r, &set, PLOT, Level::Preferred).unwrap();
        assert_eq!(
            (placed.pattern, placed.preference),
            (0, Preference::ElseHalo)
        );
        assert_eq!(
            placed.labels[0].judgement.verdicts().collect::<Vec<_>>(),
            [Verdict::MoveFirstElseHalo]
        );
    }

    /// Bars above and below a zero line at 100.
    fn both_ways() -> (Record, LabelSet) {
        let mut r = bars(&[]);
        r.enter(Occupant::new(
            Id::new("up"),
            Kind::DataMark,
            Ink::Rect(Rect::new(10.0, 60.0, 20.0, 40.0)),
        ));
        r.enter(Occupant::new(
            Id::new("down"),
            Kind::DataMark,
            Ink::Rect(Rect::new(50.0, 100.0, 20.0, 40.0)),
        ));
        gridline(&mut r, "zero", Kind::ValueLine, 100.0);
        let member = |owner: &str, end| {
            Member::new(Id::new(format!("{owner}/value")), Id::new(owner), LABEL).pointing(end)
        };
        let set = LabelSet {
            id: Id::new("values"),
            kind: Kind::Number,
            role: None,
            members: vec![member("up", Side::Above), member("down", Side::Below)],
            patterns: vec![
                Pattern::beyond_end("beyond the end", Alignment::Center),
                Pattern::inside_end("inside the end", Alignment::Center),
            ],
        };
        (r, set)
    }

    #[test]
    fn bars_above_and_below_zero_share_one_pattern() {
        // Review 9, C4.
        let (r, set) = both_ways();
        let placed = judge_set(&r, &set, PLOT, Level::Preferred).unwrap();
        assert_eq!(placed.pattern, 0);
        let rects: Vec<_> = placed.labels.iter().map(|l| l.rect).collect();
        assert_eq!(
            rects,
            [
                Rect::new(13.0, 60.0 - RELATED - 11.0, 14.0, 11.0),
                Rect::new(53.0, 140.0 + RELATED, 14.0, 11.0),
            ]
        );
        let mut set = set;
        set.patterns.remove(0);
        let placed = judge_set(&r, &set, PLOT, Level::Preferred).unwrap();
        let ys: Vec<_> = placed.labels.iter().map(|l| l.rect.y).collect();
        assert_eq!(ys, [60.0 + RELATED, 140.0 - RELATED - 11.0]);
        // A member that does not say which way it points stops the pattern.
        set.members[1].end = None;
        let failure = judge_set(&r, &set, PLOT, Level::Preferred).unwrap_err();
        assert_eq!(failure.stopped[0].reason, Reason::NoEnd);
    }

    #[test]
    fn an_end_label_anchors_on_its_lines_end() {
        // Review 9: an end label anchored on the line's bounding box, so it
        // stood level with the middle of a rising line.
        let mut r = bars(&[]);
        r.enter(Occupant::new(
            Id::new("series"),
            Kind::DataLine,
            Ink::segment(Point::new(0.0, 150.0), Point::new(150.0, 50.0), 2.0),
        ));
        let set = LabelSet {
            id: Id::new("ends"),
            kind: Kind::Words,
            role: None,
            members: vec![
                Member::new(
                    Id::new("series/name"),
                    Id::new("series"),
                    Size::new(30.0, 11.0),
                )
                .pointing(Side::Right),
            ],
            patterns: vec![Pattern::beyond_end("at the end", Alignment::Center)],
        };
        let placed = judge_set(&r, &set, PLOT, Level::Preferred).unwrap();
        let rect = placed.labels[0].rect;
        assert_eq!(rect, Rect::new(151.0 + RELATED, 44.5, 30.0, 11.0));
        assert_eq!(placed.labels[0].judgement.findings, []);
    }

    #[test]
    fn judging_a_set_leaves_the_record_unchanged() {
        // Interface risk, C9: place_set changed the record.
        let r = {
            let mut r = chart(&[100.0, 60.0, 120.0]);
            note_over_b1(&mut r);
            r
        };
        let before: Vec<_> = r.occupants().to_vec();
        let judged = judge_set(&r, &value_labels(3), PLOT, Level::Preferred).unwrap();
        assert_eq!(judged.pattern, 1);
        assert_eq!(r.occupants(), &before[..]);
        // Entering is the separate step, and place_set does both.
        let mut entered = r.clone();
        enter_set(&mut entered, &judged).unwrap();
        let mut placed_into = r.clone();
        let placed = place_set(&mut placed_into, &value_labels(3), PLOT, Level::Preferred).unwrap();
        assert_eq!(placed, judged);
        assert_eq!(entered.occupants(), placed_into.occupants());
        assert_eq!(entered.occupants().len(), before.len() + 3);
        // Later members are judged against earlier ones before any enters.
        let mut crowded = value_labels(2);
        crowded.members[1].owner = Id::new("b0");
        let failure = judge_set(&r, &crowded, PLOT, Level::Preferred).unwrap_err();
        assert!(matches!(
            &failure.stopped[0].reason,
            Reason::Rejected(Finding::Meets { against: Against::Occupant(on), .. })
                if on.as_str() == "b0/value"
        ));
    }

    #[test]
    fn the_room_a_pattern_needs_is_known_before_anything_is_placed() {
        // Interface risk, C9: no pure query for the room a pattern needs.
        let rules = Rules::default();
        let sizes = Sizes {
            label_size: 11.0,
            word_space: 3.7,
            line_width: 1.0,
        };
        let (_, set) = both_ways();
        assert_eq!(
            room_needed(&set, 0, &rules, &sizes, |_| 20.0)
                .unwrap()
                .beyond,
            Room {
                above: RELATED + 11.0,
                below: RELATED + 11.0,
                ..Room::default()
            }
        );
        assert_eq!(
            room_needed(&set, 1, &rules, &sizes, |_| 20.0)
                .unwrap()
                .beyond,
            Room::default()
        );
        let mut beside = value_labels(2);
        beside.patterns[0] = Pattern::new("right", Attach::Whole, Side::Right, Alignment::Center)
            .gap(Gap::Unrelated);
        assert_eq!(
            room_needed(&beside, 0, &rules, &sizes, |_| 20.0)
                .unwrap()
                .beyond,
            Room {
                right: 0.66 * 11.0 + 14.0,
                ..Room::default()
            }
        );
        assert_eq!(
            room_needed(&beside, 1, &rules, &sizes, |_| 20.0)
                .unwrap()
                .beyond,
            Room::default()
        );
        let (_, mut set) = both_ways();
        set.members[0].end = None;
        assert_eq!(
            room_needed(&set, 0, &rules, &sizes, |_| 20.0)
                .unwrap_err()
                .reason,
            Reason::NoEnd
        );
    }

    // ----- labels whose positions nearly agree share one line (U8) -----

    /// Value labels beyond the ends of a chart's bars, sharing lines.
    fn sharing(count: usize) -> LabelSet {
        let mut set = value_labels(count);
        set.patterns.truncate(1);
        set.patterns[0] = set.patterns[0].clone().sharing_lines();
        set
    }

    fn feet(placed: &Placed) -> Vec<f64> {
        placed.labels.iter().map(|l| l.rect.bottom()).collect()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn worked_example_value_labels_on_two_bars() {
        // The module's worked example. Her 11 px text: least 2.2, related
        // 3.696, near 5.896.
        let r = chart(&[100.0, 101.5]);
        let own = judge_set(&r, &value_labels(2), PLOT, Level::Preferred).unwrap();
        assert_eq!(
            own.labels[0].rect,
            Rect::new(13.0, 100.0 - RELATED - 11.0, 14.0, 11.0)
        );
        let f = feet(&own);
        assert!(close(f[0], 96.304) && close(f[1], 97.804));
        assert_eq!(own.lines, []);
        // Sharing lines: 1.5 px apart, less than 2.2, so one group, on the
        // outermost foot, 96.304; b1's label moves up 1.5 px and is 5.196
        // from its bar, within near.
        let shared = judge_set(&r, &sharing(2), PLOT, Level::Preferred).unwrap();
        let f = feet(&shared);
        assert!(close(f[0], 96.304) && close(f[1], 96.304));
        assert!(close(101.5 - f[1], 5.196));
        let [line] = &shared.lines[..] else {
            panic!("{:?}", shared.lines)
        };
        assert_eq!(line.members, [Id::new("b1/value"), Id::new("b0/value")]);
        assert_eq!(line.side, Side::Above);
        assert!(matches!(line.outcome, LineOutcome::Shared { at } if close(at, 96.304)));
        assert_eq!(shared.preference, Preference::InOrder);
        // 3 px apart is not less than the least distance: each its own.
        let r = chart(&[100.0, 103.0]);
        let kept = judge_set(&r, &sharing(2), PLOT, Level::Preferred).unwrap();
        let f = feet(&kept);
        assert!(close(f[0], 96.304) && close(f[1], 99.304));
        assert_eq!(kept.lines, []);
        // The room the pattern needs: 3.696 + 11 above the bars, and 14 px
        // labels on 20 px bars reach nothing across. 30 px labels reach 5
        // past each side of their bars.
        let rules = Rules::default();
        let sizes = *r.sizes();
        let room = room_needed(&sharing(2), 0, &rules, &sizes, |_| 20.0).unwrap();
        assert!(close(room.beyond.above, 14.696));
        assert_eq!(room.across, Room::default());
        let mut wide = sharing(2);
        for member in &mut wide.members {
            member.size.width = 30.0;
        }
        let room = room_needed(&wide, 0, &rules, &sizes, |_| 20.0).unwrap();
        assert_eq!(
            room.across,
            Room {
                left: 5.0,
                right: 5.0,
                ..Room::default()
            }
        );
    }

    #[test]
    fn every_step_below_the_least_distance_is_removed_under_the_default_rules() {
        // Near is the related ideal plus the least distance, so labels at
        // the related ideal whose positions differ by anything less than
        // the least distance (2.2) share the outermost one's line: the
        // farther label ends less than near from its bar, and no label is
        // brought nearer its bar than the related ideal.
        for step in [0.5, 1.5, 1.95, 2.19] {
            let r = chart(&[100.0, 100.0 + step]);
            let shared = judge_set(&r, &sharing(2), PLOT, Level::Preferred).unwrap();
            let f = feet(&shared);
            assert!(close(f[0], 96.304) && close(f[1], 96.304), "step {step}");
            assert!(matches!(
                shared.lines[0].outcome,
                LineOutcome::Shared { .. }
            ));
        }
    }

    #[test]
    fn sharing_lines_is_off_by_default() {
        let set = value_labels(2);
        assert!(set.patterns.iter().all(|p| !p.share_line));
        let r = chart(&[100.0, 101.5]);
        let placed = judge_set(&r, &set, PLOT, Level::Preferred).unwrap();
        assert!(close(feet(&placed)[1] - feet(&placed)[0], 1.5));
    }

    #[test]
    fn a_group_that_would_leave_a_label_too_near_its_bars_edge_keeps_its_positions() {
        // Inside their bars' tops: a's bar is 17 px tall, b's starts 1.5 px
        // lower. The line is b's label's top, 105.196; on it a's label
        // would end at 116.196, 0.804 from its bar's foot at 117, nearer
        // than the least distance, 2.2.
        let mut r = bars(&[]);
        for (name, rect) in [
            ("a", Rect::new(10.0, 100.0, 20.0, 17.0)),
            ("b", Rect::new(50.0, 101.5, 20.0, 98.5)),
        ] {
            r.enter(Occupant::new(
                Id::new(name),
                Kind::DataMark,
                Ink::Rect(rect),
            ));
        }
        let member =
            |owner: &str| Member::new(Id::new(format!("{owner}/value")), Id::new(owner), LABEL);
        let set = LabelSet {
            id: Id::new("values"),
            kind: Kind::Number,
            role: None,
            members: vec![member("a"), member("b")],
            patterns: vec![
                Pattern::new(
                    "inside",
                    Attach::Edge(Side::Above),
                    Side::Below,
                    Alignment::Center,
                )
                .sharing_lines(),
            ],
        };
        let placed = judge_set(&r, &set, PLOT, Level::Preferred).unwrap();
        let tops: Vec<_> = placed.labels.iter().map(|l| l.rect.y).collect();
        assert!(close(tops[0], 103.696) && close(tops[1], 105.196));
        assert_eq!(
            placed.lines[0].outcome,
            LineOutcome::Kept(KeptBecause::NotInside {
                member: Id::new("a/value")
            })
        );
    }

    #[test]
    fn a_group_that_would_lose_its_clearance_keeps_its_positions() {
        // A note 1 px above where b1's label would stand on the shared line
        // (its top, 85.304): there it is below the least distance from the
        // note; at its own place, 2.5 px off, it is only squeezed, which
        // the strong level permits.
        let mut r = chart(&[100.0, 101.5]);
        r.enter(Occupant::new(
            Id::new("note"),
            Kind::Words,
            Ink::Rect(Rect::new(53.0, 70.0, 14.0, 85.304 - 1.0 - 70.0)),
        ));
        let placed = judge_set(&r, &sharing(2), PLOT, Level::Strong).unwrap();
        assert!(close(feet(&placed)[1], 97.804));
        assert!(matches!(
            &placed.lines[0].outcome,
            LineOutcome::Kept(KeptBecause::Clearance {
                member,
                finding: Some(Finding::BelowLeast { .. }),
            }) if member.as_str() == "b1/value"
        ));
    }

    #[test]
    fn a_group_with_no_line_within_near_of_every_mark_keeps_its_positions() {
        // With near set below the least distance no line keeps a label
        // between the two: ownership comes before sharing a line.
        let mut r = chart(&[100.0, 101.5]);
        r.rules_mut().distances.diagram.near = crate::rules::Measure::em(0.1);
        let placed = judge_set(&r, &sharing(2), PLOT, Level::Preferred).unwrap();
        assert!(close(feet(&placed)[1], 97.804));
        assert_eq!(
            placed.lines[0].outcome,
            LineOutcome::Kept(KeptBecause::NoLine)
        );
    }

    #[test]
    fn labels_reach_past_their_owners_across_by_their_alignment() {
        let rules = Rules::default();
        let sizes = Sizes {
            label_size: 11.0,
            word_space: 3.7,
            line_width: 1.0,
        };
        let mut set = value_labels(1);
        set.members[0].size = Size::new(30.0, 11.0);
        let across = |set: &LabelSet, pattern, owner: f64| {
            room_needed(set, pattern, &rules, &sizes, |_| owner)
                .unwrap()
                .across
        };
        // Centred, on a 20 px bar: 5 past each side, also when inside.
        let both = Room {
            left: 5.0,
            right: 5.0,
            ..Room::default()
        };
        assert_eq!(across(&set, 0, 20.0), both);
        assert_eq!(across(&set, 1, 20.0), both);
        // Aligned with the bar's start, it reaches 10 past the end; with
        // its end, 10 past the start.
        set.patterns[0].alignment = Alignment::Start;
        assert_eq!(across(&set, 0, 20.0).right, 10.0);
        set.patterns[0].alignment = Alignment::End;
        assert_eq!(across(&set, 0, 20.0).left, 10.0);
        // Beside a bar 8 px tall, an 11 px label reaches 1.5 above and below.
        set.patterns[0] = Pattern::new("right", Attach::Whole, Side::Right, Alignment::Center);
        assert_eq!(
            across(&set, 0, 8.0),
            Room {
                above: 1.5,
                below: 1.5,
                ..Room::default()
            }
        );
        // On a 40 px bar it reaches nothing.
        assert_eq!(across(&set, 0, 40.0), Room::default());
    }

    // ----- halos (design §7, §9) -----

    /// A line from `from` to `to`, a gridline, 1 px wide.
    fn line(r: &mut Record, name: &str, from: (f64, f64), to: (f64, f64)) {
        r.enter(
            Occupant::new(
                Id::new(name),
                Kind::ValueLine,
                Ink::segment(Point::new(from.0, from.1), Point::new(to.0, to.1), 1.0),
            )
            .with_role(Role::gridline()),
        );
    }

    /// A note set (words) over the chart's bars, by `patterns`.
    fn notes(count: usize, patterns: Vec<Pattern>) -> LabelSet {
        let mut set = value_labels(count);
        set.kind = Kind::Words;
        set.patterns = patterns;
        set
    }

    fn above(name: &str, alignment: Alignment, gap: Gap) -> Pattern {
        Pattern::new(name, Attach::Whole, Side::Above, alignment).gap(gap)
    }

    fn cut_xs(label: &PlacedLabel) -> Vec<(f64, f64)> {
        label.cuts().iter().map(|c| (c.from.x, c.to.x)).collect()
    }

    #[test]
    fn words_move_when_a_clean_place_exists_and_are_haloed_when_none_does() {
        // The note beyond b0's end (13..27, 85.3..96.3) stands in the
        // gridline at 90; beside the bar it is clear.
        let (r, set) = notes_with_gridline_and_side(false);
        let moved = judge_set(&r, &set, PLOT, Level::Preferred).unwrap();
        assert_eq!((moved.pattern, moved.preference), (1, Preference::InOrder));
        assert_eq!(moved.labels[0].cuts(), []);
        // With no clean place it stays and the gridline is cut round it:
        // from 1.65 px before the note to 1.65 px after.
        let (mut r, mut set) = notes_with_gridline_and_side(false);
        set.patterns.truncate(1);
        let haloed = place_set(&mut r, &set, PLOT, Level::Preferred).unwrap();
        assert_eq!(
            (haloed.pattern, haloed.preference),
            (0, Preference::ElseHalo)
        );
        let [(from, to)] = cut_xs(&haloed.labels[0])[..] else {
            panic!("{haloed:?}")
        };
        assert!(close(from, 11.35) && close(to, 28.65));
        // Entering the set entered the cut: the caller draws the line
        // broken there.
        let cuts: Vec<_> = r.cuts_of(&Id::new("grid-90")).collect();
        assert_eq!(cuts, haloed.labels[0].cuts().iter().collect::<Vec<_>>());
    }

    #[test]
    fn a_number_is_never_haloed_into_a_line_not_its_own() {
        // Value labels beyond the bars stand in a gridline at 90; inside
        // the bars they are clear. The halo is not offered to a number.
        let mut r = chart(&[100.0, 60.0, 120.0]);
        line(&mut r, "grid-90", (0.0, 90.0), (200.0, 90.0));
        let placed = judge_set(&r, &value_labels(3), PLOT, Level::Preferred).unwrap();
        assert_eq!(
            (placed.pattern, placed.preference),
            (1, Preference::InOrder)
        );
        let mut only_beyond = value_labels(3);
        only_beyond.patterns.truncate(1);
        let failure = judge_set(&r, &only_beyond, PLOT, Level::AllPermitted).unwrap_err();
        assert!(matches!(
            &failure.stopped[0],
            Stopped {
                member,
                reason: Reason::Rejected(Finding::Meets {
                    against: Against::Occupant(on),
                    verdict: Verdict::Never,
                    ..
                }),
                ..
            } if member.as_str() == "b0/value" && on.as_str() == "grid-90"
        ));
    }

    #[test]
    fn a_cut_at_the_lines_end_is_refused_and_one_near_it_comes_later_in_order() {
        // b0 spans 10..30 and its note's place above it, aligned with the
        // bar's start, is 10..24; aligned with its end, 16..30.
        let start = || above("above, from the start", Alignment::Start, Gap::Related);
        let end = || above("above, to the end", Alignment::End, Gap::Related);
        // A line from 0: the start-aligned cut begins at 8.35, near the
        // end (within 11 px); the end-aligned one at 14.35, not near.
        let mut r = chart(&[100.0]);
        line(&mut r, "grid", (0.0, 90.0), (200.0, 90.0));
        let placed =
            judge_set(&r, &notes(1, vec![start(), end()]), PLOT, Level::Preferred).unwrap();
        assert_eq!(
            (placed.pattern, placed.preference),
            (1, Preference::ElseHalo)
        );
        let alone = judge_set(&r, &notes(1, vec![start()]), PLOT, Level::Preferred).unwrap();
        assert_eq!(alone.preference, Preference::LessPreferred);
        assert!(close(cut_xs(&alone.labels[0])[0].0, 8.35));
        // A line from 12: the start-aligned cut would reach its end; the
        // end-aligned one comes 2.35 px from it, near the end, and is taken.
        let mut r = chart(&[100.0]);
        line(&mut r, "grid", (12.0, 90.0), (200.0, 90.0));
        let set = notes(1, vec![start(), end()]);
        let placed = judge_set(&r, &set, PLOT, Level::Preferred).unwrap();
        assert_eq!(
            (placed.pattern, placed.preference),
            (1, Preference::LessPreferred)
        );
        let failure =
            judge_set(&r, &notes(1, vec![start()]), PLOT, Level::AllPermitted).unwrap_err();
        assert!(matches!(
            &failure.stopped[0].reason,
            Reason::Rejected(Finding::Cut {
                at: crate::rules::CutAt::End,
                verdict: Verdict::Never,
                ..
            })
        ));
        // An upright line at x = 20 crosses inside the end-aligned cut,
        // which leaves no place (the start-aligned one is still stopped
        // first by the line's end).
        line(&mut r, "upright", (20.0, 0.0), (20.0, 99.0));
        let failure = judge_set(&r, &set, PLOT, Level::AllPermitted).unwrap_err();
        assert!(matches!(
            &failure.stopped[1].reason,
            Reason::Rejected(Finding::Cut {
                against: Against::Occupant(on),
                at: crate::rules::CutAt::Crossing,
                by: Some(by),
                ..
            }) if on.as_str() == "grid" && by.as_str() == "upright"
        ));
    }

    #[test]
    fn a_set_that_would_mix_moving_and_haloing_haloes_every_member_or_moves_every_one() {
        // Two notes over bars b0 (10..30) and b1 (50..70), both tops at 100.
        // Above, at the related gap, both stand in the line at 90. 25 px
        // up, b0's note is clear and b1's stands in a line at 69.5 that
        // starts at 35. Chosen per label, b0's note would move up and
        // b1's be haloed where it is: the set haloes both instead.
        let mut r = chart(&[100.0, 100.0]);
        line(&mut r, "low", (-200.0, 90.0), (300.0, 90.0));
        line(&mut r, "high", (35.0, 69.5), (300.0, 69.5));
        let patterns = vec![
            above("above", Alignment::Center, Gap::Related),
            above("further above", Alignment::Center, Gap::Length(25.0)),
        ];
        let set = notes(2, patterns.clone());
        let placed = place_set(&mut r.clone(), &set, PLOT, Level::Preferred).unwrap();
        assert_eq!(placed.pattern, 0);
        let lines: Vec<Vec<&str>> = placed
            .labels
            .iter()
            .map(|l| l.cuts().iter().map(|c| c.line.as_str()).collect())
            .collect();
        assert_eq!(lines, [["low"], ["low"]]);
        // Two cuts in one line: the second note is judged against the line
        // as the first leaves it, and stands clear of the first's cut.
        assert!(close(cut_xs(&placed.labels[1])[0].0, 51.35));
        // With a place clear of every line for both, the set moves both.
        let mut set = set;
        set.patterns
            .push(above("well above", Alignment::Center, Gap::Length(40.0)));
        let placed = place_set(&mut r, &set, PLOT, Level::Preferred).unwrap();
        assert_eq!(
            (placed.pattern, placed.preference),
            (2, Preference::InOrder)
        );
        assert!(placed.labels.iter().all(|l| l.cuts().is_empty()));
        assert_eq!(r.cuts(), []);
    }
}
