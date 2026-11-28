// SPDX-License-Identifier: GPL-3.0-only

//! Joined, single-selection choices for forms and filters.

use cosmic::iced::widget::text::Wrapping;
use cosmic::iced::{Alignment, Background, Border, Color, Length};
use cosmic::widget::{self, button, tooltip::Position};
use cosmic::{Element, Theme, theme};
use household_tones::{Srgb, on_muted};
use std::rc::Rc;

use crate::layout::{self, Child, Level};
use crate::{Tip, muted_colours, tooltip_with};

/// A choice and its action. The caller retains the selected value and rebuilds
/// the control after handling a message.
pub struct Choice<M> {
    label: String,
    action: Option<M>,
    tooltip: Option<String>,
    icon: Option<widget::icon::Icon>,
    id: Option<widget::Id>,
}

impl<M> Choice<M> {
    /// An absent action disables the choice.
    pub fn new(label: impl Into<String>, action: impl Into<Option<M>>) -> Self {
        Self {
            label: label.into(),
            action: action.into(),
            tooltip: None,
            icon: None,
            id: None,
        }
    }

    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    pub fn icon(mut self, icon: widget::icon::Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Give a choice the same identity across view rebuilds or reordering.
    pub fn id(mut self, id: widget::Id) -> Self {
        self.id = Some(id);
        self
    }
}

/// A joined bar of choices. At the preferred level the choices sit side by
/// side. At the mild level they stack, with labels allowed to wrap. Give the
/// parent both levels to try if it should fall back to the stacked form.
pub fn choices<'a, M: Clone + 'a>(
    options: Vec<Choice<M>>,
    selected: Option<usize>,
    font_size: f32,
) -> Child<'a, M> {
    let count = options.len();
    let mut row = Vec::with_capacity(count);
    let mut column = Vec::with_capacity(count);
    for (index, option) in options.into_iter().enumerate() {
        let choice = Rc::new(option);
        let ends = Ends {
            first: index == 0,
            last: index + 1 == count,
        };
        let selected = selected == Some(index);
        let choice_in_row = choice.clone();
        row.push(layout::child(move |_| {
            segment(&choice_in_row, ends, false, selected, font_size)
        }));
        let space = theme::active().cosmic().spacing;
        let chrome = 2.0 * f32::from(space.space_s)
            + choice
                .icon
                .as_ref()
                .map_or(0.0, |_| 16.0 + f32::from(space.space_xxs));
        column.push(
            layout::child(move |_| segment(&choice, ends, true, selected, font_size))
                .minimum_width(move |_, size| size.width.min(chrome + 3.0 * font_size))
                .grow(1.0),
        );
    }
    layout::forms(vec![
        (Level::Preferred, frame(layout::row_child(row, 0.0))),
        (Level::Mild, frame(layout::column_child(column, 0.0))),
    ])
}

fn segment<'a, M: Clone + 'a>(
    choice: &Choice<M>,
    ends: Ends,
    stacked: bool,
    selected: bool,
    font_size: f32,
) -> Element<'a, M> {
    let space = theme::active().cosmic().spacing;
    let face = widget::text(choice.label.clone())
        .size(font_size)
        .font(if selected {
            cosmic::font::semibold()
        } else {
            cosmic::font::default()
        })
        .wrapping(if stacked {
            Wrapping::WordOrGlyph
        } else {
            Wrapping::None
        });
    let content = widget::row::with_children(
        choice
            .icon
            .clone()
            .map(|icon| icon.size(16).into())
            .into_iter()
            .chain(std::iter::once(face.into())),
    )
    .spacing(space.space_xxs)
    .align_y(Alignment::Center);
    let mut button = button::custom(content)
        .class(segment_style(ends, stacked, selected))
        .padding([space.space_xxxs, space.space_s])
        .width(if stacked {
            Length::Fill
        } else {
            Length::Shrink
        })
        .on_press_maybe(choice.action.clone());
    if let Some(id) = &choice.id {
        button = button.id(id.clone());
    }
    #[cfg(feature = "a11y")]
    {
        button = button.name(choice.label.clone());
        if let Some(tooltip) = &choice.tooltip {
            button = button.description(tooltip.clone());
        }
    }
    match choice.tooltip.clone() {
        Some(tooltip) => {
            let key = tooltip.clone();
            let tip = Tip::new(key, move || widget::text(tooltip.clone()).into());
            tooltip_with(button, tip, Position::Bottom)
        }
        None => button.into(),
    }
}

fn frame<'a, M: 'a>(child: Child<'a, M>) -> Child<'a, M> {
    child.map_view(|content| {
        widget::container(content)
            .class(theme::Container::custom(|theme| {
                let cosmic = theme.cosmic();
                cosmic::iced::widget::container::Style {
                    background: Some(Background::Color(ground(theme))),
                    border: Border {
                        radius: cosmic.corner_radii.radius_xl.into(),
                        width: 1.0,
                        color: cosmic.bg_divider().into(),
                    },
                    ..Default::default()
                }
            }))
            .into()
    })
}

fn ground(theme: &Theme) -> Color {
    theme
        .cosmic()
        .background(theme.transparent)
        .component
        .base
        .into()
}

#[derive(Clone, Copy)]
struct Ends {
    first: bool,
    last: bool,
}

impl Ends {
    fn radius(self, stacked: bool, [tl, tr, br, bl]: [f32; 4]) -> [f32; 4] {
        let (top_right, bottom_left) = if stacked {
            (self.first, self.last)
        } else {
            (self.last, self.first)
        };
        [
            if self.first { tl } else { 0.0 },
            if top_right { tr } else { 0.0 },
            if self.last { br } else { 0.0 },
            if bottom_left { bl } else { 0.0 },
        ]
    }
}

#[derive(Clone, Copy)]
enum State {
    Rest,
    Hovered,
    Pressed,
    Disabled,
}

fn segment_style(ends: Ends, stacked: bool, selected: bool) -> theme::Button {
    use cosmic::widget::button::Catalog;
    let style = move |state, focused, theme: &Theme| {
        let icon = theme::Button::Icon;
        let mut style = match state {
            State::Rest => theme.active(focused, false, &icon),
            State::Hovered => theme.hovered(focused, false, &icon),
            State::Pressed => theme.pressed(focused, false, &icon),
            State::Disabled => theme.disabled(&icon),
        };
        style.border_radius = ends
            .radius(stacked, theme.cosmic().corner_radii.radius_xl)
            .into();
        if selected {
            let (mut fill, mut ink) = muted_colours(theme);
            if matches!(state, State::Disabled) {
                fill.a *= 0.4;
                fill = crate::kept::flat(Background::Color(fill), ground(theme));
                let tone = on_muted(Srgb::new(fill.r, fill.g, fill.b));
                ink = Color::from_rgb(tone.r, tone.g, tone.b);
            }
            style.overlay = match state {
                State::Hovered => Some(Background::Color(theme.cosmic().icon_button.hover.into())),
                State::Pressed => {
                    Some(Background::Color(theme.cosmic().icon_button.pressed.into()))
                }
                _ => None,
            };
            style.background = Some(Background::Color(fill));
            style.text_color = Some(ink);
            style.icon_color = Some(ink);
        }
        style
    };
    theme::Button::Custom {
        active: Box::new(move |focused, t| style(State::Rest, focused, t)),
        hovered: Box::new(move |focused, t| style(State::Hovered, focused, t)),
        pressed: Box::new(move |focused, t| style(State::Pressed, focused, t)),
        disabled: Box::new(move |t| style(State::Disabled, false, t)),
    }
}

#[cfg(test)]
#[path = "segmented_tests.rs"]
mod tests;
