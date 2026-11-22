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
//! The trigger is the shared [`Button`](crate::button::Button) in its
//! [`Plain`](crate::button::ButtonLook::Plain) look (Juliet, 22 Nov 2026):
//! at rest the label and the trailing caret and nothing else (no fill, no
//! border), under the pointer, pressed, focused and disabled the Plain look's
//! states, and the one button's box: `space_l` tall, `space_s` at the sides,
//! the caret a trailing 16 px icon.

use crate::button::{Button, ButtonLook, label_width_in};
use crate::menu::{Menu, MenuItem, MenuMark};
use crate::window::{Declaration, Declared, Toggle, menu_button};
use cosmic::Element;
use cosmic::cosmic_theme::Spacing;
use cosmic::iced::Length;
use cosmic::widget::{self, icon};
use std::marker::PhantomData;

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
/// caret, and the button's own room ([`label_width_in`])
pub fn trigger_width(labels: &[DropdownOption]) -> f32 {
    trigger_width_in(labels, &cosmic::theme::spacing())
}

/// [`trigger_width`] at the given spacing steps
pub fn trigger_width_in(labels: &[DropdownOption], spacing: &Spacing) -> f32 {
    let labels: Vec<&str> = labels.iter().map(|option| option.label.as_str()).collect();
    label_width_in(ButtonLook::Plain, &labels, None, true, spacing)
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

/// The trigger: the selected value and a caret, a Plain
/// [`Button`](crate::button::Button) publishing a [`Toggle`], which is also
/// the focus target Enter and Space open from (it has the trigger's id)
pub(crate) fn trigger<'a>(
    shown: String,
    width: Length,
    id: Option<widget::Id>,
) -> Element<'a, Toggle> {
    let caret = icon::from_name("pan-down-symbolic").handle();
    Button::new(ButtonLook::Plain, shown)
        .trailing_icon(caret)
        .width(width)
        .id(id.unwrap_or_else(widget::Id::unique))
        .on_press(Toggle)
        .into()
}

#[cfg(test)]
#[path = "dropdown_tests.rs"]
mod tests;
