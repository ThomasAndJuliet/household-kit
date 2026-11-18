// SPDX-License-Identifier: GPL-3.0-only

//! The household's content model: Markdown with maths, as Cerea's editor
//! reads it. The source text is the document; everything else is derived
//! from it and addressed into it by byte range.
//!
//! - [`maths`]: maths recognition, with the map from what it rewrote back
//!   to the source.
//! - [`syntax`]: a text's [`syntax::Analysis`] — its leaves (paragraphs,
//!   headings, code, display maths, tables, …), the containers they sit in
//!   (quotes, list items, footnotes) and their inline elements, by source
//!   range.
//! - [`blocks`]: the analysis kept up to date edit by edit, a block at a
//!   time, with a differential test against a whole parse.
//! - [`display`]: what each leaf shows, with source offsets mapped to
//!   shown ones and back.
//! - [`document`]: a [`document::Source`] and its analysis as one
//!   snapshot, edited or appended to (streaming is an append).
//! - [`inline`]: inline content for a single line of text, such as a
//!   title.
//! - [`anchor`]: positions and annotations that follow edits.
//! - [`math`]: LaTeX laid out by RaTeX as SVG with its metrics, and the
//!   formula cache.
//! - [`highlight`]: code split into token classes by syntect.
//!
//! No module depends on libcosmic or iced (household-kit's rule): the
//! boxes that shape and draw content are household-widgets' `content`.

// pub mod anchor;
pub mod blocks;
pub mod display;
// pub mod document;
// pub mod highlight;
// pub mod inline;
// pub mod math;
pub mod maths;
pub mod syntax;

use pulldown_cmark::Options;

/// How a text is read
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Dialect {
    /// A document, as Cerea edits it: front matter at its start, a line
    /// break in a paragraph a space, and Cerea's extensions (`<u>`,
    /// `<mark>`, `<sup>`, `<sub>`, `==…==`)
    #[default]
    Document,
    /// A conversation's message: its line breaks are meant, GitHub's
    /// blockquote tags (`> [!NOTE]`) are read, and nothing is front matter
    Chat,
}

impl Dialect {
    /// pulldown-cmark's options
    pub fn options(self) -> Options {
        let common = Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_MATH
            | Options::ENABLE_FOOTNOTES;
        match self {
            Dialect::Document => common,
            Dialect::Chat => common | Options::ENABLE_GFM,
        }
    }

    /// Whether a text may start with front matter
    pub fn front_matter(self) -> bool {
        self == Dialect::Document
    }

    /// Whether a line break inside a paragraph is shown as one, not as a
    /// space
    pub fn soft_breaks_kept(self) -> bool {
        self == Dialect::Chat
    }

    /// Whether Cerea's formatting extensions are read
    pub fn extensions(self) -> bool {
        self == Dialect::Document
    }
}
