// SPDX-License-Identifier: GPL-3.0-only

//! Leaders: a label joined to what it names by a connector (design §7 and
//! §9).
//!
//! A label that cannot stand beside what it names is joined to it by a
//! leader. "A place counts only if its connector is valid too" (design §9):
//! a label's place and its leader are judged together, and what is taken
//! carries the leader, so the caller draws exactly what was judged.
//!
//! # How a leader is built
//!
//! A leader belongs to its text and to its target: it enters the record as
//! a [`Kind::Connector`] segment owned by both (semantics C1). Its target
//! ([`Target`]) may be of any kind: a bar, a comparison mark, a line, or a
//! region nothing has drawn, such as an empty slot. A target that is in the
//! record is found there by identity; one that is not is given by its
//! shape and is never entered: "Anything | a mark added only to attach it |
//! never". Entering a label with its leader enters the two and nothing
//! else.
//!
//! For each form of path in the rules' order ([`Connectors::paths`]), and
//! for each edge of the text the rules allow ([`Connectors::edges`]), the
//! leader starts at the midpoint of that edge ("a connector starts at the
//! midpoint of an edge of its text", design §9, provisional; moved out by
//! [`Connectors::start_off`], zero by default) and runs straight to the
//! target's nearest point. It is judged twice:
//!
//! - **by the record**, as any candidate, with its text as if entered: it
//!   meets its own text and its own target by design ("meant to meet"); it
//!   never crosses other text, a data mark or comparison mark not its
//!   target, or another connector; it may cross a value line but never runs
//!   along one (running along and crossing are told apart by the angle
//!   between them and their overlap, [`Record::runs_along`]); along or close
//!   to a data line it is less preferred. Where it ends on a target lying
//!   on something the order of drawing puts beneath the target (a
//!   comparison mark across its bar), that thing is covered there.
//! - **by her path rules** ([`Connectors::rows`]): leaving its edge more
//!   than about 45° from square, or arriving at less than about 60° to the
//!   outline it lands on, puts the leader later in order; leaving back
//!   over its own text overlaps it and is refused. At a corner of a target
//!   the leader lands on both edges, and the shallower angle counts. The
//!   angles are starting values, and the verdicts data.
//!
//! Of the paths of one form that nothing refuses, the one with the best
//! position in the list of places wins, then the shortest: "of the edges,
//! the one that gives the shortest path meeting the rules". A form whose
//! best path is in order is taken; otherwise the next form is tried and
//! the best of all is taken. The one-bend form is the next entry of the
//! list and is not built: it finds no path ([`LeaderStop::NotBuilt`]).
//!
//! # A label's places with a leader
//!
//! [`judge_with_leader`] tries the caller's places for one label in order.
//! At each, the label is judged by the record; then its leader is built and
//! judged with the label pending. A place whose label or leader is refused
//! is passed over; a place whose label or leader is later in order waits
//! behind every place that is not, as label sets' patterns do. The result
//! ([`WithLeader`]) holds the label and its leader; [`enter_with_leader`]
//! enters both, with any cuts the label's halo makes, into the record they
//! were judged against.
//!
//! # A worked example
//!
//! Her 11 px text gives a least distance of 2.2 px and an unrelated ideal of
//! 7.26 px. Two bars, a at x 60 to 80 and b at x 100 to 120, run from
//! y = 100 to 200. A comparison's mark lies across b at y 139 to 141, owned
//! by b. Its name, 30 by 11 px, is led by a leader 1 px wide to the mark,
//! and the caller offers two places, both level with the mark.
//!
//! - Left of a, at (20, 134.5): from the right edge's midpoint (50, 140) the
//!   leader runs to (100, 140) through bar a, which is not its target:
//!   never. From the top or bottom edge it crosses a too, and from the left
//!   edge it would leave back over its own text. The place is passed over.
//! - Right of b, at (128, 134.5), 8 px clear of b: from the left edge's
//!   midpoint (128, 140) to the mark's end at (120, 140), 8 px, leaving
//!   square and arriving square. Its end touches bar b where the mark lies
//!   on it, and the mark is drawn over its bar, so it meets the mark, its
//!   own target, and its own text, and nothing else. The other three edges
//!   would leave back over the text. The place is taken, in order, with
//!   that leader.
//!
//! (Checked by the test `worked_example_a_name_led_to_its_comparison_mark`.)

use crate::geometry::{FIT_EPSILON, Point, Rect};
use crate::identity::Id;
use crate::kinds::Kind;
use crate::labels::{PlacedLabel, Side, Stale};
use crate::level::Level;
use crate::record::{Finding, Ink, Judgement, Occupant, Record, Revision, unit};
use crate::rules::{Connectors, PathAt, PathForm, Preference, Verdict};

/// What a leader is drawn to.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    /// The target's identity: the leader is its own.
    pub id: Id,
    /// Its shape where it is not in the record (an empty slot); none to
    /// take every ink the record holds under `id` (a line's pieces).
    pub shape: Option<Ink>,
}

impl Target {
    /// A target in the record, found by its identity.
    pub fn recorded(id: Id) -> Self {
        Self { id, shape: None }
    }

    /// A target that is not in the record, by its shape. It is never
    /// entered.
    pub fn shape(id: Id, shape: Ink) -> Self {
        Self {
            id,
            shape: Some(shape),
        }
    }

    fn inks(&self, record: &Record) -> Vec<Ink> {
        match self.shape {
            Some(ink) => vec![ink],
            None => record
                .occupants()
                .iter()
                .filter(|o| o.id == self.id)
                .map(|o| o.ink)
                .collect(),
        }
    }
}

/// What a label is led to, and how wide its leader is drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Leading {
    pub target: Target,
    /// The leader's stroke width.
    pub width: f64,
}

impl Leading {
    pub fn new(target: Target, width: f64) -> Self {
        Self { target, width }
    }
}

/// One of her path rules a leader breaks: the row of
/// [`Connectors::rows`], the angle measured and the rule's limit, in
/// degrees.
#[derive(Clone, Debug, PartialEq)]
pub struct PathFinding {
    pub at: PathAt,
    pub angle: f64,
    pub limit: f64,
    pub verdict: Verdict,
    /// The row's index in [`Connectors::rows`].
    pub row: usize,
}

/// A leader, judged: what to draw, and what was found about it.
#[derive(Clone, Debug, PartialEq)]
pub struct Leader {
    /// The leader's identity: its text's, with `/leader`.
    pub id: Id,
    /// The edge of its text it starts from.
    pub edge: Side,
    pub form: PathForm,
    /// Where it starts, at its text's edge.
    pub from: Point,
    /// Where it ends, on its target.
    pub to: Point,
    pub width: f64,
    /// How far off square it leaves its edge, in degrees.
    pub leaves: f64,
    /// The angle at which it arrives at the outline it lands on, in
    /// degrees: 90 is square.
    pub arrives: f64,
    /// What enters the record: a connector owned by its text and its
    /// target.
    pub occupant: Occupant,
    /// What the record found.
    pub judgement: Judgement,
    /// The path rules it breaks.
    pub path: Vec<PathFinding>,
}

impl Leader {
    pub fn length(&self) -> f64 {
        (self.to.x - self.from.x).hypot(self.to.y - self.from.y)
    }

    /// Its position in a list of places: the latest the record's verdicts
    /// or the path rules give it.
    pub fn preference(&self) -> Preference {
        self.path
            .iter()
            .map(|f| f.verdict.preference())
            .fold(self.judgement.preference(), Preference::max)
    }

    /// Why it is refused at `level`, if it is.
    pub fn rejection_at(&self, level: Level) -> Option<LeaderStop> {
        if let Some(finding) = self.judgement.rejection_at(level) {
            return Some(LeaderStop::Rejected(finding.clone()));
        }
        self.path
            .iter()
            .find(|f| f.verdict.rejects())
            .map(|f| LeaderStop::Path(f.clone()))
    }
}

/// Why no leader of one form came from one edge.
#[derive(Clone, Debug, PartialEq)]
pub enum LeaderStop {
    /// The form of path is not built yet.
    NotBuilt,
    /// The target has no ink: it is not in the record and no shape was
    /// given.
    NoTarget,
    /// The edge's start already touches the target: there is nothing to
    /// lead.
    TouchesTarget,
    /// The record's finding that refuses it.
    Rejected(Finding),
    /// The path rule that refuses it.
    Path(PathFinding),
}

/// One edge and form tried, and why it gave no leader.
#[derive(Clone, Debug, PartialEq)]
pub struct LeaderTried {
    pub edge: Side,
    pub form: PathForm,
    pub stop: LeaderStop,
}

/// No leader from any edge, in any form: each one tried, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct NoLeader {
    pub tried: Vec<LeaderTried>,
}

/// Where a leader from `edge` of `text` starts, and the edge's outward
/// direction.
fn start(text: Rect, edge: Side, off: f64) -> (Point, Point) {
    let centre = text.centre();
    match edge {
        Side::Above => (Point::new(centre.x, text.y - off), Point::new(0.0, -1.0)),
        Side::Below => (
            Point::new(centre.x, text.bottom() + off),
            Point::new(0.0, 1.0),
        ),
        Side::Left => (Point::new(text.x - off, centre.y), Point::new(-1.0, 0.0)),
        Side::Right => (
            Point::new(text.right() + off, centre.y),
            Point::new(1.0, 0.0),
        ),
    }
}

/// The point of `ink` nearest `p`: of an area, on or in it; of a line, on
/// its centre line.
fn nearest(ink: &Ink, p: Point) -> Point {
    match *ink {
        Ink::Rect(r) => Point::new(p.x.clamp(r.x, r.right()), p.y.clamp(r.y, r.bottom())),
        Ink::Segment { from, to, .. } => {
            let (dx, dy) = (to.x - from.x, to.y - from.y);
            let length2 = dx * dx + dy * dy;
            let t = if length2 == 0.0 {
                0.0
            } else {
                (((p.x - from.x) * dx + (p.y - from.y) * dy) / length2).clamp(0.0, 1.0)
            };
            Point::new(from.x + t * dx, from.y + t * dy)
        }
    }
}

/// The angle between two directions as lines, in degrees from 0 to 90.
fn between(d: Point, u: Point) -> f64 {
    (d.x * u.x + d.y * u.y).abs().min(1.0).acos().to_degrees()
}

/// The angle at which a leader going `d` arrives at `ink` at `q`: against a
/// line, the angle to it; against an area, the angle to the edge it lands
/// on, the shallower of two at a corner. A point has no outline: square.
fn arrival(ink: &Ink, q: Point, d: Point) -> f64 {
    match *ink {
        Ink::Rect(r) => {
            let on = |a: f64, b: f64| (a - b).abs() <= FIT_EPSILON;
            let mut angle: f64 = 90.0;
            if r.height > FIT_EPSILON && (on(q.x, r.x) || on(q.x, r.right())) {
                angle = angle.min(between(d, Point::new(0.0, 1.0)));
            }
            if r.width > FIT_EPSILON && (on(q.y, r.y) || on(q.y, r.bottom())) {
                angle = angle.min(between(d, Point::new(1.0, 0.0)));
            }
            angle
        }
        Ink::Segment { from, to, .. } => unit(from, to).map_or(90.0, |u| between(d, u)),
    }
}

/// The path rules a leader leaving at `leaves` and arriving at `arrives`
/// breaks.
fn path_findings(rules: &Connectors, leaves: f64, arrives: f64) -> Vec<PathFinding> {
    let mut found = Vec::new();
    let mut find = |at, angle, limit| {
        if let Some((row, verdict)) = rules.verdict(at) {
            found.push(PathFinding {
                at,
                angle,
                limit,
                verdict,
                row,
            });
        }
    };
    if leaves >= 90.0 - FIT_EPSILON {
        find(PathAt::LeavesInward, leaves, 90.0);
    } else if leaves > rules.leave_within + FIT_EPSILON {
        find(PathAt::LeavesOblique, leaves, rules.leave_within);
    }
    if arrives < rules.arrive_at_least - FIT_EPSILON {
        find(PathAt::ArrivesShallow, arrives, rules.arrive_at_least);
    }
    found
}

/// The straight leader from `edge` of `text` to the nearest of `inks`,
/// judged; or why there is none.
fn straight(
    record: &Record,
    text: &Occupant,
    leading: &Leading,
    inks: &[Ink],
    edge: Side,
    pending: &[Occupant],
) -> Result<Leader, LeaderStop> {
    let rules = &record.rules().connectors;
    let off = rules.start_off.resolve(record.sizes());
    let (from, normal) = start(text.ink.bounds(), edge, off);
    let (ink, to) = inks
        .iter()
        .map(|ink| (ink, nearest(ink, from)))
        .min_by(|(_, a), (_, b)| {
            let da = (a.x - from.x).hypot(a.y - from.y);
            let db = (b.x - from.x).hypot(b.y - from.y);
            da.total_cmp(&db)
        })
        .ok_or(LeaderStop::NoTarget)?;
    let d = unit(from, to)
        .filter(|_| (to.x - from.x).hypot(to.y - from.y) > FIT_EPSILON)
        .ok_or(LeaderStop::TouchesTarget)?;
    let leaves = (d.x * normal.x + d.y * normal.y)
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees();
    let arrives = arrival(ink, to, d);
    let occupant = Occupant::new(
        text.id.child("leader"),
        Kind::Connector,
        Ink::segment(from, to, leading.width),
    )
    .owned_by(text.id.clone())
    .also_owned_by(leading.target.id.clone());
    let judgement = record.judge_with(&occupant, pending);
    Ok(Leader {
        id: occupant.id.clone(),
        edge,
        form: PathForm::Straight,
        from,
        to,
        width: leading.width,
        leaves,
        arrives,
        occupant,
        judgement,
        path: path_findings(rules, leaves, arrives),
    })
}

/// Builds and judges the leader of `text` (an occupant, entered or not) to
/// `leading`'s target, against the record and `pending`, with the text
/// counted as pending if it is not entered. Changes nothing.
pub fn judge_leader(
    record: &Record,
    text: &Occupant,
    leading: &Leading,
    pending: &[Occupant],
    level: Level,
) -> Result<Leader, NoLeader> {
    let rules = &record.rules().connectors;
    let inks = leading.target.inks(record);
    let mut pending = pending.to_vec();
    if record.get(&text.id).is_none() && !pending.iter().any(|o| o.id == text.id) {
        pending.push(text.clone());
    }
    let mut tried = Vec::new();
    let mut waiting: Option<Leader> = None;
    for &form in &rules.paths {
        let mut best: Option<Leader> = None;
        for &edge in &rules.edges {
            let built = match form {
                PathForm::Straight => straight(record, text, leading, &inks, edge, &pending),
                PathForm::OneBend => Err(LeaderStop::NotBuilt),
            };
            let leader = match built {
                Ok(leader) => match leader.rejection_at(level) {
                    None => leader,
                    Some(stop) => {
                        tried.push(LeaderTried { edge, form, stop });
                        continue;
                    }
                },
                Err(stop) => {
                    tried.push(LeaderTried { edge, form, stop });
                    continue;
                }
            };
            let better = best.as_ref().is_none_or(|b| {
                (leader.preference(), leader.length()) < (b.preference(), b.length())
            });
            if better {
                best = Some(leader);
            }
        }
        if let Some(leader) = best {
            if leader.preference() == Preference::InOrder {
                return Ok(leader);
            }
            if waiting
                .as_ref()
                .is_none_or(|w| leader.preference() < w.preference())
            {
                waiting = Some(leader);
            }
        }
    }
    waiting.ok_or(NoLeader { tried })
}

/// A label placed with its leader: judged, and not yet entered unless
/// [`place_with_leader`] made it.
#[derive(Clone, Debug, PartialEq)]
pub struct WithLeader {
    /// The place's index in the caller's list.
    pub place: usize,
    pub label: PlacedLabel,
    pub leader: Leader,
    /// The later of the label's and the leader's positions in the list.
    pub preference: Preference,
    /// The record's revision when they were judged.
    pub judged_at: Revision,
}

/// Why a place was passed over.
#[derive(Clone, Debug, PartialEq)]
pub enum PlaceStop {
    /// The record refuses the label there.
    Label(Finding),
    /// No leader from there is valid.
    Leader(NoLeader),
}

/// A place passed over.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaceStopped {
    /// The place's index in the caller's list.
    pub place: usize,
    pub stop: PlaceStop,
}

/// A label with no place left: every place, in order, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct NoPlace {
    pub label: Id,
    pub stopped: Vec<PlaceStopped>,
}

/// Judges `label` at each of `places` in the caller's order, with its
/// leader to `leading`'s target: the first place where neither the label
/// nor its leader is refused at `level`, a place later in order waiting
/// behind every place that is not. `label` gives the label's identity,
/// kind, owners and role; its ink is replaced by each place. Changes
/// nothing.
pub fn judge_with_leader(
    record: &Record,
    label: &Occupant,
    places: &[Rect],
    leading: &Leading,
    level: Level,
) -> Result<WithLeader, NoPlace> {
    let mut stopped = Vec::new();
    let mut waiting: Option<WithLeader> = None;
    for (place, &rect) in places.iter().enumerate() {
        let occupant = Occupant {
            ink: Ink::Rect(rect),
            ..label.clone()
        };
        let judgement = record.judge(&occupant);
        if let Some(finding) = judgement.rejection_at(level) {
            stopped.push(PlaceStopped {
                place,
                stop: PlaceStop::Label(finding.clone()),
            });
            continue;
        }
        let leader = match judge_leader(
            record,
            &occupant,
            leading,
            std::slice::from_ref(&occupant),
            level,
        ) {
            Ok(leader) => leader,
            Err(no) => {
                stopped.push(PlaceStopped {
                    place,
                    stop: PlaceStop::Leader(no),
                });
                continue;
            }
        };
        let preference = judgement.preference().max(leader.preference());
        let placed = WithLeader {
            place,
            label: PlacedLabel {
                id: occupant.id.clone(),
                rect,
                occupant,
                judgement,
            },
            leader,
            preference,
            judged_at: record.revision(),
        };
        if preference == Preference::InOrder {
            return Ok(placed);
        }
        if waiting.as_ref().is_none_or(|w| preference < w.preference) {
            waiting = Some(placed);
        }
    }
    waiting.ok_or_else(|| NoPlace {
        label: label.id.clone(),
        stopped,
    })
}

/// Enters a label and its leader, and the cuts the label's halo makes, into
/// the record they were judged against. Fails, entering nothing, if the
/// record has changed since (semantics C9).
pub fn enter_with_leader(record: &mut Record, placed: &WithLeader) -> Result<(), Stale> {
    if record.revision() != placed.judged_at {
        return Err(Stale {
            judged_at: placed.judged_at,
            now: record.revision(),
        });
    }
    record.enter_all([
        placed.label.occupant.clone(),
        placed.leader.occupant.clone(),
    ]);
    record.enter_cuts(placed.label.cuts().iter().cloned());
    Ok(())
}

/// Judges a label's places with its leader and enters what it takes:
/// [`judge_with_leader`], then [`enter_with_leader`]. On failure the record
/// is unchanged.
pub fn place_with_leader(
    record: &mut Record,
    label: &Occupant,
    places: &[Rect],
    leading: &Leading,
    level: Level,
) -> Result<WithLeader, NoPlace> {
    let placed = judge_with_leader(record, label, places, leading, level)?;
    enter_with_leader(record, &placed).expect("judged against this record a moment ago");
    Ok(placed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::Role;
    use crate::record::Against;
    use crate::rules::{Rules, Sizes};

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

    fn bar(name: &str, rect: Rect) -> Occupant {
        Occupant::new(id(name), Kind::DataMark, Ink::Rect(rect))
    }

    fn words(name: &str, rect: Rect) -> Occupant {
        Occupant::new(id(name), Kind::Words, Ink::Rect(rect))
    }

    fn line(name: &str, kind: Kind, from: (f64, f64), to: (f64, f64)) -> Occupant {
        Occupant::new(
            id(name),
            kind,
            Ink::segment(Point::new(from.0, from.1), Point::new(to.0, to.1), 1.0),
        )
    }

    fn to(target: &str) -> Leading {
        Leading::new(Target::recorded(id(target)), 1.0)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    /// The covering row a finding names.
    fn row_of(r: &Record, finding: &Finding) -> String {
        match finding {
            Finding::Meets {
                row: crate::rules::CoveringRef::Row(i),
                ..
            } => r.rules().covering.rows[*i].name.clone(),
            other => format!("{other:?}"),
        }
    }

    /// Why the straight leader from `edge` was refused.
    fn stop_from(no: &NoLeader, edge: Side) -> &LeaderStop {
        &no.tried
            .iter()
            .find(|t| t.edge == edge && t.form == PathForm::Straight)
            .expect("tried")
            .stop
    }

    #[test]
    fn worked_example_a_name_led_to_its_comparison_mark() {
        let mut r = record();
        r.enter(bar("a", Rect::new(60.0, 100.0, 20.0, 100.0)));
        r.enter(bar("b", Rect::new(100.0, 100.0, 20.0, 100.0)));
        r.enter(
            Occupant::new(
                id("b/target"),
                Kind::ComparisonMark,
                Ink::Rect(Rect::new(100.0, 139.0, 20.0, 2.0)),
            )
            .owned_by(id("b")),
        );
        let name = words("target/name", Rect::default()).owned_by(id("target"));
        let places = [
            Rect::new(20.0, 134.5, 30.0, 11.0),
            Rect::new(128.0, 134.5, 30.0, 11.0),
        ];
        let before = r.occupants().len();
        let placed =
            place_with_leader(&mut r, &name, &places, &to("b/target"), Level::Preferred).unwrap();
        // Left of a: every edge refused, so the place is passed over.
        let leader = &placed.leader;
        assert_eq!(placed.place, 1);
        assert_eq!(placed.preference, Preference::InOrder);
        assert_eq!(leader.edge, Side::Left);
        assert_eq!(
            (leader.from, leader.to),
            (Point::new(128.0, 140.0), Point::new(120.0, 140.0))
        );
        assert!(close(leader.length(), 8.0));
        assert!(close(leader.leaves, 0.0) && close(leader.arrives, 90.0));
        assert_eq!(leader.path, []);
        // It meets its own target and its own text, and not bar b.
        let met: Vec<_> = leader
            .judgement
            .findings
            .iter()
            .map(|f| (f.against().clone(), f.clone()))
            .filter_map(|(a, f)| match (a, f) {
                (Against::Occupant(on), Finding::Meets { verdict, .. }) => {
                    Some((on.as_str().to_owned(), verdict))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            met,
            [
                ("b/target".to_owned(), Verdict::MeantToMeet),
                ("target/name".to_owned(), Verdict::MeantToMeet),
            ]
        );
        assert_eq!(leader.occupant.owners, [id("target/name"), id("b/target")]);
        // The label and its leader are entered, and nothing else.
        assert_eq!(r.occupants().len(), before + 2);
        assert_eq!(
            r.get(&id("target/name/leader")).map(|o| o.kind),
            Some(Kind::Connector)
        );
        // The first place, judged alone, says why.
        let failure = judge_with_leader(
            &record_with_bars(),
            &name,
            &places[..1],
            &to("b/target"),
            Level::Preferred,
        )
        .unwrap_err();
        let [
            PlaceStopped {
                place: 0,
                stop: PlaceStop::Leader(no),
            },
        ] = &failure.stopped[..]
        else {
            panic!("{failure:?}")
        };
        let LeaderStop::Rejected(finding) = stop_from(no, Side::Right) else {
            panic!("{no:?}")
        };
        assert_eq!(finding.against(), &Against::Occupant(id("a")));
        assert_eq!(
            row_of(&r, finding),
            "a connector through a data mark not its target"
        );
        for edge in [Side::Above, Side::Below] {
            assert!(matches!(stop_from(no, edge), LeaderStop::Rejected(_)));
        }
        assert!(matches!(
            stop_from(no, Side::Left),
            LeaderStop::Rejected(_) | LeaderStop::Path(_)
        ));
    }

    /// The worked example's bars and mark, nothing placed.
    fn record_with_bars() -> Record {
        let mut r = record();
        r.enter(bar("a", Rect::new(60.0, 100.0, 20.0, 100.0)));
        r.enter(bar("b", Rect::new(100.0, 100.0, 20.0, 100.0)));
        r.enter(
            Occupant::new(
                id("b/target"),
                Kind::ComparisonMark,
                Ink::Rect(Rect::new(100.0, 139.0, 20.0, 2.0)),
            )
            .owned_by(id("b")),
        );
        r
    }

    #[test]
    fn a_leader_to_a_comparison_mark_meets_the_mark_not_the_bar_beneath() {
        let r = record_with_bars();
        let name = words("n", Rect::new(128.0, 134.5, 30.0, 11.0));
        let leader = judge_leader(&r, &name, &to("b/target"), &[], Level::Preferred).unwrap();
        assert_eq!(leader.preference(), Preference::InOrder);
        // Without the order of drawing the mark does not cover its bar, and
        // the leader touching the bar's edge is refused.
        let mut r = record_with_bars();
        r.rules_mut()
            .drawing_order
            .retain(|row| !row.above.kinds.contains(Kind::ComparisonMark));
        let no = judge_leader(&r, &name, &to("b/target"), &[], Level::Preferred).unwrap_err();
        let LeaderStop::Rejected(finding) = stop_from(&no, Side::Left) else {
            panic!("{no:?}")
        };
        assert_eq!(finding.against(), &Against::Occupant(id("b")));
    }

    #[test]
    fn a_leader_over_text_or_a_bar_not_its_target_is_refused() {
        let mut r = record();
        r.enter(bar("t", Rect::new(120.0, 100.0, 20.0, 100.0)));
        r.enter(words("other", Rect::new(70.0, 134.0, 20.0, 11.0)));
        let name = words("n", Rect::new(20.0, 134.5, 30.0, 11.0));
        let no = judge_leader(&r, &name, &to("t"), &[], Level::AllPermitted).unwrap_err();
        let LeaderStop::Rejected(finding) = stop_from(&no, Side::Right) else {
            panic!("{no:?}")
        };
        assert_eq!(finding.against(), &Against::Occupant(id("other")));
        assert_eq!(row_of(&r, finding), "a connector through text");
        // A bar between: through a data mark not its target.
        let mut r = record();
        r.enter(bar("t", Rect::new(120.0, 100.0, 20.0, 100.0)));
        r.enter(bar("between", Rect::new(70.0, 100.0, 20.0, 100.0)));
        let no = judge_leader(&r, &name, &to("t"), &[], Level::AllPermitted).unwrap_err();
        let LeaderStop::Rejected(finding) = stop_from(&no, Side::Right) else {
            panic!("{no:?}")
        };
        assert_eq!(finding.against(), &Against::Occupant(id("between")));
        assert_eq!(
            row_of(&r, finding),
            "a connector through a data mark not its target"
        );
    }

    #[test]
    fn crossing_a_gridline_is_allowed_and_running_along_one_is_not() {
        let mut r = record();
        r.enter(
            line("grid", Kind::ValueLine, (0.0, 50.0), (300.0, 50.0)).with_role(Role::gridline()),
        );
        // Straight down across it, from the note's foot to the bar's top.
        r.enter(bar("t", Rect::new(200.0, 60.0, 20.0, 100.0)));
        let note = words("note", Rect::new(195.0, 20.0, 30.0, 11.0));
        let leader = judge_leader(&r, &note, &to("t"), &[], Level::Preferred).unwrap();
        assert_eq!(leader.edge, Side::Below);
        assert_eq!(leader.preference(), Preference::InOrder);
        assert!(leader.judgement.findings.iter().any(|f| matches!(
            f,
            Finding::Meets { against: Against::Occupant(on), verdict: Verdict::Allowed, .. }
                if on.as_str() == "grid"
        )));
        // To a bar far along the line, it would cross the gridline at under
        // 7°: that is running along it, from either edge it could take.
        let mut r = record();
        r.enter(
            line("grid", Kind::ValueLine, (0.0, 50.0), (300.0, 50.0)).with_role(Role::gridline()),
        );
        r.enter(bar("t", Rect::new(250.0, 60.0, 20.0, 100.0)));
        let note = words("note", Rect::new(10.0, 30.0, 30.0, 11.0));
        let no = judge_leader(&r, &note, &to("t"), &[], Level::AllPermitted).unwrap_err();
        for edge in [Side::Right, Side::Below] {
            let LeaderStop::Rejected(finding) = stop_from(&no, edge) else {
                panic!("{no:?}")
            };
            assert_eq!(row_of(&r, finding), "a connector along a value line");
        }
        // As segments: lying on the line, and crossing it shallowly, both run
        // along it; crossing it at 45° does not.
        let connector = |from: (f64, f64), to: (f64, f64)| line("c", Kind::Connector, from, to);
        let grid = r.get(&id("grid")).unwrap().clone();
        assert!(r.runs_along(&connector((20.0, 50.0), (120.0, 50.0)), &grid));
        assert!(r.runs_along(&connector((20.0, 47.0), (120.0, 53.0)), &grid));
        assert!(!r.runs_along(&connector((20.0, 20.0), (80.0, 80.0)), &grid));
        // Touching it end to end is not running along it.
        assert!(!r.runs_along(&connector((300.0, 50.0), (350.0, 50.0)), &grid));
        // The angle is data.
        r.rules_mut().connectors.along_below = 2.0;
        assert!(!r.runs_along(&connector((20.0, 47.0), (120.0, 53.0)), &grid));
    }

    #[test]
    fn a_leader_close_to_a_data_line_comes_later_in_order() {
        let mut r = record();
        // A data line from x = 150 at y = 100, and a target whose left edge
        // a leader at y = 104 reaches square, 3 px from the line's ink.
        r.enter(line(
            "series",
            Kind::DataLine,
            (150.0, 100.0),
            (300.0, 100.0),
        ));
        r.enter(bar("t", Rect::new(200.0, 102.0, 20.0, 60.0)));
        let name = words("n", Rect::default());
        let near = Rect::new(60.0, 98.5, 30.0, 11.0);
        let clear = Rect::new(60.0, 130.0, 30.0, 11.0);
        let alone = judge_with_leader(&r, &name, &[near], &to("t"), Level::Preferred).unwrap();
        assert_eq!(alone.place, 0);
        assert_eq!(alone.preference, Preference::LessPreferred);
        assert!(matches!(
            alone.leader.judgement.findings[..],
            [Finding::Near { verdict: Verdict::LessPreferred, ref against, .. }, ..]
                if against == &Against::Occupant(id("series"))
        ));
        let both =
            judge_with_leader(&r, &name, &[near, clear], &to("t"), Level::Preferred).unwrap();
        assert_eq!(both.place, 1);
        assert_eq!(both.preference, Preference::InOrder);
    }

    #[test]
    fn two_leaders_never_cross() {
        let mut r = record();
        r.enter(bar("t1", Rect::new(200.0, 150.0, 20.0, 50.0)));
        r.enter(bar("t2", Rect::new(260.0, 150.0, 20.0, 50.0)));
        let first = words("first", Rect::default());
        place_with_leader(
            &mut r,
            &first,
            &[Rect::new(255.0, 110.0, 30.0, 11.0)],
            &to("t1"),
            Level::Preferred,
        )
        .unwrap();
        let second = words("second", Rect::new(185.0, 110.0, 30.0, 11.0));
        let no = judge_leader(&r, &second, &to("t2"), &[], Level::AllPermitted).unwrap_err();
        for edge in [Side::Right, Side::Below] {
            let LeaderStop::Rejected(finding) = stop_from(&no, edge) else {
                panic!("{no:?}")
            };
            assert_eq!(finding.against(), &Against::Occupant(id("first/leader")));
            assert_eq!(row_of(&r, finding), "a connector across another connector");
        }
    }

    #[test]
    fn the_angle_rules() {
        let only = |r: &mut Record, edge: Side| r.rules_mut().connectors.edges = vec![edge];
        let text = words("n", Rect::new(100.0, 100.0, 30.0, 11.0));
        // Leaving its right edge 63.4° off square, square onto a slanting
        // line: later in order, not refused.
        let mut r = record();
        r.enter(line("t", Kind::DataLine, (100.0, 300.0), (400.0, 150.0)));
        only(&mut r, Side::Right);
        let leader = judge_leader(&r, &text, &to("t"), &[], Level::Preferred).unwrap();
        assert!(close(leader.arrives, 90.0));
        let [finding] = &leader.path[..] else {
            panic!("{leader:?}")
        };
        assert_eq!(finding.at, PathAt::LeavesOblique);
        assert!((finding.angle - 63.435).abs() < 1e-3 && finding.limit == 45.0);
        assert_eq!(leader.preference(), Preference::LessPreferred);
        // The angle is data.
        r.rules_mut().connectors.leave_within = 70.0;
        let leader = judge_leader(&r, &text, &to("t"), &[], Level::Preferred).unwrap();
        assert_eq!(leader.path, []);
        assert_eq!(leader.preference(), Preference::InOrder);
        // From its foot to a bar's corner: it leaves 26.8° off square, and
        // lands on the corner 26.8° to the bar's side: shallow.
        let mut r = record();
        r.enter(bar("t", Rect::new(160.0, 200.0, 20.0, 50.0)));
        only(&mut r, Side::Below);
        let leader = judge_leader(&r, &text, &to("t"), &[], Level::Preferred).unwrap();
        let [finding] = &leader.path[..] else {
            panic!("{leader:?}")
        };
        assert_eq!(finding.at, PathAt::ArrivesShallow);
        assert!((finding.angle - 26.82).abs() < 0.01 && finding.limit == 60.0);
        assert_eq!(leader.preference(), Preference::LessPreferred);
        // The verdict is data: refused if the row says never.
        let row = r
            .rules()
            .connectors
            .row_named("a connector arriving at less than about 60°")
            .unwrap();
        r.rules_mut().connectors.rows[row].verdict = Verdict::Never;
        let no = judge_leader(&r, &text, &to("t"), &[], Level::AllPermitted).unwrap_err();
        assert!(matches!(
            stop_from(&no, Side::Below),
            LeaderStop::Path(PathFinding {
                at: PathAt::ArrivesShallow,
                ..
            })
        ));
        // From its top edge it would leave back over its own text: refused.
        let mut r = record();
        r.enter(bar("t", Rect::new(160.0, 200.0, 20.0, 50.0)));
        only(&mut r, Side::Above);
        let no = judge_leader(&r, &text, &to("t"), &[], Level::AllPermitted).unwrap_err();
        assert!(matches!(
            stop_from(&no, Side::Above),
            LeaderStop::Path(PathFinding {
                at: PathAt::LeavesInward,
                verdict: Verdict::Never,
                ..
            })
        ));
        // The one-bend form is the next in the list, and not built.
        assert_eq!(
            no.tried.last(),
            Some(&LeaderTried {
                edge: Side::Above,
                form: PathForm::OneBend,
                stop: LeaderStop::NotBuilt,
            })
        );
    }

    #[test]
    fn a_note_led_to_an_empty_slot_adds_no_mark_for_the_slot() {
        let mut r = record();
        r.enter(
            line("zero", Kind::ValueLine, (0.0, 200.0), (300.0, 200.0))
                .with_role(Role::zero_line()),
        );
        r.enter(
            line("g50", Kind::ValueLine, (0.0, 150.0), (300.0, 150.0)).with_role(Role::gridline()),
        );
        let slot = Target::shape(id("slot"), Ink::Rect(Rect::new(100.0, 190.0, 20.0, 10.0)));
        let note = words("note", Rect::default()).owned_by(id("slot"));
        let before = r.occupants().len();
        let placed = place_with_leader(
            &mut r,
            &note,
            &[Rect::new(95.0, 120.0, 30.0, 11.0)],
            &Leading::new(slot, 1.0),
            Level::Preferred,
        )
        .unwrap();
        assert_eq!(placed.leader.edge, Side::Below);
        assert_eq!(
            (placed.leader.from, placed.leader.to),
            (Point::new(110.0, 131.0), Point::new(110.0, 190.0))
        );
        assert_eq!(placed.preference, Preference::InOrder);
        assert_eq!(r.occupants().len(), before + 2);
        assert!(r.get(&id("slot")).is_none());
        assert!(r.cuts().is_empty());
        // A target with no ink and no shape gives no leader.
        let no = judge_leader(
            &r,
            &words("x", Rect::new(10.0, 10.0, 10.0, 10.0)),
            &to("nothing"),
            &[],
            Level::Preferred,
        )
        .unwrap_err();
        assert!(no.tried.iter().any(|t| t.stop == LeaderStop::NoTarget));
    }

    #[test]
    fn a_stale_placement_is_not_entered() {
        let mut r = record_with_bars();
        let name = words("n", Rect::default());
        let placed = judge_with_leader(
            &r,
            &name,
            &[Rect::new(128.0, 134.5, 30.0, 11.0)],
            &to("b/target"),
            Level::Preferred,
        )
        .unwrap();
        r.enter(words("later", Rect::new(300.0, 10.0, 10.0, 10.0)));
        assert!(enter_with_leader(&mut r, &placed).is_err());
        assert!(r.get(&id("n")).is_none());
    }
}
