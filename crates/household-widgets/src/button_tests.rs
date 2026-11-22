// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use cosmic::iced::advanced::layout;
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::advanced::widget::Tree;
use cosmic::iced::{Point, Size};

fn renderer() -> cosmic::Renderer {
    crate::test_fonts::renderer().expect("native renderer")
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
        (ButtonLook::Tertiary, 16.0),
        (ButtonLook::TertiaryDestructive, 16.0),
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
    let node = layout_of(button, &renderer);
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
        layout_of(button, &renderer).size()
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
                let style = style(look, selected, None, state, None, false, &theme);
                assert_eq!(
                    style.outline_width, 0.0,
                    "{look:?} {state:?}: no outline outside"
                );
                assert!(
                    style.border_width <= look.thickest_border(),
                    "{look:?} {state:?}"
                );
            }
        }
    }
    // Focus is the shared target outline, not a border of the button's own:
    // the libcosmic class the button draws with changes nothing when focused
    use cosmic::widget::button::Catalog;
    for look in [ButtonLook::Primary, ButtonLook::Secondary, ButtonLook::Icon] {
        let class = class(look, false, None, None, false, None);
        assert_eq!(
            format!("{:?}", theme.active(true, false, &class)),
            format!("{:?}", theme.active(false, false, &class))
        );
        assert_eq!(
            format!("{:?}", theme.hovered(true, false, &class)),
            format!("{:?}", theme.hovered(false, false, &class))
        );
    }
}

/// The household's keyboard-target colours, as 8-bit pixels
fn target_colours(theme: &cosmic::Theme) -> [u8; 3] {
    let dark = crate::menu_parts::RowContour::Rounded
        .target_border(theme)
        .color;
    [
        (dark.r * 255.0).round() as u8,
        (dark.g * 255.0).round() as u8,
        (dark.b * 255.0).round() as u8,
    ]
}

fn is_rgb(pixel: [u8; 4], rgb: [u8; 3]) -> bool {
    is_near(pixel, rgb, 8)
}

/// Within `slack` levels: a stroke on a curved edge is anti-aliased
fn is_near(pixel: [u8; 4], rgb: [u8; 3], slack: u8) -> bool {
    (0..3).all(|n| pixel[n].abs_diff(rgb[n]) <= slack)
}

/// The focus is the shared keyboard-target outline on the button's own edge,
/// inside it: the dark-grey stroke on the edge, and on a dark solid fill a
/// white stroke inside that; nothing outside the box, and the same layout
#[test]
fn a_focused_button_is_the_shared_outline_inside_its_box() {
    let renderer = renderer();
    let theme = cosmic::Theme::default();
    let dark = target_colours(&theme);
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
        assert_eq!(
            outside(&focused, bounds),
            0,
            "{look:?}: nothing outside the box"
        );
        assert_ne!(rest, focused, "{look:?}: focus shows");
        let (x, y) = (bounds.x as usize, bounds.center_y() as usize);
        assert!(
            is_rgb(focused[y * 96 + x], dark),
            "{look:?}: the dark stroke is on the edge: {:?} {:?} {:?} want {dark:?}",
            focused[y * 96 + x - 1],
            focused[y * 96 + x],
            focused[y * 96 + x + 1]
        );
        let double = displayed_fill(look, false, State::Rest, None, &theme)
            .is_some_and(|fill| household_tones::lighter_on(Srgb::new(fill.r, fill.g, fill.b)));
        if double {
            assert!(
                is_rgb(focused[y * 96 + x + 1], [255, 255, 255]),
                "{look:?}: white inside it"
            );
        } else {
            assert!(
                !is_rgb(focused[y * 96 + x + 1], [255, 255, 255]),
                "{look:?}: a single stroke"
            );
        }
        // The same box focused or not
        assert_eq!(layout_of(build(), &renderer).size(), size);
    }
}

/// An icon button and a chosen icon button, focused, stay in their square
/// and wear the shared outline; the chosen one's muted fill decides whether
/// it is the double
#[test]
fn a_focused_icon_button_wears_the_shared_outline_inside_its_square() {
    let renderer = renderer();
    let theme = cosmic::Theme::default();
    let dark = target_colours(&theme);
    for selected in [false, true] {
        let build = || -> Element<'static, ()> {
            let (button, _) = Button::icon_with(sized_mark(16.0), "Pin", tip())
                .selected(selected)
                .on_press(())
                .into_native();
            button
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
        let (x, y) = (bounds.x as usize, bounds.center_y() as usize);
        assert!(
            is_near(focused[y * 96 + x], dark, 20),
            "selected {selected}: dark on the edge: {:?} {:?} {:?}",
            focused[y * 96 + x - 1],
            focused[y * 96 + x],
            focused[y * 96 + x + 1]
        );
        let double = displayed_fill(ButtonLook::Icon, selected, State::Rest, None, &theme)
            .is_some_and(|fill| household_tones::lighter_on(Srgb::new(fill.r, fill.g, fill.b)));
        assert_eq!(
            is_rgb(focused[y * 96 + x + 1], [255, 255, 255]),
            double,
            "selected {selected}"
        );
    }
}

const LABELLED: [ButtonLook; 8] = [
    ButtonLook::Primary,
    ButtonLook::Secondary,
    ButtonLook::Tertiary,
    ButtonLook::Destructive,
    ButtonLook::TertiaryDestructive,
    ButtonLook::Segment,
    ButtonLook::Plain,
    ButtonLook::Icon,
];

#[test]
fn the_measurement_is_the_laid_out_width_in_whole_pixels_for_every_look() {
    let renderer = renderer();
    for look in LABELLED {
        for labels in [
            vec!["Go"],
            vec!["Copy", "Copied"],
            vec!["Set up in Settings", "Wi", "Quite a long label indeed"],
            vec!["Mmmm WWW iii"],
            // Labels the apps draw, which measured narrower than drawn when
            // the measurement was shaped differently from the label
            vec!["Send"],
            vec!["Steer"],
            vec!["Open in Cerea"],
            vec!["New terminal session"],
        ] {
            let build =
                || Button::<()>::new(look, labels[0]).other_labels(labels[1..].iter().copied());
            let laid = layout_of(build().into(), &renderer).size().width;
            assert_eq!(laid.fract(), 0.0, "{look:?} {labels:?}: whole pixels");
            assert_eq!(
                build().natural_width(&renderer),
                laid,
                "{look:?} {labels:?}"
            );
            assert_eq!(
                width_of(&renderer, look, &labels, None, false),
                laid,
                "{look:?} {labels:?}: the free measurement"
            );
            assert_eq!(
                label_width(look, &labels, None, false),
                laid,
                "{look:?} {labels:?}: measured without a renderer"
            );
            // With a leading icon and a trailing one
            let rich = || {
                build()
                    .leading_icon(widget::icon::from_name("go-next-symbolic"))
                    .trailing_icon(widget::icon::from_name("go-next-symbolic").handle())
            };
            let laid = layout_of(rich().into(), &renderer).size().width;
            assert_eq!(
                rich().natural_width(&renderer),
                laid,
                "{look:?} {labels:?} with icons"
            );
            assert_eq!(
                width_of(&renderer, look, &labels, Some(16.0), true),
                laid,
                "{look:?} {labels:?} with icons: the free measurement"
            );
            assert_eq!(
                label_width(look, &labels, Some(16.0), true),
                laid,
                "{look:?} {labels:?} with icons: measured without a renderer"
            );
        }
    }
    // The caller's own content, and a leading element with a fractional width
    let content = || Button::with_content(ButtonLook::Secondary, sized_mark(30.4), "Chip");
    let laid = layout_of(content().into(), &renderer).size().width;
    assert_eq!(laid, content().natural_width(&renderer));
    assert_eq!(laid, 31.0 + 32.0);
    let odd = || Button::<()>::primary("Go").leading(sized_mark(10.5));
    let laid = layout_of(odd().into(), &renderer).size().width;
    assert_eq!(laid.fract(), 0.0);
    assert_eq!(laid, odd().natural_width(&renderer));
}

/// A button with a trailing icon, given exactly its natural width, shows its
/// whole label: the room kept for the icon is the icon's, not also the gap
/// the row has already set aside for it
#[test]
fn a_label_with_a_trailing_icon_is_not_cut_in_its_natural_width() {
    let renderer = renderer();
    for look in [ButtonLook::Plain, ButtonLook::Secondary] {
        let build = || {
            Button::<()>::new(look, "Medium")
                .trailing_icon(widget::icon::from_name("pan-down-symbolic").handle())
        };
        let natural = build().natural_width(&renderer);
        let (element, _, cut) = build().width(Length::Fixed(natural)).assemble();
        let (flag, _) = cut.expect("a label may be cut");
        let mut element = element;
        let mut tree = Tree::new(element.as_widget());
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(500.0, 100.0)),
        );
        assert_eq!(node.size().width, natural, "{look:?}");
        assert!(!flag.get(), "{look:?}: the label is whole");
    }
}

#[test]
fn a_label_wider_than_its_room_is_cut_and_says_so_only_then() {
    let renderer = renderer();
    let long = "Set up in Settings \u{2192} Profiles";
    for look in [
        ButtonLook::Tertiary,
        ButtonLook::Secondary,
        ButtonLook::Plain,
    ] {
        let natural = Button::<()>::new(look, long).natural_width(&renderer);
        let laid = |room: f32| {
            let (element, _, cut) = Button::<()>::new(look, long).assemble();
            let (flag, full) = cut.expect("a label may be cut");
            assert_eq!(full, long);
            let mut element = element;
            let mut tree = Tree::new(element.as_widget());
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(room, 100.0)),
            );
            (node.size(), flag.get())
        };
        assert_eq!(laid(500.0), (Size::new(natural, 32.0), false), "{look:?}");
        assert_eq!(
            laid(natural),
            (Size::new(natural, 32.0), false),
            "{look:?}: exactly enough"
        );
        let (size, cut) = laid(120.0);
        assert!(cut, "{look:?}: cut in 120 px");
        assert_eq!(
            size,
            Size::new(120.0, 32.0),
            "{look:?}: takes the room it is given"
        );
        // And the cut state is not sticky: the same button with room again
        let (_, cut) = laid(natural + 40.0);
        assert!(!cut, "{look:?}");
    }
    // A button with a tooltip of its own keeps it and is given no other;
    // an icon button's name is never cut
    let (_, tip, cut) = Button::<()>::tertiary(long)
        .tooltip(tip(), Position::Top)
        .assemble();
    assert!(tip.is_some());
    assert!(cut.is_some());
    let (_, _, cut) = Button::<()>::with_content(ButtonLook::Secondary, mark(), "Chip").assemble();
    assert!(cut.is_none(), "the caller's content is never cut");
}

#[test]
fn a_cut_label_is_drawn_inside_its_box() {
    // Cut to 80 px, the label ends in an ellipsis inside the box; uncut it
    // would run past it
    let long = "Set up in Settings \u{2192} Profiles";
    for look in [
        ButtonLook::Tertiary,
        ButtonLook::Secondary,
        ButtonLook::Plain,
        ButtonLook::Primary,
    ] {
        let build = || -> Element<'static, ()> {
            Button::new(look, long)
                .width(Length::Fixed(80.0))
                .on_press(())
                .into()
        };
        let size = layout_of(build(), &renderer()).size();
        assert_eq!(size.width, 80.0);
        let pixels = drawn(build(), Point::new(1.0, 1.0), false);
        let bounds = Rectangle::new(
            Point::new((96.0 - size.width) / 2.0, (48.0 - size.height) / 2.0),
            size,
        );
        assert_eq!(outside(&pixels, bounds), 0, "{look:?}");
    }
}

#[test]
fn a_selected_labelled_look_is_the_muted_accent_and_the_same_size() {
    let renderer = renderer();
    let theme = cosmic::Theme::default();
    let (muted, on) = crate::muted_colours(&theme);
    for look in LABELLED {
        let selected = paint(look, true, None, State::Rest, &theme);
        assert_eq!(selected.background, Some(muted), "{look:?}");
        assert_eq!(selected.text, Some(on), "{look:?}");
        assert_eq!(
            selected.border.is_some(),
            look == ButtonLook::Secondary,
            "{look:?}: only Secondary keeps its border"
        );
        let build = |chosen: bool| {
            Button::<()>::new(look, "Recommended")
                .selected(chosen)
                .on_press(())
        };
        let (a, b) = (
            layout_of(build(false).into(), &renderer),
            layout_of(build(true).into(), &renderer),
        );
        assert_eq!(a.size(), b.size(), "{look:?}");
        if look == ButtonLook::Icon {
            continue;
        }
        // Drawn: the inset beside the label is the muted fill
        let size = b.size();
        let bounds = Rectangle::new(
            Point::new((96.0 - size.width) / 2.0, (48.0 - size.height) / 2.0),
            size,
        );
        let pixels = drawn(build(true).into(), Point::new(1.0, 1.0), false);
        let at = Point::new(bounds.x + 6.0, bounds.center_y());
        let pixel = pixels[(at.y as usize) * 96 + at.x as usize];
        let want = [muted.r, muted.g, muted.b].map(|c| (c * 255.0).round() as u8);
        if look != ButtonLook::Secondary {
            assert!(is_near(pixel, want, 3), "{look:?}: {pixel:?} vs {want:?}");
        }
        let plain = drawn(build(false).into(), Point::new(1.0, 1.0), false);
        assert_ne!(pixels, plain, "{look:?}");
    }
}

#[test]
fn the_caller_s_content_makes_the_same_box_in_a_chosen_state() {
    let renderer = renderer();
    let chip = |chosen: bool| {
        Button::<()>::with_content(
            ButtonLook::Secondary,
            widget::column::with_children(vec![
                widget::text("Use the shell").into(),
                widget::text("Recommended").size(11).into(),
            ])
            .into(),
            "Use the shell",
        )
        .selected(chosen)
        .on_press(())
    };
    let (a, b) = (
        layout_of(chip(false).into(), &renderer).size(),
        layout_of(chip(true).into(), &renderer).size(),
    );
    assert_eq!(a, b);
    assert_eq!(a.height, 32.0, "a chip is a control tall, too");
    assert_eq!(a.width, chip(false).natural_width(&renderer));
    // Another content it may show is room it keeps
    let wider = Button::<()>::with_content(ButtonLook::Plain, sized_mark(20.0), "Strip")
        .other_contents([sized_mark(50.0)]);
    assert_eq!(layout_of(wider.into(), &renderer).size().width, 50.0 + 32.0);
}

#[test]
fn plain_is_the_icon_looks_paint_with_a_body_text_label_in_the_same_box() {
    let renderer = renderer();
    let theme = cosmic::Theme::default();
    for state in [State::Rest, State::Hovered, State::Pressed, State::Disabled] {
        assert_eq!(
            paint(ButtonLook::Plain, false, None, state, &theme),
            paint(ButtonLook::Icon, false, None, state, &theme),
            "{state:?}"
        );
    }
    let rest = paint(ButtonLook::Plain, false, None, State::Rest, &theme);
    assert!(
        rest.background.is_none_or(|fill| fill.a == 0.0),
        "no fill at rest"
    );
    assert_ne!(
        paint(ButtonLook::Plain, false, None, State::Hovered, &theme).background,
        None,
        "the icon button's wash"
    );
    // Not the accent
    assert_ne!(rest.text, Some(theme.cosmic().accent_text_color().into()));
    // The same box: `space_l` tall, `space_s` at each side of the content
    let node = layout_of(
        Button::<()>::new(ButtonLook::Plain, "\u{00d7}4")
            .leading_icon(widget::icon::from_name("go-next-symbolic"))
            .into(),
        &renderer,
    );
    assert_eq!(node.size().height, 32.0);
    assert_eq!(sides(&node), (16.0, 16.0));
    // Its label is regular: narrower than the semibold boxed looks'
    let plain = width_of(&renderer, ButtonLook::Plain, &["Recommended"], None, false);
    let primary = width_of(
        &renderer,
        ButtonLook::Primary,
        &["Recommended"],
        None,
        false,
    );
    assert!(
        plain <= primary,
        "regular is never wider: {plain} <= {primary}"
    );
}
