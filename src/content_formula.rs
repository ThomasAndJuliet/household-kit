// SPDX-License-Identifier: GPL-3.0-only

//! The formula box: LaTeX set by RaTeX in a role's type, in a line of text
//! (its width and its baseline) or as a display block (centred in its
//! width, made smaller to fit it). Formulas are drawn on whole pixels
//! ([`super::on_pixels`]). One that can't be set shows its source, in the
//! error tone, set in the monospace font.

use std::borrow::Cow;

use super::*;
use crate::layout::{self, MeasuredSize};
use cosmic::iced::advanced::graphics::text::{Raw, to_color};
use cosmic::iced::advanced::widget::{Operation, Tree, tree};
use cosmic::iced::advanced::{Layout, Widget, layout as native, mouse, renderer};
use cosmic::iced::core::text::Renderer as _;
use cosmic::iced::{Length, Point, Vector};
use cosmic::{Element, Theme};
use cosmic_text::Wrap;

/// A formula in a text role
#[derive(Clone, Debug)]
pub struct Formula<'a> {
    latex: Cow<'a, str>,
    display: bool,
    role: TextRole,
    width: Length,
}

/// `latex` set in a line of `role`'s text: as wide as it is, with its
/// baseline
pub fn formula<'a>(latex: impl Into<Cow<'a, str>>, role: TextRole) -> Formula<'a> {
    Formula {
        latex: latex.into(),
        display: false,
        role,
        width: Length::Shrink,
    }
}

/// `latex` set as display maths beside `role`'s text: its box fills the
/// width it is given, the formula centred in it and made smaller to fit
pub fn display_formula<'a>(latex: impl Into<Cow<'a, str>>, role: TextRole) -> Formula<'a> {
    Formula {
        latex: latex.into(),
        display: true,
        role,
        width: Length::Fill,
    }
}

/// What a formula takes at a width: a pure query
#[derive(Clone, Debug, PartialEq)]
pub struct FormulaMetrics {
    /// Its box
    pub size: Size,
    /// What it draws, in its box
    pub drawn: Rectangle,
    /// From the top of its box
    pub baseline: f32,
    /// Its size against the size it is set at in this role (below 1 when
    /// made smaller to fit)
    pub scale: f32,
    /// It could not be set: its source is shown
    pub source_shown: bool,
}

impl<'a> Formula<'a> {
    /// What it takes at `width` (or unbounded)
    pub fn metrics(&self, width: Option<f32>) -> FormulaMetrics {
        self.laid(width).metrics.clone()
    }

    /// The box in a layout. A display formula fits any width it is given
    pub fn child<M: 'a>(self) -> layout::Child<'a, M> {
        let view = self.clone();
        let query = self;
        layout::child(move |_| Element::from(view.clone())).with_measurement(move |_, _| {
            let natural = query.metrics(None);
            let minimum = if query.display {
                Size::new(0.0, 0.0)
            } else {
                natural.size
            };
            MeasuredSize {
                preferred: natural.size,
                minimum,
            }
        })
    }

    fn laid(&self, width: Option<f32>) -> Arc<Laid> {
        thread_local! {
            static LAID: RefCell<ShapingCache<Laid>> = RefCell::new(ShapingCache::new());
        }
        let mut hasher = std::hash::DefaultHasher::new();
        (
            &*self.latex,
            self.display,
            self.role,
            width.map(f32::to_bits),
        )
            .hash(&mut hasher);
        shaping_key(&mut hasher);
        let key = hasher.finish();
        LAID.with(|cache| cache.borrow_mut().get_or(key, || self.lay(width)))
    }

    fn lay(&self, width: Option<f32>) -> Laid {
        let ty = self.role.typography();
        let size = ty.size * style().maths.scale;
        let mut scale = 1.0;
        let mut set = set(&self.latex, self.display, size);
        // A display formula wider than its box: smaller, in quarter pixels,
        // until it fits (its padding does not scale, so a few rounds)
        if self.display
            && let Some(width) = width
        {
            for _ in 0..4 {
                let Ok(formula) = &set else { break };
                if formula.width <= width + 0.01 {
                    break;
                }
                scale *= width / formula.width;
                let smaller = (size * scale * 4.0).floor() / 4.0;
                set = super::set(&self.latex, true, smaller);
                scale = smaller / size;
            }
        }
        match set {
            Ok(formula) => {
                let box_width = if self.display {
                    width.unwrap_or(formula.width)
                } else {
                    formula.width
                };
                let x = if self.display && style().maths.display_centred {
                    ((box_width - formula.width) / 2.0).max(0.0)
                } else {
                    0.0
                };
                Laid {
                    metrics: FormulaMetrics {
                        size: Size::new(box_width, formula.height),
                        drawn: Rectangle::new(
                            Point::new(x, 0.0),
                            Size::new(formula.width, formula.height),
                        ),
                        baseline: formula.baseline,
                        scale,
                        source_shown: false,
                    },
                    shown: Shown::Set(formula),
                }
            }
            Err(_) => {
                let mono = attrs(cosmic::font::mono()).color(to_color(error_color()));
                let code_size = style().inline_code_size(ty.size);
                let spans = [(0..self.latex.len(), mono.clone())];
                let wrap = if self.display {
                    Wrap::WordOrGlyph
                } else {
                    Wrap::None
                };
                let buffer = shape_spans(
                    &self.latex,
                    &spans,
                    &mono,
                    Metrics::new(code_size, ty.line),
                    if self.display { width } else { None },
                    wrap,
                );
                let mut drawn = Size::ZERO;
                let mut baseline = ty.line / 2.0;
                for (i, run) in buffer.layout_runs().enumerate() {
                    if i == 0 {
                        baseline = run.line_y;
                    }
                    drawn.width = drawn.width.max(run.line_w);
                    drawn.height = run.line_top + run.line_height;
                }
                drawn.width = drawn.width.ceil();
                let box_width = if self.display {
                    width.unwrap_or(drawn.width)
                } else {
                    drawn.width
                };
                Laid {
                    metrics: FormulaMetrics {
                        size: Size::new(box_width, drawn.height),
                        drawn: Rectangle::new(Point::ORIGIN, drawn),
                        baseline,
                        scale: 1.0,
                        source_shown: true,
                    },
                    shown: Shown::Source(Arc::new(buffer)),
                }
            }
        }
    }
}

/// A formula laid out at a width
#[derive(Debug)]
pub(crate) struct Laid {
    metrics: FormulaMetrics,
    shown: Shown,
}

#[derive(Debug)]
enum Shown {
    Set(Arc<Set>),
    /// Its source, in the error tone
    Source(Arc<Buffer>),
}

#[derive(Default)]
struct State {
    laid: Option<Arc<Laid>>,
}

struct FormulaWidget<'a> {
    formula: Formula<'a>,
    #[cfg(feature = "a11y")]
    id: cosmic::iced::advanced::widget::Id,
}

impl<'a, Message: 'a> From<Formula<'a>> for Element<'a, Message> {
    fn from(formula: Formula<'a>) -> Self {
        Element::new(FormulaWidget {
            formula,
            #[cfg(feature = "a11y")]
            id: cosmic::iced::advanced::widget::Id::unique(),
        })
    }
}

impl<Message> Widget<Message, Theme, Renderer> for FormulaWidget<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(self.formula.width, Length::Shrink)
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
        let laid = self.formula.laid(max.is_finite().then_some(max));
        let size = limits.resolve(self.formula.width, Length::Shrink, laid.metrics.size);
        tree.state.downcast_mut::<State>().laid = Some(laid);
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
        let Some(laid) = &tree.state.downcast_ref::<State>().laid else {
            return;
        };
        let bounds = layout.bounds();
        if !bounds.intersects(viewport) {
            return;
        }
        let scale = style.scale_factor as f32;
        let at = Vector::new(bounds.x, bounds.y);
        match &laid.shown {
            Shown::Set(formula) => draw_formula(
                renderer,
                formula,
                laid.metrics.drawn + at,
                *viewport,
                scale,
                style.text_color,
            ),
            Shown::Source(buffer) => renderer.fill_raw(Raw {
                buffer: Arc::downgrade(buffer),
                position: Point::new(
                    (bounds.x * scale).round() / scale,
                    (bounds.y * scale).round() / scale,
                ),
                color: style.text_color,
                clip_bounds: *viewport,
            }),
        }
    }

    /// Its LaTeX, as written
    #[cfg(feature = "a11y")]
    fn a11y_nodes(
        &self,
        layout: Layout<'_>,
        _: &Tree,
        _: mouse::Cursor,
    ) -> iced_accessibility::A11yTree {
        super::inline_box::a11y_text(layout.bounds(), &self.formula.latex, &self.id)
    }

    /// What it draws, not its box
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if let Some(laid) = &tree.state.downcast_ref::<State>().laid {
            let bounds = layout.bounds();
            let drawn = laid.metrics.drawn + Vector::new(bounds.x, bounds.y);
            operation.text(None, drawn, &self.formula.latex);
        }
    }
}
