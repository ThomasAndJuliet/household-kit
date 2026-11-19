// SPDX-License-Identifier: GPL-3.0-only

//! Positions and ranges in a source that follow its edits: an [`Anchor`]
//! (a selection's end, a caret) and [`Annotations`], ranges of the source
//! marked with who wrote them and in which version. Authors and versions
//! are ids; the names and colours they are shown with are the app's.

use std::ops::Range;

use crate::document::Edit;

/// Which way a position goes when text is put in where it is
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Affinity {
    /// It stays before what is put in
    Before,
    /// It moves after what is put in
    After,
}

/// A byte position in a source, kept on its text through edits
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Anchor {
    offset: usize,
    affinity: Affinity,
}

impl Anchor {
    pub fn new(offset: usize, affinity: Affinity) -> Self {
        Anchor { offset, affinity }
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn affinity(&self) -> Affinity {
        self.affinity
    }

    /// Where it is after `edit`: before the edit, where it was; after it,
    /// moved by what it changed; in what it replaced, at the replacement's
    /// start or end, by its affinity
    pub fn rebase(&mut self, edit: &Edit) {
        self.offset = rebase(self.offset, self.affinity, edit);
    }
}

fn rebase(offset: usize, affinity: Affinity, edit: &Edit) -> usize {
    let range = &edit.range;
    let inserted = edit.insert.len();
    if offset < range.start || (offset == range.start && affinity == Affinity::Before) {
        offset
    } else if offset > range.end {
        offset - range.len() + inserted
    } else {
        match affinity {
            Affinity::Before => range.start,
            Affinity::After => range.start + inserted,
        }
    }
}

/// Who wrote a range: an id, not a name
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AuthorId(pub String);

/// The version of the text a range was written in
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VersionId(pub u64);

/// A range of the source and who wrote it
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Annotation {
    pub range: Range<usize>,
    pub author: AuthorId,
    pub version: VersionId,
}

/// A source's annotations, in order of their start, following its edits
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Annotations {
    items: Vec<Annotation>,
}

impl Annotations {
    pub fn add(&mut self, annotation: Annotation) {
        let at = self
            .items
            .partition_point(|a| a.range.start <= annotation.range.start);
        self.items.insert(at, annotation);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Annotation> {
        self.items.iter()
    }

    /// The annotations over `offset`
    pub fn at(&self, offset: usize) -> impl Iterator<Item = &Annotation> {
        self.items
            .iter()
            .filter(move |a| a.range.start <= offset && offset < a.range.end)
    }

    /// Each range after `edit`: text put in at an end is not the range's,
    /// and a range the edit took away goes
    pub fn rebase(&mut self, edit: &Edit) {
        for a in &mut self.items {
            let start = rebase(a.range.start, Affinity::After, edit);
            let end = rebase(a.range.end, Affinity::Before, edit);
            a.range = start..end.max(start);
        }
        self.items.retain(|a| !a.range.is_empty());
        self.items.sort_by_key(|a| a.range.start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(range: Range<usize>, insert: &str) -> Edit {
        Edit {
            range,
            insert: insert.into(),
        }
    }

    #[test]
    fn anchors_follow_edits_by_their_affinity() {
        let mut before = Anchor::new(5, Affinity::Before);
        let mut after = Anchor::new(5, Affinity::After);
        before.rebase(&edit(5..5, "abc"));
        after.rebase(&edit(5..5, "abc"));
        assert_eq!((before.offset(), after.offset()), (5, 8));
        let mut later = Anchor::new(10, Affinity::Before);
        later.rebase(&edit(2..4, ""));
        assert_eq!(later.offset(), 8);
        let mut inside = Anchor::new(3, Affinity::After);
        inside.rebase(&edit(2..6, "xy"));
        assert_eq!(inside.offset(), 4);
    }

    #[test]
    fn annotations_keep_their_text_through_edits() {
        let mut notes = Annotations::default();
        let ann = |range: Range<usize>, who: &str| Annotation {
            range,
            author: AuthorId(who.into()),
            version: VersionId(1),
        };
        notes.add(ann(4..9, "juliet"));
        notes.add(ann(0..2, "agent"));
        // Typed at the start of Juliet's range: not hers
        notes.rebase(&edit(4..4, "new "));
        assert_eq!(
            notes.iter().map(|a| a.range.clone()).collect::<Vec<_>>(),
            [0..2, 8..13]
        );
        assert_eq!(
            notes.at(9).next().unwrap().author,
            AuthorId("juliet".into())
        );
        // Deleted whole: gone
        notes.rebase(&edit(0..2, ""));
        notes.rebase(&edit(6..11, ""));
        assert_eq!(notes.iter().count(), 0);
    }
}
