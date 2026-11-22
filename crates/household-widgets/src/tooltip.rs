// SPDX-License-Identifier: GPL-3.0-only

//! The household tooltip bubble and its stable widget shape.

use crate::hover_card::{self, TEXT_MIN_WIDTH};
use crate::layout::{self, Alignment, AnchorCandidate, Side};
use crate::spacing::Kind;
use cosmic::iced::Padding;
use cosmic::widget::{self, tooltip::Position};
use cosmic::{Element, theme};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

pub const TIP_WIDTH: f32 = 320.0;

pub fn tip_padding() -> Padding {
    Kind::InformationCard.inset(&theme::spacing())
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
/// opposite one, flush with the source and centred on it, moved inward only
/// as far as the window requires.
fn candidates(position: Position) -> Vec<AnchorCandidate> {
    let sides = match position {
        Position::Top => [Side::Above, Side::Below],
        Position::Left => [Side::Left, Side::Right],
        Position::Right => [Side::Right, Side::Left],
        // Beneath the source: a source has no cursor-relative place
        Position::Bottom | Position::FollowCursor => [Side::Below, Side::Above],
    };
    sides
        .into_iter()
        .map(|side| AnchorCandidate::new(side, Alignment::CenterBounded))
        .collect()
}

/// The legacy-tooltip adapter: information about `content`, the window's one
/// informational hover (after the shared delay, at once on Alt+H, dismissed
/// by Escape). The source is retained by the tree, so the widget is the
/// same whether the tip is `hidden` or not.
fn adapt<'a, M: 'a>(
    content: Element<'a, M>,
    tip: Tip<'a, M>,
    position: Position,
    at: Option<f32>,
    hidden: impl Fn() -> bool + 'a,
    claims: Option<Rc<dyn Fn() -> bool + 'a>>,
) -> Element<'a, M> {
    let view = tip.view;
    let child = layout::child(move |_| view())
        .minimum_width(|_, preferred| preferred.width.min(TEXT_MIN_WIDTH))
        .hidden_when(move |_| hidden());
    hover_card::retained_hover_box(tip.key, at, content, child, candidates(position), claims)
}

/// Information about `content` as a tooltip says it.
pub fn tooltip_with<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    tip: Tip<'a, M>,
    position: Position,
) -> Element<'a, M> {
    adapt(content.into(), tip, position, None, || false, None)
}

/// [`tooltip_with`], giving no information while `hidden`. The tree is the
/// same either way, so what `content` holds keeps its state.
pub fn tooltip_unless<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    tip: Tip<'a, M>,
    position: Position,
    hidden: bool,
) -> Element<'a, M> {
    adapt(content.into(), tip, position, None, move || hidden, None)
}

/// [`tooltip_with`] for text that may be cut short: the household's
/// convention for cut-off text (rules 14-16) is the ellipsis and the whole
/// text in a tooltip, and *only* when it is cut. `cut` is the flag the
/// content sets while it is laid out cut (the button's label does), read each
/// time the tip would show, so a tip is given only while the text is cut.
pub fn tooltip_when_cut<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    tip: Tip<'a, M>,
    position: Position,
    cut: std::rc::Rc<std::cell::Cell<bool>>,
) -> Element<'a, M> {
    // Uncut, the box is not the hover's: it gives the pointer to whatever
    // else is under it (an outer tooltip), as if it were not there
    let claims: Rc<dyn Fn() -> bool + 'a> = Rc::new({
        let cut = cut.clone();
        move || cut.get()
    });
    adapt(content.into(), tip, position, None, move || !cut.get(), Some(claims))
}

/// [`tooltip_unless`] about the one place `x` pixels from `content`'s left
/// edge, which a wide control (a tab bar) says it is hovered at: the card
/// sits there, not at the whole control.
pub fn tooltip_at<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    tip: Tip<'a, M>,
    position: Position,
    x: f32,
    hidden: bool,
) -> Element<'a, M> {
    adapt(content.into(), tip, position, Some(x), move || hidden, None)
}

/// A libcosmic tooltip, for the two sites the window's information cannot
/// yet serve, both in this crate: a greyed row of a text box's menu,
/// libcosmic's popup outside any window owner (until editor menus move,
/// package 2), and compact tabs' label, whose place is the pointer's
/// (`Position::FollowCursor`; the shared tab strip, Stage 6). `hidden`
/// leaves the same tree with no tip.
pub(crate) fn native_tooltip<'a, M: 'a>(
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

#[cfg(test)]
mod tests {
    /// Pin (Standard density): an information card is 12 px above and
    /// below its text and 16 at the sides
    #[test]
    fn pin_standard_density_tip_padding() {
        let standard = cosmic::cosmic_theme::Spacing::default();
        assert_eq!(standard.space_xs, 12);
        assert_eq!(standard.space_s, 16);
        let padding = super::tip_padding();
        if cosmic::theme::spacing() == standard {
            assert_eq!((padding.top, padding.left), (12.0, 16.0));
        }
    }

    /// libcosmic's tooltip is called in this file alone: every other
    /// tooltip is the window's information or goes through
    /// [`super::native_tooltip`]
    #[test]
    fn no_production_code_bypasses_the_tooltip_helpers() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = Vec::new();
        for entry in std::fs::read_dir(root).unwrap().flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if !name.ends_with(".rs") || name.ends_with("_tests.rs") || name == "tooltip.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            // What follows a `#[cfg(test)]` is test code
            let production = text.split("#[cfg(test)]").next().unwrap_or("");
            for (n, line) in production.lines().enumerate() {
                if line.contains("widget::tooltip(") || line.contains("Tooltip::new(") {
                    found.push(format!("{name}:{}", n + 1));
                }
            }
        }
        assert!(
            found.is_empty(),
            "tooltips that bypass the helpers: {found:?}"
        );
    }
}
