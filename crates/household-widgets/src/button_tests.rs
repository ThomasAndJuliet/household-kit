// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use cosmic::iced::advanced::layout;
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::advanced::widget::Tree;
use cosmic::iced::{Font, Pixels, Point, Size};

fn renderer() -> cosmic::Renderer {
    cosmic::iced::futures::executor::block_on(<cosmic::Renderer as Headless>::new(
        Font::default(),
        Pixels(14.0),
        Some("tiny-skia"),
    ))
    .expect("native renderer")
}

fn layout_of(mut element: Element<'_, ()>, renderer: &cosmic::Renderer) -> layout::Node {
    let mut tree = Tree::new(element.as_widget());
    element.as_widget_mut().layout(
        &mut tree,
        renderer,
        &layout::Limits::new(Size::ZERO, Size::new(500.0, 100.0)),
    )
}

fn tip() -> Tip<'static, ()> {
    Tip::new("tip", || widget::text("A tip").into())
}

fn mark() -> Element<'static, ()> {
    widget::container(widget::Space::new().width(12).height(12))
        .style(|_| cosmic::widget::container::Style {
            background: Some(cosmic::iced::Color::from_rgb(1.0, 0.0, 0.0).into()),
            ..Default::default()
        })
        .into()
}

fn sized_mark(size: f32) -> Element<'static, ()> {
    widget::Space::new().width(size).height(size).into()
}

/// The space left and right of a labelled button's content
fn sides(node: &layout::Node) -> (f32, f32) {
    let content = &node.children()[0];
    let left = content.bounds().x;
    (left, node.size().width - (left + content.size().width))
}

#[test]
fn each_look_at_standard_density_is_as_tall_and_inset_as_before() {
    let renderer = renderer();
    let standard = cosmic::cosmic_theme::Spacing::default();
    assert_eq!(
        standard.space_l, 32,
        "the sizes below are standard density's"
    );
    for (look, inset) in [
        (ButtonLook::Primary, 16.0),
        (ButtonLook::Secondary, 16.0),
        (ButtonLook::Destructive, 16.0),
        (ButtonLook::Tertiary, 8.0),
        (ButtonLook::TertiaryDestructive, 8.0),
    ] {
        let node = layout_of(Button::new(look, "Open").on_press(()).into(), &renderer);
        assert_eq!(node.size().height, 32.0, "{look:?}");
        let (left, right) = sides(&node);
        assert_eq!(left, inset, "{look:?} left");
        assert!((right - inset).abs() < 0.01, "{look:?} right: {right}");
        let label = &node.children()[0].children()[0];
        let top = node.children()[0].bounds().y + label.bounds().y;
        assert!(
            (top - (32.0 - label.size().height) / 2.0).abs() < 0.5,
            "{look:?}: the label is centred vertically"
        );
    }
}

#[test]
fn the_icon_look_is_the_square_with_a_sixteen_pixel_icon() {
    let renderer = renderer();
    let icon = widget::icon::from_name("edit-copy-symbolic").icon();
    let (button, tip) = Button::icon(icon, "Copy", tip()).on_press(()).into_native();
    assert!(tip.is_some(), "an icon button carries its tooltip");
    let node = layout_of(button.into(), &renderer);
    assert_eq!(node.size(), Size::new(32.0, 32.0));
    let icon = &node.children()[0];
    assert_eq!(icon.size(), Size::new(16.0, 16.0));
    assert_eq!(icon.bounds().x, 8.0, "centred");
    assert_eq!(icon.bounds().y, 8.0, "centred");
}

#[test]
fn the_compact_form_draws_a_twenty_pixel_mark_whole_and_centred() {
    let renderer = renderer();
    let build = |size: f32| -> Element<'static, ()> {
        Button::tertiary("Effort")
            .leading(sized_mark(size))
            .compact(true)
            .on_press(())
            .into()
    };
    let node = layout_of(build(20.0), &renderer);
    assert_eq!(node.size(), Size::new(32.0, 32.0), "the space_l square");
    let mark = &node.children()[0];
    assert_eq!(mark.size(), Size::new(20.0, 20.0), "not narrowed");
    let left = mark.bounds().x;
    let right = node.size().width - (left + mark.size().width);
    assert!((left - right).abs() < 0.5, "centred: {left} vs {right}");
    assert_eq!(clipped(build(20.0), 20.0, &renderer), None);
    assert_eq!(clipped(build(32.0), 32.0, &renderer), None);
    for look in [ButtonLook::Primary, ButtonLook::Secondary] {
        let node = layout_of(
            Button::new(look, "Effort")
                .leading(sized_mark(20.0))
                .compact(true)
                .into(),
            &renderer,
        );
        assert_eq!(node.size(), Size::new(32.0, 32.0), "{look:?} compact");
    }
}

/// How many pixels of a `requested`-wide compact mark the button's square
/// cannot show: layout limits the mark to the square, so a larger one comes
/// out narrower than asked and is clipped. This makes that design error
/// visible in a test instead of silent. `None` when the mark is drawn whole.
fn clipped(element: Element<'_, ()>, requested: f32, renderer: &cosmic::Renderer) -> Option<f32> {
    let node = layout_of(element, renderer);
    let shown = node.children()[0].size().width;
    (shown + 0.01 < requested).then_some(requested - shown)
}

#[test]
fn a_mark_larger_than_the_square_is_detectable() {
    let renderer = renderer();
    let element: Element<'static, ()> = Button::tertiary("Big")
        .leading(sized_mark(40.0))
        .compact(true)
        .into();
    assert_eq!(clipped(element, 40.0, &renderer), Some(8.0));
}

#[test]
fn compact_with_nothing_leading_stays_labelled() {
    let renderer = renderer();
    let compact = layout_of(Button::tertiary("Effort").compact(true).into(), &renderer);
    let labelled = layout_of(Button::tertiary("Effort").into(), &renderer);
    assert_eq!(compact.size(), labelled.size());
}

#[test]
fn size_never_depends_on_selection_enablement_or_which_label_is_shown() {
    let renderer = renderer();
    for look in [
        ButtonLook::Primary,
        ButtonLook::Secondary,
        ButtonLook::Tertiary,
        ButtonLook::Destructive,
    ] {
        let size = |label: &str, other: &str, selected: bool, enabled: bool| {
            let button = Button::new(look, label)
                .other_labels([other])
                .selected(selected)
                .on_press_maybe(enabled.then_some(()));
            layout_of(button.into(), &renderer).size()
        };
        let copy = size("Copy", "Copied", false, true);
        assert_eq!(copy, size("Copied", "Copy", false, true), "{look:?}");
        assert_eq!(copy, size("Copy", "Copied", true, true), "{look:?}");
        assert_eq!(copy, size("Copy", "Copied", false, false), "{look:?}");
        let alone = layout_of(Button::new(look, "Copied").into(), &renderer).size();
        assert_eq!(copy, alone, "{look:?}: as wide as its widest label");
    }
    let icon = |selected: bool, enabled: bool| {
        let (button, _) = Button::icon_with(sized_mark(16.0), "Pin", tip())
            .selected(selected)
            .on_press_maybe(enabled.then_some(()))
            .into_native();
        layout_of(button.into(), &renderer).size()
    };
    for (selected, enabled) in [(true, true), (false, false), (true, false)] {
        assert_eq!(icon(false, true), icon(selected, enabled));
    }
}

/// `element` drawn in a 96 × 48 canvas, centred, the pointer at `cursor`
/// and pressed there if `press`: the pixels
fn drawn(element: Element<'static, ()>, cursor: Point, press: bool) -> Vec<[u8; 4]> {
    drawn_with(element, cursor, press, false)
}

/// Gives the first focusable widget the keyboard's focus
struct FocusFirst(bool);

impl cosmic::iced::advanced::widget::Operation for FocusFirst {
    fn traverse(
        &mut self,
        visit: &mut dyn FnMut(&mut dyn cosmic::iced::advanced::widget::Operation),
    ) {
        visit(self);
    }

    fn focusable(
        &mut self,
        _: Option<&cosmic::widget::Id>,
        _: Rectangle,
        focus: &mut dyn cosmic::iced::advanced::widget::operation::Focusable,
    ) {
        if !self.0 {
            focus.focus();
            self.0 = true;
        }
    }
}

/// [`drawn`], the keyboard's focus first given to `focus` the first button
fn drawn_with(
    element: Element<'static, ()>,
    cursor: Point,
    press: bool,
    focus: bool,
) -> Vec<[u8; 4]> {
    use cosmic::iced::advanced::{clipboard, renderer::Style};
    use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
    use cosmic::iced::{Event, mouse};
    let mut renderer = renderer();
    let canvas = Size::new(96.0, 48.0);
    let centred: Element<'static, ()> = widget::container(element).center(Length::Fill).into();
    let mut ui = UserInterface::build(centred, canvas, Cache::default(), &mut renderer);
    let cursor = mouse::Cursor::Available(cursor);
    if focus {
        ui.operate(&renderer, &mut FocusFirst(false));
    }
    if press {
        let mut messages = Vec::new();
        let _ = ui.update(
            &[Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Left,
            ))],
            cursor,
            &mut renderer,
            &mut clipboard::Null,
            &mut messages,
        );
    }
    ui.draw(
        &mut renderer,
        &cosmic::Theme::default(),
        &Style::default(),
        cursor,
    );
    // A mid grey ground, on which a light or a dark wash both show
    let pixels = renderer.screenshot(Size::new(96, 48), 1.0, Color::from_rgb(0.5, 0.5, 0.5));
    pixels.as_chunks::<4>().0.to_vec()
}

/// Pixels of a 96-wide picture outside `bounds` that are not the ground
/// (its corner's colour)
fn outside(pixels: &[[u8; 4]], bounds: Rectangle) -> usize {
    let ground = pixels[0];
    pixels
        .iter()
        .enumerate()
        .filter(|(n, pixel)| {
            let (x, y) = ((n % 96) as f32 + 0.5, (n / 96) as f32 + 0.5);
            !bounds.contains(Point::new(x, y)) && **pixel != ground
        })
        .count()
}

#[test]
fn washes_are_drawn_inside_the_buttons_box() {
    let renderer = renderer();
    for look in [
        ButtonLook::Secondary,
        ButtonLook::Tertiary,
        ButtonLook::Primary,
    ] {
        let build = || -> Element<'static, ()> {
            Button::new(look, "Go")
                .width(Length::Fixed(64.0))
                .on_press(())
                .into()
        };
        let size = layout_of(build(), &renderer).size();
        let bounds = Rectangle::new(
            Point::new((96.0 - size.width) / 2.0, (48.0 - size.height) / 2.0),
            size,
        );
        let centre = bounds.center();
        let rest = drawn(build(), Point::new(1.0, 1.0), false);
        for press in [false, true] {
            let washed = drawn(build(), centre, press);
            assert_eq!(outside(&washed, bounds), 0, "{look:?} press {press}");
            if look != ButtonLook::Primary {
                assert_ne!(rest, washed, "{look:?} washes (press {press})");
            }
        }
    }
}

#[test]
fn every_tertiary_washes_on_hover_and_press_and_has_none_at_rest() {
    let theme = cosmic::Theme::default();
    for look in [
        ButtonLook::Tertiary,
        ButtonLook::TertiaryDestructive,
        ButtonLook::Secondary,
    ] {
        let at = |state| paint(look, false, None, state, &theme).background;
        assert_eq!(at(State::Rest), None, "{look:?}");
        assert_eq!(at(State::Disabled), None, "{look:?}");
        assert!(at(State::Hovered).is_some(), "{look:?}");
        assert!(at(State::Pressed).is_some(), "{look:?}");
    }
}

#[test]
fn the_menus_more_row_washes_on_hover_as_a_tertiary_does() {
    use cosmic::iced::widget::button::{Catalog, Status};
    let theme = <cosmic::Theme as Default>::default();
    let class = crate::menu_parts::tertiary_row_style(crate::menu_parts::RowContour::Rounded);
    let style = |status| Catalog::style(&theme, &class, status);
    assert_eq!(style(Status::Active).background, None);
    let hovered = paint(ButtonLook::Tertiary, false, None, State::Hovered, &theme);
    assert_eq!(
        style(Status::Hovered).background,
        hovered.background.map(Background::Color)
    );
    assert!(style(Status::Pressed).background.is_some());
    assert_eq!(Some(style(Status::Active).text_color), hovered.text);
}

#[test]
fn a_selected_icon_is_the_muted_accent_and_a_fill_hook_replaces_the_theme() {
    let theme = cosmic::Theme::default();
    let (muted, on) = crate::muted_colours(&theme);
    let selected = paint(ButtonLook::Icon, true, None, State::Rest, &theme);
    assert_eq!(selected.background, Some(muted));
    assert_eq!(selected.text, Some(on));
    let fill = FillColours {
        base: Color::from_rgb(0.2, 0.3, 0.4),
        hovered: Color::from_rgb(0.3, 0.3, 0.4),
        pressed: Color::from_rgb(0.4, 0.3, 0.4),
        on: Color::WHITE,
    };
    let at = |state| paint(ButtonLook::Primary, false, Some(fill), state, &theme);
    assert_eq!(at(State::Rest).background, Some(fill.base));
    assert_eq!(at(State::Hovered).background, Some(fill.hovered));
    assert_eq!(at(State::Pressed).background, Some(fill.pressed));
    assert_eq!(at(State::Rest).text, Some(Color::WHITE));
}

#[test]
fn compact_draws_its_own_mark_and_responds_to_a_click() {
    use cosmic::iced::advanced::{clipboard, renderer::Style};
    use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
    use cosmic::iced::{Event, mouse};

    let mut renderer = renderer();
    let element: Element<'_, ()> = Button::tertiary("Named action")
        .leading(mark())
        .compact(true)
        .on_press(())
        .into();
    let mut ui = UserInterface::build(
        element,
        Size::new(32.0, 32.0),
        Cache::default(),
        &mut renderer,
    );
    let cursor = mouse::Cursor::Available(Point::new(16.0, 16.0));
    let events = [
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ];
    let mut messages = Vec::new();
    let _ = ui.update(
        &events,
        cursor,
        &mut renderer,
        &mut clipboard::Null,
        &mut messages,
    );
    assert_eq!(messages.len(), 1);
    ui.draw(
        &mut renderer,
        &cosmic::Theme::default(),
        &Style::default(),
        cursor,
    );
    let pixels = renderer.screenshot(Size::new(32, 32), 1.0, Color::BLACK);
    assert!(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] > 240 && p[1] < 15 && p[2] < 15),
        "the custom red mark survives rendering"
    );
}

#[test]
fn a_labelled_tertiary_responds_to_a_click_on_its_label() {
    use cosmic::iced::advanced::clipboard;
    use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
    use cosmic::iced::{Event, mouse};

    let mut renderer = renderer();
    let element: Element<'_, ()> = Button::tertiary("Learn more").on_press(()).into();
    let size = layout_of(Button::tertiary("Learn more").into(), &renderer).size();
    let mut ui = UserInterface::build(element, size, Cache::default(), &mut renderer);
    let cursor = mouse::Cursor::Available(Point::new(size.width / 2.0, size.height / 2.0));
    let mut messages = Vec::new();
    let _ = ui.update(
        &[
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ],
        cursor,
        &mut renderer,
        &mut clipboard::Null,
        &mut messages,
    );
    assert_eq!(messages.len(), 1, "the label does not swallow the click");
}

/// Every look in every state: nothing is drawn outside the box (no outline
/// field is used), and no border is thicker than the look's thickest, which
/// its side inset covers, so a border never meets the content
#[test]
fn every_looks_borders_are_inside_and_within_its_inset() {
    let theme = cosmic::Theme::default();
    let spacing = theme.cosmic().spacing;
    for look in [
        ButtonLook::Primary,
        ButtonLook::Secondary,
        ButtonLook::Tertiary,
        ButtonLook::Destructive,
        ButtonLook::TertiaryDestructive,
        ButtonLook::Icon,
        ButtonLook::Segment,
    ] {
        let room = if look == ButtonLook::Icon {
            f32::from(spacing.space_l) / 2.0
        } else {
            look.inset(&spacing)
        };
        assert!(
            look.thickest_border() <= room,
            "{look:?}: the inset covers the thickest border"
        );
        for selected in [false, true] {
            for state in [State::Rest, State::Hovered, State::Pressed, State::Disabled] {
                for focused in [false, true] {
                    let style = style(look, selected, None, state, focused, None, &theme);
                    assert_eq!(style.outline_width, 0.0, "{look:?} {state:?}: no outline outside");
                    assert!(
                        style.border_width <= look.thickest_border(),
                        "{look:?} {state:?} focused {focused}: {}",
                        style.border_width
                    );
                    if focused && state != State::Disabled {
                        assert_eq!(style.border_width, look.thickest_border(), "{look:?} ring");
                        let (width, colour) = focus_border(look, selected, None, state, &theme);
                        assert_eq!((style.border_width, style.border_color), (width, colour));
                    }
                }
            }
        }
    }
    // Secondary's focus is thicker than its rest border, so it shows
    let rest = style(ButtonLook::Secondary, false, None, State::Rest, false, None, &theme);
    let focus = style(ButtonLook::Secondary, false, None, State::Rest, true, None, &theme);
    assert!(focus.border_width > rest.border_width);
}

/// A solid fill's ring is the label's colour, which the fill shows; the
/// accent line it would otherwise be is the fill's own colour
#[test]
fn a_ring_on_a_solid_fill_is_not_the_fills_own_colour() {
    for theme in [cosmic::Theme::light(), cosmic::Theme::dark()] {
        for look in [ButtonLook::Primary, ButtonLook::Destructive] {
            let (_, ring) = focus_border(look, false, None, State::Rest, &theme);
            let fill = paint(look, false, None, State::Rest, &theme).background;
            assert_ne!(Some(ring), fill, "{look:?}");
        }
        let (_, ring) = focus_border(ButtonLook::Icon, true, None, State::Rest, &theme);
        assert_ne!(Some(ring), muted_paint(State::Rest, &theme).background);
    }
}

/// The focus ring is drawn on the button's own edge: focused, a button
/// paints nothing outside its box, and changes pixels inside its edge
#[test]
fn a_focused_button_paints_only_inside_its_box() {
    let renderer = renderer();
    for look in [
        ButtonLook::Primary,
        ButtonLook::Secondary,
        ButtonLook::Tertiary,
        ButtonLook::Destructive,
        ButtonLook::TertiaryDestructive,
    ] {
        let build = || -> Element<'static, ()> {
            Button::new(look, "Go")
                .width(Length::Fixed(64.0))
                .on_press(())
                .into()
        };
        let size = layout_of(build(), &renderer).size();
        let bounds = Rectangle::new(
            Point::new((96.0 - size.width) / 2.0, (48.0 - size.height) / 2.0),
            size,
        );
        let away = Point::new(1.0, 1.0);
        let rest = drawn(build(), away, false);
        let focused = drawn_with(build(), away, false, true);
        assert_eq!(outside(&focused, bounds), 0, "{look:?}: nothing outside the box");
        assert_ne!(rest, focused, "{look:?}: focus shows");
        // On the edge itself, in the middle of the left side: the ring's
        // innermost pixel, which the rest paint (Secondary's thinner
        // border included) does not reach
        let (x, y) = (bounds.x as usize, bounds.center_y() as usize);
        let inner = x + look.thickest_border() as usize - 1;
        assert_ne!(rest[y * 96 + inner], focused[y * 96 + inner], "{look:?}: the ring is on the edge");
        // And the layout does not move: the same box focused or not
        assert_eq!(layout_of(build(), &renderer).size(), size);
    }
}

/// An icon button and a chosen icon button, focused, stay in their square
#[test]
fn a_focused_icon_button_paints_only_inside_its_square() {
    let renderer = renderer();
    for selected in [false, true] {
        let build = || -> Element<'static, ()> {
            let (button, _) = Button::icon_with(sized_mark(16.0), "Pin", tip())
                .selected(selected)
                .on_press(())
                .into_native();
            button.into()
        };
        let size = layout_of(build(), &renderer).size();
        let bounds = Rectangle::new(
            Point::new((96.0 - size.width) / 2.0, (48.0 - size.height) / 2.0),
            size,
        );
        let away = Point::new(1.0, 1.0);
        let rest = drawn(build(), away, false);
        let focused = drawn_with(build(), away, false, true);
        assert_eq!(outside(&focused, bounds), 0, "selected {selected}");
        assert_ne!(rest, focused, "selected {selected}: focus shows");
    }
}
