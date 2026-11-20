// SPDX-License-Identifier: GPL-3.0-only

//! The record of occupied space (design §7 and §11).
//!
//! The engine says where the boxes of a chart go; the record keeps what
//! has been drawn inside them, so that what is placed later does not land
//! on it or come too near it. Everything drawn is entered with its ink (a
//! rectangle or a line of some width), its [`Kind`], its identity, and the
//! identity of its owner if it has one: a value label is its bar's, a
//! series' name is its line's. Nothing entered is ever moved.
//!
//! Anything placed later asks the record first. [`Record::judge`] takes a
//! candidate and returns a [`Judgement`]: one [`Finding`] for each thing
//! the candidate meets or comes too near, each naming that thing and the
//! row of the tables that gave it, so that a caller can say why a place
//! was refused. The tables are the rules' data ([`Rules`]):
//!
//! - Where the candidate **meets** something, the covering table gives the
//!   verdict: never, allowed, allowed in the toggle's ink, last in order,
//!   and so on. Only `Never` rejects; the others say what the caller must
//!   do (draw a halo, use the toggle's ink) or how far down its list of
//!   places this one goes ([`Judgement::preference`]).
//! - Where it comes **near** without meeting, the distance table applies.
//!   Things that are each other's own keep the related ideal and other
//!   things the unrelated ideal. Below the least distance a place is
//!   rejected at every level. Between the least distance and the ideal it
//!   is squeezed: permitted from the rules' squeeze level (`Strong` by
//!   default) and flagged, so that squeezing is always a choice.
//!
//! Judging changes nothing. Entering is a separate step
//! ([`Record::enter`]), and every change to the record gives it a new
//! [`Revision`], so a judgement made before a change cannot be entered
//! after it by mistake (the label sets check this).
//!
//! Some pairs are judged otherwise. A thing is another's own only when one
//! owns the other (semantics C1); two labels of one bar are not each
//! other's own. The order of drawing is data (C5): a candidate standing on
//! a mark is not judged against a gridline the rules draw beneath that
//! mark and that passes under it. A pair the caller joins by a text gap is
//! judged by that text's own spacing instead (C6). A pair the covering
//! table says are meant to meet keeps no distance (C7). The record also
//! answers what is at a point, innermost first ([`Record::at`]), and which
//! lines pass under a rectangle ([`Record::lines_under`]).
//!
//! # A worked example
//!
//! Her 11 px text gives a least distance of 2.2 px and ideals of 3.696 px
//! (related) and 7.26 px (unrelated). A bar stands from (10, 100), 20 px
//! wide and 100 px tall, and is entered.
//!
//! - Its value label, 14 by 11 px and owned by the bar, centred 3.696 px
//!   above it at (13, 85.304): the label and the bar are each other's own,
//!   the gap is the related ideal, and there are no findings.
//! - The same label 0.8 px above the bar is below the least distance: a
//!   `BelowLeast` finding against the bar, which rejects it at every
//!   level.
//! - Once the label is entered at its ideal, a note of the same size 3 px
//!   above it is unrelated to it and is squeezed: 3 px is below the
//!   unrelated ideal of 7.26 px and above the least distance. The place is
//!   rejected at `Preferred` and permitted at `Strong`, flagged.
//!
//! (Checked by the test `worked_example_a_bar_its_label_and_a_note`.)
//!
//! # Halos
//!
//! Where a candidate meets a line by a verdict that needs a halo (words in
//! a value line, a number in its own value line, a series' name in its own
//! data line), the judgement also carries the cut the halo makes in the
//! line ([`Judgement::cuts`]) and a [`Finding::Cut`] for every row of the
//! cut table that applies: at a crossing, a bend or the line's end, never;
//! near an end or over a third of the line, less preferred. A caller that
//! takes the place enters the cuts with the label
//! ([`Record::enter_cuts`]; the label sets do it in
//! [`crate::labels::enter_set`]) and draws each line broken where its cuts
//! say ([`Record::cuts_of`]). A line that is cut stays one occupant; later
//! candidates are judged against the ink it still shows, and one standing
//! in another text's cut is refused. What a caller enters for a line, and
//! the geometry, are in [`crate::halo`].
//!
//! # A label on its own mark's line
//!
//! A pair the caller declares [`Relation::OwnLine`] (a label on its own
//! mark's line, and a mark neighbouring that mark across the line) keeps
//! the distance the table names for it, by default the least distance
//! only, and is never flagged as squeezed. The comment on the relation
//! says what the caller must be able to show to declare it.
//!
//! # Connectors
//!
//! A connector is entered as a segment owned by its text and by its target
//! ([`Occupant::also_owned_by`]); its target may be of any kind. Two
//! things about meeting are particular to lines:
//!
//! - **Running along is not crossing.** Where a segment meets a segment at
//!   less than the rules' [`crate::rules::Connectors::along_below`] and
//!   overlaps it along its length by more than the least distance, it runs
//!   along it ([`Record::runs_along`]), and the covering table's rows
//!   marked [`crate::rules::Contact::Along`] apply before the rest: "may
//!   cross, never runs along" is two rows, one for each.
//! - **A target covers what it lies on.** Where a connector meets an area
//!   only inside its own target, and the order of drawing puts that area
//!   beneath the target, the area is not judged: a leader ending on a
//!   comparison mark where the mark lies across its bar meets the mark.
//!
//! How a leader is built from these is in [`crate::leaders`].
//!
//! Not yet here: the record's judging puts no upper bound on a related
//! distance (Juliet's "clearly belongs" test, near and far; label sets
//! that share lines use near). This version searches a list; an index for
//! many marks can replace it behind the same methods.

use crate::geometry::{FIT_EPSILON, Point, Rect};
use crate::halo::{self, Cut, Line, Piece};
use crate::identity::Id;
use crate::kinds::{Kind, Role};
use crate::level::Level;
use crate::rules::{
    CoveringRef, CutAt, DistanceRow, Preference, Relation, Rules, Sizes, TextGap, TextMetrics,
    Verdict,
};

/// What an occupant draws, for the record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ink {
    /// A filled or bounded area: a text's box, a bar, a plate, a region.
    Rect(Rect),
    /// A straight line from `from` to `to`, `width` wide.
    Segment { from: Point, to: Point, width: f64 },
}

impl Ink {
    pub fn segment(from: Point, to: Point, width: f64) -> Self {
        Self::Segment { from, to, width }
    }

    /// The shortest distance between two inks: zero where they touch or
    /// overlap.
    pub fn gap(&self, other: &Ink) -> f64 {
        match (*self, *other) {
            (Ink::Rect(a), Ink::Rect(b)) => a.gap(b),
            (Ink::Rect(r), Ink::Segment { from, to, width })
            | (Ink::Segment { from, to, width }, Ink::Rect(r)) => {
                (segment_rect_distance(from, to, r) - width / 2.0).max(0.0)
            }
            (
                Ink::Segment {
                    from: a0,
                    to: a1,
                    width: wa,
                },
                Ink::Segment {
                    from: b0,
                    to: b1,
                    width: wb,
                },
            ) => (segment_segment_distance(a0, a1, b0, b1) - (wa + wb) / 2.0).max(0.0),
        }
    }

    /// Whether the two inks touch or overlap.
    pub fn meets(&self, other: &Ink) -> bool {
        self.gap(other) <= 0.0
    }

    /// Whether `point` lies on the ink.
    pub fn holds(&self, point: Point) -> bool {
        match *self {
            Ink::Rect(r) => r.holds(point),
            Ink::Segment { from, to, width } => {
                point_segment_distance(point, from, to) <= width / 2.0
            }
        }
    }

    /// The ink's area, for ordering what is at a point.
    pub fn area(&self) -> f64 {
        match *self {
            Ink::Rect(r) => r.width * r.height,
            Ink::Segment { from, to, width } => (to.x - from.x).hypot(to.y - from.y) * width,
        }
    }

    /// The smallest rectangle holding the ink.
    pub fn bounds(&self) -> Rect {
        match *self {
            Ink::Rect(r) => r,
            Ink::Segment { from, to, width } => {
                let half = width / 2.0;
                let x = from.x.min(to.x) - half;
                let y = from.y.min(to.y) - half;
                Rect::new(
                    x,
                    y,
                    from.x.max(to.x) + half - x,
                    from.y.max(to.y) + half - y,
                )
            }
        }
    }
}

fn point_segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length2 = dx * dx + dy * dy;
    let t = if length2 == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / length2).clamp(0.0, 1.0)
    };
    (p.x - (a.x + t * dx)).hypot(p.y - (a.y + t * dy))
}

fn cross(o: Point, a: Point, b: Point) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

fn segments_cross(a0: Point, a1: Point, b0: Point, b1: Point) -> bool {
    let d1 = cross(b0, b1, a0);
    let d2 = cross(b0, b1, a1);
    let d3 = cross(a0, a1, b0);
    let d4 = cross(a0, a1, b1);
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

fn segment_segment_distance(a0: Point, a1: Point, b0: Point, b1: Point) -> f64 {
    if segments_cross(a0, a1, b0, b1) {
        return 0.0;
    }
    point_segment_distance(a0, b0, b1)
        .min(point_segment_distance(a1, b0, b1))
        .min(point_segment_distance(b0, a0, a1))
        .min(point_segment_distance(b1, a0, a1))
}

/// The unit vector from `a` to `b`, if they differ.
pub(crate) fn unit(a: Point, b: Point) -> Option<Point> {
    let length = (b.x - a.x).hypot(b.y - a.y);
    (length > 0.0).then(|| Point::new((b.x - a.x) / length, (b.y - a.y) / length))
}

/// The stretch of the segment from `a` to `b` inside `r`, if any
/// (Liang and Barsky's clipping).
fn clip(a: Point, b: Point, r: Rect) -> Option<(Point, Point)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (mut low, mut high) = (0.0_f64, 1.0_f64);
    for (p, q) in [
        (-dx, a.x - r.x),
        (dx, r.right() - a.x),
        (-dy, a.y - r.y),
        (dy, r.bottom() - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else if p < 0.0 {
            low = low.max(q / p);
        } else {
            high = high.min(q / p);
        }
    }
    let at = |t: f64| Point::new(a.x + t * dx, a.y + t * dy);
    (low <= high).then(|| (at(low), at(high)))
}

fn segment_rect_distance(a: Point, b: Point, r: Rect) -> f64 {
    if r.holds(a) || r.holds(b) {
        return 0.0;
    }
    let top_left = Point::new(r.x, r.y);
    let top_right = Point::new(r.right(), r.y);
    let bottom_right = Point::new(r.right(), r.bottom());
    let bottom_left = Point::new(r.x, r.bottom());
    [
        (top_left, top_right),
        (top_right, bottom_right),
        (bottom_right, bottom_left),
        (bottom_left, top_left),
    ]
    .into_iter()
    .map(|(from, to)| segment_segment_distance(a, b, from, to))
    .fold(f64::INFINITY, f64::min)
}

/// A thing drawn, or a candidate for drawing: its ink, its kind, its
/// identity, its owner's identity if it has one, and its role where the
/// covering table needs one.
#[derive(Clone, Debug, PartialEq)]
pub struct Occupant {
    pub ink: Ink,
    pub kind: Kind,
    pub id: Id,
    /// Who owns this thing. A connector is both its text's own and its
    /// target's own, so there may be several (semantics C1).
    pub owners: Vec<Id>,
    pub role: Option<Role>,
}

impl Occupant {
    pub fn new(id: Id, kind: Kind, ink: Ink) -> Self {
        Self {
            ink,
            kind,
            id,
            owners: Vec::new(),
            role: None,
        }
    }

    /// This occupant owned by `owner` alone.
    pub fn owned_by(mut self, owner: Id) -> Self {
        self.owners = vec![owner];
        self
    }

    /// This occupant also owned by `owner`, besides any it has.
    pub fn also_owned_by(mut self, owner: Id) -> Self {
        if !self.owners.contains(&owner) {
            self.owners.push(owner);
        }
        self
    }

    /// The first owner, if any.
    pub fn owner(&self) -> Option<&Id> {
        self.owners.first()
    }

    pub fn with_role(mut self, role: Role) -> Self {
        self.role = Some(role);
        self
    }

    /// Whether `other` is this occupant's own: one owns the other. Two
    /// things with the same owner are not each other's own (semantics C1).
    pub fn is_own(&self, other: &Occupant) -> bool {
        self.owners.contains(&other.id) || other.owners.contains(&self.id)
    }
}

/// What a finding is against.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Against {
    /// An occupant of the record, by identity.
    Occupant(Id),
    /// The record's bounds: the figure's edge.
    FigureEdge,
}

/// One thing the record found about a candidate.
#[derive(Clone, Debug, PartialEq)]
pub enum Finding {
    /// The candidate meets something; the covering table's verdict.
    Meets {
        against: Against,
        verdict: Verdict,
        row: CoveringRef,
    },
    /// Closer than the least distance: rejected.
    BelowLeast {
        against: Against,
        gap: f64,
        least: f64,
        row: DistanceRow,
    },
    /// Closer than its ideal and not below the least distance: allowed
    /// from `allowed_from` (the rules' squeeze level), and flagged so that
    /// squeezing is a choice.
    Squeezed {
        against: Against,
        gap: f64,
        ideal: f64,
        row: DistanceRow,
        allowed_from: Level,
    },
    /// The candidate comes within the distance of something without
    /// meeting it, and the covering row that applies holds near as well
    /// ("along or close to a data line"): the row's verdict, in place of
    /// the distance findings.
    Near {
        against: Against,
        gap: f64,
        verdict: Verdict,
        row: CoveringRef,
    },
    /// The candidate stands in a line with a halo, or in another text's
    /// cut, and the cut table has a row for where the cut falls: the row's
    /// verdict. `against` is the line; `by` is the other thing involved,
    /// where there is one (the line crossing the cut, the text whose cut it
    /// stands in). `row` indexes [`crate::rules::Cuts::rows`].
    Cut {
        against: Against,
        at: CutAt,
        by: Option<Id>,
        verdict: Verdict,
        row: usize,
    },
}

impl Finding {
    pub fn against(&self) -> &Against {
        match self {
            Self::Meets { against, .. }
            | Self::BelowLeast { against, .. }
            | Self::Squeezed { against, .. }
            | Self::Near { against, .. }
            | Self::Cut { against, .. } => against,
        }
    }

    /// Whether this finding rejects the place at `level`. A `Never` verdict
    /// and the least distance always reject; squeezing below an ideal is
    /// permitted from the level the finding carries.
    pub fn rejects_at(&self, level: Level) -> bool {
        match self {
            Self::Meets { verdict, .. }
            | Self::Near { verdict, .. }
            | Self::Cut { verdict, .. } => verdict.rejects(),
            Self::BelowLeast { .. } => true,
            Self::Squeezed { allowed_from, .. } => level < *allowed_from,
        }
    }
}

/// Everything the record found about one candidate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Judgement {
    pub findings: Vec<Finding>,
    /// The cuts a halo round the candidate makes in the lines it stands in
    /// by a verdict that needs a halo, one for each straight piece it
    /// meets. A caller taking the place enters them
    /// ([`Record::enter_cuts`]) and draws the lines broken there.
    pub cuts: Vec<Cut>,
}

impl Judgement {
    /// The first finding that rejects the place at `level`, if any.
    pub fn rejection_at(&self, level: Level) -> Option<&Finding> {
        self.findings.iter().find(|f| f.rejects_at(level))
    }

    /// The findings that flag squeezing.
    pub fn squeezes(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|f| matches!(f, Finding::Squeezed { .. }))
    }

    /// The verdicts of everything the candidate meets or, by a row that
    /// holds near, comes close to, and of where its halos' cuts fall.
    pub fn verdicts(&self) -> impl Iterator<Item = Verdict> + '_ {
        self.findings.iter().filter_map(|f| match f {
            Finding::Meets { verdict, .. }
            | Finding::Near { verdict, .. }
            | Finding::Cut { verdict, .. } => Some(*verdict),
            _ => None,
        })
    }

    /// The place's position in a list of places: the latest any verdict
    /// gives it.
    pub fn preference(&self) -> Preference {
        self.verdicts()
            .map(Verdict::preference)
            .max()
            .unwrap_or_default()
    }
}

/// The record of occupied space.
#[derive(Clone, Debug)]
pub struct Record {
    rules: Rules,
    sizes: Sizes,
    bounds: Option<Rect>,
    occupants: Vec<Occupant>,
    declared: Vec<(Id, Id, Relation)>,
    joined: Vec<Join>,
    cuts: Vec<Cut>,
    revision: Revision,
}

/// Which state of a record a judgement was made against: it changes with
/// every change that could alter what the record would judge (semantics
/// C9). A record's states, one after another, never share one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Revision(u64);

/// A pair joined by a text gap, with that text's metrics.
#[derive(Clone, Debug)]
struct Join {
    a: Id,
    b: Id,
    gap: TextGap,
    metrics: TextMetrics,
}

impl Join {
    fn is(&self, a: &Id, b: &Id) -> bool {
        (&self.a == a && &self.b == b) || (&self.a == b && &self.b == a)
    }
}

impl Record {
    /// An empty record judging by `rules` at `sizes`.
    pub fn new(rules: Rules, sizes: Sizes) -> Self {
        Self {
            rules,
            sizes,
            bounds: None,
            occupants: Vec::new(),
            declared: Vec::new(),
            joined: Vec::new(),
            cuts: Vec::new(),
            revision: Revision::default(),
        }
    }

    /// The record's current revision.
    pub fn revision(&self) -> Revision {
        self.revision
    }

    fn changed(&mut self) {
        self.revision.0 += 1;
    }

    /// The figure's edge: a candidate not wholly inside it meets the edge.
    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = Some(bounds);
        self.changed();
        self
    }

    pub fn rules(&self) -> &Rules {
        &self.rules
    }

    /// The tables, to change a row or a number.
    pub fn rules_mut(&mut self) -> &mut Rules {
        self.changed();
        &mut self.rules
    }

    pub fn sizes(&self) -> &Sizes {
        &self.sizes
    }

    pub fn bounds(&self) -> Option<Rect> {
        self.bounds
    }

    /// Declares how two things stand to each other (by identity, whether
    /// or not they are entered yet). A later declaration for the same pair
    /// replaces an earlier one.
    pub fn declare(&mut self, a: Id, b: Id, relation: Relation) {
        self.changed();
        self.declared
            .retain(|(x, y, _)| !((x == &a && y == &b) || (x == &b && y == &a)));
        self.declared.push((a, b, relation));
    }

    /// Declares two things (by identity) joined by a text gap: a swatch and
    /// its name, a note mark and its text, a unit after its number. The
    /// pair is judged by `gap` in a text with `metrics` instead of by the
    /// diagram spacing, and is not flagged as squeezed (semantics C6). A
    /// later join for the same pair replaces an earlier one.
    pub fn join(&mut self, a: Id, b: Id, gap: TextGap, metrics: TextMetrics) {
        self.changed();
        self.joined.retain(|j| !j.is(&a, &b));
        self.joined.push(Join { a, b, gap, metrics });
    }

    /// How two things stand to each other: as declared, else related when
    /// one is the other's own, else unrelated.
    pub fn relation(&self, a: &Occupant, b: &Occupant) -> Relation {
        self.declared
            .iter()
            .find(|(x, y, _)| (x == &a.id && y == &b.id) || (x == &b.id && y == &a.id))
            .map(|(_, _, r)| *r)
            .unwrap_or(if a.is_own(b) {
                Relation::Related
            } else {
                Relation::Unrelated
            })
    }

    /// Everything entered, in the order entered.
    pub fn occupants(&self) -> &[Occupant] {
        &self.occupants
    }

    pub fn get(&self, id: &Id) -> Option<&Occupant> {
        self.occupants.iter().find(|o| &o.id == id)
    }

    /// Enters a drawn thing. It is never moved.
    pub fn enter(&mut self, occupant: Occupant) {
        self.changed();
        self.occupants.push(occupant);
    }

    /// Enters the cuts halos make in lines: those of a judgement whose place
    /// was taken ([`Judgement::cuts`]). The lines stay as they were entered;
    /// later candidates are judged against what they still show.
    pub fn enter_cuts(&mut self, cuts: impl IntoIterator<Item = Cut>) {
        self.changed();
        self.cuts.extend(cuts);
    }

    /// Every cut entered, in the order entered.
    pub fn cuts(&self) -> &[Cut] {
        &self.cuts
    }

    /// The cuts entered in the line `line`: where to draw it broken.
    pub fn cuts_of<'a>(&'a self, line: &Id) -> impl Iterator<Item = &'a Cut> + use<'a> {
        let line = line.clone();
        self.cuts.iter().filter(move |cut| cut.line == line)
    }

    /// Judges a candidate against the figure's edge and everything it would
    /// meet or come too near. Changes nothing.
    pub fn judge(&self, candidate: &Occupant) -> Judgement {
        self.judge_with(candidate, &[])
    }

    /// Judges a candidate as [`Record::judge`] does, as if `pending` were
    /// entered too: for a set whose members are judged against each other
    /// before any is entered. Changes nothing.
    pub fn judge_with(&self, candidate: &Occupant, pending: &[Occupant]) -> Judgement {
        let covering = &self.rules.covering;
        let spacing = &self.rules.distances.diagram;
        let placed = (candidate.kind, candidate.role.as_ref());
        let everything: Vec<&Occupant> = self.occupants.iter().chain(pending).collect();
        let pending_cuts = self.pending_cuts(pending, &everything);
        let all_cuts: Vec<&Cut> = self.cuts.iter().chain(&pending_cuts).collect();
        let mut findings = Vec::new();
        let mut cuts = Vec::new();
        if let Some(bounds) = self.bounds
            && !bounds.contains(candidate.ink.bounds())
            && let Some((verdict, row)) = covering.edge_verdict(placed)
        {
            findings.push(Finding::Meets {
                against: Against::FigureEdge,
                verdict,
                row,
            });
        }
        for &occupant in &everything {
            if self.hidden_from(candidate, occupant, everything.iter().copied())
                || self.covered_by_target(candidate, occupant, &everything)
            {
                continue;
            }
            let against = || Against::Occupant(occupant.id.clone());
            let relation = self.relation(candidate, occupant);
            let met = (occupant.kind, occupant.role.as_ref());
            let own = candidate.is_own(occupant);
            // A line is judged as it now shows: without the stretches
            // earlier halos cut from it, and a candidate standing in one of
            // those is refused by the cut table.
            let piece = Piece::of(&occupant.ink);
            let mut spans = Vec::new();
            for cut in all_cuts.iter().filter(|cut| cut.line == occupant.id) {
                let Some(span) = piece.as_ref().and_then(|p| p.span_of(cut)) else {
                    continue;
                };
                spans.push(span);
                if cut.label != candidate.id && candidate.ink.meets(&cut.ink()) {
                    self.cut_finding(
                        &mut findings,
                        occupant,
                        CutAt::InAnotherCut,
                        Some(cut.label.clone()),
                    );
                }
            }
            let gap = match &piece {
                Some(piece) if !spans.is_empty() => piece
                    .shown(&spans)
                    .iter()
                    .map(|ink| candidate.ink.gap(ink))
                    .fold(f64::INFINITY, f64::min),
                _ => candidate.ink.gap(&occupant.ink),
            };
            if gap <= 0.0 {
                let (verdict, row) = self.meeting_verdict(candidate, occupant);
                findings.push(Finding::Meets {
                    against: against(),
                    verdict,
                    row,
                });
                if verdict.needs_halo() {
                    self.halo(candidate, occupant, &everything, &mut findings, &mut cuts);
                }
                continue;
            }
            if !candidate.kind.is_ink() || !occupant.kind.is_ink() {
                continue;
            }
            if let Some(join) = self
                .joined
                .iter()
                .find(|j| j.is(&candidate.id, &occupant.id))
            {
                let least = self.rules.distances.text.distance(join.gap, &join.metrics);
                if gap < least - FIT_EPSILON {
                    findings.push(Finding::BelowLeast {
                        against: against(),
                        gap,
                        least,
                        row: DistanceRow::Text(join.gap),
                    });
                }
                continue;
            }
            let Some((ideal, ideal_row)) =
                spacing.ideal_between(relation, candidate.kind, occupant.kind, &self.sizes)
            else {
                continue;
            };
            if gap >= ideal - FIT_EPSILON {
                continue;
            }
            let (least, least_row) =
                spacing.least_between(candidate.kind, occupant.kind, &self.sizes);
            if let Some((index, row)) = covering.row_entry_for(placed, met, own) {
                if row.verdict == Verdict::MeantToMeet {
                    continue;
                }
                // Near applies between the least distance and the ideal.
                if row.near && gap >= least - FIT_EPSILON {
                    findings.push(Finding::Near {
                        against: against(),
                        gap,
                        verdict: row.verdict,
                        row: CoveringRef::Row(index),
                    });
                    continue;
                }
            }
            if gap < least - FIT_EPSILON {
                findings.push(Finding::BelowLeast {
                    against: against(),
                    gap,
                    least,
                    row: least_row,
                });
            } else {
                findings.push(Finding::Squeezed {
                    against: against(),
                    gap,
                    ideal,
                    row: ideal_row,
                    allowed_from: self.rules.squeeze_from,
                });
            }
        }
        Judgement { findings, cuts }
    }

    /// The covering table's verdict for a candidate meeting an occupant,
    /// unless the caller declared the pair meant to meet. Rows for running
    /// along apply where the candidate runs along the occupant
    /// ([`Self::runs_along`]).
    fn meeting_verdict(&self, candidate: &Occupant, occupant: &Occupant) -> (Verdict, CoveringRef) {
        if self.relation(candidate, occupant) == Relation::MeantToMeet {
            (Verdict::MeantToMeet, CoveringRef::Declared)
        } else {
            self.rules.covering.verdict_along(
                (candidate.kind, candidate.role.as_ref()),
                (occupant.kind, occupant.role.as_ref()),
                candidate.is_own(occupant),
                self.runs_along(candidate, occupant),
            )
        }
    }

    /// Whether `candidate`, drawn as a segment, runs along `occupant`, a
    /// segment it meets: the two lie at less than the rules'
    /// [`crate::rules::Connectors::along_below`] to each other, and the
    /// candidate overlaps the occupant along its length by more than the
    /// least distance between their kinds. A line crossing another at a
    /// wider angle, or touching it only end to end, does not run along it.
    pub fn runs_along(&self, candidate: &Occupant, occupant: &Occupant) -> bool {
        let (
            Ink::Segment {
                from: a0, to: a1, ..
            },
            Ink::Segment {
                from: b0, to: b1, ..
            },
        ) = (candidate.ink, occupant.ink)
        else {
            return false;
        };
        let (Some(u), Some(v)) = (unit(a0, a1), unit(b0, b1)) else {
            return false;
        };
        let angle = (u.x * v.x + u.y * v.y).abs().min(1.0).acos().to_degrees();
        if angle >= self.rules.connectors.along_below {
            return false;
        }
        // The overlap along the occupant: the candidate's ends projected on
        // its line, clipped to its length.
        let along = |p: Point| (p.x - b0.x) * v.x + (p.y - b0.y) * v.y;
        let length = (b1.x - b0.x).hypot(b1.y - b0.y);
        let (s, t) = (along(a0), along(a1));
        let overlap = s.max(t).min(length) - s.min(t).max(0.0);
        let (least, _) =
            self.rules
                .distances
                .diagram
                .least_between(candidate.kind, occupant.kind, &self.sizes);
        overlap > least + FIT_EPSILON
    }

    /// Whether, where `candidate` (a segment) meets `occupant` (an area),
    /// it meets only inside a target of its own that the rules draw above
    /// the occupant: a leader ending on a comparison mark where the mark
    /// lies across its bar meets the mark, and the bar is covered there.
    /// The target must be the candidate's own and meant to meet it.
    fn covered_by_target(
        &self,
        candidate: &Occupant,
        occupant: &Occupant,
        everything: &[&Occupant],
    ) -> bool {
        let (Ink::Segment { from, to, width }, Ink::Rect(met)) = (candidate.ink, occupant.ink)
        else {
            return false;
        };
        let reach = width / 2.0;
        let Some((a, b)) = clip(from, to, met.grown(reach)) else {
            return false;
        };
        let lower = (occupant.kind, occupant.role.as_ref());
        everything.iter().any(|&target| {
            target.id != occupant.id
                && candidate.owners.contains(&target.id)
                && matches!(target.ink, Ink::Rect(area)
                    if area.grown(reach + FIT_EPSILON).holds(a)
                        && area.grown(reach + FIT_EPSILON).holds(b))
                && self
                    .rules
                    .drawn_beneath(lower, (target.kind, target.role.as_ref()))
                && self.meeting_verdict(candidate, target).0 == Verdict::MeantToMeet
        })
    }

    /// A finding by the cut table's row for `at`, if it has one.
    fn cut_finding(&self, findings: &mut Vec<Finding>, line: &Occupant, at: CutAt, by: Option<Id>) {
        if let Some((row, verdict)) = self.rules.cuts.verdict(at) {
            findings.push(Finding::Cut {
                against: Against::Occupant(line.id.clone()),
                at,
                by,
                verdict,
                row,
            });
        }
    }

    /// The cut a halo round `candidate` makes in one piece of `line`, and
    /// the findings of the cut table about where it falls ([`halo`]).
    fn halo(
        &self,
        candidate: &Occupant,
        line: &Occupant,
        everything: &[&Occupant],
        findings: &mut Vec<Finding>,
        cuts: &mut Vec<Cut>,
    ) {
        let Some(piece) = Piece::of(&line.ink) else {
            self.cut_finding(findings, line, CutAt::NotALine, None);
            return;
        };
        let spacing = &self.rules.distances.diagram;
        let width = spacing.halo.resolve(&self.sizes);
        let Some(span) = piece.span(candidate.ink.bounds(), width) else {
            return;
        };
        let whole = Line::of(&line.id, everything.iter().copied());
        let others: Vec<(Id, Piece)> = everything
            .iter()
            .filter(|o| o.id != line.id && o.id != candidate.id && o.kind.is_ink())
            .filter_map(|o| Piece::of(&o.ink).map(|p| (o.id.clone(), p)))
            .collect();
        let near_end = spacing.halo_near_end.resolve(&self.sizes);
        for (at, by) in halo::places(
            &whole,
            &piece,
            span,
            &others,
            near_end,
            spacing.halo_cut_share,
        ) {
            self.cut_finding(findings, line, at, by);
        }
        cuts.extend(halo::cut(
            &line.id,
            &candidate.id,
            &piece,
            candidate.ink.bounds(),
            width,
        ));
    }

    /// The cuts the halos of `pending` things would make, as if they were
    /// entered with their cuts: so that the members of a set are judged
    /// against the lines as the members before them leave them.
    fn pending_cuts(&self, pending: &[Occupant], everything: &[&Occupant]) -> Vec<Cut> {
        let width = self.rules.distances.diagram.halo.resolve(&self.sizes);
        let mut cuts = Vec::new();
        for text in pending {
            for &line in everything {
                let Some(piece) = Piece::of(&line.ink) else {
                    continue;
                };
                if line.id == text.id
                    || !text.ink.meets(&line.ink)
                    || self.hidden_from(text, line, everything.iter().copied())
                    || !self.meeting_verdict(text, line).0.needs_halo()
                {
                    continue;
                }
                cuts.extend(halo::cut(
                    &line.id,
                    &text.id,
                    &piece,
                    text.ink.bounds(),
                    width,
                ));
            }
        }
        cuts
    }

    /// Whether `line` is hidden from `candidate` by the order of drawing:
    /// the candidate stands wholly on a mark that the rules draw above the
    /// line, and the line passes under that mark. Where the candidate stands
    /// the line is covered, and beyond the mark's edge the edge stands
    /// between them.
    fn hidden_from<'a>(
        &self,
        candidate: &Occupant,
        line: &Occupant,
        mut everything: impl Iterator<Item = &'a Occupant>,
    ) -> bool {
        let lower = (line.kind, line.role.as_ref());
        let standing = candidate.ink.bounds();
        everything.any(|mark| {
            !std::ptr::eq(mark, line)
                && matches!(mark.ink, Ink::Rect(area) if area.contains(standing))
                && self
                    .rules
                    .drawn_beneath(lower, (mark.kind, mark.role.as_ref()))
                && line.ink.meets(&mark.ink)
        })
    }

    /// Enters everything placed by a judged set or free place, in order.
    pub fn enter_all(&mut self, occupants: impl IntoIterator<Item = Occupant>) {
        self.changed();
        self.occupants.extend(occupants);
    }

    /// What is at `point`, innermost (smallest) first; of equal size, the
    /// later entered first.
    pub fn at(&self, point: Point) -> Vec<&Occupant> {
        let mut found: Vec<(usize, &Occupant)> = self
            .occupants
            .iter()
            .enumerate()
            .filter(|(_, o)| o.ink.holds(point))
            .collect();
        found.sort_by(|(i, a), (j, b)| a.ink.area().total_cmp(&b.ink.area()).then(j.cmp(i)));
        found.into_iter().map(|(_, o)| o).collect()
    }

    /// The lines (occupants drawn as segments) that pass under `rect`.
    pub fn lines_under(&self, rect: Rect) -> Vec<&Occupant> {
        let rect = Ink::Rect(rect);
        self.occupants
            .iter()
            .filter(|o| matches!(o.ink, Ink::Segment { .. }) && o.ink.meets(&rect))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{Measure, Named, PairLeast};

    /// Her 11 px text: a least distance of 2.2 px, ideals of 3.7 and 7.3.
    fn record() -> Record {
        Record::new(
            Rules::default(),
            Sizes {
                label_size: 11.0,
                word_space: 3.7,
                line_width: 1.0,
            },
        )
        .with_bounds(Rect::new(0.0, 0.0, 400.0, 300.0))
    }

    fn id(s: &str) -> Id {
        Id::new(s)
    }

    fn label(name: &str, rect: Rect) -> Occupant {
        Occupant::new(id(name), Kind::Words, Ink::Rect(rect))
    }

    fn hline(name: &str, kind: Kind, y: f64) -> Occupant {
        Occupant::new(
            id(name),
            kind,
            Ink::segment(Point::new(0.0, y), Point::new(300.0, y), 0.0),
        )
    }

    #[test]
    fn unrelated_things_3_px_apart_are_squeezed_allowed_and_flagged() {
        let mut r = record();
        r.enter(label("a", Rect::new(10.0, 10.0, 20.0, 11.0)));
        let b = label("b", Rect::new(33.0, 10.0, 20.0, 11.0));
        let j = r.judge(&b);
        assert_eq!(j.findings.len(), 1);
        let Finding::Squeezed {
            gap, ideal, row, ..
        } = &j.findings[0]
        else {
            panic!("{j:?}")
        };
        assert!((gap - 3.0).abs() < 1e-9 && (ideal - 7.26).abs() < 1e-9);
        assert_eq!(*row, DistanceRow::Unrelated);
        assert_eq!(j.rejection_at(Level::Strong), None);
        assert!(j.rejection_at(Level::Preferred).is_some());
    }

    #[test]
    fn a_label_08_px_from_its_own_edge_is_below_the_least_distance() {
        let mut r = record();
        r.enter(hline("edge", Kind::Connector, 50.0));
        let f = label("f", Rect::new(100.0, 38.2, 10.0, 11.0)).owned_by(id("edge"));
        let j = r.judge(&f);
        let rejection = j.rejection_at(Level::AllPermitted).expect("rejected");
        let Finding::BelowLeast {
            against,
            gap,
            least,
            row,
        } = rejection
        else {
            panic!("{rejection:?}")
        };
        assert_eq!(against, &Against::Occupant(id("edge")));
        assert!((gap - 0.8).abs() < 1e-9 && (least - 2.2).abs() < 1e-9);
        assert_eq!(*row, DistanceRow::Least);
        // At its related ideal it is clean.
        let f = label("f", Rect::new(100.0, 35.3, 10.0, 11.0)).owned_by(id("edge"));
        assert_eq!(r.judge(&f).findings, []);
    }

    #[test]
    fn an_arrow_and_its_target_are_exempt() {
        let mut r = record();
        r.enter(Occupant::new(
            id("bar"),
            Kind::DataMark,
            Ink::Rect(Rect::new(100.0, 100.0, 20.0, 50.0)),
        ));
        r.declare(id("arrow"), id("bar"), Relation::MeantToMeet);
        let near = Occupant::new(
            id("arrow"),
            Kind::Connector,
            Ink::segment(Point::new(50.0, 90.0), Point::new(99.5, 99.5), 1.0),
        );
        assert_eq!(r.judge(&near).findings, []);
        let touching = Occupant {
            ink: Ink::segment(Point::new(50.0, 90.0), Point::new(105.0, 105.0), 1.0),
            ..near
        };
        let j = r.judge(&touching);
        assert_eq!(
            j.findings,
            [Finding::Meets {
                against: Against::Occupant(id("bar")),
                verdict: Verdict::MeantToMeet,
                row: CoveringRef::Declared,
            }]
        );
        assert_eq!(j.rejection_at(Level::Preferred), None);
    }

    #[test]
    fn a_pair_of_kinds_given_its_own_least_distance_is_judged_by_it() {
        let mut r = record();
        r.enter(hline("tick", Kind::Tick, 50.0));
        let n = Occupant::new(
            id("n"),
            Kind::Number,
            Ink::Rect(Rect::new(10.0, 53.0, 10.0, 11.0)),
        );
        assert_eq!(r.judge(&n).rejection_at(Level::Strong), None);
        r.rules_mut()
            .distances
            .diagram
            .least_for_pairs
            .push(PairLeast {
                a: Kind::Tick,
                b: Kind::Number,
                least: Measure::new(1.0, Named::WordSpace),
            });
        assert_eq!(
            r.judge(&n).rejection_at(Level::Strong),
            Some(&Finding::BelowLeast {
                against: Against::Occupant(id("tick")),
                gap: 3.0,
                least: 3.7,
                row: DistanceRow::PairLeast(0),
            })
        );
    }

    fn verdict_on(r: &Record, candidate: &Occupant, on: &str) -> (Verdict, CoveringRef) {
        r.judge(candidate)
            .findings
            .iter()
            .find_map(|f| match f {
                Finding::Meets {
                    against: Against::Occupant(a),
                    verdict,
                    row,
                } if a.as_str() == on => Some((*verdict, *row)),
                _ => None,
            })
            .expect("meets")
    }

    #[test]
    fn a_number_stands_only_in_its_own_value_line_and_words_move_first() {
        let mut r = record();
        r.enter(hline("target", Kind::ValueLine, 50.0));
        let across = Rect::new(100.0, 44.5, 20.0, 11.0);
        let number = |owner: &str| {
            Occupant::new(id("n"), Kind::Number, Ink::Rect(across)).owned_by(id(owner))
        };
        let (v, row) = verdict_on(&r, &number("bar"), "target");
        assert_eq!(v, Verdict::Never);
        let CoveringRef::Row(i) = row else { panic!() };
        assert_eq!(
            r.rules().covering.rows[i].name,
            "a number in a value line not its own"
        );
        assert_eq!(
            verdict_on(&r, &number("target"), "target").0,
            Verdict::AllowedWithHalo
        );
        assert_eq!(
            r.judge(&number("target")).rejection_at(Level::Preferred),
            None
        );
        let words = label("w", across);
        assert_eq!(
            verdict_on(&r, &words, "target").0,
            Verdict::MoveFirstElseHalo
        );
    }

    #[test]
    fn a_comparisons_name_never_stands_in_another_comparisons_line() {
        let mut r = record();
        r.enter(hline("cmp-a", Kind::ValueLine, 50.0).with_role(Role::comparison()));
        let name = |owner: &str| {
            label("name", Rect::new(100.0, 44.5, 30.0, 11.0))
                .owned_by(id(owner))
                .with_role(Role::comparison())
        };
        assert_eq!(verdict_on(&r, &name("cmp-b"), "cmp-a").0, Verdict::Never);
        // In its own comparison's line (the line owns it): the row for words
        // in a value line.
        assert_eq!(
            verdict_on(&r, &name("cmp-a"), "cmp-a").0,
            Verdict::MoveFirstElseHalo
        );
    }

    #[test]
    fn changing_a_row_changes_the_verdict_and_nothing_else() {
        let mut r = record();
        r.enter(hline("grid", Kind::ValueLine, 50.0));
        let n = Occupant::new(
            id("n"),
            Kind::Number,
            Ink::Rect(Rect::new(100.0, 44.5, 20.0, 11.0)),
        );
        assert_eq!(verdict_on(&r, &n, "grid").0, Verdict::Never);
        let row = r
            .rules()
            .covering
            .row_named("a number in a value line not its own")
            .unwrap();
        r.rules_mut().covering.rows[row].verdict = Verdict::AllowedWithHalo;
        assert_eq!(
            verdict_on(&r, &n, "grid"),
            (Verdict::AllowedWithHalo, CoveringRef::Row(row))
        );
    }

    #[test]
    fn text_crossing_the_figures_edge_is_rejected() {
        let r = record();
        let j = r.judge(&label("t", Rect::new(395.0, 10.0, 20.0, 11.0)));
        assert!(matches!(
            j.rejection_at(Level::AllPermitted),
            Some(Finding::Meets {
                against: Against::FigureEdge,
                verdict: Verdict::Never,
                ..
            })
        ));
    }

    #[test]
    fn a_reading_path_puts_a_place_last_and_keeps_no_distance() {
        let mut r = record();
        r.enter(Occupant::new(
            id("path"),
            Kind::ReadingPath,
            Ink::Rect(Rect::new(0.0, 0.0, 100.0, 100.0)),
        ));
        let j = r.judge(&label("t", Rect::new(10.0, 10.0, 20.0, 11.0)));
        assert_eq!(j.preference(), Preference::LastInOrder);
        assert_eq!(j.rejection_at(Level::Preferred), None);
        assert_eq!(
            r.judge(&label("t", Rect::new(101.0, 10.0, 20.0, 11.0)))
                .findings,
            []
        );
    }

    #[test]
    fn the_point_query_returns_the_innermost_occupant_first() {
        let mut r = record();
        r.enter(Occupant::new(
            id("plate"),
            Kind::Plate,
            Ink::Rect(Rect::new(0.0, 0.0, 200.0, 200.0)),
        ));
        r.enter(Occupant::new(
            id("bar"),
            Kind::DataMark,
            Ink::Rect(Rect::new(10.0, 10.0, 40.0, 100.0)),
        ));
        r.enter(
            Occupant::new(
                id("bar/value"),
                Kind::Number,
                Ink::Rect(Rect::new(15.0, 15.0, 20.0, 11.0)),
            )
            .owned_by(id("bar")),
        );
        r.enter(hline("grid", Kind::ValueLine, 20.0));
        let ids: Vec<_> = r
            .at(Point::new(20.0, 20.0))
            .iter()
            .map(|o| o.id.as_str())
            .collect();
        assert_eq!(ids, ["grid", "bar/value", "bar", "plate"]);
        assert!(r.at(Point::new(300.0, 300.0)).is_empty());
    }

    #[test]
    fn lines_under_a_rectangle() {
        let mut r = record();
        r.enter(hline("grid-50", Kind::ValueLine, 50.0));
        r.enter(hline("grid-80", Kind::ValueLine, 80.0));
        r.enter(Occupant::new(
            id("bar"),
            Kind::DataMark,
            Ink::Rect(Rect::new(10.0, 40.0, 20.0, 60.0)),
        ));
        r.enter(Occupant::new(
            id("series"),
            Kind::DataLine,
            Ink::segment(Point::new(0.0, 0.0), Point::new(100.0, 100.0), 2.0),
        ));
        let ids: Vec<_> = r
            .lines_under(Rect::new(40.0, 45.0, 20.0, 10.0))
            .iter()
            .map(|o| o.id.as_str())
            .collect();
        assert_eq!(ids, ["grid-50", "series"]);
    }

    // ----- review findings 6, 7, 10 and the hard-coded squeeze level -----

    fn row_name(r: &Record, row: CoveringRef) -> &str {
        let CoveringRef::Row(i) = row else {
            panic!("{row:?}")
        };
        &r.rules().covering.rows[i].name
    }

    fn bar(name: &str, rect: Rect) -> Occupant {
        Occupant::new(id(name), Kind::DataMark, Ink::Rect(rect))
    }

    #[test]
    fn a_scale_label_is_not_its_siblings_value_line() {
        // Review 6: scale label "20" and gridline "40", both owned by the
        // axis. They are siblings, not each other's own (C1).
        let mut r = record();
        r.enter(hline("grid-40", Kind::ValueLine, 50.0).owned_by(id("axis")));
        let twenty = Occupant::new(
            id("label-20"),
            Kind::Number,
            Ink::Rect(Rect::new(10.0, 44.5, 14.0, 11.0)),
        )
        .owned_by(id("axis"));
        let (verdict, row) = verdict_on(&r, &twenty, "grid-40");
        assert_eq!(verdict, Verdict::Never);
        assert_eq!(row_name(&r, row), "a number in a value line not its own");
        let line = r.get(&id("grid-40")).unwrap().clone();
        assert_eq!(r.relation(&twenty, &line), Relation::Unrelated);
        // The caller can declare siblings related; that sets the distance,
        // not ownership.
        r.declare(id("label-20"), id("grid-40"), Relation::Related);
        assert_eq!(r.relation(&twenty, &line), Relation::Related);
        assert_eq!(verdict_on(&r, &twenty, "grid-40").0, Verdict::Never);
        // Owned by the line itself, it is that line's value.
        let forty = Occupant {
            owners: vec![id("grid-40")],
            ..twenty
        };
        assert_eq!(
            verdict_on(&r, &forty, "grid-40").0,
            Verdict::AllowedWithHalo
        );
    }

    #[test]
    fn a_label_on_its_bar_does_not_meet_a_gridline_the_bar_covers() {
        // Review 7, C5: the gridline at y = 50 is drawn beneath the bar.
        let mut r = record();
        r.enter(hline("grid", Kind::ValueLine, 50.0).with_role(Role::gridline()));
        r.enter(bar("bar", Rect::new(100.0, 40.0, 20.0, 100.0)));
        let value = |x: f64, y: f64| {
            Occupant::new(
                id("v"),
                Kind::Number,
                Ink::Rect(Rect::new(x, y, 14.0, 11.0)),
            )
            .owned_by(id("bar"))
        };
        let inside = r.judge(&value(103.0, 45.0));
        assert_eq!(inside.rejection_at(Level::Preferred), None);
        assert_eq!(
            inside.verdicts().collect::<Vec<_>>(),
            [Verdict::AllowedInToggleInk]
        );
        // 1 px from the covered gridline: no distance either.
        assert_eq!(
            r.judge(&value(103.0, 51.0)).rejection_at(Level::Preferred),
            None
        );
        // Hanging off the bar, it stands in the gridline where it shows.
        let (verdict, row) = verdict_on(&r, &value(112.0, 45.0), "grid");
        assert_eq!(verdict, Verdict::Never);
        assert_eq!(row_name(&r, row), "a number in a value line not its own");
        // A data line is not drawn beneath the bar: it stops the label.
        r.enter(hline("series", Kind::DataLine, 48.0));
        assert_eq!(
            verdict_on(&r, &value(103.0, 45.0), "series").0,
            Verdict::Never
        );
        // A comparison line across the bar is drawn over it and is judged.
        r.enter(hline("target", Kind::ValueLine, 52.0).with_role(Role::comparison()));
        assert_eq!(
            verdict_on(&r, &value(103.0, 45.0), "target").0,
            Verdict::Never
        );
        // The order of drawing is data: without it the gridline stops it too.
        let mut r = record();
        r.enter(hline("grid", Kind::ValueLine, 50.0).with_role(Role::gridline()));
        r.enter(bar("bar", Rect::new(100.0, 40.0, 20.0, 100.0)));
        r.rules_mut().drawing_order.clear();
        assert_eq!(
            verdict_on(&r, &value(103.0, 45.0), "grid").0,
            Verdict::Never
        );
    }

    #[test]
    fn squeezing_starts_at_the_level_the_rules_say() {
        // Hard-coded in review: the squeeze level was fixed at Strong (A7).
        let mut r = record();
        r.enter(label("a", Rect::new(10.0, 10.0, 20.0, 11.0)));
        let b = label("b", Rect::new(33.0, 10.0, 20.0, 11.0));
        assert!(r.judge(&b).rejection_at(Level::Mild).is_some());
        assert_eq!(r.rules().squeeze_from, Level::Strong);
        r.rules_mut().squeeze_from = Level::Mild;
        let j = r.judge(&b);
        assert!(matches!(
            j.findings[..],
            [Finding::Squeezed {
                allowed_from: Level::Mild,
                ..
            }]
        ));
        assert_eq!(j.rejection_at(Level::Mild), None);
        assert!(j.rejection_at(Level::Preferred).is_some());
    }

    #[test]
    fn a_swatch_and_its_name_one_word_space_apart_are_not_squeezed() {
        // Hard-coded in review: the text-spacing family was never used (C6).
        let metrics = TextMetrics {
            letter_space: 0.5,
            word_space: 3.0,
            line_spacing: 13.0,
        };
        let mut r = record();
        r.enter(Occupant::new(
            id("swatch"),
            Kind::Key,
            Ink::Rect(Rect::new(10.0, 10.0, 8.0, 8.0)),
        ));
        let name = |x: f64| label("name", Rect::new(x, 8.5, 30.0, 11.0));
        // Not joined: one word space is under the unrelated ideal.
        assert_eq!(r.judge(&name(21.0)).squeezes().count(), 1);
        r.join(id("swatch"), id("name"), TextGap::Attachment, metrics);
        assert_eq!(r.judge(&name(21.0)).findings, []);
        assert_eq!(r.judge(&name(40.0)).findings, []);
        assert_eq!(
            r.judge(&name(20.0)).findings,
            [Finding::BelowLeast {
                against: Against::Occupant(id("swatch")),
                gap: 2.0,
                least: 3.0,
                row: DistanceRow::Text(TextGap::Attachment),
            }]
        );
    }

    #[test]
    fn a_meant_to_meet_row_waives_the_distance() {
        // Review 10, C7: a connector ending 1 px from its own text.
        let mut r = record();
        r.enter(label("comment", Rect::new(100.0, 100.0, 40.0, 11.0)));
        let arrow = |owner: &str| {
            Occupant::new(
                id("arrow"),
                Kind::Connector,
                Ink::segment(Point::new(50.0, 50.0), Point::new(99.0, 105.0), 0.0),
            )
            .owned_by(id(owner))
        };
        assert_eq!(r.judge(&arrow("comment")).findings, []);
        assert!(matches!(
            r.judge(&arrow("other")).rejection_at(Level::AllPermitted),
            Some(Finding::BelowLeast { .. })
        ));
    }

    #[test]
    fn the_default_connector_rows_follow_the_design() {
        // Review 10, C8.
        let mut r = record();
        r.enter(hline("grid", Kind::ValueLine, 50.0));
        r.enter(hline("series", Kind::DataLine, 120.0));
        r.enter(bar("target", Rect::new(200.0, 150.0, 20.0, 100.0)));
        r.enter(bar("other", Rect::new(150.0, 150.0, 20.0, 100.0)));
        let connector = |from: Point, to: Point| {
            Occupant::new(id("leader"), Kind::Connector, Ink::segment(from, to, 1.0))
        };
        // May cross a gridline.
        let across = connector(Point::new(20.0, 30.0), Point::new(20.0, 70.0));
        let (verdict, row) = verdict_on(&r, &across, "grid");
        assert_eq!(verdict, Verdict::Allowed);
        assert_eq!(row_name(&r, row), "a connector across a gridline");
        assert_eq!(r.judge(&across).rejection_at(Level::Preferred), None);
        // Close to a data line: less preferred, never rejected.
        let close = connector(Point::new(20.0, 117.0), Point::new(100.0, 117.0));
        let j = r.judge(&close);
        assert_eq!(j.rejection_at(Level::Preferred), None);
        assert_eq!(j.preference(), Preference::LessPreferred);
        assert!(matches!(
            j.findings[..],
            [Finding::Near {
                verdict: Verdict::LessPreferred,
                ..
            }]
        ));
        // Below the least distance (2.2 px at 11 px) it is rejected like
        // any other pair: 0.3 px from the line, edge to edge.
        let touching = connector(Point::new(20.0, 119.2), Point::new(100.0, 119.2));
        let j = r.judge(&touching);
        assert!(matches!(
            j.findings[..],
            [Finding::BelowLeast { gap, least, .. }] if (gap - 0.3).abs() < 1e-9 && least > 2.0
        ));
        assert!(j.rejection_at(Level::AllPermitted).is_some());
        let crossing = connector(Point::new(20.0, 100.0), Point::new(20.0, 140.0));
        assert_eq!(r.judge(&crossing).preference(), Preference::LessPreferred);
        // Its own target does not reject it; another data mark does.
        let to_target =
            connector(Point::new(210.0, 135.0), Point::new(210.0, 155.0)).owned_by(id("target"));
        let j = r.judge(&to_target);
        assert_eq!(j.rejection_at(Level::Preferred), None);
        assert_eq!(j.verdicts().collect::<Vec<_>>(), [Verdict::MeantToMeet]);
        let through =
            connector(Point::new(140.0, 160.0), Point::new(210.0, 160.0)).owned_by(id("target"));
        let (verdict, row) = verdict_on(&r, &through, "other");
        assert_eq!(verdict, Verdict::Never);
        assert_eq!(
            row_name(&r, row),
            "a connector through a data mark not its target"
        );
    }

    #[test]
    fn a_connector_is_the_own_of_its_text_and_its_target() {
        // Second review, item 4: with one owner one of the two landed on a
        // Never row.
        let mut r = record();
        r.enter(label("comment", Rect::new(100.0, 100.0, 40.0, 11.0)));
        r.enter(bar("target", Rect::new(130.0, 105.0, 20.0, 100.0)));
        let arrow = Occupant::new(
            id("arrow"),
            Kind::Connector,
            Ink::segment(Point::new(120.0, 105.0), Point::new(140.0, 105.0), 1.0),
        )
        .owned_by(id("comment"))
        .also_owned_by(id("target"));
        assert_eq!(arrow.owners, [id("comment"), id("target")]);
        assert_eq!(arrow.owner(), Some(&id("comment")));
        let j = r.judge(&arrow);
        assert_eq!(j.rejection_at(Level::Preferred), None);
        assert_eq!(
            j.verdicts().collect::<Vec<_>>(),
            [Verdict::MeantToMeet, Verdict::MeantToMeet]
        );
        // `owned_by` still sets the one owner.
        let single = arrow.clone().owned_by(id("target"));
        assert_eq!(single.owners, [id("target")]);
        assert!(r.judge(&single).rejection_at(Level::Preferred).is_some());
    }

    #[test]
    fn judging_with_pending_occupants_counts_them_and_enters_nothing() {
        let r = record();
        let a = label("a", Rect::new(10.0, 10.0, 20.0, 11.0));
        let b = label("b", Rect::new(20.0, 10.0, 20.0, 11.0));
        assert_eq!(r.judge(&b).findings, []);
        assert_eq!(
            r.judge_with(&b, std::slice::from_ref(&a))
                .verdicts()
                .collect::<Vec<_>>(),
            [Verdict::Never]
        );
        assert!(r.occupants().is_empty());
    }

    #[test]
    fn ink_gaps_count_line_widths() {
        let line = Ink::segment(Point::new(0.0, 0.0), Point::new(10.0, 0.0), 2.0);
        assert_eq!(line.gap(&Ink::Rect(Rect::new(2.0, 4.0, 2.0, 2.0))), 3.0);
        let crossing = Ink::segment(Point::new(5.0, -5.0), Point::new(5.0, 5.0), 0.0);
        assert_eq!(line.gap(&crossing), 0.0);
        assert!(line.meets(&Ink::Rect(Rect::new(-1.0, -1.0, 2.0, 2.0))));
        let far = Ink::segment(Point::new(0.0, 10.0), Point::new(10.0, 10.0), 0.0);
        assert_eq!(line.gap(&far), 9.0);
    }

    #[test]
    fn worked_example_a_bar_its_label_and_a_note() {
        // The module's worked example.
        let mut r = record();
        let bar = Occupant::new(
            id("bar"),
            Kind::DataMark,
            Ink::Rect(Rect::new(10.0, 100.0, 20.0, 100.0)),
        );
        r.enter(bar);
        let value = |y: f64| {
            Occupant::new(
                id("value"),
                Kind::Number,
                Ink::Rect(Rect::new(13.0, y, 14.0, 11.0)),
            )
            .owned_by(id("bar"))
        };
        let at_ideal = value(100.0 - 3.696 - 11.0);
        assert_eq!(r.judge(&at_ideal).findings, []);
        let too_near = r.judge(&value(100.0 - 0.8 - 11.0));
        assert!(matches!(
            too_near.rejection_at(Level::AllPermitted),
            Some(Finding::BelowLeast { against: Against::Occupant(on), .. }) if on == &id("bar")
        ));
        r.enter(at_ideal);
        let note = label(
            "note",
            Rect::new(13.0, 100.0 - 3.696 - 11.0 - 3.0 - 11.0, 14.0, 11.0),
        );
        let judged = r.judge(&note);
        let [
            Finding::Squeezed {
                against,
                gap,
                ideal,
                ..
            },
        ] = &judged.findings[..]
        else {
            panic!("{judged:?}")
        };
        assert_eq!(against, &Against::Occupant(id("value")));
        assert!((gap - 3.0).abs() < 1e-9 && (ideal - 7.26).abs() < 1e-9);
        assert!(judged.rejection_at(Level::Preferred).is_some());
        assert_eq!(judged.rejection_at(Level::Strong), None);
        assert_eq!(judged.squeezes().count(), 1);
    }

    // ----- halos and the own line -----

    #[test]
    fn a_number_is_haloed_only_into_its_own_value_line() {
        let mut r = record();
        r.enter(hline("target", Kind::ValueLine, 50.0).with_role(Role::comparison()));
        let number = |owner: &str| {
            Occupant::new(
                id("n"),
                Kind::Number,
                Ink::Rect(Rect::new(100.0, 44.5, 20.0, 11.0)),
            )
            .owned_by(id(owner))
        };
        // Another's number: never, and no halo is offered.
        let j = r.judge(&number("bar"));
        assert!(j.rejection_at(Level::AllPermitted).is_some());
        assert_eq!(j.cuts, []);
        // The line's own value: allowed, with the line cut round it.
        let j = r.judge(&number("target"));
        assert_eq!(j.rejection_at(Level::Preferred), None);
        assert_eq!(j.preference(), Preference::InOrder);
        let [cut] = &j.cuts[..] else { panic!("{j:?}") };
        assert_eq!(cut.line, id("target"));
        assert!((cut.from.x - 98.35).abs() < 1e-9 && (cut.to.x - 121.65).abs() < 1e-9);
    }

    #[test]
    fn a_halo_in_a_line_drawn_as_an_area_is_refused() {
        let mut r = record();
        r.enter(Occupant::new(
            id("grid"),
            Kind::ValueLine,
            Ink::Rect(Rect::new(0.0, 49.5, 300.0, 1.0)),
        ));
        let j = r.judge(&label("w", Rect::new(100.0, 44.5, 20.0, 11.0)));
        assert!(matches!(
            j.rejection_at(Level::AllPermitted),
            Some(Finding::Cut {
                at: CutAt::NotALine,
                ..
            })
        ));
    }

    #[test]
    fn a_label_on_its_bars_line_beside_a_longer_touching_neighbour_is_not_squeezed() {
        // Two touching horizontal bars of a cluster: a (y 100..116, 50 px
        // long) and b below it (y 116..132, 80 px). a's value label stands
        // just beyond a's end, centred on a's line, 2.5 px from b.
        let mut r = record();
        r.enter(bar("a", Rect::new(0.0, 100.0, 50.0, 16.0)));
        r.enter(bar("b", Rect::new(0.0, 116.0, 80.0, 16.0)));
        let value = |y: f64| {
            Occupant::new(
                id("a/value"),
                Kind::Number,
                Ink::Rect(Rect::new(50.0 + 3.696, y, 14.0, 11.0)),
            )
            .owned_by(id("a"))
        };
        let on_line = value(102.5);
        let j = r.judge(&on_line);
        assert_eq!(j.squeezes().count(), 1);
        assert!(j.rejection_at(Level::Preferred).is_some());
        // Declared on its own line: judged by the least distance only.
        r.declare(id("a/value"), id("b"), Relation::OwnLine);
        let j = r.judge(&on_line);
        assert_eq!(j.findings, []);
        assert_eq!(j.rejection_at(Level::Preferred), None);
        // Below the least distance (2.2 px) it is still refused.
        let j = r.judge(&value(103.5));
        assert!(matches!(
            j.rejection_at(Level::AllPermitted),
            Some(Finding::BelowLeast { against: Against::Occupant(on), row: DistanceRow::Least, .. })
                if on == &id("b")
        ));
        // The row is data: a pair row for the kinds counts, and the
        // relation can be given the related ideal instead.
        r.rules_mut()
            .distances
            .diagram
            .least_for_pairs
            .push(PairLeast {
                a: Kind::Number,
                b: Kind::DataMark,
                least: Measure::em(0.1),
            });
        assert_eq!(r.judge(&value(103.5)).findings, []);
        r.rules_mut().distances.diagram.own_line = crate::rules::IdealRow::Related;
        assert!(matches!(
            r.judge(&on_line).findings[..],
            [Finding::Squeezed {
                row: DistanceRow::Related,
                ..
            }]
        ));
    }
}
