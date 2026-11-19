// SPDX-License-Identifier: GPL-3.0-only

//! Anchors, and sets of labels placed by pattern (design §9).
//!
//! The anchor equation is household-widgets' (`layout_popup.rs`: `Side`,
//! `Alignment`, `AnchorCandidate`, `offer`, `origin`) in `f64`, bounded by
//! a region the caller gives rather than a window.
//!
//! A set of labels (value labels, end labels, slice labels, empty-slot
//! notes) takes one pattern for every member: "it is just really
//! inconsistent to have some labels to the left and some to the right". A
//! [`Pattern`] is one place, applied to each member's owner, so a mixed set
//! cannot be expressed; a place can follow the data ("beyond the end"), so
//! bars above and below zero share one pattern. The patterns are tried in
//! the caller's order. If any member is rejected, the pattern is; the first
//! pattern with no rejection is taken. A pattern that some member can take
//! only with a halo it could have moved away from, at a less preferred
//! place, or last in order, waits behind every pattern that needs none of
//! these. A set with no pattern left says which member stopped each pattern
//! and why, so that its owner can take its next form.
//!
//! Judging a set ([`judge_set`]) changes nothing; [`enter_set`] enters the
//! result; [`place_set`] does both. [`room_needed`] says, before anything is
//! placed, how much room a pattern needs beyond its owners. Nothing is moved
//! after it is placed.

use crate::geometry::{Point, Rect, Size};
use crate::identity::Id;
use crate::kinds::{Kind, Role};
use crate::level::Level;
use crate::record::{Finding, Ink, Judgement, Occupant, Record};
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
            Self::Beside { on, side } => Some((
                side,
                matches!(on, Attach::Edge(e) if e == side.opposite()),
            )),
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
        }
    }

    pub fn gap(mut self, gap: Gap) -> Self {
        self.gap = gap;
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

/// A set placed by one pattern: judged, and not yet entered unless
/// [`place_set`] made it.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    /// The pattern's index in the set's list.
    pub pattern: usize,
    pub preference: Preference,
    pub labels: Vec<PlacedLabel>,
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
    for index in 0..set.patterns.len() {
        match judge_pattern(record, set, index, region, level) {
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

/// Enters a judged set's labels in the record, in the members' order.
pub fn enter_set(record: &mut Record, placed: &Placed) {
    record.enter_all(placed.labels.iter().map(|label| label.occupant.clone()));
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
    enter_set(record, &placed);
    Ok(placed)
}

/// The room a pattern needs beyond its owners on each side.
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

/// The room pattern `pattern` of `set` needs beyond its owners, along the
/// side each label goes on: the gap and the label's depth, the most over
/// the members. A label inside its owner needs none. A pure query that
/// needs neither the record nor the owners, so a chart can reserve the room
/// before anything is placed (design §9, semantics C9). How far a label
/// wider than its owner reaches across depends on the owner's size and is
/// not counted. A member with no end stops a pattern that follows the data.
pub fn room_needed(
    set: &LabelSet,
    pattern: usize,
    rules: &Rules,
    sizes: &Sizes,
) -> Result<Room, Stopped> {
    let pattern_ref = &set.patterns[pattern];
    let gap = pattern_ref.gap.resolve(rules, sizes);
    let mut room = Room::default();
    for member in &set.members {
        let Some((side, inside)) = pattern_ref.place.side_for(member.end) else {
            return Err(Stopped {
                pattern,
                member: member.id.clone(),
                reason: Reason::NoEnd,
            });
        };
        if inside {
            continue;
        }
        let depth = match side {
            Side::Above | Side::Below => member.size.height,
            Side::Left | Side::Right => member.size.width,
        };
        room.widen(side, gap + depth);
    }
    Ok(room)
}

/// Judges every member by one pattern, each against the record and the
/// members before it; stops at the first rejection.
fn judge_pattern(
    record: &Record,
    set: &LabelSet,
    index: usize,
    region: Rect,
    level: Level,
) -> Result<Placed, Stopped> {
    let pattern = &set.patterns[index];
    let gap = pattern.gap.resolve(record.rules(), record.sizes());
    let mut labels: Vec<PlacedLabel> = Vec::with_capacity(set.members.len());
    let mut pending: Vec<Occupant> = Vec::with_capacity(set.members.len());
    let mut preference = Preference::InOrder;
    for member in &set.members {
        let stop = |reason| Stopped {
            pattern: index,
            member: member.id.clone(),
            reason,
        };
        let Some(owner) = record.get(&member.owner) else {
            return Err(stop(Reason::OwnerNotRecorded));
        };
        let Some((anchor, side, inside)) = pattern.place.resolve(&owner.ink, member.end) else {
            return Err(stop(Reason::NoEnd));
        };
        let candidate = AnchorCandidate::new(side, pattern.alignment).gap(gap);
        let offered = offer(region, anchor, candidate);
        if !offered.is_some_and(|room| member.size.fits(room)) {
            return Err(stop(Reason::NoRoom { offered }));
        }
        let rect = Rect::from_origin(origin(region, anchor, candidate, member.size), member.size);
        if let Some(edge) = inside {
            let within = inset_but(record, set.kind, owner, edge);
            if !within.contains(rect) {
                return Err(stop(Reason::NotInside {
                    label: rect,
                    within,
                }));
            }
        }
        let mut label = Occupant::new(member.id.clone(), set.kind, Ink::Rect(rect))
            .owned_by(member.owner.clone());
        label.role = set.role.clone();
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
    })
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
        r.enter(Occupant::new(
            Id::new("zero"),
            Kind::ValueLine,
            Ink::segment(Point::new(0.0, BASELINE), Point::new(200.0, BASELINE), 1.0),
        ));
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
}
