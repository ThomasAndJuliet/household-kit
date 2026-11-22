// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::segmented::OUTLINE;
use cosmic::iced::advanced::layout;
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::advanced::widget::Tree;
use cosmic::iced::{Color, Font, Pixels, Point, Size};

fn renderer() -> cosmic::Renderer {
    cosmic::iced::futures::executor::block_on(<cosmic::Renderer as Headless>::new(
        Font::default(),
        Pixels(14.0),
        Some("tiny-skia"),
    ))
    .expect("native renderer")
}

fn split(open: bool) -> Element<'static, u8> {
    SplitButton::new("New terminal session", "Choose a machine", 0u8, widget::Id::new("split"))
        .on_press(1)
        .open(open)
        .into()
}

fn layout_of(mut element: Element<'_, u8>, renderer: &cosmic::Renderer) -> layout::Node {
    let mut tree = Tree::new(element.as_widget());
    element.as_widget_mut().layout(
        &mut tree,
        renderer,
        &layout::Limits::new(Size::ZERO, Size::new(900.0, 200.0)),
    )
}

fn drawn(element: Element<'static, u8>) -> Vec<[u8; 4]> {
    use cosmic::iced::advanced::renderer::Style;
    use cosmic::iced::mouse;
    use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
    let mut renderer = renderer();
    let mut ui = UserInterface::build(element, Size::new(240.0, 48.0), Cache::default(), &mut renderer);
    let cursor = mouse::Cursor::Available(Point::new(239.0, 47.0));
    ui.draw(&mut renderer, &cosmic::Theme::default(), &Style::default(), cursor);
    renderer
        .screenshot(Size::new(240, 48), 1.0, Color::from_rgb(0.5, 0.5, 0.5))
        .as_chunks::<4>()
        .0
        .to_vec()
}

fn pixel(pixels: &[[u8; 4]], x: f32, y: f32) -> [u8; 4] {
    pixels[(y as usize) * 240 + x as usize]
}

/// The pair is as tall as every button, its opener the `space_l` square
/// touching the main half, and as wide as the main button and the square
#[test]
fn the_pair_is_one_button_tall_with_a_square_opener_touching_the_main_half() {
    let renderer = renderer();
    let node = layout_of(split(false), &renderer);
    let space_l = crate::button::button_height();
    let main = &node.children()[0];
    let opener = &node.children()[1];
    assert_eq!(node.size().height, space_l);
    assert_eq!(main.size().height, space_l);
    assert_eq!(opener.size(), Size::new(space_l, space_l));
    assert_eq!(opener.bounds().x, main.bounds().x + main.size().width, "no gap");
    let alone = crate::button::label_width(ButtonLook::Segment, &["New terminal session"], None, false);
    assert_eq!(main.size().width, alone, "the main half is a Segment button's box");
    assert_eq!(node.size().width, alone + space_l);
}

/// One outline round the pair on its own edge, one line between the halves,
/// and nothing outside the box
#[test]
fn the_pair_shares_one_outline_and_a_line_between_the_halves() {
    let renderer = renderer();
    let node = layout_of(split(false), &renderer);
    let opener = node.children()[1].bounds();
    let size = node.size();
    let pixels = drawn(split(false));
    let line = pixel(&pixels, (size.width / 2.0).floor(), 0.0);
    let ground = pixel(&pixels, 239.0, 47.0);
    assert_ne!(line, ground, "the outline is on the pair's own edge");
    assert_eq!(pixel(&pixels, opener.x, 16.0), line, "the line between, in the outline's colour");
    assert_ne!(pixel(&pixels, opener.x + 4.0, 16.0), line, "and only one pixel wide");
    assert_ne!(pixel(&pixels, opener.x - OUTLINE - 3.0, 16.0), line);
    assert_eq!(pixel(&pixels, (size.width / 2.0).floor(), size.height), ground, "nothing below");
    assert_eq!(pixel(&pixels, size.width, 16.0), ground, "nor beside");
}

/// While its menu is open the opener is chosen (the muted accent), the main
/// half not
#[test]
fn the_opener_is_chosen_while_its_menu_is_open() {
    let renderer = renderer();
    let node = layout_of(split(true), &renderer);
    let opener = node.children()[1].bounds();
    let main = node.children()[0].bounds();
    let (muted, _) = crate::muted_colours(&cosmic::Theme::default());
    let near = |p: [u8; 4]| {
        [(p[0], muted.r), (p[1], muted.g), (p[2], muted.b)]
            .iter()
            .all(|(a, b)| (f32::from(*a) - b * 255.0).abs() <= 3.0)
    };
    let closed = drawn(split(false));
    let open = drawn(split(true));
    assert!(near(pixel(&open, opener.x + 3.0, 16.0)));
    assert!(!near(pixel(&closed, opener.x + 3.0, 16.0)));
    assert!(!near(pixel(&open, main.x + 3.0, 16.0)));
}
