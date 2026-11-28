// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::advanced::widget::Tree;
use cosmic::iced::advanced::{Widget, layout as native_layout};
use cosmic::iced::{Font, Length, Pixels, Size};
use cosmic::{Element, Renderer};

fn renderer() -> Renderer {
    cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
        Font::default(),
        Pixels(14.0),
        Some("tiny-skia"),
    ))
    .expect("headless tiny-skia renderer")
}

fn path_input(id: widget::Id) -> Child<'static, String> {
    layout::child(move |_| {
        widget::text_input("Path", "/tmp/project")
            .id(id.clone())
            .on_input(|path| path)
            .width(Length::Fill)
            .into()
    })
    .minimum_width(|_, _| 150.0)
    .preferred_width(240.0)
}

#[test]
fn fields_align_controls_and_keep_support_with_its_field_when_stacked() {
    let renderer = renderer();
    let help = layout::child(|_| {
        widget::text("The directory must exist on the chosen machine.")
            .wrapping(Wrapping::WordOrGlyph)
            .into()
    })
    .minimum_width(|_, _| 36.0);
    let mut form = fields(
        vec![
            Field::new("From", path_input(widget::Id::unique())).supporting(help),
            Field::new("Destination", path_input(widget::Id::unique())),
        ],
        112.0,
    );
    let mut tree = Tree::empty();
    for (width, stage) in [(520.0, 0), (230.0, 1), (520.0, 0)] {
        let node = form.layout(
            &mut tree,
            &renderer,
            &native_layout::Limits::new(Size::ZERO, Size::new(width, 400.0)),
        );
        let plan = form.selection().unwrap().as_ref().unwrap();
        assert_eq!(plan.stage_index, stage);
        assert!(node.size().height > 0.0 && node.size().width <= width);
        let first = &plan.geometry.children[0].geometry.children;
        let second = &plan.geometry.children[1].geometry.children;
        assert_eq!(first[1].position.x, second[1].position.x);
        if stage == 0 {
            assert!(first[1].position.x > 112.0);
            assert_eq!(first[1].position.y, 0.0);
        } else {
            assert_eq!(first[1].position.x, 0.0);
            assert!(first[1].position.y >= first[0].geometry.size.height);
        }
        // The fill-width layer reserves the control's column. Its sole
        // child has the input and supporting text in that order.
        let content = &first[1].geometry.children[0].geometry.children;
        assert_eq!(content.len(), 2);
        assert_eq!(content[0].position.x, content[1].position.x);
        assert!(content[1].position.y >= content[0].geometry.size.height);
    }
}

#[test]
fn the_stacked_form_can_request_a_mild_control() {
    let renderer = renderer();
    let control = layout::child(|level| -> Element<'_, ()> {
        let width = if level == Level::Preferred { 300 } else { 90 };
        widget::Space::new().width(width).height(32).into()
    });
    let mut form = fields(vec![Field::new("Where", control)], 100.0);
    let node = form.layout(
        &mut Tree::empty(),
        &renderer,
        &native_layout::Limits::new(Size::ZERO, Size::new(120.0, 160.0)),
    );
    assert_eq!(
        form.selection().unwrap().as_ref().unwrap().level,
        Level::Mild
    );
    assert!(node.size().height > 32.0);
}
