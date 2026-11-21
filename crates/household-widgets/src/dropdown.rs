// SPDX-License-Identifier: GPL-3.0-only

//! A dropdown: a trigger showing the selected value, whose list is the
//! window's one menu chain rather than an overlay of its own. Opening it
//! replaces any other open menu, the list's rows can show information (a
//! disabled option says why), and the keyboard moves through it as through
//! any menu: Enter or Space on the trigger opens it with the selected option the
//! keyboard's target, Up and Down move, Enter chooses, Escape closes.
//!
//! The trigger is a [`menu_button`](crate::window::menu_button); the app's
//! window adapter builds the list's panels from the [`DropdownMenu`] it
//! declares, as whole rows with the selected value in a grey fill and
//! semibold text ([`MenuSurface::choice_rows`](crate::menu_surface::MenuSurface::choice_rows)),
//! as wide as the trigger ([`DropdownMenu::width`]), the keyboard's target
//! the selected option ([`DropdownMenu::selected`]).
//!
//! The trigger ([`Face`]) is libcosmic's dropdown as it draws today: the
//! theme's pick-list style at rest and hovered, the [`Field`](Kind::Field)
//! kind's padding (`space_xxs`), the label at 14 px in 1.2 line height, the
//! 16 px `pan-down-symbolic` caret one of its gaps (`space_xxxs`) from the
//! right edge.

use crate::menu::{Menu, MenuItem, MenuMark, measured_width};
use crate::spacing::Kind;
use crate::window::{Declaration, Declared, Toggle, menu_button};
use cosmic::Element;
use cosmic::cosmic_theme::Spacing;
use cosmic::iced::advanced::widget::Tree;
use cosmic::iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, renderer, svg, text,
};
use cosmic::iced::widget::pick_list::{Catalog, Status};
use cosmic::iced::{Event, Pixels, Rectangle, Size, alignment};
use cosmic::iced::{Length, Padding};
use cosmic::widget::{self, icon};
use cosmic::{Renderer, Theme};
use std::marker::PhantomData;

const TEXT_SIZE: f32 = 14.0;
const CARET: f32 = 16.0;

/// What a dropdown declares to its window: the list as a menu, and the
/// width of its trigger, which the list is as wide as.
#[derive(Clone)]
pub struct DropdownMenu<M> {
    pub menu: Menu<M>,
    pub width: f32,
    /// The selected option, which the keyboard's target starts on
    pub selected: Option<usize>,
}

/// One option of a dropdown
#[derive(Clone, Debug, PartialEq)]
pub struct DropdownOption {
    pub label: String,
    /// Why it cannot be chosen; the list shows it as the row's information
    pub disabled_reason: Option<String>,
}

/// The width a trigger needs to show any of `labels` whole: the widest, the
/// caret, and the padding
pub fn trigger_width(labels: &[DropdownOption]) -> f32 {
    trigger_width_in(labels, &cosmic::theme::spacing())
}

/// [`trigger_width`] at the given spacing steps
pub fn trigger_width_in(labels: &[DropdownOption], spacing: &Spacing) -> f32 {
    let (padding, gap) = face_space(spacing);
    let font = cosmic::font::default();
    let text = labels
        .iter()
        .map(|option| measured_width(&option.label, TEXT_SIZE, font))
        .fold(0.0, f32::max);
    (text + gap + CARET + 2.0 * padding).ceil()
}

/// The list as a menu: every option a row, the selected one marked, a
/// disabled one with its reason, and choosing option `i` giving `on(i)`
pub fn dropdown_menu<M>(
    options: &[DropdownOption],
    selected: Option<usize>,
    on: impl Fn(usize) -> M,
) -> DropdownMenu<M> {
    let mut menu = Menu::new();
    for (i, option) in options.iter().enumerate() {
        menu = menu.item(MenuItem {
            label: option.label.clone(),
            message: on(i),
            icon: None,
            shortcut: None,
            mark: Some(MenuMark::Radio(selected == Some(i))),
            disabled_reason: option.disabled_reason.clone(),
            destructive: false,
            detail: None,
        });
    }
    DropdownMenu {
        menu,
        width: trigger_width(options),
        selected,
    }
}

/// A dropdown being made; turn it into an element with `.into()`. `R` is the
/// app's root type, and `T` its declaration of a dropdown's list.
pub struct Dropdown<'a, M, R, T> {
    options: Vec<DropdownOption>,
    selected: Option<usize>,
    choose: Box<dyn Fn(usize) -> M + 'a>,
    width: Option<Length>,
    name: Option<String>,
    _kinds: PhantomData<fn() -> (R, T)>,
}

/// A dropdown of `labels` showing `selected`, choosing option `i` giving `on(i)`
pub fn dropdown<'a, M, R, T>(
    labels: impl IntoIterator<Item = impl AsRef<str>>,
    selected: Option<usize>,
    on: impl Fn(usize) -> M + 'a,
) -> Dropdown<'a, M, R, T> {
    Dropdown {
        options: labels
            .into_iter()
            .map(|label| DropdownOption {
                label: label.as_ref().to_owned(),
                disabled_reason: None,
            })
            .collect(),
        selected,
        choose: Box::new(on),
        width: None,
        name: None,
        _kinds: PhantomData,
    }
}

impl<'a, M, R, T> Dropdown<'a, M, R, T> {
    /// The trigger's width, where not its options'
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = Some(width.into());
        self
    }

    /// Option `index` cannot be chosen, for `reason`
    pub fn disabled(mut self, index: usize, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        assert!(!reason.trim().is_empty(), "a disabled option says why");
        if let Some(option) = self.options.get_mut(index) {
            option.disabled_reason = Some(reason);
        }
        self
    }

    /// The dropdown's name, which makes its identity in its window and its
    /// trigger's, for focus ([`trigger_id`]), where its place in the view
    /// is not enough
    pub fn id(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

impl<'a, M, R, T> From<Dropdown<'a, M, R, T>> for Element<'a, M>
where
    M: Clone + 'a,
    R: From<Declared> + Clone + 'static,
    T: Declaration + From<DropdownMenu<M>>,
{
    fn from(dropdown: Dropdown<'a, M, R, T>) -> Self {
        let Dropdown {
            options,
            selected,
            choose,
            width,
            name,
            ..
        } = dropdown;
        let mut declared = dropdown_menu(&options, selected, choose);
        // A fixed trigger gives its width to the list
        if let Some(Length::Fixed(fixed)) = width {
            declared.width = fixed;
        }
        let shown = selected
            .and_then(|i| options.get(i))
            .map(|option| option.label.clone())
            .unwrap_or_default();
        let width = width.unwrap_or(Length::Fixed(declared.width));
        let source = name.clone().map(widget::Id::new);
        let button = name.as_deref().map(trigger_id);
        menu_button::<M, R, T>(source, T::from(declared), trigger(shown, width, button))
    }
}

/// The focus identity of the trigger of the dropdown named `name`
pub fn trigger_id(name: &str) -> widget::Id {
    widget::Id::new(format!("{name}:dropdown-trigger"))
}

/// The trigger: the selected value and a caret, publishing a [`Toggle`], a
/// focus target Enter and Space open from
fn trigger<'a>(shown: String, width: Length, id: Option<widget::Id>) -> Element<'a, Toggle> {
    let face = Element::new(Face::new(shown, width));
    let id = id.unwrap_or_else(widget::Id::unique);
    crate::popup::focus_target(id, face)
        .on_activate(Toggle)
        .into()
}

/// libcosmic's dropdown trigger, drawn as it draws it
pub struct Face {
    label: String,
    width: Length,
    /// The steps it was built with: its padding and gap
    spacing: Spacing,
    caret: Option<svg::Handle>,
}

impl Face {
    fn new(label: String, width: Length) -> Self {
        Self::in_spacing(label, width, cosmic::theme::spacing())
    }

    fn in_spacing(label: String, width: Length, spacing: Spacing) -> Self {
        let caret = match icon::from_name("pan-down-symbolic").size(16).handle().data {
            icon::Data::Svg(handle) => Some(handle),
            icon::Data::Image(_) => None,
        };
        Self {
            label,
            width,
            spacing,
            caret,
        }
    }
}

/// The face's padding at every side and the gap its caret sits in
fn face_space(spacing: &Spacing) -> (f32, f32) {
    (Kind::Field.inset(spacing).left, Kind::Field.gap(spacing))
}

const LINE: text::LineHeight = text::LineHeight::Relative(1.2);

impl Widget<Toggle, Theme, Renderer> for Face {
    fn size(&self) -> Size<Length> {
        Size::new(self.width, Length::Shrink)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let (inset, gap) = face_space(&self.spacing);
        let padding = Padding::new(inset);
        let limits = limits
            .width(self.width)
            .height(Length::Shrink)
            .shrink(padding);
        let text = match self.width {
            Length::Shrink => {
                measured_width(&self.label, TEXT_SIZE, cosmic::font::default()).round()
            }
            _ => 0.0,
        };
        let intrinsic = Size::new(
            text + gap + CARET,
            f32::from(LINE.to_absolute(Pixels(TEXT_SIZE))),
        );
        layout::Node::new(
            limits
                .resolve(self.width, Length::Shrink, intrinsic)
                .expand(padding),
        )
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Toggle>,
        _viewport: &Rectangle,
    ) {
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) && cursor.is_over(layout.bounds())
        {
            shell.publish(Toggle);
            shell.capture_event();
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use cosmic::iced::advanced::Renderer as _;
        use cosmic::iced::advanced::svg::Renderer as _;
        use cosmic::iced::advanced::text::Renderer as _;
        let bounds = layout.bounds();
        let (inset, gap) = face_space(&self.spacing);
        let status = if cursor.is_over(bounds) {
            Status::Hovered
        } else {
            Status::Active
        };
        let style = Catalog::style(theme, &(), status);
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: style.border,
                shadow: cosmic::iced::Shadow::default(),
                snap: true,
            },
            style.background,
        );
        if let Some(handle) = self.caret.clone() {
            let at = Rectangle {
                x: bounds.x + bounds.width - gap - CARET,
                y: bounds.center_y() - CARET / 2.0,
                width: CARET,
                height: CARET,
            };
            renderer.draw_svg(svg::Svg::new(handle).color(style.text_color), at, at);
        }
        let area = Rectangle {
            x: bounds.x + inset,
            y: bounds.center_y(),
            width: bounds.width - 2.0 * inset,
            height: f32::from(LINE.to_absolute(Pixels(TEXT_SIZE))),
        };
        renderer.fill_text(
            text::Text {
                content: self.label.clone(),
                size: Pixels(TEXT_SIZE),
                line_height: LINE,
                font: cosmic::font::default(),
                bounds: area.size(),
                align_x: text::Alignment::Left,
                align_y: alignment::Vertical::Center,
                shaping: text::Shaping::Advanced,
                wrapping: text::Wrapping::default(),
                ellipsize: text::Ellipsize::default(),
            },
            area.position(),
            style.text_color,
            *viewport,
        );
    }
}

#[cfg(test)]
#[path = "dropdown_tests.rs"]
mod tests;
