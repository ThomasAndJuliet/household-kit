// SPDX-License-Identifier: GPL-3.0-only

//! The household's one button. "These buttons should not actually behave
//! that differently in any case and should share most of their code"
//! (Juliet, 21 Nov 2026): every button look, Primary, Secondary, Tertiary,
//! Destructive and Icon, is this one [`Button`]; a look changes only its
//! paint and its side inset.
//!
//! # What every button shares
//!
//! - **Height:** the theme's `space_l` (32 px at standard density), as
//!   libcosmic's own text button is `space_l` tall. The content is centred
//!   in it, so nothing below sets a vertical padding.
//!   A toggle's part ([`ButtonLook::Segment`]) is the exception: toggles
//!   are [`TOGGLE_HEIGHT`] (26 px) tall, not buttons' height.
//! - **Width:** its content plus its look's side inset on each side
//!   ([`ButtonLook::inset`]), *measured in its largest state*. A button's
//!   size never depends on its state ("the hovered size should be the same
//!   as the non-hovered size from a layout perspective", Juliet, 21 Nov):
//!   hover, press, focus, selection and the disabled fade only repaint, and a
//!   label that changes with the button's own state (Copy, then Copied) is
//!   given to [`Button::other_labels`], so the button is laid out as wide as
//!   its widest label whichever one it shows. A *value* shown as the label
//!   (which model is chosen) is not a state: a new value is a new button.
//!   [`Button::width`] gives a fixed or filling width instead, the content
//!   centred in it.
//! - **The compact form** ([`Button::compact`]; rule 48, a responsive
//!   component owns its compact form): a `space_l` square showing only the
//!   leading icon or element, centred, with no side inset, so a mark up to
//!   the square's size (rule 10's 20 px identity marks) is drawn whole. The
//!   label stays its accessible name. The Icon look is always this square.
//! - **Hover and pressed washes** drawn inside the button's own box, with
//!   its pill corners (rule 25d: "very rounded for buttons"; libcosmic's
//!   `radius_xl`).
//! - **The disabled fade** (rule 6): with no message the button is disabled
//!   and its colours are mixed towards the page.
//! - **The keyboard-target outline**, drawn on the button's own edge, inside
//!   it (rule 6a; see [Borders](#borders)).
//! - **Its tooltip** ([`Button::tooltip`]; the Icon look cannot be built
//!   without one, rule 4) and its accessible name (the label).
//!
//! # What a look changes
//!
//! | look | paint | side inset |
//! |---|---|---|
//! | Primary | accent fill, on-accent semibold label | `space_s` |
//! | Secondary | 2 px accent border drawn inside, accent semibold label | `space_s` |
//! | Tertiary | accent bold underlined label, a wash on hover and press | `space_s` |
//! | Destructive | Primary's, in the theme's destructive colours | `space_s` |
//! | TertiaryDestructive | Tertiary's, in the destructive text colour | `space_s` |
//! | Icon | libcosmic's icon button; the muted accent while selected (on) | none: always the square |
//! | Segment | the Icon look's washes, the muted accent while chosen; a part of a [`crate::segmented::Segmented`] bar | `space_s` |
//! | Plain | the Icon look's paint, a regular body-text-colour label (not accent, not underlined) | `space_s` |
//!
//! Every labelled look can be chosen ([`Button::selected`]): the muted
//! accent fill and its on colours, the size unchanged.
//!
//! # Measuring, and a label that does not fit
//!
//! A button's width is whole pixels, and a parent asks it
//! ([`Button::natural_width`], [`width_of`]) rather than estimating: the
//! answer is the very measurement layout makes. The natural width is the
//! full label's; only a parent giving *less* room cuts the label, which then
//! ends in an ellipsis and, for a button with no tooltip of its own, is given
//! whole in a tooltip, while it is cut and only then (rules 14-16).
//!
//! Every size above is a theme spacing step (`theme.cosmic().spacing`,
//! which follows COSMIC's Density setting), never a pixel count. The fixed
//! sizes left are not layout: the 16 px action icon is an icon's role
//! (rule 10), as is a caller's 20 px identity mark, and the label's 14 px
//! type on a 20 px line is the button's typography.
//!
//! # Borders
//!
//! A border is part of its box (Juliet, 21 Nov 2026: "the iced approach of
//! considering the border as drawn inside the box ... makes the measurements
//! much more predictable"). A button draws every border it has, Secondary's
//! and the keyboard-target outline, *inside its own edge*, over its fill, and none of
//! them takes layout room: a button is the same size at rest, hovered,
//! pressed, focused and disabled, and whatever the width of its border.
//! Nothing is drawn outside the box. The side inset is measured from the
//! box's outer edge and is at least [`ButtonLook::thickest_border`], so a
//! border that thickens with state never meets the content.
//!
//! The keyboard's focus is the household's keyboard-target outline
//! ([`crate::keyboard_target`], rule 6a), the same for every look and for a
//! segmented bar's parts, and the same code that draws a navigation entry's:
//! a 1 px dark-grey stroke on the button's own edge, and on a solid fill
//! dark enough for it to be indistinct (Primary, Destructive, a chosen Icon
//! or Segment) a white stroke just inside it, nearest the fill. It is
//! resolved against the fill actually shown, not the look.
//!
//! # Example
//!
//! ```
//! use household_widgets::button::{Button, ButtonLook};
//! use cosmic::cosmic_theme::Spacing;
//!
//! // Every size is a spacing step, and every labelled look has the same
//! // box: at standard density the side inset is space_s (16 px) for each
//! let standard = Spacing::default();
//! assert_eq!(ButtonLook::Primary.inset(&standard), 16.0);
//! assert_eq!(ButtonLook::Tertiary.inset(&standard), 16.0);
//! assert_eq!(ButtonLook::Icon.inset(&standard), 0.0);
//!
//! // A Copy button that says Copied for a moment is as wide as "Copied"
//! // in both states; an app picks the look and nothing else
//! #[derive(Clone)]
//! enum Message { Copy }
//! let copy: cosmic::Element<'_, Message> = Button::secondary("Copy")
//!     .other_labels(["Copied"])
//!     .on_press(Message::Copy)
//!     .into();
//! # let _ = copy;
//! ```

use cosmic::iced::advanced::layout;
use cosmic::iced::advanced::renderer;
use cosmic::iced::advanced::text::LineHeight;
use cosmic::iced::advanced::widget::{Operation, Tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, mouse, overlay};
use cosmic::iced::widget::text::Wrapping;
use cosmic::iced::{Alignment, Background, Color, Event, Length, Padding, Rectangle, Size, Vector};
use cosmic::widget::tooltip::Position;
use cosmic::widget::{self, button};
use cosmic::{Element, Renderer, Theme, theme};
use household_tones::Srgb;

use crate::spacing::TOGGLE_HEIGHT;
use crate::tooltip::{Tip, tooltip_when_cut, tooltip_with};
use std::cell::Cell;
use std::rc::Rc;

/// The label's type: 14 px on a 20 px line, as libcosmic's buttons set it
pub(crate) const LABEL_SIZE: f32 = 14.0;
const LABEL_LINE: f32 = 20.0;
/// An action icon's size (rule 10): the 16 px canvas
pub const ACTION_ICON: u16 = 16;
/// Secondary's border, drawn inside its box (Juliet, 15 Oct: 2 px)
pub(crate) const SECONDARY_BORDER: f32 = 2.0;

/// The height of every button: the theme's `space_l`. A row that lines
/// buttons up with something else asks this rather than counting pixels
pub fn button_height() -> f32 {
    f32::from(theme::active().cosmic().spacing.space_l)
}

/// What a button looks like: its paint and its side inset (see the
/// module's table). Everything else is the one [`Button`]'s
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ButtonLook {
    /// The one main action of a view or dialog (rule 7)
    Primary,
    /// The other actions beside it
    Secondary,
    /// Low-weight actions (Learn more, Reset to default)
    Tertiary,
    /// What cannot be undone, and stop: Primary's shape in the theme's
    /// destructive colours
    Destructive,
    /// A rare destructive action kept out of the way: Tertiary in the
    /// theme's destructive text colour
    TertiaryDestructive,
    /// An icon alone (rule 4), its name in its tooltip
    Icon,
    /// One part of a segmented control ([`crate::segmented`]): no fill and
    /// the icon button's washes at rest, the muted accent while chosen
    /// ([`Button::selected`]), its label semibold when chosen and regular
    /// otherwise, measured in both ([`Button::corners`] joins it to its
    /// neighbours)
    Segment,
    /// The Icon look's paint (no fill at rest, the icon button's hover and
    /// pressed washes, the muted accent while selected) with a label in the
    /// body text colour: regular weight, not accent, not underlined. For a
    /// control that is a plain readable part of a line and still a button:
    /// an activity strip's icon and "×4" count, each opening its list. The
    /// same box as every labelled look
    Plain,
}

impl ButtonLook {
    /// The space at each side of a labelled button's content: `space_s`
    /// for every labelled look, so the same content makes the same box
    /// whatever the look, and only the paint differs (Juliet, 21 Nov 2026:
    /// "all button types have exactly the same box size and the only
    /// difference is the styling"; "Make all the buttons the same").
    /// None for Icon, which is always the square
    pub fn inset(self, spacing: &cosmic::cosmic_theme::Spacing) -> f32 {
        f32::from(match self {
            Self::Icon => 0,
            Self::Primary
            | Self::Secondary
            | Self::Destructive
            | Self::Segment
            | Self::Plain
            | Self::Tertiary
            | Self::TertiaryDestructive => spacing.space_s,
        })
    }

    /// The thickest border this look ever draws, in any state: Secondary's
    /// 2 px, or the keyboard-target outline's two strokes (1 px dark, 1 px
    /// white) on a solid fill. The look's inset is at least this, so no
    /// state's border meets the content, and none of them is added to the
    /// button's size
    pub fn thickest_border(self) -> f32 {
        SECONDARY_BORDER
    }

    fn tertiary(self) -> bool {
        matches!(self, Self::Tertiary | Self::TertiaryDestructive)
    }

    /// The looks that are drawn with the icon button's washes
    fn washed(self) -> bool {
        matches!(self, Self::Icon | Self::Segment | Self::Plain)
    }

    pub(crate) fn filled(self) -> bool {
        matches!(self, Self::Primary | Self::Destructive)
    }
}

/// A button's state as its paint shows it. None of these changes its size
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum State {
    Rest,
    Hovered,
    Pressed,
    Disabled,
}

/// A filled look's colours where the app knows them better than the theme
/// it is drawn in: libcosmic draws every window in the theme of the window
/// in front, so a window behind others gives its Send button its own faded
/// accent here ([`Button::fill_colours`])
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillColours {
    /// The fill at rest
    pub base: Color,
    pub hovered: Color,
    pub pressed: Color,
    /// The label and icons on the fill
    pub on: Color,
}

/// What a button's look draws in one state, for an app's tests to say
/// what they expect ([`paint`])
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paint {
    /// The fill inside the button's box: a filled look's fill, or a wash
    pub background: Option<Color>,
    /// A wash laid over the fill (a selected Icon's hover and press)
    pub overlay: Option<Color>,
    /// The label's and icons' colour; `None` leaves libcosmic's
    pub text: Option<Color>,
    /// A border drawn inside the box (Secondary's)
    pub border: Option<Color>,
}

/// The paint of `look` in `state`, `selected` (an Icon's toggle that is
/// on), with `fill` replacing a filled look's theme colours. For the Icon
/// look unselected this is libcosmic's own icon button, read back from
/// its theme
pub fn paint(
    look: ButtonLook,
    selected: bool,
    fill: Option<FillColours>,
    state: State,
    theme: &Theme,
) -> Paint {
    let cosmic = theme.cosmic();
    let page = crate::look::page(theme);
    if selected {
        // Chosen, on any look (rule 6: toggles and chosen parts in the muted
        // accent). Secondary keeps its border, so a chosen chip is still
        // the outlined pill it was
        let mut chosen = muted_paint(state, theme);
        if look == ButtonLook::Secondary {
            chosen.border = Some(unselected_border(state, theme));
        }
        return chosen;
    }
    if look.washed() {
        let style = icon_style(state, theme);
        let colour = |background: Option<Background>| match background {
            Some(Background::Color(colour)) => Some(colour),
            _ => None,
        };
        return Paint {
            background: colour(style.background),
            overlay: colour(style.overlay),
            text: style.icon_color,
            border: None,
        };
    }
    if look.filled() {
        let component = if look == ButtonLook::Destructive {
            &cosmic.destructive_button
        } else {
            &cosmic.accent_button
        };
        let fill = fill.unwrap_or_else(|| FillColours {
            base: component.base.into(),
            hovered: component.hover.into(),
            pressed: component.pressed.into(),
            // The on colour through the fill rule against the fill at
            // rest, greys going to that same on colour (rule 6), so
            // COSMIC's own is kept
            on: on_fill_on(component.on.into(), component.base.into()),
        });
        let background = match state {
            State::Rest => fill.base,
            State::Hovered => fill.hovered,
            State::Pressed => fill.pressed,
            State::Disabled => fade(fill.base, page),
        };
        // Disabled: on the faded fill, its own on colour, dimmed (rule 6,
        // "What is drawn on a fill")
        let text = match state {
            State::Disabled => on_disabled_fill(background),
            _ => fill.on,
        };
        return Paint {
            background: Some(background),
            overlay: None,
            text: Some(text),
            border: None,
        };
    }
    // Secondary and the Tertiary looks: no fill, COSMIC's button washes
    let background = match state {
        State::Hovered => Some(cosmic.icon_button.hover.into()),
        State::Pressed => Some(cosmic.icon_button.pressed.into()),
        State::Rest | State::Disabled => None,
    };
    let colour: Color = if look == ButtonLook::TertiaryDestructive {
        cosmic.destructive_text_color().into()
    } else {
        cosmic.accent_text_color().into()
    };
    let text = match state {
        State::Disabled => fade(colour, page),
        _ => colour,
    };
    Paint {
        background,
        overlay: None,
        text: Some(text),
        border: (look == ButtonLook::Secondary).then_some(text),
    }
}

/// Secondary's border colour in `state`
fn unselected_border(state: State, theme: &Theme) -> Color {
    paint(ButtonLook::Secondary, false, None, state, theme)
        .border
        .unwrap_or(Color::TRANSPARENT)
}

/// A toggle that is on (rule 6): the muted accent, COSMIC's washes laid
/// over it when hovered and pressed, so they come out darker (rule 6a);
/// disabled, the fill faded with its own on colour dimmed
fn muted_paint(state: State, theme: &Theme) -> Paint {
    let cosmic = theme.cosmic();
    let (muted, on) = crate::muted_colours(theme);
    match state {
        State::Disabled => {
            let fill = fade(muted, crate::look::page(theme));
            Paint {
                background: Some(fill),
                overlay: None,
                text: Some(on_disabled_fill(fill)),
                border: None,
            }
        }
        _ => Paint {
            background: Some(muted),
            overlay: match state {
                State::Hovered => Some(cosmic.icon_button.hover.into()),
                State::Pressed => Some(cosmic.icon_button.pressed.into()),
                _ => None,
            },
            text: Some(on),
            border: None,
        },
    }
}

/// libcosmic's icon button in `state`; disabled, its icon faded, as the
/// Icon class draws a disabled icon as strongly as an enabled one
fn icon_style(state: State, theme: &Theme) -> button::Style {
    use cosmic::widget::button::Catalog;
    let class = theme::Button::Icon;
    match state {
        State::Rest => theme.active(false, false, &class),
        State::Hovered => theme.hovered(false, false, &class),
        State::Pressed => theme.pressed(false, false, &class),
        State::Disabled => {
            let colour = fade(
                theme.cosmic().on_bg_color().into(),
                crate::look::page(theme),
            );
            button::Style {
                icon_color: Some(colour),
                text_color: Some(colour),
                ..button::Style::new()
            }
        }
    }
}

/// The libcosmic style of `look` in `state`
fn style(
    look: ButtonLook,
    selected: bool,
    fill: Option<FillColours>,
    state: State,
    corners: Option<[f32; 4]>,
    joined: bool,
    theme: &Theme,
) -> button::Style {
    let radius: [f32; 4] = corners.unwrap_or_else(|| theme.cosmic().corner_radii.radius_xl);
    if look.washed() && !selected {
        let mut style = icon_style(state, theme);
        style.border_radius = radius.into();
        style
    } else {
        let paint = paint(look, selected, fill, state, theme);
        button::Style {
            shadow_offset: Vector::default(),
            background: paint.background.map(Background::Color),
            overlay: paint.overlay.map(Background::Color),
            // Pills, every look, washes too (rule 25d); a segment's part is a
            // pill only at the bar's ends
            border_radius: radius.into(),
            border_width: if paint.border.is_some() && !joined {
                SECONDARY_BORDER
            } else {
                0.0
            },
            border_color: paint
                .border
                .filter(|_| !joined)
                .unwrap_or(Color::TRANSPARENT),
            outline_width: 0.0,
            outline_color: Color::TRANSPARENT,
            icon_color: paint.text,
            text_color: paint.text,
        }
    }
}

/// The solid fill `look` shows in `state` (its fill and any wash over it),
/// which the keyboard-target outline resolves its contrast against
pub(crate) fn displayed_fill(
    look: ButtonLook,
    selected: bool,
    state: State,
    corners: Option<[f32; 4]>,
    theme: &Theme,
) -> Option<Color> {
    crate::keyboard_target::displayed_solid(&style(
        look, selected, None, state, corners, false, theme,
    ))
}

/// libcosmic's own button `class` with its focus ring left off. libcosmic
/// draws focus as a 1 px accent line round the button, *outside* it, which
/// breaks the border rule (module documentation); a widget that draws the
/// household's keyboard-target outline itself (a menu row's contour, a menu
/// bar's title) uses this so that nothing else is drawn
pub(crate) fn unringed(class: theme::Button) -> theme::Button {
    use cosmic::widget::button::Catalog;
    let class = std::rc::Rc::new(class);
    theme::Button::Custom {
        active: Box::new({
            let class = class.clone();
            move |_, theme| theme.active(false, false, &class)
        }),
        disabled: Box::new({
            let class = class.clone();
            move |theme| theme.disabled(&class)
        }),
        hovered: Box::new({
            let class = class.clone();
            move |_, theme| theme.hovered(false, false, &class)
        }),
        pressed: Box::new(move |_, theme| theme.pressed(false, false, &class)),
    }
}

/// The libcosmic class drawing `look`
fn class(
    look: ButtonLook,
    selected: bool,
    fill: Option<FillColours>,
    corners: Option<[f32; 4]>,
    joined: bool,
    shown: Option<State>,
) -> theme::Button {
    theme::Button::Custom {
        active: Box::new(move |_, theme| {
            style(
                look,
                selected,
                fill,
                shown.unwrap_or(State::Rest),
                corners,
                joined,
                theme,
            )
        }),
        disabled: Box::new(move |theme| {
            style(
                look,
                selected,
                fill,
                State::Disabled,
                corners,
                joined,
                theme,
            )
        }),
        hovered: Box::new(move |_, theme| {
            style(
                look,
                selected,
                fill,
                shown.unwrap_or(State::Hovered),
                corners,
                joined,
                theme,
            )
        }),
        pressed: Box::new(move |_, theme| {
            style(
                look,
                selected,
                fill,
                shown.unwrap_or(State::Pressed),
                corners,
                joined,
                theme,
            )
        }),
    }
}

/// The Tertiary look's wash and label colour in `state`, for a Tertiary
/// drawn by another widget: the menu's More row ([`crate::menu_parts::tertiary_row_style`])
pub(crate) fn tertiary_paint(state: State, theme: &Theme) -> Paint {
    paint(ButtonLook::Tertiary, false, None, state, theme)
}

/// A colour mixed toward the ground, for what cannot act now; mixed
/// rather than made translucent, as a symbolic icon's colour loses its
/// alpha in some renderers
pub(crate) fn fade(on: Color, ground: Color) -> Color {
    let mix = |a: f32, b: f32| b + (a - b) * 0.4;
    Color::from_rgb(
        mix(on.r, ground.r),
        mix(on.g, ground.g),
        mix(on.b, ground.b),
    )
}

fn srgb(c: Color) -> Srgb {
    Srgb::new(c.r, c.g, c.b)
}

fn with_alpha(c: Srgb, a: f32) -> Color {
    Color {
        r: c.r,
        g: c.g,
        b: c.b,
        a,
    }
}

/// `on` on `fill` through the fill rule (rule 6), greys going to `on`
fn on_fill_on(on: Color, fill: Color) -> Color {
    with_alpha(
        household_tones::on_fill(srgb(on), srgb(fill), srgb(on)),
        on.a,
    )
}

/// A disabled item on the faded `fill` (rule 6): the faded fill's own on
/// colour, dimmed towards it
fn on_disabled_fill(fill: Color) -> Color {
    let on = household_tones::on_muted(srgb(fill));
    with_alpha(household_tones::dimmed_over(on, srgb(fill)), fill.a)
}

/// The household's one button (see the module's documentation). Pick a
/// look with a constructor, then say what it does
pub struct Button<'a, M> {
    look: ButtonLook,
    label: String,
    other_labels: Vec<String>,
    content: Option<Element<'a, M>>,
    other_contents: Vec<Element<'a, M>>,
    leading: Option<Element<'a, M>>,
    trailing: Option<widget::icon::Handle>,
    compact: bool,
    selected: bool,
    on_press: Option<M>,
    id: Option<widget::Id>,
    width: Length,
    start: bool,
    tip: Option<(Tip<'a, M>, Position)>,
    fill: Option<FillColours>,
    corners: Option<[f32; 4]>,
    joined: bool,
    shown: Option<State>,
}

impl<'a, M: Clone + 'a> Button<'a, M> {
    /// A button of `look` labelled `label`. The label is also its
    /// accessible name, in the compact form too
    pub fn new(look: ButtonLook, label: impl Into<String>) -> Self {
        Self {
            look,
            label: label.into(),
            other_labels: Vec::new(),
            content: None,
            other_contents: Vec::new(),
            leading: None,
            trailing: None,
            compact: false,
            selected: false,
            on_press: None,
            id: None,
            width: Length::Shrink,
            start: false,
            tip: None,
            fill: None,
            corners: None,
            joined: false,
            shown: None,
        }
    }

    /// A button of `look` whose content is `content`, an element of the
    /// caller's (a chip's name over its caption, a row's cells), in place of
    /// a label; `name` is its accessible name. It is the same box as every
    /// button: `space_l` tall, its look's side inset, the content centred,
    /// as wide as the content is, whole pixels. It is never cut with an
    /// ellipsis (the caller's content is the caller's to fit; give a
    /// [`Button::width`] and a content that fits it). A content that changes
    /// with the button's own state is given its other forms in
    /// [`Button::other_contents`]
    pub fn with_content(
        look: ButtonLook,
        content: Element<'a, M>,
        name: impl Into<String>,
    ) -> Self {
        let mut button = Self::new(look, name);
        button.content = Some(content);
        button
    }

    /// The other contents a [`Button::with_content`] button shows in its own
    /// states: it is laid out as wide as the widest, as
    /// [`Button::other_labels`] does for labels
    pub fn other_contents(mut self, contents: impl IntoIterator<Item = Element<'a, M>>) -> Self {
        self.other_contents = contents.into_iter().collect();
        self
    }

    pub fn primary(label: impl Into<String>) -> Self {
        Self::new(ButtonLook::Primary, label)
    }

    pub fn secondary(label: impl Into<String>) -> Self {
        Self::new(ButtonLook::Secondary, label)
    }

    pub fn tertiary(label: impl Into<String>) -> Self {
        Self::new(ButtonLook::Tertiary, label)
    }

    pub fn destructive(label: impl Into<String>) -> Self {
        Self::new(ButtonLook::Destructive, label)
    }

    pub fn tertiary_destructive(label: impl Into<String>) -> Self {
        Self::new(ButtonLook::TertiaryDestructive, label)
    }

    /// An icon button (rule 4): `icon` at the action icon's 16 px in the
    /// `space_l` square, named `name` for assistive technology, and the
    /// tooltip it cannot be built without, below it unless moved
    /// ([`Button::tooltip`])
    pub fn icon(icon: widget::icon::Icon, name: impl Into<String>, tip: Tip<'a, M>) -> Self {
        Self::icon_with(icon.size(ACTION_ICON).into(), name, tip)
    }

    /// [`Button::icon`] showing an element of the caller's, centred in the
    /// square: a 20 px identity mark (rule 10), a recoloured icon
    pub fn icon_with(content: Element<'a, M>, name: impl Into<String>, tip: Tip<'a, M>) -> Self {
        let mut button = Self::new(ButtonLook::Icon, name);
        button.leading = Some(content);
        button.tip = Some((tip, Position::Bottom));
        button
    }

    /// What a press does. Without it the button is disabled (faded)
    pub fn on_press(mut self, message: M) -> Self {
        self.on_press = Some(message);
        self
    }

    pub fn on_press_maybe(mut self, message: Option<M>) -> Self {
        self.on_press = message;
        self
    }

    /// A stable identity for focus navigation and for a menu it opens
    pub fn id(mut self, id: widget::Id) -> Self {
        self.id = Some(id);
        self
    }

    /// A 16 px icon before the label
    pub fn leading_icon(self, icon: impl Into<widget::icon::Handle>) -> Self {
        self.leading(widget::icon::icon(icon.into()).size(ACTION_ICON).into())
    }

    /// An element of the caller's before the label (a mark), shown alone
    /// in the compact form
    pub fn leading(mut self, element: Element<'a, M>) -> Self {
        self.leading = Some(element);
        self
    }

    /// A 16 px icon after the label; the compact form leaves it out
    pub fn trailing_icon(mut self, icon: impl Into<widget::icon::Handle>) -> Self {
        self.trailing = Some(icon.into());
        self
    }

    /// The other labels this button shows in its own states (Copy, then
    /// Copied): it is laid out as wide as the widest, whichever it shows,
    /// so its size never changes with its state
    pub fn other_labels<S: Into<String>>(mut self, labels: impl IntoIterator<Item = S>) -> Self {
        self.other_labels = labels.into_iter().map(Into::into).collect();
        self
    }

    /// The compact form: the `space_l` square with the leading icon or
    /// element centred in it and no side inset, the label kept as the
    /// accessible name. A mark up to the square's size is drawn whole; one
    /// larger is a design error, which layout limits to the square and
    /// clips (`button_tests::clipped` finds it in a test). With nothing
    /// leading there is nothing to show alone, and the button stays
    /// labelled
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// The corner radii (top left, top right, bottom right, bottom left)
    /// in place of the pill's, so a parent joining buttons can square the
    /// corners that touch a neighbour ([`crate::segmented::Segmented`])
    pub fn corners(mut self, radii: [f32; 4]) -> Self {
        self.corners = Some(radii);
        self
    }

    /// The button is one of boxes joined with no gap, so it draws no border
    /// of its own: its parent draws one outline round the group and any
    /// lines between the boxes ([`crate::spacing`], Borders; it is
    /// [`crate::split_button`]'s). Fill, washes, label colours and the
    /// keyboard-target outline are the look's as ever
    pub(crate) fn joined(mut self, joined: bool) -> Self {
        self.joined = joined;
        self
    }

    /// The button is drawn in `state` whatever the pointer does (a disabled
    /// button stays disabled): a part of [`crate::split_button`] whose menu
    /// is open is drawn pressed, as the whole button is
    pub(crate) fn shown_as(mut self, state: Option<State>) -> Self {
        self.shown = state;
        self
    }

    /// Whether the button is chosen (rule 6): a toggle that is on, a
    /// button whose menu or panel is open, the chosen part of a segmented
    /// control, a chosen chip or row. It is drawn in the muted accent with
    /// its on colours, on every look (Secondary keeps its 2 px border); the
    /// button's size does not change. For a chosen option of a list or a
    /// question use Secondary (a chip with a border) or Plain (a row or a
    /// strip part with no border at rest)
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// A fixed or filling width, the content centred in it, in place of
    /// the content's own
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Start the content at the inset instead of centring it in a wider
    /// box ([`Button::width`]). For parts of a form whose icons and labels
    /// stand in columns ([`crate::segmented::Segmented::form`]): centring
    /// each part's content in the one shared width would put each part's
    /// icon at its own x. Has no effect on a button as wide as its content
    pub fn content_start(mut self, start: bool) -> Self {
        self.start = start;
        self
    }

    /// The button's tooltip, on `position`'s side
    pub fn tooltip(mut self, tip: Tip<'a, M>, position: Position) -> Self {
        self.tip = Some((tip, position));
        self
    }

    /// Move the tooltip to `position`'s side
    pub fn tooltip_position(mut self, position: Position) -> Self {
        if let Some((_, at)) = self.tip.as_mut() {
            *at = position;
        }
        self
    }

    /// A filled look's colours from the app (see [`FillColours`]); `None`
    /// keeps the theme's
    pub fn fill_colours(mut self, fill: Option<FillColours>) -> Self {
        self.fill = fill;
        self
    }

    /// The width this button is laid out at with room to spare: its content
    /// (every label in every weight it draws, [`Button::other_labels`], the
    /// leading element and the trailing icon, with the row's spacing) plus its
    /// look's side inset on each side, in whole pixels. A parent that wants to
    /// know how much room the button needs asks this before layout and does
    /// not estimate (rule 48); it is the very measurement layout makes, so the
    /// two are equal (`button_tests` checks every look). The leading element
    /// is measured in place, hence `&mut self`
    ///
    /// ```
    /// use household_widgets::button::{Button, width_of, ButtonLook};
    /// # fn main() {
    /// # let renderer = household_widgets::button::headless_renderer().expect("native renderer");
    /// let mut button = Button::<()>::tertiary("Set up in Settings");
    /// let wide = button.natural_width(&renderer);
    /// assert_eq!(wide.fract(), 0.0);
    /// assert_eq!(wide, width_of(&renderer, ButtonLook::Tertiary, &["Set up in Settings"], None, false));
    /// # }
    /// ```
    pub fn natural_width(&mut self, renderer: &Renderer) -> f32 {
        let space = theme::active().cosmic().spacing;
        let square = f32::from(space.space_l);
        if self.is_compact() {
            return square;
        }
        let leading = self.leading.as_mut().map(|element| {
            let mut tree = Tree::new(element.as_widget());
            element
                .as_widget_mut()
                .layout(&mut tree, renderer, &unbounded())
                .size()
                .width
        });
        let mut forms = self.forms(false);
        let widest = natural(&mut forms, renderer).width;
        row_width(leading, widest, self.trailing.is_some(), &space) + 2.0 * self.look.inset(&space)
    }

    /// Whether the button is the compact square
    fn is_compact(&self) -> bool {
        (self.compact || self.look == ButtonLook::Icon) && self.leading.is_some()
    }

    /// Every form of the content the button must have room for: the label
    /// (in every weight it draws) and its other labels, or the caller's
    /// content and its other contents. `cut` makes labels the ellipsizing
    /// kind a parent's narrow room cuts
    fn forms(&mut self, cut: bool) -> Vec<Element<'a, M>> {
        if let Some(content) = self.content.take() {
            let mut forms = vec![content];
            forms.append(&mut self.other_contents);
            return forms;
        }
        label_forms(
            self.look,
            self.selected,
            cut,
            &self.label,
            &self.other_labels,
        )
    }

    /// The libcosmic button itself, wearing the household's keyboard-target
    /// outline ([`crate::keyboard_target`]), and its tooltip, for a place
    /// that wraps the button in something of its own (a menu opening from
    /// it) and shows the tooltip there. The tooltip is the one given; the
    /// full label's that shows only when a label is cut belongs to the
    /// finished [`Element`]
    pub fn into_native(self) -> (Element<'a, M>, Option<(Tip<'a, M>, Position)>) {
        let (button, tip, _) = self.assemble();
        (button, tip)
    }

    /// The button, its given tooltip, and, for a label that a narrow room may
    /// cut, the flag layout sets when it did cut it with the full label
    #[allow(clippy::type_complexity)]
    fn assemble(
        mut self,
    ) -> (
        Element<'a, M>,
        Option<(Tip<'a, M>, Position)>,
        Option<(Rc<Cell<bool>>, String)>,
    ) {
        let space = theme::active().cosmic().spacing;
        let square = f32::from(space.space_l);
        // A toggle's part is not as tall as a button (Juliet, 22 Nov 2026)
        let tall = if self.look == ButtonLook::Segment {
            TOGGLE_HEIGHT
        } else {
            square
        };
        let compact = self.is_compact();
        #[cfg(feature = "a11y")]
        let name = self.label.clone();
        let look = self.look;
        let mut cut_flag = None;
        let content: Element<'a, M> = if compact {
            self.leading
                .take()
                .unwrap_or_else(|| widget::Space::new().into())
        } else {
            let mut parts: Vec<Element<'a, M>> = Vec::with_capacity(3);
            parts.extend(self.leading.take());
            // An icon alone (a segmented part with no label): no label to
            // make a gap for, so the icon is all the content
            let icon_only = self.content.is_none() && self.label.is_empty() && !parts.is_empty();
            let labelled = self.content.is_none() && !icon_only;
            let shown = if labelled {
                Some(label(look, self.selected, true, self.label.clone()))
            } else {
                None
            };
            let flag = Rc::new(Cell::new(false));
            if labelled {
                cut_flag = Some((flag.clone(), self.label.clone()));
            }
            let reserve = if self.trailing.is_some() {
                f32::from(ACTION_ICON) + f32::from(space.space_xxs)
            } else {
                0.0
            };
            if !icon_only {
                let forms = self.forms(false);
                parts.push(Element::new(Widest {
                    labels: forms,
                    cut: shown,
                    reserve,
                    is_cut: flag,
                }));
            }
            parts.extend(
                self.trailing
                    .take()
                    .map(|icon| widget::icon::icon(icon).size(ACTION_ICON).into()),
            );
            Element::new(Whole {
                content: widget::row::with_children(parts)
                    .spacing(space.space_xxs)
                    .align_y(Alignment::Center)
                    .into(),
            })
        };
        let (width, padding) = if compact {
            (Length::Fixed(square), Padding::ZERO)
        } else {
            let inset = look.inset(&space);
            (self.width, Padding::from([0.0, inset]))
        };
        let enabled = self.on_press.is_some();
        let button = button::custom(content)
            .class(class(
                look,
                self.selected,
                self.fill,
                self.corners,
                self.joined,
                self.shown,
            ))
            .padding(padding)
            .height(Length::Fixed(tall))
            .width(width)
            .align_x(if self.start {
                Alignment::Start
            } else {
                Alignment::Center
            })
            .align_y(Alignment::Center)
            .on_press_maybe(self.on_press);
        let button = match self.id {
            Some(id) => button.id(id),
            None => button,
        };
        #[cfg(feature = "a11y")]
        let button = button.name(name);
        let button = Element::new(crate::keyboard_target::Target {
            content: button.into(),
            class: class(
                look,
                self.selected,
                self.fill,
                self.corners,
                self.joined,
                self.shown,
            ),
            enabled,
        });
        // A toggle's part with one rounded end has its centred content on
        // the centroid of its shape, not the middle of its box
        let button = match (look, self.start, self.corners) {
            (ButtonLook::Segment, false, Some(radii)) => Element::new(ShapeCentred {
                content: button,
                radii,
                height: tall,
            }),
            _ => button,
        };
        (button, self.tip, cut_flag)
    }
}

impl<'a, M: Clone + 'a> From<Button<'a, M>> for Element<'a, M> {
    /// The button with its tooltip. A button with no tooltip of its own
    /// whose label a parent's room cuts short ends the label in an ellipsis
    /// and gives it whole in a tooltip, and only then (rules 14-16)
    fn from(button: Button<'a, M>) -> Self {
        let (button, tip, cut) = button.assemble();
        match (tip, cut) {
            (Some((tip, position)), _) => tooltip_with(button, tip, position),
            (None, Some((flag, full))) => {
                let tip = Tip::new(("cut label", full.clone()), move || {
                    widget::text(full.clone()).into()
                });
                tooltip_when_cut(button, tip, Position::Bottom, flag)
            }
            (None, None) => button,
        }
    }
}

/// The width of a button of `look` with room to spare, from what it holds:
/// the `labels` (the label and its [`Button::other_labels`]), the width of a
/// leading element if any, and whether it has a trailing icon, in whole
/// pixels. This is [`Button::natural_width`] for a button an app has not
/// built yet; a [`Button`] with a leading element measures it in place
pub fn width_of(
    renderer: &Renderer,
    look: ButtonLook,
    labels: &[&str],
    leading: Option<f32>,
    trailing_icon: bool,
) -> f32 {
    let space = theme::active().cosmic().spacing;
    if look == ButtonLook::Icon && leading.is_some() {
        return f32::from(space.space_l);
    }
    let (first, others) = labels.split_first().map_or(("", &[][..]), |(f, o)| (*f, o));
    let others: Vec<String> = others.iter().map(|s| (*s).to_string()).collect();
    let mut forms: Vec<Element<'_, ()>> = label_forms(look, false, false, first, &others);
    let widest = natural(&mut forms, renderer).width;
    row_width(leading, widest, trailing_icon, &space) + 2.0 * look.inset(&space)
}

/// [`width_of`] for a view, which decides layout without a `Renderer`: the
/// text is measured by shaping it with the settings [`label`] draws it with
/// (the same [`label_font`], [`LABEL_SIZE`] and [`label_shaping`], no
/// wrapping), the widest of `labels` (the label and its
/// [`Button::other_labels`]; a segment's in both of its weights) kept. The
/// answer is whole pixels and equals [`Button::natural_width`] (`button_tests`
/// checks every look, in the apps' text environment). `leading` is the width
/// of a leading element, if any
pub fn label_width(
    look: ButtonLook,
    labels: &[&str],
    leading: Option<f32>,
    trailing_icon: bool,
) -> f32 {
    let space = theme::active().cosmic().spacing;
    if look == ButtonLook::Icon && leading.is_some() {
        return f32::from(space.space_l);
    }
    row_width(leading, text_width(look, labels), trailing_icon, &space) + 2.0 * look.inset(&space)
}

/// The widest of `labels` as `look` draws them, in each weight the look draws
/// (a segment's chosen and unchosen), unrounded
pub(crate) fn text_width(look: ButtonLook, labels: &[&str]) -> f32 {
    let chosen = [true, false];
    let weights = if look == ButtonLook::Segment {
        &chosen[..]
    } else {
        &chosen[..1]
    };
    labels
        .iter()
        .flat_map(|text| {
            weights.iter().map(move |chosen| {
                crate::menu::measured_width_shaped(
                    text,
                    LABEL_SIZE,
                    label_font(look, *chosen),
                    label_shaping(look),
                )
            })
        })
        .fold(0.0, f32::max)
}

/// The font a label of `look` is drawn in: semibold for the boxed looks; bold
/// for Tertiary; a segment's semibold when `chosen` and regular otherwise
/// (rule 6a's weights); Plain's regular
fn label_font(look: ButtonLook, chosen: bool) -> cosmic::iced::Font {
    if look.tertiary() {
        return cosmic::font::bold();
    }
    let regular = match look {
        ButtonLook::Segment => !chosen,
        ButtonLook::Plain => true,
        _ => false,
    };
    if regular {
        cosmic::font::default()
    } else {
        cosmic::font::semibold()
    }
}

/// How every label is shaped: in full, as the apps' libcosmic shapes text by
/// default (they build it with `advanced-shaping`) and as Tertiary's rich
/// text always is. Said here rather than left to the text widget's default,
/// which depends on a feature the build chooses, so that a label drawn and a
/// label measured ([`label_width`]) cannot be shaped differently, in an app or
/// in this crate's tests
fn label_shaping(_look: ButtonLook) -> cosmic::iced::advanced::text::Shaping {
    cosmic::iced::advanced::text::Shaping::Advanced
}

/// A headless renderer, for a doctest or an app's test to measure with;
/// `None` where the native tiny-skia renderer cannot be made. It does not pin
/// fonts: this crate's own tests build theirs with `test_fonts::renderer`
#[doc(hidden)]
pub fn headless_renderer() -> Option<Renderer> {
    use cosmic::iced::advanced::renderer::Headless;
    cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
        cosmic::iced::Font::default(),
        cosmic::iced::Pixels(14.0),
        Some("tiny-skia"),
    ))
}

/// The content row's width in whole pixels: its parts and the spacing
/// between them, as the row lays them out, then rounded up
fn row_width(
    leading: Option<f32>,
    widest: f32,
    trailing: bool,
    space: &cosmic::cosmic_theme::Spacing,
) -> f32 {
    // No label: an icon alone has no gap for one
    let mut parts: Vec<f32> = if widest > 0.0 || leading.is_none() {
        vec![widest.ceil()]
    } else {
        Vec::new()
    };
    parts.extend(leading);
    if trailing {
        parts.push(f32::from(ACTION_ICON));
    }
    let gaps = parts.len().saturating_sub(1) as f32;
    (parts.iter().sum::<f32>() + gaps * f32::from(space.space_xxs)).ceil()
}

fn unbounded() -> layout::Limits {
    layout::Limits::new(Size::ZERO, Size::new(f32::INFINITY, f32::INFINITY))
}

/// The largest size of `forms` laid out with all the room they want
fn natural<M>(forms: &mut [Element<'_, M>], renderer: &Renderer) -> Size {
    forms.iter_mut().fold(Size::ZERO, |most, form| {
        let mut tree = Tree::new(form.as_widget());
        let size = form
            .as_widget_mut()
            .layout(&mut tree, renderer, &unbounded())
            .size();
        Size::new(most.width.max(size.width), most.height.max(size.height))
    })
}

/// The forms of a label to measure: the label and `others`, each in the
/// weight `look` draws it, and a segment's in both of its weights so that
/// choosing moves nothing
fn label_forms<'a, M: 'a>(
    look: ButtonLook,
    chosen: bool,
    cut: bool,
    text: &str,
    others: &[String],
) -> Vec<Element<'a, M>> {
    let mut forms = vec![label(look, chosen, cut, text.to_string())];
    if look == ButtonLook::Segment {
        forms.push(label(look, !chosen, cut, text.to_string()));
    }
    for other in others {
        if look == ButtonLook::Segment {
            forms.push(label(look, !chosen, cut, other.clone()));
        }
        forms.push(label(look, chosen, cut, other.clone()));
    }
    forms
}

/// A label in `look`'s type: semibold for the boxed looks; bold and
/// underlined for Tertiary; a segment's semibold when `chosen` and regular
/// otherwise (rule 6a's weights); Plain's regular. `cut` makes it fill the
/// room it is given and end in an ellipsis if that is too little
fn label<'a, M: 'a>(look: ButtonLook, chosen: bool, cut: bool, text: String) -> Element<'a, M> {
    use cosmic::iced::advanced::text::{Ellipsize, EllipsizeHeightLimit};
    let ellipsis = Ellipsize::End(EllipsizeHeightLimit::Lines(1));
    if look.tertiary() {
        let span: cosmic::iced::widget::text::Span<'a, (), cosmic::iced::Font> =
            cosmic::iced::widget::text::Span::new(text)
                .underline(true)
                .font(label_font(look, chosen));
        let rich = cosmic::iced::widget::rich_text([span])
            .size(LABEL_SIZE)
            .line_height(LineHeight::Absolute(LABEL_LINE.into()))
            .wrapping(Wrapping::None);
        return if cut {
            rich.ellipsize(ellipsis).width(Length::Fill).into()
        } else {
            rich.into()
        };
    }
    let text = widget::text(text)
        .size(LABEL_SIZE)
        .line_height(LineHeight::Absolute(LABEL_LINE.into()))
        .font(label_font(look, chosen))
        .shaping(label_shaping(look))
        .wrapping(Wrapping::None);
    if cut {
        text.ellipsize(ellipsis).width(Length::Fill).into()
    } else {
        text.into()
    }
}

/// The content rounded up to whole pixels: its width is the next whole
/// pixel, so a button's width is whole whatever its content measures
struct Whole<'a, M> {
    content: Element<'a, M>,
}

impl<M> Widget<M, Theme, Renderer> for Whole<'_, M> {
    forward_widget!(M, Theme, Renderer; children, diff, size, operate, update, draw, mouse_interaction, overlay, drag_destinations, a11y_nodes);

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let node = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        let size = node.size();
        layout::Node::with_children(
            Size::new(size.width.ceil(), size.height),
            node.children().to_vec(),
        )
    }
}

// Test only: lay content out centred in the part's box, as it was before
// it was centred on the part's shape (for a picture of before and after)
#[cfg(test)]
thread_local! {
    pub(crate) static GEOMETRIC_CENTRING: Cell<bool> = const { Cell::new(false) };
}

/// A toggle's part with the content of the part laid out as centred in its
/// box, moved to be centred on the centroid of the part's shape
/// ([`crate::segmented::end_centroid_offset`]): an end part (one side
/// rounded) is nearer its divider than its box's middle. The content's x is
/// a whole pixel, so the same part is always the same and the parts of a bar
/// do not drift from one another. A part rounded at both ends or neither is
/// symmetric and its content is only put on a whole pixel. Layout only: the
/// tree, drawing, events and operations are the button's
struct ShapeCentred<'a, M> {
    content: Element<'a, M>,
    /// The part's corner radii: top left, top right, bottom right, bottom left
    radii: [f32; 4],
    height: f32,
}

impl<M> Widget<M, Theme, Renderer> for ShapeCentred<'_, M> {
    forward_widget!(M, Theme, Renderer; children, diff, size, operate, update, draw, mouse_interaction, overlay, drag_destinations, a11y_nodes);

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let node = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        #[cfg(test)]
        if GEOMETRIC_CENTRING.with(Cell::get) {
            return node;
        }
        let width = node.size().width;
        let [top_left, top_right, bottom_right, bottom_left] = self.radii;
        let left = top_left.max(bottom_left);
        let right = top_right.max(bottom_right);
        // Towards the divider: right for a part rounded only at its left
        let shift = if left > 0.0 && right <= 0.0 {
            crate::segmented::end_centroid_offset(width, self.height, left)
        } else if right > 0.0 && left <= 0.0 {
            -crate::segmented::end_centroid_offset(width, self.height, right)
        } else {
            0.0
        };
        let mut children = node.children().to_vec();
        if let Some(content) = children.first_mut() {
            let at = content.bounds();
            *content = content
                .clone()
                .move_to(cosmic::iced::Point::new((at.x + shift).round(), at.y));
        }
        layout::Node::with_children(node.size(), children)
    }
}

/// The first of `labels`, laid out in the room of the largest of them
/// all and centred in it: how a button is as wide in every state. Only
/// the first is drawn or seen by operations; the others are measured.
///
/// Given less room than the largest wants, and a `cut` form (the label
/// filling its room, ending in an ellipsis), it takes all the room and draws
/// that form instead, saying so in `is_cut`; a button's natural width stays
/// the full label's, and only a parent giving less cuts it. `reserve` is room
/// the row keeps after it (a trailing icon)
struct Widest<'a, M> {
    labels: Vec<Element<'a, M>>,
    cut: Option<Element<'a, M>>,
    reserve: f32,
    is_cut: Rc<Cell<bool>>,
}

impl<M> Widest<'_, M> {
    /// The tree of the cut form, after the labels'
    fn cut_tree<'t>(&self, tree: &'t Tree) -> Option<&'t Tree> {
        tree.children.get(self.labels.len())
    }
}

impl<M> Widget<M, Theme, Renderer> for Widest<'_, M> {
    fn children(&self) -> Vec<Tree> {
        self.labels
            .iter()
            .chain(self.cut.iter())
            .map(|label| Tree::new(label))
            .collect()
    }

    fn diff(&mut self, tree: &mut Tree) {
        let n = self.labels.len();
        tree.diff_children(self.labels.as_mut_slice());
        // The cut form's tree sits after the labels'; keep it in step
        if let Some(cut) = self.cut.as_mut() {
            if tree.children.len() <= n {
                tree.children.push(Tree::new(&*cut));
            }
            tree.children[n].diff(cut);
        }
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let n = self.labels.len();
        let open = layout::Limits::new(Size::ZERO, Size::new(f32::INFINITY, limits.max().height));
        let mut nodes = self
            .labels
            .iter_mut()
            .zip(tree.children.iter_mut())
            .map(|(label, tree)| label.as_widget_mut().layout(tree, renderer, &open));
        let Some(shown) = nodes.next() else {
            return layout::Node::new(Size::ZERO);
        };
        let size = nodes.fold(shown.size(), |most, node| {
            Size::new(
                most.width.max(node.size().width),
                most.height.max(node.size().height),
            )
        });
        let size = Size::new(size.width.ceil(), size.height);
        let room = limits.max().width - self.reserve;
        if let (Some(cut), true) = (
            self.cut.as_mut(),
            size.width > room + 0.01 && room.is_finite(),
        ) && let Some(cut_tree) = tree.children.get_mut(n)
        {
            let room = room.max(0.0);
            let fitted = cut.as_widget_mut().layout(
                cut_tree,
                renderer,
                &layout::Limits::new(Size::ZERO, Size::new(room, limits.max().height)),
            );
            self.is_cut.set(true);
            let whole = Size::new(room, size.height);
            return layout::Node::with_children(
                whole,
                vec![fitted.align(Alignment::Start, Alignment::Center, whole)],
            );
        }
        self.is_cut.set(false);
        let shown = shown.align(Alignment::Center, Alignment::Center, size);
        layout::Node::with_children(size, vec![shown])
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
        let Some(layout) = layout.children().next() else {
            return;
        };
        let shown = match (self.is_cut.get(), self.cut.as_ref(), self.cut_tree(tree)) {
            (true, Some(cut), Some(state)) => Some((cut, state)),
            _ => self.labels.first().zip(tree.children.first()),
        };
        if let Some((label, state)) = shown {
            if self.is_cut.get() {
                // A cut label's underline is measured on the whole label and
                // would run on past the ellipsis: clip it to the room given
                use cosmic::iced::advanced::Renderer as _;
                renderer.with_layer(layout.bounds(), |renderer| {
                    label
                        .as_widget()
                        .draw(state, renderer, theme, style, layout, cursor, viewport);
                });
            } else {
                label
                    .as_widget()
                    .draw(state, renderer, theme, style, layout, cursor, viewport);
            }
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if let (Some(label), Some(state), Some(layout)) = (
            self.labels.first_mut(),
            tree.children.first_mut(),
            layout.children().next(),
        ) {
            label
                .as_widget_mut()
                .operate(state, layout, renderer, operation);
        }
    }
}

#[cfg(test)]
#[path = "button_tests.rs"]
mod tests;
