// SPDX-License-Identifier: GPL-3.0-only

//! Maths recognition: the ways people and agents write LaTeX, made into
//! the `$…$` and `$$…$$` that pulldown-cmark reads, with every change
//! recorded so that each offset of the rewritten text maps back to the
//! bytes the author wrote.
//!
//! - `\(…\)` becomes `$…$`; `\[…\]` becomes `$$…$$`.
//! - A bare display environment (`\begin{cases}` and 25 others) on lines
//!   of its own is wrapped in `$$…$$`.
//! - A display formula's lines are joined into one line, each line break
//!   replaced by a marker absent from the text. pulldown-cmark finds
//!   `$$…$$` inside a block after it has found the blocks, so otherwise a
//!   formula's lines are read as Markdown first: a line of `=` makes a
//!   heading of the paragraph above it, `- x` starts a list (Terminal PR
//!   145). The marker stands for the line break and for the quotation marks
//!   and indent of the line after it, which are not the formula's; those
//!   are recorded as [`Rewrite::hidden`].
//!
//! Code (fences and code spans) is left alone. The recognition also says
//! which lines start inside something ([`Recognised::held`]): a code fence,
//! a formula, or the lines a formula's opening mark had to look through to
//! learn whether it is one. A line starting outside all of them reads the
//! same whatever text came before it, which is what the incremental
//! analysis ([`crate::blocks`]) relies on.

use std::ops::Range;

/// The environments that are display maths on lines of their own
pub const ENVIRONMENTS: [&str; 26] = [
    "align",
    "align*",
    "alignat",
    "alignat*",
    "aligned",
    "alignedat",
    "array",
    "Bmatrix",
    "bmatrix",
    "cases",
    "dcases",
    "eqnarray",
    "eqnarray*",
    "equation",
    "equation*",
    "gather",
    "gather*",
    "gathered",
    "matrix",
    "multline",
    "multline*",
    "pmatrix",
    "rcases",
    "split",
    "Vmatrix",
    "vmatrix",
];

/// Where a rewritten piece of text came from: `out` in the rewritten text
/// stands for `src` in the original
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rewrite {
    pub src: Range<usize>,
    pub out: Range<usize>,
    /// Part of `src` that is not content: the quotation marks and indent of
    /// a formula's line, joined to the line before it
    pub hidden: Option<Range<usize>>,
}

/// A text as Markdown should read it, and how it maps back
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recognised {
    /// The rewritten text
    pub text: String,
    /// Every change, in order of both texts
    pub rewrites: Vec<Rewrite>,
    /// Lines of the original that start inside something, as whole lines
    pub held: Vec<Range<usize>>,
    /// What stands for a formula's line break in [`Self::text`]
    pub marker: String,
}

impl Recognised {
    /// The map from the rewritten text's offsets to the original's
    pub fn map(&self) -> OffsetMap<'_> {
        OffsetMap::new(&self.rewrites)
    }

    /// `s`, a part of the rewritten text, with the formula line breaks put
    /// back, if it holds any
    pub fn with_breaks(&self, s: &str) -> String {
        if self.marker.is_empty() || !s.contains(&self.marker) {
            return s.to_string();
        }
        s.replace(&self.marker, "\n")
    }
}

/// Maps offsets into a rewritten text back to the original
pub struct OffsetMap<'a> {
    rewrites: &'a [Rewrite],
}

impl<'a> OffsetMap<'a> {
    pub fn new(rewrites: &'a [Rewrite]) -> Self {
        OffsetMap { rewrites }
    }

    /// The original offset of rewritten offset `at`. Inside a rewrite, its
    /// start; at its end, the end of what it replaced
    pub fn source(&self, at: usize) -> usize {
        // The last rewrite starting at or before `at`
        let i = self.rewrites.partition_point(|r| r.out.start <= at);
        let Some(r) = i.checked_sub(1).map(|i| &self.rewrites[i]) else {
            return at;
        };
        if at >= r.out.end {
            r.src.end + (at - r.out.end)
        } else {
            r.src.start
        }
    }

    pub fn range(&self, range: &Range<usize>) -> Range<usize> {
        let start = self.source(range.start);
        start..self.source(range.end).max(start)
    }
}

/// A line-break marker absent from `text`. Private-use characters are
/// valid text, so no fixed character is safe; the two different ends keep
/// an inserted marker from overlapping literal text next to it
pub fn break_marker(text: &str) -> String {
    let used: std::collections::HashSet<usize> = text
        .split('\u{e000}')
        .skip(1)
        .filter_map(|part| part.split_once('\u{e001}')?.0.parse().ok())
        .collect();
    let n = (0..=used.len()).find(|n| !used.contains(n)).unwrap_or(0);
    format!("\u{e000}{n}\u{e001}")
}

/// What ends a display formula
#[derive(Clone, Copy)]
enum Close<'a> {
    Dollars,
    Bracket,
    /// `\end{…}` of this environment, which is part of the formula
    Environment(&'a str),
}

/// How far a display formula goes
enum Reach {
    /// To its closing mark: where that starts, and where it ends
    To(usize, usize),
    /// It is not one: something no formula holds comes before any end, on
    /// the line starting at this offset
    Nowhere(usize),
    /// The text ends first, so more of it could yet close it
    NotYet,
}

/// The range of `close`'s mark in `line`, if it has one; `depth` counts the
/// same environment opened inside this one
fn closing_mark(line: &str, close: Close<'_>, depth: &mut usize) -> Option<Range<usize>> {
    let (begin, end) = match close {
        Close::Environment(name) => (format!("\\begin{{{name}}}"), format!("\\end{{{name}}}")),
        _ => (String::new(), String::new()),
    };
    let bytes = line.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        let rest = &bytes[at..];
        match close {
            Close::Dollars if rest.starts_with(b"$$") => return Some(at..at + 2),
            Close::Bracket if rest.starts_with(b"\\]") => return Some(at..at + 2),
            Close::Environment(_) if rest.starts_with(end.as_bytes()) => {
                if *depth == 0 {
                    return Some(at..at + end.len());
                }
                *depth -= 1;
            }
            Close::Environment(_) if rest.starts_with(begin.as_bytes()) => *depth += 1,
            _ => {}
        }
        // A backslash escapes what follows it
        at += if bytes[at] == b'\\' { 2 } else { 1 };
    }
    None
}

/// `line` less its first `depth` quote marks, or `None` if it has fewer
fn unquoted(line: &str, depth: usize) -> Option<&str> {
    let mut rest = line;
    for _ in 0..depth {
        rest = rest.trim_start().strip_prefix('>')?;
        rest = rest.strip_prefix(' ').unwrap_or(rest);
    }
    Some(rest)
}

/// How many quote marks open `line`
fn quote_depth(line: &str) -> usize {
    line.chars()
        .take_while(|c| matches!(c, ' ' | '\t' | '>'))
        .filter(|c| *c == '>')
        .count()
}

fn is_fence(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("```") || line.starts_with("~~~")
}

/// The end of the line that `at` is in, past its line break
fn line_end(text: &str, at: usize) -> usize {
    text[at..]
        .find('\n')
        .map_or(text.len(), |line| at + line + 1)
}

/// How far the display formula whose text starts at `from` goes, it being
/// in `quote` levels of quotation. A blank line ends the search unless
/// `blanks`; a code fence, or the end of the quotation, always does
fn reach(text: &str, from: usize, close: Close<'_>, quote: usize, blanks: bool) -> Reach {
    let mut depth = 0;
    let mut at = from;
    while at < text.len() {
        let end = line_end(text, at);
        let line = &text[at..end];
        let body = if at == from {
            line
        } else {
            match unquoted(line, quote) {
                Some(body) if !is_fence(body) && (blanks || !body.trim().is_empty()) => body,
                _ => return Reach::Nowhere(at),
            }
        };
        if let Some(mark) = closing_mark(body, close, &mut depth) {
            let offset = at + line.len() - body.len();
            return Reach::To(offset + mark.start, offset + mark.end);
        }
        at = end;
    }
    Reach::NotYet
}

/// The rewriting under way
struct Out<'t> {
    text: &'t str,
    out: String,
    rewrites: Vec<Rewrite>,
    held: Vec<Range<usize>>,
    marker: String,
}

impl Out<'_> {
    /// `with` in place of `src`
    fn replace(&mut self, src: Range<usize>, with: &str, hidden: Option<Range<usize>>) {
        if &self.text[src.clone()] == with && hidden.is_none() {
            self.out.push_str(with);
            return;
        }
        let start = self.out.len();
        self.out.push_str(with);
        self.rewrites.push(Rewrite {
            src,
            out: start..self.out.len(),
            hidden: hidden.filter(|h| !h.is_empty()),
        });
    }

    /// The lines starting in `range` (from a line's start) held
    fn hold(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        match self.held.last_mut() {
            Some(h) if h.end >= range.start => h.end = h.end.max(range.end),
            _ => self.held.push(range),
        }
    }

    /// The display formula `body`, opened by `open` and closed by `close`,
    /// as Markdown should see it: one line, between dollars. `quote` levels
    /// of quotation and `indent` spaces, which its first line comes after,
    /// are not part of the lines after it
    fn formula(
        &mut self,
        open: Range<usize>,
        body: Range<usize>,
        close: Range<usize>,
        quote: usize,
        indent: usize,
    ) {
        let text = self.text;
        self.replace(open, "$$", None);
        let mut kept_end = body.start;
        let mut line_start = body.start;
        let mut first = true;
        loop {
            let line_stop = text[line_start..body.end]
                .find('\n')
                .map_or(body.end, |i| line_start + i);
            let mut seg = line_start..line_stop;
            if text[seg.clone()].ends_with('\r') {
                seg.end -= 1;
            }
            if !first {
                let line = &text[seg.clone()];
                let rest = unquoted(line, quote).unwrap_or(line);
                let spaces = rest.len() - rest.trim_start_matches(' ').len();
                let skip = line.len() - rest.len() + spaces.min(indent);
                let content = seg.start + skip;
                let marker = self.marker.clone();
                self.replace(kept_end..content, &marker, Some(seg.start..content));
                seg.start = content;
            }
            self.out.push_str(&text[seg.clone()]);
            kept_end = seg.end;
            first = false;
            if line_stop >= body.end {
                break;
            }
            line_start = line_stop + 1;
        }
        self.replace(kept_end..close.end, "$$", None);
    }
}

/// Rewrite `\(…\)` as `$…$`, and `\[…\]`, `$$…$$` and bare display
/// environments such as `\begin{align}` as `$$…$$` on one line, outside
/// code; with each rewrite and the lines that start inside something
pub fn recognise(text: &str) -> Recognised {
    let mut o = Out {
        text,
        out: String::with_capacity(text.len() + 16),
        rewrites: Vec::new(),
        held: Vec::new(),
        marker: break_marker(text),
    };
    // The open fence, and the quotation it is in
    let mut in_fence: Option<(String, usize)> = None;
    // A display formula with no end yet: what follows may yet be inside it
    let mut in_display = false;
    // Between single dollars, in this paragraph
    let mut in_inline = false;
    // A formula was opened that nothing in the text closes: everything
    // after it could be read otherwise once more text comes, so all of it
    // is held, whatever closes `in_display` (a stray `\]` does)
    let mut open_to_end = false;
    let mut at = 0;
    while at < text.len() {
        let mut end = line_end(text, at);
        if in_fence.is_some() || in_display || open_to_end {
            o.hold(at..end);
        }
        let line = &text[at..end];
        let quote = quote_depth(line);
        let trimmed = line.trim_start_matches([' ', '\t', '>']);
        if let Some((fence, depth)) = &in_fence {
            if quote >= *depth {
                if trimmed.starts_with(fence.as_str()) {
                    in_fence = None;
                }
                o.out.push_str(line);
                at = end;
                continue;
            }
            in_fence = None;
        }
        if is_fence(trimmed) {
            let fence: String = trimmed
                .chars()
                .take_while(|c| *c == '`' || *c == '~')
                .collect();
            in_fence = Some((fence, quote));
            o.out.push_str(line);
            at = end;
            continue;
        }
        if trimmed.trim_end().is_empty() {
            in_inline = false;
        }
        let mut i = at;
        // How far in the line's text starts, past its quotation
        let margin = |i: usize| {
            let text = unquoted(line, quote).unwrap_or(line);
            let indent = text.len() - text.trim_start_matches(' ').len();
            i.saturating_sub(at).min(indent)
        };
        // Environments that start their lines
        let environment = trimmed
            .strip_prefix("\\begin{")
            .and_then(|name| name.split('}').next())
            .filter(|name| ENVIRONMENTS.contains(name));
        if let Some(env) = environment
            && !in_display
            && !in_inline
        {
            let from = end - trimmed.len();
            match reach(
                text,
                from + "\\begin{}".len() + env.len(),
                Close::Environment(env),
                quote,
                true,
            ) {
                Reach::To(_, stop) => {
                    o.out.push_str(&text[at..from]);
                    o.formula(from..from, from..stop, stop..stop, quote, margin(from));
                    i = stop;
                    end = line_end(text, stop);
                    o.hold(line_end(text, at)..end);
                }
                Reach::Nowhere(line) => o.hold(line_end(text, at)..line_end(text, line)),
                Reach::NotYet => (in_display, open_to_end) = (true, true),
            }
        }
        let mut in_code = 0usize;
        while i < end {
            let Some(c) = text[i..].chars().next() else {
                break;
            };
            let next = text[i + c.len_utf8()..]
                .chars()
                .next()
                .filter(|_| i + c.len_utf8() < end);
            if c == '`' {
                let run = text[i..end].len() - text[i..end].trim_start_matches('`').len();
                if in_code == 0 {
                    in_code = run;
                } else if in_code == run {
                    in_code = 0;
                }
                o.out.push_str(&text[i..i + run]);
                i += run;
                continue;
            }
            if in_code == 0 {
                // A display formula, ending here or further on. A `$$` alone
                // at its line's end is one, even past a blank line; with text
                // after it, it could be a stray pair of dollars
                let opens = match (c, next) {
                    ('$', Some('$')) if !in_inline => Some(Close::Dollars),
                    ('\\', Some('[')) => Some(Close::Bracket),
                    _ => None,
                };
                if let Some(close) = opens {
                    let body = i + 2;
                    let blanks =
                        matches!(close, Close::Bracket) || text[body..end].trim().is_empty();
                    match reach(text, body, close, quote, blanks) {
                        Reach::To(mark, stop) => {
                            let line_end_before = line_end(text, i);
                            o.formula(i..body, body..mark, mark..stop, quote, margin(i));
                            i = stop;
                            end = line_end(text, stop);
                            o.hold(line_end_before..end);
                            continue;
                        }
                        Reach::Nowhere(line) => o.hold(line_end(text, i)..line_end(text, line)),
                        Reach::NotYet => (in_display, open_to_end) = (true, true),
                    }
                    o.replace(i..body, "$$", None);
                    i = body;
                    continue;
                }
                if c == '$' {
                    in_inline = !in_inline;
                }
                if c == '\\' {
                    match next {
                        Some('(') | Some(')') => {
                            o.replace(i..i + 2, "$", None);
                            in_inline = next == Some('(');
                            i += 2;
                            continue;
                        }
                        Some(']') if in_display => {
                            o.replace(i..i + 2, "$$", None);
                            in_display = false;
                            i += 2;
                            continue;
                        }
                        Some(next) => {
                            o.out.push_str(&text[i..i + 1 + next.len_utf8()]);
                            i += 1 + next.len_utf8();
                            continue;
                        }
                        None => {}
                    }
                }
            }
            o.out.push(c);
            i += c.len_utf8();
        }
        at = end;
    }
    Recognised {
        text: o.out,
        rewrites: o.rewrites,
        held: o.held,
        marker: o.marker,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rewritten text, with the markers shown as `⏎`
    fn shown(r: &Recognised) -> String {
        r.text.replace(&r.marker, "⏎")
    }

    #[test]
    fn delimiters_are_rewritten_and_map_back() {
        let text = "a \\(x\\) b\n\\[y\\]\n\\begin{align}\nz\n\\end{align}\nc\n";
        let r = recognise(text);
        assert_eq!(
            shown(&r),
            "a $x$ b\n$$y$$\n$$\\begin{align}⏎z⏎\\end{align}$$\nc\n"
        );
        let map = r.map();
        assert_eq!(&text[map.range(&(3..4))], "x");
        assert_eq!(&text[map.range(&(2..5))], "\\(x\\)");
        assert_eq!(&text[map.source(5)..map.source(7)], " b");
        let env = r.text.find("$$\\begin").unwrap()..r.text.rfind("$$").unwrap() + 2;
        assert_eq!(&text[map.range(&env)], "\\begin{align}\nz\n\\end{align}");
        assert_eq!(&text[map.source(r.text.len() - 2)..], "c\n");
    }

    #[test]
    fn a_formulas_lines_are_one_line_with_their_prefixes_hidden() {
        let text = "> $$\n> a\n> =\n> b\n> $$\n";
        let r = recognise(text);
        assert_eq!(shown(&r), "> $$⏎a⏎=⏎b⏎$$\n");
        let hidden: Vec<&str> = r
            .rewrites
            .iter()
            .filter_map(|w| w.hidden.clone())
            .map(|h| &text[h])
            .collect();
        assert_eq!(hidden, ["> ", "> ", "> ", "> "]);
        // The whole formula maps back to its source, delimiters and all
        let map = r.map();
        assert_eq!(
            &text[map.range(&(2..r.text.len() - 1))],
            "$$\n> a\n> =\n> b\n> $$"
        );
        // Every line after the first starts inside it
        assert_eq!(r.held, vec![5..text.len()]);
        assert_eq!(r.with_breaks(&r.text[4..r.text.len() - 3]), "\na\n=\nb\n");
    }

    #[test]
    fn code_is_left_alone_and_held() {
        let text = "```\n\\(x\\)\n$$\na\n$$\n```\nAfter \\(y\\).\n";
        let r = recognise(text);
        assert_eq!(r.text, "```\n\\(x\\)\n$$\na\n$$\n```\nAfter $y$.\n");
        let after = text.find("After").unwrap();
        assert_eq!(r.held, vec![4..after]);
    }

    #[test]
    fn a_formula_not_closed_yet_holds_the_rest() {
        let text = "Text\n\n\\[\nx\n\nmore\n";
        let r = recognise(text);
        assert_eq!(r.held, vec![9..text.len()]);
    }

    /// What a formula's opening mark looked through to learn it was none
    /// is held: removing what stopped it would change the mark's reading
    #[test]
    fn what_an_opening_mark_looked_through_is_held() {
        let text = "\\[ a\n\nb\n\n```\nc\n```\n";
        let r = recognise(text);
        let fence = text.find("```").unwrap();
        assert_eq!(r.held[0].start, 5, "{:?}", r.held);
        assert!(r.held[0].contains(&fence));
    }

    #[test]
    fn the_marker_is_absent_from_the_text() {
        assert_eq!(break_marker("plain"), "\u{e000}0\u{e001}");
        assert_eq!(break_marker("\u{e000}0\u{e001}"), "\u{e000}1\u{e001}");
    }
}
