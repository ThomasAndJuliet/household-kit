// SPDX-License-Identifier: GPL-3.0-only

//! The kinds of thing that occupy space (design §7, "Kinds").
//!
//! Everything drawn enters the record of occupied space with one of these
//! kinds. Words and numbers are separate kinds because a number standing in
//! a value line reads as that line's value, which words do not.
//!
//! The covering table sometimes needs to tell apart members of one kind (a
//! comparison's line from a gridline, the outer time dividers from the inner
//! ones). The caller says which with a [`Role`]; the crate attaches no
//! meaning to a role beyond matching it against the table's rows.

/// A kind of thing that occupies space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// Names, titles, notes, comments, headings.
    Words,
    /// Value labels, scale labels.
    Number,
    /// A bar, dot, slice, tile, cell, area, or an image used as a mark.
    DataMark,
    /// A series' line.
    DataLine,
    /// Gridlines in either direction, the zero line, comparison and event
    /// lines.
    ValueLine,
    /// A target or reference marker on a bar or dot.
    ComparisonMark,
    /// A comment arrow, leader or title link, with its head.
    Connector,
    /// A swatch.
    Key,
    /// Time ticks, a ramp key's ticks.
    Tick,
    /// Time dividers, table rules, frames.
    Divider,
    /// A fill set behind by design.
    Plate,
    /// Not ink: the region between an axis and the data read against it.
    ReadingPath,
}

impl Kind {
    /// Every kind, in the order of design §7.
    pub const ALL: [Self; 12] = [
        Self::Words,
        Self::Number,
        Self::DataMark,
        Self::DataLine,
        Self::ValueLine,
        Self::ComparisonMark,
        Self::Connector,
        Self::Key,
        Self::Tick,
        Self::Divider,
        Self::Plate,
        Self::ReadingPath,
    ];

    /// Whether this kind is text: words or a number.
    pub fn is_text(self) -> bool {
        matches!(self, Self::Words | Self::Number)
    }

    /// Whether this kind is ink. The reading path is a region that nothing
    /// draws, so distances do not apply to it.
    pub fn is_ink(self) -> bool {
        self != Self::ReadingPath
    }

    const fn bit(self) -> u16 {
        1 << self as u16
    }
}

/// A set of kinds, for the rows of the rule tables.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Kinds(u16);

impl Kinds {
    /// No kind.
    pub const NONE: Self = Self(0);
    /// Every kind.
    pub const ANY: Self = Self((1 << Kind::ALL.len()) - 1);
    /// Words and numbers.
    pub const TEXT: Self = Self(Kind::Words.bit() | Kind::Number.bit());

    /// The set holding only `kind`.
    pub const fn only(kind: Kind) -> Self {
        Self(kind.bit())
    }

    /// The set holding each of `kinds`.
    pub fn of(kinds: &[Kind]) -> Self {
        kinds.iter().fold(Self::NONE, |set, &kind| set.with(kind))
    }

    /// This set with `kind` added.
    pub const fn with(self, kind: Kind) -> Self {
        Self(self.0 | kind.bit())
    }

    pub const fn contains(self, kind: Kind) -> bool {
        self.0 & kind.bit() != 0
    }

    /// The kinds in this set, in the order of [`Kind::ALL`].
    pub fn iter(self) -> impl Iterator<Item = Kind> {
        Kind::ALL
            .into_iter()
            .filter(move |&kind| self.contains(kind))
    }
}

impl From<Kind> for Kinds {
    fn from(kind: Kind) -> Self {
        Self::only(kind)
    }
}

/// Which member of a kind an occupant is, where the covering table needs to
/// know: a comparison's name and its line, a time divider, an outer divider,
/// a comment. The caller names the roles; the default tables use the names
/// below.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Role(pub String);

impl Role {
    pub fn new(role: impl Into<String>) -> Self {
        Self(role.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A comparison's name, and a comparison's line or mark.
    pub fn comparison() -> Self {
        Self::new("comparison")
    }

    /// A comment's words.
    pub fn comment() -> Self {
        Self::new("comment")
    }

    /// A time divider that is not one of the outer two.
    pub fn time_divider() -> Self {
        Self::new("time divider")
    }

    /// One of the outer dividers.
    pub fn outer_divider() -> Self {
        Self::new("outer divider")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_twelve_kinds_and_only_the_reading_path_is_not_ink() {
        assert_eq!(Kind::ALL.len(), 12);
        let not_ink: Vec<_> = Kind::ALL.into_iter().filter(|k| !k.is_ink()).collect();
        assert_eq!(not_ink, [Kind::ReadingPath]);
        assert_eq!(Kinds::ANY.iter().count(), 12);
    }

    #[test]
    fn text_is_words_and_numbers() {
        assert_eq!(
            Kinds::TEXT.iter().collect::<Vec<_>>(),
            [Kind::Words, Kind::Number]
        );
        assert!(Kinds::of(&[Kind::Key, Kind::Words]).contains(Kind::Key));
        assert!(!Kinds::TEXT.contains(Kind::DataLine));
    }
}
