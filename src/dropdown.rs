// SPDX-License-Identifier: GPL-3.0-only

//! A dropdown: a trigger showing the selected value, whose list is the
//! window's one menu chain rather than an overlay of its own. Opening it
//! replaces any other open menu, the list's rows can show information (a
//! disabled option says why), and the keyboard moves through it as through
//! any menu: Enter or Space on the trigger opens it with its first row the
//! keyboard's target, Up and Down move, Enter chooses, Escape closes.
//!
//! The trigger is a [`menu_button`](crate::window::menu_button); the app's
//! window adapter builds the list's panels from the [`DropdownMenu`] it
//! declares, as whole rows with the selected value in a grey fill and
//! semibold text ([`MenuSurface::choice_rows`](crate::menu_surface::MenuSurface::choice_rows)),
//! as wide as the trigger ([`DropdownMenu::width`]).

use crate::menu::{Menu, MenuItem, MenuMark, measured_width};
use crate::window::{Declaration, Declared, Toggle, menu_button};
use cosmic::iced::{Alignment, Length, Padding};
use cosmic::widget::{self, button, icon};
use cosmic::{Element, theme};
use std::marker::PhantomData;

const TEXT_SIZE: f32 = 14.0;
const PADDING: f32 = 8.0;
const GAP: f32 = 4.0;
const CARET: f32 = 16.0;

/// What a dropdown declares to its window: the list as a menu, and the
/// width of its trigger, which the list is as wide as.
#[derive(Clone)]
pub struct DropdownMenu<M> {
    pub menu: Menu<M>,
    pub width: f32,
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
    let font = cosmic::font::default();
    let text = labels
        .iter()
        .map(|option| measured_width(&option.label, TEXT_SIZE, font))
        .fold(0.0, f32::max);
    (text + GAP + CARET + 2.0 * PADDING).ceil()
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
        });
    }
    DropdownMenu {
        menu,
        width: trigger_width(options),
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
        let declared = dropdown_menu(&options, selected, choose);
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

/// The trigger: the selected value and a caret, publishing a [`Toggle`]
fn trigger<'a>(shown: String, width: Length, id: Option<widget::Id>) -> Element<'a, Toggle> {
    let content = widget::row::with_children(vec![
        widget::text(shown)
            .size(TEXT_SIZE)
            .wrapping(cosmic::iced::widget::text::Wrapping::None)
            .width(Length::Fill)
            .into(),
        icon::from_name("pan-down-symbolic")
            .size(CARET as u16)
            .icon()
            .into(),
    ])
    .spacing(GAP)
    .align_y(Alignment::Center);
    let trigger = button::custom(content);
    let trigger = match id {
        Some(id) => trigger.id(id),
        None => trigger,
    };
    trigger
        .class(theme::Button::Standard)
        .padding(Padding::new(PADDING))
        .width(width)
        .on_press(Toggle)
        .into()
}

#[cfg(test)]
#[path = "dropdown_tests.rs"]
mod tests;
