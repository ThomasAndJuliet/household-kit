// SPDX-License-Identifier: GPL-3.0-only

//! A split button: a main action and, joined to it, a button opening a menu
//! of related ones ("New terminal session" and its ▾). It is the one
//! [`Button`] twice, joined by its parent as a [`crate::segmented`] bar is:
//!
//! - **The halves** are buttons in the [`ButtonLook::Segment`] look: `space_l`
//!   tall, `space_s` at each side of the main label (the same box as every
//!   button), the opener the compact `space_l` square holding a 16 px
//!   chevron. The main half is rounded on its left and the opener on its
//!   right, the corners that meet are square.
//! - **The pair** is drawn by the parent, not the halves: one background, one
//!   [`OUTLINE`](crate::segmented::OUTLINE) stroke over its outer edge, drawn
//!   inside the pair's box, and the 1 px line between the halves. Nothing is
//!   drawn outside the box and hover, press and focus (the keyboard-target
//!   outline, on the half's own edge) stay inside their half.
//! - **The opener** is a trigger of one root of the window's menu chain
//!   ([`crate::window::trigger`]), as a menu bar's title is: a press, Enter or
//!   Space opens or closes the menu, the app gives the root's panel
//!   `Panel::source(id)` so it opens below the *pair's* opener. It is drawn
//!   chosen (the muted accent) while its menu is open ([`SplitButton::open`]).
//!
//! The pair is only as wide as the main label and the square need. Only the
//! Segment look is offered: the look's paint is washes with no border of its
//! own, which the shared outline needs; a filled or bordered pair would draw
//! its own border twice.
//!
//! ```
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
//!     "New terminal session",
//!     "Choose a machine",
//!     Root::Machines,
//!     Id::new("new-session"),
//! )
//! .on_press(Message::NewSession)
//! .into();
//! # let _ = button;
//! ```

use crate::button::{Button, ButtonLook};
use crate::segmented::{outlined, part_corners};
use crate::tooltip::Tip;
use crate::window::Toggle;
use cosmic::iced::Alignment;
use cosmic::widget::{self, tooltip::Position};
use cosmic::{Element, theme};

/// A main action joined to a menu opener (see the module's documentation)
pub struct SplitButton<'a, M, R> {
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
    /// A split button whose main half is labelled `label`, its opener named
    /// `opener_name` (also its tooltip: an icon alone says what it is,
    /// rule 4) and opening `root` of the window's chain, with `id` the
    /// opener's identity, which the root's panel is anchored at
    pub fn new(
        label: impl Into<String>,
        opener_name: impl Into<String>,
        root: R,
        id: impl Into<widget::Id>,
    ) -> Self {
        Self {
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

    /// What the main half does. Without it that half is disabled; the
    /// opener is not
    pub fn on_press(mut self, message: M) -> Self {
        self.on_press = Some(message);
        self
    }

    /// A 16 px icon before the main label
    pub fn leading_icon(mut self, icon: impl Into<widget::icon::Handle>) -> Self {
        self.leading = Some(icon.into());
        self
    }

    /// The main half's tooltip, below it
    pub fn tooltip(mut self, tip: Tip<'a, M>) -> Self {
        self.tip = Some(tip);
        self
    }

    /// Whether the opener's menu is open now: the opener is drawn chosen
    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }
}

impl<'a, M: Clone + 'a, R: Clone + 'static> From<SplitButton<'a, M, R>> for Element<'a, M> {
    fn from(split: SplitButton<'a, M, R>) -> Self {
        let radius = theme::active().cosmic().corner_radii.radius_xl;
        let mut main = Button::new(ButtonLook::Segment, split.label)
            .corners(part_corners(0, 2, radius))
            .on_press_maybe(split.on_press);
        if let Some(icon) = split.leading {
            main = main.leading_icon(icon);
        }
        if let Some(tip) = split.tip {
            main = main.tooltip(tip, Position::Bottom);
        }
        let name = split.opener_name;
        let words = name.clone();
        let tip = Tip::new(name.clone(), move || widget::text(words.clone()).into());
        let opener: Element<'a, Toggle> = Button::new(ButtonLook::Segment, name)
            .compact(true)
            .leading_icon(widget::icon::from_name("pan-down-symbolic").handle())
            .selected(split.open)
            .corners(part_corners(1, 2, radius))
            .tooltip(tip, Position::Bottom)
            .on_press(Toggle)
            .into();
        let opener = crate::window::trigger(
            split.id.clone(),
            split.root,
            crate::popup::focus_target(split.id, opener).on_activate(Toggle),
        );
        outlined(
            widget::row::with_children(vec![main.into(), opener])
                .align_y(Alignment::Center)
                .into(),
            2,
            split.open.then_some(1),
            true,
        )
    }
}

#[cfg(test)]
#[path = "split_button_tests.rs"]
mod tests;
