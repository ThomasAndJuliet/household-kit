// SPDX-License-Identifier: GPL-3.0-only

//! A document as one immutable snapshot: its [`Source`] (the text, which is
//! the document, and its revision) and the [`Analysis`] of it in a
//! [`Dialect`]. An edit makes a new snapshot and the [`Splice`]s that say
//! which leaves changed; streaming text in is an edit at the end
//! ([`DocumentSnapshot::append`]).

use std::ops::Range;
use std::sync::Arc;

use crate::Dialect;
use crate::blocks::{Blocks, Splice};
use crate::syntax::{Analysis, LeafKind};

/// A text's revision: every edit makes a new one
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Revision(pub u64);

/// A document's text at one revision
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    text: Arc<str>,
    revision: Revision,
}

impl Source {
    pub fn new(text: impl Into<Arc<str>>, revision: Revision) -> Self {
        Source {
            text: text.into(),
            revision,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }
}

/// Bytes of a source at one revision, on character boundaries
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SourceRange {
    pub revision: Revision,
    pub bytes: Range<usize>,
}

impl SourceRange {
    /// `bytes` of `source`, if they are in it and on character boundaries
    pub fn new(source: &Source, bytes: Range<usize>) -> Option<Self> {
        let text = source.text();
        (bytes.start <= bytes.end
            && bytes.end <= text.len()
            && text.is_char_boundary(bytes.start)
            && text.is_char_boundary(bytes.end))
        .then_some(SourceRange {
            revision: source.revision(),
            bytes,
        })
    }
}

/// `range` of a text replaced by `insert`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub insert: String,
}

impl Edit {
    /// How far the offsets after the edit move
    pub fn delta(&self) -> isize {
        self.insert.len() as isize - self.range.len() as isize
    }
}

/// A source and its analysis
#[derive(Clone, Debug)]
pub struct DocumentSnapshot {
    source: Source,
    dialect: Dialect,
    blocks: Arc<Blocks>,
}

/// A leaf of a snapshot: its index, what it is, and its source range
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockRef<'a> {
    pub index: usize,
    pub kind: &'a LeafKind,
    pub range: &'a Range<usize>,
}

impl DocumentSnapshot {
    /// `text` analysed whole, in `dialect`
    pub fn parse(source: Source, dialect: Dialect) -> Self {
        let blocks = Blocks::new_in(source.text(), source.revision().0, dialect);
        DocumentSnapshot {
            source,
            dialect,
            blocks: Arc::new(blocks),
        }
    }

    pub fn source(&self) -> &Source {
        &self.source
    }

    pub fn text(&self) -> &str {
        self.source.text()
    }

    pub fn dialect(&self) -> Dialect {
        self.dialect
    }

    pub fn analysis(&self) -> &Analysis {
        self.blocks.analysis()
    }

    /// Bytes parsed to make this snapshot from the last
    pub fn parsed(&self) -> usize {
        self.blocks.parsed
    }

    /// The snapshot after `edit`, which must be on character boundaries,
    /// and what changed in the leaves; only the blocks around the edit are
    /// parsed again
    pub fn edit(&self, edit: &Edit) -> (Self, Vec<Splice>) {
        let old = self.text();
        assert!(
            edit.range.end <= old.len()
                && old.is_char_boundary(edit.range.start)
                && old.is_char_boundary(edit.range.end),
            "an edit on character boundaries"
        );
        let mut text = String::with_capacity(old.len() + edit.insert.len());
        text.push_str(&old[..edit.range.start]);
        text.push_str(&edit.insert);
        text.push_str(&old[edit.range.end..]);
        let revision = Revision(self.source.revision.0 + 1);
        let mut blocks = (*self.blocks).clone();
        let splices = blocks.edit(
            &text,
            edit.range.clone(),
            &old[edit.range.clone()],
            edit.insert.len(),
            revision.0,
        );
        (
            DocumentSnapshot {
                source: Source::new(text, revision),
                dialect: self.dialect,
                blocks: Arc::new(blocks),
            },
            splices,
        )
    }

    /// The snapshot with `more` on the end: streaming
    pub fn append(&self, more: &str) -> (Self, Vec<Splice>) {
        let end = self.text().len();
        self.edit(&Edit {
            range: end..end,
            insert: more.to_string(),
        })
    }

    /// Its leaves, in order
    pub fn blocks(&self) -> impl Iterator<Item = BlockRef<'_>> {
        self.analysis()
            .leaves
            .iter()
            .enumerate()
            .map(|(index, leaf)| BlockRef {
                index,
                kind: &leaf.kind,
                range: &leaf.range,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::analyse_in;

    #[test]
    fn edits_and_appends_give_the_analysis_of_the_whole() {
        let doc = DocumentSnapshot::parse(
            Source::new("Para one.\n\nPara two.\n", Revision(0)),
            Dialect::Chat,
        );
        let (doc, splices) = doc.append("\\[\nx\n=\ny\n\\]\n");
        assert_eq!(doc.source().revision(), Revision(1));
        assert!(!splices.is_empty());
        assert_eq!(doc.analysis(), &analyse_in(doc.text(), Dialect::Chat));
        assert!(
            doc.blocks()
                .any(|b| matches!(b.kind, LeafKind::Math { latex } if latex == "\nx\n=\ny\n"))
        );
        let (doc, _) = doc.edit(&Edit {
            range: 0..4,
            insert: "# Heading".into(),
        });
        assert_eq!(doc.analysis(), &analyse_in(doc.text(), Dialect::Chat));
        assert!(matches!(
            doc.blocks().next().unwrap().kind,
            LeafKind::Heading(1)
        ));
    }

    #[test]
    fn source_ranges_are_on_character_boundaries() {
        let source = Source::new("é!", Revision(3));
        assert!(SourceRange::new(&source, 0..2).is_some());
        assert!(SourceRange::new(&source, 1..2).is_none());
        assert!(SourceRange::new(&source, 0..4).is_none());
        assert_eq!(
            SourceRange::new(&source, 2..3).unwrap().revision,
            Revision(3)
        );
    }
}
