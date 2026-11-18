// SPDX-License-Identifier: GPL-3.0-only

//! The inline text box: inline content shaped with cosmic-text, its
//! formulas set by RaTeX and drawn where a placeholder holds their room.
//!
//! A formula is a single unbreakable cluster in shaping: a letter drawn
//! transparent, whose letter spacing makes it as wide as the formula (not a
//! space, which cosmic-text hangs past a line's end). In one line the
//! formula is scaled down until it fits the line about the text's baseline
//! (D3); where the text wraps the line grows instead, by twice the larger
//! of what the formula rises above `maths.rise` and drops below
//! `maths.drop`, so the line stays centred on its text (the views' rule).
//!
//! A limited box that does not fit ends in an ellipsis after the last
//! cluster that fits with it. A formula is one cluster, so the ellipsis
//! comes before it or after it, never through it.

use super::*;
use crate::layout::{self, InformationLoss, LossKind, MeasuredSize, SourceId};
use cosmic::iced::advanced::graphics::text::Raw;
use cosmic::iced::advanced::widget::{Operation, Tree, tree};
use cosmic::iced::advanced::{Layout, Widget, layout as native, mouse, renderer};
use cosmic::iced::core::renderer::Renderer as _;
use cosmic::iced::core::text::Renderer as _;
use cosmic::iced::{Background, Length, Point, Vector};
use cosmic::{Element, Theme};
use cosmic_text::{AttrsOwned, Weight, Wrap};
use household_content::display::{Look, Object};

const ELLIPSIS: &str = "\u{2026}";
/// What holds a formula's room in the text
const PLACEHOLDER: &str = "x";
/// The metadata of struck-through glyphs (formulas' are their number, from 1)
const STRUCK: usize = usize::MAX;
const EPSILON: f32 = 0.01;

/// Inline content in a text role: see the [module](super) documentation
#[derive(Clone, Debug)]
pub struct InlineText<'a> {
    content: &'a InlineContent,
    role: TextRole,
    lines: Lines,
    width: Length,
    source: Option<(SourceId, u64)>,
}

/// `content` set in `role`, on one line
pub fn inline_text(content: &InlineContent, role: TextRole) -> InlineText<'_> {
    InlineText {
        content,
        role,
        lines: Lines::One,
        width: Length::Shrink,
        source: None,
    }
}

/// A line's box, from the top of the text: where it starts, its baseline,
/// and how far it reaches above and below the baseline (a tall formula's
/// included)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LineMetrics {
    pub top: f32,
    pub baseline: f32,
    pub ascent: f32,
    pub descent: f32,
}

/// What an inline text takes at a width: a pure query of its prepared
/// content
#[derive(Clone, Debug, PartialEq)]
pub struct InlineMetrics {
    /// The widest line and all the lines: what is drawn
    pub size: Size,
    pub lines: usize,
    pub first_line: LineMetrics,
    pub last_line: LineMetrics,
    /// The source not shown, when the text was shortened: from the first
    /// byte cut away to the end
    pub lost: Option<Range<usize>>,
    /// The formulas drawn
    pub formulas: usize,
}

impl<'a> InlineText<'a> {
    pub fn lines(mut self, lines: Lines) -> Self {
        if let Lines::AtMost(n) = lines {
            assert!(n >= 1, "an inline text takes at least one line");
        }
        self.lines = lines;
        self
    }

    /// The width of the box: [`Length::Shrink`] (the text's, by default) or
    /// [`Length::Fill`]. Either way it reports the text's own bounds as
    /// what it draws
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Report a shortened text's loss ([`super::TEXT`]) for the source
    /// `source` at `revision`, in [`Self::child`]'s plans
    pub fn report(mut self, source: SourceId, revision: u64) -> Self {
        self.source = Some((source, revision));
        self
    }

    /// What the text takes at `width` (or unbounded), and what it loses
    pub fn metrics(&self, width: Option<f32>) -> InlineMetrics {
        self.shaped(width).metrics.clone()
    }

    /// The narrowest useful width: an ellipsis for a limited text; the
    /// widest word or formula for one that wraps
    pub fn minimum_width(&self) -> f32 {
        let natural = self.metrics(None).size.width;
        let floor = match self.lines {
            Lines::Unlimited => self.shaped_with(Some(0.0), Wrap::Word).metrics.size.width,
            _ => width_of(
                ELLIPSIS,
                &attrs(self.role.typography().font),
                self.role.typography(),
            )
            .ceil(),
        };
        floor.min(natural)
    }

    /// The loss at `width`, as a source report states it
    pub fn losses(&self, width: f32) -> Vec<InformationLoss> {
        match self.metrics(Some(width)).lost {
            Some(_) => vec![InformationLoss {
                identity: super::TEXT,
                kind: LossKind::Shortened,
            }],
            None => Vec::new(),
        }
    }

    /// The box in a layout: measured from its content (its natural width
    /// preferred, [`Self::minimum_width`] the least), with its loss
    /// reported when [`Self::report`] named the source
    pub fn child<M: 'a>(self) -> layout::Child<'a, M> {
        let view = self.clone();
        let query = self.clone();
        let child =
            layout::child(move |_| Element::from(view.clone())).with_measurement(move |_, _| {
                let natural = query.metrics(None).size;
                let minimum = query.minimum_width();
                let height = query.metrics(Some(minimum)).size.height;
                MeasuredSize {
                    preferred: natural,
                    minimum: Size::new(minimum, height.min(natural.height)),
                }
            });
        match self.source {
            Some((source, revision)) => {
                child.report_information(source, revision, move |_, plan| {
                    self.losses(plan.size.width)
                })
            }
            None => child,
        }
    }

    fn wrap(&self) -> Wrap {
        match self.lines {
            Lines::One => Wrap::None,
            _ => Wrap::WordOrGlyph,
        }
    }

    pub(crate) fn shaped(&self, width: Option<f32>) -> Arc<Shaped> {
        self.shaped_with(width, self.wrap())
    }

    fn shaped_with(&self, width: Option<f32>, wrap: Wrap) -> Arc<Shaped> {
        thread_local! {
            static SHAPED: RefCell<ShapingCache<Shaped>> = RefCell::new(ShapingCache::new());
        }
        let mut hasher = std::hash::DefaultHasher::new();
        self.content.source().hash(&mut hasher);
        self.content.shown().hash(&mut hasher);
        for run in self.content.runs() {
            (&run.src, &run.shown, run.look, &run.object, &run.link).hash(&mut hasher);
        }
        (self.role, self.lines, width.map(f32::to_bits)).hash(&mut hasher);
        (wrap as u8).hash(&mut hasher);
        shaping_key(&mut hasher);
        let key = hasher.finish();
        SHAPED.with(|cache| cache.borrow_mut().get_or(key, || self.shape(width, wrap)))
    }

    /// The content shaped at `width`: formulas fitted, then shortened
    fn shape(&self, width: Option<f32>, wrap: Wrap) -> Shaped {
        let ty = self.role.typography();
        let limit = match self.lines {
            Lines::One => Some(1),
            Lines::AtMost(n) => Some(usize::from(n)),
            Lines::Unlimited => None,
        };
        let shape_width = if wrap == Wrap::None { None } else { width };
        let mut fit = Vec::new();
        let mut assembly = self.assemble(ty, &fit);
        let mut buffer = assembly.shape(ty, shape_width, wrap);
        // D3: in one line a formula is made smaller until it fits about the
        // baseline; sizes go in quarter pixels, so a few rounds settle it
        if self.lines == Lines::One {
            for _ in 0..4 {
                let Some(next) = assembly.fit(&buffer, &fit) else {
                    break;
                };
                fit = next;
                assembly = self.assemble(ty, &fit);
                buffer = assembly.shape(ty, shape_width, wrap);
            }
        }
        let mut lost = None;
        if let (Some(limit), Some(width)) = (limit, width)
            && let Some((short, cut)) = assembly.shorten(&buffer, ty, limit, width, wrap)
        {
            lost = Some(cut..self.content.source().len());
            buffer = short.shape(ty, shape_width, wrap);
            assembly = short;
        }
        Shaped::new(assembly, buffer, ty, lost)
    }

    /// The content as spans of text with their attributes: formulas set at
    /// the maths size times their `fit`, a newline a space in one line
    fn assemble(&self, ty: Type, fit: &[f32]) -> Assembly {
        let base = attrs(ty.font);
        let mono = attrs(cosmic::font::mono());
        let style = style();
        let maths_size = ty.size * style.maths.scale;
        let code_size = style.inline_code_size(ty.size);
        let one_line = self.lines == Lines::One;
        let source = self.content.source();
        let shown = self.content.shown();
        let mut assembly = Assembly::default();
        for run in self.content.runs() {
            let look_attrs = look(&run.look, &base, &mono, ty, code_size);
            match &run.object {
                Some(Object::Math(latex)) => {
                    let factor = fit.get(assembly.slots).copied().unwrap_or(1.0);
                    assembly.slots += 1;
                    let size = if factor < 1.0 {
                        (maths_size * factor * 4.0).floor() / 4.0
                    } else {
                        maths_size
                    };
                    match set(latex, false, size) {
                        Ok(formula) => {
                            let mut attrs = formula_attrs(&run.look, &base, &formula, ty);
                            if !one_line {
                                let extra = growth(&formula, ty, &style);
                                if extra > 0.0 {
                                    attrs = attrs.metrics(Metrics::new(ty.size, ty.line + extra));
                                }
                            }
                            assembly.push_formula(formula, attrs, run.src.clone());
                        }
                        // Its source as written, in the error tone
                        Err(_) => assembly.push_text(
                            &source[run.src.clone()],
                            mono.clone()
                                .metrics(Metrics::new(code_size, ty.line))
                                .color(cosmic::iced::advanced::graphics::text::to_color(
                                    error_color(),
                                )),
                            run.src.clone(),
                            true,
                            one_line,
                        ),
                    }
                }
                _ => {
                    let text = &shown[run.shown.clone()];
                    let linear = text == &source[run.src.clone()];
                    assembly.push_text(text, look_attrs, run.src.clone(), linear, one_line);
                }
            }
        }
        assembly.source_end = source.len();
        assembly
    }
}

/// How much taller a wrapped line is made for `formula`: twice the larger
/// of how far it rises above `maths.rise` and drops below `maths.drop`
fn growth(formula: &Set, ty: Type, style: &household_type::Style) -> f32 {
    let above = formula.baseline - ty.size * style.maths.rise;
    let below = (formula.height - formula.baseline) - ty.size * style.maths.drop;
    2.0 * above.max(below).max(0.0)
}

/// A run's attributes. Links are drawn as text; `==mark==` and `<u>` are
/// read but drawn plainly until the document boxes draw them (M3)
fn look(
    look: &Look,
    base: &Attrs<'static>,
    mono: &Attrs<'static>,
    ty: Type,
    code_size: f32,
) -> Attrs<'static> {
    let mut attrs = if look.code || look.mono {
        mono.clone().metrics(Metrics::new(code_size, ty.line))
    } else {
        base.clone()
    };
    if look.bold {
        attrs = attrs.weight(Weight::BOLD);
    }
    if look.italic {
        attrs = attrs.style(cosmic_text::Style::Italic);
    }
    if look.strike {
        attrs = attrs.metadata(STRUCK);
    }
    attrs
}

/// The placeholder for `formula`: transparent, as wide as the formula
fn formula_attrs(look: &Look, base: &Attrs<'static>, formula: &Set, ty: Type) -> Attrs<'static> {
    let mut attrs = base.clone();
    if look.bold {
        attrs = attrs.weight(Weight::BOLD);
    }
    if look.italic {
        attrs = attrs.style(cosmic_text::Style::Italic);
    }
    let placeholder = width_of(PLACEHOLDER, &attrs, ty);
    attrs
        .color(cosmic_text::Color::rgba(0, 0, 0, 0))
        .letter_spacing((formula.width - placeholder) / ty.size)
}

/// How wide `text` is set in `attrs` at `ty`'s size, as one line: for the
/// ellipsis and the placeholder, kept by what they are set in
fn width_of(text: &'static str, attrs: &Attrs<'static>, ty: Type) -> f32 {
    thread_local! {
        static WIDTHS: RefCell<ShapingCache<f32>> = RefCell::new(ShapingCache::new());
    }
    let mut hasher = std::hash::DefaultHasher::new();
    text.hash(&mut hasher);
    AttrsOwned::new(attrs).hash(&mut hasher);
    (ty.size.to_bits(), ty.line.to_bits()).hash(&mut hasher);
    shaping_key(&mut hasher);
    let key = hasher.finish();
    let width = WIDTHS.with(|widths| {
        widths.borrow_mut().get_or(key, || {
            let spans = [(0..text.len(), attrs.clone())];
            shape_spans(
                text,
                &spans,
                attrs,
                Metrics::new(ty.size, ty.line),
                None,
                Wrap::None,
            )
            .layout_runs()
            .map(|run| run.line_w)
            .fold(0.0, f32::max)
        })
    });
    *width
}

/// A piece of the text shaped: its range in the text and in the source
#[derive(Clone, Debug)]
struct Span {
    text: Range<usize>,
    src: Range<usize>,
    /// Its text is its source, byte for byte
    linear: bool,
    attrs: Attrs<'static>,
}

/// The text to shape, its spans, and its formulas by number
#[derive(Clone, Debug, Default)]
struct Assembly {
    text: String,
    spans: Vec<Span>,
    /// Formulas set, numbered from 1 in their placeholders' metadata
    formulas: Vec<Arc<Set>>,
    /// Formula objects met, set or not (a formula's fit goes by this)
    slots: usize,
    /// Each formula's object number
    formula_slots: Vec<usize>,
    source_end: usize,
}

impl Assembly {
    fn push_text(
        &mut self,
        text: &str,
        attrs: Attrs<'static>,
        src: Range<usize>,
        linear: bool,
        one_line: bool,
    ) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        // A newline is a space in one line; a carriage return always is,
        // so that the text's lines are its newlines (same lengths)
        for c in text.chars() {
            match c {
                '\n' if one_line => self.text.push(' '),
                '\r' => self.text.push(' '),
                c => self.text.push(c),
            }
        }
        self.spans.push(Span {
            text: start..self.text.len(),
            src,
            linear,
            attrs,
        });
    }

    fn push_formula(&mut self, formula: Arc<Set>, attrs: Attrs<'static>, src: Range<usize>) {
        self.formulas.push(formula);
        self.formula_slots.push(self.slots - 1);
        let start = self.text.len();
        self.text.push_str(PLACEHOLDER);
        self.spans.push(Span {
            text: start..self.text.len(),
            src,
            linear: false,
            attrs: attrs.metadata(self.formulas.len()),
        });
    }

    fn shape(&self, ty: Type, width: Option<f32>, wrap: Wrap) -> Buffer {
        let spans: Vec<_> = self
            .spans
            .iter()
            .map(|span| (span.text.clone(), span.attrs.clone()))
            .collect();
        shape_spans(
            &self.text,
            &spans,
            &attrs(ty.font),
            Metrics::new(ty.size, ty.line),
            width,
            wrap,
        )
    }

    /// The formulas' scales for one line, if one does not fit about the
    /// baseline: each made smaller by how much its ink overreaches (its
    /// clear margin may lie outside the line)
    fn fit(&self, buffer: &Buffer, fit: &[f32]) -> Option<Vec<f32>> {
        let mut next: Vec<f32> = (0..self.slots)
            .map(|slot| fit.get(slot).copied().unwrap_or(1.0))
            .collect();
        let mut changed = false;
        for run in buffer.layout_runs() {
            let above = run.line_y - run.line_top;
            let below = run.line_top + run.line_height - run.line_y;
            for glyph in run.glyphs {
                let Some(formula) = self.formula_of(glyph.metadata) else {
                    continue;
                };
                // Its ink, which scales with it; the margin is clear
                let rise = (formula.baseline - PAD).max(EPSILON);
                let drop = (formula.height - formula.baseline - PAD).max(EPSILON);
                let ratio = (above / rise).min(below / drop).min(1.0);
                if ratio < 1.0 - 1e-4 {
                    let slot = self.formula_slots[glyph.metadata - 1];
                    next[slot] *= ratio;
                    changed = true;
                }
            }
        }
        changed.then_some(next)
    }

    fn formula_of(&self, metadata: usize) -> Option<&Arc<Set>> {
        (metadata >= 1 && metadata != STRUCK)
            .then(|| self.formulas.get(metadata - 1))
            .flatten()
    }

    /// The text shortened to `limit` lines of `width`, ending in an
    /// ellipsis, and the source offset from which it is not shown; `None`
    /// if it fits
    fn shorten(
        &self,
        buffer: &Buffer,
        ty: Type,
        limit: usize,
        width: f32,
        wrap: Wrap,
    ) -> Option<(Assembly, usize)> {
        let fits = |buffer: &Buffer| {
            let mut lines = 0;
            for run in buffer.layout_runs() {
                lines += 1;
                if run.line_w > width + EPSILON {
                    return false;
                }
            }
            lines <= limit
        };
        if fits(buffer) {
            return None;
        }
        let shape_width = if wrap == Wrap::None {
            None
        } else {
            Some(width)
        };
        // The last line kept starts here
        let starts = line_starts(&self.text);
        let tail_start = if limit == 1 {
            0
        } else {
            buffer
                .layout_runs()
                .nth(limit - 1)
                .map(|run| {
                    starts[run.line_i] + run.glyphs.iter().map(|g| g.start).min().unwrap_or(0)
                })
                .unwrap_or(0)
        };
        // Where the last line could be cut: before each cluster, and where
        // the text in it so far ends
        let tail = self.slice(tail_start..self.text.len(), true);
        let tail_buffer = tail.shape(ty, None, Wrap::None);
        let mut cuts: Vec<(usize, f32)> = Vec::new();
        for run in tail_buffer.layout_runs() {
            let mut previous = None;
            for glyph in run.glyphs {
                if previous != Some(glyph.start) {
                    cuts.push((glyph.start, glyph.x));
                }
                previous = Some(glyph.start);
            }
        }
        cuts.sort_by_key(|(at, _)| *at);
        let ellipsis = width_of(ELLIPSIS, &attrs(ty.font), ty);
        // The longest that fits with its ellipsis, then shorter while the
        // whole does not fit as shaped; at the narrowest, the ellipsis alone
        // (overflowing a width narrower than it)
        let first = cuts.first().map_or(0, |(at, _)| *at);
        let candidates = cuts
            .iter()
            .rev()
            .filter(|(_, x)| x + ellipsis <= width + EPSILON)
            .map(|(at, _)| (*at, false))
            .chain(std::iter::once((first, true)));
        for (cut, last_resort) in candidates {
            let at = tail_start + cut;
            let mut short = self.slice(0..at, false);
            short.trim_end();
            let attrs = short
                .spans
                .last()
                .filter(|span| !span.text.is_empty() && short.formula_at(span).is_none())
                .map_or_else(|| attrs(ty.font), |span| span.attrs.clone());
            let src = self.source_at(at);
            let start = short.text.len();
            short.text.push_str(ELLIPSIS);
            short.spans.push(Span {
                text: start..short.text.len(),
                src: src..src,
                linear: false,
                attrs: attrs.metadata(0),
            });
            if last_resort || fits(&short.shape(ty, shape_width, wrap)) {
                return Some((short, src));
            }
        }
        None
    }

    fn formula_at(&self, span: &Span) -> Option<usize> {
        let metadata = span.attrs.metadata;
        self.formula_of(metadata).map(|_| metadata)
    }

    /// The source offset of text offset `at`
    fn source_at(&self, at: usize) -> usize {
        for span in &self.spans {
            if span.text.start <= at && at < span.text.end {
                return if span.linear {
                    span.src.start + (at - span.text.start)
                } else {
                    span.src.start
                };
            }
        }
        self.source_end
    }

    /// The text in `range` (on character boundaries), its newlines spaces
    /// if `one_line`; formulas keep their numbers
    fn slice(&self, range: Range<usize>, one_line: bool) -> Assembly {
        let mut out = Assembly {
            formulas: self.formulas.clone(),
            slots: self.slots,
            formula_slots: self.formula_slots.clone(),
            source_end: self.source_end,
            ..Assembly::default()
        };
        for span in &self.spans {
            let start = span.text.start.max(range.start);
            let end = span.text.end.min(range.end);
            if start >= end {
                continue;
            }
            let text = &self.text[start..end];
            let at = out.text.len();
            if one_line {
                out.text
                    .extend(text.chars().map(|c| if c == '\n' { ' ' } else { c }));
            } else {
                out.text.push_str(text);
            }
            let src = if span.linear {
                span.src.start + (start - span.text.start)..span.src.start + (end - span.text.start)
            } else {
                span.src.clone()
            };
            out.spans.push(Span {
                text: at..out.text.len(),
                src,
                linear: span.linear,
                attrs: span.attrs.clone(),
            });
        }
        out
    }

    /// Without trailing white space
    fn trim_end(&mut self) {
        let keep = self.text.trim_end().len();
        self.text.truncate(keep);
        self.spans.retain_mut(|span| {
            span.text.end = span.text.end.min(keep);
            if span.linear {
                span.src.end = span.src.start + span.text.len();
            }
            !span.text.is_empty()
        });
    }
}

/// Where each of `text`'s lines starts
fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect()
}

/// Shaped content and where its formulas and strokes go, from the top
/// left of the text
pub(crate) struct Shaped {
    buffer: Arc<Buffer>,
    pub metrics: InlineMetrics,
    formulas: Vec<(Arc<Set>, Rectangle)>,
    struck: Vec<Rectangle>,
    shown: String,
}

impl std::fmt::Debug for Shaped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shaped")
            .field("metrics", &self.metrics)
            .field("shown", &self.shown)
            .finish_non_exhaustive()
    }
}

impl Shaped {
    fn new(assembly: Assembly, buffer: Buffer, ty: Type, lost: Option<Range<usize>>) -> Self {
        let mut lines = Vec::new();
        let mut width: f32 = 0.0;
        let mut formulas = Vec::new();
        let mut struck = Vec::new();
        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            lines.push(LineMetrics {
                top: run.line_top,
                baseline: run.line_y,
                ascent: run.line_y - run.line_top,
                descent: run.line_top + run.line_height - run.line_y,
            });
            let mut stroke: Option<(f32, f32)> = None;
            for glyph in run.glyphs {
                if let Some(formula) = assembly.formula_of(glyph.metadata) {
                    formulas.push((
                        formula.clone(),
                        Rectangle::new(
                            Point::new(glyph.x, run.line_y - formula.baseline),
                            Size::new(formula.width, formula.height),
                        ),
                    ));
                }
                if glyph.metadata == STRUCK {
                    let (from, _) = stroke.unwrap_or((glyph.x, 0.0));
                    stroke = Some((from, glyph.x + glyph.w));
                } else if let Some((from, to)) = stroke.take() {
                    struck.push(strike(from, to, run.line_y, ty));
                }
            }
            if let Some((from, to)) = stroke {
                struck.push(strike(from, to, run.line_y, ty));
            }
        }
        if lines.is_empty() {
            lines.push(LineMetrics {
                top: 0.0,
                baseline: ty.line / 2.0,
                ascent: ty.line / 2.0,
                descent: ty.line / 2.0,
            });
        }
        let first = lines[0];
        let last = lines[lines.len() - 1];
        let metrics = InlineMetrics {
            size: Size::new(width.ceil(), last.baseline + last.descent),
            lines: lines.len(),
            first_line: first,
            last_line: last,
            lost,
            formulas: formulas.len(),
        };
        Shaped {
            buffer: Arc::new(buffer),
            metrics,
            formulas,
            struck,
            shown: assembly.text,
        }
    }
}

/// A stroke through text from `from` to `to`, about half its x-height
fn strike(from: f32, to: f32, baseline: f32, ty: Type) -> Rectangle {
    let thickness = (ty.size / 14.0).max(1.0);
    Rectangle::new(
        Point::new(from, baseline - ty.size * 0.3 - thickness / 2.0),
        Size::new(to - from, thickness),
    )
}

#[derive(Default)]
struct State {
    shaped: Option<Arc<Shaped>>,
}

struct InlineTextWidget<'a> {
    text: InlineText<'a>,
    #[cfg(feature = "a11y")]
    id: cosmic::iced::advanced::widget::Id,
}

impl<'a, Message: 'a> From<InlineText<'a>> for Element<'a, Message> {
    fn from(text: InlineText<'a>) -> Self {
        Element::new(InlineTextWidget {
            text,
            #[cfg(feature = "a11y")]
            id: cosmic::iced::advanced::widget::Id::unique(),
        })
    }
}

/// `at` on a whole pixel: glyph rows are cut from where the text starts
fn on_pixel(at: f32, scale: f32) -> f32 {
    (at * scale).round() / scale
}

impl<Message> Widget<Message, Theme, Renderer> for InlineTextWidget<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(self.text.width, Length::Shrink)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        _renderer: &Renderer,
        limits: &native::Limits,
    ) -> native::Node {
        let max = limits.max().width;
        let shaped = self.text.shaped(max.is_finite().then_some(max));
        let size = limits.resolve(self.text.width, Length::Shrink, shaped.metrics.size);
        tree.state.downcast_mut::<State>().shaped = Some(shaped);
        native::Node::new(size)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let Some(shaped) = &tree.state.downcast_ref::<State>().shaped else {
            return;
        };
        let bounds = layout.bounds();
        if !bounds.intersects(viewport) {
            return;
        }
        let scale = style.scale_factor as f32;
        let origin = Point::new(on_pixel(bounds.x, scale), on_pixel(bounds.y, scale));
        let color = style.text_color;
        renderer.fill_raw(Raw {
            buffer: Arc::downgrade(&shaped.buffer),
            position: origin,
            color,
            clip_bounds: *viewport,
        });
        let offset = Vector::new(origin.x, origin.y);
        for stroke in &shaped.struck {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: *stroke + offset,
                    ..renderer::Quad::default()
                },
                Background::Color(color),
            );
        }
        for (formula, rect) in &shaped.formulas {
            draw_formula(renderer, formula, *rect + offset, *viewport, scale, color);
        }
    }

    /// The text as written, all of it: a shortened title is read whole
    #[cfg(feature = "a11y")]
    fn a11y_nodes(
        &self,
        layout: Layout<'_>,
        _: &Tree,
        _: mouse::Cursor,
    ) -> iced_accessibility::A11yTree {
        a11y_text(layout.bounds(), self.text.content.source(), &self.id)
    }

    /// What it draws, not its box: the text's own bounds
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if let Some(shaped) = &tree.state.downcast_ref::<State>().shaped {
            let bounds = layout.bounds();
            let size = shaped.metrics.size;
            let drawn = Rectangle::new(
                bounds.position(),
                Size::new(size.width.min(bounds.width), size.height),
            );
            operation.text(None, drawn, &shaped.shown);
        }
    }
}

/// A text's node for assistive technology: a paragraph labelled with the
/// text as written, as iced's text has
#[cfg(feature = "a11y")]
pub(crate) fn a11y_text(
    bounds: Rectangle,
    label: &str,
    id: &cosmic::iced::advanced::widget::Id,
) -> iced_accessibility::A11yTree {
    use iced_accessibility::accesskit::{Node, Rect, Role};
    let mut node = Node::new(Role::Paragraph);
    node.set_label(label.to_string().into_boxed_str());
    node.set_bounds(Rect::new(
        f64::from(bounds.x),
        f64::from(bounds.y),
        f64::from(bounds.x + bounds.width),
        f64::from(bounds.y + bounds.height),
    ));
    iced_accessibility::A11yTree::leaf(node, id.clone())
}
