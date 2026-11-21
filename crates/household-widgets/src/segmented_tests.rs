// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use cosmic::iced::advanced::layout;
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::advanced::widget::Tree;
use cosmic::iced::{Color, Font, Pixels, Point, Rectangle, Size};

fn renderer() -> cosmic::Renderer {
    cosmic::iced::futures::executor::block_on(<cosmic::Renderer as Headless>::new(
        Font::default(),
        Pixels(14.0),
        Some("tiny-skia"),
    ))
    .expect("native renderer")
}

fn layout_of(mut element: Element<'_, u8>, renderer: &cosmic::Renderer) -> layout::Node {
    let mut tree = Tree::new(element.as_widget());
    element.as_widget_mut().layout(
        &mut tree,
        renderer,
        &layout::Limits::new(Size::ZERO, Size::new(900.0, 200.0)),
    )
}

fn bar<'a>(labels: &[&str], chosen: Option<usize>, enabled: bool) -> Segmented<'a, u8> {
    Segmented::new(
        labels.iter().enumerate().map(|(n, label)| {
            Part::new(*label).on_press_maybe(enabled.then_some(n as u8))
        }),
        chosen,
    )
}

/// The parts' boxes within the bar, and the bar's size
fn parts_of(node: &layout::Node) -> (Size, Vec<Rectangle>) {
    // The bar's layout is its row's: the parts are its children
    let parts = node
        .children()
        .iter()
        .map(|part| {
            Rectangle::new(
                part.bounds().position(),
                part.size(),
            )
        })
        .collect();
    (node.size(), parts)
}

const LABELS: [&str; 3] = ["Paper", "Text", "Terminal"];

#[test]
fn the_bar_is_space_l_tall_and_its_parts_fill_it_with_no_gap() {
    let renderer = renderer();
    let node = layout_of(bar(&LABELS, Some(1), true).into(), &renderer);
    let (size, parts) = parts_of(&node);
    assert_eq!(parts.len(), 3);
    let space_l = f32::from(theme::active().cosmic().spacing.space_l);
    assert_eq!(space_l, crate::button::button_height());
    for part in &parts {
        assert_eq!(part.height, space_l);
        assert_eq!(part.y, 0.0, "the parts fill the bar");
    }
    assert_eq!(size.height, space_l, "as tall as a button");
    assert_eq!(parts[0].x, 0.0);
    for pair in parts.windows(2) {
        assert!((pair[1].x - (pair[0].x + pair[0].width)).abs() < 0.01, "no gap");
    }
    let end = parts[2].x + parts[2].width;
    assert!((size.width - end).abs() < 0.01, "only as wide as its parts");
}

#[test]
fn a_parts_width_is_its_widest_weight_and_never_changes_with_the_choice() {
    let renderer = renderer();
    let widths = |chosen, enabled| {
        let node = layout_of(bar(&LABELS, chosen, enabled).into(), &renderer);
        parts_of(&node).1.iter().map(|p| p.width).collect::<Vec<_>>()
    };
    let base = widths(None, true);
    for chosen in [Some(0), Some(1), Some(2)] {
        assert_eq!(widths(chosen, true), base, "chosen {chosen:?}");
        assert_eq!(widths(chosen, false), base, "disabled, chosen {chosen:?}");
    }
    for (label, width) in LABELS.iter().zip(&base) {
        // The form's measure is a pixel or two over what the button lays
        // out, never under it: parts at that width always hold their label
        assert!(
            (0.0..=2.0).contains(&(part_width(label, false) - width)),
            "{label}: {width} vs {}",
            part_width(label, false)
        );
    }
}

#[test]
fn only_the_bars_ends_are_rounded() {
    let radius = [16.0, 17.0, 18.0, 19.0];
    assert_eq!(part_corners(0, 3, radius), [16.0, 0.0, 0.0, 19.0]);
    assert_eq!(part_corners(1, 3, radius), [0.0; 4]);
    assert_eq!(part_corners(2, 3, radius), [0.0, 17.0, 18.0, 0.0]);
    assert_eq!(part_corners(0, 1, radius), radius, "a bar of one is rounded both sides");
}

/// `element` drawn in a 240 × 48 canvas at its top left, on a mid grey
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

fn is_muted(pixel: [u8; 4]) -> bool {
    let (muted, _) = crate::muted_colours(&cosmic::Theme::default());
    let near = |a: u8, b: f32| (f32::from(a) - b * 255.0).abs() <= 3.0;
    near(pixel[0], muted.r) && near(pixel[1], muted.g) && near(pixel[2], muted.b)
}

#[test]
fn the_chosen_part_is_the_muted_fill_with_square_inner_corners() {
    let renderer = renderer();
    let node = layout_of(bar(&LABELS, Some(1), true).into(), &renderer);
    let (_, parts) = parts_of(&node);
    let middle = parts[1];
    let pixels = drawn(bar(&LABELS, Some(1), true).into());
    // The middle part's own corners are square: the fill reaches them
    let (left, right) = (middle.x.ceil(), (middle.x + middle.width).floor() - 1.0);
    // One pixel in from the bar's edge, which the outline is painted over
    let (top, bottom) = (middle.y.ceil() + OUTLINE, (middle.y + middle.height).floor() - 1.0 - OUTLINE);
    for (x, y) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
        assert!(is_muted(pixel(&pixels, x, y)), "square corner at ({x}, {y})");
    }
    assert!(is_muted(pixel(&pixels, middle.x + 3.0, middle.y + 16.0)), "its fill");
    // The outline is on the bar's own edge, over the chosen fill, and
    // nothing is drawn beyond it
    let mid = (middle.x + middle.width / 2.0).floor();
    let edge = pixel(&pixels, mid, 0.0);
    assert!(!is_muted(edge), "the outline lies over the part's top edge");
    assert_ne!(edge, pixel(&pixels, mid, top), "and differs from the fill under it");
    assert_eq!(pixel(&pixels, mid, middle.height), pixel(&pixels, 120.0, 40.0), "nothing below the bar");
    assert_eq!(pixel(&pixels, mid, 31.0), edge, "the same line at the bottom edge");
    assert!(!is_muted(pixel(&pixels, parts[0].x + 3.0, parts[0].y + 16.0)), "an unchosen part has none");
    // The first part, chosen, is rounded at the bar's end and square inside
    let pixels = drawn(bar(&LABELS, Some(0), true).into());
    let first = parts[0];
    assert!(!is_muted(pixel(&pixels, first.x.ceil(), top)), "rounded outer corner");
    assert!(is_muted(pixel(&pixels, (first.x + first.width).floor() - 1.0, top)), "square inner corner");
    let pixels = drawn(bar(&LABELS, Some(2), true).into());
    let last = parts[2];
    assert!(!is_muted(pixel(&pixels, (last.x + last.width).floor() - 1.0, top)), "rounded outer corner");
    assert!(is_muted(pixel(&pixels, last.x.ceil(), top)), "square inner corner");
}

#[test]
fn hover_and_press_wash_inside_one_part_and_choosing_follows_a_click() {
    use cosmic::iced::advanced::clipboard;
    use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
    use cosmic::iced::{Event, mouse};
    let mut renderer = renderer();
    let node = layout_of(bar(&LABELS, Some(0), true).into(), &renderer);
    let (_, parts) = parts_of(&node);
    let element: Element<'static, u8> = bar(&LABELS, Some(0), true).into();
    let mut ui = UserInterface::build(
        element,
        Size::new(240.0, 48.0),
        Cache::default(),
        &mut renderer,
    );
    let cursor = mouse::Cursor::Available(parts[2].center());
    let mut messages: Vec<u8> = Vec::new();
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
    assert_eq!(messages, vec![2], "the part under the pointer is chosen");
    // A part with no message does nothing
    let element: Element<'static, u8> = bar(&LABELS, Some(0), false).into();
    let mut ui = UserInterface::build(
        element,
        Size::new(240.0, 48.0),
        Cache::default(),
        &mut renderer,
    );
    let mut messages: Vec<u8> = Vec::new();
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
    assert!(messages.is_empty(), "disabled");
}

/// Counts the focusable widgets an operation meets
struct Count(usize);

impl cosmic::iced::advanced::widget::Operation for Count {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn cosmic::iced::advanced::widget::Operation)) {
        visit(self);
    }

    fn focusable(
        &mut self,
        _: Option<&cosmic::widget::Id>,
        _: Rectangle,
        _: &mut dyn cosmic::iced::advanced::widget::operation::Focusable,
    ) {
        self.0 += 1;
    }
}

/// Terminal's control has no arrow keys: each part is a button the Tab key
/// reaches and Enter or Space presses. So every part is a Tab stop
#[test]
fn each_part_is_reached_by_tab() {
    let renderer = renderer();
    let count = |enabled: bool| {
        let mut element: Element<'_, u8> = bar(&LABELS, Some(0), enabled).into();
        let mut tree = Tree::new(element.as_widget());
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(900.0, 200.0)),
        );
        let mut operation = Count(0);
        element.as_widget_mut().operate(
            &mut tree,
            cosmic::iced::advanced::Layout::new(&node),
            &renderer,
            &mut operation,
        );
        operation.0
    };
    assert_eq!(count(true), 3);
    // libcosmic keeps a disabled button focusable, so that a keyboard user
    // can reach it to read its tooltip (rule 6a, disabled entries stay
    // reachable); it cannot be pressed
    assert_eq!(count(false), 3);
}

#[test]
fn stacked_bars_in_a_form_share_one_part_width_and_line_up() {
    let renderer = renderer();
    let kinds = ["Conversation", "Session"];
    let others = ["A new worktree"];
    let part = form_part(
        kinds
            .iter()
            .chain(LABELS.iter())
            .chain(others.iter())
            .map(|label| (*label, false)),
        3,
        None,
    );
    assert!(!part.icons);
    assert_eq!(
        part.width,
        part_width("A new worktree", false).max(part_width("Conversation", false))
    );
    let mut edges = Vec::new();
    for labels in [&kinds[..], &LABELS[..]] {
        let node = layout_of(bar(labels, Some(0), true).form(part).into(), &renderer);
        let (_, parts) = parts_of(&node);
        for p in &parts {
            assert_eq!(p.width, part.width);
        }
        edges.push(parts.iter().map(|p| p.x).collect::<Vec<_>>());
    }
    assert_eq!(edges[0][..2], edges[1][..2]);
}

#[test]
fn a_form_with_room_shares_it_and_with_icons_keeps_an_icons_room() {
    let part = form_part(LABELS.iter().map(|l| (*l, false)), 3, Some(150.0));
    assert_eq!(part.width, 50.0, "a third of the room, whole pixels");
    assert!(form_part([("A", true), ("B", false)], 2, None).icons);
    let with = part_width("Text", true);
    let without = part_width("Text", false);
    let space = theme::active().cosmic().spacing;
    assert!((with - without - (16.0 + f32::from(space.space_xxs))).abs() <= 1.0);
    assert!(with > without);
    // Icons: an icon-less part in an icon form is as wide as its siblings
    let renderer = renderer();
    let part = form_part([("Text", true), ("Paper", false)], 2, None);
    let node = layout_of(
        Segmented::new(
            [Part::new("Text").on_press(0u8), Part::new("Paper").on_press(1u8)],
            None,
        )
        .form(part)
        .into(),
        &renderer,
    );
    let (_, parts) = parts_of(&node);
    assert!(parts.iter().all(|p| p.width == part.width));
}

#[test]
fn the_bar_in_a_form_cuts_a_long_label_short_to_its_part() {
    let renderer = renderer();
    let part = FormPart { width: 60.0, icons: false };
    let node = layout_of(bar(&["A very long label indeed"], Some(0), true).form(part).into(), &renderer);
    let (size, parts) = parts_of(&node);
    assert_eq!(parts[0].width, 60.0);
    assert_eq!(size.width, 60.0);
}

/// `element` drawn as [`drawn`] is, the keyboard's focus first given to its
/// `nth` focusable part
fn drawn_focused(element: Element<'static, u8>, nth: usize) -> Vec<[u8; 4]> {
    use cosmic::iced::advanced::renderer::Style;
    use cosmic::iced::mouse;
    use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
    struct Nth(usize, usize);
    impl cosmic::iced::advanced::widget::Operation for Nth {
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
            if self.1 == self.0 {
                focus.focus();
            }
            self.1 += 1;
        }
    }
    let mut renderer = renderer();
    let mut ui = UserInterface::build(element, Size::new(240.0, 48.0), Cache::default(), &mut renderer);
    ui.operate(&renderer, &mut Nth(nth, 0));
    let cursor = mouse::Cursor::Available(Point::new(239.0, 47.0));
    ui.draw(&mut renderer, &cosmic::Theme::default(), &Style::default(), cursor);
    renderer
        .screenshot(Size::new(240, 48), 1.0, Color::from_rgb(0.5, 0.5, 0.5))
        .as_chunks::<4>()
        .0
        .to_vec()
}

fn is_colour(pixel: [u8; 4], colour: Color) -> bool {
    // A stroke on the curved edge is anti-aliased: a few levels of slack
    let near = |a: u8, b: f32| (f32::from(a) - b * 255.0).abs() <= 8.0;
    near(pixel[0], colour.r) && near(pixel[1], colour.g) && near(pixel[2], colour.b)
}

/// The bar's outline is painted over the parts, which would cover a part's
/// keyboard-target outline on the bar's edge: an end part's outline (the
/// shared dark-grey stroke on a part with no fill) is still whole at the
/// bar's end, top and bottom, and nothing is drawn outside the bar
#[test]
fn a_focused_end_part_shows_its_outline_over_the_bars_outline() {
    let renderer = renderer();
    let node = layout_of(bar(&LABELS, Some(1), true).into(), &renderer);
    let (size, parts) = parts_of(&node);
    let theme = cosmic::Theme::default();
    let dark: Color = crate::menu_parts::RowContour::Rounded.target_border(&theme).color;
    let (first, last) = (parts[0], parts[2]);
    for (nth, part, end_x) in [
        (0, first, first.x.floor()),
        (2, last, (last.x + last.width).floor() - 1.0),
    ] {
        let pixels = drawn_focused(bar(&LABELS, Some(1), true).into(), nth);
        let mid_y = (part.y + part.height / 2.0).floor();
        assert!(is_colour(pixel(&pixels, end_x, mid_y), dark), "part {nth}: the outline at the bar's end");
        let mid_x = (part.x + part.width / 2.0).floor();
        assert!(is_colour(pixel(&pixels, mid_x, 0.0), dark), "part {nth}: the outline at the top");
        assert!(is_colour(pixel(&pixels, mid_x, size.height.floor() - 1.0), dark), "part {nth}: the outline at the bottom");
        let ground = pixel(&pixels, 239.0, 47.0);
        for y in 0..48 {
            for x in 0..240 {
                let inside = (x as f32) < size.width && (y as f32) < size.height;
                if !inside {
                    assert_eq!(pixels[y * 240 + x], ground, "({x}, {y}) is outside the bar");
                }
            }
        }
    }
    // The unfocused bar has no such stroke there
    let pixels = drawn(bar(&LABELS, Some(1), true).into());
    assert!(!is_colour(pixel(&pixels, first.x.floor(), (first.height / 2.0).floor()), dark));
}

/// A chosen end part has a solid muted fill: whether its outline is the
/// double is the shared rule's, resolved against that fill
#[test]
fn a_focused_chosen_part_follows_the_shared_fill_rule() {
    let renderer = renderer();
    let node = layout_of(bar(&LABELS, Some(0), true).into(), &renderer);
    let (_, parts) = parts_of(&node);
    let theme = cosmic::Theme::default();
    let first = parts[0];
    let pixels = drawn_focused(bar(&LABELS, Some(0), true).into(), 0);
    let mid_y = (first.height / 2.0).floor();
    let dark: Color = crate::menu_parts::RowContour::Rounded.target_border(&theme).color;
    assert!(is_colour(pixel(&pixels, first.x.floor(), mid_y), dark));
    let double = crate::button::displayed_fill(ButtonLook::Segment, true, crate::button::State::Rest, None, &theme)
        .is_some_and(|f| household_tones::lighter_on(household_tones::Srgb::new(f.r, f.g, f.b)));
    assert_eq!(is_colour(pixel(&pixels, first.x.floor() + 1.0, mid_y), Color::WHITE), double);
}

/// The parts' size is the same whichever has the focus
#[test]
fn the_bars_size_is_the_same_with_any_part_focused() {
    let renderer = renderer();
    let before = layout_of(bar(&LABELS, Some(1), true).into(), &renderer);
    let mut element: Element<'_, u8> = bar(&LABELS, Some(1), true).into();
    let mut tree = Tree::new(element.as_widget());
    let limits = layout::Limits::new(Size::ZERO, Size::new(900.0, 200.0));
    let node = element.as_widget_mut().layout(&mut tree, &renderer, &limits);
    struct Focus;
    impl cosmic::iced::advanced::widget::Operation for Focus {
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn cosmic::iced::advanced::widget::Operation)) {
            visit(self);
        }
        fn focusable(
            &mut self,
            _: Option<&cosmic::widget::Id>,
            _: Rectangle,
            focus: &mut dyn cosmic::iced::advanced::widget::operation::Focusable,
        ) {
            focus.focus();
        }
    }
    element.as_widget_mut().operate(&mut tree, cosmic::iced::advanced::Layout::new(&node), &renderer, &mut Focus);
    let after = element.as_widget_mut().layout(&mut tree, &renderer, &limits);
    assert_eq!(before.size(), after.size());
    assert_eq!(parts_of(&before).1, parts_of(&after).1);
}
