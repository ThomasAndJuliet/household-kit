//! How the household's rendered views set prose: the **Paper view** and the
//! **Text view**, in the terminal's conversations and in Cerea's documents
//! alike (Juliet, 16 Oct: "paper mode" means the chat's Paper view, and
//! Cerea's Text view should look like the chat's Text view).
//!
//! Every value is plain data: font families by name, sizes as ratios of the
//! view's text size, spacings in ems of it, rules in pixels. The values are
//! cosmic-term's (origin/main f79a869e, 16 Oct), whose views are the
//! reference: `src/thread/view.rs` (`ThreadStyle::with_paper`) and
//! `src/thread/rich.rs` (`build_block`, `gap_before`, `CellSpec::new`).
//! Each app turns them into its own widgets; nothing here names a libcosmic
//! or iced type.
//!
//! The view's text size and line spacing are the shared settings
//! (`paper-size` and `paper-line-spacing`, `prose-size` and
//! `prose-line-spacing`, household rule 33); [`Style::size`] and
//! [`Style::line`] are their defaults. Everything else follows from them.
//!
//! [`reference`] holds a sample and where the terminal sets each of its
//! lines. Each app's tests lay the sample out and compare
//! ([`reference::compare`]), so an app that drifts from these values, or
//! from the other app, fails its tests.

pub mod reference;

/// A view's typography
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    /// The text's family, or empty for the interface's sans-serif
    pub body: &'static str,
    /// Code's family. The terminal sets code in its terminal font (Noto Sans
    /// Mono unless changed); this is that default
    pub mono: &'static str,
    /// The text's size by default, in pixels
    pub size: f32,
    /// Line spacing by default, as a multiple of the size
    pub line: f32,
    /// Paragraphs, list items and quotations justified (never headings,
    /// code or table cells)
    pub justify: bool,
    /// The text column's widest, in ems
    pub measure: f32,
    pub weights: Weights,
    /// Headings 1 to 6
    pub headings: [Heading; 6],
    pub spacing: Spacing,
    pub code: Code,
    pub maths: Maths,
    pub list: List,
    pub quote: Quote,
    /// Code, display maths, tables and figures are padded by this on every
    /// side, in ems, within the column (the terminal draws a plate there)
    pub plate_pad: f32,
    pub table: Table,
    pub rule: HorizontalRule,
    pub figures: Figures,
    /// Links set in bold, as colour alone is easily missed in a reading
    /// font's thin strokes
    pub links_bold: bool,
    pub inks: Inks,
}

/// Weights in the text's and in code's family
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Weights {
    pub regular: u16,
    pub bold: u16,
    /// Regular text with Thicken text on, where the family has it
    pub thickened: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Heading {
    /// Size against the text's (household rule 23a)
    pub scale: f32,
    /// Its own line spacing, shrinking as it grows (rule 23a)
    pub line: f32,
    pub weight: u16,
    pub justify: bool,
}

/// The room between blocks, in ems of the text's size
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spacing {
    /// Between any two blocks
    pub block: f32,
    /// Between two list items of one list: the items' first blocks, both
    /// with a bullet or number, at any depth (a nested list's first item
    /// under its parent's, and the next outer item after a nested list,
    /// too); not where a new list starts at the same depth or further out
    pub tight: f32,
    /// Added above a heading
    pub heading_extra: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Code {
    /// Under a serif, code as large as it is in a sans-serif's small text:
    /// both kinds of code are set at this times their own ratio
    pub scale: f32,
    /// A code block's size against the text's, before `scale`
    pub block: f32,
    /// Inline code's size against the text's, before `scale`; its line
    /// keeps the text's line height
    pub inline: f32,
    /// A code block's line spacing, as a multiple of its own size: the
    /// text's line spacing ([`Style::line`])
    pub line_follows_text: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Maths {
    /// Formulas' size against the text's
    pub scale: f32,
    /// A display formula is centred in the column, inside the plate's
    /// padding, and scaled down to fit it
    pub display_centred: bool,
    /// An inline formula's line is made taller where it rises more than
    /// this above the baseline, or drops more than `drop` below it (ems);
    /// by twice the larger excess, so the line stays centred on its text
    pub rise: f32,
    pub drop: f32,
    /// Display formulas are set unnumbered (the terminal turns `align` into
    /// `align*`): each formula in a reply stands alone
    pub numbered: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct List {
    /// An item's text, from its list's edge, per level, in ems
    pub indent: f32,
    /// Its bullet's or number's right edge, before the text, in ems
    pub marker_gap: f32,
    /// Bullets: the first for the outermost list, the second for every
    /// list inside it
    pub bullets: [&'static str; 2],
    /// A numbered item's mark is its number and this
    pub number_suffix: &'static str,
    /// A task's mark, open and done
    pub tasks: [&'static str; 2],
    /// The marks at the text's size, in [`Ink::Muted`]
    pub marker_ink: Ink,
    /// An item's text set as a paragraph is: justified where the body is
    pub justify: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quote {
    /// Its text's indent per level, in ems
    pub indent: f32,
    /// The bar: where it starts within the indent (ems), its width and its
    /// corners' radius (px)
    pub bar_at: f32,
    pub bar_width: f32,
    pub bar_radius: f32,
    pub bar_ink: Ink,
    /// Its text: the body's soft tone (rules 25d, 25e: long reading),
    /// justified where the body is
    pub ink: Ink,
    pub justify: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Table {
    /// Each cell's padding, in ems
    pub cell_pad: f32,
    /// Cells at the text's size and line spacing; the heading row in bold,
    /// the same size as the rows (rule 25f)
    pub head_weight: u16,
    /// The rule under the heading row, and between the other rows (none
    /// under the last), in px
    pub head_rule: f32,
    pub head_rule_ink: Ink,
    pub row_rule: f32,
    pub row_rule_ink: Ink,
    /// No column narrower than this (ems) before fitting, nor fitted below
    /// its natural width or `narrowest` (ems), whichever is less; a table
    /// wider than the column scrolls
    pub least_column: f32,
    pub narrowest: f32,
    /// The table centred in the column
    pub centred: bool,
    /// A pixel spare in each column's natural width, so its text set again
    /// that wide keeps to its line
    pub spare: f32,
}

/// A horizontal rule: a 1 px line this far down (ems) a block this high
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HorizontalRule {
    pub at: f32,
    pub height: f32,
    pub thickness: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Figures {
    /// An image no wider than this (px) nor higher than `image_height` ems
    pub image_width: f32,
    pub image_height: f32,
    /// A chart no wider than this (ems); its labels at `chart_label` of the
    /// text's size
    pub chart_width: f32,
    pub chart_label: f32,
    /// A diagram no higher than this (ems)
    pub diagram_height: f32,
}

/// The tone each part is set in. The colours are each app's, from the
/// household tones (rules 25d, 25e); this says which role
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    /// Body text: the soft dark tone on a light page
    Text,
    /// Headings and bold: the darkest
    Strong,
    /// The text's tone at [`MUTED_ALPHA`]
    Muted,
    /// The text's tone at [`RULE_ALPHA`]
    Rule,
}

/// Muted text's opacity over the page
pub const MUTED_ALPHA: f32 = 0.68;
/// Rules' opacity
pub const RULE_ALPHA: f32 = 0.25;

/// Which tone each kind of text takes
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Inks {
    pub body: Ink,
    pub headings: Ink,
    pub bold: Ink,
    /// The heading row of a table: bold, in the darkest, as bold is
    /// everywhere
    pub table_head: Ink,
    pub code: Ink,
}

const HEADINGS: [Heading; 6] = [
    Heading {
        scale: 1.55,
        line: 1.2,
        weight: 700,
        justify: false,
    },
    Heading {
        scale: 1.32,
        line: 1.3,
        weight: 700,
        justify: false,
    },
    Heading {
        scale: 1.16,
        line: 1.35,
        weight: 700,
        justify: false,
    },
    Heading {
        scale: 1.05,
        line: 1.4,
        weight: 700,
        justify: false,
    },
    Heading {
        scale: 1.0,
        line: 1.4,
        weight: 700,
        justify: false,
    },
    Heading {
        scale: 1.0,
        line: 1.4,
        weight: 700,
        justify: false,
    },
];

const COMMON: Style = Style {
    body: "",
    mono: "Noto Sans Mono",
    size: 16.0,
    line: 1.5,
    justify: false,
    measure: 52.0,
    weights: Weights {
        regular: 400,
        bold: 700,
        thickened: 600,
    },
    headings: HEADINGS,
    spacing: Spacing {
        block: 0.7,
        tight: 0.25,
        heading_extra: 0.5,
    },
    code: Code {
        scale: 1.0,
        block: 0.88,
        inline: 0.92,
        line_follows_text: true,
    },
    maths: Maths {
        scale: 1.1,
        display_centred: true,
        rise: 0.8,
        drop: 0.25,
        numbered: false,
    },
    list: List {
        indent: 1.5,
        marker_gap: 0.45,
        bullets: ["•", "◦"],
        number_suffix: ".",
        tasks: ["☐", "☑"],
        marker_ink: Ink::Muted,
        justify: false,
    },
    quote: Quote {
        indent: 1.1,
        bar_at: 0.0,
        bar_width: 3.0,
        bar_radius: 1.5,
        bar_ink: Ink::Rule,
        ink: Ink::Text,
        justify: false,
    },
    plate_pad: 0.5,
    table: Table {
        cell_pad: 0.4,
        head_weight: 700,
        head_rule: 1.5,
        head_rule_ink: Ink::Muted,
        row_rule: 1.0,
        row_rule_ink: Ink::Rule,
        least_column: 2.0,
        narrowest: 8.0,
        centred: true,
        spare: 1.0,
    },
    rule: HorizontalRule {
        at: 0.4,
        height: 0.8,
        thickness: 1.0,
    },
    figures: Figures {
        image_width: 520.0,
        image_height: 24.0,
        chart_width: 42.0,
        chart_label: 0.8,
        diagram_height: 40.0,
    },
    links_bold: true,
    inks: Inks {
        body: Ink::Text,
        headings: Ink::Strong,
        bold: Ink::Strong,
        table_head: Ink::Strong,
        code: Ink::Text,
    },
};

/// The Paper view: Libertinus Serif at 20 px, 1.5 apart, justified, 48 ems
/// wide; code smaller under the serif
pub const fn paper() -> Style {
    Style {
        body: "Libertinus Serif",
        size: 20.0,
        justify: true,
        measure: 48.0,
        code: Code {
            scale: 0.86,
            ..COMMON.code
        },
        list: List {
            justify: true,
            ..COMMON.list
        },
        quote: Quote {
            justify: true,
            ..COMMON.quote
        },
        ..COMMON
    }
}

/// The Text view: the interface's sans-serif at 16 px, 1.5 apart, ragged
/// right, 52 ems wide
pub const fn text() -> Style {
    COMMON
}

/// What [`Style::gap`] needs to know of a block
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Block {
    pub heading: bool,
    /// A list item's first block (it has the item's bullet or number), and
    /// how many lists deep it is
    pub item: Option<u16>,
    /// The first item of its list
    pub first: bool,
}

impl Style {
    /// Heading `level`'s style (1 to 6; others as 6)
    pub fn heading(&self, level: u8) -> Heading {
        self.headings[usize::from(level.clamp(1, 6)) - 1]
    }

    /// Heading `level`'s size with the text at `size`, unrounded
    pub fn heading_size(&self, level: u8, size: f32) -> f32 {
        size * self.heading(level).scale
    }

    /// The room above `next` after `previous`, in ems: list items close
    /// together, at any depth, headings with more room above
    pub fn gap(&self, previous: Block, next: Block) -> f32 {
        let tight = match (previous.item, next.item) {
            (Some(before), Some(depth)) => !(next.first && depth <= before),
            _ => false,
        };
        let gap = if tight {
            self.spacing.tight
        } else {
            self.spacing.block
        };
        if next.heading {
            gap + self.spacing.heading_extra
        } else {
            gap
        }
    }

    /// A code block's size with the text at `size`
    pub fn code_block_size(&self, size: f32) -> f32 {
        size * self.code.block * self.code.scale
    }

    /// Inline code's size with the text at `size`
    pub fn inline_code_size(&self, size: f32) -> f32 {
        size * self.code.inline * self.code.scale
    }
}

/// The choices the views' settings offer, as the terminal offers them
pub mod choices {
    pub const PAPER_SIZES: [u16; 9] = [14, 15, 16, 17, 18, 19, 20, 21, 22];
    pub const TEXT_SIZES: [u16; 8] = [12, 13, 14, 15, 16, 17, 18, 20];
    /// Line spacings, in hundredths
    pub const LINE_SPACINGS: [u16; 5] = [120, 130, 140, 150, 160];
    /// The Paper view's measure, in ems
    pub const PAPER_MEASURES: [u16; 6] = [28, 32, 36, 40, 48, 60];
    /// Formulas against the text, in hundredths
    pub const MATH_SCALES: [u16; 6] = [90, 100, 110, 121, 130, 150];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_views_are_the_terminals() {
        let (p, t) = (paper(), text());
        assert_eq!(
            (p.body, p.size, p.line, p.justify, p.measure),
            ("Libertinus Serif", 20.0, 1.5, true, 48.0)
        );
        assert_eq!(
            (t.body, t.size, t.line, t.justify, t.measure),
            ("", 16.0, 1.5, false, 52.0)
        );
        assert!((p.code_block_size(20.0) - 15.136).abs() < 1e-4);
        assert!((t.code_block_size(16.0) - 14.08).abs() < 1e-4);
        assert!((p.inline_code_size(20.0) - 15.824).abs() < 1e-4);
        assert!((p.heading_size(2, 20.0) - 26.4).abs() < 1e-4);
        assert_eq!(p.heading(5).line, 1.4);
    }

    #[test]
    fn gaps() {
        let s = paper();
        let para = Block::default();
        let item = |d| Block {
            heading: false,
            item: Some(d),
            first: false,
        };
        let first = |d| Block {
            heading: false,
            item: Some(d),
            first: true,
        };
        let heading = Block {
            heading: true,
            item: None,
            first: false,
        };
        assert_eq!(s.gap(para, item(1)), 0.7);
        assert_eq!(s.gap(item(1), item(1)), 0.25);
        assert_eq!(s.gap(item(1), first(2)), 0.25, "a nested list's first item");
        assert_eq!(s.gap(item(2), item(1)), 0.25, "and back out");
        assert_eq!(
            s.gap(item(2), first(1)),
            0.7,
            "a new list after a nested one"
        );
        assert_eq!(s.gap(item(1), first(1)), 0.7, "a new list after another");
        assert_eq!(s.gap(para, heading), 1.2);
        assert_eq!(s.gap(heading, para), 0.7);
    }
}
