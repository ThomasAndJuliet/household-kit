// SPDX-License-Identifier: GPL-3.0-only

//! The household padding system: the one table of space.
//!
//! # How it is meant to work
//!
//! Juliet's brief: "None of this should be hardcoded ... we want something
//! simple with few choices that users can get wrong. Ideally, parents should
//! make more choices than children." And of gaps: "they are meant to be in
//! between elements, not after each".
//!
//! - **Children set no space.** A control, a line of text or an icon reports
//!   the size of its content and nothing else: no padding, no margin.
//! - **A container has a [`Kind`], and the kind decides the space.** It gives
//!   the container's [`inset`](Kind::inset) (the space between its edge and
//!   its children) and its [`gap`](Kind::gap) (the space *between* its
//!   children). A developer's only spacing choice is the kind.
//! - **Gaps go between neighbours.** n children have n - 1 gaps, never one
//!   after the last: iced's `spacing` already works so, and a container that
//!   draws its own gaps must too. The space around all the children is the
//!   inset.
//! - **The numbers are libcosmic's spacing steps**, `space_xxxs` ...
//!   `space_l` ([`Spacing`]), which follow COSMIC's Density setting
//!   (Compact, Standard, Spacious). Nothing here is a pixel value, so every
//!   size follows the user's density. A widget asks for the steps with
//!   [`cosmic::theme::spacing()`] when it builds its view and passes them
//!   here; the functions take `&Spacing`, not the theme, so a test can ask
//!   what Compact gives without a window.
//!
//! The values are what the approved looks measure at Standard density,
//! written as steps (Standard: `space_xxxs` 4, `space_xxs` 8, `space_xs` 12,
//! `space_s` 16, `space_m` 24, `space_l` 32).
//!
//! | Kind | Inset | Gap |
//! |---|---|---|
//! | [`Page`](Kind::Page) | `space_m` at the sides | `space_m` between sections |
//! | [`Card`](Kind::Card) | `space_s` | `space_m` |
//! | [`InputCard`](Kind::InputCard) | `space_xs` | `space_xs` |
//! | [`InformationCard`](Kind::InformationCard) | `space_xs` above and below, `space_s` at the sides | `space_xxxs` |
//! | [`Popup`](Kind::Popup) | `space_xxxs` above and below, the surface's 1 px border at the sides | none between rows |
//! | [`DenseRow`](Kind::DenseRow) | `space_xxxs` / `space_xxs` | `space_xxs` |
//! | [`ListRow`](Kind::ListRow) | `space_xxs` / `space_xs` | `space_xxs` |
//! | [`MenuRow`](Kind::MenuRow) | at least `space_l` tall, content centred; `space_xxs` at the sides | `space_xxs` |
//! | [`GroupRelated`](Kind::GroupRelated) | none | `space_xs` |
//! | [`GroupSeparate`](Kind::GroupSeparate) | none | `space_m` |
//! | [`Toolbar`](Kind::Toolbar) | `space_xxs` | `space_xs` |
//! | [`Tab`](Kind::Tab) | `space_s` at the sides (a close control is nested: it sits one gap from the end) | `space_xxs` |
//! | [`Header`](Kind::Header) | `space_xs` | `space_xxs` |
//!
//! A row that centres its content in `space_l` has no pixel for the space
//! above and below: it is what is left when the line of text is centred
//! ([`centred_in_control`]).
//!
//! # What is not a kind
//!
//! Every **control** (a button, a dropdown's trigger, a menu bar's title, an
//! overflow button) is [`control_height`] tall with its content centred, and
//! sets no vertical padding at all; a control's side room belongs to its look
//! ([`ButtonLook::inset`](crate::button::ButtonLook::inset)). Those are not
//! containers, so a developer is never asked to choose a kind for them. The
//! kinds above are the *containers*, the only thing a developer chooses
//! between. What a container does inside itself (the frame an open folder
//! draws inside a menu, a close control inside a tab) is the container's own
//! detail, derived from its steps.
//!
//! # Borders take no room
//!
//! "I think the iced approach of considering the border as drawn inside the
//! box is the approach we want to take as it makes the measurements much more
//! predictable" (Juliet, 21 Nov 2026).
//!
//! - **A border is part of its box.** It is drawn inside the box's edge and
//!   never takes layout room: a box is the same size with or without a
//!   border, and whatever the border's width. Nothing is drawn outside a
//!   box, so no parent keeps room for a focus ring or a thicker line.
//! - **An inset is measured from the box's outer edge** and is at least the
//!   thickest border that box ever draws, so a border that thickens with
//!   state (a selected content block's 2 px, a focused text box's 2 px, a
//!   menu's frame's 2 px) never moves its content. The space between border
//!   and content is the inset minus the border. Code never adds a border's
//!   width to a padding or a size; where a widget needs the border's width it
//!   asks for the *thickest* ([`Look::thickest_border`](crate::look::Look::thickest_border),
//!   [`ButtonLook::thickest_border`](crate::button::ButtonLook::thickest_border))
//!   and takes the larger of that and the kind's inset.
//! - **Boxes joined with no gap** (a segmented bar's parts, table cells) do
//!   not each draw an outer border: their parent draws one outline round the
//!   group inside its own edge, and any lines between them
//!   ([`crate::segmented`]).
//! - **Focus is drawn on the box's own border**, inside its edge: focus
//!   changes an outline the component already has (rule 6a). A button's ring
//!   is on the button's edge ([`crate::button`]); a menu row's keyboard
//!   outline is 1 px inside the row's bounds; a navigation entry's double
//!   outline on a dark fill is both strokes inside the edge
//!   ([`crate::navigation_style`]).
//!
//! # Example
//!
//! ```
//! use household_widgets::spacing::{Kind, centred_in_control};
//! use cosmic::cosmic_theme::{Density, Spacing};
//!
//! let standard = Spacing::default();
//! // A card is padded `space_s` all round, with `space_m` between sections
//! assert_eq!(Kind::Card.inset(&standard).left, 16.0);
//! assert_eq!(Kind::Card.gap(&standard), 24.0);
//! // A menu row is `space_l` tall with a 20 px line centred in it
//! assert_eq!(centred_in_control(&standard, 20.0), 6.0);
//!
//! // The same kinds in Compact density are smaller, with no other change
//! let compact = Spacing::from(Density::Compact);
//! assert_eq!(Kind::Card.inset(&compact).left, 8.0);
//! assert_eq!(centred_in_control(&compact, 20.0), 2.0);
//! ```

use cosmic::cosmic_theme::Spacing;
use cosmic::iced::Padding;

/// The line of the label type role (14 px text on a 20 px line): what a
/// control centres in its `space_l`. A type role, not a layout value
pub const LABEL_LINE: f32 = 20.0;

/// What a container is for, which decides its inset and its gap (see the
/// module's table)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A window's content area: one column of sections
    Page,
    /// A framed block of related content
    Card,
    /// The composer: a rounded box the user types into
    InputCard,
    /// A plain-text tooltip (rule 16a)
    InformationCard,
    /// A menu or popup's frame: rows run to its sides
    Popup,
    /// A row in a sidebar
    DenseRow,
    /// A row in a content list
    ListRow,
    /// A row in a menu: at least `space_l` tall, its content centred
    MenuRow,
    /// Actions that belong together
    GroupRelated,
    /// Actions that are not (when in doubt, they are not)
    GroupSeparate,
    /// Actions that appear over a block or figure on hover
    Toolbar,
    /// A tab's content
    Tab,
    /// A panel's header row
    Header,
}

impl Kind {
    /// The space between the container's edge and its children
    pub fn inset(self, spacing: &Spacing) -> Padding {
        let [xxxs, xxs, xs, s, m] = steps(spacing);
        match self {
            Self::Page => Padding::from([0.0, m]),
            Self::Card => Padding::new(s),
            Self::InputCard | Self::Header => Padding::new(xs),
            Self::InformationCard => Padding::from([xs, s]),
            Self::Popup => Padding::from([xxxs, crate::popup_surface::BORDER]),
            Self::Toolbar => Padding::new(xxs),
            Self::DenseRow => Padding::from([xxxs, xxs]),
            Self::ListRow => Padding::from([xxs, xs]),
            Self::MenuRow => Padding::from([centred_in_control(spacing, LABEL_LINE), xxs]),
            Self::GroupRelated | Self::GroupSeparate => Padding::ZERO,
            Self::Tab => Padding::from([0.0, s]),
        }
    }

    /// The space between the container's children: between neighbours,
    /// never after the last
    pub fn gap(self, spacing: &Spacing) -> f32 {
        let [xxxs, xxs, xs, _, m] = steps(spacing);
        match self {
            Self::Page | Self::Card | Self::GroupSeparate => m,
            Self::InputCard | Self::GroupRelated | Self::Toolbar => xs,
            Self::InformationCard => xxxs,
            Self::DenseRow | Self::ListRow | Self::MenuRow | Self::Tab | Self::Header => xxs,
            Self::Popup => 0.0,
        }
    }
}

/// The height of every control: `space_l`, as libcosmic's own text button
pub fn control_height(spacing: &Spacing) -> f32 {
    f32::from(spacing.space_l)
}

/// What is left above and below a line of `line` pixels centred in a
/// control's height ([`control_height`]); none if the line is taller. This
/// is where a menu row's 6 px comes from: `(space_l - 20) / 2`
pub fn centred_in_control(spacing: &Spacing, line: f32) -> f32 {
    ((control_height(spacing) - line) / 2.0).max(0.0)
}

/// The steps the kinds are made of, smallest first:
/// `space_xxxs`, `space_xxs`, `space_xs`, `space_s`, `space_m`
fn steps(spacing: &Spacing) -> [f32; 5] {
    [
        f32::from(spacing.space_xxxs),
        f32::from(spacing.space_xxs),
        f32::from(spacing.space_xs),
        f32::from(spacing.space_s),
        f32::from(spacing.space_m),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::cosmic_theme::Density;

    const ALL: [Kind; 13] = [
        Kind::Page,
        Kind::Card,
        Kind::InputCard,
        Kind::InformationCard,
        Kind::Popup,
        Kind::DenseRow,
        Kind::ListRow,
        Kind::MenuRow,
        Kind::GroupRelated,
        Kind::GroupSeparate,
        Kind::Toolbar,
        Kind::Tab,
        Kind::Header,
    ];

    fn pad(top: f32, right: f32, bottom: f32, left: f32) -> Padding {
        Padding {
            top,
            right,
            bottom,
            left,
        }
    }

    #[test]
    fn standard_density_gives_the_approved_values() {
        let s = Spacing::default();
        assert_eq!(Kind::Page.inset(&s), pad(0.0, 24.0, 0.0, 24.0));
        assert_eq!(Kind::Card.inset(&s), pad(16.0, 16.0, 16.0, 16.0));
        assert_eq!(Kind::InformationCard.inset(&s), pad(12.0, 16.0, 12.0, 16.0));
        assert_eq!(Kind::Popup.inset(&s), pad(4.0, 1.0, 4.0, 1.0));
        assert_eq!(Kind::MenuRow.inset(&s), pad(6.0, 8.0, 6.0, 8.0));
        assert_eq!(Kind::Tab.inset(&s), pad(0.0, 16.0, 0.0, 16.0));
        assert_eq!(Kind::GroupRelated.gap(&s), 12.0);
        assert_eq!(Kind::GroupSeparate.gap(&s), 24.0);
        assert_eq!(control_height(&s), 32.0);
    }

    /// A box's border is drawn inside its edge and takes no layout room, so
    /// a container's inset must cover the thickest border its box draws
    #[test]
    fn insets_cover_the_thickest_border_of_their_box() {
        use crate::look::Look;
        let theme = cosmic::Theme::default();
        let s = Spacing::default();
        let least = |inset: Padding| inset.top.min(inset.right).min(inset.bottom).min(inset.left);
        // The popup surface: its 1 px border, in every side
        assert!(least(Kind::Popup.inset(&s)) >= crate::popup_surface::BORDER);
        // The composer's box is the prominent look: 4 px when focused
        assert!(least(Kind::InputCard.inset(&s)) >= Look::prominent().thickest_border(&theme));
        // Text boxes: 2 px focused, with no kind padding of their own
        assert_eq!(Look::normal().thickest_border(&theme), 2.0);
        assert_eq!(Look::prominent().thickest_border(&theme), 4.0);
        // The menu's inline frame draws that border inside an inset of its size
        assert_eq!(crate::menu_parts::frame_inset(), 2.0);
        for kind in [Kind::Card, Kind::InformationCard, Kind::Toolbar, Kind::Header] {
            assert!(least(kind.inset(&s)) >= Look::normal().thickest_border(&theme), "{kind:?}");
        }
    }

    #[test]
    fn every_kind_follows_the_density() {
        let (compact, standard, spacious) = (
            Spacing::from(Density::Compact),
            Spacing::default(),
            Spacing::from(Density::Spacious),
        );
        for kind in ALL {
            let sum = |s: &Spacing| {
                let p = kind.inset(s);
                p.top + p.right + p.bottom + p.left + kind.gap(s)
            };
            assert!(sum(&compact) <= sum(&standard), "{kind:?}");
            assert!(sum(&standard) <= sum(&spacious), "{kind:?}");
        }
        assert_eq!(Kind::Card.inset(&compact).left, 8.0);
        assert_eq!(Kind::MenuRow.inset(&compact).top, 2.0);
        assert_eq!(control_height(&compact), 24.0);
    }

    #[test]
    fn a_line_taller_than_the_control_leaves_nothing() {
        assert_eq!(
            centred_in_control(&Spacing::from(Density::Compact), 40.0),
            0.0
        );
    }
}
