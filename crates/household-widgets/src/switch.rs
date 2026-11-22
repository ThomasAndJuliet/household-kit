// SPDX-License-Identifier: GPL-3.0-only

//! libcosmic's switch in the muted accent: "yes use the muted accent for
//! them all" (Juliet, 30 Oct; household rule 6). Only the "on" fill is
//! ours, through libcosmic's own toggler class: hover and disabled are
//! derived from that fill as from the accent, and the rest is libcosmic's.

use cosmic::iced::advanced::widget::{Operation, Tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use cosmic::iced::{Color, Event, Length, Point, Rectangle, Size, Vector};
use cosmic::widget;
use cosmic::widget::list::{self, ListButton};
use cosmic::widget::settings::item::Item;
use cosmic::{Element, Theme, theme};
use household_tones::{Srgb, muted_accent, on_muted};

use crate::spacing::TOGGLE_HEIGHT;

/// The switch's width: twice [`TOGGLE_HEIGHT`], the 2:1 shape libcosmic's
/// 48 x 24 switch has. Its knob's inset from the edge is libcosmic's 2 px
/// (2 x 26 / 24 is 2.2, kept on the whole pixel), so the knob is 22 px
const SWITCH_WIDTH: f32 = 2.0 * TOGGLE_HEIGHT;

/// libcosmic's switch, laid out [`SWITCH_WIDTH`] x [`TOGGLE_HEIGHT`] in place
/// of its fixed 48 x 24 (Juliet, 22 Nov 2026: "Let's go for 26px for
/// everything"). libcosmic lays its switch out as the label, then the gap,
/// then a 48 x 24 box, and draws the knob from the box it is given: this
/// puts a 52 x 26 box in the same place, and the label centred beside it.
/// Layout only; the tree, drawing, animation and events are libcosmic's
pub(crate) struct Sized<'a, M> {
    pub(crate) content: Element<'a, M>,
}

impl<M> Widget<M, Theme, cosmic::Renderer> for Sized<'_, M> {
    forward_widget!(M, Theme, cosmic::Renderer; children, diff, size, operate, update, draw, mouse_interaction, overlay, drag_destinations, a11y_nodes);

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let node = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        // libcosmic's children: the label, then the switch
        let [label, _] = node.children() else {
            return node;
        };
        let height = label.size().height.max(TOGGLE_HEIGHT);
        let gap = node.children()[1].bounds().x - label.size().width;
        let label = label
            .clone()
            .move_to(Point::new(0.0, (height - label.size().height) / 2.0));
        let switch_at = Point::new(label.size().width + gap, (height - TOGGLE_HEIGHT) / 2.0);
        let switch = layout::Node::new(Size::new(SWITCH_WIDTH, TOGGLE_HEIGHT)).move_to(switch_at);
        layout::Node::with_children(
            Size::new(switch_at.x + SWITCH_WIDTH, height),
            vec![label, switch],
        )
    }
}

/// The muted accent of `theme`'s accent (household rule 6), and the text and
/// icon colour on it. The accent's alpha is kept.
pub fn muted_colours(theme: &Theme) -> (Color, Color) {
    let c = theme.cosmic();
    let accent = c.accent_color();
    let muted = muted_accent(Srgb::new(accent.red, accent.green, accent.blue), c.is_dark);
    let on = on_muted(muted);
    let colour = |c: Srgb| Color {
        r: c.r,
        g: c.g,
        b: c.b,
        a: accent.alpha,
    };
    (colour(muted), colour(on))
}

/// libcosmic's `widget::toggler` in the muted accent, for a switch with
/// options: `toggler(on).label(label).on_toggle(message)`. A label sits the
/// theme's standard label-to-control gap (`space_xs`) from the switch, which
/// libcosmic leaves to the caller; `.spacing(..)` departs from it. Without
/// `on_toggle` it is disabled.
pub fn toggler<'a, M>(on: bool) -> Toggler<'a, M> {
    Toggler {
        inner: widget::toggler(on).class(theme::Toggler::accent(|t: &Theme| muted_colours(t).0)),
    }
}

/// The household switch builder: libcosmic's own [`widget::Toggler`] with
/// the label gap given. A switch without a label has no gap (libcosmic
/// would still reserve one).
pub struct Toggler<'a, M> {
    inner: widget::Toggler<'a, M>,
}

impl<'a, M> Toggler<'a, M> {
    /// The label, at the standard gap from the switch
    pub fn label(mut self, label: impl Into<Option<String>>) -> Self {
        let label = label.into();
        if label.is_some() {
            self.inner = self.inner.spacing(theme::spacing().space_xs);
        }
        self.inner = self.inner.label(label);
        self
    }
    /// A gap other than the standard one
    pub fn spacing(mut self, spacing: impl Into<cosmic::iced::Pixels>) -> Self {
        self.inner = self.inner.spacing(spacing);
        self
    }
    pub fn on_toggle(mut self, on_toggle: impl Fn(bool) -> M + 'a) -> Self {
        self.inner = self.inner.on_toggle(on_toggle);
        self
    }
    pub fn on_toggle_maybe(mut self, on_toggle: Option<impl Fn(bool) -> M + 'a>) -> Self {
        self.inner = self.inner.on_toggle_maybe(on_toggle);
        self
    }
    pub fn width(mut self, width: impl Into<cosmic::iced::Length>) -> Self {
        self.inner = self.inner.width(width);
        self
    }
    pub fn size(mut self, size: impl Into<cosmic::iced::Pixels>) -> Self {
        self.inner = self.inner.size(size);
        self
    }
    pub fn text_size(mut self, size: impl Into<cosmic::iced::Pixels>) -> Self {
        self.inner = self.inner.text_size(size);
        self
    }
    pub fn font(mut self, font: impl Into<cosmic::font::Font>) -> Self {
        self.inner = self.inner.font(font);
        self
    }
    /// libcosmic's toggler, for anything not forwarded here. It is
    /// libcosmic's own 48 x 24 switch: the household's is
    /// [`TOGGLE_HEIGHT`] tall only through [`switch`] and `Element::from`
    pub fn into_inner(self) -> widget::Toggler<'a, M> {
        self.inner
    }
}

impl<'a, M: 'static> From<Toggler<'a, M>> for Element<'a, M> {
    fn from(toggler: Toggler<'a, M>) -> Self {
        Element::new(Sized { content: toggler.inner.into() })
    }
}

/// A switch in the muted accent. A click publishes `message` with the new
/// state.
pub fn switch<'a, M: 'static>(on: bool, message: impl Fn(bool) -> M + 'a) -> Element<'a, M> {
    toggler(on).on_toggle(message).into()
}

/// A switch in the muted accent, disabled without a message.
pub fn switch_maybe<'a, M: 'static>(
    on: bool,
    message: Option<impl Fn(bool) -> M + 'a>,
) -> Element<'a, M> {
    toggler(on).on_toggle_maybe(message).into()
}

/// A settings row with a switch in the muted accent: libcosmic's
/// `settings::item::builder(title).toggler(on, message)` and
/// `.toggler_maybe(on, message)`, with the same row that a click anywhere
/// flips. Import this and write `.switch` and `.switch_maybe` instead.
pub trait SwitchItem<'a, M> {
    /// libcosmic's `.toggler`, in the muted accent
    fn switch(self, on: bool, message: impl Fn(bool) -> M + 'static) -> ListButton<'a, M>;

    /// libcosmic's `.toggler_maybe`, in the muted accent: disabled without a
    /// message
    fn switch_maybe(
        self,
        on: bool,
        message: Option<impl Fn(bool) -> M + 'static>,
    ) -> ListButton<'a, M>;
}

impl<'a, M: Clone + 'static> SwitchItem<'a, M> for Item<'a, M> {
    fn switch(self, on: bool, message: impl Fn(bool) -> M + 'static) -> ListButton<'a, M> {
        let press = message(!on);
        list::button(self.control(switch(on, message))).on_press(press)
    }

    fn switch_maybe(
        self,
        on: bool,
        message: Option<impl Fn(bool) -> M + 'static>,
    ) -> ListButton<'a, M> {
        let press = message.as_ref().map(|message| message(!on));
        list::button(self.control(switch_maybe(on, message))).on_press_maybe(press)
    }
}
