// SPDX-License-Identifier: GPL-3.0-only

//! The record of occupied space (design §7 and §11).
//!
//! Everything drawn is entered with its ink, its kind, its identity and its
//! owner's identity. Everything placed later asks the record first: a
//! candidate is judged against everything it would meet or come too near,
//! by the covering table and the distance table, and every finding names
//! the occupant and the row of the table that gave it, so a caller can
//! report why a place was rejected. Judging changes nothing; entering is a
//! separate step. Nothing entered is ever moved.
//!
//! A thing is another's own only when one owns the other (semantics C1).
//! The order of drawing is data (C5): a candidate standing on a mark is not
//! judged against a line the rules draw beneath that mark and that passes
//! under it. A pair the caller joins by a text gap is judged by that text's
//! spacing (C6). A pair the covering table says are meant to meet keeps no
//! distance (C7).
//!
//! Not in this slice: Juliet's "clearly belongs" test (near and far), so a
//! related distance has no upper bound yet; halo geometry.
//!
//! The record also answers what is at a point, innermost first, and which
//! lines pass under a rectangle.
//!
//! This slice searches a list. An index for many marks can replace it
//! behind the same methods.

use crate::geometry::{FIT_EPSILON, Point, Rect};
use crate::identity::Id;
use crate::kinds::{Kind, Role};
use crate::level::Level;
use crate::rules::{
    CoveringRef, DistanceRow, Preference, Relation, Rules, Sizes, TextGap, TextMetrics, Verdict,
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

fn segment_rect_distance(a: Point, b: Point, r: Rect) -> f64 {
    if r.holds(a) || r.holds(b) {
        return 0.0;
    }
    let corners = [
        Point::new(r.x, r.y),
        Point::new(r.right(), r.y),
        Point::new(r.right(), r.bottom()),
        Point::new(r.x, r.bottom()),
    ];
    (0..4)
        .map(|i| segment_segment_distance(a, b, corners[i], corners[(i + 1) % 4]))
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
}

impl Finding {
    pub fn against(&self) -> &Against {
        match self {
            Self::Meets { against, .. }
            | Self::BelowLeast { against, .. }
            | Self::Squeezed { against, .. }
            | Self::Near { against, .. } => against,
        }
    }

    /// Whether this finding rejects the place at `level`. A `Never` verdict
    /// and the least distance always reject; squeezing below an ideal is
    /// permitted from the level the finding carries.
    pub fn rejects_at(&self, level: Level) -> bool {
        match self {
            Self::Meets { verdict, .. } | Self::Near { verdict, .. } => verdict.rejects(),
            Self::BelowLeast { .. } => true,
            Self::Squeezed { allowed_from, .. } => level < *allowed_from,
        }
    }
}

/// Everything the record found about one candidate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Judgement {
    pub findings: Vec<Finding>,
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
    /// holds near, comes close to.
    pub fn verdicts(&self) -> impl Iterator<Item = Verdict> + '_ {
        self.findings.iter().filter_map(|f| match f {
            Finding::Meets { verdict, .. } | Finding::Near { verdict, .. } => Some(*verdict),
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
        let everything = || self.occupants.iter().chain(pending);
        let mut findings = Vec::new();
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
        for occupant in everything() {
            if self.hidden_from(candidate, occupant, everything()) {
                continue;
            }
            let against = || Against::Occupant(occupant.id.clone());
            let relation = self.relation(candidate, occupant);
            let met = (occupant.kind, occupant.role.as_ref());
            let own = candidate.is_own(occupant);
            let gap = candidate.ink.gap(&occupant.ink);
            if gap <= 0.0 {
                let (verdict, row) = if relation == Relation::MeantToMeet {
                    (Verdict::MeantToMeet, CoveringRef::Declared)
                } else {
                    covering.verdict(placed, met, own)
                };
                findings.push(Finding::Meets {
                    against: against(),
                    verdict,
                    row,
                });
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
            let Some((ideal, ideal_row)) = spacing.ideal(relation, &self.sizes) else {
                continue;
            };
            if gap >= ideal - FIT_EPSILON {
                continue;
            }
            let (least, least_row) =
                spacing.least_between(candidate.kind, occupant.kind, &self.sizes);
            if let Some(index) = covering.row_for(placed, met, own) {
                let row = &covering.rows[index];
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
        Judgement { findings }
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
}
