// SPDX-License-Identifier: GPL-3.0-only

//! Inline content: a short text, such as a title or a label, read with
//! the same inline rules as a message's text (Juliet's decision D2,
//! 18 Nov) — emphasis, code spans, links, maths — while what would make a
//! block of it (`# `, `- `, `> `, a fence) stays as it was written, as in
//! a one-line control it has no block to make. Every run of it keeps its
//! source range, so the text can be edited as written and copied as
//! written.

use std::ops::Range;

use crate::Dialect;
use crate::display::{Look, Object};
use crate::syntax::{InlineKind, Leaf, LeafKind, analyse_in};

/// A run of inline content: its source, what it shows, how it looks, and
/// the formula or box drawn in its place, if one is
#[derive(Clone, Debug, PartialEq)]
pub struct InlineRun {
    pub src: Range<usize>,
    pub shown: Range<usize>,
    pub look: Look,
    /// Drawn in place of the run's shown text (a single
    /// [`crate::display::OBJECT`])
    pub object: Option<Object>,
    /// The address of the link it is in
    pub link: Option<String>,
}

/// A short text as it shows
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InlineContent {
    source: String,
    shown: String,
    runs: Vec<InlineRun>,
}

impl InlineContent {
    /// `text` as it is: nothing interpreted (a file name, a password's
    /// label)
    pub fn literal(text: &str) -> Self {
        InlineContent {
            source: text.to_string(),
            shown: text.to_string(),
            runs: vec![InlineRun {
                src: 0..text.len(),
                shown: 0..text.len(),
                look: Look::default(),
                object: None,
                link: None,
            }],
        }
    }

    /// `text` read as inline Markdown with maths
    pub fn parse(text: &str) -> Self {
        let analysis = analyse_in(text, Dialect::Chat);
        let mut content = InlineContent {
            source: text.to_string(),
            ..Self::default()
        };
        let mut at = 0;
        for leaf in &analysis.leaves {
            // What lies between leaves (a line break) as it is
            if leaf.range.start > at {
                content.push(at..leaf.range.start, Look::default(), None, None);
            }
            content.leaf(leaf);
            at = at.max(leaf.range.end);
        }
        if at < text.len() {
            content.push(at..text.len(), Look::default(), None, None);
        }
        content
    }

    /// The text as written
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The text as it shows, an object's place an [`crate::display::OBJECT`]
    pub fn shown(&self) -> &str {
        &self.shown
    }

    pub fn runs(&self) -> &[InlineRun] {
        &self.runs
    }

    fn push(
        &mut self,
        src: Range<usize>,
        look: Look,
        object: Option<Object>,
        link: Option<String>,
    ) {
        if src.is_empty() {
            return;
        }
        let start = self.shown.len();
        match &object {
            Some(_) => self.shown.push(crate::display::OBJECT),
            None => self.shown.push_str(&self.source[src.clone()]),
        }
        let shown = start..self.shown.len();
        if object.is_none()
            && let Some(last) = self.runs.last_mut()
            && last.object.is_none()
            && last.look == look
            && last.link == link
            && last.src.end == src.start
        {
            last.src.end = src.end;
            last.shown.end = shown.end;
            return;
        }
        self.runs.push(InlineRun {
            src,
            shown,
            look,
            object,
            link,
        });
    }

    /// A leaf's text: its inline markup hidden, all else as written
    fn leaf(&mut self, leaf: &Leaf) {
        let text = leaf.range.clone();
        if let LeafKind::Math { latex } = &leaf.kind {
            let look = Look::default();
            self.push(
                text,
                look,
                Some(Object::Math(latex.trim().to_string())),
                None,
            );
            return;
        }
        if !matches!(
            leaf.kind,
            LeafKind::Paragraph | LeafKind::Heading(_) | LeafKind::Image { .. }
        ) {
            self.push(text, Look::default(), None, None);
            return;
        }
        // Inline markup: marks an inline element reveals (`**`, a link's
        // brackets and address, a backslash), not the block's own (`# `)
        let hidden = |at: usize| {
            leaf.marks
                .iter()
                .any(|m| m.range.start <= at && at < m.range.end && m.reveal != leaf.range)
        };
        let mut cuts: Vec<usize> = vec![text.start, text.end];
        for m in &leaf.marks {
            cuts.extend([m.range.start, m.range.end]);
        }
        for i in &leaf.inlines {
            cuts.extend([i.range.start, i.range.end]);
        }
        cuts.retain(|c| text.start <= *c && *c <= text.end);
        cuts.sort_unstable();
        cuts.dedup();
        let mut done_until = text.start;
        for seg in cuts.windows(2).map(|w| w[0]..w[1]) {
            if seg.start < done_until || hidden(seg.start) {
                continue;
            }
            let replaced = leaf.inlines.iter().find(|i| {
                i.kind.is_replaced() && i.range.start <= seg.start && seg.end <= i.range.end
            });
            let (look, link) = look_at(leaf, seg.start);
            match replaced.map(|i| (&i.kind, i.range.clone())) {
                Some((InlineKind::Math { latex, .. }, range)) => {
                    self.push(
                        range.clone(),
                        look,
                        Some(Object::Math(latex.trim().to_string())),
                        link,
                    );
                    done_until = range.end;
                }
                Some((InlineKind::Entity { text: reads }, range)) => {
                    let start = self.shown.len();
                    self.shown.push_str(reads);
                    self.runs.push(InlineRun {
                        src: range.clone(),
                        shown: start..self.shown.len(),
                        look,
                        object: None,
                        link,
                    });
                    done_until = range.end;
                }
                // A task's box is block syntax: as written
                _ => self.push(seg, look, None, link),
            }
        }
    }
}

/// How the text at `at` looks, and the link it is in
fn look_at(leaf: &Leaf, at: usize) -> (Look, Option<String>) {
    let mut look = Look::default();
    let mut link = None;
    for element in leaf
        .inlines
        .iter()
        .filter(|i| i.range.start <= at && at < i.range.end)
    {
        match &element.kind {
            InlineKind::Emphasis => look.italic = true,
            InlineKind::Strong => look.bold = true,
            InlineKind::Strikethrough => look.strike = true,
            InlineKind::Code => look.code = true,
            InlineKind::Link { url } => {
                look.link = true;
                link = Some(url.clone());
            }
            InlineKind::Footnote => look.sup = true,
            _ => {}
        }
    }
    (look, link)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown_runs(c: &InlineContent) -> Vec<(String, Look)> {
        c.runs()
            .iter()
            .map(|r| (c.shown()[r.shown.clone()].to_string(), r.look))
            .collect()
    }

    /// D2's example: maths and code spans are read, as is `**speed**`, as
    /// body text reads them
    #[test]
    fn a_title_reads_as_body_text_reads() {
        let c = InlineContent::parse("Fix `$PATH` handling for $\\mathrm{SL}_n$ and **speed**");
        let runs = shown_runs(&c);
        assert_eq!(c.shown(), "Fix $PATH handling for x and speed");
        assert!(runs.iter().any(|(t, l)| t == "$PATH" && l.code), "{runs:?}");
        assert!(runs.iter().any(|(t, l)| t == "speed" && l.bold), "{runs:?}");
        let math: Vec<&InlineRun> = c.runs().iter().filter(|r| r.object.is_some()).collect();
        assert_eq!(math.len(), 1);
        assert_eq!(math[0].object, Some(Object::Math("\\mathrm{SL}_n".into())));
        assert_eq!(&c.source()[math[0].src.clone()], "$\\mathrm{SL}_n$");
    }

    /// In a one-line control a block's syntax has no block to make
    #[test]
    fn block_syntax_stays_as_written() {
        for title in [
            "# Not a heading",
            "- not a list",
            "> not a quote",
            "1. not a list",
            "---",
        ] {
            assert_eq!(InlineContent::parse(title).shown(), title, "{title:?}");
        }
        let c = InlineContent::parse("## Bound for $\\mathrm{SL}_n$");
        assert_eq!(c.shown(), "## Bound for x");
        let c = InlineContent::parse("A [link](https://example.org) here");
        assert_eq!(c.shown(), "A link here");
        assert!(
            c.runs()
                .iter()
                .any(|r| r.link.as_deref() == Some("https://example.org"))
        );
    }

    #[test]
    fn literal_content_is_its_text() {
        let c = InlineContent::literal("Diameter of $\\mathrm{SL}_n(F)$");
        assert_eq!(c.shown(), c.source());
        assert_eq!(c.runs().len(), 1);
    }

    #[test]
    fn display_maths_in_a_title_is_a_formula() {
        let c = InlineContent::parse("\\[x^2\\]");
        assert_eq!(c.shown(), "x");
        assert_eq!(c.runs()[0].object, Some(Object::Math("x^2".into())));
    }
}
