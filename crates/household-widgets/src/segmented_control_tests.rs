// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::runtime::{UserInterface, user_interface::Cache};
use cosmic::iced::{Font, Pixels, keyboard, window};
use std::time::Instant;

fn renderer() -> Renderer {
    cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
        Font::default(),
        Pixels(TEXT_SIZE),
        Some("tiny-skia"),
    ))
    .expect("native renderer")
}

fn choices(middle: Option<usize>) -> Vec<Choice<usize>> {
    [Some(1), middle, Some(3)]
        .into_iter()
        .enumerate()
        .map(|(index, message)| Choice {
            label: format!("Choice {}", index + 1),
            icon: None,
            message,
            tip: (index == 1).then(|| "Second choice explanation".into()),
        })
        .collect()
}

fn lay_out(
    element: &mut Element<'_, usize>,
    tree: &mut Tree,
    renderer: &Renderer,
    width: f32,
) -> layout::Node {
    tree.diff(element.as_widget_mut());
    element.as_widget_mut().layout(
        tree,
        renderer,
        &layout::Limits::new(Size::ZERO, Size::new(width, 100.0)),
    )
}

#[test]
fn segment_bounds_fill_the_bar_and_scale_down_with_it() {
    let renderer = renderer();
    let mut tree = Tree::empty();
    let mut control = view(choices(None), Some(0), Some(70.0), true);
    for (available, expected) in [(400.0, 210.0), (111.0, 111.0), (0.0, 0.0)] {
        let node = lay_out(&mut control, &mut tree, &renderer, available);
        assert_eq!(node.size(), Size::new(expected, 32.0));
        let regions: Vec<_> = Layout::new(&node).children().skip(1).collect();
        assert_eq!(regions.len(), 3);
        for (index, region) in regions.iter().enumerate() {
            let bounds = region.bounds();
            assert_eq!(bounds.x, index as f32 * expected / 3.0);
            assert_eq!(bounds.width, expected / 3.0);
        }
    }
    for count in [0, 1] {
        let mut control = view(
            choices(None).into_iter().take(count).collect(),
            Some(5),
            Some(70.0),
            false,
        );
        let node = lay_out(&mut control, &mut tree, &renderer, 400.0);
        assert_eq!(node.size(), Size::new(count as f32 * 70.0, 32.0));
    }
}

#[test]
fn disabled_choice_keeps_its_own_hover_region_at_narrow_width() {
    let mut renderer = renderer();
    let owner = crate::window::WindowUi::<()>::new();
    let control = view(choices(None), Some(0), Some(70.0), true);
    let root: Element<'_, _> = owner.mount(control, Vec::new()).into();
    let mut ui = UserInterface::build(
        root,
        Size::new(111.0, 300.0),
        Cache::default(),
        &mut renderer,
    );
    let position = Point::new(55.0, 16.0);
    let cursor = mouse::Cursor::Available(position);
    let mut messages = Vec::new();
    let start = Instant::now();
    for event in [
        Event::Mouse(mouse::Event::CursorMoved { position }),
        Event::Window(window::Event::RedrawRequested(
            start + crate::popup::INFORMATION_DELAY * 2,
        )),
    ] {
        ui.update(
            &[event],
            cursor,
            &mut renderer,
            &mut cosmic::iced::advanced::clipboard::Null,
            &mut messages,
        );
    }
    let anchors = owner.anchors();
    let source = anchors.hovered().expect("disabled segment hover source");
    assert!(anchors.information_visible(&source));
    assert_eq!(
        anchors.visible(&source),
        Some(Rectangle::new(Point::new(37.0, 0.0), Size::new(37.0, 32.0)))
    );
}

#[test]
fn native_pointer_and_keyboard_use_current_availability() {
    let renderer = renderer();
    let mut tree = Tree::empty();
    let mut control = view(choices(Some(2)), Some(0), Some(70.0), false);
    let mut node = lay_out(&mut control, &mut tree, &renderer, 400.0);
    let at = |x| mouse::Cursor::Available(Point::new(x, 16.0));
    let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    let key = keyboard::Key::Named(keyboard::key::Named::Enter);
    let enter = Event::Keyboard(keyboard::Event::KeyReleased {
        key: key.clone(),
        modified_key: key,
        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Enter),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::empty(),
    });
    let mut messages = Vec::new();
    let mut send = |control: &mut Element<'_, usize>,
                    tree: &mut Tree,
                    node: &layout::Node,
                    event: &Event,
                    cursor: mouse::Cursor| {
        control.as_widget_mut().update(
            tree,
            event,
            Layout::new(node),
            cursor,
            &renderer,
            &mut cosmic::iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(Size::new(400.0, 100.0)),
        );
    };
    send(&mut control, &mut tree, &node, &press, at(105.0));
    send(&mut control, &mut tree, &node, &release, at(105.0));

    // Selection changes rebuild the view while native keyboard focus stays.
    control = view(choices(Some(20)), Some(1), Some(70.0), false);
    node = lay_out(&mut control, &mut tree, &renderer, 400.0);
    send(
        &mut control,
        &mut tree,
        &node,
        &enter,
        mouse::Cursor::Unavailable,
    );

    // That same focused entity can be disabled in a later view.
    control = view(choices(None), Some(1), Some(70.0), false);
    node = lay_out(&mut control, &mut tree, &renderer, 400.0);
    send(
        &mut control,
        &mut tree,
        &node,
        &enter,
        mouse::Cursor::Unavailable,
    );
    send(&mut control, &mut tree, &node, &press, at(105.0));
    send(&mut control, &mut tree, &node, &release, at(105.0));
    assert_eq!(messages, [2, 20]);
}
