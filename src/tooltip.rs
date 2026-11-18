// SPDX-License-Identifier: GPL-3.0-only

//! The household tooltip bubble and its stable widget shape.

use crate::hover_card::{self, TEXT_MIN_WIDTH};
use crate::layout::{self, Alignment, AnchorCandidate, Side};
use cosmic::iced::Padding;
use cosmic::widget::{self, tooltip::Position};
use cosmic::{Element, theme};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

pub const TIP_WIDTH: f32 = 320.0;

pub fn tip_padding() -> Padding {
    let space = theme::active().cosmic().spacing;
    Padding::from([space.space_xs, space.space_s])
}

pub fn tip_bubble<'a, M: 'a>(
    tip: impl Into<Element<'a, M>>,
) -> widget::Container<'a, M, cosmic::Theme> {
    widget::container(tip).padding(tip_padding())
}

/// What a tooltip says, built afresh each time its box is measured or
/// drawn, with a key naming it: the same place saying something else is
/// another source of information, with its own delay.
pub struct Tip<'a, M> {
    key: u64,
    view: Rc<dyn Fn() -> Element<'a, M> + 'a>,
}

impl<'a, M: 'a> Tip<'a, M> {
    /// `key` is anything that tells this tip's content from another's.
    pub fn new(key: impl Hash, view: impl Fn() -> Element<'a, M> + 'a) -> Self {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        Self {
            key: hasher.finish(),
            view: Rc::new(view),
        }
    }
}

/// Where a tooltip may sit about its source: the named side first, then the
/// opposite one, centred on the source.
fn candidates(position: Position) -> Vec<AnchorCandidate> {
    let gap = theme::active().cosmic().spacing.space_xxs as f32;
    let sides = match position {
        Position::Top => [Side::Above, Side::Below],
        Position::Left => [Side::Left, Side::Right],
        Position::Right => [Side::Right, Side::Left],
        // Beneath the source: a source has no cursor-relative place
        Position::Bottom | Position::FollowCursor => [Side::Below, Side::Above],
    };
    sides
        .into_iter()
        .map(|side| AnchorCandidate::new(side, Alignment::Center).gap(gap))
        .collect()
}

/// Information about `content`, the window's one informational hover (after
/// the shared delay, at once on Alt+H, dismissed by Escape), through the
/// legacy-tooltip adapter: the source is retained by the tree.
pub fn tooltip_with<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    tip: Tip<'a, M>,
    position: Position,
) -> Element<'a, M> {
    let view = tip.view;
    let child = layout::child(move |_| view())
        .minimum_width(|_, preferred| preferred.width.min(TEXT_MIN_WIDTH));
    hover_card::retained_hover_box(tip.key, content.into(), child, candidates(position))
}

/// [`tooltip_with`], giving no information while `hidden`. The tree is the
/// same either way, so what `content` holds keeps its state.
pub fn tooltip_unless<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    tip: Tip<'a, M>,
    position: Position,
    hidden: bool,
) -> Element<'a, M> {
    let view = tip.view;
    let child = layout::child(move |_| view())
        .minimum_width(|_, preferred| preferred.width.min(TEXT_MIN_WIDTH))
        .hidden_when(move |_| hidden);
    hover_card::retained_hover_box(tip.key, content.into(), child, candidates(position))
}

/// A libcosmic tooltip, for the sites the window's information cannot yet
/// serve: a row of libcosmic's own menu (which gives its rows no viewport
/// and may be a native popup outside any window owner; these go with the
/// menus' migration), and compact tabs' label, whose place is the pointer's
/// (`Position::FollowCursor`). `hidden` leaves the same tree with no tip.
pub fn native_tooltip<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    tip: impl Into<Element<'a, M>>,
    position: Position,
    hidden: bool,
) -> Element<'a, M> {
    let padding = tip_padding();
    let tip: Element<'a, M> = if hidden {
        widget::Space::new().width(0.0).height(0.0).into()
    } else {
        tip_bubble(tip)
            .max_width(TIP_WIDTH + padding.left + padding.right)
            .into()
    };
    widget::tooltip(content, tip, position).padding(0.0).into()
}

pub fn link_tooltip<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    tip: Option<impl Into<Element<'a, M>>>,
) -> Element<'a, M> {
    match tip {
        Some(tip) => {
            let gap = theme::active().cosmic().spacing.space_xs;
            let bubble = tip_bubble(tip).class(theme::Container::Tooltip);
            let tip = widget::column::with_children(vec![
                bubble.into(),
                widget::Space::new().height(gap).into(),
            ]);
            widget::tooltip(content, tip, Position::FollowCursor)
                .padding(0.0)
                .class(theme::Container::Transparent)
                .into()
        }
        None => widget::tooltip(content, widget::Space::new(), Position::FollowCursor)
            .padding(0.0)
            .class(theme::Container::Transparent)
            .into(),
    }
}
