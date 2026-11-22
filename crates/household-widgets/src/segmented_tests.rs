// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use cosmic::iced::advanced::layout;
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::advanced::widget::Tree;
use cosmic::iced::{Color, Point, Rectangle, Size};

fn renderer() -> cosmic::Renderer {
    crate::test_fonts::renderer().expect("native renderer")
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
        labels
            .iter()
            .enumerate()
            .map(|(n, label)| Part::new(*label).on_press_maybe(enabled.then_some(n as u8))),
        chosen,
    )
}

/// The parts' boxes within the bar, and the bar's size
fn parts_of(node: &layout::Node) -> (Size, Vec<Rectangle>) {
    // The bar's layout is its row's: the parts are its children
    let parts = node
        .children()
        .iter()
        .map(|part| Rectangle::new(part.bounds().position(), part.size()))
        .collect();
    (node.size(), parts)
}

const LABELS: [&str; 3] = ["Paper", "Text", "Terminal"];

#[test]
fn the_bar_is_toggle_height_tall_and_its_parts_fill_it_with_no_gap() {
    let renderer = renderer();
    let node = layout_of(bar(&LABELS, Some(1), true).into(), &renderer);
    let (size, parts) = parts_of(&node);
    assert_eq!(parts.len(), 3);
    // Juliet, 22 Nov 2026: "Let's go for 26px for everything"; buttons stay space_l
    assert_eq!(crate::spacing::TOGGLE_HEIGHT, 26.0);
    assert_eq!(crate::button::button_height(), 32.0);
    for part in &parts {
        assert_eq!(part.height, crate::spacing::TOGGLE_HEIGHT);
        assert_eq!(part.y, 0.0, "the parts fill the bar");
    }
    assert_eq!(
        size.height,
        crate::spacing::TOGGLE_HEIGHT,
        "as tall as every toggle"
    );
    assert_eq!(parts[0].x, 0.0);
    for pair in parts.windows(2) {
        assert!(
            (pair[1].x - (pair[0].x + pair[0].width)).abs() < 0.01,
            "no gap"
        );
    }
    let end = parts[2].x + parts[2].width;
    assert!((size.width - end).abs() < 0.01, "only as wide as its parts");
}

#[test]
fn a_parts_width_is_its_widest_weight_and_never_changes_with_the_choice() {
    let renderer = renderer();
    let widths = |chosen, enabled| {
        let node = layout_of(bar(&LABELS, chosen, enabled).into(), &renderer);
        parts_of(&node)
            .1
            .iter()
            .map(|p| p.width)
            .collect::<Vec<_>>()
    };
    let base = widths(None, true);
    for chosen in [Some(0), Some(1), Some(2)] {
        assert_eq!(widths(chosen, true), base, "chosen {chosen:?}");
        assert_eq!(widths(chosen, false), base, "disabled, chosen {chosen:?}");
    }
    for (label, width) in LABELS.iter().zip(&base) {
        // The form's measure is what the button lays out, to the pixel
        assert_eq!(part_width(label, false), *width, "{label}");
    }
}

#[test]
fn only_the_bars_ends_are_rounded() {
    let radius = [16.0, 17.0, 18.0, 19.0];
    assert_eq!(part_corners(0, 3, radius), [16.0, 0.0, 0.0, 19.0]);
    assert_eq!(part_corners(1, 3, radius), [0.0; 4]);
    assert_eq!(part_corners(2, 3, radius), [0.0, 17.0, 18.0, 0.0]);
    assert_eq!(
        part_corners(0, 1, radius),
        radius,
        "a bar of one is rounded both sides"
    );
}

/// `element` drawn in a 240 × 48 canvas at its top left, on a mid grey
fn drawn(element: Element<'static, u8>) -> Vec<[u8; 4]> {
    use cosmic::iced::advanced::renderer::Style;
    use cosmic::iced::mouse;
    use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
    let mut renderer = renderer();
    let mut ui = UserInterface::build(
        element,
        Size::new(240.0, 48.0),
        Cache::default(),
        &mut renderer,
    );
    let cursor = mouse::Cursor::Available(Point::new(239.0, 47.0));
    ui.draw(
        &mut renderer,
        &cosmic::Theme::default(),
        &Style::default(),
        cursor,
    );
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
    let (top, bottom) = (
        middle.y.ceil() + OUTLINE,
        (middle.y + middle.height).floor() - 1.0 - OUTLINE,
    );
    for (x, y) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
        assert!(
            is_muted(pixel(&pixels, x, y)),
            "square corner at ({x}, {y})"
        );
    }
    assert!(
        is_muted(pixel(&pixels, middle.x + 3.0, middle.y + 13.0)),
        "its fill"
    );
    // The outline is on the bar's own edge, over the chosen fill, and
    // nothing is drawn beyond it
    let mid = (middle.x + middle.width / 2.0).floor();
    let edge = pixel(&pixels, mid, 0.0);
    assert!(!is_muted(edge), "the outline lies over the part's top edge");
    assert_ne!(
        edge,
        pixel(&pixels, mid, top),
        "and differs from the fill under it"
    );
    assert_eq!(
        pixel(&pixels, mid, middle.height),
        pixel(&pixels, 120.0, 40.0),
        "nothing below the bar"
    );
    assert_eq!(
        pixel(&pixels, mid, middle.height - 1.0),
        edge,
        "the same line at the bottom edge"
    );
    assert!(
        !is_muted(pixel(&pixels, parts[0].x + 3.0, parts[0].y + 13.0)),
        "an unchosen part has none"
    );
    // The first part, chosen, is rounded at the bar's end and square inside
    let pixels = drawn(bar(&LABELS, Some(0), true).into());
    let first = parts[0];
    assert!(
        !is_muted(pixel(&pixels, first.x.ceil(), top)),
        "rounded outer corner"
    );
    assert!(
        is_muted(pixel(&pixels, (first.x + first.width).floor() - 1.0, top)),
        "square inner corner"
    );
    let pixels = drawn(bar(&LABELS, Some(2), true).into());
    let last = parts[2];
    assert!(
        !is_muted(pixel(&pixels, (last.x + last.width).floor() - 1.0, top)),
        "rounded outer corner"
    );
    assert!(
        is_muted(pixel(&pixels, last.x.ceil(), top)),
        "square inner corner"
    );
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
            [
                Part::new("Text").on_press(0u8),
                Part::new("Paper").on_press(1u8),
            ],
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
    let part = FormPart {
        width: 60.0,
        icons: false,
    };
    let node = layout_of(
        bar(&["A very long label indeed"], Some(0), true)
            .form(part)
            .into(),
        &renderer,
    );
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
    let mut ui = UserInterface::build(
        element,
        Size::new(240.0, 48.0),
        Cache::default(),
        &mut renderer,
    );
    ui.operate(&renderer, &mut Nth(nth, 0));
    let cursor = mouse::Cursor::Available(Point::new(239.0, 47.0));
    ui.draw(
        &mut renderer,
        &cosmic::Theme::default(),
        &Style::default(),
        cursor,
    );
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
    let dark: Color = crate::menu_parts::RowContour::Rounded
        .target_border(&theme)
        .color;
    let (first, last) = (parts[0], parts[2]);
    for (nth, part, end_x) in [
        (0, first, first.x.floor()),
        (2, last, (last.x + last.width).floor() - 1.0),
    ] {
        let pixels = drawn_focused(bar(&LABELS, Some(1), true).into(), nth);
        let mid_y = (part.y + part.height / 2.0).floor();
        assert!(
            is_colour(pixel(&pixels, end_x, mid_y), dark),
            "part {nth}: the outline at the bar's end"
        );
        let mid_x = (part.x + part.width / 2.0).floor();
        assert!(
            is_colour(pixel(&pixels, mid_x, 0.0), dark),
            "part {nth}: the outline at the top"
        );
        assert!(
            is_colour(pixel(&pixels, mid_x, size.height.floor() - 1.0), dark),
            "part {nth}: the outline at the bottom"
        );
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
    assert!(!is_colour(
        pixel(&pixels, first.x.floor(), (first.height / 2.0).floor()),
        dark
    ));
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
    let dark: Color = crate::menu_parts::RowContour::Rounded
        .target_border(&theme)
        .color;
    assert!(is_colour(pixel(&pixels, first.x.floor(), mid_y), dark));
    let double = crate::button::displayed_fill(
        ButtonLook::Segment,
        true,
        crate::button::State::Rest,
        None,
        &theme,
    )
    .is_some_and(|f| household_tones::lighter_on(household_tones::Srgb::new(f.r, f.g, f.b)));
    assert_eq!(
        is_colour(pixel(&pixels, first.x.floor() + 1.0, mid_y), Color::WHITE),
        double
    );
}

/// The parts' size is the same whichever has the focus
#[test]
fn the_bars_size_is_the_same_with_any_part_focused() {
    let renderer = renderer();
    let before = layout_of(bar(&LABELS, Some(1), true).into(), &renderer);
    let mut element: Element<'_, u8> = bar(&LABELS, Some(1), true).into();
    let mut tree = Tree::new(element.as_widget());
    let limits = layout::Limits::new(Size::ZERO, Size::new(900.0, 200.0));
    let node = element
        .as_widget_mut()
        .layout(&mut tree, &renderer, &limits);
    struct Focus;
    impl cosmic::iced::advanced::widget::Operation for Focus {
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
            focus.focus();
        }
    }
    element.as_widget_mut().operate(
        &mut tree,
        cosmic::iced::advanced::Layout::new(&node),
        &renderer,
        &mut Focus,
    );
    let after = element
        .as_widget_mut()
        .layout(&mut tree, &renderer, &limits);
    assert_eq!(before.size(), after.size());
    assert_eq!(parts_of(&before).1, parts_of(&after).1);
}

/// The x, in the bar's own space, of each part's content row's children
/// (its icon's room, then its label): the first node below the part with
/// more than one child
fn content_columns(node: &layout::Node) -> Vec<Vec<f32>> {
    fn row(node: &layout::Node, x: f32) -> Vec<f32> {
        let x = x + node.bounds().x;
        match node.children() {
            [] => Vec::new(),
            [only] => row(only, x),
            many => many.iter().map(|child| x + child.bounds().x).collect(),
        }
    }
    node.children()
        .iter()
        .map(|part| row(part, node.bounds().x))
        .collect()
}

/// Bug B (Terminal's UI pass): a part given a form width starts its content
/// at the inset, so in stacked bars the icons stand in one column and the
/// labels in another (Juliet, 15 Oct)
#[test]
fn stacked_form_bars_keep_icons_in_one_column_and_labels_in_another() {
    let renderer = renderer();
    let icon =
        || cosmic::widget::icon::from_svg_bytes(&b"<svg xmlns='http://www.w3.org/2000/svg'/>"[..]);
    let first: Vec<(&str, bool)> = vec![("Session", true), ("Codex", true), ("Text", true)];
    let second: Vec<(&str, bool)> = vec![("Conversation", false), ("Terminal", true)];
    let part = form_part(first.iter().chain(second.iter()).copied(), 3, None);
    assert!(part.icons);
    let build = |parts: &[(&str, bool)]| -> Element<'static, u8> {
        Segmented::new(
            parts.iter().map(|(label, has)| {
                let p = Part::new(*label).on_press(0);
                if *has { p.icon(icon()) } else { p }
            }),
            Some(0),
        )
        .form(part)
        .into()
    };
    let columns: Vec<Vec<Vec<f32>>> = [&first, &second]
        .iter()
        .map(|parts| content_columns(&layout_of(build(parts), &renderer)))
        .collect();
    let mut icons = Vec::new();
    let mut labels = Vec::new();
    for bar in &columns {
        let (mut x, widths) = (0.0, part.width);
        for (n, columns) in bar.iter().enumerate() {
            assert_eq!(columns.len(), 2, "an icon's room, then the label");
            // Relative to the part's own left edge
            let left = x;
            icons.push(columns[0] - left);
            labels.push(columns[1] - left);
            let _ = n;
            x += widths;
        }
    }
    for column in [&icons, &labels] {
        assert!(
            column
                .windows(2)
                .all(|pair| (pair[0] - pair[1]).abs() < 0.01),
            "{column:?}"
        );
    }
    let inset = ButtonLook::Segment.inset(&theme::active().cosmic().spacing);
    assert!(
        (icons[0] - inset).abs() < 0.01,
        "from the inset: {}",
        icons[0]
    );
}

/// A part may carry a coloured element in place of an icon handle; it is
/// measured as a button's leading element is, and in a form it stands in the
/// icons' column
#[test]
fn a_part_can_lead_with_its_own_element() {
    let renderer = renderer();
    let dot = || -> Element<'static, u8> {
        widget::Space::new()
            .width(Length::Fixed(16.0))
            .height(Length::Fixed(16.0))
            .into()
    };
    let inset = ButtonLook::Segment.inset(&theme::active().cosmic().spacing);
    let with = layout_of(
        Segmented::new([Part::new("Mac").leading(dot()).on_press(0)], None).into(),
        &renderer,
    );
    let without = layout_of(
        Segmented::new([Part::new("Mac").on_press(0)], None).into(),
        &renderer,
    );
    let gap = f32::from(theme::active().cosmic().spacing.space_xxs);
    assert_eq!(
        with.size().width,
        (without.size().width - 2.0 * inset + 16.0 + gap + 2.0 * inset).ceil()
    );
    assert_eq!(content_columns(&with)[0][0], inset, "from the inset");
    // Measured as an icon's room, so a form lines it up with icon parts
    assert_eq!(
        Part::<u8>::new("Mac").leading(dot()).measure(),
        ("Mac", true)
    );
    let form = form_part([("Mac", true), ("Windows", false)], 2, None);
    let node = layout_of(
        Segmented::new(
            [
                Part::new("Mac").leading(dot()).on_press(0),
                Part::new("Windows").on_press(1),
            ],
            None,
        )
        .form(form)
        .into(),
        &renderer,
    );
    let columns = content_columns(&node);
    assert_eq!(columns[0][0], columns[1][0] - form.width, "icons' column");
    assert_eq!(columns[0][1], columns[1][1] - form.width, "labels' column");
}

/// A part's content box, relative to the part: where its content starts
/// and its width
fn content_in(node: &layout::Node, n: usize) -> (f32, f32) {
    let part = &node.children()[n];
    let content = part.children()[0].bounds();
    (content.x, content.width)
}

/// The offset of an end part's area's centre of mass from its box's middle,
/// by summing a pixel grid of the part's shape (a rectangle, an outer end
/// rounded with `radius`), 8 samples a pixel each way
fn sampled_offset(width: f32, height: f32, radius: f32) -> f32 {
    let r = radius.min(height / 2.0).min(width / 2.0);
    let n = 8.0;
    let (mut sum, mut count) = (0.0f64, 0.0f64);
    let mut y = 0.5 / n;
    while y < height {
        let mut x = 0.5 / n;
        while x < width {
            // A corner's centre is (r, r) from the top left, (r, height - r) at the bottom
            let cy = if y < r {
                r
            } else if y > height - r {
                height - r
            } else {
                y
            };
            let inside = x >= r || (x - r).powi(2) + (y - cy).powi(2) <= r * r;
            if inside {
                sum += f64::from(x);
                count += 1.0;
            }
            x += 1.0 / n;
        }
        y += 1.0 / n;
    }
    (sum / count) as f32 - width / 2.0
}

#[test]
fn an_end_parts_centroid_is_towards_its_divider_as_the_geometry_says() {
    use super::end_centroid_offset;
    for (width, height) in [
        (48.0, 26.0),
        (60.0, 26.0),
        (100.0, 26.0),
        (48.0, 32.0),
        (80.0, 32.0),
    ] {
        let offset = end_centroid_offset(width, height, height / 2.0);
        let sampled = sampled_offset(width, height, height / 2.0);
        assert!(
            (offset - sampled).abs() < 0.02,
            "{width} x {height}: {offset} against {sampled}"
        );
        assert!(offset > 0.0);
    }
    // A rectangle (half circle) and half disc, by the textbook formula
    let (w, h) = (60.0f32, 26.0f32);
    let r = h / 2.0;
    let (rect, disc) = ((w - r) * h, std::f32::consts::PI * r * r / 2.0);
    let centroid = (rect * (r + w) / 2.0 + disc * (r - 4.0 * r / (3.0 * std::f32::consts::PI)))
        / (rect + disc);
    assert!((end_centroid_offset(w, h, r) - (centroid - w / 2.0)).abs() < 1e-3);
    // Where a pixel-high figure was measured: about 1.5 at 32, less at 26
    assert!((1.4..1.7).contains(&end_centroid_offset(48.0, 32.0, 16.0)));
    assert!((1.2..1.4).contains(&end_centroid_offset(48.0, 26.0, 13.0)));
    assert_eq!(
        end_centroid_offset(48.0, 26.0, 0.0),
        0.0,
        "square: no shift"
    );
}

fn icon_part<'a>(n: u8) -> Part<'a, u8> {
    let mark: Element<'a, u8> =
        cosmic::widget::container(cosmic::widget::Space::new().width(16).height(16))
            .style(|_| cosmic::widget::container::Style {
                background: Some(Color::from_rgb(1.0, 0.0, 0.0).into()),
                ..Default::default()
            })
            .into();
    Part::new("").leading(mark).on_press(n)
}

fn icon_bar<'a>(count: u8) -> Segmented<'a, u8> {
    Segmented::new((0..count).map(icon_part), Some(1))
}

#[test]
fn an_icon_only_bar_has_its_icons_on_each_parts_shape_centroid() {
    let renderer = renderer();
    let node = layout_of(icon_bar(3).into(), &renderer);
    let (size, parts) = parts_of(&node);
    assert_eq!(size.height, 26.0);
    for part in &parts {
        assert_eq!(
            part.width, 48.0,
            "an icon and its inset, no gap for a label"
        );
        assert_eq!(part.x.fract(), 0.0);
    }
    let pixels = drawn(icon_bar(3).into());
    let ink = |part: &Rectangle| {
        let (mut sum, mut count) = (0.0, 0.0);
        for y in 0..48 {
            for x in (part.x as usize)..((part.x + part.width) as usize) {
                let p = pixels[y * 240 + x];
                if p[0] > 200 && p[1] < 60 && p[2] < 60 {
                    sum += x as f32 + 0.5;
                    count += 1.0;
                }
            }
        }
        assert!(count > 200.0, "the icon is drawn");
        sum / count
    };
    let offset = super::end_centroid_offset(48.0, 26.0, 13.0);
    for (part, shift) in parts.iter().zip([offset, 0.0, -offset]) {
        let target = part.x + part.width / 2.0 + shift;
        let centre = ink(part);
        assert!(
            (centre - target).abs() <= 0.5,
            "ink at {centre}, the shape's centroid at {target}"
        );
    }
}

#[test]
fn equal_parts_do_not_drift_and_middle_parts_are_unchanged() {
    let renderer = renderer();
    let node = layout_of(icon_bar(5).into(), &renderer);
    let (_, parts) = parts_of(&node);
    let offsets: Vec<f32> = (0..5).map(|n| content_in(&node, n).0).collect();
    // Middle parts: the content is where centring in the box puts it
    for n in 1..4 {
        assert_eq!(offsets[n], 16.0, "part {n}");
        assert_eq!(parts[n].width, 48.0);
        assert_eq!(
            parts[n].x,
            48.0 * n as f32,
            "whole pixels, no accumulating rounding"
        );
    }
    // The ends are shifted inwards by the same whole amount, mirrored
    assert_eq!(offsets[0], 17.0);
    assert_eq!(offsets[4], 15.0);
    // Labelled equal parts: the same, whatever their position
    let node = layout_of(
        bar(&["Same", "Same", "Same", "Same"], Some(0), true).into(),
        &renderer,
    );
    let (_, parts) = parts_of(&node);
    let (x1, w1) = content_in(&node, 1);
    let (x2, w2) = content_in(&node, 2);
    assert_eq!((x1, w1), (x2, w2));
    assert_eq!(parts[1].width, parts[2].width);
    for part in &parts {
        assert_eq!(part.x.fract(), 0.0);
        assert_eq!(part.width.fract(), 0.0);
    }
    let (x0, _) = content_in(&node, 0);
    let (x3, _) = content_in(&node, 3);
    assert_eq!(x0 - x1, -(x3 - x1), "mirrored ends");
    assert!(x0 > x1);
}

#[test]
fn a_form_bar_keeps_its_content_columns() {
    let renderer = renderer();
    let part = FormPart {
        width: 90.0,
        icons: false,
    };
    let node = layout_of(
        bar(&["Alpha", "Beta", "Gamma"], Some(0), true)
            .form(part)
            .into(),
        &renderer,
    );
    for n in 0..3 {
        assert_eq!(
            content_in(&node, n).0,
            16.0,
            "content starts at the inset, part {n}"
        );
    }
}

/// `element` drawn at 1:1 on `ground` in the light theme, cropped to `size`
fn picture(element: Element<'static, u8>, size: Size<u32>, ground: Color) -> Vec<[u8; 4]> {
    use cosmic::iced::advanced::renderer::Style;
    use cosmic::iced::mouse;
    use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
    let mut renderer = renderer();
    let bounds = Size::new(size.width as f32, size.height as f32);
    let mut ui = UserInterface::build(element, bounds, Cache::default(), &mut renderer);
    ui.draw(
        &mut renderer,
        &cosmic::Theme::light(),
        &Style::default(),
        mouse::Cursor::Unavailable,
    );
    renderer
        .screenshot(size, 1.0, ground)
        .as_chunks::<4>()
        .0
        .to_vec()
}

/// Prints the pictures of `toggles-26-built.png`, one `PIECE:name:width:height:hex`
/// line each, for the script that lays them out (`--ignored --nocapture`)
#[test]
#[ignore = "writes pictures for a person to look at"]
fn render_the_toggle_sheet() {
    use crate::button::GEOMETRIC_CENTRING;
    let ground = Color::from_rgb(0.96, 0.96, 0.96);
    let piece = |name: &str, element: Element<'static, u8>, size: Size<u32>| {
        let hex: String = picture(element, size, ground)
            .iter()
            .map(|p| format!("{:02x}{:02x}{:02x}", p[0], p[1], p[2]))
            .collect();
        println!("PIECE:{name}:{}:{}:{hex}", size.width, size.height);
    };
    let labelled = || -> Element<'static, u8> { bar(&["Dark", "Light"], Some(0), true).into() };
    piece("labelled", labelled(), Size::new(130, 26));
    piece(
        "switch-on",
        crate::switch::<u8>(true, |_| 0),
        Size::new(52, 26),
    );
    piece(
        "switch-off",
        crate::switch::<u8>(false, |_| 0),
        Size::new(52, 26),
    );
    piece(
        "button",
        Button::new(ButtonLook::Secondary, "Secondary")
            .on_press(0u8)
            .into(),
        Size::new(120, 32),
    );
    GEOMETRIC_CENTRING.with(|c| c.set(true));
    piece("icons-before", icon_bar(3).into(), Size::new(144, 26));
    GEOMETRIC_CENTRING.with(|c| c.set(false));
    piece("icons-after", icon_bar(3).into(), Size::new(144, 26));
}
