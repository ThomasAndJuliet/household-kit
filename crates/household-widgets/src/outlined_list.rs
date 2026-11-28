// SPDX-License-Identifier: GPL-3.0-only

//! An outlined list in a form. The caller supplies its rows and empty content.
//! An invalid list follows the form-field error treatment in household rule 24b.

use cosmic::iced::{Background, Color, Length};
use cosmic::{Element, theme, widget};

use crate::look::{Look, State};

/// A list with the normal field outline or, when invalid, an error outline and tint.
pub fn outlined_list<'a, Message: 'a>(
    mut rows: Vec<Element<'a, Message>>,
    empty: Element<'a, Message>,
    invalid: bool,
) -> Element<'a, Message> {
    let spacing = theme::active().cosmic().spacing;
    if rows.is_empty() {
        rows.push(empty);
    }

    widget::container(widget::column::with_children(rows).spacing(spacing.space_xxxs))
        .padding(spacing.space_xxxs)
        .width(Length::Fill)
        .class(theme::Container::custom(move |theme| {
            let mut border = Look::normal().border(theme, State::Rest);
            let background = invalid.then(|| {
                let error: Color = theme.cosmic().destructive_text_color().into();
                border.color = error;
                Background::Color(Color { a: 0.08, ..error })
            });
            cosmic::iced::widget::container::Style {
                border,
                background,
                ..Default::default()
            }
        }))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::iced::advanced::renderer::{Headless, Style};
    use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
    use cosmic::iced::{Font, Pixels, Size, mouse};
    use cosmic::{Renderer, Theme};

    #[test]
    fn validation_changes_the_outline_and_tints_the_list() {
        let mut renderer = cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
            Font::default(),
            Pixels(14.0),
            Some("tiny-skia"),
        ))
        .expect("headless renderer");
        let theme = Theme::dark();
        let mut render = |invalid| {
            let empty = widget::Space::new().height(24.0).into();
            let list = outlined_list::<()>(Vec::new(), empty, invalid);
            let mut ui = UserInterface::build(
                list,
                Size::new(160.0, 48.0),
                Cache::default(),
                &mut renderer,
            );
            ui.draw(
                &mut renderer,
                &theme,
                &Style::default(),
                mouse::Cursor::Unavailable,
            );
            renderer.screenshot(Size::new(160, 48), 1.0, Color::BLACK)
        };

        let normal = render(false);
        let invalid = render(true);
        let outline = 4 * 80;
        let centre = 4 * (160 * 14 + 80);
        assert_ne!(
            &normal[outline..outline + 3],
            &invalid[outline..outline + 3]
        );
        assert_eq!(&normal[centre..centre + 3], &[0, 0, 0]);
        assert_ne!(&invalid[centre..centre + 3], &[0, 0, 0]);
    }
}
