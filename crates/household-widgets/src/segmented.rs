// SPDX-License-Identifier: GPL-3.0-only

//! The segmented control: a choice of one of several (rule 5). "The
//! segmented control is the same button repeated, with its parent joining
//! the corners (first and last rounded, inner ones square, no gap)"
//! (Juliet, 21 Nov 2026). Each part is the household's one [`Button`] in
//! its [`ButtonLook::Segment`] look; this module is the parent that joins
//! them.
//!
//! # What the parent does, and what the parts do
//!
//! - **The parts** are buttons, but toggles are not as tall as buttons:
//!   [`TOGGLE_HEIGHT`](crate::spacing::TOGGLE_HEIGHT) (26 px) tall, the label in the button's
//!   type (14 px on a 20 px line), `space_s` at each side, hover and
//!   pressed washes inside each part's own box, the keyboard-target
//!   outline as every button's (on the part's own edge, inside it). Each
//!   part is Tab-focusable and Enter or Space chooses it, as a button does; the arrow keys do nothing, as in
//!   Terminal's control today. The chosen part is filled with the muted
//!   accent (rule 6: toggles and chosen parts) with its on colours, its
//!   label semibold and the others regular (rule 6a). A part's width is
//!   measured over *both* weights, so choosing a part, like hovering it,
//!   moves nothing ("the hovered size should be the same as the
//!   non-hovered size from a layout perspective", Juliet, 21 Nov).
//! - **The bar** is drawn by the parent, not by the parts: one background
//!   under the row and one [`OUTLINE`] stroke over its outer edge, drawn
//!   inside the bar's box as Secondary's border is, the parts filling the
//!   bar with no gap. The bar is [`TOGGLE_HEIGHT`](crate::spacing::TOGGLE_HEIGHT) tall, as every toggle is
//!   (Juliet, 22 Nov 2026: "Let's go for 26px for everything"), and a chosen end part's fill meets
//!   the outline; the bar draws the focused part's outline again over its
//!   outline, so the outline never covers focus at the ends. The first
//!   part is rounded on its left and the last on its right (the theme's `radius_xl`, as buttons are rounded, rule
//!   25d); every inner corner is square, so the parts touch. A bar of one
//!   part is rounded on both sides.
//! - **Content is centred on the part's shape.** An end part is a rectangle
//!   with a half circle for its outer end, so its area leans towards the
//!   divider and content centred in its box looks too far out ("the icons
//!   don't seem correctly centred in their toggle space", Juliet, 22 Nov
//!   2026). The content of a centred part stands on the centroid of the
//!   part's shape ([`end_centroid_offset`], about 1.3 px at 26 px high), on a whole
//!   pixel; a middle part's shape is symmetric and is as it was. Content
//!   starting at the inset ([`Segmented::form`]) keeps its column. A part
//!   with an icon and no label is icon-only: the icon alone, with no gap
//!   for a label ([`Part::new`] with `""`).
//! - **A part with no message is disabled** (rule 6a: faded, no wash),
//!   and may carry its own tooltip saying why ([`Part::tip`], rule 17).
//!
//! The bar is only as wide as its parts need: it never stretches across a
//! row (rule 5). Put in a settings row it goes through libcosmic's
//! `flex_control` (rule 24a), so a bar too wide for the row drops under the
//! label instead of being cut off; the bar is an ordinary element for that.
//!
//! # Stacked bars in a form
//!
//! Where several bars stand under each other, every part of every bar is as
//! wide as the widest part among them, so that their edges line up (Juliet,
//! 15 Oct). [`form_part`] works that width out from every part of every bar
//! (bars laid out elsewhere count too) and [`Segmented::form`] lays a bar's
//! parts out in it. Where any part of the form has an icon, every part
//! keeps an icon's room, so the icons stand in one column and the labels
//! start in another. `room` is how wide the controls may be: a bar of many
//! parts shares it, its parts narrower, and a label that no longer fits is
//! cut short with an ellipsis, whole in the part's tooltip.
//!
//! # Example
//!
//! ```
//! use household_widgets::segmented::{Part, Segmented, form_part};
//!
//! #[derive(Clone)]
//! enum Message { View(usize) }
//!
//! // A bar alone is as wide as its own parts
//! let views = ["Paper", "Text", "Terminal"];
//! let bar: cosmic::Element<'_, Message> = Segmented::new(
//!     views.iter().enumerate().map(|(n, v)| Part::new(*v).on_press(Message::View(n))),
//!     Some(0),
//! )
//! .into();
//! # let _ = bar;
//!
//! // Two bars stacked in a form: one part width for both, the widest
//! // label's (measured in both weights), here with no room limit
//! let kinds = ["Conversation", "Session"];
//! let part = form_part(
//!     kinds.iter().chain(views.iter()).map(|label| (*label, false)),
//!     3,
//!     None,
//! );
//! assert!(!part.icons);
//! let first: cosmic::Element<'_, Message> = Segmented::new(
//!     kinds.iter().map(|k| Part::new(*k).on_press(Message::View(0))),
//!     Some(0),
//! )
//! .form(part)
//! .into();
//! # let _ = first;
//! ```

use cosmic::iced::advanced::widget::{Operation, Tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use cosmic::iced::{Alignment, Background, Border, Color, Event, Length, Rectangle, Size, Vector};
use cosmic::widget::{self, tooltip::Position};
use cosmic::{Element, Renderer, Theme, theme};

use crate::button::{ACTION_ICON, Button, ButtonLook, LABEL_SIZE, displayed_fill};
use crate::menu::measured_width;
use crate::tooltip::Tip;

/// The stroke round the bar: a line, not a layout size. It is painted over
/// the parts' outer edges, inside the bar's box
pub const OUTLINE: f32 = 1.0;

/// One part of a segmented control: a label, what choosing it does, and
/// optionally an icon and a tooltip
pub struct Part<'a, M> {
    label: String,
    on_press: Option<M>,
    icon: Option<widget::icon::Handle>,
    leading: Option<Element<'a, M>>,
    tip: Option<Tip<'a, M>>,
}

impl<'a, M: Clone + 'a> Part<'a, M> {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            on_press: None,
            icon: None,
            leading: None,
            tip: None,
        }
    }

    /// What choosing this part does. Without it the part is disabled
    pub fn on_press(mut self, message: M) -> Self {
        self.on_press = Some(message);
        self
    }

    pub fn on_press_maybe(mut self, message: Option<M>) -> Self {
        self.on_press = message;
        self
    }

    /// A 16 px icon before the label
    pub fn icon(mut self, icon: impl Into<widget::icon::Handle>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// An element of the caller's before the label, in place of an icon
    /// handle: a coloured icon (a machine's colour), say. It is measured like
    /// any button's leading element ([`Button::leading`]). A form
    /// ([`form_part`]) gives every part an action icon's room, 16 px square,
    /// so the element should be that size to stand in the icons' column
    pub fn leading(mut self, element: Element<'a, M>) -> Self {
        self.leading = Some(element);
        self
    }

    /// The part's tooltip, below it (why it is disabled, or more about it)
    pub fn tip(mut self, tip: Tip<'a, M>) -> Self {
        self.tip = Some(tip);
        self
    }

    /// What [`form_part`] measures of this part: its label, and whether
    /// it has an icon
    pub fn measure(&self) -> (&str, bool) {
        (&self.label, self.icon.is_some() || self.leading.is_some())
    }
}

/// The parts of a form's bars: their one width, and whether each keeps an
/// icon's room before its label ([`form_part`])
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FormPart {
    pub width: f32,
    pub icons: bool,
}

/// How wide a part is for `label` with its icon's room if `icon`: the
/// label measured in both weights, the wider kept, and the part's side
/// inset at each side. It is what [`Button`] lays a part out as, whole
/// pixels
pub fn part_width(label: &str, icon: bool) -> f32 {
    let space = theme::active().cosmic().spacing;
    let icon = if icon {
        f32::from(ACTION_ICON) + f32::from(space.space_xxs)
    } else {
        0.0
    };
    let text = measured_width(label, LABEL_SIZE, cosmic::font::semibold())
        .max(measured_width(label, LABEL_SIZE, cosmic::font::default()));
    (icon + text + 2.0 * ButtonLook::Segment.inset(&space)).ceil()
}

/// The one width of every part of every bar stacked in a form: the widest
/// part among `parts` (each its label and whether it has an icon; include
/// the parts of bars laid out elsewhere). `most` is the most parts any one
/// bar has, and `room` how wide a bar may be: where given, a part is no
/// wider than its share of it
pub fn form_part<'s>(
    parts: impl IntoIterator<Item = (&'s str, bool)>,
    most: usize,
    room: Option<f32>,
) -> FormPart {
    let parts: Vec<(&str, bool)> = parts.into_iter().collect();
    let icons = parts.iter().any(|(_, icon)| *icon);
    let width = parts
        .iter()
        .map(|(label, _)| part_width(label, icons))
        .fold(0.0, f32::max);
    let width = match room {
        Some(room) => width.min((room / most.max(1) as f32).floor()),
        None => width,
    };
    FormPart { width, icons }
}

/// A choice of one of several (see the module's documentation)
pub struct Segmented<'a, M> {
    parts: Vec<Part<'a, M>>,
    chosen: Option<usize>,
    form: Option<FormPart>,
}

impl<'a, M: Clone + 'a> Segmented<'a, M> {
    /// A bar of `parts`, the one at index `chosen` (if any) filled
    pub fn new(parts: impl IntoIterator<Item = Part<'a, M>>, chosen: Option<usize>) -> Self {
        Self {
            parts: parts.into_iter().collect(),
            chosen,
            form: None,
        }
    }

    /// Lay the parts out as a form's bars have them ([`form_part`])
    pub fn form(mut self, part: FormPart) -> Self {
        self.form = Some(part);
        self
    }
}

/// How far the area of an end part lies from the middle of its box, towards
/// its divider (the inner side), in pixels. An end part is a rectangle
/// `width` x `height` whose two outer corners are rounded with `radius`
/// (clamped to what fits, as the painter clamps it); with a radius of half the
/// height it is a rectangle `width - r` wide and a half disc of radius `r`
/// at its outer end. Rounding takes more away at the outer side, so the
/// centroid of what is left is nearer the divider than the box's middle.
///
/// Each rounded corner removes the square `r` x `r` less its quarter disc:
/// area `r^2 (1 - pi/4)`, moment about the outer edge `r^3 (5/6 - pi/4)`
/// (the quarter disc, centred `r` in from the edge, has its centroid
/// `4r/3pi` short of its centre). With the outer edge at 0 the shape's
/// centroid is `(w h / 2 w - 2 moment) / (w h - 2 area)` and its offset from
/// the middle `w / 2` is `(w area - 2 moment) / (w h - 2 area)`. At `height`
/// 32 and a pill's radius it is about 1.6 px, at 26 about 1.3
pub fn end_centroid_offset(width: f32, height: f32, radius: f32) -> f32 {
    use std::f32::consts::PI;
    let r = radius.min(height / 2.0).min(width / 2.0).max(0.0);
    let corner = r * r * (1.0 - PI / 4.0);
    let moment = r * r * r * (5.0 / 6.0 - PI / 4.0);
    let area = width * height - 2.0 * corner;
    if area <= 0.0 {
        return 0.0;
    }
    (width * corner - 2.0 * moment) / area
}

/// The corner radii of part `n` of `count` (top left, top right, bottom
/// right, bottom left): the theme's `radius_xl` where it is an end of the
/// bar, square where it touches a neighbour
pub fn part_corners(n: usize, count: usize, radius: [f32; 4]) -> [f32; 4] {
    let [top_left, top_right, bottom_right, bottom_left] = radius;
    let first = n == 0;
    let last = n + 1 >= count;
    [
        if first { top_left } else { 0.0 },
        if last { top_right } else { 0.0 },
        if last { bottom_right } else { 0.0 },
        if first { bottom_left } else { 0.0 },
    ]
}

impl<'a, M: Clone + 'a> From<Segmented<'a, M>> for Element<'a, M> {
    fn from(bar: Segmented<'a, M>) -> Self {
        let radius = theme::active().cosmic().corner_radii.radius_xl;
        let count = bar.parts.len();
        let chosen = bar.chosen;
        let form = bar.form;
        let parts: Vec<Element<'a, M>> = bar
            .parts
            .into_iter()
            .enumerate()
            .map(|(n, part)| {
                let mut button = Button::new(ButtonLook::Segment, part.label)
                    .selected(chosen == Some(n))
                    .corners(part_corners(n, count, radius))
                    .on_press_maybe(part.on_press);
                button = match (part.leading, part.icon, form) {
                    (Some(element), _, _) => button.leading(element),
                    (None, Some(icon), _) => button.leading_icon(icon),
                    // An icon's room, where the form's other parts have icons
                    (None, None, Some(form)) if form.icons => button.leading(
                        widget::Space::new()
                            .width(Length::Fixed(f32::from(ACTION_ICON)))
                            .height(Length::Fixed(f32::from(ACTION_ICON)))
                            .into(),
                    ),
                    (None, None, _) => button,
                };
                if let Some(form) = form {
                    // Content from the inset, so every part's icon and label stand in
                    // the form's columns rather than each centred in the shared width
                    button = button.width(Length::Fixed(form.width)).content_start(true);
                }
                if let Some(tip) = part.tip {
                    button = button.tooltip(tip, Position::Bottom);
                }
                button.into()
            })
            .collect();
        // The bar: one background under parts that touch, one outline over
        // their outer edges, inside the bar's own box
        Element::new(Outlined {
            content: widget::row::with_children(parts)
                .align_y(Alignment::Center)
                .into(),
            count,
            chosen,
        })
    }
}

/// The bar's paint round its row of parts: the background beneath them and
/// the [`OUTLINE`] stroke over them, drawn inside the bar's edge as
/// Secondary's border is drawn inside its box. The bar is the row's size,
/// [`TOGGLE_HEIGHT`](crate::spacing::TOGGLE_HEIGHT) tall like every toggle;
/// the chosen part's fill meets the outline. Every event, size and
/// operation is the row's.
///
/// The outline is painted over the parts, so it would cover the outer pixel
/// of a focused part's keyboard-target outline along the bar's edge (a
/// part's outline is drawn on the part's own edge, which at the top, the
/// bottom and the bar's ends is the bar's). The bar therefore finds which
/// part has the keyboard's focus and draws that part's outline again over
/// its own, so focus stays whole
struct Outlined<'a, M> {
    content: Element<'a, M>,
    /// How many parts the row has, and which is chosen: what the outline of a
    /// focused part is drawn from ([`part_corners`], [`crate::keyboard_target`])
    count: usize,
    chosen: Option<usize>,
}

/// Which part has the keyboard's focus, as the bar last found it
#[derive(Default)]
struct Focused(Option<Rectangle>);

/// Finds the focused widget's box
#[derive(Default)]
struct Probe(Option<Rectangle>);

impl Operation for Probe {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }

    fn focusable(
        &mut self,
        _: Option<&cosmic::widget::Id>,
        bounds: Rectangle,
        focus: &mut dyn cosmic::iced::advanced::widget::operation::Focusable,
    ) {
        if focus.is_focused() {
            self.0 = Some(bounds);
        }
    }
}

impl<M> Outlined<'_, M> {
    fn probe(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer) {
        let mut probe = Probe::default();
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, &mut probe);
        tree.state.downcast_mut::<Focused>().0 = probe.0;
    }
}

impl<M> Widget<M, Theme, Renderer> for Outlined<'_, M> {
    forward_widget!(M, Theme, Renderer; children, diff, size, layout, mouse_interaction, overlay, drag_destinations, a11y_nodes);

    fn tag(&self) -> cosmic::iced::advanced::widget::tree::Tag {
        cosmic::iced::advanced::widget::tree::Tag::of::<Focused>()
    }

    fn state(&self) -> cosmic::iced::advanced::widget::tree::State {
        cosmic::iced::advanced::widget::tree::State::new(Focused::default())
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
        self.probe(tree, layout, renderer);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        self.probe(tree, layout, renderer);
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use cosmic::iced::advanced::Renderer as _;
        let cosmic = theme.cosmic();
        let radius = cosmic.corner_radii.radius_xl;
        let bounds = layout.bounds();
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border {
                    radius: radius.into(),
                    ..Border::default()
                },
                ..renderer::Quad::default()
            },
            Background::Color(cosmic.background(theme.transparent).component.base.into()),
        );
        self.content
            .as_widget()
            .draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border {
                    radius: radius.into(),
                    width: OUTLINE,
                    color: cosmic.bg_divider().into(),
                },
                ..renderer::Quad::default()
            },
            Background::Color(Color::TRANSPARENT),
        );
        // The focused part's outline, again, over the outline
        let Some(focused) = tree.state.downcast_ref::<Focused>().0 else {
            return;
        };
        let Some((n, part)) = layout
            .children()
            .enumerate()
            .find(|(_, part)| part.bounds().contains(focused.center()))
        else {
            return;
        };
        let corners = part_corners(n, self.count, radius);
        let state = if cursor.is_over(part.bounds()) {
            crate::button::State::Hovered
        } else {
            crate::button::State::Rest
        };
        crate::keyboard_target::draw_outline(
            renderer,
            part.bounds(),
            corners.into(),
            displayed_fill(ButtonLook::Segment, self.chosen == Some(n), state, Some(corners), theme),
            theme,
        );
    }
}

#[cfg(test)]
#[path = "segmented_tests.rs"]
mod tests;
