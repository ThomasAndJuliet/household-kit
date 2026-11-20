// SPDX-License-Identifier: GPL-3.0-only

//! Halos: where a line is cut round a text standing in it (design §7).
//!
//! A text standing in a line is read as that line's label, so the covering
//! table lets only some texts stand in some lines, and those with a halo:
//! words in a value line ("move first, while it still clearly belongs; else
//! a halo"), a number in its own value line, a series' name in its own data
//! line. A halo is clear space round the text: the line is not drawn
//! there. This module says where the line is cut and what the record finds
//! about the cut; the record ([`crate::record`]) asks it whenever a
//! candidate meets a line by a verdict that needs a halo
//! ([`crate::rules::Verdict::needs_halo`]).
//!
//! # What a caller enters for a line
//!
//! A line is everything entered in the record under one identity with a
//! segment's ink ([`crate::record::Ink::Segment`]). A straight line is one
//! segment. A line with bends, such as a series' line through its points,
//! is entered as one segment per straight piece, each with the line's
//! identity, kind, owners and role, consecutive pieces sharing their end
//! points exactly (to [`FIT_EPSILON`]). From that the record knows:
//!
//! - **the line's ends**: the end points of its pieces that no other piece
//!   shares;
//! - **its bends**: the shared end points where the two pieces meet at an
//!   angle (pieces that continue in a straight line meet at a join, which
//!   is not a bend);
//! - **its crossings**: wherever another line (an occupant of another
//!   identity drawn as a segment, entered or pending) crosses one of its
//!   pieces;
//! - **its length**: the sum of its pieces' lengths.
//!
//! Everything else about a line (its kind, owner, role) is as for any
//! occupant. A line entered as a rectangle has no line to cut, and a halo
//! in it is judged by the cut table's row for that ([`CutAt::NotALine`]).
//!
//! # The geometry of a cut
//!
//! The halo's width `h` is the distance table's
//! [`crate::rules::DiagramSpacing::halo`]: the clear space left between the
//! text and the line's ink on every side. The line is drawn with square
//! ends where it is cut, so its ink stops where its centre line stops. The
//! cut is the stretch of a piece's centre line along which the line's ink,
//! a stroke `w` wide, would come within `h` of the text's box: the piece's
//! centre line clipped to the text's box grown by `h` on every side and
//! then widened by the stroke's half width across the line. For a line
//! across a text the cut runs from `h` before the text's box to `h` after
//! it; for a slanting line the same holds at the box's corners. A text
//! that meets two pieces of one line makes one cut in each.
//!
//! # Judging a cut
//!
//! The cut table ([`crate::rules::Cuts`]) gives a verdict for each place a
//! cut may fall, and every row that applies gives the record a finding
//! ([`crate::record::Finding::Cut`]):
//!
//! - at a crossing, a bend or the line's end: never. A cut reaches an end
//!   or a bend when it runs up to the end point of its piece;
//! - near an end (within [`crate::rules::DiagramSpacing::halo_near_end`] of
//!   the line's end, measured from either end of the cut) and over a third
//!   of the line ([`crate::rules::DiagramSpacing::halo_cut_share`] of its
//!   length): less preferred, which never rejects and puts the place after
//!   every place without it;
//! - a text standing in a cut that another text's halo has made: never.
//!
//! A cut, once entered ([`crate::record::Record::enter_cuts`], or with its
//! label by [`crate::labels::enter_set`]), stays part of the line: the line
//! is still one occupant, and later candidates are judged against the ink
//! it still shows, with the cut's stretch empty.
//!
//! # A worked example
//!
//! Her 11 px text gives a halo 1.65 px wide, "near an end" at 11 px and a
//! third of the line at one third. A gridline runs from (20, 50) to
//! (320, 50), 1 px wide: 300 px long, so a cut of more than 100 px is over
//! a third. A note 30 by 11 px stands across it.
//!
//! - At (100, 44.5) the line is cut from (98.35, 50) to (131.65, 50),
//!   33.3 px. No row applies: the place is one that needs a halo
//!   ([`crate::rules::Preference::ElseHalo`]).
//! - At (25, 44.5) the cut runs from 23.35 to 56.65, 3.35 px from the
//!   line's start: near an end, less preferred.
//! - At (15, 44.5) the cut would start at 13.35, before the line does: it
//!   reaches the line's end, never.
//! - With a second line upright at x = 115, the cut at (100, 44.5) holds
//!   the crossing: never.
//! - A note 100 px wide at (100, 44.5) cuts 103.3 px: over a third, less
//!   preferred.
//!
//! (Checked by the test `worked_example_a_note_across_a_gridline`.)

use crate::geometry::{FIT_EPSILON, Point, Rect};
use crate::identity::Id;
use crate::record::{Ink, Occupant};
use crate::rules::CutAt;

/// The stretch of a line a halo removes: the line is drawn up to `from`
/// and on from `to`, and not between. Both lie on one straight piece of
/// the line, `from` nearer the piece's start.
#[derive(Clone, Debug, PartialEq)]
pub struct Cut {
    /// The line's identity.
    pub line: Id,
    /// The text whose halo it is.
    pub label: Id,
    pub from: Point,
    pub to: Point,
    /// The line's width there, for drawing and for judging what stands in
    /// the cut.
    pub width: f64,
}

impl Cut {
    pub fn length(&self) -> f64 {
        distance(self.from, self.to)
    }

    /// The stretch removed, as ink: what a later text may not stand in.
    pub fn ink(&self) -> Ink {
        Ink::segment(self.from, self.to, self.width)
    }
}

fn distance(a: Point, b: Point) -> f64 {
    (b.x - a.x).hypot(b.y - a.y)
}

fn dot(a: Point, b: Point) -> f64 {
    a.x * b.x + a.y * b.y
}

/// One straight piece of a line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Piece {
    pub from: Point,
    pub to: Point,
    pub width: f64,
}

impl Piece {
    pub fn of(ink: &Ink) -> Option<Self> {
        match *ink {
            Ink::Segment { from, to, width } => Some(Self { from, to, width }),
            Ink::Rect(_) => None,
        }
    }

    fn length(&self) -> f64 {
        distance(self.from, self.to)
    }

    /// The unit vector along the piece.
    fn along(&self) -> Option<Point> {
        let length = self.length();
        (length > 0.0).then(|| {
            Point::new(
                (self.to.x - self.from.x) / length,
                (self.to.y - self.from.y) / length,
            )
        })
    }

    fn at(&self, t: f64) -> Point {
        let u = self.along().unwrap_or_default();
        Point::new(self.from.x + t * u.x, self.from.y + t * u.y)
    }

    /// How far along the piece the foot of `point` lies.
    fn position(&self, point: Point) -> f64 {
        let u = self.along().unwrap_or_default();
        dot(Point::new(point.x - self.from.x, point.y - self.from.y), u)
    }

    /// Whether `point` lies on the piece's centre line, between its ends.
    fn holds(&self, point: Point) -> bool {
        let t = self.position(point);
        t >= -FIT_EPSILON
            && t <= self.length() + FIT_EPSILON
            && distance(self.at(t.clamp(0.0, self.length())), point) <= FIT_EPSILON
    }

    /// The span `(start, end)` along the piece that a halo `halo` wide
    /// round `text` removes, or none where the line's ink does not come
    /// within the halo of the text. The span is the piece's centre line
    /// clipped to the text's box grown by `halo` and widened across the
    /// line by the stroke's half width: six half-planes, the box's four
    /// sides moved out by the stroke's reach in their direction and the two
    /// ends of the box's extent along the line.
    pub fn span(&self, text: Rect, halo: f64) -> Option<(f64, f64)> {
        let u = self.along()?;
        let n = Point::new(-u.y, u.x);
        let grown = text.grown(halo);
        let corners = [
            Point::new(grown.x, grown.y),
            Point::new(grown.right(), grown.y),
            Point::new(grown.right(), grown.bottom()),
            Point::new(grown.x, grown.bottom()),
        ];
        let (mut low, mut high) = (0.0_f64, self.length());
        for m in [
            Point::new(1.0, 0.0),
            Point::new(-1.0, 0.0),
            Point::new(0.0, 1.0),
            Point::new(0.0, -1.0),
            u,
            Point::new(-u.x, -u.y),
        ] {
            let reach = corners.iter().map(|&c| dot(m, c)).fold(f64::MIN, f64::max)
                + dot(m, n).abs() * self.width / 2.0;
            let start = dot(m, self.from);
            let rate = dot(m, u);
            if rate.abs() < 1e-12 {
                if start > reach + FIT_EPSILON {
                    return None;
                }
            } else if rate > 0.0 {
                high = high.min((reach - start) / rate);
            } else {
                low = low.max((reach - start) / rate);
            }
        }
        (high > low).then_some((low, high))
    }

    /// Where `other` crosses this piece's centre line, as a distance along
    /// this piece; none where they are parallel or do not cross.
    fn crossing(&self, other: &Piece) -> Option<f64> {
        let (u, v) = (self.along()?, other.along()?);
        let denominator = u.x * v.y - u.y * v.x;
        if denominator.abs() < 1e-12 {
            return None;
        }
        let w = Point::new(other.from.x - self.from.x, other.from.y - self.from.y);
        let t = (w.x * v.y - w.y * v.x) / denominator;
        let s = (w.x * u.y - w.y * u.x) / denominator;
        let reach = other.width / 2.0 + FIT_EPSILON;
        (s >= -reach && s <= other.length() + reach).then_some(t)
    }

    /// The pieces of this one left showing once `cuts` (spans along it)
    /// are taken out.
    pub fn shown(&self, cuts: &[(f64, f64)]) -> Vec<Ink> {
        let mut spans: Vec<(f64, f64)> = cuts.to_vec();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut shown = Vec::new();
        let mut start = 0.0;
        for (a, b) in spans {
            if a > start + FIT_EPSILON {
                shown.push(Ink::segment(self.at(start), self.at(a), self.width));
            }
            start = start.max(b);
        }
        if self.length() > start + FIT_EPSILON {
            shown.push(Ink::segment(self.at(start), self.to, self.width));
        }
        shown
    }

    /// The span along this piece of a cut lying on it, if it does.
    pub fn span_of(&self, cut: &Cut) -> Option<(f64, f64)> {
        (self.holds(cut.from) && self.holds(cut.to))
            .then(|| (self.position(cut.from), self.position(cut.to)))
    }
}

/// What a piece's end point is, in the line it belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EndPoint {
    /// No other piece of the line shares it.
    End,
    /// Another piece shares it at an angle.
    Bend,
    /// Another piece shares it and continues straight on.
    Join,
}

/// Every straight piece of one line: the occupants of one identity drawn
/// as segments.
pub(crate) struct Line {
    pieces: Vec<Piece>,
}

impl Line {
    pub fn of<'a>(id: &Id, everything: impl Iterator<Item = &'a Occupant>) -> Self {
        Self {
            pieces: everything
                .filter(|o| &o.id == id)
                .filter_map(|o| Piece::of(&o.ink))
                .collect(),
        }
    }

    fn length(&self) -> f64 {
        self.pieces.iter().map(Piece::length).sum()
    }

    /// What `point`, an end point of `piece`, is in the line.
    fn end_point(&self, piece: &Piece, point: Point) -> EndPoint {
        let near = |p: Point| distance(p, point) <= FIT_EPSILON;
        // The direction a piece leaves `point` in.
        let away = |p: &Piece| {
            let u = p.along()?;
            Some(if near(p.from) {
                u
            } else {
                Point::new(-u.x, -u.y)
            })
        };
        let mut kind = EndPoint::End;
        for other in &self.pieces {
            if other == piece || !(near(other.from) || near(other.to)) {
                continue;
            }
            let (Some(u), Some(v)) = (away(piece), away(other)) else {
                continue;
            };
            // Two pieces leaving a point in opposite directions run on in
            // a straight line: a join. Any other angle is a bend.
            if dot(u, v) > -1.0 + 1e-9 {
                return EndPoint::Bend;
            }
            kind = EndPoint::Join;
        }
        kind
    }

    /// The line's ends: its pieces' end points that no other piece shares.
    fn ends(&self) -> Vec<Point> {
        self.pieces
            .iter()
            .flat_map(|p| [(p, p.from), (p, p.to)])
            .filter(|(p, point)| self.end_point(p, *point) == EndPoint::End)
            .map(|(_, point)| point)
            .collect()
    }
}

/// Where a cut on `piece` of `line`, spanning `span`, falls, by the places
/// the cut table names: each with the other thing involved (the crossing
/// line). `others` are the other lines' pieces, with their identities.
pub(crate) fn places(
    line: &Line,
    piece: &Piece,
    span: (f64, f64),
    others: &[(Id, Piece)],
    near_end: f64,
    share: f64,
) -> Vec<(CutAt, Option<Id>)> {
    let mut found = Vec::new();
    let (start, end) = span;
    let length = piece.length();
    let mut reached = Vec::new();
    if start <= FIT_EPSILON {
        reached.push(line.end_point(piece, piece.from));
    }
    if end >= length - FIT_EPSILON {
        reached.push(line.end_point(piece, piece.to));
    }
    for at in [EndPoint::End, EndPoint::Bend] {
        if reached.contains(&at) {
            found.push((
                if at == EndPoint::End {
                    CutAt::End
                } else {
                    CutAt::Bend
                },
                None,
            ));
        }
    }
    for (id, other) in others {
        if let Some(t) = piece.crossing(other)
            && t >= start - other.width / 2.0 - FIT_EPSILON
            && t <= end + other.width / 2.0 + FIT_EPSILON
            && t >= -FIT_EPSILON
            && t <= length + FIT_EPSILON
        {
            found.push((CutAt::Crossing, Some(id.clone())));
        }
    }
    let (from, to) = (piece.at(start), piece.at(end));
    if !reached.contains(&EndPoint::End)
        && line
            .ends()
            .iter()
            .any(|&e| distance(from, e).min(distance(to, e)) < near_end - FIT_EPSILON)
    {
        found.push((CutAt::NearEnd, None));
    }
    if end - start > share * line.length() + FIT_EPSILON {
        found.push((CutAt::OverShare, None));
    }
    found
}

/// The cut a halo `halo` wide round `text` makes in `piece` of line `line`.
pub(crate) fn cut(line: &Id, label: &Id, piece: &Piece, text: Rect, halo: f64) -> Option<Cut> {
    let (start, end) = piece.span(text, halo)?;
    Some(Cut {
        line: line.clone(),
        label: label.clone(),
        from: piece.at(start),
        to: piece.at(end),
        width: piece.width,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(from: (f64, f64), to: (f64, f64), width: f64) -> Piece {
        Piece {
            from: Point::new(from.0, from.1),
            to: Point::new(to.0, to.1),
            width,
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn a_line_across_a_text_is_cut_the_halo_beyond_its_box() {
        let line = piece((0.0, 50.0), (300.0, 50.0), 1.0);
        let (a, b) = line.span(Rect::new(100.0, 44.5, 30.0, 11.0), 1.65).unwrap();
        assert!(close(a, 98.35) && close(b, 131.65));
        // Upright, through the same box.
        let up = piece((115.0, 0.0), (115.0, 100.0), 1.0);
        let (a, b) = up.span(Rect::new(100.0, 44.5, 30.0, 11.0), 1.65).unwrap();
        assert!(close(a, 42.85) && close(b, 57.15));
    }

    #[test]
    fn a_line_grazing_a_text_is_cut_where_its_ink_comes_within_the_halo() {
        // The centre line runs 2 px below the box; with the stroke 2 px
        // wide its ink is 1 px from the box, inside a 1.65 px halo.
        let line = piece((0.0, 57.5), (300.0, 57.5), 2.0);
        let text = Rect::new(100.0, 44.5, 30.0, 11.0);
        assert!(line.span(text, 1.65).is_some());
        // 3 px below the box's foot, its ink is 2 px off: no cut.
        let line = piece((0.0, 58.5), (300.0, 58.5), 2.0);
        assert_eq!(line.span(text, 1.65), None);
    }

    #[test]
    fn a_slanting_line_is_cut_where_its_stroke_clears_the_grown_box() {
        // A 45° line through the box's centre (115, 50).
        let line = piece((65.0, 0.0), (165.0, 100.0), 0.0);
        let text = Rect::new(100.0, 44.5, 30.0, 11.0);
        let (a, b) = line.span(text, 1.0).unwrap();
        // It enters the grown box (top 43.5) at y = 43.5 and leaves at
        // y = 56.5 (bottom): x from 108.5 to 121.5.
        let s = 2.0_f64.sqrt();
        assert!(close(a, 43.5 * s) && close(b, 56.5 * s));
    }

    #[test]
    fn the_pieces_of_a_line_give_its_ends_bends_and_joins() {
        let line = Line {
            pieces: vec![
                piece((0.0, 0.0), (10.0, 0.0), 1.0),
                piece((10.0, 0.0), (20.0, 0.0), 1.0),
                piece((20.0, 0.0), (30.0, 10.0), 1.0),
            ],
        };
        let [a, b, c] = [line.pieces[0], line.pieces[1], line.pieces[2]];
        assert_eq!(line.end_point(&a, a.from), EndPoint::End);
        assert_eq!(line.end_point(&a, a.to), EndPoint::Join);
        assert_eq!(line.end_point(&b, b.to), EndPoint::Bend);
        assert_eq!(line.end_point(&c, c.from), EndPoint::Bend);
        assert_eq!(line.ends(), [Point::new(0.0, 0.0), Point::new(30.0, 10.0)]);
        assert!(close(line.length(), 20.0 + 200.0_f64.sqrt()));
    }

    // ----- the worked example, through the record -----

    use crate::kinds::{Kind, Role};
    use crate::level::Level;
    use crate::record::{Against, Finding, Record};
    use crate::rules::{Preference, Rules, Sizes, Verdict};

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

    fn gridline(name: &str, from: (f64, f64), to: (f64, f64)) -> Occupant {
        Occupant::new(
            Id::new(name),
            Kind::ValueLine,
            Ink::segment(Point::new(from.0, from.1), Point::new(to.0, to.1), 1.0),
        )
        .with_role(Role::gridline())
    }

    fn note(x: f64, width: f64) -> Occupant {
        Occupant::new(
            Id::new("note"),
            Kind::Words,
            Ink::Rect(Rect::new(x, 44.5, width, 11.0)),
        )
    }

    /// The places of the cut table that the judgement found, in order.
    fn cut_places(judgement: &crate::record::Judgement) -> Vec<(CutAt, Verdict)> {
        judgement
            .findings
            .iter()
            .filter_map(|f| match f {
                Finding::Cut { at, verdict, .. } => Some((*at, *verdict)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn worked_example_a_note_across_a_gridline() {
        let mut r = record();
        r.enter(gridline("grid", (20.0, 50.0), (320.0, 50.0)));
        // Mid-line: cut from 98.35 to 131.65, no row applies.
        let j = r.judge(&note(100.0, 30.0));
        assert_eq!(cut_places(&j), []);
        assert_eq!(j.preference(), Preference::ElseHalo);
        let [cut] = &j.cuts[..] else { panic!("{j:?}") };
        assert_eq!((cut.line.as_str(), cut.label.as_str()), ("grid", "note"));
        assert!(close(cut.from.x, 98.35) && close(cut.to.x, 131.65));
        assert!(close(cut.from.y, 50.0) && close(cut.length(), 33.3));
        // 3.35 px from the start: near an end, less preferred.
        let j = r.judge(&note(25.0, 30.0));
        assert!(close(j.cuts[0].from.x, 23.35));
        assert_eq!(cut_places(&j), [(CutAt::NearEnd, Verdict::LessPreferred)]);
        assert_eq!(j.preference(), Preference::LessPreferred);
        assert_eq!(j.rejection_at(Level::Preferred), None);
        // Reaching the start: never.
        let j = r.judge(&note(15.0, 30.0));
        assert_eq!(cut_places(&j), [(CutAt::End, Verdict::Never)]);
        assert!(j.rejection_at(Level::AllPermitted).is_some());
        // 100 px wide: a cut of 103.3 px on a line of 300, over a third.
        let j = r.judge(&note(100.0, 100.0));
        assert!(close(j.cuts[0].length(), 103.3));
        assert_eq!(cut_places(&j), [(CutAt::OverShare, Verdict::LessPreferred)]);
        // An upright line at x = 115 crosses the line inside the cut.
        r.enter(gridline("upright", (115.0, 0.0), (115.0, 100.0)));
        let j = r.judge(&note(100.0, 30.0));
        let crossings: Vec<_> = j
            .findings
            .iter()
            .filter_map(|f| match f {
                Finding::Cut {
                    against: Against::Occupant(line),
                    at: CutAt::Crossing,
                    by: Some(by),
                    verdict: Verdict::Never,
                    ..
                } => Some((line.as_str(), by.as_str())),
                _ => None,
            })
            .collect();
        // The note stands in both lines, and each cut holds the crossing.
        assert_eq!(crossings, [("grid", "upright"), ("upright", "grid")]);
    }

    #[test]
    fn a_cut_at_a_bend_of_a_series_line_is_refused() {
        // A series' line through (20, 100), (120, 50) and (220, 100),
        // entered as two pieces under one identity; its name, owned by it,
        // stands in it with a halo.
        let mut r = record();
        for (a, b) in [
            ((20.0, 100.0), (120.0, 50.0)),
            ((120.0, 50.0), (220.0, 100.0)),
        ] {
            r.enter(Occupant::new(
                Id::new("series"),
                Kind::DataLine,
                Ink::segment(Point::new(a.0, a.1), Point::new(b.0, b.1), 2.0),
            ));
        }
        let name = |x: f64, y: f64| {
            Occupant::new(
                Id::new("series/name"),
                Kind::Words,
                Ink::Rect(Rect::new(x, y, 30.0, 11.0)),
            )
            .owned_by(Id::new("series"))
        };
        // Over the bend: it meets both pieces, and both cuts reach the bend.
        let j = r.judge(&name(105.0, 44.5));
        assert_eq!(j.cuts.len(), 2);
        assert_eq!(
            cut_places(&j),
            [(CutAt::Bend, Verdict::Never), (CutAt::Bend, Verdict::Never)]
        );
        // On the first piece, half way along: allowed with a halo, in order.
        let j = r.judge(&name(55.0, 69.5));
        assert_eq!(cut_places(&j), []);
        assert_eq!(j.cuts.len(), 1);
        assert_eq!(j.preference(), Preference::InOrder);
        assert_eq!(j.rejection_at(Level::Preferred), None);
        assert_eq!(j.verdicts().collect::<Vec<_>>(), [Verdict::AllowedWithHalo]);
    }

    #[test]
    fn a_line_that_is_cut_is_judged_as_it_then_shows() {
        // With a halo of 2 em (22 px, changed as data) the note at 100..130
        // cuts the line from 78 to 152.
        let mut r = record();
        r.rules_mut().distances.diagram.halo = crate::rules::Measure::em(2.0);
        r.enter(gridline("grid", (20.0, 50.0), (320.0, 50.0)));
        let first = note(100.0, 30.0);
        let j = r.judge(&first);
        assert!(close(j.cuts[0].from.x, 78.0) && close(j.cuts[0].to.x, 152.0));
        // A number 6 px wide 3 px above the line, in the cut's stretch.
        let number = |y: f64| {
            Occupant::new(
                Id::new("n"),
                Kind::Number,
                Ink::Rect(Rect::new(86.0, y, 6.0, 11.0)),
            )
        };
        let above = number(50.0 - 0.5 - 3.0 - 11.0);
        // Before the note is entered, it is squeezed against the line.
        assert_eq!(r.judge(&above).squeezes().count(), 1);
        r.enter(first);
        r.enter_cuts(j.cuts);
        assert_eq!(r.cuts_of(&Id::new("grid")).count(), 1);
        // The line still is one occupant; where it is cut there is no ink,
        // so the number keeps its distance from the line as it shows.
        assert_eq!(
            r.occupants()
                .iter()
                .filter(|o| o.id.as_str() == "grid")
                .count(),
            1
        );
        assert_eq!(r.judge(&above).findings, []);
        // Standing in the cut itself, it is refused: a second text may not
        // stand in the first one's cut.
        let j = r.judge(&number(44.5));
        assert!(j.findings.iter().any(|f| matches!(
            f,
            Finding::Cut {
                at: CutAt::InAnotherCut,
                by: Some(by),
                verdict: Verdict::Never,
                ..
            } if by.as_str() == "note"
        )));
        assert!(j.rejection_at(Level::AllPermitted).is_some());
    }

    #[test]
    fn the_cut_rows_and_numbers_are_data() {
        let mut r = record();
        r.enter(gridline("grid", (20.0, 50.0), (320.0, 50.0)));
        let at_end = note(15.0, 30.0);
        assert!(r.judge(&at_end).rejection_at(Level::Preferred).is_some());
        let row = r
            .rules()
            .cuts
            .row_named("a halo's cut at the line's end")
            .unwrap();
        r.rules_mut().cuts.rows[row].verdict = Verdict::LessPreferred;
        let j = r.judge(&at_end);
        assert_eq!(j.rejection_at(Level::Preferred), None);
        assert_eq!(j.preference(), Preference::LessPreferred);
        // "Near an end" at 2 px instead of 11: the cut 3.35 px off is not.
        let near = note(25.0, 30.0);
        assert_eq!(r.judge(&near).preference(), Preference::LessPreferred);
        r.rules_mut().distances.diagram.halo_near_end =
            crate::rules::Measure::new(2.0, crate::rules::Named::LineWidth);
        assert_eq!(r.judge(&near).preference(), Preference::ElseHalo);
        // A third becomes a tenth: the 33.3 px cut is over a tenth of 300.
        r.rules_mut().distances.diagram.halo_cut_share = 0.1;
        assert_eq!(
            cut_places(&r.judge(&near)),
            [(CutAt::OverShare, Verdict::LessPreferred)]
        );
    }

    #[test]
    fn what_a_cut_leaves_showing() {
        let line = piece((0.0, 50.0), (100.0, 50.0), 1.0);
        let shown = line.shown(&[(60.0, 70.0), (20.0, 30.0)]);
        assert_eq!(
            shown,
            [
                Ink::segment(Point::new(0.0, 50.0), Point::new(20.0, 50.0), 1.0),
                Ink::segment(Point::new(30.0, 50.0), Point::new(60.0, 50.0), 1.0),
                Ink::segment(Point::new(70.0, 50.0), Point::new(100.0, 50.0), 1.0),
            ]
        );
        assert_eq!(line.shown(&[(0.0, 100.0)]), []);
    }
}
