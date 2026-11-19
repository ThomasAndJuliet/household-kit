// SPDX-License-Identifier: GPL-3.0-only

//! libcosmic's switch in the muted accent: "yes use the muted accent for
//! them all" (Juliet, 30 Oct; household rule 6). Only the "on" fill is
//! ours, through libcosmic's own toggler class: hover and disabled are
//! derived from that fill as from the accent, and the rest is libcosmic's.

use cosmic::iced::Color;
use cosmic::widget;
use cosmic::widget::list::{self, ListButton};
use cosmic::widget::settings::item::Item;
use cosmic::{Element, Theme, theme};
use household_tones::{Srgb, muted_accent, on_muted};

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
    /// libcosmic's toggler, for anything not forwarded here
    pub fn into_inner(self) -> widget::Toggler<'a, M> {
        self.inner
    }
}

impl<'a, M: 'static> From<Toggler<'a, M>> for Element<'a, M> {
    fn from(toggler: Toggler<'a, M>) -> Self {
        toggler.inner.into()
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
