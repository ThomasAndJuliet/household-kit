// SPDX-License-Identifier: GPL-3.0-only

//! What each leaf shows, given where the caret is: its visible text, how
//! each part of it looks, the objects drawn in it (formulas, checkboxes),
//! and the figure drawn for it (a formula, an image, a table, a rule).
//!
//! The shown text is made of *pieces*, each a source range shown as it is,
//! or, for an entity or a formula, shown as something else. Source offsets
//! map to shown offsets and back through them, so the caret, selections,
//! clicks and spelling stay in source offsets.

use std::ops::Range;

use crate::Dialect;
use crate::syntax::{Analysis, ContainerKind, InlineKind, Leaf, LeafKind};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Rendered, with the markup of what the caret is in shown
    #[default]
    Literate,
    /// The text as it is, highlighted
    Source,
}

/// How a run of shown text looks
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Look {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    /// An inline code span
    pub code: bool,
    pub link: bool,
    /// Markup, shown because the caret is in it
    pub markup: bool,
    /// Set in the monospace font
    pub mono: bool,
    /// A heading's level, or 0
    pub heading: u8,
    /// A formula's source
    pub math: bool,
    /// `<u>…</u>`
    pub underline: bool,
    /// `==…==`: drawn on a highlighter's wash
    pub highlight: bool,
    /// `<sup>…</sup>` and `<sub>…</sub>`: set smaller, raised or lowered
    pub sup: bool,
    pub sub: bool,
}

/// Something drawn in the text in place of its source
#[derive(Clone, Debug, PartialEq, Hash)]
pub enum Object {
    Math(String),
    Checkbox(bool),
}

/// A piece of shown text and where it comes from
#[derive(Clone, Debug, PartialEq, Hash)]
pub struct Piece {
    pub src: Range<usize>,
    pub shown: Range<usize>,
    /// Shown as something other than its source: offsets inside it go to
    /// its ends
    pub atomic: bool,
}

/// What a leaf draws besides its text
#[derive(Clone, Debug, PartialEq, Hash)]
pub enum Figure {
    Math(String),
    /// A Mermaid diagram's source
    Diagram(String),
    /// A chartgen chart's specification
    Chart(String),
    Image {
        url: String,
        alt: String,
    },
    Table,
    Rule,
}

/// A leaf as shown
#[derive(Clone, Debug, Default, PartialEq, Hash)]
pub struct Shown {
    pub text: String,
    /// Covering `text`, in order
    pub looks: Vec<(Range<usize>, Look)>,
    pub pieces: Vec<Piece>,
    /// The objects in the text, each at an [`OBJECT`] in it, in order
    pub objects: Vec<(usize, Object)>,
    pub figure: Option<Figure>,
    /// The figure is drawn instead of the text: the caret is not in the leaf
    pub figure_only: bool,
    /// Nothing is shown: a spacer the caret is not on
    pub collapsed: bool,
    /// The caret is in the leaf
    pub active: bool,
}

/// The character an object stands on in the shown text: a letter, drawn
/// transparent, whose letter spacing makes room for the object. Not a space,
/// as cosmic-text hangs spaces past the end of a line
pub const OBJECT: char = 'x';

/// Whether `caret` is within `range`, ends included
fn within(caret: Option<usize>, range: &Range<usize>) -> bool {
    caret.is_some_and(|c| range.start <= c && c <= range.end)
}

/// [`show`], in the Paper view (`paper`) with a heading's marks (`## `,
/// a setext underline) never shown: the typeset look; headings are changed
/// from Format ▸ Turn into, the Styles panel and the keys
pub fn show_in(
    text: &str,
    analysis: &Analysis,
    index: usize,
    caret: Option<usize>,
    mode: Mode,
    paper: bool,
) -> Shown {
    let leaf = &analysis.leaves[index];
    if paper && mode == Mode::Literate && matches!(leaf.kind, LeafKind::Heading(_)) {
        let mut plain = analysis.clone_leaf(index);
        // Hidden whatever the caret: revealed nowhere
        for m in plain.marks.iter_mut().filter(|m| m.reveal == leaf.range) {
            m.reveal = usize::MAX..usize::MAX;
        }
        let one = Analysis {
            leaves: vec![plain],
            containers: analysis.containers.clone(),
        };
        return show(text, &one, 0, caret, mode);
    }
    show(text, analysis, index, caret, mode)
}

/// The leaf `index` of `analysis` as shown, with the caret at `caret`
pub fn show(
    text: &str,
    analysis: &Analysis,
    index: usize,
    caret: Option<usize>,
    mode: Mode,
) -> Shown {
    show_with(text, analysis, index, caret, mode, Dialect::Document)
}

/// [`show`] of a text in `dialect`: in a chat, a line break in a paragraph
/// stays a line break ([`Dialect::soft_breaks_kept`])
pub fn show_with(
    text: &str,
    analysis: &Analysis,
    index: usize,
    caret: Option<usize>,
    mode: Mode,
    dialect: Dialect,
) -> Shown {
    let keep = dialect.soft_breaks_kept();
    let inline = |text: &str, leaf: &Leaf, caret: Option<usize>, base: Look| {
        inline_with(text, leaf, caret, base, keep)
    };
    let leaf = &analysis.leaves[index];
    let caret = caret.filter(|c| leaf.range.start <= *c && *c <= leaf.range.end);
    if mode == Mode::Source {
        return show_source(text, leaf, caret);
    }
    let active = caret.is_some();
    let base = Look {
        heading: if let LeafKind::Heading(n) = leaf.kind {
            n
        } else {
            0
        },
        ..Look::default()
    };
    let mut shown = match &leaf.kind {
        LeafKind::Spacer if !active => Shown {
            collapsed: true,
            ..Shown::default()
        },
        LeafKind::Math { latex } => {
            let mut shown = verbatim(
                text,
                leaf,
                Look {
                    mono: true,
                    math: true,
                    ..base
                },
            );
            shown.figure = Some(Figure::Math(latex.clone()));
            shown
        }
        LeafKind::Image { url, alt } => {
            let mut shown = inline(text, leaf, caret, base);
            shown.figure = Some(Figure::Image {
                url: url.clone(),
                alt: alt.clone(),
            });
            shown
        }
        // Always a table: its cells are edited where they are drawn
        LeafKind::Table(_) => Shown {
            figure: Some(Figure::Table),
            ..Shown::default()
        },
        LeafKind::Rule => {
            let mut shown = verbatim(
                text,
                leaf,
                Look {
                    markup: true,
                    ..base
                },
            );
            shown.figure = Some(Figure::Rule);
            shown
        }
        LeafKind::Code { lang }
            if (lang == "mermaid" && fenced_shut(text, leaf)) || lang == "chartgen" =>
        {
            // Drawn, with its source while the caret is in it
            let source = inline(text, leaf, None, base).text;
            let mut shown = inline(text, leaf, caret, Look { mono: true, ..base });
            shown.figure = Some(if lang == "mermaid" {
                Figure::Diagram(source)
            } else {
                Figure::Chart(source)
            });
            shown
        }
        LeafKind::Code { .. } | LeafKind::Html | LeafKind::FrontMatter => {
            inline(text, leaf, caret, Look { mono: true, ..base })
        }
        LeafKind::Paragraph | LeafKind::Heading(_) | LeafKind::Spacer => {
            inline(text, leaf, caret, base)
        }
    };
    shown.active = active;
    if shown.figure.is_some() && (!active || matches!(leaf.kind, LeafKind::Table(_))) {
        shown.figure_only = true;
        shown.text.clear();
        shown.looks.clear();
        shown.pieces.clear();
        shown.objects.clear();
    }
    shown
}

/// Whether a fenced code block has its closing fence: one still being
/// written shows as code
pub fn fenced_shut(text: &str, leaf: &Leaf) -> bool {
    leaf.marks
        .iter()
        .filter(|m| {
            let s = text[m.range.clone()].trim_matches(|c: char| c.is_whitespace() || c == '>');
            s.starts_with("```") || s.starts_with("~~~")
        })
        .count()
        >= 2
}

/// A table cell's text, without the spaces around it
pub fn cell_text(text: &str, cell: &Range<usize>) -> Range<usize> {
    let s = &text[cell.clone()];
    let start = cell.start + (s.len() - s.trim_start().len());
    let end = (cell.end - (s.len() - s.trim_end().len())).max(start);
    start..end
}

/// A table cell as shown in the table's figure: its text, rendered, with
/// the markup the caret is in shown
pub fn show_cell(text: &str, leaf: &Leaf, cell: Range<usize>, caret: Option<usize>) -> Shown {
    let range = cell_text(text, &cell);
    let sub = Leaf {
        range: range.clone(),
        kind: LeafKind::Paragraph,
        prefixes: Vec::new(),
        marks: leaf
            .marks
            .iter()
            .filter(|m| range.start <= m.range.start && m.range.end <= range.end)
            .cloned()
            .collect(),
        inlines: leaf
            .inlines
            .iter()
            .filter(|i| range.start <= i.range.start && i.range.end <= range.end)
            .cloned()
            .collect(),
        containers: Vec::new(),
    };
    let mut shown = inline(text, &sub, caret, Look::default());
    if shown.pieces.is_empty() {
        // An empty cell still takes the caret
        shown.pieces.push(Piece {
            src: range.clone(),
            shown: 0..0,
            atomic: false,
        });
    }
    shown
}

/// The leaf's text with only its container prefixes hidden
fn verbatim(text: &str, leaf: &Leaf, look: Look) -> Shown {
    let mut shown = Shown::default();
    let mut at = leaf.range.start;
    for prefix in leaf
        .prefixes
        .iter()
        .chain(std::iter::once(&(leaf.range.end..leaf.range.end)))
    {
        if prefix.start > at {
            push(&mut shown, text, at..prefix.start, look);
        }
        at = at.max(prefix.end);
    }
    shown
}

fn push(shown: &mut Shown, text: &str, src: Range<usize>, look: Look) {
    let start = shown.text.len();
    shown.text.push_str(&text[src.clone()]);
    let shown_range = start..shown.text.len();
    add_look(shown, shown_range.clone(), look);
    shown.pieces.push(Piece {
        src,
        shown: shown_range,
        atomic: false,
    });
}

/// [`push`], with the soft line breaks in `src` (a line ending within a
/// paragraph, not a hard break) shown as the space they are in Markdown,
/// and the spaces around them not shown (16 Oct: they broke the line, so a
/// paragraph wrapped in its source was set in short lines, which the Paper
/// view's justification then stretched)
fn push_soft(shown: &mut Shown, text: &str, src: Range<usize>, look: Look, leaf: &Leaf) {
    let hard = |at: usize| {
        leaf.inlines.iter().any(|i| {
            matches!(i.kind, InlineKind::HardBreak) && i.range.start <= at && at < i.range.end
        })
    };
    let mut at = src.start;
    for (i, _) in text[src.clone()].match_indices('\n') {
        let newline = src.start + i;
        if newline < at || hard(newline) {
            continue;
        }
        let before = text[at..newline].trim_end_matches([' ', '\t']);
        if !before.is_empty() {
            push(shown, text, at..at + before.len(), look);
        }
        let start = shown.text.len();
        shown.text.push(' ');
        add_look(shown, start..start + 1, look);
        shown.pieces.push(Piece {
            src: newline..newline + 1,
            shown: start..start + 1,
            atomic: false,
        });
        at = newline + 1;
        at += text[at..src.end].len() - text[at..src.end].trim_start_matches([' ', '\t']).len();
    }
    if at < src.end {
        push(shown, text, at..src.end, look);
    }
}

fn add_look(shown: &mut Shown, range: Range<usize>, look: Look) {
    if range.is_empty() {
        return;
    }
    // An object's look stays its own, so it can be found by its range
    let object = |at: usize| shown.objects.iter().any(|(o, _)| *o == at);
    let merge = shown.looks.last().is_some_and(|(last, l)| {
        *l == look && last.end == range.start && !object(last.start) && !object(range.start)
    });
    match shown.looks.last_mut() {
        Some((last, _)) if merge => last.end = range.end,
        _ => shown.looks.push((range, look)),
    }
}

/// Text with inline elements: their markup hidden unless the caret is in
/// them, formulas and the like shown as objects
fn inline(text: &str, leaf: &Leaf, caret: Option<usize>, base: Look) -> Shown {
    inline_with(text, leaf, caret, base, false)
}

/// [`inline`], with a paragraph's line breaks kept if `keep`
fn inline_with(text: &str, leaf: &Leaf, caret: Option<usize>, base: Look, keep: bool) -> Shown {
    let mut shown = Shown::default();
    // Prose, whose line endings are spaces; not code, HTML or front matter
    let soft = !keep
        && matches!(
            leaf.kind,
            LeafKind::Paragraph | LeafKind::Heading(_) | LeafKind::Image { .. }
        )
        && !base.mono;
    // Every place something starts or stops
    let mut cuts: Vec<usize> = vec![leaf.range.start, leaf.range.end];
    for r in &leaf.prefixes {
        cuts.extend([r.start, r.end]);
    }
    for m in &leaf.marks {
        cuts.extend([m.range.start, m.range.end]);
    }
    for i in &leaf.inlines {
        cuts.extend([i.range.start, i.range.end]);
    }
    cuts.retain(|c| leaf.range.start <= *c && *c <= leaf.range.end);
    cuts.sort_unstable();
    cuts.dedup();

    let replaced: Vec<&crate::syntax::Inline> = leaf
        .inlines
        .iter()
        .filter(|i| i.kind.is_replaced() && !within(caret, &i.range))
        .collect();
    let segments = cuts.windows(2).map(|w| w[0]..w[1]);
    for seg in segments {
        if leaf
            .prefixes
            .iter()
            .any(|p| p.start <= seg.start && seg.end <= p.end)
        {
            continue;
        }
        if let Some(element) = replaced
            .iter()
            .find(|i| i.range.start <= seg.start && seg.end <= i.range.end)
        {
            // Shown once, for its first segment
            if seg.start != element.range.start {
                continue;
            }
            let start = shown.text.len();
            let look = looks_at(leaf, element.range.start, caret, base);
            match &element.kind {
                InlineKind::Entity { text: reads } => shown.text.push_str(reads),
                InlineKind::Math { latex, .. } => {
                    shown.objects.push((start, Object::Math(latex.clone())));
                    shown.text.push(OBJECT);
                }
                InlineKind::Task { checked } => {
                    shown.objects.push((start, Object::Checkbox(*checked)));
                    shown.text.push(OBJECT);
                }
                _ => unreachable!(),
            }
            let range = start..shown.text.len();
            add_look(&mut shown, range.clone(), look);
            shown.pieces.push(Piece {
                src: element.range.clone(),
                shown: range,
                atomic: true,
            });
            continue;
        }
        let mark = leaf
            .marks
            .iter()
            .find(|m| m.range.start <= seg.start && seg.end <= m.range.end);
        let look = match mark {
            Some(m) if !within(caret, &m.reveal) => continue,
            Some(_) => Look {
                markup: true,
                ..looks_at(leaf, seg.start, caret, base)
            },
            None => looks_at(leaf, seg.start, caret, base),
        };
        if soft {
            push_soft(&mut shown, text, seg, look, leaf);
        } else {
            push(&mut shown, text, seg, look);
        }
    }
    shown
}

/// How the text at `at` looks, from the elements it is in
fn looks_at(leaf: &Leaf, at: usize, caret: Option<usize>, base: Look) -> Look {
    let mut look = base;
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
            InlineKind::Link { .. } | InlineKind::Image { .. } => look.link = true,
            // A footnote's reference: its label raised, as a link
            InlineKind::Footnote => {
                look.link = true;
                look.sup = true;
            }
            InlineKind::Math { .. } if within(caret, &element.range) => {
                look.math = true;
                look.mono = true;
            }
            InlineKind::Html => look.markup = true,
            InlineKind::Underline => look.underline = true,
            InlineKind::Highlight => look.highlight = true,
            InlineKind::Superscript => look.sup = true,
            InlineKind::Subscript => look.sub = true,
            _ => {}
        }
    }
    look
}

/// The source as it is: markup and prefixes muted, headings bold
fn show_source(text: &str, leaf: &Leaf, caret: Option<usize>) -> Shown {
    let mut shown = Shown {
        active: caret.is_some(),
        ..Shown::default()
    };
    let base = Look {
        bold: matches!(leaf.kind, LeafKind::Heading(_)),
        mono: matches!(
            leaf.kind,
            LeafKind::Code { .. }
                | LeafKind::Html
                | LeafKind::FrontMatter
                | LeafKind::Table(_)
                | LeafKind::Math { .. }
        ),
        ..Look::default()
    };
    let mut cuts: Vec<usize> = vec![leaf.range.start, leaf.range.end];
    for r in leaf
        .prefixes
        .iter()
        .chain(leaf.marks.iter().map(|m| &m.range))
        .chain(leaf.inlines.iter().map(|i| &i.range))
    {
        cuts.extend([r.start, r.end]);
    }
    cuts.retain(|c| leaf.range.start <= *c && *c <= leaf.range.end);
    cuts.sort_unstable();
    cuts.dedup();
    for seg in cuts.windows(2).map(|w| w[0]..w[1]) {
        let markup = leaf
            .prefixes
            .iter()
            .chain(leaf.marks.iter().map(|m| &m.range))
            .any(|r| r.start <= seg.start && seg.end <= r.end);
        let mut look = looks_at(leaf, seg.start, None, base);
        look.markup |= markup;
        // The source keeps its lines even: nothing raised or lowered
        (look.sup, look.sub) = (false, false);
        if leaf.inlines.iter().any(|i| {
            matches!(i.kind, InlineKind::Math { .. })
                && i.range.start <= seg.start
                && seg.end <= i.range.end
        }) {
            look.math = true;
        }
        let start = shown.text.len();
        shown.text.push_str(&text[seg.clone()]);
        let range = start..shown.text.len();
        add_look(&mut shown, range.clone(), look);
        shown.pieces.push(Piece {
            src: seg,
            shown: range,
            atomic: false,
        });
    }
    if leaf.range.is_empty() {
        shown.pieces.push(Piece {
            src: leaf.range.clone(),
            shown: 0..0,
            atomic: false,
        });
    }
    shown
}

impl Shown {
    /// Where source offset `src` shows. Hidden source shows where the next
    /// shown piece starts.
    pub fn to_shown(&self, src: usize) -> usize {
        for piece in &self.pieces {
            if src < piece.src.start {
                return piece.shown.start;
            }
            if src <= piece.src.end {
                if piece.atomic {
                    return if src == piece.src.end {
                        piece.shown.end
                    } else {
                        piece.shown.start
                    };
                }
                return piece.shown.start + (src - piece.src.start).min(piece.shown.len());
            }
        }
        self.text.len()
    }

    /// The source offset shown at `shown`. Where two pieces meet, the start
    /// of the later if `after`, else the end of the earlier.
    pub fn to_source(&self, shown: usize, after: bool) -> Option<usize> {
        let mut found = None;
        for piece in &self.pieces {
            if shown < piece.shown.start {
                break;
            }
            if shown <= piece.shown.end {
                let src = if piece.atomic {
                    if shown == piece.shown.start {
                        piece.src.start
                    } else {
                        piece.src.end
                    }
                } else {
                    piece.src.start + (shown - piece.shown.start)
                };
                found = Some(src);
                if !after || shown < piece.shown.end {
                    break;
                }
            }
        }
        found
    }

    /// The parts of shown text that `src` covers
    pub fn shown_range(&self, src: Range<usize>) -> Range<usize> {
        self.to_shown(src.start)..self.to_shown(src.end)
    }

    /// Its source offsets moved by `by`: a leaf's layout is kept relative
    /// to the leaf's start, so that it holds wherever the leaf moves
    pub fn rebase(&mut self, by: isize) {
        for piece in &mut self.pieces {
            piece.src.start = piece.src.start.saturating_add_signed(by);
            piece.src.end = piece.src.end.saturating_add_signed(by);
        }
    }

    /// Hashed as it would be rebased to start from `base`: the same for
    /// the same leaf wherever it is in the text
    pub fn hash_from<H: std::hash::Hasher>(&self, base: usize, state: &mut H) {
        use std::hash::Hash;
        (
            &self.text,
            &self.looks,
            &self.objects,
            &self.figure,
            self.figure_only,
            self.collapsed,
            self.active,
        )
            .hash(state);
        for piece in &self.pieces {
            (
                piece.src.start.wrapping_sub(base),
                piece.src.end.wrapping_sub(base),
                &piece.shown,
                piece.atomic,
            )
                .hash(state);
        }
    }
}

/// The caret moved out of what is never shown, a container's prefix, to
/// the start of the text after it; in a table, into the nearest cell
pub fn settle(analysis: &Analysis, caret: usize) -> usize {
    let leaf = &analysis.leaves[analysis.leaf_at(caret)];
    if let LeafKind::Table(table) = &leaf.kind {
        let cells: Vec<&Range<usize>> = table.rows.iter().flatten().collect();
        if cells.iter().any(|c| c.start <= caret && caret <= c.end) {
            return caret;
        }
        return match cells.iter().find(|c| c.start > caret).or(cells.last()) {
            Some(c) if c.start > caret => c.start + 1.min(c.len()),
            Some(c) => c.end.saturating_sub(1).max(c.start),
            None => caret,
        };
    }
    leaf.prefixes
        .iter()
        .find(|prefix| prefix.start <= caret && caret < prefix.end)
        .map_or(caret, |prefix| prefix.end)
}

/// Each container a leaf is in, as drawn beside it
#[derive(Clone, Debug, PartialEq, Hash)]
pub enum Gutter {
    /// A quote's bar
    Bar,
    /// A list item's bullet or number, on its first leaf; otherwise only its
    /// indent
    Item {
        bullet: Option<String>,
        task: bool,
    },
    Footnote {
        label: Option<String>,
    },
}

/// What is drawn to the left of a leaf, outermost first
pub fn gutters(analysis: &Analysis, index: usize) -> Vec<Gutter> {
    let leaf = &analysis.leaves[index];
    leaf.containers
        .iter()
        .map(|&c| {
            let container = &analysis.containers[c];
            match &container.kind {
                ContainerKind::Quote => Gutter::Bar,
                ContainerKind::Item {
                    number, first_leaf, ..
                } => {
                    let first = *first_leaf == Some(leaf.range.start);
                    let task = leaf
                        .inlines
                        .first()
                        .is_some_and(|i| matches!(i.kind, InlineKind::Task { .. }));
                    let bullet = first.then(|| match number {
                        Some(n) => format!("{n}."),
                        // As the terminal's: one bullet for the outermost
                        // list, another for every list inside it
                        // (household-type)
                        None => {
                            let depth = leaf
                                .containers
                                .iter()
                                .filter(|&&c| {
                                    matches!(
                                        analysis.containers[c].kind,
                                        ContainerKind::Item { .. }
                                    )
                                })
                                .count();
                            household_type::text().list.bullets[usize::from(depth > 1)].to_string()
                        }
                    });
                    Gutter::Item {
                        bullet: if task && first { None } else { bullet },
                        task: task && first,
                    }
                }
                ContainerKind::Footnote { label } => Gutter::Footnote {
                    label: (container.range.start == leaf.range.start
                        || leaf.range.start <= container.range.start)
                        .then(|| label.clone()),
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::analyse;

    fn shown_at(text: &str, leaf: usize, caret: Option<usize>) -> Shown {
        show(text, &analyse(text), leaf, caret, Mode::Literate)
    }

    #[test]
    fn markup_hides_until_the_caret_is_in_it() {
        let text = "a **b** c";
        assert_eq!(shown_at(text, 0, None).text, "a b c");
        assert_eq!(shown_at(text, 0, Some(0)).text, "a b c");
        // At the element's ends, and inside it
        assert_eq!(shown_at(text, 0, Some(2)).text, "a **b** c");
        assert_eq!(shown_at(text, 0, Some(5)).text, "a **b** c");
        assert_eq!(shown_at(text, 0, Some(7)).text, "a **b** c");
        assert_eq!(shown_at(text, 0, Some(8)).text, "a b c");
        let shown = shown_at(text, 0, None);
        let bold = shown.looks.iter().find(|(_, l)| l.bold).unwrap();
        assert_eq!(&shown.text[bold.0.clone()], "b");
    }

    #[test]
    fn offsets_map_both_ways() {
        let text = "a **b** c";
        let shown = shown_at(text, 0, None);
        assert_eq!(shown.to_shown(0), 0);
        assert_eq!(shown.to_shown(2), 2);
        // Hidden markup goes to where the next shown text starts
        assert_eq!(shown.to_shown(3), 2);
        assert_eq!(shown.to_shown(4), 2);
        assert_eq!(shown.to_shown(5), 3);
        assert_eq!(shown.to_shown(9), 5);
        assert_eq!(shown.to_source(2, false), Some(2));
        assert_eq!(shown.to_source(2, true), Some(4));
        assert_eq!(shown.to_source(3, false), Some(5));
        assert_eq!(shown.to_source(3, true), Some(7));
        assert_eq!(shown.to_source(5, false), Some(9));
    }

    #[test]
    fn headings_show_their_hashes_only_when_active() {
        let text = "## Title\n";
        let a = analyse(text);
        // In the Paper view, never
        assert_eq!(
            show_in(text, &a, 0, Some(5), Mode::Literate, true).text,
            "Title",
            "{:?}",
            a.leaves[0]
        );
        assert_eq!(shown_at(text, 0, None).text, "Title");
        let active = shown_at(text, 0, Some(5));
        assert_eq!(active.text, "## Title");
        assert!(active.looks[0].1.markup && active.looks[0].1.heading == 2);
    }

    #[test]
    fn quotes_and_lists_never_show_their_prefixes() {
        let text = "> one\n> two\n\n- item\n  more\n";
        let a = analyse(text);
        // (their lines one paragraph each: a line ending is a space)
        assert_eq!(show(text, &a, 0, Some(8), Mode::Literate).text, "one two");
        assert_eq!(show(text, &a, 2, None, Mode::Literate).text, "item more");
        assert_eq!(gutters(&a, 0), [Gutter::Bar]);
        assert_eq!(
            gutters(&a, 2),
            [Gutter::Item {
                bullet: Some("•".into()),
                task: false
            }]
        );
        // The caret is kept out of them
        assert_eq!(settle(&a, 6), 8);
        assert_eq!(settle(&a, 13), 15);
    }

    #[test]
    fn formulas_and_entities_are_objects_until_entered() {
        let text = "x $a^2$ &amp; y";
        let shown = shown_at(text, 0, None);
        assert_eq!(shown.text, format!("x {OBJECT} & y"));
        assert_eq!(shown.objects, [(2, Object::Math("a^2".into()))]);
        // The formula is one unit
        assert_eq!(shown.to_shown(4), 2);
        assert_eq!(shown.to_shown(7), 2 + OBJECT.len_utf8());
        assert_eq!(shown.to_source(2 + OBJECT.len_utf8(), false), Some(7));
        let inside = shown_at(text, 0, Some(4));
        assert_eq!(inside.text, "x $a^2$ & y");
        assert!(
            inside
                .looks
                .iter()
                .any(|(r, l)| l.math && &inside.text[r.clone()] == "$a^2$")
        );
    }

    #[test]
    fn blocks_show_figures_until_entered() {
        let text = "Text\n\n$$\nx\n$$\n\n![a](i.png)\n\n---\n";
        let a = analyse(text);
        let math = show(text, &a, 2, None, Mode::Literate);
        assert!(math.figure_only && math.text.is_empty());
        assert_eq!(math.figure, Some(Figure::Math("\nx\n".into())));
        let editing = show(text, &a, 2, Some(7), Mode::Literate);
        assert!(!editing.figure_only);
        assert_eq!(editing.text, "$$\nx\n$$");
        let image = show(text, &a, 4, Some(16), Mode::Literate);
        assert_eq!(image.text, "![a](i.png)");
        let rule = show(text, &a, 6, None, Mode::Literate);
        assert_eq!(rule.figure, Some(Figure::Rule));
        // Spacers collapse unless the caret is on one
        assert!(show(text, &a, 1, None, Mode::Literate).collapsed);
        assert!(!show(text, &a, 1, Some(5), Mode::Literate).collapsed);
    }

    #[test]
    fn diagrams_and_charts_are_figures() {
        let text = "```mermaid\ngraph LR\n  A --> B\n```\n\n```chartgen\n{}\n```\n\n```mermaid\ngraph LR\n";
        let a = analyse(text);
        let diagram = show(text, &a, 0, None, Mode::Literate);
        assert_eq!(
            diagram.figure,
            Some(Figure::Diagram("graph LR\n  A --> B".into()))
        );
        assert!(diagram.figure_only);
        let editing = show(text, &a, 0, Some(12), Mode::Literate);
        assert!(!editing.figure_only && editing.text.starts_with("```mermaid"));
        assert_eq!(
            show(text, &a, 2, None, Mode::Literate).figure,
            Some(Figure::Chart("{}".into()))
        );
        // Still being written: code
        let last = a
            .leaves
            .iter()
            .rposition(|l| matches!(l.kind, LeafKind::Code { .. }))
            .unwrap();
        assert_eq!(show(text, &a, last, None, Mode::Literate).figure, None);
    }

    #[test]
    fn code_shows_its_fences_when_entered() {
        let text = "```rust\nlet x;\n```\n";
        let a = analyse(text);
        assert_eq!(show(text, &a, 0, None, Mode::Literate).text, "let x;");
        assert_eq!(
            show(text, &a, 0, Some(9), Mode::Literate).text,
            "```rust\nlet x;\n```"
        );
    }

    #[test]
    fn source_mode_shows_everything() {
        let text = "> ## A *b*\n";
        let a = analyse(text);
        let shown = show(text, &a, 0, None, Mode::Source);
        assert_eq!(shown.text, "> ## A *b*");
        assert_eq!(shown.to_shown(5), 5);
        assert!(
            shown
                .looks
                .iter()
                .any(|(r, l)| l.markup && shown.text[r.clone()].starts_with("> "))
        );
        assert!(
            shown
                .looks
                .iter()
                .any(|(r, l)| l.italic && &shown.text[r.clone()] == "b")
        );
    }

    #[test]
    fn table_cells_render() {
        let text = "| a | **b** |\n|---|---|\n| `c` | d |\n";
        let a = analyse(text);
        let leaf = &a.leaves[0];
        let LeafKind::Table(table) = &leaf.kind else {
            panic!()
        };
        let cells: Vec<String> = table
            .rows
            .iter()
            .flatten()
            .map(|c| show_cell(text, leaf, c.clone(), None).text)
            .collect();
        assert_eq!(
            show_cell(text, leaf, table.rows[0][1].clone(), Some(8)).text,
            "**b**"
        );
        assert_eq!(cells, ["a", "b", "c", "d"]);
    }

    #[test]
    fn tasks_show_a_checkbox() {
        let text = "- [x] done\n";
        let a = analyse(text);
        let shown = show(text, &a, 0, None, Mode::Literate);
        assert_eq!(shown.objects, [(0, Object::Checkbox(true))]);
        assert_eq!(
            gutters(&a, 0),
            [Gutter::Item {
                bullet: None,
                task: true
            }]
        );
    }

    /// Item 11: a footnote reference "[^keys]" shows as its label, raised
    /// and in the link colour; its "[^" and "]" come back while the caret
    /// is in it, and the label is no longer raised then
    #[test]
    fn a_footnote_reference_shows_as_a_raised_label() {
        let text = "Read it.[^keys] Then go.\n\n[^keys]: The keys are in the drawer.\n";
        let at = text.find("[^keys]").unwrap();
        let shown = shown_at(text, 0, None);
        assert_eq!(shown.text, "Read it.keys Then go.");
        let label = shown
            .looks
            .iter()
            .find(|(r, _)| &shown.text[r.clone()] == "keys")
            .expect("the label has its own look")
            .1;
        assert!(
            label.sup && label.link,
            "raised, in the link colour: {label:?}"
        );
        assert!(
            shown.looks.iter().filter(|(_, l)| l.sup).count() == 1,
            "nothing else is raised"
        );
        // The caret beside it, not in it
        assert_eq!(shown_at(text, 0, Some(0)).text, "Read it.keys Then go.");
        // In it, and at its ends: the markup comes back
        for caret in [at, at + 2, at + 4, at + "[^keys]".len()] {
            let editing = shown_at(text, 0, Some(caret));
            assert_eq!(editing.text, "Read it.[^keys] Then go.", "caret {caret}");
            let marks: Vec<&str> = editing
                .looks
                .iter()
                .filter(|(_, l)| l.markup)
                .map(|(r, _)| &editing.text[r.clone()])
                .collect();
            assert_eq!(marks, ["[^", "]"], "caret {caret}");
        }
        // A link is still a link and not raised
        let link = "See [notes](https://example.org).";
        let shown = shown_at(link, 0, None);
        let look = shown
            .looks
            .iter()
            .find(|(r, _)| &shown.text[r.clone()] == "notes")
            .unwrap()
            .1;
        assert!(look.link && !look.sup);
    }
}
