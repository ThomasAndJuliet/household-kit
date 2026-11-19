// SPDX-License-Identifier: GPL-3.0-only

//! The distance table and the covering table, as data (design §7 and §8).
//!
//! Both tables are plain values with public fields. The agreed content is
//! their [`Default`], and this file's `Default` implementations are the only
//! place the agreed numbers appear: "this is a matter of numbers not
//! architecture so should be easy to tune". A caller changes a number or a
//! row and the engine judges by it; nothing else changes.

use crate::kinds::{Kind, Kinds, Role};
use crate::level::Level;

/// The tables, the order of drawing, and the level from which squeezing is
/// permitted.
#[derive(Clone, Debug, PartialEq)]
pub struct Rules {
    pub distances: Distances,
    pub covering: Covering,
    /// Which kinds are drawn beneath which (semantics C5). A candidate
    /// standing on a thing drawn above a line does not meet that line where
    /// the thing covers it. By default only gridlines and the zero line
    /// (value lines with those roles) are beneath data marks; a comparison
    /// or event line is drawn over a bar and is judged.
    pub drawing_order: Vec<Beneath>,
    /// The level from which a distance below its ideal is accepted
    /// (semantics A7). Compositions take the same value for their gaps.
    pub squeeze_from: Level,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            distances: Distances::default(),
            covering: Covering::default(),
            drawing_order: vec![
                Beneath::new(
                    Match::kinds(Kind::ValueLine).with_role(Role::gridline()),
                    Match::kinds(Kind::DataMark),
                ),
                Beneath::new(
                    Match::kinds(Kind::ValueLine).with_role(Role::zero_line()),
                    Match::kinds(Kind::DataMark),
                ),
            ],
            squeeze_from: Level::Strong,
        }
    }
}

impl Rules {
    /// Whether `lower` is drawn beneath `upper` by the order of drawing.
    pub fn drawn_beneath(
        &self,
        lower: (Kind, Option<&Role>),
        upper: (Kind, Option<&Role>),
    ) -> bool {
        self.drawing_order
            .iter()
            .any(|row| row.beneath.matches(lower.0, lower.1) && row.above.matches(upper.0, upper.1))
    }
}

/// One entry of the order of drawing: things matching `beneath` are drawn
/// beneath things matching `above` ("Gridline | data marks | beneath them").
#[derive(Clone, Debug, PartialEq)]
pub struct Beneath {
    pub beneath: Match,
    pub above: Match,
}

impl Beneath {
    pub fn new(beneath: Match, above: Match) -> Self {
        Self { beneath, above }
    }
}

// ---------------------------------------------------------------------------
// Distances
// ---------------------------------------------------------------------------

/// A size that distances are multiples of (design §7: "Every value is a
/// multiple of a named size").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Named {
    /// The label size: one em of the labels' font size.
    LabelSize,
    /// One word space of the labels' font at the label size.
    WordSpace,
    /// The width of a line.
    LineWidth,
}

/// The named sizes of one diagram, in its units (px or pt).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sizes {
    pub label_size: f64,
    pub word_space: f64,
    pub line_width: f64,
}

impl Sizes {
    pub fn get(&self, named: Named) -> f64 {
        match named {
            Named::LabelSize => self.label_size,
            Named::WordSpace => self.word_space,
            Named::LineWidth => self.line_width,
        }
    }
}

/// A distance as a multiple of a named size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measure {
    pub times: f64,
    pub of: Named,
}

impl Measure {
    pub const fn new(times: f64, of: Named) -> Self {
        Self { times, of }
    }

    /// `times` em of the label size.
    pub const fn em(times: f64) -> Self {
        Self::new(times, Named::LabelSize)
    }

    pub fn resolve(&self, sizes: &Sizes) -> f64 {
        self.times * sizes.get(self.of)
    }
}

/// How two distinct objects stand to each other, which sets the distance
/// they keep.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Relation {
    /// A label and its own element; things that belong together.
    Related,
    /// Everything else.
    Unrelated,
    /// Meant to meet: an arrow and its target, bars touching by design, a
    /// text and the line it stands in. No distance.
    MeantToMeet,
}

/// The distance table: two families, text spacing and diagram spacing.
#[derive(Clone, Debug, PartialEq)]
pub struct Distances {
    pub text: TextSpacing,
    pub diagram: DiagramSpacing,
}

impl Default for Distances {
    fn default() -> Self {
        Self {
            text: TextSpacing {
                note_mark: TextMeasure::new(1.0, TextSize::LetterSpace),
                attachment: TextMeasure::new(1.0, TextSize::WordSpace),
                lines: TextMeasure::new(1.0, TextSize::LineSpacing),
            },
            diagram: DiagramSpacing {
                least: Measure::em(0.2),
                related: Measure::em(0.336),
                unrelated: Measure::em(0.66),
                near: Measure::em((0.336 + 0.66) / 2.0),
                least_for_pairs: Vec::new(),
                halo: Measure::em(0.15),
            },
        }
    }
}

/// A size of one text's own font at its size. The caller supplies the
/// numbers for each text ([`TextMetrics`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextSize {
    LetterSpace,
    WordSpace,
    LineSpacing,
}

/// One text's own spacing, from its font at its size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMetrics {
    pub letter_space: f64,
    pub word_space: f64,
    pub line_spacing: f64,
}

impl TextMetrics {
    pub fn get(&self, size: TextSize) -> f64 {
        match size {
            TextSize::LetterSpace => self.letter_space,
            TextSize::WordSpace => self.word_space,
            TextSize::LineSpacing => self.line_spacing,
        }
    }
}

/// A distance as a multiple of one of a text's own sizes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMeasure {
    pub times: f64,
    pub of: TextSize,
}

impl TextMeasure {
    pub const fn new(times: f64, of: TextSize) -> Self {
        Self { times, of }
    }

    pub fn resolve(&self, metrics: &TextMetrics) -> f64 {
        self.times * metrics.get(self.of)
    }
}

/// The places where text spacing applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextGap {
    /// A note mark and its text.
    NoteMark,
    /// A swatch, icon or bullet and its text; a unit phrase after its
    /// number.
    Attachment,
    /// The lines of one text.
    Lines,
}

/// Text spacing: from a text's own font and size.
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpacing {
    pub note_mark: TextMeasure,
    pub attachment: TextMeasure,
    pub lines: TextMeasure,
}

impl TextSpacing {
    pub fn measure(&self, gap: TextGap) -> TextMeasure {
        match gap {
            TextGap::NoteMark => self.note_mark,
            TextGap::Attachment => self.attachment,
            TextGap::Lines => self.lines,
        }
    }

    /// The distance for `gap` in a text with these metrics.
    pub fn distance(&self, gap: TextGap, metrics: &TextMetrics) -> f64 {
        self.measure(gap).resolve(metrics)
    }
}

/// Diagram spacing: between distinct objects.
#[derive(Clone, Debug, PartialEq)]
pub struct DiagramSpacing {
    /// Nothing comes closer than this, unless a pair has its own.
    pub least: Measure,
    /// The ideal between related things.
    pub related: Measure,
    /// The ideal between unrelated things.
    pub unrelated: Measure,
    /// The farthest a label may stand from its own mark and still read as
    /// its own (K2). By default halfway between the related and the
    /// unrelated ideal. Label sets that share lines use it (U8); the
    /// record's judging does not yet bound a related distance.
    pub near: Measure,
    /// Pairs of kinds with a least distance of their own; the first entry
    /// naming a pair, in either order, is used.
    pub least_for_pairs: Vec<PairLeast>,
    /// The halo's width. The halo's geometry is not in this slice; the
    /// number lives here with the others.
    pub halo: Measure,
}

/// A least distance of its own for a pair of kinds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PairLeast {
    pub a: Kind,
    pub b: Kind,
    pub least: Measure,
}

/// Which entry of the distance table a judgement used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DistanceRow {
    /// The least distance.
    Least,
    /// An entry of [`DiagramSpacing::least_for_pairs`], by index.
    PairLeast(usize),
    /// The related ideal.
    Related,
    /// The unrelated ideal.
    Unrelated,
    /// Text spacing, for a pair the caller joined by a text gap.
    Text(TextGap),
}

impl DiagramSpacing {
    /// The least distance between two kinds, and the row it comes from.
    pub fn least_between(&self, a: Kind, b: Kind, sizes: &Sizes) -> (f64, DistanceRow) {
        match self
            .least_for_pairs
            .iter()
            .position(|p| (p.a, p.b) == (a, b) || (p.a, p.b) == (b, a))
        {
            Some(index) => (
                self.least_for_pairs[index].least.resolve(sizes),
                DistanceRow::PairLeast(index),
            ),
            None => (self.least.resolve(sizes), DistanceRow::Least),
        }
    }

    /// The ideal for a relation and the row it comes from; none for things
    /// meant to meet.
    pub fn ideal(&self, relation: Relation, sizes: &Sizes) -> Option<(f64, DistanceRow)> {
        match relation {
            Relation::Related => Some((self.related.resolve(sizes), DistanceRow::Related)),
            Relation::Unrelated => Some((self.unrelated.resolve(sizes), DistanceRow::Unrelated)),
            Relation::MeantToMeet => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Covering
// ---------------------------------------------------------------------------

/// What happens when a thing being placed meets another (design §7, "What
/// may stand on what").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Verdict {
    /// Never here.
    Never,
    /// Allowed.
    Allowed,
    /// Allowed in the toggle's ink; if neither ink reads, not here. The
    /// caller checks contrast.
    AllowedInToggleInk,
    /// Allowed; the caller checks contrast against what it stands on.
    AllowedContrastChecked,
    /// Move first, while it still clearly belongs; else a halo. Moving is a
    /// place earlier in the caller's list; at this place, a halo.
    MoveFirstElseHalo,
    /// Allowed with a halo.
    AllowedWithHalo,
    /// Allowed; the thing met (a time divider) breaks round it.
    BreaksRound,
    /// Meant to meet.
    MeantToMeet,
    /// Allowed, after every place without this verdict.
    LessPreferred,
    /// Allowed, after every other place.
    LastInOrder,
}

impl Verdict {
    /// Whether this verdict rejects the place. Only `Never` does: "less
    /// preferred" and "last in order" are positions in a list of places.
    pub fn rejects(self) -> bool {
        self == Self::Never
    }

    /// The position this verdict gives a place in a list of places.
    pub fn preference(self) -> Preference {
        match self {
            Self::MoveFirstElseHalo => Preference::ElseHalo,
            Self::LessPreferred => Preference::LessPreferred,
            Self::LastInOrder => Preference::LastInOrder,
            _ => Preference::InOrder,
        }
    }
}

/// A place's position in a list of places: places in order first, then
/// those that need a halo where moving would have avoided one, then the
/// less preferred ones, then those last in order, each group in the
/// caller's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Preference {
    #[default]
    InOrder,
    /// "Move first … else a halo": after every clean place (semantics C3).
    ElseHalo,
    LessPreferred,
    LastInOrder,
}

/// Which side of a thing a row matches: its kinds, and a role if the row
/// names one.
#[derive(Clone, Debug, PartialEq)]
pub struct Match {
    pub kinds: Kinds,
    pub role: Option<Role>,
}

impl Match {
    pub fn kinds(kinds: impl Into<Kinds>) -> Self {
        Self {
            kinds: kinds.into(),
            role: None,
        }
    }

    pub fn with_role(mut self, role: Role) -> Self {
        self.role = Some(role);
        self
    }

    pub fn matches(&self, kind: Kind, role: Option<&Role>) -> bool {
        self.kinds.contains(kind) && self.role.as_ref().is_none_or(|r| Some(r) == role)
    }
}

/// What a row's placed thing meets.
#[derive(Clone, Debug, PartialEq)]
pub enum Meets {
    /// An occupant of the record.
    Occupant(Match),
    /// The figure's edge: the record's bounds.
    FigureEdge,
}

/// Whether a row applies to the placed thing's own occupant, another's, or
/// either. A thing is another's own only when one owns the other; two
/// things with the same owner are not each other's own (semantics C1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ownership {
    Any,
    Own,
    NotOwn,
}

impl Ownership {
    pub fn matches(self, own: bool) -> bool {
        match self {
            Self::Any => true,
            Self::Own => own,
            Self::NotOwn => !own,
        }
    }
}

/// One row of the covering table.
#[derive(Clone, Debug, PartialEq)]
pub struct CoveringRow {
    /// The row's words, for reports.
    pub name: String,
    pub placed: Match,
    pub meets: Meets,
    pub ownership: Ownership,
    pub verdict: Verdict,
    /// Whether the verdict holds also where the placed thing comes within
    /// the distance without meeting, in place of the ideal's findings
    /// ("along or close to a data line"). It applies between the least
    /// distance and the ideal; below the least distance the pair is
    /// rejected as any other (semantics C8).
    pub near: bool,
}

impl CoveringRow {
    pub fn new(
        name: impl Into<String>,
        placed: Match,
        meets: Meets,
        ownership: Ownership,
        verdict: Verdict,
    ) -> Self {
        Self {
            name: name.into(),
            placed,
            meets,
            ownership,
            verdict,
            near: false,
        }
    }

    /// This row with its verdict holding within the distance as well.
    pub fn near(mut self) -> Self {
        self.near = true;
        self
    }
}

/// Which row of the covering table gave a verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CoveringRef {
    /// A row, by index.
    Row(usize),
    /// No row matched; the table's verdict for an unlisted meeting.
    Unlisted,
    /// The caller declared the pair meant to meet.
    Declared,
}

/// The covering table: rows tried in order, the first that matches giving
/// the verdict.
#[derive(Clone, Debug, PartialEq)]
pub struct Covering {
    pub rows: Vec<CoveringRow>,
    /// The verdict when two things' ink meets and no row matches. Overlap
    /// rejects (design §8), so the default is `Never`.
    pub unlisted: Verdict,
}

impl Covering {
    /// The verdict for a placed thing meeting an occupant, and its row.
    pub fn verdict(
        &self,
        placed: (Kind, Option<&Role>),
        met: (Kind, Option<&Role>),
        own: bool,
    ) -> (Verdict, CoveringRef) {
        self.rows
            .iter()
            .position(|row| {
                row.placed.matches(placed.0, placed.1)
                    && matches!(&row.meets, Meets::Occupant(m) if m.matches(met.0, met.1))
                    && row.ownership.matches(own)
            })
            .map_or((self.unlisted, CoveringRef::Unlisted), |index| {
                (self.rows[index].verdict, CoveringRef::Row(index))
            })
    }

    /// The verdict for a placed thing crossing the figure's edge, if a row
    /// names it. Where none does, the edge is no concern of the table.
    pub fn edge_verdict(&self, placed: (Kind, Option<&Role>)) -> Option<(Verdict, CoveringRef)> {
        self.rows
            .iter()
            .position(|row| {
                row.placed.matches(placed.0, placed.1) && row.meets == Meets::FigureEdge
            })
            .map(|index| (self.rows[index].verdict, CoveringRef::Row(index)))
    }

    /// The row a placed thing would meet an occupant by, if one matches;
    /// for things that come near without meeting.
    pub fn row_for(
        &self,
        placed: (Kind, Option<&Role>),
        met: (Kind, Option<&Role>),
        own: bool,
    ) -> Option<usize> {
        match self.verdict(placed, met, own).1 {
            CoveringRef::Row(index) => Some(index),
            _ => None,
        }
    }

    /// The index of the row with this name, for a caller changing it.
    pub fn row_named(&self, name: &str) -> Option<usize> {
        self.rows.iter().position(|row| row.name == name)
    }
}

impl Default for Covering {
    /// Design §7, "What may stand on what", for the rows this slice can
    /// judge by meeting: text, keys and connectors placed against what is
    /// already recorded. More specific rows come first.
    fn default() -> Self {
        use Kind::*;
        use Ownership::{Any, NotOwn, Own};
        use Verdict::*;
        let on = |kinds: Kinds| Meets::Occupant(Match::kinds(kinds));
        let text = || Match::kinds(Kinds::TEXT);
        let row = CoveringRow::new;
        Self {
            rows: vec![
                row("text on other text", text(), on(Kinds::TEXT), Any, Never),
                row(
                    "text at the figure's edge",
                    text(),
                    Meets::FigureEdge,
                    Any,
                    Never,
                ),
                row(
                    "a comment on data marks",
                    text().with_role(Role::comment()),
                    on(DataMark.into()),
                    Any,
                    Never,
                ),
                row(
                    "text on its own fill",
                    text(),
                    on(DataMark.into()),
                    Own,
                    AllowedInToggleInk,
                ),
                row(
                    "text on a data mark not its own",
                    text(),
                    on(DataMark.into()),
                    NotOwn,
                    Never,
                ),
                row(
                    "text on a plate",
                    text(),
                    on(Plate.into()),
                    Any,
                    AllowedContrastChecked,
                ),
                row(
                    "a comparison's name in another comparison's line",
                    Match::kinds(Words).with_role(Role::comparison()),
                    Meets::Occupant(Match::kinds(ValueLine).with_role(Role::comparison())),
                    NotOwn,
                    Never,
                ),
                row(
                    "words in a value line",
                    Match::kinds(Words),
                    on(ValueLine.into()),
                    Any,
                    MoveFirstElseHalo,
                ),
                row(
                    "a number in its own value line",
                    Match::kinds(Number),
                    on(ValueLine.into()),
                    Own,
                    AllowedWithHalo,
                ),
                row(
                    "a number in a value line not its own",
                    Match::kinds(Number),
                    on(ValueLine.into()),
                    NotOwn,
                    Never,
                ),
                row(
                    "a series' name in its own data line",
                    Match::kinds(Words),
                    on(DataLine.into()),
                    Own,
                    AllowedWithHalo,
                ),
                row(
                    "any other text in a data line",
                    text(),
                    on(DataLine.into()),
                    Any,
                    Never,
                ),
                row(
                    "text at an outer divider",
                    text(),
                    Meets::Occupant(Match::kinds(Divider).with_role(Role::outer_divider())),
                    Any,
                    Never,
                ),
                row(
                    "text at a time divider",
                    text(),
                    Meets::Occupant(Match::kinds(Divider).with_role(Role::time_divider())),
                    Any,
                    BreaksRound,
                ),
                row(
                    "text or a key in a reading path",
                    Match::kinds(Kinds::TEXT.with(Key)),
                    on(ReadingPath.into()),
                    Any,
                    LastInOrder,
                ),
                row(
                    "a key on data marks",
                    Match::kinds(Key),
                    on(DataMark.into()),
                    Any,
                    Never,
                ),
                row(
                    "a connector and its own text",
                    Match::kinds(Connector),
                    on(Kinds::TEXT),
                    Own,
                    MeantToMeet,
                ),
                row(
                    "a connector through text",
                    Match::kinds(Connector),
                    on(Kinds::TEXT),
                    NotOwn,
                    Never,
                ),
                row(
                    "a connector and its own target",
                    Match::kinds(Connector),
                    on(DataMark.into()),
                    Own,
                    MeantToMeet,
                ),
                row(
                    "a connector through a data mark not its target",
                    Match::kinds(Connector),
                    on(DataMark.into()),
                    NotOwn,
                    Never,
                ),
                row(
                    "a connector across a gridline",
                    Match::kinds(Connector),
                    on(ValueLine.into()),
                    Any,
                    Allowed,
                ),
                row(
                    "a connector along or close to a data line",
                    Match::kinds(Connector),
                    on(DataLine.into()),
                    Any,
                    LessPreferred,
                )
                .near(),
                row(
                    "a connector across another connector",
                    Match::kinds(Connector),
                    on(Connector.into()),
                    Any,
                    Never,
                ),
            ],
            unlisted: Never,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZES: Sizes = Sizes {
        label_size: 11.0,
        word_space: 3.7,
        line_width: 1.0,
    };

    #[test]
    fn the_default_diagram_spacing_gives_her_numbers_at_11_px() {
        let d = Distances::default().diagram;
        let near = |a: f64, b: f64| (a - b).abs() < 0.05;
        assert!(near(d.least.resolve(&SIZES), 2.2));
        assert!(near(d.related.resolve(&SIZES), 3.7));
        assert!(near(d.unrelated.resolve(&SIZES), 7.3));
        // Halfway between 3.696 and 7.26.
        assert!((d.near.resolve(&SIZES) - 5.478).abs() < 1e-9);
        assert_eq!(d.ideal(Relation::MeantToMeet, &SIZES), None);
    }

    #[test]
    fn a_pair_of_kinds_can_have_its_own_least_distance_in_either_order() {
        let mut d = Distances::default().diagram;
        d.least_for_pairs.push(PairLeast {
            a: Kind::Number,
            b: Kind::Tick,
            least: Measure::new(1.0, Named::WordSpace),
        });
        assert_eq!(
            d.least_between(Kind::Tick, Kind::Number, &SIZES),
            (3.7, DistanceRow::PairLeast(0))
        );
        assert_eq!(
            d.least_between(Kind::Words, Kind::Tick, &SIZES).1,
            DistanceRow::Least
        );
    }

    #[test]
    fn text_spacing_comes_from_the_texts_own_metrics() {
        let t = Distances::default().text;
        let m = TextMetrics {
            letter_space: 0.5,
            word_space: 3.3,
            line_spacing: 13.0,
        };
        assert_eq!(t.distance(TextGap::NoteMark, &m), 0.5);
        assert_eq!(t.distance(TextGap::Attachment, &m), 3.3);
        assert_eq!(t.distance(TextGap::Lines, &m), 13.0);
    }

    #[test]
    fn only_never_rejects() {
        let rejecting: Vec<_> = [
            Verdict::Never,
            Verdict::Allowed,
            Verdict::AllowedInToggleInk,
            Verdict::AllowedContrastChecked,
            Verdict::MoveFirstElseHalo,
            Verdict::AllowedWithHalo,
            Verdict::BreaksRound,
            Verdict::MeantToMeet,
            Verdict::LessPreferred,
            Verdict::LastInOrder,
        ]
        .into_iter()
        .filter(|v| v.rejects())
        .collect();
        assert_eq!(rejecting, [Verdict::Never]);
        assert!(Verdict::LastInOrder.preference() > Verdict::LessPreferred.preference());
    }

    #[test]
    fn a_halo_one_could_move_from_comes_after_clean_places_and_before_less_preferred() {
        let order = [
            Verdict::Allowed,
            Verdict::AllowedWithHalo,
            Verdict::MoveFirstElseHalo,
            Verdict::LessPreferred,
            Verdict::LastInOrder,
        ]
        .map(Verdict::preference);
        assert_eq!(
            order,
            [
                Preference::InOrder,
                Preference::InOrder,
                Preference::ElseHalo,
                Preference::LessPreferred,
                Preference::LastInOrder,
            ]
        );
    }

    #[test]
    fn the_default_rules_draw_value_lines_beneath_data_marks_and_squeeze_from_strong() {
        let rules = Rules::default();
        assert_eq!(rules.squeeze_from, Level::Strong);
        let beneath = |role: Option<Role>| {
            rules.drawn_beneath((Kind::ValueLine, role.as_ref()), (Kind::DataMark, None))
        };
        assert!(beneath(Some(Role::gridline())));
        assert!(beneath(Some(Role::zero_line())));
        // A comparison or event line is drawn over a bar (C5).
        assert!(!beneath(Some(Role::comparison())));
        assert!(!beneath(Some(Role::new("event"))));
        assert!(!beneath(None));
        assert!(!rules.drawn_beneath((Kind::DataLine, None), (Kind::DataMark, None)));
        assert!(!rules.drawn_beneath((Kind::DataMark, None), (Kind::ValueLine, None)));
        let near: Vec<_> = rules
            .covering
            .rows
            .iter()
            .filter(|row| row.near)
            .map(|row| row.name.as_str())
            .collect();
        assert_eq!(near, ["a connector along or close to a data line"]);
    }

    #[test]
    fn the_first_matching_row_gives_the_verdict() {
        let c = Covering::default();
        let comparison = Role::comparison();
        let (verdict, at) = c.verdict(
            (Kind::Words, Some(&comparison)),
            (Kind::ValueLine, Some(&comparison)),
            false,
        );
        assert_eq!(verdict, Verdict::Never);
        assert_eq!(
            at,
            CoveringRef::Row(
                c.row_named("a comparison's name in another comparison's line")
                    .unwrap()
            )
        );
        let (verdict, _) = c.verdict(
            (Kind::Words, Some(&comparison)),
            (Kind::ValueLine, Some(&comparison)),
            true,
        );
        assert_eq!(verdict, Verdict::MoveFirstElseHalo);
        assert_eq!(
            c.verdict((Kind::Words, None), (Kind::Tick, None), false),
            (Verdict::Never, CoveringRef::Unlisted)
        );
    }
}
