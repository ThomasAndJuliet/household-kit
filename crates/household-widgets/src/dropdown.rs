// SPDX-License-Identifier: GPL-3.0-only

//! A dropdown: a trigger showing the selected value, whose list is the
//! window's one menu chain rather than an overlay of its own. Opening it
//! replaces any other open menu, the list's rows can show information (a
//! disabled option says why), and the keyboard moves through it as through
//! any menu: Enter or Space on the trigger opens it with the selected option the
//! keyboard's target, Up and Down move, Enter chooses, Escape closes.
//!
//! The trigger is a [`menu_button`](crate::window::menu_button); the app's
//! window adapter builds the list's panels from [`DropdownMenu::rendered`],
//! as whole rows with the selected value in a grey fill and
//! semibold text ([`MenuSurface::dropdown_rows`](crate::menu_surface::MenuSurface::dropdown_rows)),
//! as wide as the trigger ([`DropdownMenu::width`]), the keyboard's target
//! the selected option ([`DropdownMenu::selected`]).
//!
//! The trigger ([`Face`]) is libcosmic's dropdown as it draws today: the
//! theme's pick-list style at rest and hovered, 8 px padding, the label at
//! 14 px by default in 1.2 line height, the 16 px `pan-down-symbolic` caret 4 px from
//! the right edge.

use crate::menu::{ICON_SIZE, Menu, MenuItem, MenuMark, Row, measured_width};
use crate::window::{ChainRequest, Declaration, Declared, Toggle, UiEvent, menu_button};
use cosmic::Element;
use cosmic::iced::advanced::widget::Tree;
use cosmic::iced::advanced::{
    Clipboard, Layout, Shell, Widget, image, layout, mouse, renderer, svg, text,
};
use cosmic::iced::widget::pick_list::{Catalog, Status};
use cosmic::iced::{Event, Length, Pixels, Rectangle, Size, alignment};
use cosmic::widget::{self, icon};
use cosmic::{Renderer, Theme};
use std::marker::PhantomData;

const TEXT_SIZE: f32 = 14.0;
const PADDING: f32 = 8.0;
const GAP: f32 = 4.0;
const CARET: f32 = 16.0;
const REVEAL: usize = 0;

/// What a dropdown declares to its window: the list as a menu, and the
/// width of its trigger, which the list is as wide as.
#[derive(Clone)]
pub struct DropdownMenu<M> {
    /// All choices for declaration tracking. The optional tail is the final
    /// `Row::Folder`, with its label and choices. Use [`Self::rendered`]
    /// for the rows to display.
    pub menu: Menu<M>,
    pub width: f32,
    /// The selected option, which the keyboard's target starts on
    pub selected: Option<usize>,
}

impl<M: Clone> DropdownMenu<M> {
    /// The choices visible in this open session, with the disclosure as a
    /// window action. Activating it replaces the disclosure with its choices
    /// in this same menu.
    pub fn rendered<R: 'static, X>(
        &self,
        request: &ChainRequest<'_, R>,
        lift: impl Fn(M) -> X,
    ) -> Menu<UiEvent<R, X>> {
        let application = |message| UiEvent::Application(lift(message));
        let Some((Row::Folder(label, hidden), visible)) = self.menu.rows().split_last() else {
            return self.menu.clone().map(application);
        };
        let menu = visible.iter().fold(Menu::new(), |menu, row| match row {
            Row::Item(item) => menu.item(MenuItem {
                label: item.label.clone(),
                message: application(item.message.clone()),
                icon: item.icon.clone(),
                shortcut: item.shortcut.clone(),
                mark: item.mark,
                disabled_reason: item.disabled_reason.clone(),
                destructive: item.destructive,
            }),
            Row::Divider => menu.divider(),
            Row::Folder(label, submenu) => {
                menu.folder(label.clone(), submenu.clone().map(application))
            }
        });
        if request.is_expanded(REVEAL) {
            let hidden = hidden.clone().map(application);
            if menu.is_empty() {
                hidden
            } else {
                menu.append(hidden)
            }
        } else {
            let menu = if menu.is_empty() {
                menu
            } else {
                menu.divider()
            };
            menu.item(MenuItem {
                label: label.clone(),
                message: request.expand(REVEAL, true),
                icon: Some(icon::from_name("go-down-symbolic").handle()),
                shortcut: None,
                mark: None,
                disabled_reason: None,
                destructive: false,
            })
        }
    }
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
    choice_width(
        labels.iter().map(|option| (option.label.as_str(), 0.0)),
        TEXT_SIZE,
    )
}

fn choice_width<'a>(choices: impl Iterator<Item = (&'a str, f32)>, text_size: f32) -> f32 {
    let font = cosmic::font::default();
    let text = choices
        .map(|(label, leading_width)| measured_width(label, text_size, font) + leading_width)
        .fold(0.0, f32::max);
    (text + GAP + CARET + 2.0 * PADDING).ceil()
}

fn leading_width() -> f32 {
    ICON_SIZE + icon_gap()
}

pub(crate) fn icon_gap() -> f32 {
    f32::from(cosmic::theme::spacing().space_xxs)
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
        menu = menu.item(choice_item(
            option.clone(),
            None,
            selected == Some(i),
            on(i),
        ));
    }
    DropdownMenu {
        menu,
        width: trigger_width(options),
        selected,
    }
}

fn choice_item<M>(
    option: DropdownOption,
    icon: Option<icon::Handle>,
    selected: bool,
    message: M,
) -> MenuItem<M> {
    MenuItem {
        label: option.label,
        message,
        icon,
        shortcut: None,
        mark: Some(MenuMark::Radio(selected)),
        disabled_reason: option.disabled_reason,
        destructive: false,
    }
}

/// A dropdown being made; turn it into an element with `.into()`. `R` is the
/// app's root type, and `T` its declaration of a dropdown's list.
pub struct Dropdown<'a, M, R, T> {
    choices: Vec<Choice>,
    reveal: Option<(usize, String)>,
    locked: bool,
    selected: Option<usize>,
    choose: Box<dyn Fn(usize) -> M + 'a>,
    text_size: f32,
    width: Option<Length>,
    name: Option<String>,
    _kinds: PhantomData<fn() -> (R, T)>,
}

struct Choice {
    option: DropdownOption,
    icon: Option<icon::Handle>,
}

/// A dropdown of `labels` showing `selected`, choosing option `i` giving `on(i)`
pub fn dropdown<'a, M, R, T>(
    labels: impl IntoIterator<Item = impl AsRef<str>>,
    selected: Option<usize>,
    on: impl Fn(usize) -> M + 'a,
) -> Dropdown<'a, M, R, T> {
    Dropdown {
        choices: labels
            .into_iter()
            .map(|label| Choice {
                option: DropdownOption {
                    label: label.as_ref().to_owned(),
                    disabled_reason: None,
                },
                icon: None,
            })
            .collect(),
        reveal: None,
        locked: false,
        selected,
        choose: Box::new(on),
        text_size: TEXT_SIZE,
        width: None,
        name: None,
        _kinds: PhantomData,
    }
}

impl<'a, M, R, T> Dropdown<'a, M, R, T> {
    /// The trigger width needed for the widest choice, including its icon
    /// and gap. This includes choices behind the disclosure and is independent
    /// of the current selection and any [`Self::width`] override.
    pub fn preferred_width(&self) -> f32 {
        let icon_width = leading_width();
        choice_width(
            self.choices.iter().map(|choice| {
                let leading = if choice.icon.is_some() {
                    icon_width
                } else {
                    0.0
                };
                (choice.option.label.as_str(), leading)
            }),
            self.text_size,
        )
    }

    /// The closed trigger's label size in logical pixels, 14 by default.
    /// The window adapter supplies the open menu's typography.
    pub fn text_size(mut self, size: f32) -> Self {
        self.text_size = size;
        self
    }

    /// The trigger's width, where not its options'
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = Some(width.into());
        self
    }

    /// Option `index` cannot be chosen, for `reason`
    pub fn disabled(mut self, index: usize, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        assert!(!reason.trim().is_empty(), "a disabled option says why");
        if let Some(choice) = self.choices.get_mut(index) {
            choice.option.disabled_reason = Some(reason);
        }
        self
    }

    /// Show an icon with this option, including on the trigger when selected.
    pub fn option_icon(mut self, index: usize, icon: icon::Handle) -> Self {
        if let Some(choice) = self.choices.get_mut(index) {
            choice.icon = Some(icon);
        }
        self
    }

    /// Initially hide the choices after `visible_count` under a disclosure
    /// row. Place the selected choice in the visible group to focus it when
    /// the dropdown opens.
    pub fn reveal_after(mut self, visible_count: usize, label: impl Into<String>) -> Self {
        self.reveal = Some((visible_count, label.into()));
        self
    }

    /// Keep the current choice visible while preventing the list from opening.
    pub fn locked(mut self, locked: bool) -> Self {
        self.locked = locked;
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
        let preferred = match dropdown.width {
            Some(Length::Fixed(fixed)) => fixed,
            _ => dropdown.preferred_width(),
        };
        let Dropdown {
            choices,
            reveal,
            locked,
            selected,
            choose,
            text_size,
            width,
            name,
            ..
        } = dropdown;
        let shown = selected.and_then(|i| choices.get(i));
        let label = shown
            .map(|choice| choice.option.label.clone())
            .unwrap_or_default();
        let leading = shown.and_then(|choice| choice.icon.clone());
        let face_width = width.unwrap_or(Length::Fixed(preferred));
        if locked {
            return Element::new(Face::new(label, leading, face_width, text_size, None));
        }
        let face = Face::new(label, leading, face_width, text_size, Some(Toggle));

        let reveal = reveal.filter(|(visible_count, _)| *visible_count < choices.len());
        let mut visible = Menu::new();
        let mut hidden = Menu::new();
        for (index, Choice { option, icon }) in choices.into_iter().enumerate() {
            let item = choice_item(option, icon, selected == Some(index), choose(index));
            if reveal.as_ref().is_some_and(|(count, _)| index >= *count) {
                hidden = hidden.item(item);
            } else {
                visible = visible.item(item);
            }
        }
        let menu = match reveal {
            Some((_, label)) => visible.folder(label, hidden),
            None => visible,
        };
        let declared = DropdownMenu {
            menu,
            width: preferred,
            selected,
        };
        let source = name.clone().map(widget::Id::new);
        let button = name.as_deref().map(trigger_id);
        menu_button::<M, R, T>(source, T::from(declared), trigger(face, button))
    }
}

/// The focus identity of the trigger of the dropdown named `name`
pub fn trigger_id(name: &str) -> widget::Id {
    widget::Id::new(format!("{name}:dropdown-trigger"))
}

/// The trigger: the selected value and a caret, publishing a [`Toggle`], a
/// focus target Enter and Space open from
fn trigger<'a>(face: Face, id: Option<widget::Id>) -> Element<'a, Toggle> {
    let id = id.unwrap_or_else(widget::Id::unique);
    crate::popup::focus_target(id, Element::new(face))
        .on_activate(Toggle)
        .into()
}

/// libcosmic's dropdown trigger, drawn as it draws it
pub struct Face<M = Toggle> {
    label: String,
    label_width: f32,
    text_size: f32,
    leading: Option<icon::Handle>,
    leading_width: f32,
    width: Length,
    on_press: Option<M>,
    caret: Option<svg::Handle>,
}

struct FaceParts {
    label: Rectangle,
    leading: Option<Rectangle>,
    caret: Option<Rectangle>,
}

impl<M> Face<M> {
    fn new(
        label: String,
        leading: Option<icon::Handle>,
        width: Length,
        text_size: f32,
        on_press: Option<M>,
    ) -> Self {
        let caret = match icon::from_name("pan-down-symbolic").size(16).handle().data {
            icon::Data::Svg(handle) => Some(handle),
            icon::Data::Image(_) => None,
        };
        Self {
            label_width: measured_width(&label, text_size, cosmic::font::default()),
            label,
            text_size,
            leading_width: if leading.is_some() {
                leading_width()
            } else {
                0.0
            },
            leading,
            width,
            on_press,
            caret,
        }
    }

    fn line_height(&self) -> f32 {
        f32::from(LINE.to_absolute(Pixels(self.text_size)))
    }

    fn parts(&self, bounds: Rectangle) -> FaceParts {
        // Keep up to one em for the label before allocating room for
        // the caret, and up to two ems before adding a leading icon.
        let caret_fits =
            bounds.width >= CARET + self.label_width.min(self.text_size) && bounds.height >= CARET;
        let (caret_width, edge_gap) = if caret_fits { (CARET, GAP) } else { (0.0, 0.0) };
        let padding = 2.0 * PADDING + edge_gap;
        let room = bounds.width - caret_width;
        let leading_fits = self.leading.is_some()
            && bounds.height >= ICON_SIZE
            && room >= self.leading_width + padding + self.label_width.min(2.0 * self.text_size);
        let leading_width = if leading_fits {
            self.leading_width
        } else {
            0.0
        };
        let room = room - leading_width;
        // Give a narrow label extra room by reducing all horizontal insets.
        let inset_scale = ((room - self.label_width) / padding).clamp(0.0, 1.0);
        let start = bounds.x + PADDING * inset_scale;
        FaceParts {
            label: Rectangle {
                x: start + leading_width,
                y: bounds.center_y(),
                width: (room - padding * inset_scale).max(0.0),
                height: self.line_height().min(bounds.height),
            },
            leading: leading_fits.then_some(Rectangle {
                x: start,
                y: bounds.center_y() - ICON_SIZE / 2.0,
                width: ICON_SIZE,
                height: ICON_SIZE,
            }),
            caret: caret_fits.then_some(Rectangle {
                x: bounds.x + bounds.width - GAP * inset_scale - CARET,
                y: bounds.center_y() - CARET / 2.0,
                width: CARET,
                height: CARET,
            }),
        }
    }
}

const LINE: text::LineHeight = text::LineHeight::Relative(1.2);

impl<M: Clone> Widget<M, Theme, Renderer> for Face<M> {
    fn size(&self) -> Size<Length> {
        Size::new(self.width, Length::Shrink)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let text = match self.width {
            Length::Shrink => self.label_width.round(),
            _ => 0.0,
        };
        let intrinsic = Size::new(
            text + self.leading_width + GAP + CARET + 2.0 * PADDING,
            self.line_height() + 2.0 * PADDING,
        );
        layout::Node::new(limits.resolve(self.width, Length::Shrink, intrinsic))
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        if let Some(message) = &self.on_press
            && matches!(
                event,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            )
            && cursor.is_over(layout.bounds())
        {
            shell.publish(message.clone());
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
        if self.on_press.is_some() && cursor.is_over(layout.bounds()) {
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
        use cosmic::iced::advanced::image::Renderer as _;
        use cosmic::iced::advanced::svg::Renderer as _;
        use cosmic::iced::advanced::text::Renderer as _;
        let bounds = layout.bounds();
        let locked = self.on_press.is_none();
        let status = if !locked && cursor.is_over(bounds) {
            Status::Hovered
        } else {
            Status::Active
        };
        let style = Catalog::style(theme, &(), status);
        let color = if locked {
            theme
                .cosmic()
                .background(theme.transparent)
                .component
                .on_disabled
                .into()
        } else {
            style.text_color
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: style.border,
                shadow: cosmic::iced::Shadow::default(),
                snap: true,
            },
            style.background,
        );
        let FaceParts {
            label,
            leading,
            caret,
        } = self.parts(bounds);
        if let (Some(handle), Some(at)) = (&self.caret, caret) {
            let handle = handle.clone();
            renderer.draw_svg(svg::Svg::new(handle).color(color), at, at);
        }
        if let (Some(icon), Some(at)) = (&self.leading, leading) {
            let opacity: f32 = if locked { 0.5 } else { 1.0 };
            match &icon.data {
                icon::Data::Svg(handle) => {
                    let svg = svg::Svg::new(handle.clone());
                    let svg = if icon.symbolic {
                        svg.color(color)
                    } else {
                        svg.opacity(opacity)
                    };
                    renderer.draw_svg(svg, at, at);
                }
                icon::Data::Image(handle) => {
                    renderer.draw_image(image::Image::new(handle.clone()).opacity(opacity), at, at);
                }
            }
        }
        if label.width <= 0.0 || label.height <= 0.0 {
            return;
        }
        let (wrapping, ellipsize) = if self.leading.is_some() || label.width < self.label_width {
            (
                text::Wrapping::None,
                text::Ellipsize::End(text::EllipsizeHeightLimit::Lines(1)),
            )
        } else {
            (text::Wrapping::default(), text::Ellipsize::default())
        };
        renderer.fill_text(
            text::Text {
                content: self.label.clone(),
                size: Pixels(self.text_size),
                line_height: LINE,
                font: cosmic::font::default(),
                bounds: label.size(),
                align_x: text::Alignment::Left,
                align_y: alignment::Vertical::Center,
                shaping: text::Shaping::Advanced,
                wrapping,
                ellipsize,
            },
            label.position(),
            color,
            *viewport,
        );
    }
}

#[cfg(test)]
#[path = "dropdown_tests.rs"]
mod tests;
