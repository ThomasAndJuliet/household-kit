// SPDX-License-Identifier: GPL-3.0-only

//! The record of occupied space (design §7 and §11).
//!
//! Everything drawn is entered with its ink, its kind, its identity and its
//! owner's identity. Everything placed later asks the record first: a
//! candidate is judged against everything it would meet or come too near,
//! by the covering table and the distance table, and every finding names
//! the occupant and the row of the table that gave it, so a caller can
//! report why a place was rejected. Nothing entered is ever moved.
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
use crate::rules::{CoveringRef, DistanceRow, Preference, Relation, Rules, Sizes, Verdict};

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
    pub owner: Option<Id>,
    pub role: Option<Role>,
}

impl Occupant {
    pub fn new(id: Id, kind: Kind, ink: Ink) -> Self {
        Self {
            ink,
            kind,
            id,
            owner: None,
            role: None,
        }
    }

    pub fn owned_by(mut self, owner: Id) -> Self {
        self.owner = Some(owner);
        self
    }

    pub fn with_role(mut self, role: Role) -> Self {
        self.role = Some(role);
        self
    }

    /// Whether `other` is this occupant's own: one owns the other, or both
    /// have the same owner.
    pub fn is_own(&self, other: &Occupant) -> bool {
        self.owner.as_ref() == Some(&other.id)
            || other.owner.as_ref() == Some(&self.id)
            || (self.owner.is_some() && self.owner == other.owner)
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
    /// Closer than its ideal and not below the least distance: allowed,
    /// and flagged so that squeezing is a choice.
    Squeezed {
        against: Against,
        gap: f64,
        ideal: f64,
        row: DistanceRow,
    },
}

impl Finding {
    pub fn against(&self) -> &Against {
        match self {
            Self::Meets { against, .. }
            | Self::BelowLeast { against, .. }
            | Self::Squeezed { against, .. } => against,
        }
    }

    /// Whether this finding rejects the place at `level`. A `Never` verdict
    /// and the least distance always reject; squeezing below an ideal is
    /// permitted from [`Level::Strong`].
    pub fn rejects_at(&self, level: Level) -> bool {
        match self {
            Self::Meets { verdict, .. } => verdict.rejects(),
            Self::BelowLeast { .. } => true,
            Self::Squeezed { .. } => level < Level::Strong,
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

    /// The verdicts of everything the candidate meets.
    pub fn verdicts(&self) -> impl Iterator<Item = Verdict> + '_ {
        self.findings.iter().filter_map(|f| match f {
            Finding::Meets { verdict, .. } => Some(*verdict),
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
        }
    }

    /// The figure's edge: a candidate not wholly inside it meets the edge.
    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = Some(bounds);
        self
    }

    pub fn rules(&self) -> &Rules {
        &self.rules
    }

    /// The tables, to change a row or a number.
    pub fn rules_mut(&mut self) -> &mut Rules {
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
        self.declared
            .retain(|(x, y, _)| !((x == &a && y == &b) || (x == &b && y == &a)));
        self.declared.push((a, b, relation));
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
        self.occupants.push(occupant);
    }

    /// How many occupants there are; with [`Record::truncate`], lets a
    /// caller in this crate try a set of places and take them back.
    pub(crate) fn len(&self) -> usize {
        self.occupants.len()
    }

    pub(crate) fn truncate(&mut self, len: usize) {
        self.occupants.truncate(len);
    }

    /// Judges a candidate against the figure's edge and everything it would
    /// meet or come too near.
    pub fn judge(&self, candidate: &Occupant) -> Judgement {
        let covering = &self.rules.covering;
        let spacing = &self.rules.distances.diagram;
        let placed = (candidate.kind, candidate.role.as_ref());
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
        for occupant in &self.occupants {
            let against = || Against::Occupant(occupant.id.clone());
            let relation = self.relation(candidate, occupant);
            let gap = candidate.ink.gap(&occupant.ink);
            if gap <= 0.0 {
                let (verdict, row) = if relation == Relation::MeantToMeet {
                    (Verdict::MeantToMeet, CoveringRef::Declared)
                } else {
                    covering.verdict(
                        placed,
                        (occupant.kind, occupant.role.as_ref()),
                        candidate.is_own(occupant),
                    )
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
            let Some((ideal, ideal_row)) = spacing.ideal(relation, &self.sizes) else {
                continue;
            };
            let (least, least_row) =
                spacing.least_between(candidate.kind, occupant.kind, &self.sizes);
            if gap < least - FIT_EPSILON {
                findings.push(Finding::BelowLeast {
                    against: against(),
                    gap,
                    least,
                    row: least_row,
                });
            } else if gap < ideal - FIT_EPSILON {
                findings.push(Finding::Squeezed {
                    against: against(),
                    gap,
                    ideal,
                    row: ideal_row,
                });
            }
        }
        Judgement { findings }
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
        r.enter(
            hline("cmp-a/line", Kind::ValueLine, 50.0)
                .owned_by(id("cmp-a"))
                .with_role(Role::comparison()),
        );
        let name = |owner: &str| {
            label("name", Rect::new(100.0, 44.5, 30.0, 11.0))
                .owned_by(id(owner))
                .with_role(Role::comparison())
        };
        assert_eq!(
            verdict_on(&r, &name("cmp-b"), "cmp-a/line").0,
            Verdict::Never
        );
        // In its own comparison's line: the row for words in a value line.
        assert_eq!(
            verdict_on(&r, &name("cmp-a"), "cmp-a/line").0,
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
