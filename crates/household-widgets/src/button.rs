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
//! - **The keyboard focus ring** on its box (libcosmic's: a 1 px accent
//!   line round it; rule 6a).
//! - **Its tooltip** ([`Button::tooltip`]; the Icon look cannot be built
//!   without one, rule 4) and its accessible name (the label).
//!
//! # What a look changes
//!
//! | look | paint | side inset |
//! |---|---|---|
//! | Primary | accent fill, on-accent semibold label | `space_s` |
//! | Secondary | 2 px accent border drawn inside, accent semibold label | `space_s` |
//! | Tertiary | accent bold underlined label, a wash on hover and press | `space_xxs` |
//! | Destructive | Primary's, in the theme's destructive colours | `space_s` |
//! | TertiaryDestructive | Tertiary's, in the destructive text colour | `space_xxs` |
//! | Icon | libcosmic's icon button; the muted accent while selected (on) | none: always the square |
//!
//! Every size above is a theme spacing step (`theme.cosmic().spacing`,
//! which follows COSMIC's Density setting), never a pixel count. The fixed
//! sizes left are not layout: the 16 px action icon is an icon's role
//! (rule 10), as is a caller's 20 px identity mark, and the label's 14 px
//! type on a 20 px line is the button's typography.
//!
//! # Example
//!
//! ```
//! use household_widgets::button::{Button, ButtonLook};
//! use cosmic::cosmic_theme::Spacing;
//!
//! // Every size is a spacing step: at standard density the Primary look's
//! // side inset is space_s (16 px) and Tertiary's space_xxs (8 px)
//! let standard = Spacing::default();
//! assert_eq!(ButtonLook::Primary.inset(&standard), 16.0);
//! assert_eq!(ButtonLook::Tertiary.inset(&standard), 8.0);
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
use cosmic::iced::advanced::{Layout, Widget};
use cosmic::iced::widget::text::Wrapping;
use cosmic::iced::{Alignment, Background, Color, Length, Padding, Rectangle, Size, Vector, mouse};
use cosmic::widget::tooltip::Position;
use cosmic::widget::{self, button};
use cosmic::{Element, Renderer, Theme, theme};
use household_tones::Srgb;

use crate::tooltip::{Tip, tooltip_with};

/// The label's type: 14 px on a 20 px line, as libcosmic's buttons set it
const LABEL_SIZE: f32 = 14.0;
const LABEL_LINE: f32 = 20.0;
/// An action icon's size (rule 10): the 16 px canvas
pub const ACTION_ICON: u16 = 16;
/// Secondary's border, drawn inside its box (Juliet, 15 Oct: 2 px)
const SECONDARY_BORDER: f32 = 2.0;

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
}

impl ButtonLook {
    /// The space at each side of a labelled button's content: `space_s`
    /// for the boxed looks, `space_xxs` for Tertiary, none for Icon (always
    /// the square)
    pub fn inset(self, spacing: &cosmic::cosmic_theme::Spacing) -> f32 {
        f32::from(match self {
            Self::Primary | Self::Secondary | Self::Destructive => spacing.space_s,
            Self::Tertiary | Self::TertiaryDestructive => spacing.space_xxs,
            Self::Icon => 0,
        })
    }

    fn tertiary(self) -> bool {
        matches!(self, Self::Tertiary | Self::TertiaryDestructive)
    }

    fn filled(self) -> bool {
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
    if look == ButtonLook::Icon {
        if selected {
            return muted_paint(state, theme);
        }
        let style = icon_style(state, false, theme);
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
fn icon_style(state: State, focused: bool, theme: &Theme) -> button::Style {
    use cosmic::widget::button::Catalog;
    let class = theme::Button::Icon;
    match state {
        State::Rest => theme.active(focused, false, &class),
        State::Hovered => theme.hovered(focused, false, &class),
        State::Pressed => theme.pressed(focused, false, &class),
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
    focused: bool,
    theme: &Theme,
) -> button::Style {
    if look == ButtonLook::Icon && !selected {
        return icon_style(state, focused, theme);
    }
    let paint = paint(look, selected, fill, state, theme);
    let style = button::Style {
        shadow_offset: Vector::default(),
        background: paint.background.map(Background::Color),
        overlay: paint.overlay.map(Background::Color),
        // Pills, every look, washes too (rule 25d)
        border_radius: theme.cosmic().corner_radii.radius_xl.into(),
        border_width: if paint.border.is_some() {
            SECONDARY_BORDER
        } else {
            0.0
        },
        border_color: paint.border.unwrap_or(Color::TRANSPARENT),
        outline_width: 0.0,
        outline_color: Color::TRANSPARENT,
        icon_color: paint.text,
        text_color: paint.text,
    };
    focus_ring(style, focused, theme)
}

/// The keyboard's focus on a button: libcosmic's ring, a 1 px line of the
/// accent round its box, as its own buttons have. It is drawn round the
/// box, so it moves nothing
fn focus_ring(mut style: button::Style, focused: bool, theme: &Theme) -> button::Style {
    if focused {
        style.outline_width = 1.0;
        style.outline_color = theme.cosmic().accent.base.into();
        style.border_width = 2.0;
        style.border_color = Color::TRANSPARENT;
    }
    style
}

/// The libcosmic class drawing `look`
fn class(look: ButtonLook, selected: bool, fill: Option<FillColours>) -> theme::Button {
    theme::Button::Custom {
        active: Box::new(move |focused, theme| {
            style(look, selected, fill, State::Rest, focused, theme)
        }),
        disabled: Box::new(move |theme| style(look, selected, fill, State::Disabled, false, theme)),
        hovered: Box::new(move |focused, theme| {
            style(look, selected, fill, State::Hovered, focused, theme)
        }),
        pressed: Box::new(move |focused, theme| {
            style(look, selected, fill, State::Pressed, focused, theme)
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
    leading: Option<Element<'a, M>>,
    trailing: Option<widget::icon::Handle>,
    compact: bool,
    selected: bool,
    on_press: Option<M>,
    id: Option<widget::Id>,
    width: Length,
    tip: Option<(Tip<'a, M>, Position)>,
    fill: Option<FillColours>,
}

impl<'a, M: Clone + 'a> Button<'a, M> {
    /// A button of `look` labelled `label`. The label is also its
    /// accessible name, in the compact form too
    pub fn new(look: ButtonLook, label: impl Into<String>) -> Self {
        Self {
            look,
            label: label.into(),
            other_labels: Vec::new(),
            leading: None,
            trailing: None,
            compact: false,
            selected: false,
            on_press: None,
            id: None,
            width: Length::Shrink,
            tip: None,
            fill: None,
        }
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

    /// Whether an Icon button is on (rule 6): a toggle that is on, a button
    /// whose menu or panel is open. It is drawn in the muted accent. The
    /// labelled looks have no selected paint
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

    /// The libcosmic button itself and its tooltip, for a place that wraps
    /// the button in something of its own (a menu opening from it) and
    /// shows the tooltip there
    pub fn into_native(self) -> (button::Button<'a, M>, Option<(Tip<'a, M>, Position)>) {
        let space = theme::active().cosmic().spacing;
        let square = f32::from(space.space_l);
        let compact = (self.compact || self.look == ButtonLook::Icon) && self.leading.is_some();
        #[cfg(feature = "a11y")]
        let name = self.label.clone();
        let look = self.look;
        let content: Element<'a, M> = if compact {
            self.leading.unwrap_or_else(|| widget::Space::new().into())
        } else {
            let mut parts: Vec<Element<'a, M>> = Vec::with_capacity(3);
            parts.extend(self.leading);
            let mut labels = vec![label(look, self.label)];
            labels.extend(
                self.other_labels
                    .into_iter()
                    .map(|other| label(look, other)),
            );
            parts.push(Element::new(Widest { labels }));
            parts.extend(
                self.trailing
                    .map(|icon| widget::icon::icon(icon).size(ACTION_ICON).into()),
            );
            widget::row::with_children(parts)
                .spacing(space.space_xxs)
                .align_y(Alignment::Center)
                .into()
        };
        let (width, padding) = if compact {
            (Length::Fixed(square), Padding::ZERO)
        } else {
            let inset = look.inset(&space);
            (self.width, Padding::from([0.0, inset]))
        };
        let button = button::custom(content)
            .class(class(look, self.selected, self.fill))
            .padding(padding)
            .height(Length::Fixed(square))
            .width(width)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .on_press_maybe(self.on_press);
        let button = match self.id {
            Some(id) => button.id(id),
            None => button,
        };
        #[cfg(feature = "a11y")]
        let button = button.name(name);
        (button, self.tip)
    }
}

impl<'a, M: Clone + 'a> From<Button<'a, M>> for Element<'a, M> {
    fn from(button: Button<'a, M>) -> Self {
        let (button, tip) = button.into_native();
        match tip {
            Some((tip, position)) => tooltip_with(button, tip, position),
            None => button.into(),
        }
    }
}

/// A label in `look`'s type: semibold for the boxed looks; bold and
/// underlined for Tertiary (rule 6a's weights)
fn label<'a, M: 'a>(look: ButtonLook, text: String) -> Element<'a, M> {
    if look.tertiary() {
        let span: cosmic::iced::widget::text::Span<'a, (), cosmic::iced::Font> =
            cosmic::iced::widget::text::Span::new(text)
                .underline(true)
                .font(cosmic::font::bold());
        cosmic::iced::widget::rich_text([span])
            .size(LABEL_SIZE)
            .line_height(LineHeight::Absolute(LABEL_LINE.into()))
            .wrapping(Wrapping::None)
            .into()
    } else {
        widget::text(text)
            .size(LABEL_SIZE)
            .line_height(LineHeight::Absolute(LABEL_LINE.into()))
            .font(cosmic::font::semibold())
            .wrapping(Wrapping::None)
            .into()
    }
}

/// The first of `labels`, laid out in the room of the largest of them
/// all and centred in it: how a button is as wide in every state. Only
/// the first is drawn or seen by operations; the others are measured
struct Widest<'a, M> {
    labels: Vec<Element<'a, M>>,
}

impl<M> Widget<M, Theme, Renderer> for Widest<'_, M> {
    fn children(&self) -> Vec<Tree> {
        self.labels.iter().map(|label| Tree::new(label)).collect()
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(self.labels.as_mut_slice());
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
        let mut nodes = self
            .labels
            .iter_mut()
            .zip(tree.children.iter_mut())
            .map(|(label, tree)| label.as_widget_mut().layout(tree, renderer, limits));
        let Some(shown) = nodes.next() else {
            return layout::Node::new(Size::ZERO);
        };
        let size = nodes.fold(shown.size(), |most, node| {
            Size::new(
                most.width.max(node.size().width),
                most.height.max(node.size().height),
            )
        });
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
        if let (Some(label), Some(state), Some(layout)) = (
            self.labels.first(),
            tree.children.first(),
            layout.children().next(),
        ) {
            label
                .as_widget()
                .draw(state, renderer, theme, style, layout, cursor, viewport);
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
