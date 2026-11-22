// SPDX-License-Identifier: GPL-3.0-only

//! A split button: an action and, at its end, a menu opener ("New terminal
//! session" and its chevron). Juliet's design (22 Nov 2026): "We need to
//! design a new button variant which contains a side dropdown option. It
//! needs to be available in primary, secondary and tertiary. It should use
//! the same height and rounding and general styling as the normal buttons."
//! Of four prototypes she chose the one that is one pill, "but when you
//! hover the chevron the full button should be highlighted".
//!
//! It is exactly an ordinary [`Button`] of its look, plus the menu part, and
//! it **draws nothing a [`Button`] of the same look does not**, except the
//! divider and the chevron: no colour, fill, border or radius of its own.
//!
//! - **The action part** is the ordinary button's box: `space_l` tall, the
//!   label (and an optional leading icon) with `space_s` at each side, the
//!   look's type and label colours, the pill's outer end (`radius_xl`) and
//!   a square end at the divider.
//! - **The menu part** is a `space_l` square holding a chevron in the look's
//!   label colour, rounded at the outer end and square at the divider.
//! - **The divider** is always shown. Primary (and Destructive): a thin
//!   (1 px) line in the on-fill colour, inset from the top and bottom by half
//!   of `space_xs` (6 px at standard density). Secondary: its 2 px border
//!   colour, the full height, so it joins the outline. Tertiary (and the
//!   other unboxed looks): a short 1 px line in the muted text tone (the
//!   page's text mixed towards the page, as a disabled label is), inset as
//!   Primary's, since Tertiary has no box at rest.
//! - **Hover and pressed:** the pointer on the action part gives that part
//!   the look's hover (Primary's hovered fill, the look's wash), rounded at
//!   the outer end and square at the divider, and nothing else changes. The
//!   pointer on the menu part gives the *whole* button its hover. Pressed
//!   likewise. A disabled action part (no [`SplitButton::on_press`]) shows the
//!   look's fade; the menu part stays live.
//! - **Focus** is the household's keyboard-target outline
//!   ([`crate::keyboard_target`]) on the focused part's own edge, inside it.
//! - **The menu part** is a trigger of one root of the window's menu chain
//!   ([`crate::window::trigger`], as the tab overflow buttons are): a
//!   press, Enter or Space opens or closes the menu, and the app gives the
//!   root's panel `Panel::source(id)` so it opens below the menu part. While
//!   the menu is open ([`SplitButton::open`]) the whole button is shown
//!   pressed ([`Button::shown_as`]); no new fill.
//! - **Width:** the parent's. By default the button is as wide as the label
//!   needs and the square; [`SplitButton::width`] with `Length::Fill` makes
//!   the action part fill, and the menu square stays `space_l`.
//! - **Looks:** Primary, Secondary, Tertiary, Destructive,
//!   TertiaryDestructive and Plain. Icon and Segment are drawn as Plain.
//!
//! The two parts are real [`Button`]s with `joined(true)` (no border of
//! their own), so every behaviour and paint of a button is theirs. This
//! widget lays them side by side, draws the one outline and the divider over
//! them, and lets the action part show its hover while the pointer is on the
//! menu part by drawing it with the pointer at its own centre.
//!
//! ```
//! use household_widgets::button::ButtonLook;
//! use household_widgets::split_button::SplitButton;
//! use cosmic::widget::Id;
//!
//! #[derive(Clone)]
//! enum Message { NewSession }
//!
//! #[derive(Clone, PartialEq)]
//! enum Root { Machines }
//!
//! let button: cosmic::Element<'_, Message> = SplitButton::new(
//!     ButtonLook::Secondary,
//!     "New terminal session",
//!     "Choose a machine",
//!     Root::Machines,
//!     Id::new("new-session"),
//! )
//! .on_press(Message::NewSession)
//! .into();
//! # let _ = button;
//! ```

use crate::button::{Button, ButtonLook, State};
use crate::segmented::part_corners;
use crate::tooltip::Tip;
use crate::window::Toggle;
use cosmic::iced::advanced::widget::{Operation, Tree, tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use cosmic::iced::{Background, Border, Color, Event, Length, Point, Rectangle, Size, Vector};
use cosmic::widget::{self, tooltip::Position};
use cosmic::{Element, Renderer, Theme, theme};

/// An action with a menu part at its end (see the module's documentation)
pub struct SplitButton<'a, M, R> {
    look: ButtonLook,
    width: Length,
    label: String,
    on_press: Option<M>,
    leading: Option<widget::icon::Handle>,
    tip: Option<Tip<'a, M>>,
    opener_name: String,
    root: R,
    id: widget::Id,
    open: bool,
}

impl<'a, M: Clone + 'a, R: Clone + 'static> SplitButton<'a, M, R> {
    /// A split button of `look` (see the module's list of supported looks)
    /// whose action part is labelled `label`, its menu part named
    /// `opener_name` (also its tooltip: an icon alone says what it is,
    /// rule 4) and opening `root` of the window's chain, with `id` the
    /// menu part's identity, which the root's panel is anchored at
    pub fn new(
        look: ButtonLook,
        label: impl Into<String>,
        opener_name: impl Into<String>,
        root: R,
        id: impl Into<widget::Id>,
    ) -> Self {
        Self {
            look: match look {
                ButtonLook::Icon | ButtonLook::Segment => ButtonLook::Plain,
                other => other,
            },
            width: Length::Shrink,
            label: label.into(),
            on_press: None,
            leading: None,
            tip: None,
            opener_name: opener_name.into(),
            root,
            id: id.into(),
            open: false,
        }
    }

    /// What the action part does. Without it that part is disabled; the
    /// menu part is not
    pub fn on_press(mut self, message: M) -> Self {
        self.on_press = Some(message);
        self
    }

    /// A 16 px icon before the label
    pub fn leading_icon(mut self, icon: impl Into<widget::icon::Handle>) -> Self {
        self.leading = Some(icon.into());
        self
    }

    /// The action part's tooltip, below it
    pub fn tooltip(mut self, tip: Tip<'a, M>) -> Self {
        self.tip = Some(tip);
        self
    }

    /// The button's width: `Length::Fill` makes the action part fill and the
    /// menu part stays the `space_l` square
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Whether the menu is open now: the whole button is drawn pressed
    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }
}

impl<'a, M: Clone + 'a, R: Clone + 'static> From<SplitButton<'a, M, R>> for Element<'a, M> {
    fn from(split: SplitButton<'a, M, R>) -> Self {
        let radius = theme::active().cosmic().corner_radii.radius_xl;
        let look = split.look;
        let shown = split.open.then_some(State::Pressed);
        let enabled = split.on_press.is_some();
        let mut action = Button::new(look, split.label)
            .width(split.width)
            .joined(true)
            .shown_as(shown)
            .corners(part_corners(0, 2, radius))
            .on_press_maybe(split.on_press);
        if let Some(icon) = split.leading {
            action = action.leading_icon(icon);
        }
        if let Some(tip) = split.tip {
            action = action.tooltip(tip, Position::Bottom);
        }
        let name = split.opener_name;
        let words = name.clone();
        let tip = Tip::new(name.clone(), move || widget::text(words.clone()).into());
        let menu: Element<'a, Toggle> = Button::new(look, name)
            .compact(true)
            .leading_icon(widget::icon::from_name("pan-down-symbolic").handle())
            .joined(true)
            .shown_as(shown)
            .corners(part_corners(1, 2, radius))
            .tooltip(tip, Position::Bottom)
            .on_press(Toggle)
            .into();
        let menu = crate::window::trigger(
            split.id.clone(),
            split.root,
            crate::popup::focus_target(split.id, menu).on_activate(Toggle),
        );
        Element::new(Split {
            look,
            open: split.open,
            enabled,
            parts: [action.into(), menu],
        })
    }
}

/// The two parts side by side and what is drawn over them (see the module's
/// documentation)
struct Split<'a, M> {
    look: ButtonLook,
    open: bool,
    /// Whether the action part can be pressed
    enabled: bool,
    /// The action part, then the menu part
    parts: [Element<'a, M>; 2],
}

/// Which part has the keyboard's focus, as last found: 0 action, 1 menu
#[derive(Default)]
struct Focused(Option<usize>);

/// Whether the widget it visits is focused
#[derive(Default)]
struct Probe(bool);

impl Operation for Probe {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }

    fn focusable(
        &mut self,
        _: Option<&widget::Id>,
        _: Rectangle,
        focus: &mut dyn cosmic::iced::advanced::widget::operation::Focusable,
    ) {
        self.0 |= focus.is_focused();
    }
}

impl<M> Split<'_, M> {
    fn probe(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer) {
        let mut found = None;
        for (n, ((part, state), part_layout)) in
            self.parts.iter_mut().zip(&mut tree.children).zip(layout.children()).enumerate()
        {
            let mut probe = Probe::default();
            part.as_widget_mut().operate(state, part_layout, renderer, &mut probe);
            if probe.0 {
                found = Some(n);
            }
        }
        tree.state.downcast_mut::<Focused>().0 = found;
    }

    /// The state part `n` is drawn in: pressed while the menu is open, else
    /// hovered when the pointer is on it or, for the action part, on the
    /// menu part (the whole button is lit), else at rest
    fn state_of(&self, n: usize, over: Option<usize>) -> State {
        if n == 0 && !self.enabled {
            State::Disabled
        } else if self.open {
            State::Pressed
        } else if over == Some(n) || (n == 0 && over == Some(1)) {
            State::Hovered
        } else {
            State::Rest
        }
    }
}

impl<M> Widget<M, Theme, Renderer> for Split<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Focused>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Focused::default())
    }

    fn children(&self) -> Vec<Tree> {
        self.parts.iter().map(Tree::new).collect()
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.parts);
    }

    fn size(&self) -> Size<Length> {
        Size::new(
            self.parts[0].as_widget().size().width,
            Length::Fixed(crate::button::button_height()),
        )
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let square = crate::button::button_height();
        let menu = self.parts[1].as_widget_mut().layout(
            &mut tree.children[1],
            renderer,
            &layout::Limits::new(Size::new(square, square), Size::new(square, square)),
        );
        let action = self.parts[0].as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &limits.shrink(Size::new(square, 0.0)),
        );
        let width = action.size().width;
        layout::Node::with_children(
            Size::new(width + square, square),
            vec![action, menu.move_to(Point::new(width, 0.0))],
        )
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((part, state), part_layout) in self.parts.iter_mut().zip(&mut tree.children).zip(layout.children()) {
                part.as_widget_mut().operate(state, part_layout, renderer, operation);
            }
        });
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
        for ((part, state), part_layout) in self.parts.iter_mut().zip(&mut tree.children).zip(layout.children()) {
            part.as_widget_mut()
                .update(state, event, part_layout, cursor, renderer, clipboard, shell, viewport);
        }
        self.probe(tree, layout, renderer);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.parts
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((part, state), part_layout)| {
                part.as_widget().mouse_interaction(state, part_layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        overlay::from_children(&mut self.parts, tree, layout, renderer, viewport, translation)
    }

    fn drag_destinations(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        dnd_rectangles: &mut cosmic::iced::advanced::clipboard::DndDestinationRectangles,
    ) {
        for ((part, state), part_layout) in self.parts
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
        {
            part.as_widget().drag_destinations(state, part_layout, renderer, dnd_rectangles);
        }
    }

    #[cfg(feature = "a11y")]
    fn a11y_nodes(
        &self,
        layout: Layout<'_>,
        tree: &Tree,
        cursor: mouse::Cursor,
    ) -> iced_accessibility::A11yTree {
        iced_accessibility::A11yTree::join(
            self.parts
                .iter()
                .zip(layout.children())
                .zip(&tree.children)
                .map(|((part, part_layout), state)| part.as_widget().a11y_nodes(part_layout, state, cursor)),
        )
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
        let mut parts = layout.children();
        let (Some(action), Some(menu)) = (parts.next(), parts.next()) else {
            return;
        };
        let (action, menu) = (action.bounds(), menu.bounds());
        let over = if cursor.is_over(menu) {
            Some(1)
        } else if cursor.is_over(action) {
            Some(0)
        } else {
            None
        };
        // The pointer on the menu part lights the whole button: the action
        // part is drawn with the pointer on it
        let action_cursor = if over == Some(1) {
            mouse::Cursor::Available(action.center())
        } else {
            cursor
        };
        let mut children = layout.children();
        for (n, (part, state)) in self.parts.iter().zip(&tree.children).enumerate() {
            let Some(part_layout) = children.next() else {
                return;
            };
            let at = if n == 0 { action_cursor } else { cursor };
            part.as_widget().draw(state, renderer, theme, style, part_layout, at, viewport);
        }
        let bounds = layout.bounds();
        let radius = theme.cosmic().corner_radii.radius_xl;
        let look = self.look;
        let secondary = crate::button::paint(look, false, None, State::Rest, theme).border.is_some();
        let space = theme.cosmic().spacing;
        // Secondary's outline, each part's in its own state's colour (a
        // disabled action fades as a button's border does)
        if secondary {
            for (n, part) in [action, menu].into_iter().enumerate() {
                let colour = crate::button::paint(look, false, None, if n == 0 && !self.enabled { State::Disabled } else { State::Rest }, theme)
                    .border
                    .unwrap_or(Color::TRANSPARENT);
                renderer.with_layer(part, |renderer| {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds,
                            border: Border {
                                radius: radius.into(),
                                width: crate::button::SECONDARY_BORDER,
                                color: colour,
                            },
                            ..renderer::Quad::default()
                        },
                        Background::Color(Color::TRANSPARENT),
                    );
                });
            }
        }
        // The divider, on whole pixels at the join
        let rest = crate::button::paint(look, false, None, State::Rest, theme);
        let (colour, width, inset) = if let Some(border) = rest.border {
            (border, crate::button::SECONDARY_BORDER, 0.0)
        } else {
            let colour = if let Some(on) = rest.text.filter(|_| look.filled()) {
                on
            } else {
                crate::button::fade(theme.cosmic().on_bg_color().into(), crate::look::page(theme))
            };
            (colour, 1.0, f32::from(space.space_xs) / 2.0)
        };
        let join = menu.x;
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: join - (width / 2.0).floor(),
                    y: bounds.y + inset,
                    width,
                    height: bounds.height - 2.0 * inset,
                },
                ..renderer::Quad::default()
            },
            Background::Color(colour),
        );
        // The focused part's outline, again, over the outline and divider
        let Some(n) = tree.state.downcast_ref::<Focused>().0 else {
            return;
        };
        let part = if n == 0 { action } else { menu };
        let corners = part_corners(n, 2, radius);
        crate::keyboard_target::draw_outline(
            renderer,
            part,
            corners.into(),
            crate::button::displayed_fill(look, false, self.state_of(n, over), Some(corners), theme),
            theme,
        );
    }
}

#[cfg(test)]
#[path = "split_button_tests.rs"]
mod tests;
