// SPDX-License-Identifier: GPL-3.0-only

//! Shared panel header, composed from ordinary measured children.

use crate::layout::{Child, framed_child, row_child_top};
use crate::spacing::Kind;
use cosmic::{theme, widget};

/// The square panel surface shared with the application's top panel.
pub fn style() -> theme::Container<'static> {
    theme::Container::custom(|theme| {
        use cosmic::iced::widget::container::Catalog;
        use household_tones::{Arrangement, Mode, Surface};
        let mode = if theme.cosmic().is_dark {
            Mode::Dark
        } else {
            Mode::Light
        };
        let tone = Arrangement::A.tone(Surface::Card, mode);
        let on = theme.cosmic().background(theme.transparent).on.into();
        let mut style = theme.style(&theme::Container::Card);
        style.background = Some(cosmic::iced::Background::Color(
            cosmic::iced::Color::from_rgb(tone.r, tone.g, tone.b),
        ));
        style.text_color = Some(on);
        style.icon_color = Some(on);
        style.border.radius = 0.0.into();
        style.snap = true;
        style
    })
}

/// Compose navigation/content controls and the fixed trailing menu and toggle.
///
/// The leading child retains its own wrapping and compaction behaviour. Menu
/// and toggle are protected from horizontal shrinking and stay at the top right.
/// Quick actions can be included in `leading` before the menu.
pub fn header<'a, Message: 'a>(
    leading: Child<'a, Message>,
    menu: Child<'a, Message>,
    toggle: Child<'a, Message>,
) -> Child<'a, Message> {
    let spacing = theme::spacing();
    let gap = Kind::Header.gap(&spacing);
    let padding = Kind::Header.inset(&spacing);
    let controls = row_child_top(
        vec![leading.fill_width(), menu.weight(0.0), toggle.weight(0.0)],
        gap,
    )
    .grow(1.0);
    framed_child(controls, padding, move |content| {
        widget::container(content)
            .padding(padding)
            .class(style())
            .into()
    })
    .grow(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Level, SourceId, Stage, child, stages, wrap_child};
    use cosmic::iced::Size;
    use cosmic::iced::advanced::{
        layout,
        widget::{Tree, Widget},
    };
    use cosmic::{Renderer, Theme};

    fn renderer() -> Renderer {
        crate::test_fonts::renderer().expect("headless renderer")
    }

    fn box_child(width: f32, height: f32) -> Child<'static, ()> {
        child(move |_| {
            cosmic::iced::widget::Space::new()
                .width(width)
                .height(height)
                .into()
        })
    }

    #[test]
    fn wrapping_keeps_trailing_controls_at_top_right_and_commits() {
        let renderer = renderer();
        let leading = wrap_child(
            vec![
                box_child(40.0, 20.0),
                box_child(40.0, 20.0),
                box_child(40.0, 20.0),
            ],
            2.0,
        )
        .report_information(SourceId(1), 1, |_, _| vec![]);
        let header = header(leading, box_child(16.0, 16.0), box_child(16.0, 16.0));
        for width in [130.0, 260.0] {
            let offer = Size::new(width, 300.0);
            let plan = header.query_at(Level::Preferred, &renderer, offer).unwrap();
            let frame = &plan.children[0].geometry;
            let nav = &frame.children[0];
            let menu = &frame.children[1];
            let toggle = &frame.children[2];
            assert_eq!(menu.position.y, 0.0);
            assert_eq!(toggle.position.y, 0.0);
            assert_eq!(plan.size.width, width);
            assert_eq!(
                toggle.position.x + toggle.geometry.size.width,
                frame.size.width
            );
            assert!(menu.position.x + menu.geometry.size.width <= toggle.position.x);
            if width == 130.0 {
                assert!(nav.geometry.size.height > 20.0);
            }
            assert_eq!(plan.reports().len(), 1);
            let mut root = stages(vec![Stage::new(Level::Preferred, header.clone())]);
            let node = <_ as Widget<(), Theme, Renderer>>::layout(
                &mut root,
                &mut Tree::empty(),
                &renderer,
                &layout::Limits::new(Size::ZERO, offer),
            );
            assert!(root.selection().unwrap().is_ok());
            assert_eq!(node.children()[0].size(), plan.size);
        }
    }
}
