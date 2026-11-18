// SPDX-License-Identifier: GPL-3.0-only

//! The content boxes: what household-kit's content model
//! (`household-content`) looks like on screen. Stage 2, step M2: the inline
//! text box and the formula box.
//!
//! - [`InlineText`] ([`inline_text`]): one box for every run of inline
//!   text that is not a document — a tab's title, a list row's title, a
//!   label. Its content is an [`InlineContent`], read with the same inline
//!   rules as a message's text (Juliet's decision D2): emphasis, code spans,
//!   links, maths. In a one-line control a block's syntax stays as written
//!   and a link is drawn as text. It is one line ([`Lines::One`]), at most a
//!   number of lines, or as many as its text wraps to; a limited box ends
//!   in an ellipsis that never falls inside a formula, and reports the loss
//!   (decision D3).
//! - [`Formula`] ([`formula`], [`display_formula`]): LaTeX set by RaTeX, in
//!   a line of text with its baseline, or as a display block that fits its
//!   width. A formula that can't be set shows its source as written, in the
//!   error tone.
//!
//! Both are queryable boxes under [`crate::layout`]'s contract: their
//! sizes, baselines and losses are pure queries of prepared content at an
//! offered width ([`InlineText::metrics`], [`Formula::metrics`]), and
//! [`InlineText::child`] and [`Formula::child`] put them in a layout. Each
//! reports what it actually draws, not its layout box: a native traversal
//! sees the text's or formula's own bounds
//! ([`Operation::text`](cosmic::iced::advanced::widget::Operation::text)),
//! which is what an information card anchors to.
//!
//! **Formulas on whole pixels.** wgpu rasterises an SVG to `ceil(scale ×
//! size)` pixels and draws it in a quad whose corners it rounds, with the
//! nearest texel; where the quad comes out a pixel smaller than the raster,
//! a middle row of the formula is never drawn, and that row can be a bar of
//! its "=" (Cerea's fix `d50e53ab`). Every formula here is drawn through
//! [`on_pixels`], at every scale.
//!
//! Text sizes come from the role ([`TextRole`]): hosts never pass sizes or
//! formula scales. Formulas are set at the views' formula scale against
//! the text (household-type's `maths.scale`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::{Arc, LazyLock, Mutex};

use cosmic::Renderer;
use cosmic::font::Font;
use cosmic::iced::advanced::graphics::text::cosmic_text::{self, Attrs, Buffer, Metrics, Shaping};
use cosmic::iced::advanced::graphics::text::{font_system, to_attributes};
use cosmic::iced::advanced::svg;
use cosmic::iced::core::svg::Renderer as _;
use cosmic::iced::{Color, Rectangle, Size};

pub use household_content::inline::{InlineContent, InlineRun};

#[path = "content_formula.rs"]
mod formula_box;
#[path = "content_inline.rs"]
mod inline_box;

pub use formula_box::{Formula, FormulaMetrics, display_formula, formula};
pub use inline_box::{InlineMetrics, InlineText, LineMetrics, inline_text, inline_text_shared};

/// The information a shortened inline text loses, in its source reports
/// (`layout::Child::report_information`): its full text belongs in the
/// information card
pub const TEXT: crate::layout::InformationId = crate::layout::InformationId("content.text");

/// What a run of text is in the interface, which decides its type: the
/// COSMIC type ramp (`cosmic::widget::text`'s sizes, line heights and
/// weights)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextRole {
    /// Ordinary interface text: a tab's title, a list row's title
    #[default]
    Body,
    /// Body text in semibold: a selected or hovered tab's title
    BodyEmphasis,
    /// Body text in bold, as `cosmic::widget::text::heading`
    Heading,
    Caption,
    CaptionHeading,
    /// A page's or panel's title, 1 (largest) to 4; any other as 4
    Title(u8),
}

/// A role's type
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Type {
    pub size: f32,
    pub line: f32,
    pub font: Font,
}

impl TextRole {
    pub(crate) fn typography(self) -> Type {
        let (size, line, font) = match self {
            TextRole::Body => (14.0, 21.0, cosmic::font::default()),
            TextRole::BodyEmphasis => (14.0, 21.0, cosmic::font::semibold()),
            TextRole::Heading => (14.0, 21.0, cosmic::font::bold()),
            TextRole::Caption => (12.0, 17.0, cosmic::font::default()),
            TextRole::CaptionHeading => (12.0, 17.0, cosmic::font::semibold()),
            TextRole::Title(1) => (35.0, 52.0, cosmic::font::semibold()),
            TextRole::Title(2) => (29.0, 43.0, cosmic::font::semibold()),
            TextRole::Title(3) => (24.0, 36.0, cosmic::font::bold()),
            TextRole::Title(_) => (20.0, 30.0, cosmic::font::bold()),
        };
        Type { size, line, font }
    }
}

/// The views' typography this box shares (household rule 33's defaults):
/// formulas' size against the text, and how far a line grows for one
pub(crate) fn style() -> household_type::Style {
    household_type::text()
}

/// How many lines an inline text may take
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Lines {
    /// One line, never wrapped: a tab's or a row's title. Its formulas are
    /// scaled down to fit the line (D3); what does not fit ends in an
    /// ellipsis
    #[default]
    One,
    /// Wrapped onto at most this many lines (at least 1), the last ending
    /// in an ellipsis if the text goes on
    AtMost(u16),
    /// Wrapped onto as many lines as it needs; a line holding a tall
    /// formula grows, keeping the text's baseline centred
    Unlimited,
}

/// `bounds` on whole pixels, in a window of `scale` physical pixels to a
/// logical one: its corners rounded to pixels, its size the raster's.
///
/// wgpu rasterises an SVG to `ceil(scale × size)` pixels, rounds the corners
/// of the quad it draws it in to whole pixels and takes the nearest texel,
/// so a size between pixels can give a quad a pixel smaller than the raster,
/// and then the raster's middle row or column is never drawn. A formula's
/// middle row can be the upper bar of its "=", which then reads "−". With
/// the whole size the raster's, every pixel of it is drawn (Cerea's
/// `math::on_pixels`, `d50e53ab`).
pub fn on_pixels(bounds: Rectangle, scale: f32) -> Rectangle {
    let whole = |at: f32| (at * scale).round() / scale;
    let size = |length: f32| {
        let pixels = (length * scale).ceil();
        let length = pixels / scale;
        // Dividing can land just over: wgpu would round that up a pixel
        if (length * scale).ceil() > pixels {
            length.next_down()
        } else {
            length
        }
    };
    Rectangle {
        x: whole(bounds.x),
        y: whole(bounds.y),
        width: size(bounds.width),
        height: size(bounds.height),
    }
}

/// A formula set by RaTeX at one size, in black: drawn in any colour by
/// tinting (`Svg::color`), as a formula has one colour
#[derive(Debug)]
pub(crate) struct Set {
    handle: svg::Handle,
    pub width: f32,
    pub height: f32,
    /// From the top to the baseline
    pub baseline: f32,
    pub latex: String,
    pub display: bool,
}

/// The clear margin `household_content::math::draw` leaves around every
/// formula, in pixels: not ink, so it need not fit in a line
pub(crate) const PAD: f32 = 1.0;

/// The most SVG the formula cache holds, in bytes
const FORMULA_BUDGET: usize = 16 << 20;

static FORMULAS: LazyLock<Mutex<household_content::math::FormulaCache<Set>>> =
    LazyLock::new(|| Mutex::new(household_content::math::FormulaCache::new(FORMULA_BUDGET)));

/// Sizes go to the cache in quarter pixels: set them there
pub(crate) fn quarter(size: f32) -> f32 {
    (size * 4.0).round() / 4.0
}

/// `latex` set at `size` pixels per em (in quarters), from the cache, or
/// why it can't be. RaTeX takes well under a millisecond, so this is done
/// while laying out
pub(crate) fn set(latex: &str, display: bool, size: f32) -> Result<Arc<Set>, String> {
    use household_content::math::{FormulaKey, draw};
    let size = quarter(size).max(0.25);
    let key = FormulaKey::new(latex, display, size, [0, 0, 0]);
    let mut cache = FORMULAS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(found) = cache.get(&key) {
        return found;
    }
    let result = draw(latex, display, size, [0, 0, 0]);
    let bytes = result.as_ref().map_or(0, |drawn| drawn.svg.len());
    let result = result.map(|drawn| {
        Arc::new(Set {
            handle: svg::Handle::from_memory(drawn.svg.into_bytes()),
            width: drawn.width,
            height: drawn.height,
            baseline: drawn.baseline,
            latex: latex.to_string(),
            display,
        })
    });
    cache.insert(key, result.clone(), bytes);
    result
}

/// `formula` drawn in `bounds` (logical, as laid out) on whole pixels, in
/// `color`
pub(crate) fn draw_formula(
    renderer: &mut Renderer,
    formula: &Set,
    bounds: Rectangle,
    clip: Rectangle,
    scale: f32,
    color: Color,
) {
    let drawn = on_pixels(bounds, scale);
    testing::note(formula, bounds, drawn, scale);
    renderer.draw_svg(
        svg::Svg::new(formula.handle.clone()).color(color),
        drawn,
        clip,
    );
}

/// The error tone a formula's source is shown in when it can't be set
pub(crate) fn error_color() -> Color {
    Color::from(cosmic::theme::active().cosmic().destructive_text_color())
}

/// Shaped text kept across rebuilds of the same view, so that a title is
/// shaped once, not on every frame: by everything the shaping depends on
pub(crate) struct ShapingCache<T> {
    entries: HashMap<u64, Arc<T>>,
}

/// How many shaped texts a thread keeps before starting again
const SHAPED_KEPT: usize = 512;

impl<T> ShapingCache<T> {
    pub(crate) fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub(crate) fn get_or(&mut self, key: u64, make: impl FnOnce() -> T) -> Arc<T> {
        if let Some(found) = self.entries.get(&key) {
            return found.clone();
        }
        if self.entries.len() >= SHAPED_KEPT {
            self.entries.clear();
        }
        let made = Arc::new(make());
        self.entries.insert(key, made.clone());
        made
    }
}

/// A key for everything shaping depends on besides the caller's own:
/// the fonts loaded, the interface fonts and the error tone
pub(crate) fn shaping_key(hasher: &mut impl Hasher) {
    font_system()
        .read()
        .map(|fonts| fonts.version())
        .unwrap_or_default()
        .hash(hasher);
    cosmic::font::default().hash(hasher);
    cosmic::font::mono().hash(hasher);
    error_color().into_rgba8().hash(hasher);
}

/// A buffer of `spans`, shaped `wrap`ped at `width`
pub(crate) fn shape_spans(
    text: &str,
    spans: &[(Range<usize>, Attrs<'static>)],
    base: &Attrs<'static>,
    metrics: Metrics,
    width: Option<f32>,
    wrap: cosmic_text::Wrap,
) -> Buffer {
    let mut fonts = font_system().write().unwrap_or_else(|p| p.into_inner());
    let mut buffer = Buffer::new_empty(metrics);
    buffer.set_size(width, None);
    buffer.set_wrap(wrap);
    buffer.set_rich_text(
        spans
            .iter()
            .map(|(range, attrs)| (&text[range.clone()], attrs.clone())),
        base,
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(fonts.raw(), false);
    buffer
}

/// A font's attributes, as the text renderer sets them
pub(crate) fn attrs(font: Font) -> Attrs<'static> {
    to_attributes(font)
}

/// What the boxes draw, for tests through the real renderer: recorded on
/// this thread while [`testing::record`] runs.
#[doc(hidden)]
pub mod testing {
    use super::*;

    /// A formula drawn
    #[derive(Clone, Debug, PartialEq)]
    pub struct DrawnFormula {
        pub latex: String,
        pub display: bool,
        /// Where it was laid out, before going on whole pixels
        pub requested: Rectangle,
        /// Where it was drawn
        pub bounds: Rectangle,
        /// Its ink as laid out: `requested` without its clear margin
        pub ink: Rectangle,
        /// The baseline of it as laid out, from the top of `requested`
        pub baseline: f32,
        /// The window's physical pixels to a logical one
        pub scale: f32,
    }

    thread_local! {
        static DRAWN: RefCell<Option<Vec<DrawnFormula>>> = const { RefCell::new(None) };
    }

    /// What `run` drew, in the order drawn
    pub fn record<R>(run: impl FnOnce() -> R) -> (R, Vec<DrawnFormula>) {
        let outer = DRAWN.with(|drawn| drawn.borrow_mut().replace(Vec::new()));
        let result = run();
        let drawn = DRAWN.with(|drawn| std::mem::replace(&mut *drawn.borrow_mut(), outer));
        (result, drawn.unwrap_or_default())
    }

    pub(crate) fn note(formula: &Set, requested: Rectangle, bounds: Rectangle, scale: f32) {
        DRAWN.with(|drawn| {
            if let Some(drawn) = drawn.borrow_mut().as_mut() {
                drawn.push(DrawnFormula {
                    latex: formula.latex.clone(),
                    display: formula.display,
                    requested,
                    bounds,
                    ink: Rectangle::new(
                        cosmic::iced::Point::new(requested.x + PAD, requested.y + PAD),
                        Size::new(requested.width - 2.0 * PAD, requested.height - 2.0 * PAD),
                    ),
                    baseline: formula.baseline * requested.height / formula.height,
                    scale,
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::iced::Point;

    /// However a scale divides, the size wgpu rasterises is the size drawn
    /// (Cerea's `on_pixels_gives_the_raster_size_at_any_scale`)
    #[test]
    fn on_pixels_gives_the_raster_size_at_any_scale() {
        for scale in [1.0, 1.1, 1.25, 1.33, 1.5, 1.75, 2.0, 2.25, 3.0_f32] {
            for tenth in 1..3000 {
                let length = tenth as f32 / 10.0 + 0.03;
                let bounds = Rectangle::new(
                    Point::new(tenth as f32 * 0.37, tenth as f32 * 0.73),
                    Size::new(length, length),
                );
                let drawn = on_pixels(bounds, scale);
                let raster = (drawn.height * scale).ceil();
                let quad = (scale * (drawn.y + drawn.height)).round_ties_even()
                    - (scale * drawn.y).round_ties_even();
                assert_eq!(raster, quad, "{bounds:?} at {scale}: {drawn:?}");
                assert_eq!(raster, (length * scale).ceil(), "{bounds:?} at {scale}");
            }
        }
    }

    #[test]
    fn formulas_are_set_once_and_failures_kept() {
        let a = set("x^2", false, 15.4).unwrap();
        let b = set("x^2", false, 15.4).unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        assert!(a.baseline > 0.0 && a.baseline < a.height);
        assert!(set(r"\frac{1}{", false, 15.4).is_err());
    }

    /// The margin is the kit's: the same at every size, so a formula twice
    /// the size is twice as wide less one margin each side
    #[test]
    fn the_clear_margin_is_the_kits() {
        let one = set(r"x^2 + y^2 = z^2", false, 16.0).unwrap();
        let two = set(r"x^2 + y^2 = z^2", false, 32.0).unwrap();
        let margin = (2.0 * one.width - two.width) / 2.0;
        assert!((margin - PAD).abs() < 0.01, "{margin}");
    }
}
