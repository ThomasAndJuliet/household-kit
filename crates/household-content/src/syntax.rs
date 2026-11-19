// SPDX-License-Identifier: GPL-3.0-only

//! What a Markdown text is made of, by source range: the *leaves* that are
//! laid out (paragraphs, headings, code blocks, tables, …), the containers
//! they sit in (quotes, list items), and the inline elements in them.
//!
//! Everything here is in byte offsets into the text, which stays the only
//! document. pulldown-cmark gives every construct its range; an inline
//! element's *markers* are its range less its children's, so every syntax
//! (`**a**`, `[a](b)`, `<a>`, `[a][b]`) is handled alike. See
//! Cerea's `docs/architecture.md`.
//!
//! Maths is recognised first ([`crate::maths`]): the text is parsed with
//! its formulas rewritten and every range mapped back to the text. A
//! display formula on lines of its own is a leaf of its own, even inside a
//! paragraph.

use std::ops::Range;

use pulldown_cmark::{
    Alignment, BrokenLink, CodeBlockKind, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};

use crate::Dialect;

/// The options a document is parsed with ([`Dialect::Document`]). Not
/// pulldown-cmark's YAML metadata blocks, which it reads anywhere in a
/// text: front matter is only a block at the very start of a document,
/// found by [`front_matter_end`]
pub fn options() -> Options {
    Dialect::Document.options()
}

/// Where a document's front matter ends, if it starts with some: the byte
/// after its closing line. Front matter is only a block at the very start
/// of a document, read as pulldown-cmark's YAML metadata scanner reads one:
/// a `---` line (exactly three dashes, then only white space), a next line
/// neither blank nor closing, and a closing `---` or `...` line after it.
/// The same lines further on are not front matter
pub fn front_matter_end(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let line_end = |at: usize| {
        bytes[at..]
            .iter()
            .position(|b| *b == b'\n')
            .map_or(bytes.len(), |i| at + i)
    };
    let first = line_end(0);
    let line = &bytes[..first];
    if !line.starts_with(b"---")
        || line.get(3) == Some(&b'-')
        || !line[3..].iter().all(|b| b.is_ascii_whitespace())
    {
        return None;
    }
    let next = bytes.get(first + 1..).filter(|n| !n.is_empty())?;
    let blank = next
        .iter()
        .position(|b| !matches!(b, b'\t' | 0x0b | 0x0c | b' '))
        .is_none_or(|i| matches!(next[i], b'\n' | b'\r'));
    if blank || is_metadata_closer(next) {
        return None;
    }
    let mut at = line_end(first + 1);
    while at < bytes.len() {
        at += 1;
        if is_metadata_closer(&bytes[at..]) {
            return Some((line_end(at) + 1).min(bytes.len()));
        }
        at = line_end(at);
    }
    None
}

/// Whether `line` (and what follows it) starts with a line closing front
/// matter, as pulldown-cmark's scanner reads one: `---` or `...`, spaces,
/// then a line's end (`\n`, `\r`) or the text's
pub fn is_metadata_closer(line: &[u8]) -> bool {
    if !(line.starts_with(b"---") || line.starts_with(b"...")) || line.get(3) == Some(&line[0]) {
        return false;
    }
    let after = line[3..]
        .iter()
        .position(|b| *b != b' ')
        .map_or(line.len(), |i| i + 3);
    matches!(line.get(after), None | Some(b'\n' | b'\r'))
}

/// `text` with `range` (its front matter) blanked: each of its characters
/// but the line breaks a space, so that a parse reads on after it as after
/// front matter, its blank lines making nothing, and every offset holds
pub fn blank(text: &str, range: Range<usize>) -> std::borrow::Cow<'_, str> {
    if range.is_empty() {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..range.start]);
    out.extend(
        text[range.clone()]
            .bytes()
            .map(|b| if b == b'\n' { '\n' } else { ' ' }),
    );
    out.push_str(&text[range.end..]);
    std::borrow::Cow::Owned(out)
}

/// `text` (a whole document) with its front matter, if it starts with
/// some, blanked ([`blank`]): for a parse with [`options`] that leaves it out
pub fn without_front_matter(text: &str) -> std::borrow::Cow<'_, str> {
    blank(text, 0..front_matter_end(text).unwrap_or(0))
}

/// The parts of a text
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Analysis {
    /// In order, covering every line of the text once
    pub leaves: Vec<Leaf>,
    /// Quotes and list items, outermost first where they nest
    pub containers: Vec<Container>,
}

/// A unit that is laid out on its own: whole source lines
#[derive(Clone, Debug, PartialEq)]
pub struct Leaf {
    /// From the start of its first line to the end of its last, without the
    /// last line's newline
    pub range: Range<usize>,
    pub kind: LeafKind,
    /// Container prefixes (`> `, `- `, indentation): never shown
    pub prefixes: Vec<Range<usize>>,
    /// Markup shown only while the caret is within `reveal`
    pub marks: Vec<Mark>,
    /// Inline elements, in order of their start
    pub inlines: Vec<Inline>,
    /// Indices into [`Analysis::containers`] of the containers it is in,
    /// outermost first
    pub containers: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LeafKind {
    Paragraph,
    Heading(u8),
    Code {
        lang: String,
    },
    /// Display maths on lines of its own: a paragraph that is only display
    /// maths, or the lines of one that a display formula takes up
    Math {
        latex: String,
    },
    /// A paragraph that is only an image
    Image {
        url: String,
        alt: String,
    },
    Table(Table),
    Rule,
    Html,
    FrontMatter,
    /// A line no other leaf owns: blank, or only container markers
    Spacer,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub alignments: Vec<Alignment>,
    /// Each row's cells' ranges; the first row is the head
    pub rows: Vec<Vec<Range<usize>>>,
}

/// Markup hidden unless the caret is within `reveal`
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub range: Range<usize>,
    pub reveal: Range<usize>,
}

/// An inline element
#[derive(Clone, Debug, PartialEq)]
pub struct Inline {
    pub range: Range<usize>,
    pub kind: InlineKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InlineKind {
    Emphasis,
    Strong,
    Strikethrough,
    Code,
    Link {
        url: String,
    },
    Image {
        url: String,
    },
    /// A formula: its LaTeX (line breaks as written, without the quotation
    /// marks and indent of the lines after its first), and whether it is
    /// display maths
    Math {
        latex: String,
        display: bool,
    },
    /// An escaped character, `\*`
    Escape,
    /// Text written differently from how it reads: `&amp;`
    Entity {
        text: String,
    },
    HardBreak,
    Html,
    Footnote,
    Task {
        checked: bool,
    },
    /// `<u>…</u>`: Markdown has no underline of its own
    Underline,
    /// `==…==` (the highlight extension), or `<mark>…</mark>`
    Highlight,
    /// `<sup>…</sup>` and `<sub>…</sub>`
    Superscript,
    Subscript,
}

impl InlineKind {
    /// Whether the element is shown as something else while the caret is
    /// outside it, rather than as its text less its markers
    pub fn is_replaced(&self) -> bool {
        matches!(
            self,
            Self::Math { .. } | Self::Entity { .. } | Self::Task { .. }
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Container {
    pub range: Range<usize>,
    pub kind: ContainerKind,
    /// How deep it is: 0 for the outermost
    pub depth: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ContainerKind {
    Quote,
    /// A list item: its number if the list is ordered, and the offset of the
    /// first leaf in it, which shows the bullet
    Item {
        number: Option<u64>,
        tight: bool,
        first_leaf: Option<usize>,
    },
    Footnote {
        label: String,
    },
}

/// The start of the line `at` is on
pub fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |i| i + 1)
}

/// The end of the line `at` is on, before its newline
pub fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |i| at + i)
}

/// A leaf being gathered
struct Open {
    kind: LeafKind,
    /// The block's own range, from pulldown-cmark
    block: Range<usize>,
    /// Inline events' ranges, at the leaf's top level, and every inline
    /// element's range with its children's
    covered: Vec<Range<usize>>,
    inlines: Vec<Inline>,
    /// Open inline elements: index into `inlines`, children's ranges
    stack: Vec<(usize, Vec<Range<usize>>)>,
    marks: Vec<Mark>,
    /// Display maths and images seen, and whether anything else was
    only: Option<OnlyOne>,
    others: bool,
    table: Option<Table>,
}

#[derive(Clone)]
enum OnlyOne {
    Math(String),
    Image(String, String, usize),
}

impl Open {
    fn new(kind: LeafKind, block: Range<usize>) -> Self {
        Open {
            kind,
            block,
            covered: Vec::new(),
            inlines: Vec::new(),
            stack: Vec::new(),
            marks: Vec::new(),
            only: None,
            others: false,
            table: None,
        }
    }

    /// An inline event or element at `range`, at the current depth
    fn cover(&mut self, range: Range<usize>) {
        match self.stack.last_mut() {
            Some((_, children)) => children.push(range),
            None => self.covered.push(range),
        }
    }
}

/// What `text`, a document ([`Dialect::Document`]), is made of. Maths is
/// read as [`crate::maths`] recognises it: `\\(…\\)`, `\\[…\\]` and
/// display environments on lines of their own are maths, as `$…$` and
/// `$$…$$` are, and a display formula's lines are never Markdown. Every
/// range is in `text`.
pub fn analyse(text: &str) -> Analysis {
    analyse_parts(text, &mut |_| None).0
}

/// [`analyse`] in `dialect`
pub fn analyse_in(text: &str, dialect: Dialect) -> Analysis {
    analyse_region_in(text, Some(0), &mut |_| None, dialect).0
}

/// What a parse of a text says besides its [`Analysis`], for the
/// incremental analysis ([`crate::blocks`]): what in it reaches past it,
/// and where it may be cut. Offsets into the text
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Extras {
    /// Link reference definitions, as pulldown-cmark keys them: where,
    /// label, destination and title
    pub defs: Vec<(usize, String, String, String)>,
    /// The labels of the references in the text, where they are: those it
    /// defines and those asked of the caller
    pub refs: Vec<(usize, String)>,
    /// Footnote definitions' labels, where they are
    pub footnotes: Vec<(usize, String)>,
    /// Where each block at the top level starts, with nothing open
    pub tops: Vec<usize>,
    /// A `---` rule at the document's start followed by a line of text:
    /// front matter if a closing `---` or `...` line comes anywhere after
    /// it (the look ahead to the end of the text)
    pub open_rules: Vec<usize>,
    /// Lines the maths rewriting starts in the middle of something (a code
    /// fence, an open `\[`)
    pub held: Vec<Range<usize>>,
}

/// [`analyse`], with [`Extras`]; references to labels the text does not
/// define asked of `refs` (destination, title)
pub fn analyse_parts(
    text: &str,
    refs: &mut dyn FnMut(&str) -> Option<(String, String)>,
) -> (Analysis, Extras) {
    analyse_region(text, Some(0), refs)
}

/// [`analyse_parts`] of a part of a document, which starts in `text` at
/// `start` if `text` holds its start: only there can front matter be.
/// Nothing before `start` may be rewritten as maths
pub fn analyse_region(
    text: &str,
    start: Option<usize>,
    refs: &mut dyn FnMut(&str) -> Option<(String, String)>,
) -> (Analysis, Extras) {
    analyse_region_in(text, start, refs, Dialect::Document)
}

/// [`analyse_region`] in `dialect`. A chat has no front matter, so `start`
/// is only where the text starts
pub fn analyse_region_in(
    text: &str,
    start: Option<usize>,
    refs: &mut dyn FnMut(&str) -> Option<(String, String)>,
    dialect: Dialect,
) -> (Analysis, Extras) {
    let start = start.filter(|_| dialect.front_matter());
    // Front matter is the source's, read before any maths: its lines are
    // left alone, and no formula's lines can hide its closing line
    let front = start.and_then(|at| front_matter_end(&text[at..]).map(|end| at..at + end));
    let mut recognised = crate::maths::recognise(&blank(text, front.clone().unwrap_or(0..0)));
    if let Some(front) = front {
        recognised.text.replace_range(front.clone(), &text[front]);
    }
    let held = recognised.held.clone();
    let mut extras = Extras::default();
    if recognised.rewrites.is_empty() {
        let analysis = analyse_plain(text, start, refs, &mut extras, dialect, &recognised);
        extras.held = held;
        return (analysis, extras);
    }
    let rewritten = recognised.text.as_str();
    let analysis = analyse_plain(rewritten, start, refs, &mut extras, dialect, &recognised);
    let rewrites = &recognised.rewrites;
    let map = recognised.map();
    for at in extras.tops.iter_mut().chain(extras.open_rules.iter_mut()) {
        *at = map.source(*at);
    }
    for at in extras
        .defs
        .iter_mut()
        .map(|d| &mut d.0)
        .chain(extras.refs.iter_mut().map(|r| &mut r.0))
        .chain(extras.footnotes.iter_mut().map(|f| &mut f.0))
    {
        *at = map.source(*at);
    }
    extras.held = held;
    let r = |range: &Range<usize>| map.range(range);
    // The quotation marks and indent of a formula's joined lines are prefixes
    let hidden: Vec<Range<usize>> = rewrites.iter().filter_map(|w| w.hidden.clone()).collect();
    let analysis = Analysis {
        leaves: analysis
            .leaves
            .into_iter()
            .map(|leaf| Leaf {
                range: r(&leaf.range),
                kind: match leaf.kind {
                    LeafKind::Table(table) => LeafKind::Table(Table {
                        alignments: table.alignments,
                        rows: table
                            .rows
                            .iter()
                            .map(|row| row.iter().map(r).collect())
                            .collect(),
                    }),
                    kind => kind,
                },
                prefixes: {
                    let range = r(&leaf.range);
                    let mut prefixes: Vec<Range<usize>> = leaf
                        .prefixes
                        .iter()
                        .map(r)
                        .filter(|p| !p.is_empty())
                        .chain(
                            hidden
                                .iter()
                                .filter(|h| range.start <= h.start && h.end <= range.end)
                                .cloned(),
                        )
                        .collect();
                    prefixes.sort_by_key(|p| p.start);
                    prefixes
                },
                marks: leaf
                    .marks
                    .iter()
                    .map(|m| Mark {
                        range: r(&m.range),
                        reveal: r(&m.reveal),
                    })
                    .filter(|m| !m.range.is_empty())
                    .collect(),
                inlines: leaf
                    .inlines
                    .into_iter()
                    .map(|i| Inline {
                        range: r(&i.range),
                        kind: i.kind,
                    })
                    .collect(),
                containers: leaf.containers,
            })
            .collect(),
        containers: analysis
            .containers
            .into_iter()
            .map(|c| Container {
                range: r(&c.range),
                kind: match c.kind {
                    ContainerKind::Item {
                        number,
                        tight,
                        first_leaf,
                    } => ContainerKind::Item {
                        number,
                        tight,
                        first_leaf: first_leaf.map(|at| map.source(at)),
                    },
                    kind => kind,
                },
                depth: c.depth,
            })
            .collect(),
    };
    (analysis, extras)
}

/// A `---` line at `at`, a document's start, that is front matter's start
/// if a closing line follows it anywhere: exactly three dashes, then only
/// white space, and a next line neither blank nor a closing line. Lines as
/// pulldown-cmark's front matter scanner sees them: up to each `\n`, and a
/// `\r` ends one as well for "blank" and "closing"
fn is_open_rule(text: &str, at: usize) -> bool {
    let rest = &text.as_bytes()[at..];
    let eol = rest.iter().position(|b| *b == b'\n').unwrap_or(rest.len());
    let line = &rest[..eol];
    if !line.starts_with(b"---")
        || line.get(3) == Some(&b'-')
        || !line[3..].iter().all(|b| b.is_ascii_whitespace())
    {
        return false;
    }
    let Some(next) = rest.get(eol + 1..) else {
        return false;
    };
    if next.is_empty() {
        return false;
    }
    let blank = next
        .iter()
        .position(|b| !matches!(b, b'\t' | 0x0b | 0x0c | b' '))
        .is_none_or(|i| matches!(next[i], b'\n' | b'\r'));
    !blank && !is_metadata_closer(next)
}

/// A link or image written as a reference, `[a][b]`, `[a][]`, `[a]`
fn is_reference(link_type: pulldown_cmark::LinkType) -> bool {
    use pulldown_cmark::LinkType as T;
    matches!(
        link_type,
        T::Reference
            | T::ReferenceUnknown
            | T::Collapsed
            | T::CollapsedUnknown
            | T::Shortcut
            | T::ShortcutUnknown
    )
}

fn analyse_plain(
    text: &str,
    start: Option<usize>,
    refs: &mut dyn FnMut(&str) -> Option<(String, String)>,
    extras: &mut Extras,
    dialect: Dialect,
    recognised: &crate::maths::Recognised,
) -> Analysis {
    // A formula's LaTeX with the line breaks the recognition joined put
    // back: only where its source holds a marker the recognition put
    // there, as a character reference can decode to the marker too
    let latex_of = |latex: &str, range: &Range<usize>| -> String {
        if text[range.clone()].contains(&recognised.marker) {
            recognised.with_breaks(latex)
        } else {
            latex.to_string()
        }
    };
    let mut analysis = Analysis::default();
    let mut leaves: Vec<Leaf> = Vec::new();
    // Open containers: index into analysis.containers
    let mut containers: Vec<usize> = Vec::new();
    // Each list's next number and tightness
    let mut lists: Vec<(Option<u64>, bool)> = Vec::new();
    let mut open: Option<Open> = None;
    // Items whose content is not in a paragraph (tight lists) open a leaf
    // at their first inline event
    let mut item_text = false;

    let close =
        |open: &mut Option<Open>, leaves: &mut Vec<Leaf>, containers: &[usize], end: usize| {
            if let Some(o) = open.take() {
                leaves.push(finish(text, o, containers.to_vec(), end));
            }
        };

    let asked = std::cell::RefCell::new(Vec::new());
    let mut callback = |link: BrokenLink<'_>| {
        asked
            .borrow_mut()
            .push((link.span.start, link.reference.to_string()));
        refs(&link.reference).map(|(dest, title)| (CowStr::from(dest), CowStr::from(title)))
    };
    // Front matter, only at the document's start: its events are
    // pulldown-cmark's for it alone, read with its YAML metadata blocks, and
    // the rest is parsed with it blanked, which reads on after it as after
    // front matter, offsets unchanged
    let front = start.and_then(|at| front_matter_end(&text[at..]).map(|end| at..at + end));
    let front_events: Vec<(Event<'_>, Range<usize>)> = front.as_ref().map_or_else(Vec::new, |f| {
        Parser::new_ext(
            &text[f.clone()],
            dialect.options() | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS,
        )
        .into_offset_iter()
        .map(|(event, range)| (event, range.start + f.start..range.end + f.start))
        .collect()
    });
    let blanked = blank(text, front.clone().unwrap_or(0..0));
    let mut parser =
        Parser::new_with_broken_link_callback(&blanked, dialect.options(), Some(&mut callback))
            .into_offset_iter();
    // How many tags are open: a block starting with none is at the top
    let mut depth = 0usize;
    let mut front_events = front_events.into_iter().peekable();
    let mut rest = parser.by_ref().peekable();
    let events = std::iter::from_fn(move || {
        let front_next = match (front_events.peek(), rest.peek()) {
            (Some((_, f)), Some((_, r))) => f.start <= r.start,
            (f, _) => f.is_some(),
        };
        if front_next {
            front_events.next()
        } else {
            rest.next()
        }
    });
    for (event, range) in events {
        match &event {
            Event::Start(tag) => {
                if depth == 0 {
                    extras.tops.push(range.start);
                }
                match tag {
                    Tag::FootnoteDefinition(label) => {
                        extras.footnotes.push((range.start, label.to_string()))
                    }
                    Tag::Link { link_type, id, .. } | Tag::Image { link_type, id, .. }
                        if !id.is_empty() && is_reference(*link_type) =>
                    {
                        extras.refs.push((range.start, id.to_string()));
                    }
                    _ => {}
                }
                depth += 1;
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::Rule => {
                if depth == 0 {
                    extras.tops.push(range.start);
                }
                if start == Some(range.start) && is_open_rule(text, range.start) {
                    extras.open_rules.push(range.start);
                }
            }
            _ => {}
        }
        match event {
            Event::Start(Tag::BlockQuote(_)) => {
                close(&mut open, &mut leaves, &containers, range.start);
                containers.push(analysis.containers.len());
                analysis.containers.push(Container {
                    range,
                    kind: ContainerKind::Quote,
                    depth: containers.len() - 1,
                });
            }
            Event::Start(Tag::FootnoteDefinition(label)) => {
                close(&mut open, &mut leaves, &containers, range.start);
                containers.push(analysis.containers.len());
                analysis.containers.push(Container {
                    range,
                    kind: ContainerKind::Footnote {
                        label: label.to_string(),
                    },
                    depth: containers.len() - 1,
                });
            }
            Event::Start(Tag::List(start)) => {
                close(&mut open, &mut leaves, &containers, range.start);
                lists.push((start, true));
            }
            Event::End(TagEnd::List(_)) => {
                close(&mut open, &mut leaves, &containers, range.end);
                lists.pop();
            }
            Event::Start(Tag::Item) => {
                close(&mut open, &mut leaves, &containers, range.start);
                let number = lists.last_mut().and_then(|(next, _)| {
                    let number = *next;
                    if let Some(n) = next {
                        *n += 1;
                    }
                    number
                });
                containers.push(analysis.containers.len());
                analysis.containers.push(Container {
                    range,
                    kind: ContainerKind::Item {
                        number,
                        tight: true,
                        first_leaf: None,
                    },
                    depth: containers.len() - 1,
                });
                item_text = true;
            }
            Event::End(TagEnd::BlockQuote(_) | TagEnd::Item | TagEnd::FootnoteDefinition) => {
                close(&mut open, &mut leaves, &containers, range.end);
                containers.pop();
                item_text = false;
            }
            Event::Start(Tag::Paragraph) => {
                close(&mut open, &mut leaves, &containers, range.start);
                item_text = false;
                // A paragraph in a list item makes it loose
                if let Some(&c) = containers.last()
                    && let ContainerKind::Item { tight, .. } = &mut analysis.containers[c].kind
                {
                    *tight = false;
                }
                open = Some(Open::new(LeafKind::Paragraph, range));
            }
            Event::Start(Tag::Heading { level, .. }) => {
                close(&mut open, &mut leaves, &containers, range.start);
                item_text = false;
                open = Some(Open::new(LeafKind::Heading(heading_level(level)), range));
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                close(&mut open, &mut leaves, &containers, range.start);
                item_text = false;
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                open = Some(Open::new(LeafKind::Code { lang }, range));
            }
            Event::Start(Tag::HtmlBlock) => {
                close(&mut open, &mut leaves, &containers, range.start);
                item_text = false;
                open = Some(Open::new(LeafKind::Html, range));
            }
            Event::Start(Tag::MetadataBlock(_)) => {
                close(&mut open, &mut leaves, &containers, range.start);
                open = Some(Open::new(LeafKind::FrontMatter, range));
            }
            Event::Start(Tag::Table(alignments)) => {
                close(&mut open, &mut leaves, &containers, range.start);
                item_text = false;
                let mut o = Open::new(LeafKind::Paragraph, range);
                o.table = Some(Table {
                    alignments,
                    rows: Vec::new(),
                });
                open = Some(o);
            }
            Event::Start(Tag::TableHead | Tag::TableRow) => {
                if let Some(table) = open.as_mut().and_then(|o| o.table.as_mut()) {
                    table.rows.push(Vec::new());
                }
            }
            Event::Start(Tag::TableCell) => {
                if let Some(o) = open.as_mut()
                    && let Some(row) = o.table.as_mut().and_then(|t| t.rows.last_mut())
                {
                    row.push(range.clone());
                    o.covered.push(range);
                }
            }
            Event::Rule => {
                close(&mut open, &mut leaves, &containers, range.start);
                item_text = false;
                let mut o = Open::new(LeafKind::Rule, range.clone());
                let end = range.end - usize::from(text[..range.end].ends_with('\n'));
                o.marks.push(Mark {
                    range: range.start..end,
                    reveal: range.start..end,
                });
                leaves.push(finish(text, o, containers.clone(), end));
            }
            Event::End(
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::CodeBlock
                | TagEnd::HtmlBlock
                | TagEnd::MetadataBlock(_)
                | TagEnd::Table,
            ) => {
                close(&mut open, &mut leaves, &containers, range.end);
            }
            Event::Start(
                tag @ (Tag::Emphasis
                | Tag::Strong
                | Tag::Strikethrough
                | Tag::Link { .. }
                | Tag::Image { .. }
                | Tag::Superscript
                | Tag::Subscript),
            ) => {
                let o = open_inline(&mut open, &mut item_text, &range);
                let kind = match tag {
                    Tag::Emphasis => InlineKind::Emphasis,
                    Tag::Strong => InlineKind::Strong,
                    Tag::Strikethrough => InlineKind::Strikethrough,
                    Tag::Link { dest_url, .. } => InlineKind::Link {
                        url: dest_url.to_string(),
                    },
                    Tag::Image { dest_url, .. } => {
                        if o.stack.is_empty() {
                            if o.only.is_none() && !o.others {
                                o.only = Some(OnlyOne::Image(
                                    dest_url.to_string(),
                                    String::new(),
                                    range.start,
                                ));
                            } else {
                                o.others = true;
                            }
                        }
                        InlineKind::Image {
                            url: dest_url.to_string(),
                        }
                    }
                    _ => InlineKind::Html,
                };
                if !matches!(kind, InlineKind::Image { .. }) && o.stack.is_empty() {
                    o.others = true;
                }
                o.cover(range.clone());
                o.inlines.push(Inline { range, kind });
                o.stack.push((o.inlines.len() - 1, Vec::new()));
            }
            Event::End(
                TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Link
                | TagEnd::Image
                | TagEnd::Superscript
                | TagEnd::Subscript,
            ) => {
                if let Some(o) = open.as_mut()
                    && let Some((index, children)) = o.stack.pop()
                {
                    let element = o.inlines[index].range.clone();
                    for gap in gaps(&element, &children) {
                        o.marks.push(Mark {
                            range: gap,
                            reveal: element.clone(),
                        });
                    }
                    if let InlineKind::Image { .. } = o.inlines[index].kind
                        && let Some(OnlyOne::Image(_, alt, start)) = &mut o.only
                        && *start == element.start
                    {
                        // Its text less the markup in it
                        let mut hidden: Vec<Range<usize>> = o
                            .marks
                            .iter()
                            .filter(|m| {
                                element.start <= m.range.start && m.range.end <= element.end
                            })
                            .map(|m| m.range.clone())
                            .collect();
                        hidden.sort_by_key(|r| r.start);
                        *alt = gaps(&element, &hidden)
                            .into_iter()
                            .map(|r| &text[r])
                            .collect();
                    }
                }
            }
            Event::Text(t) => {
                let o = open_inline(&mut open, &mut item_text, &range);
                if o.stack.is_empty() && !t.trim().is_empty() {
                    o.others = true;
                }
                if matches!(
                    o.kind,
                    LeafKind::Code { .. } | LeafKind::Html | LeafKind::FrontMatter
                ) {
                    o.covered.push(range);
                } else if text.get(range.clone()) == Some(&*t) {
                    o.cover(range);
                } else {
                    // An entity, written otherwise than it reads
                    o.cover(range.clone());
                    o.inlines.push(Inline {
                        range,
                        kind: InlineKind::Entity {
                            text: t.to_string(),
                        },
                    });
                }
            }
            Event::Code(_) => {
                let o = open_inline(&mut open, &mut item_text, &range);
                if o.stack.is_empty() {
                    o.others = true;
                }
                let content = code_content(text, range.clone());
                for gap in gaps(&range, std::slice::from_ref(&content)) {
                    o.marks.push(Mark {
                        range: gap,
                        reveal: range.clone(),
                    });
                }
                o.cover(range.clone());
                o.inlines.push(Inline {
                    range,
                    kind: InlineKind::Code,
                });
            }
            Event::InlineMath(ref latex) | Event::DisplayMath(ref latex) => {
                let display = matches!(event, Event::DisplayMath(_));
                let latex = latex_of(latex, &range);
                let o = open_inline(&mut open, &mut item_text, &range);
                if o.stack.is_empty() {
                    if display && o.only.is_none() && !o.others {
                        o.only = Some(OnlyOne::Math(latex.clone()));
                    } else {
                        o.others = true;
                    }
                }
                o.cover(range.clone());
                o.inlines.push(Inline {
                    range,
                    kind: InlineKind::Math { latex, display },
                });
            }
            Event::Html(_) => {
                if let Some(o) = open.as_mut() {
                    o.covered.push(range);
                }
            }
            Event::InlineHtml(_) | Event::FootnoteReference(_) => {
                let o = open_inline(&mut open, &mut item_text, &range);
                o.others = true;
                o.cover(range.clone());
                let footnote = matches!(event, Event::FootnoteReference(_));
                // A footnote reference shows as its raised label: its "[^"
                // and "]" are markup, shown while the caret is in it
                if footnote
                    && text[range.clone()].starts_with("[^")
                    && text[range.clone()].ends_with(']')
                    && range.len() > 3
                {
                    o.marks.push(Mark {
                        range: range.start..range.start + 2,
                        reveal: range.clone(),
                    });
                    o.marks.push(Mark {
                        range: range.end - 1..range.end,
                        reveal: range.clone(),
                    });
                }
                let kind = if footnote {
                    InlineKind::Footnote
                } else {
                    InlineKind::Html
                };
                o.inlines.push(Inline { range, kind });
            }
            Event::SoftBreak => {
                let o = open_inline(&mut open, &mut item_text, &range);
                o.cover(range);
            }
            Event::HardBreak => {
                let o = open_inline(&mut open, &mut item_text, &range);
                o.cover(range.clone());
                // Its spaces or backslash; the newline always shows
                let newline = range.end - usize::from(text[..range.end].ends_with('\n'));
                if newline > range.start {
                    o.marks.push(Mark {
                        range: range.start..newline,
                        reveal: range.clone(),
                    });
                }
                o.inlines.push(Inline {
                    range,
                    kind: InlineKind::HardBreak,
                });
            }
            Event::TaskListMarker(checked) => {
                let o = open_inline(&mut open, &mut item_text, &range);
                o.cover(range.clone());
                o.inlines.push(Inline {
                    range,
                    kind: InlineKind::Task { checked },
                });
            }
            _ => {}
        }
    }
    close(&mut open, &mut leaves, &containers, text.len());
    extras.defs = parser
        .reference_definitions()
        .iter()
        .map(|(label, def)| {
            (
                def.span.start,
                label.to_string(),
                def.dest.to_string(),
                def.title.as_deref().unwrap_or_default().to_string(),
            )
        })
        .collect();
    extras.defs.sort();
    drop(parser);
    extras.refs.extend(asked.into_inner());

    let leaves = split_display_maths(text, leaves);
    // Each item's first leaf shows its bullet
    for leaf in &leaves {
        if let Some(&c) = leaf.containers.last()
            && let ContainerKind::Item { first_leaf, .. } = &mut analysis.containers[c].kind
            && first_leaf.is_none()
        {
            *first_leaf = Some(leaf.range.start);
        }
    }
    // Nested lists' items whose first content is another list: the outer
    // item's bullet goes on the first leaf in it
    for c in 0..analysis.containers.len() {
        let range = analysis.containers[c].range.clone();
        if let ContainerKind::Item {
            first_leaf: first @ None,
            ..
        } = &mut analysis.containers[c].kind
        {
            *first = leaves
                .iter()
                .find(|l| range.contains(&l.range.start))
                .map(|l| l.range.start);
        }
    }
    analysis.leaves = with_spacers(text, leaves, &analysis.containers);
    for leaf in &mut analysis.leaves {
        if dialect.extensions()
            && matches!(
                leaf.kind,
                LeafKind::Paragraph
                    | LeafKind::Heading(_)
                    | LeafKind::Table(_)
                    | LeafKind::Image { .. }
            )
        {
            pair_tags(text, leaf);
            pair_highlights(text, leaf);
        }
    }
    analysis
}

/// The paragraphs with display formulas on lines of their own split
/// there: the formula's lines a [`LeafKind::Math`] leaf, and the lines
/// before and after it paragraphs. A formula is split off only where
/// nothing on its lines but container prefixes and spaces is not its own,
/// and no other inline element reaches across its lines
fn split_display_maths(text: &str, leaves: Vec<Leaf>) -> Vec<Leaf> {
    let mut out = Vec::with_capacity(leaves.len());
    for leaf in leaves {
        let mut rest = Some(leaf);
        while let Some(leaf) = rest.take() {
            match split_one(text, &leaf) {
                Some((before, math, after)) => {
                    out.extend(before);
                    out.push(math);
                    rest = after;
                }
                None => out.push(leaf),
            }
        }
    }
    out
}

/// The first display formula of `leaf` that [`split_display_maths`] splits
/// off: what comes before it, it, and what comes after it
fn split_one(text: &str, leaf: &Leaf) -> Option<(Option<Leaf>, Leaf, Option<Leaf>)> {
    if leaf.kind != LeafKind::Paragraph {
        return None;
    }
    let in_prefix = |at: usize| leaf.prefixes.iter().any(|p| p.start <= at && at < p.end);
    // Only prefixes, spaces and marks: nothing shown
    let empty = |range: Range<usize>| {
        range.clone().all(|at| {
            in_prefix(at)
                || matches!(text.as_bytes()[at], b' ' | b'\t' | b'\r' | b'\n')
                || leaf
                    .marks
                    .iter()
                    .any(|m| m.range.start <= at && at < m.range.end)
        })
    };
    for formula in &leaf.inlines {
        let InlineKind::Math {
            latex,
            display: true,
        } = &formula.kind
        else {
            continue;
        };
        let f = formula.range.clone();
        let (start, end) = (line_start(text, f.start), line_end(text, f.end));
        if (start == leaf.range.start && end == leaf.range.end)
            || !empty(start..f.start)
            || !empty(f.end..end)
        {
            continue;
        }
        let part = |range: Range<usize>| range.start <= range.end && !empty(range.clone());
        let crosses = leaf.inlines.iter().any(|i| {
            i.range != f
                && ![leaf.range.start..start, start..end, end..leaf.range.end]
                    .iter()
                    .any(|p| p.start <= i.range.start && i.range.end <= p.end)
        });
        if crosses {
            continue;
        }
        let sub = |range: Range<usize>, kind: LeafKind| Leaf {
            range: range.clone(),
            kind,
            prefixes: leaf
                .prefixes
                .iter()
                .filter(|p| range.start <= p.start && p.end <= range.end)
                .cloned()
                .collect(),
            marks: leaf
                .marks
                .iter()
                .filter(|m| range.start <= m.range.start && m.range.end <= range.end)
                .map(|m| Mark {
                    range: m.range.clone(),
                    reveal: if m.reveal == leaf.range {
                        range.clone()
                    } else {
                        m.reveal.clone()
                    },
                })
                .collect(),
            inlines: leaf
                .inlines
                .iter()
                .filter(|i| range.start <= i.range.start && i.range.end <= range.end)
                .cloned()
                .collect(),
            containers: leaf.containers.clone(),
        };
        let before = (start > leaf.range.start)
            .then(|| leaf.range.start..start - 1)
            .filter(|r| part(r.clone()))
            .map(|r| sub(r, LeafKind::Paragraph));
        let after = (end < leaf.range.end)
            .then(|| end + 1..leaf.range.end)
            .filter(|r| part(r.clone()))
            .map(|r| sub(r, LeafKind::Paragraph));
        let math = sub(
            start..end,
            LeafKind::Math {
                latex: latex.clone(),
            },
        );
        return Some((before, math, after));
    }
    None
}

/// The HTML tags Cerea writes for formatting Markdown has no syntax for,
/// open and close, and what they make
const TAGS: [(&str, InlineKind); 4] = [
    ("u", InlineKind::Underline),
    ("mark", InlineKind::Highlight),
    ("sup", InlineKind::Superscript),
    ("sub", InlineKind::Subscript),
];

/// `<u>`, `<mark>`, `<sup>` and `<sub>` with their closing tags in one
/// leaf: one element each, its tags markers shown only at the caret, as
/// `**` is (pulldown-cmark gives each tag as inline HTML of its own)
fn pair_tags(text: &str, leaf: &mut Leaf) {
    let tag = |range: &Range<usize>| -> Option<(bool, usize)> {
        let s = text[range.clone()].to_ascii_lowercase();
        let inner = s.strip_prefix('<')?.strip_suffix('>')?;
        let (close, name) = match inner.strip_prefix('/') {
            Some(name) => (true, name),
            None => (false, inner),
        };
        TAGS.iter()
            .position(|(t, _)| *t == name.trim())
            .map(|i| (close, i))
    };
    let mut open: Vec<(usize, usize)> = Vec::new();
    let mut pairs: Vec<(usize, usize, usize)> = Vec::new();
    for (i, inline) in leaf.inlines.iter().enumerate() {
        if inline.kind != InlineKind::Html {
            continue;
        }
        match tag(&inline.range) {
            Some((false, t)) => open.push((i, t)),
            Some((true, t)) => {
                if let Some(at) = open.iter().rposition(|(_, o)| *o == t) {
                    let (start, _) = open.remove(at);
                    open.truncate(at);
                    pairs.push((start, i, t));
                }
            }
            None => {}
        }
    }
    if pairs.is_empty() {
        return;
    }
    let mut remove = Vec::new();
    for (a, b, t) in pairs {
        let (o, c) = (leaf.inlines[a].range.clone(), leaf.inlines[b].range.clone());
        let whole = o.start..c.end;
        leaf.marks.push(Mark {
            range: o,
            reveal: whole.clone(),
        });
        leaf.marks.push(Mark {
            range: c,
            reveal: whole.clone(),
        });
        leaf.inlines.push(Inline {
            range: whole,
            kind: TAGS[t].1.clone(),
        });
        remove.extend([a, b]);
    }
    remove.sort_unstable();
    for i in remove.into_iter().rev() {
        leaf.inlines.remove(i);
    }
    leaf.inlines
        .sort_by_key(|i| (i.range.start, std::cmp::Reverse(i.range.end)));
}

/// `==text==` (the highlight extension, as Typora and Obsidian write it):
/// an opening `==` before a non-space, a closing one after a non-space, in
/// text that is not code, maths, a tag or markup already
fn pair_highlights(text: &str, leaf: &mut Leaf) {
    let src = &text[leaf.range.clone()];
    if !src.contains("==") {
        return;
    }
    let busy: Vec<Range<usize>> = leaf
        .inlines
        .iter()
        .filter(|i| {
            matches!(
                i.kind,
                InlineKind::Code
                    | InlineKind::Math { .. }
                    | InlineKind::Html
                    | InlineKind::Escape
                    | InlineKind::Entity { .. }
            )
        })
        .map(|i| i.range.clone())
        .chain(leaf.marks.iter().map(|m| m.range.clone()))
        .chain(leaf.prefixes.iter().cloned())
        .collect();
    let free = |r: &Range<usize>| !busy.iter().any(|b| b.start < r.end && r.start < b.end);
    let bytes = text.as_bytes();
    let mut found: Vec<usize> = Vec::new();
    let mut at = leaf.range.start;
    while let Some(i) = text[at..leaf.range.end].find("==") {
        let start = at + i;
        // A run of three or more is not a marker
        let mut end = start + 2;
        while end < leaf.range.end && bytes[end] == b'=' {
            end += 1;
        }
        if end - start == 2 && (start == 0 || bytes[start - 1] != b'=') && free(&(start..end)) {
            found.push(start);
        }
        at = end;
    }
    let mut pairs = Vec::new();
    let mut open: Option<usize> = None;
    for at in found {
        let before = text[..at].chars().next_back();
        let after = text[at + 2..].chars().next();
        match open {
            None if after.is_some_and(|c| !c.is_whitespace()) && at + 2 < leaf.range.end => {
                open = Some(at)
            }
            Some(o) if before.is_some_and(|c| !c.is_whitespace()) && at > o + 2 => {
                pairs.push((o, at));
                open = None;
            }
            _ => {}
        }
    }
    for (o, c) in pairs {
        let whole = o..c + 2;
        leaf.marks.push(Mark {
            range: o..o + 2,
            reveal: whole.clone(),
        });
        leaf.marks.push(Mark {
            range: c..c + 2,
            reveal: whole.clone(),
        });
        leaf.inlines.push(Inline {
            range: whole,
            kind: InlineKind::Highlight,
        });
    }
    leaf.inlines
        .sort_by_key(|i| (i.range.start, std::cmp::Reverse(i.range.end)));
}

/// The open leaf for an inline event, opening a tight list item's
fn open_inline<'a>(
    open: &'a mut Option<Open>,
    item_text: &mut bool,
    range: &Range<usize>,
) -> &'a mut Open {
    if open.is_none() || *item_text {
        if *item_text {
            *item_text = false;
        }
        if open.is_none() {
            *open = Some(Open::new(LeafKind::Paragraph, range.clone()));
        }
    }
    open.as_mut().unwrap()
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// A code span's content, inside its backticks and the one space each side
/// that CommonMark strips
fn code_content(text: &str, range: Range<usize>) -> Range<usize> {
    let s = &text[range.clone()];
    let ticks = s.bytes().take_while(|b| *b == b'`').count();
    let mut start = range.start + ticks;
    let mut end = range.end - ticks.min(s.len() - ticks);
    let inner = &text[start..end];
    if inner.len() >= 2
        && inner.starts_with(' ')
        && inner.ends_with(' ')
        && !inner.trim().is_empty()
    {
        start += 1;
        end -= 1;
    }
    start..end.max(start)
}

/// `outer` less `inner`, which are in order and within it
fn gaps(outer: &Range<usize>, inner: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut at = outer.start;
    let mut sorted: Vec<&Range<usize>> = inner.iter().collect();
    sorted.sort_by_key(|r| r.start);
    for r in sorted {
        if r.start > at {
            out.push(at..r.start);
        }
        at = at.max(r.end);
    }
    if at < outer.end {
        out.push(at..outer.end);
    }
    out
}

/// Turn a gathered leaf into a [`Leaf`]: whole lines, with what in them is
/// not content sorted into prefixes and marks
fn finish(text: &str, o: Open, containers: Vec<usize>, end: usize) -> Leaf {
    let mut covered = o.covered.clone();
    covered.sort_by_key(|r| r.start);
    let first = o
        .block
        .start
        .min(covered.first().map_or(usize::MAX, |r| r.start));
    let start = line_start(text, first.min(text.len()));
    // The block's end, without its last newline
    let block_end = o
        .block
        .end
        .max(covered.last().map_or(0, |r| r.end))
        .min(end.max(o.block.end));
    let mut stop = block_end.min(text.len());
    while stop > start && text.as_bytes()[stop - 1] == b'\n' {
        stop -= 1;
    }
    let stop = line_end(text, stop.max(start));
    let range = start..stop;

    let kind = match (&o.kind, &o.table, &o.only, o.others) {
        (_, Some(table), _, _) => LeafKind::Table(table.clone()),
        (LeafKind::Paragraph, None, Some(OnlyOne::Math(latex)), false) => LeafKind::Math {
            latex: latex.clone(),
        },
        (LeafKind::Paragraph, None, Some(OnlyOne::Image(url, alt, _)), false) => LeafKind::Image {
            url: url.clone(),
            alt: alt.clone(),
        },
        (kind, _, _, _) => kind.clone(),
    };

    let mut marks = o.marks;
    let mut prefixes = Vec::new();
    // What on each line is not content: before the line's first content, a
    // prefix; elsewhere, a mark (a heading's #s, a fence, an escape)
    let mut line = range.start;
    while line <= range.end {
        let line_stop = line_end(text, line);
        let on_line: Vec<Range<usize>> = covered
            .iter()
            .filter(|r| r.start < line_stop.max(line + 1) && r.end > line)
            .map(|r| r.start.max(line)..r.end.min(line_stop))
            .filter(|r| r.start < r.end || r.start == line_stop)
            .collect();
        let content_start = on_line.first().map(|r| r.start);
        if let LeafKind::Heading(_) = kind
            && line > range.start
            && content_start.is_none()
        {
            // A setext heading's underline, with the line break before it
            marks.push(Mark {
                range: line - 1..line_stop,
                reveal: range.clone(),
            });
            if line_stop >= range.end {
                break;
            }
            line = line_stop + 1;
            continue;
        }
        let own = match &kind {
            // Where the block itself starts on its first line
            LeafKind::Heading(_)
            | LeafKind::Code { .. }
            | LeafKind::Rule
            | LeafKind::Html
            | LeafKind::FrontMatter
                if line == range.start =>
            {
                Some(o.block.start.clamp(line, line_stop))
            }
            _ => None,
        };
        let prefix_end = match (&kind, own, content_start) {
            (_, Some(own), _) => own,
            (LeafKind::Code { .. }, None, None) if is_fence_line(text, line..line_stop) => {
                line + text[line..line_stop].find(['`', '~']).unwrap_or(0)
            }
            (_, None, Some(content)) => content,
            (_, None, None) => line_stop,
        };
        if prefix_end > line {
            prefixes.push(line..prefix_end);
        }
        let rest: Vec<Range<usize>> = gaps(&(prefix_end..line_stop), &on_line);
        for gap in rest {
            if marks
                .iter()
                .any(|m| m.range.start <= gap.start && m.range.end >= gap.end)
            {
                continue;
            }
            let reveal = match &kind {
                LeafKind::Paragraph | LeafKind::Table(_) if text[gap.clone()].starts_with('\\') => {
                    // An escape: its backslash
                    marks.push(Mark {
                        range: gap.start..gap.start + 1,
                        reveal: gap.start..(gap.start + 2).min(range.end),
                    });
                    if gap.end > gap.start + 1 {
                        prefixes.push(gap.start + 1..gap.end);
                    }
                    continue;
                }
                // Trailing spaces and the like show while the caret is in
                // the leaf, so that it is drawn after them
                _ => range.clone(),
            };
            marks.push(Mark { range: gap, reveal });
        }
        if line_stop >= range.end {
            break;
        }
        line = line_stop + 1;
    }
    marks.sort_by_key(|m| m.range.start);
    prefixes.sort_by_key(|r| r.start);
    if let LeafKind::Code { .. } = kind {
        // A fence takes its line break with it, so that a hidden fence
        // leaves no empty line
        let fences: Vec<usize> = marks
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                is_fence_line(
                    text,
                    line_start(text, m.range.start)..line_end(text, m.range.start),
                )
            })
            .map(|(i, _)| i)
            .collect();
        for i in fences {
            let m = &mut marks[i];
            if m.range.end < range.end
                && text.as_bytes()[m.range.end] == b'\n'
                && m.range.start == o.block.start.max(range.start)
            {
                m.range.end += 1;
            } else if m.range.start != o.block.start.max(range.start) {
                // A closing fence: the newline before it, and the prefix on
                // its line, are its
                let line = line_start(text, m.range.start);
                if line > range.start {
                    let fence = m.range.start;
                    m.range.start = line - 1;
                    prefixes.retain(|p| !(p.start >= line && p.end <= fence));
                }
            }
        }
    }
    let mut inlines = o.inlines;
    inlines.sort_by_key(|i| i.range.start);
    Leaf {
        range,
        kind,
        prefixes,
        marks,
        inlines,
        containers,
    }
}

/// Where a fenced code block without a label would take one: after its
/// opening fence, to the end of that line. `None` for a block with a label,
/// an indented block, or a leaf that is not code
pub fn label_slot(text: &str, leaf: &Leaf) -> Option<Range<usize>> {
    if !matches!(&leaf.kind, LeafKind::Code { lang } if lang.is_empty()) {
        return None;
    }
    let line = leaf.range.start..line_end(text, leaf.range.start);
    if !is_fence_line(text, line.clone()) {
        return None;
    }
    let s = &text[line.clone()];
    let at = s.find(['`', '~'])?;
    let fence = s.as_bytes()[at];
    let end = at + s[at..].bytes().take_while(|b| *b == fence).count();
    s[end..]
        .trim()
        .is_empty()
        .then(|| line.start + end..line.end)
}

fn is_fence_line(text: &str, line: Range<usize>) -> bool {
    let s = text[line].trim_start_matches([' ', '>', '\t']);
    s.starts_with("```") || s.starts_with("~~~")
}

/// The leaves, with a spacer for each line none of them covers
fn with_spacers(text: &str, leaves: Vec<Leaf>, containers: &[Container]) -> Vec<Leaf> {
    let mut out = Vec::with_capacity(leaves.len() * 2);
    let mut at = 0;
    let spacer = |start: usize, out: &mut Vec<Leaf>| {
        let end = line_end(text, start);
        let inside: Vec<usize> = containers
            .iter()
            .enumerate()
            .filter(|(_, c)| c.range.start <= start && start < c.range.end.max(c.range.start + 1))
            .map(|(i, _)| i)
            .collect();
        // A quote's markers on the line are its prefix
        let prefix_end = start
            + text[start..end]
                .char_indices()
                .take_while(|(_, c)| *c == '>' || *c == ' ' || *c == '\t')
                .last()
                .map_or(0, |(i, c)| i + c.len_utf8());
        let prefixes = if prefix_end > start {
            std::iter::once(start..prefix_end).collect()
        } else {
            Vec::new()
        };
        out.push(Leaf {
            range: start..end,
            kind: LeafKind::Spacer,
            prefixes,
            marks: Vec::new(),
            inlines: Vec::new(),
            containers: inside,
        });
        end
    };
    for leaf in leaves {
        while at < leaf.range.start {
            let end = spacer(at, &mut out);
            at = end + 1;
        }
        at = at.max(leaf.range.end + 1);
        out.push(leaf);
    }
    while at <= text.len() {
        if at == text.len() && !text.is_empty() && !text.ends_with('\n') {
            break;
        }
        let end = spacer(at, &mut out);
        at = end + 1;
    }
    out
}

impl Analysis {
    /// The leaf `offset` is in: the one whose range holds it, ends included,
    /// preferring the later at a shared boundary only if the earlier ends
    /// before it
    /// A copy of leaf `index`
    pub fn clone_leaf(&self, index: usize) -> Leaf {
        self.leaves[index].clone()
    }

    pub fn leaf_at(&self, offset: usize) -> usize {
        // The leaves are in order: the first ending at or after `offset`
        let i = self.leaves.partition_point(|leaf| leaf.range.end < offset);
        i.min(self.leaves.len().saturating_sub(1))
    }

    /// The headings, for the outline: level, title, and where
    pub fn headings<'a>(&'a self, text: &'a str) -> impl Iterator<Item = (u8, String, usize)> + 'a {
        self.leaves.iter().filter_map(move |leaf| match leaf.kind {
            LeafKind::Heading(level) => Some((level, plain_text(text, leaf), leaf.range.start)),
            _ => None,
        })
    }
}

/// A leaf's text as it reads: no markup
pub fn plain_text(text: &str, leaf: &Leaf) -> String {
    let mut hidden: Vec<Range<usize>> = leaf.prefixes.clone();
    hidden.extend(leaf.marks.iter().map(|m| m.range.clone()));
    hidden.sort_by_key(|r| r.start);
    gaps(&leaf.range, &hidden)
        .into_iter()
        .map(|r| &text[r])
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(text: &str, ranges: &[Range<usize>]) -> Vec<String> {
        ranges.iter().map(|r| text[r.clone()].to_string()).collect()
    }

    fn marks(text: &str, leaf: &Leaf) -> Vec<String> {
        leaf.marks
            .iter()
            .map(|m| text[m.range.clone()].to_string())
            .collect()
    }

    fn kinds(a: &Analysis) -> Vec<String> {
        a.leaves
            .iter()
            .map(|l| match &l.kind {
                LeafKind::Paragraph => "p".into(),
                LeafKind::Heading(n) => format!("h{n}"),
                LeafKind::Code { lang } => format!("code:{lang}"),
                LeafKind::Math { .. } => "math".into(),
                LeafKind::Image { .. } => "image".into(),
                LeafKind::Table(_) => "table".into(),
                LeafKind::Rule => "rule".into(),
                LeafKind::Html => "html".into(),
                LeafKind::FrontMatter => "front".into(),
                LeafKind::Spacer => "_".into(),
            })
            .collect()
    }

    #[test]
    fn an_unlabelled_fence_has_a_slot_for_a_label() {
        let text = "```\necho hi\n```\n\n> ~~~~  \n> x\n> ~~~~\n\n```sh\nls\n```\n\n    indented\n";
        let a = analyse(text);
        let slots: Vec<Option<&str>> = a
            .leaves
            .iter()
            .filter(|l| matches!(l.kind, LeafKind::Code { .. }))
            .map(|l| label_slot(text, l).map(|r| &text[r]))
            .collect();
        assert_eq!(slots, [Some(""), Some("  "), None, None], "{a:#?}");
        let first = a
            .leaves
            .iter()
            .find(|l| matches!(l.kind, LeafKind::Code { .. }))
            .unwrap();
        assert_eq!(label_slot(text, first), Some(3..3));
    }

    /// Front matter is only a block at the very start of a document. The
    /// same `---` / `key: value` / `---` block further on is read as
    /// CommonMark reads it (a rule, then a heading of the lines down to the
    /// second `---`), and a `---` there is no open rule
    #[test]
    fn front_matter_is_only_a_leading_block() {
        let block = "---\ntitle: x\nauthor: J\n---\n";
        let leading = format!("{block}\nBody.\n");
        let a = analyse(&leading);
        assert_eq!(kinds(&a)[0], "front", "{a:#?}");
        assert_eq!(&leading[a.leaves[0].range.clone()], block.trim_end());
        assert_eq!(
            a.leaves
                .iter()
                .filter(|l| l.kind == LeafKind::FrontMatter)
                .count(),
            1
        );
        let mid = format!("Intro.\n\n{block}\nMore.\n");
        let m = analyse(&mid);
        let solid: Vec<String> = kinds(&m).into_iter().filter(|k| k != "_").collect();
        assert_eq!(solid, ["p", "rule", "h2", "p"], "{m:#?}");
        let heading = m
            .leaves
            .iter()
            .find(|l| l.kind == LeafKind::Heading(2))
            .unwrap();
        assert_eq!(&mid[heading.range.clone()], "title: x\nauthor: J\n---");
        // Both: only the first is front matter
        let both = format!("{block}\n{mid}");
        assert_eq!(
            analyse(&both)
                .leaves
                .iter()
                .filter(|l| l.kind == LeafKind::FrontMatter)
                .count(),
            1
        );
        // A `---` with text after it, no closing line yet: an open rule at
        // the start, nowhere else
        assert_eq!(
            analyse_parts("---\ntitle: x\n\nBody.\n", &mut |_| None)
                .1
                .open_rules,
            [0]
        );
        assert!(
            analyse_parts("Intro.\n\n---\ntitle: x\n\nBody.\n", &mut |_| None)
                .1
                .open_rules
                .is_empty()
        );
        // Where a part of a text does not hold the document's start, a
        // block at its start is not front matter either
        let (part, _) = analyse_region(&format!("{block}\nMore.\n"), None, &mut |_| None);
        assert!(
            part.leaves.iter().all(|l| l.kind != LeafKind::FrontMatter),
            "{part:#?}"
        );
    }

    #[test]
    fn leaves_cover_every_line_once() {
        let text = "# Title\n\nSome text\nmore.\n\n- a\n- b\n\n```\ncode\n```\n";
        let a = analyse(text);
        assert_eq!(
            kinds(&a),
            ["h1", "_", "p", "_", "p", "p", "_", "code:", "_"]
        );
        let mut at = 0;
        for leaf in &a.leaves {
            assert_eq!(leaf.range.start, at, "{:?}", a.leaves);
            at = leaf.range.end + 1;
        }
        assert_eq!(at, text.len() + 1);
    }

    #[test]
    fn inline_markers_are_element_less_children() {
        let text = "Some **strong *nested* text**, `code`, [link](http://x.y \"t\").";
        let a = analyse(text);
        let leaf = &a.leaves[0];
        assert_eq!(
            marks(text, leaf),
            ["**", "*", "*", "**", "`", "`", "[", "](http://x.y \"t\")"]
        );
        assert!(leaf.prefixes.is_empty());
        let link = leaf
            .inlines
            .iter()
            .find(|i| matches!(i.kind, InlineKind::Link { .. }))
            .unwrap();
        assert_eq!(&text[link.range.clone()], "[link](http://x.y \"t\")");
        // Each marker shows with its own element
        let star = &leaf.marks[1];
        assert_eq!(&text[star.reveal.clone()], "*nested*");
    }

    #[test]
    fn heading_marks_show_in_the_heading() {
        let text = "## Title ##\nSetext\n===\n";
        let a = analyse(text);
        assert_eq!(kinds(&a), ["h2", "h1", "_"]);
        assert_eq!(marks(text, &a.leaves[0]), ["## ", " ##"]);
        assert_eq!(a.leaves[0].marks[0].reveal, a.leaves[0].range);
        assert_eq!(marks(text, &a.leaves[1]), ["\n==="]);
    }

    #[test]
    fn quote_and_list_prefixes_are_hidden() {
        let text =
            "> one\n> *two*\n>\n> - three\n\n- [ ] task\n- second\n  continued\n  1. nested\n";
        let a = analyse(text);
        assert_eq!(kinds(&a), ["p", "_", "p", "_", "p", "p", "p", "_"]);
        assert_eq!(show(text, &a.leaves[0].prefixes), ["> ", "> "]);
        assert_eq!(show(text, &a.leaves[1].prefixes), [">"]);
        assert_eq!(show(text, &a.leaves[2].prefixes), ["> - "]);
        assert_eq!(show(text, &a.leaves[5].prefixes), ["- ", "  "]);
        assert_eq!(show(text, &a.leaves[6].prefixes), ["  1. "]);
        // The quote, its item, and the list's items
        let quote = &a.containers[a.leaves[2].containers[0]];
        assert_eq!(quote.kind, ContainerKind::Quote);
        let item = &a.containers[a.leaves[2].containers[1]];
        assert!(
            matches!(item.kind, ContainerKind::Item { number: None, first_leaf: Some(s), .. } if s == a.leaves[2].range.start)
        );
        let nested = &a.containers[*a.leaves[6].containers.last().unwrap()];
        assert!(matches!(
            nested.kind,
            ContainerKind::Item {
                number: Some(1),
                ..
            }
        ));
        assert_eq!(a.leaves[6].containers.len(), 2);
        let task = &a.leaves[4].inlines[0];
        assert_eq!(task.kind, InlineKind::Task { checked: false });
    }

    #[test]
    fn escapes_and_entities() {
        let text = r"a \*b\* &amp; c";
        let a = analyse(text);
        let leaf = &a.leaves[0];
        assert_eq!(marks(text, leaf), ["\\", "\\"]);
        assert_eq!(&text[leaf.marks[0].reveal.clone()], "\\*");
        let entity = leaf
            .inlines
            .iter()
            .find(|i| matches!(i.kind, InlineKind::Entity { .. }))
            .unwrap();
        assert_eq!(&text[entity.range.clone()], "&amp;");
    }

    #[test]
    fn code_fences_and_content() {
        let text = "> ```rust\n> let x = 1;\n>\n> ```\n";
        let a = analyse(text);
        assert_eq!(kinds(&a), ["code:rust", "_"]);
        let leaf = &a.leaves[0];
        assert_eq!(marks(text, leaf), ["```rust\n", "\n> ```"]);
        assert_eq!(show(text, &leaf.prefixes), ["> ", "> ", ">"]);
    }

    #[test]
    fn maths_images_tables_and_rules() {
        let text = "$$\n\\int x\n$$\n\n![alt *text*](img.png)\n\n| a | b |\n|---|:-:|\n| 1 | **2** |\n\n---\n\nInline $x^2$ maths.\n";
        let a = analyse(text);
        assert_eq!(
            kinds(&a),
            [
                "math", "_", "image", "_", "table", "_", "rule", "_", "p", "_"
            ]
        );
        assert_eq!(
            a.leaves[0].kind,
            LeafKind::Math {
                latex: "\n\\int x\n".into()
            }
        );
        assert_eq!(
            a.leaves[2].kind,
            LeafKind::Image {
                url: "img.png".into(),
                alt: "alt text".into()
            }
        );
        let LeafKind::Table(table) = &a.leaves[4].kind else {
            panic!()
        };
        assert_eq!(table.rows.len(), 2);
        assert_eq!(show(text, &table.rows[1]), [" 1 ", " **2** "]);
        assert_eq!(marks(text, &a.leaves[6]), ["---"]);
        let math = &a.leaves[8].inlines[0];
        assert!(math.kind.is_replaced());
        assert_eq!(&text[math.range.clone()], "$x^2$");
    }

    #[test]
    fn maths_as_threads_read_it() {
        let text = "Inline \\(x^2\\) here.\n\n\\[\n\\int f\n\\]\n\n\\begin{align}\na &= b\n\\end{align}\n\nDone.\n";
        let a = analyse(text);
        assert_eq!(kinds(&a), ["p", "_", "math", "_", "math", "_", "p", "_"]);
        let math = &a.leaves[0].inlines[0];
        assert_eq!(&text[math.range.clone()], "\\(x^2\\)");
        assert_eq!(
            math.kind,
            InlineKind::Math {
                latex: "x^2".into(),
                display: false,
            }
        );
        assert_eq!(&text[a.leaves[2].range.clone()], "\\[\n\\int f\n\\]");
        assert_eq!(
            &text[a.leaves[4].range.clone()],
            "\\begin{align}\na &= b\n\\end{align}"
        );
        let LeafKind::Math { latex } = &a.leaves[4].kind else {
            panic!()
        };
        assert!(latex.contains("\\begin{align}"));
        // Every line, once
        let mut at = 0;
        for leaf in &a.leaves {
            assert_eq!(leaf.range.start, at, "{:?}", a.leaves);
            at = leaf.range.end + 1;
        }
    }

    #[test]
    fn plain_text_reads_without_markup() {
        let text = "## A *very* `good` [title](x) ##\n";
        let a = analyse(text);
        assert_eq!(
            a.headings(text).collect::<Vec<_>>(),
            [(2, "A very good title".to_string(), 0)]
        );
    }

    #[test]
    fn empty_and_unterminated_texts() {
        assert_eq!(kinds(&analyse("")), ["_"]);
        assert_eq!(kinds(&analyse("a")), ["p"]);
        assert_eq!(kinds(&analyse("a\n")), ["p", "_"]);
        assert_eq!(kinds(&analyse("# \n")), ["h1", "_"]);
    }

    /// Item 11: a footnote reference's "[^" and "]" are markup, shown while
    /// the caret is in the reference; its label between them is the text
    #[test]
    fn a_footnote_reference_has_its_brackets_as_markup() {
        let text = "Read it.[^keys] Then go.[^1]\n\n[^keys]: The keys.\n\n[^1]: One.\n";
        let a = analyse(text);
        let leaf = &a.leaves[0];
        assert_eq!(marks(text, leaf), ["[^", "]", "[^", "]"]);
        let footnotes: Vec<&str> = leaf
            .inlines
            .iter()
            .filter(|i| i.kind == InlineKind::Footnote)
            .map(|i| &text[i.range.clone()])
            .collect();
        assert_eq!(footnotes, ["[^keys]", "[^1]"]);
        // Each pair of marks reveals the whole reference
        for mark in &leaf.marks {
            assert!(
                footnotes.contains(&&text[mark.reveal.clone()]),
                "{:?}",
                &text[mark.reveal.clone()]
            );
        }
        // Inline HTML is not a footnote and has no such marks
        let html = "A <b>word</b> here.\n";
        let a = analyse(html);
        assert!(
            a.leaves[0]
                .inlines
                .iter()
                .all(|i| i.kind != InlineKind::Footnote)
        );
    }
}
