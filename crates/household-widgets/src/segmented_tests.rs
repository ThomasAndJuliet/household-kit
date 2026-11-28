// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use cosmic::Renderer;
use cosmic::iced::advanced::clipboard;
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::advanced::widget::Tree;
use cosmic::iced::advanced::{Layout, Shell, Widget, layout as native_layout, mouse};
use cosmic::iced::{Event, Font, Pixels, Point, Rectangle, Size};

const SIZE: Size = Size::new(600.0, 240.0);

fn renderer() -> Renderer {
    cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
        Font::default(),
        Pixels(14.0),
        Some("tiny-skia"),
    ))
    .expect("headless tiny-skia renderer")
}

#[test]
fn both_arrangements_route_clicks_and_keep_disabled_choices_visible() {
    let renderer = renderer();
    let choices = choices(
        vec![
            Choice::new("This machine", 1).id(widget::Id::unique()),
            Choice::new("Another available machine", 2).id(widget::Id::unique()),
            Choice::new("Offline", None).tooltip("This machine is offline"),
        ],
        Some(0),
        14.0,
    );
    let mut root = layout::responsive_child(Level::Mild, choices);
    let mut tree = Tree::empty();

    for (width, level) in [(SIZE.width, Level::Preferred), (145.0, Level::Mild)] {
        let node = root.layout(
            &mut tree,
            &renderer,
            &native_layout::Limits::new(Size::ZERO, Size::new(width, SIZE.height)),
        );
        let plan = root.selection().unwrap().as_ref().unwrap();
        assert_eq!(plan.level, level);
        assert!(plan.geometry.size.width <= width);
        let parts = &plan.geometry.children;
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].position, Point::ORIGIN);
        if level == Level::Preferred {
            assert!(parts[1].position.x > 0.0);
            assert_eq!(parts[1].position.y, 0.0);
        } else {
            assert_eq!(parts[1].position.x, 0.0);
            assert_eq!(parts[1].position.y, parts[0].geometry.size.height);
            assert!(parts[1].geometry.size.height > parts[0].geometry.size.height);
            assert_eq!(parts[1].geometry.size.width, parts[0].geometry.size.width);
        }

        let centers: Vec<_> = parts
            .iter()
            .map(|part| {
                Point::new(
                    part.position.x + part.geometry.size.width / 2.0,
                    part.position.y + part.geometry.size.height / 2.0,
                )
            })
            .collect();
        for (point, expected) in centers.into_iter().zip([Some(1), Some(2), None]) {
            let mut messages = Vec::new();
            for event in [
                mouse::Event::ButtonPressed(mouse::Button::Left),
                mouse::Event::ButtonReleased(mouse::Button::Left),
            ] {
                let mut shell = Shell::new(&mut messages);
                root.update(
                    &mut tree,
                    &Event::Mouse(event),
                    Layout::new(&node),
                    mouse::Cursor::Available(point),
                    &renderer,
                    &mut clipboard::Null,
                    &mut shell,
                    &Rectangle::with_size(SIZE),
                );
            }
            assert_eq!(messages, expected.into_iter().collect::<Vec<_>>());
        }
    }
}

#[test]
fn selection_uses_muted_accent_and_only_outer_corners_are_rounded() {
    use cosmic::widget::button::Catalog;

    let theme = theme::active();
    let ends = Ends {
        first: true,
        last: false,
    };
    let [top_left, top_right, _, bottom_left] = theme.cosmic().corner_radii.radius_xl;
    let horizontal = theme.active(false, false, &segment_style(ends, false, true));
    let vertical = theme.active(false, false, &segment_style(ends, true, true));
    let (fill, ink) = muted_colours(&theme);

    assert_eq!(horizontal.background, Some(Background::Color(fill)));
    assert_eq!(horizontal.text_color, Some(ink));
    assert_eq!(
        horizontal.border_radius,
        [top_left, 0.0, 0.0, bottom_left].into()
    );
    assert_eq!(
        vertical.border_radius,
        [top_left, top_right, 0.0, 0.0].into()
    );
}
