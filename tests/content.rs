// SPDX-License-Identifier: GPL-3.0-only
//! The content boxes through the actual native runtime and tiny-skia
//! renderer: inline text with maths in titles, baselines, shortening that
//! never cuts a formula and reports its loss, formulas fitted to one line,
//! wrapping, the bounds of what is drawn, and formulas on whole pixels.
#![cfg(feature = "content")]

use cosmic::iced::advanced::widget::{Id, Operation};
use cosmic::iced::advanced::{renderer, renderer::Headless};
use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
use cosmic::iced::{Font, Length, Pixels, Rectangle, Size, mouse};
use cosmic::{Element, Renderer, widget};
use household_widgets::content::testing::{DrawnFormula, record};
use household_widgets::content::{
    self, InlineContent, Lines, TextRole, display_formula, formula, inline_text,
};
use household_widgets::layout::{self, InformationLoss, LossKind, SourceId, SourceReports};

struct Window {
    renderer: Renderer,
}

impl Window {
    fn new() -> Self {
        let renderer = cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
            Font::default(),
            Pixels(14.0),
            Some("tiny-skia"),
        ))
        .expect("the actual tiny-skia renderer");
        Self { renderer }
    }

    /// `content` laid out in a window `size` big, its native tree traversed
    /// by `operation`, and drawn at `scale`: the formulas drawn
    fn show(
        &mut self,
        content: Element<'_, ()>,
        size: Size,
        operation: Option<&mut dyn Operation>,
        scale: f32,
    ) -> Vec<DrawnFormula> {
        let root = widget::container(content)
            .width(Length::Fill)
            .height(Length::Fill);
        let mut ui = UserInterface::build(root, size, Cache::default(), &mut self.renderer);
        if let Some(operation) = operation {
            ui.operate(&self.renderer, operation);
        }
        let ((), drawn) = record(|| {
            ui.draw(
                &mut self.renderer,
                &cosmic::Theme::default(),
                &renderer::Style {
                    scale_factor: f64::from(scale),
                    ..renderer::Style::default()
                },
                mouse::Cursor::Unavailable,
            )
        });
        drawn
    }
}

/// The text the native traversal reports as drawn, with its bounds
#[derive(Default)]
struct Drawn(Vec<(Rectangle, String)>);
impl Operation for Drawn {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn text(&mut self, _: Option<&Id>, bounds: Rectangle, text: &str) {
        self.0.push((bounds, text.to_string()));
    }
}

const TITLE: &str = r"Diameter of $\mathrm{SL}_n(F)$";
/// Taller than a line of body text by half (D3's own example,
/// `\frac{n+1}{2}`, is set in text style in a title: about as tall as the
/// line, it is made a little smaller to fit about the baseline)
const TALL: &str = r"Bounds for $\dfrac{n+1}{2}$";
const LONG: &str = r"Bounds for $\frac{n+1}{2}$ and the rest of a title that goes on";
const BODY_LINE: f32 = 21.0;

#[test]
fn a_title_with_maths_draws_its_formula() {
    let mut window = Window::new();
    let title = InlineContent::parse(TITLE);
    let drawn = window.show(
        inline_text(&title, TextRole::Body).into(),
        Size::new(600.0, 100.0),
        None,
        1.0,
    );
    assert_eq!(drawn.len(), 1, "{drawn:?}");
    assert_eq!(drawn[0].latex, r"\mathrm{SL}_n(F)");
    assert!(!drawn[0].display);
    // Literal content interprets nothing: no formula
    let literal = InlineContent::literal(TITLE);
    let drawn = window.show(
        inline_text(&literal, TextRole::Body).into(),
        Size::new(600.0, 100.0),
        None,
        1.0,
    );
    assert!(drawn.is_empty(), "{drawn:?}");
    // A heading's marker stays as written in a title; its maths is still maths
    let heading = InlineContent::parse(r"## Bound for $\mathrm{SL}_n$");
    let mut texts = Drawn::default();
    let drawn = window.show(
        inline_text(&heading, TextRole::Body).into(),
        Size::new(600.0, 100.0),
        Some(&mut texts),
        1.0,
    );
    assert_eq!(drawn.len(), 1);
    assert!(texts.0[0].1.starts_with("## Bound for "), "{:?}", texts.0);
}

/// An inline formula's baseline is the text's, and the formula box alone
/// measures the same formula as the text sets
#[test]
fn an_inline_formula_sits_on_the_texts_baseline() {
    let mut window = Window::new();
    let text = InlineContent::parse(r"where $x^2 + y_k$ grows");
    for role in [TextRole::Body, TextRole::Caption, TextRole::Title(2)] {
        let element = inline_text(&text, role);
        let metrics = element.metrics(None);
        let top = 10.0;
        let drawn = window.show(
            widget::container(element)
                .padding(cosmic::iced::Padding {
                    top,
                    ..cosmic::iced::Padding::ZERO
                })
                .into(),
            Size::new(600.0, 200.0),
            None,
            1.0,
        );
        assert_eq!(drawn.len(), 1, "{role:?}");
        let formula_baseline = drawn[0].requested.y + drawn[0].baseline;
        let text_baseline = top + metrics.first_line.baseline;
        assert!(
            (formula_baseline - text_baseline).abs() < 0.01,
            "{role:?}: the formula's baseline {formula_baseline}, the text's {text_baseline}"
        );
        let alone = formula(r"x^2 + y_k", role).metrics(None);
        assert!(
            (alone.baseline - drawn[0].baseline).abs() < 0.01,
            "{role:?}"
        );
        assert!(
            (alone.size.width - drawn[0].requested.width).abs() < 0.01,
            "{role:?}"
        );
        assert_eq!(metrics.lines, 1);
        assert!(metrics.first_line.ascent > 0.0 && metrics.first_line.descent > 0.0);
    }
}

/// D3: the ellipsis comes before a formula or after it, never through it;
/// what is cut away is reported as the source's loss
#[test]
fn one_line_shortening_never_cuts_a_formula_and_reports_the_loss() {
    let title = InlineContent::parse(LONG);
    let maths = title
        .runs()
        .iter()
        .find(|run| run.object.is_some())
        .expect("a formula")
        .src
        .clone();
    let text = inline_text(&title, TextRole::Body);
    let natural = text.metrics(None);
    assert!(natural.lost.is_none());
    let floor = text.minimum_width();
    let mut cut_before = 0;
    let mut cut_after = 0;
    let (mut before_width, mut after_width) = (0.0, 0.0);
    for width in (floor.ceil() as u32)..(natural.size.width.ceil() as u32 + 10) {
        let width = width as f32;
        let metrics = text.metrics(Some(width));
        assert!(metrics.size.width <= width + 0.01, "{width}: {metrics:?}");
        assert_eq!(metrics.lines, 1);
        match &metrics.lost {
            None => assert!(width >= natural.size.width, "{width}: nothing lost"),
            Some(lost) => {
                assert!(width < natural.size.width);
                assert_eq!(lost.end, LONG.len());
                assert!(
                    lost.start <= maths.start || lost.start >= maths.end,
                    "{width}: cut at {} inside the formula {maths:?}",
                    lost.start
                );
                if lost.start <= maths.start {
                    cut_before += 1;
                    before_width = width;
                    assert_eq!(metrics.formulas, 0);
                } else {
                    cut_after += 1;
                    after_width = width;
                    assert_eq!(metrics.formulas, 1);
                }
                assert_eq!(
                    text.losses(width),
                    vec![InformationLoss {
                        identity: content::TEXT,
                        kind: LossKind::Shortened,
                    }]
                );
            }
        }
    }
    assert!(cut_before > 0 && cut_after > 0, "{cut_before} {cut_after}");

    // Through the layout: the accepted plan publishes the loss, and a
    // formula that is drawn is drawn whole
    let mut window = Window::new();
    let mut whole = None;
    for (width, shortened) in [
        (natural.size.width + 40.0, false),
        (after_width, true),
        (before_width, true),
    ] {
        let mut reports = SourceReports::default();
        let child = inline_text(&title, TextRole::Body)
            .report(SourceId(7), 3)
            .child::<()>();
        let drawn = window.show(
            widget::container(layout::responsive_child(layout::Level::Preferred, child))
                .width(width)
                .into(),
            Size::new(800.0, 100.0),
            Some(&mut reports),
            1.0,
        );
        let report = reports
            .reports()
            .iter()
            .find(|accepted| accepted.report.source == SourceId(7))
            .expect("the accepted source report");
        assert_eq!(report.report.revision, 3);
        assert_eq!(!report.report.losses.is_empty(), shortened, "{width}");
        assert_eq!(drawn.len(), usize::from(width != before_width), "{width}");
        for formula in &drawn {
            let whole = *whole.get_or_insert(formula.requested.size());
            assert_eq!(formula.requested.size(), whole, "{width}");
            assert!(formula.requested.x + formula.requested.width <= width + 0.01);
        }
    }
}

/// D3: a formula taller than a one-line title is made smaller to fit its
/// line; where text wraps, the line grows instead
#[test]
fn a_tall_formula_is_scaled_to_the_line() {
    let mut window = Window::new();
    let title = InlineContent::parse(TALL);
    let natural = formula(r"\dfrac{n+1}{2}", TextRole::Body).metrics(None);
    assert!(
        natural.size.height > BODY_LINE,
        "the test needs a tall formula"
    );

    let one = inline_text(&title, TextRole::Body);
    let metrics = one.metrics(None);
    assert_eq!(metrics.size.height, BODY_LINE, "the title keeps its line");
    let drawn = window.show(one.into(), Size::new(600.0, 100.0), None, 1.0);
    assert_eq!(drawn.len(), 1);
    let f = &drawn[0];
    assert!(f.requested.height < natural.size.height, "{f:?}");
    // Its ink in the line: nothing clipped
    assert!(f.ink.y >= -0.01, "{f:?}");
    assert!(f.ink.y + f.ink.height <= BODY_LINE + 0.01, "{f:?}");
    let baseline = f.requested.y + f.baseline;
    assert!((baseline - metrics.first_line.baseline).abs() < 0.01);

    let wrapping = inline_text(&title, TextRole::Body).lines(Lines::Unlimited);
    let grown = wrapping.metrics(None);
    assert!(grown.size.height > BODY_LINE, "{grown:?}");
    let drawn = window.show(wrapping.into(), Size::new(600.0, 100.0), None, 1.0);
    assert!((drawn[0].requested.height - natural.size.height).abs() < 0.01);
}

/// Wrapping: as many lines as the text needs, within the width; at most
/// two, ending in an ellipsis and reporting the loss
#[test]
fn the_wrapping_form_takes_lines_and_at_most_shortens() {
    let mut window = Window::new();
    let title = InlineContent::parse(LONG);
    let width = 150.0;
    let wrapped = inline_text(&title, TextRole::Body).lines(Lines::Unlimited);
    let metrics = wrapped.metrics(Some(width));
    assert!(metrics.lines > 2, "{metrics:?}");
    assert!(metrics.size.width <= width);
    assert!(metrics.lost.is_none());
    assert!(metrics.size.height >= BODY_LINE * metrics.lines as f32);
    assert!(wrapped.minimum_width() < width);
    let drawn = window.show(
        widget::container(wrapped).width(width).into(),
        Size::new(600.0, 400.0),
        None,
        1.0,
    );
    assert_eq!(drawn.len(), 1);
    assert!(drawn[0].requested.x + drawn[0].requested.width <= width);

    let two = inline_text(&title, TextRole::Body).lines(Lines::AtMost(2));
    let metrics = two.metrics(Some(width));
    assert_eq!(metrics.lines, 2, "{metrics:?}");
    assert!(metrics.size.width <= width);
    assert!(metrics.lost.is_some());
    assert_eq!(two.losses(width).len(), 1);
    let mut texts = Drawn::default();
    window.show(
        widget::container(two).width(width).into(),
        Size::new(600.0, 400.0),
        Some(&mut texts),
        1.0,
    );
    assert!(texts.0[0].1.ends_with('\u{2026}'), "{:?}", texts.0);
}

/// A box reports what it draws: the text's bounds and the formula's, not
/// the width a Fill box takes
#[test]
fn drawn_bounds_are_narrower_than_a_fill_box() {
    let mut window = Window::new();
    let title = InlineContent::parse(TITLE);
    let text = inline_text(&title, TextRole::Body).width(Length::Fill);
    let width = text.metrics(Some(400.0)).size.width;
    let display = display_formula(r"x^2 + y^2 = z^2", TextRole::Body);
    let formula_width = display.metrics(Some(400.0)).drawn.width;
    let mut texts = Drawn::default();
    window.show(
        widget::Column::from_vec(vec![text.into(), display.into()])
            .width(400)
            .into(),
        Size::new(400.0, 300.0),
        Some(&mut texts),
        1.0,
    );
    assert_eq!(texts.0.len(), 2, "{:?}", texts.0);
    let (bounds, _) = &texts.0[0];
    assert_eq!(bounds.x, 0.0);
    assert!(
        (bounds.width - width).abs() < 0.01 && width < 200.0,
        "{bounds:?}"
    );
    let (bounds, latex) = &texts.0[1];
    assert_eq!(latex, r"x^2 + y^2 = z^2");
    assert!((bounds.width - formula_width).abs() < 0.01, "{bounds:?}");
    assert!(
        (bounds.x - (400.0 - formula_width) / 2.0).abs() < 0.01,
        "centred: {bounds:?}"
    );
}

/// A display formula wider than its box is made smaller to fit it
#[test]
fn a_display_formula_fits_its_width() {
    let display = display_formula(
        r"\sum_{k=1}^{n} \frac{1}{k^2} = \frac{\pi^2}{6} - \epsilon_n",
        TextRole::Body,
    );
    let natural = display.metrics(None);
    let narrow = display.metrics(Some(natural.drawn.width / 2.0));
    assert!(narrow.scale < 0.55, "{narrow:?}");
    assert!(narrow.drawn.width <= natural.drawn.width / 2.0 + 0.01);
    assert!(!narrow.source_shown);
    let broken = display_formula(r"\frac{1}{", TextRole::Body).metrics(Some(300.0));
    assert!(broken.source_shown);
}

/// wgpu draws an SVG from a raster of `ceil(scale × size)` pixels in a quad
/// whose corners it rounds to whole pixels (WGSL's `round`, to even at a
/// half), with the nearest texel: where the quad is the smaller, a row or
/// column of the formula is never drawn, and a row can be a bar of its "=".
/// tiny-skia has no such quad, so this does wgpu's sums on what the boxes
/// ask for (Cerea's `formulas_lose_no_row_of_pixels_wherever_they_fall`,
/// and Terminal's repro of it)
#[test]
fn formulas_lose_no_row_of_pixels_wherever_they_fall() {
    let mut window = Window::new();
    let paragraphs: Vec<InlineContent> = (0..24)
        .map(|i| {
            let words = "the pools fill at high tide ".repeat(1 + i * 7 % 11);
            InlineContent::parse(&format!("{words}and $E = mc^2$ holds."))
        })
        .collect();
    let mut off_pixels = 0;
    for scale in [1.0, 1.25, 1.5, 1.75, 2.0_f32] {
        let mut column = Vec::new();
        for paragraph in &paragraphs {
            column.push(
                inline_text(paragraph, TextRole::Body)
                    .lines(Lines::Unlimited)
                    .into(),
            );
            column.push(display_formula(r"x^2 + y^2 = z^2", TextRole::Body).into());
        }
        let drawn = window.show(
            widget::Column::from_vec(column)
                .spacing(0.37)
                .width(640)
                .into(),
            Size::new(640.0, 8000.0),
            None,
            scale,
        );
        assert!(drawn.len() >= 40, "{} formulas drawn", drawn.len());
        for formula in drawn {
            let rect = formula.bounds;
            assert_eq!(formula.scale, scale);
            for (start, length) in [(rect.y, rect.height), (rect.x, rect.width)] {
                let raster = (scale * length).ceil();
                let quad = (scale * (start + length)).round_ties_even()
                    - (scale * start).round_ties_even();
                assert_eq!(raster, quad, "at {scale}, losing a pixel: {formula:?}");
            }
            // Where it would have gone had it not been put on whole pixels
            let asked = formula.requested;
            let raster = (scale * asked.height).ceil();
            let quad = (scale * (asked.y + asked.height)).round_ties_even()
                - (scale * asked.y).round_ties_even();
            if raster != quad {
                off_pixels += 1;
            }
        }
    }
    assert!(
        off_pixels > 0,
        "the layout put every formula on whole pixels anyway: the test proves nothing"
    );
}
