// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use cosmic::iced::advanced::layout;
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::advanced::widget::Tree;
use cosmic::iced::{Color, Point, Rectangle, Size};

fn renderer() -> cosmic::Renderer {
    crate::test_fonts::renderer().expect("native renderer")
}

/// The looks a split button supports
const LOOKS: [ButtonLook; 6] = [
    ButtonLook::Primary,
    ButtonLook::Secondary,
    ButtonLook::Tertiary,
    ButtonLook::Destructive,
    ButtonLook::TertiaryDestructive,
    ButtonLook::Plain,
];

const LABEL: &str = "New terminal session";

fn split_of(look: ButtonLook, open: bool) -> SplitButton<'static, u8, u8> {
    SplitButton::new(look, LABEL, "Choose a machine", 0u8, widget::Id::new("split"))
        .on_press(1)
        .open(open)
}

fn split(look: ButtonLook, open: bool) -> Element<'static, u8> {
    split_of(look, open).into()
}

fn ordinary(look: ButtonLook) -> Element<'static, u8> {
    Button::new(look, LABEL).on_press(1).into()
}

fn menu_alone(look: ButtonLook) -> Element<'static, u8> {
    Button::new(look, "Choose a machine")
        .compact(true)
        .leading_icon(widget::icon::from_name("pan-down-symbolic").handle())
        .on_press(1)
        .into()
}

fn layout_in(mut element: Element<'_, u8>, renderer: &cosmic::Renderer, width: f32) -> layout::Node {
    let mut tree = Tree::new(element.as_widget());
    element.as_widget_mut().layout(
        &mut tree,
        renderer,
        &layout::Limits::new(Size::ZERO, Size::new(width, 200.0)),
    )
}

fn layout_of(element: Element<'_, u8>, renderer: &cosmic::Renderer) -> layout::Node {
    layout_in(element, renderer, 900.0)
}

/// Every box of a layout, relative to the node's own origin
fn boxes(node: &layout::Node, origin: Point, out: &mut Vec<(f32, f32, f32, f32)>) {
    let at = node.bounds();
    out.push((at.x - origin.x, at.y - origin.y, at.width, at.height));
    for child in node.children() {
        boxes(child, origin, out);
    }
}

/// Every box of `node`'s subtree, relative to the node's own origin (a
/// child's bounds are relative to its parent, so the origin is the node's)
fn boxes_of(node: &layout::Node) -> Vec<(f32, f32, f32, f32)> {
    let mut out = Vec::new();
    let at = node.bounds();
    out.push((0.0, 0.0, at.width, at.height));
    for child in node.children() {
        boxes(child, Point::ORIGIN, &mut out);
    }
    out
}

const W: u32 = 260;
const H: u32 = 48;

fn drawn_at(element: Element<'static, u8>, cursor: Option<Point>) -> Vec<[u8; 4]> {
    drawn_on(element, cursor, Color::from_rgb(0.5, 0.5, 0.5))
}

fn drawn_on(element: Element<'static, u8>, cursor: Option<Point>, ground: Color) -> Vec<[u8; 4]> {
    use cosmic::iced::advanced::renderer::Style;
    use cosmic::iced::mouse;
    use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
    use crate::window::{UiEvent, WindowUi};
    let mut renderer = renderer();
    // In a window, as the menu part needs: its root is the window's
    let window = WindowUi::<u8>::new();
    let element: Element<'_, UiEvent<u8, u8>> = window.mount(element, Vec::new()).into();
    let mut ui = UserInterface::build(element, Size::new(W as f32, H as f32), Cache::default(), &mut renderer);
    let cursor = cursor.map_or(mouse::Cursor::Unavailable, mouse::Cursor::Available);
    if let mouse::Cursor::Available(position) = cursor {
        // A button knows it is hovered from the move, not from the cursor at draw
        let _ = ui.update(
            &[cosmic::iced::Event::Mouse(mouse::Event::CursorMoved { position })],
            cursor,
            &mut renderer,
            &mut cosmic::iced::advanced::clipboard::Null,
            &mut Vec::new(),
        );
    }
    ui.draw(&mut renderer, &cosmic::Theme::default(), &Style::default(), cursor);
    renderer
        .screenshot(Size::new(W, H), 1.0, ground)
        .as_chunks::<4>()
        .0
        .to_vec()
}

fn drawn(element: Element<'static, u8>) -> Vec<[u8; 4]> {
    drawn_at(element, None)
}

/// Gives the keyboard's focus to the `at`-th focusable widget
struct FocusNth {
    at: usize,
    seen: usize,
}

impl cosmic::iced::advanced::widget::Operation for FocusNth {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn cosmic::iced::advanced::widget::Operation)) {
        visit(self);
    }

    fn focusable(
        &mut self,
        _: Option<&widget::Id>,
        _: Rectangle,
        focus: &mut dyn cosmic::iced::advanced::widget::operation::Focusable,
    ) {
        if self.seen == self.at {
            focus.focus();
        }
        self.seen += 1;
    }
}

/// `drawn`, with the keyboard's focus on part `part` (0 the action, 1 the menu)
fn drawn_focused(element: Element<'static, u8>, part: usize) -> Vec<[u8; 4]> {
    use cosmic::iced::advanced::renderer::Style;
    use cosmic::iced::mouse;
    use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
    use crate::window::{UiEvent, WindowUi};
    let mut renderer = renderer();
    let window = WindowUi::<u8>::new();
    let element: Element<'_, UiEvent<u8, u8>> = window.mount(element, Vec::new()).into();
    let mut ui = UserInterface::build(element, Size::new(W as f32, H as f32), Cache::default(), &mut renderer);
    ui.operate(&renderer, &mut FocusNth { at: part, seen: 0 });
    let _ = ui.update(
        &[cosmic::iced::Event::Window(cosmic::iced::window::Event::RedrawRequested(std::time::Instant::now()))],
        mouse::Cursor::Unavailable,
        &mut renderer,
        &mut cosmic::iced::advanced::clipboard::Null,
        &mut Vec::new(),
    );
    ui.draw(&mut renderer, &cosmic::Theme::default(), &Style::default(), mouse::Cursor::Unavailable);
    renderer
        .screenshot(Size::new(W, H), 1.0, Color::from_rgb(0.5, 0.5, 0.5))
        .as_chunks::<4>()
        .0
        .to_vec()
}

fn pixel(pixels: &[[u8; 4]], x: f32, y: f32) -> [u8; 4] {
    pixels[(y as usize) * W as usize + x as usize]
}

fn ordinary_width(renderer: &cosmic::Renderer, look: ButtonLook) -> f32 {
    layout_of(ordinary(look), renderer).size().width
}

/// A plain button of `look`, `width` wide
fn plain(look: ButtonLook, width: f32) -> Element<'static, u8> {
    Button::new(look, LABEL).width(Length::Fixed(width)).content_start(true).on_press(1).into()
}

/// Each look: as tall as its button, the menu part the `space_l` square
/// touching the action part, the action part exactly the ordinary button's
/// box with its label where a button's is
#[test]
fn each_look_is_as_tall_as_its_button_with_the_ordinary_buttons_action_part() {
    let renderer = renderer();
    let space_l = crate::button::button_height();
    for look in LOOKS {
        let node = layout_of(split(look, false), &renderer);
        let alone = layout_of(ordinary(look), &renderer);
        let action = &node.children()[0];
        let menu = &node.children()[1];
        assert_eq!(node.size().height, alone.size().height, "{look:?}");
        assert_eq!(node.size().height, space_l, "{look:?}");
        assert_eq!(menu.size(), Size::new(space_l, space_l), "{look:?}");
        assert_eq!(menu.bounds().x, action.bounds().x + action.size().width, "{look:?}: no gap");
        assert_eq!(node.size().width, alone.size().width + space_l, "{look:?}");
        assert_eq!(boxes_of(action), boxes_of(&alone), "{look:?}: the label is where a button's is");
        let square = layout_of(menu_alone(look), &renderer);
        assert_eq!(boxes_of(menu), boxes_of(&square), "{look:?}: the menu part is a compact button");
    }
}

/// With `.width(Fill)` the action part fills and the menu part stays the square
#[test]
fn a_filling_button_fills_in_its_action_part() {
    let renderer = renderer();
    let space_l = crate::button::button_height();
    for look in LOOKS {
        let node = layout_in(split_of(look, false).width(Length::Fill).into(), &renderer, 400.0);
        assert_eq!(node.size().width, 400.0, "{look:?}");
        assert_eq!(node.children()[1].size().width, space_l, "{look:?}");
        assert_eq!(node.children()[0].size().width, 400.0 - space_l, "{look:?}");
        let filling = layout_in(
            Button::new(look, LABEL).width(Length::Fill).on_press(1).into(),
            &renderer,
            400.0 - space_l,
        );
        assert_eq!(boxes_of(&node.children()[0]), boxes_of(&filling), "{look:?}");
    }
}

/// Where the pixels of a split button of `look` may differ from a plain
/// button of the same look and width: the divider's columns and the chevron
fn differs_only_at_divider_and_chevron(look: ButtonLook, divider: f32) {
    let renderer = renderer();
    let space_l = crate::button::button_height();
    let n = ordinary_width(&renderer, look);
    let total = n + space_l;
    let pair = drawn(split(look, false));
    let alone = drawn(plain(look, total));
    let (cx, cy) = (n + space_l / 2.0, space_l / 2.0);
    for y in 0..H {
        for x in 0..W {
            let (xf, yf) = (x as f32, y as f32);
            let in_divider = xf >= n - divider && xf < n + divider;
            let in_chevron = (xf - cx).abs() <= 8.0 && (yf - cy).abs() <= 8.0;
            if in_divider || in_chevron {
                continue;
            }
            assert_eq!(pixel(&pair, xf, yf), pixel(&alone, xf, yf), "{look:?} pixel ({x}, {y})");
        }
    }
    // and the chevron and divider are drawn
    assert_ne!(pixel(&pair, n, cy), pixel(&alone, n, cy), "{look:?}: a divider");
    assert!((0..16).any(|d| (0..16).any(|e| {
        let (x, y) = (cx - 8.0 + d as f32, cy - 8.0 + e as f32);
        pixel(&pair, x, y) != pixel(&alone, x, y)
    })), "{look:?}: a chevron");
}

/// A Secondary split button is a plain Secondary button's pixels (outline,
/// label, background) but for the divider column and the chevron
#[test]
fn secondary_is_the_ordinary_buttons_pixels_but_for_the_divider_and_chevron() {
    differs_only_at_divider_and_chevron(ButtonLook::Secondary, 1.0);
    let renderer = renderer();
    let n = ordinary_width(&renderer, ButtonLook::Secondary);
    let pair = drawn(split(ButtonLook::Secondary, false));
    // The divider is the 2 px border colour, the full height, joining the outline
    let border = pixel(&pair, (n / 2.0).floor(), 0.0);
    for y in 0..32 {
        assert_eq!(pixel(&pair, n - 1.0, y as f32), border, "divider at y {y}");
        assert_eq!(pixel(&pair, n, y as f32), border, "divider at y {y}");
    }
    assert_ne!(pixel(&pair, n - 2.0, 16.0), border, "and no wider");
    assert_ne!(pixel(&pair, n + 1.0, 16.0), border, "and no wider");
}

/// Primary likewise, for the fill
#[test]
fn primary_is_the_ordinary_buttons_pixels_but_for_the_divider_and_chevron() {
    differs_only_at_divider_and_chevron(ButtonLook::Primary, 1.0);
    // the divider: 1 px, on-accent, inset 6 px at the top and bottom
    let renderer = renderer();
    let n = ordinary_width(&renderer, ButtonLook::Primary);
    let pair = drawn(split(ButtonLook::Primary, false));
    let fill = pixel(&pair, 8.0, 16.0);
    let on = pixel(&pair, n, 16.0);
    assert_ne!(on, fill);
    assert_eq!(pixel(&pair, n, 6.0), on);
    assert_eq!(pixel(&pair, n, 25.0), on);
    assert_eq!(pixel(&pair, n, 5.0), fill, "inset 6 px at the top");
    assert_eq!(pixel(&pair, n, 26.0), fill, "and at the bottom");
    assert_eq!(pixel(&pair, n - 1.0, 16.0), fill, "1 px");
    assert_eq!(pixel(&pair, n + 1.0, 16.0), fill, "1 px");
}

/// Tertiary has no box at rest: only a short 1 px line and the chevron
#[test]
fn tertiary_is_the_ordinary_buttons_pixels_but_for_a_short_divider_and_chevron() {
    differs_only_at_divider_and_chevron(ButtonLook::Tertiary, 1.0);
    let renderer = renderer();
    let n = ordinary_width(&renderer, ButtonLook::Tertiary);
    let pair = drawn(split(ButtonLook::Tertiary, false));
    let ground = pixel(&pair, 2.0, 2.0);
    assert_ne!(pixel(&pair, n, 16.0), ground);
    assert_eq!(pixel(&pair, n, 5.0), ground, "short: inset 6 px");
    assert_eq!(pixel(&pair, n, 26.0), ground);
    assert_eq!(pixel(&pair, n + 1.0, 16.0), ground, "1 px");
}

/// The pointer on the action part lights that part only, as the ordinary
/// button's hover; on the menu part it lights the whole button, round at
/// both ends
#[test]
fn hover_lights_the_action_part_alone_or_the_whole_button() {
    let renderer = renderer();
    let space_l = crate::button::button_height();
    for look in [ButtonLook::Secondary, ButtonLook::Primary, ButtonLook::Tertiary] {
        let n = ordinary_width(&renderer, look);
        let total = n + space_l;
        let rest = drawn(split(look, false));
        let lit = drawn_at(plain(look, total), Some(Point::new(8.0, 16.0)));
        let on_action = drawn_at(split(look, false), Some(Point::new(8.0, 16.0)));
        let on_menu = drawn_at(split(look, false), Some(Point::new(n + 20.0, 16.0)));
        // Points inside the action part, near each end, and in the menu part
        let left = (6.0, 16.0);
        let by_divider = (n - 4.0, 4.0);
        let in_menu = (n + 5.0, 4.0);
        let right_end = (total - 4.0, 16.0);
        for (name, (x, y)) in [("left", left), ("by the divider", by_divider)] {
            assert_ne!(pixel(&on_action, x, y), pixel(&rest, x, y), "{look:?} {name}: lit");
            assert_eq!(pixel(&on_action, x, y), pixel(&lit, x, y), "{look:?} {name}: the look's hover");
        }
        for (name, (x, y)) in [("in the menu part", in_menu), ("right end", right_end)] {
            assert_eq!(pixel(&on_action, x, y), pixel(&rest, x, y), "{look:?} {name}: not lit");
            assert_ne!(pixel(&on_menu, x, y), pixel(&rest, x, y), "{look:?} {name}: lit");
            assert_eq!(pixel(&on_menu, x, y), pixel(&lit, x, y), "{look:?} {name}: the look's hover");
        }
        for (name, (x, y)) in [("left", left), ("by the divider", by_divider)] {
            assert_eq!(pixel(&on_menu, x, y), pixel(&lit, x, y), "{look:?} {name}: the whole button");
        }
        // The action part's hover is square at the divider and round at the outer end
        assert_eq!(pixel(&on_action, 0.0, 0.0), pixel(&rest, 0.0, 0.0), "{look:?}: round at the outer end");
        assert_eq!(pixel(&on_menu, total - 1.0, 0.0), pixel(&rest, total - 1.0, 0.0), "{look:?}: round at the far end");
    }
}

/// While the menu is open the whole button is shown pressed
#[test]
fn the_whole_button_is_pressed_while_the_menu_is_open() {
    let renderer = renderer();
    for look in [ButtonLook::Primary, ButtonLook::Secondary, ButtonLook::Tertiary] {
        let n = ordinary_width(&renderer, look);
        let rest = drawn(split(look, false));
        let open = drawn(split(look, true));
        let hovered = drawn_at(split(look, false), Some(Point::new(n + 20.0, 16.0)));
        for (x, y) in [(6.0, 16.0), (n - 4.0, 4.0), (n + 5.0, 4.0), (n + 28.0, 16.0)] {
            assert_ne!(pixel(&open, x, y), pixel(&rest, x, y), "{look:?} ({x}, {y})");
        }
        if look == ButtonLook::Primary {
            assert_ne!(pixel(&open, 6.0, 16.0), pixel(&hovered, 6.0, 16.0), "pressed, not hovered");
            assert_eq!(pixel(&open, 6.0, 16.0), pixel(&open, n + 5.0, 4.0), "one fill");
        }
    }
}

/// Without a message the action part shows the look's fade, as a disabled
/// button does; the menu part stays live
#[test]
fn a_disabled_action_part_is_faded_and_the_menu_part_is_not() {
    let renderer = renderer();
    let space_l = crate::button::button_height();
    for look in [ButtonLook::Primary, ButtonLook::Secondary, ButtonLook::Tertiary] {
        let n = ordinary_width(&renderer, look);
        let total = n + space_l;
        let split: Element<'static, u8> =
            SplitButton::new(look, LABEL, "Choose a machine", 0u8, widget::Id::new("split")).into();
        let pair = drawn(split);
        let disabled: Element<'static, u8> = Button::new(look, LABEL).width(Length::Fixed(total)).content_start(true).into();
        let disabled = drawn(disabled);
        let enabled = drawn(split_of(look, false).into());
        // Everything left of the divider is the disabled button's
        for y in 0..space_l as u32 {
            for x in 4..(n - 2.0) as u32 {
                if look == ButtonLook::Secondary && !(2..30).contains(&y) {
                    continue; // the outline is drawn in the action part's own state, checked below
                }
                assert_eq!(
                    pixel(&pair, x as f32, y as f32),
                    pixel(&disabled, x as f32, y as f32),
                    "{look:?} ({x}, {y})"
                );
            }
        }
        assert_ne!(pixel(&pair, 40.0, 16.0), pixel(&enabled, 40.0, 16.0), "{look:?}: faded");
        // The menu part is as when the action is enabled
        assert_eq!(pixel(&pair, total - 6.0, 16.0), pixel(&enabled, total - 6.0, 16.0), "{look:?}: menu live");
        assert_eq!(pixel(&pair, n + 5.0, 2.0), pixel(&enabled, n + 5.0, 2.0), "{look:?}: menu live");
    }
}

/// The keyboard-target outline is drawn inside the focused part's own edge
/// and nowhere in the other part
#[test]
fn focus_is_drawn_inside_the_focused_part_only() {
    let renderer = renderer();
    let space_l = crate::button::button_height();
    for look in [ButtonLook::Primary, ButtonLook::Secondary, ButtonLook::Tertiary] {
        let n = ordinary_width(&renderer, look);
        let rest = drawn(split(look, false));
        for part in [0usize, 1] {
            let focused = drawn_focused(split(look, false), part);
            let mut changed = Vec::new();
            for y in 0..H {
                for x in 0..W {
                    if pixel(&focused, x as f32, y as f32) != pixel(&rest, x as f32, y as f32) {
                        changed.push((x as f32, y as f32));
                    }
                }
            }
            assert!(!changed.is_empty(), "{look:?} part {part}: a focus outline");
            let (from, to) = if part == 0 { (0.0, n) } else { (n, n + space_l) };
            for (x, y) in changed {
                assert!(
                    x >= from && x < to && y < space_l,
                    "{look:?} part {part}: pixel ({x}, {y}) outside the part"
                );
            }
        }
    }
}

/// Renders the three looks in rest, action hovered and menu hovered, from
/// the real widget, as a PPM printed in hex after `SHEET:` (run with
/// `--ignored --nocapture`); it asserts nothing
#[test]
#[ignore = "writes a picture for a person to look at"]
fn render_the_sheet() {
    let renderer = renderer();
    let ground = Color::from_rgb(0.84, 0.84, 0.84);
    let (cell_w, cell_h) = (230usize, 44usize);
    let mut sheet = vec![[214u8, 214, 214, 255]; cell_w * 3 * cell_h * 3];
    for (column, look) in [ButtonLook::Primary, ButtonLook::Secondary, ButtonLook::Tertiary].into_iter().enumerate() {
        let n = layout_of(Button::new(look, "New session").on_press(1).into(), &renderer).size().width;
        for (row, at) in [None, Some(Point::new(20.0, 16.0)), Some(Point::new(n + 16.0, 16.0))].into_iter().enumerate() {
            let element: Element<'static, u8> =
                SplitButton::new(look, "New session", "Choose a machine", 0u8, widget::Id::new("split"))
                    .on_press(1)
                    .into();
            let picture = drawn_on(element, at, ground);
            for y in 0..32usize {
                for x in 0..cell_w.min(W as usize) {
                    sheet[(row * cell_h + y + 6) * cell_w * 3 + column * cell_w + x + 10] = picture[y * W as usize + x];
                }
            }
        }
    }
    for (y, row) in sheet.chunks(cell_w * 3).enumerate() {
        let hex: String = row.iter().map(|p| format!("{:02x}{:02x}{:02x}", p[0], p[1], p[2])).collect();
        println!("SHEET:{y}:{hex}");
    }
}

mod chain {
    use super::*;
    use crate::window::{UiEvent, WindowUi};
    use cosmic::iced::advanced::widget::operation;
    use cosmic::iced::runtime::{UserInterface, user_interface};
    use crate::layout::{Attachment, Level, Stage, child};
    use crate::popup::Panel;
    use cosmic::iced::{Event, Rectangle, keyboard, mouse, window as native_window};
    use std::time::Instant;

    type Ev = UiEvent<u8, u8>;

    struct App {
        window: WindowUi<u8>,
        renderer: cosmic::Renderer,
        cache: Option<user_interface::Cache>,
        cursor: mouse::Cursor,
        app: Vec<u8>,
    }

    /// The app's panel for the open root, anchored at the menu part
    fn panels(window: &WindowUi<u8>) -> Vec<Panel<'_, Ev>> {
        if window.chain().is_none() {
            return Vec::new();
        }
        vec![
            Panel::new(
                Attachment::below(Rectangle::default()),
                vec![Stage::new(
                    Level::Preferred,
                    child(|_| cosmic::iced::widget::text("Machines").into()),
                )],
            )
            .source("split"),
        ]
    }

    fn view(window: &WindowUi<u8>) -> Element<'_, Ev> {
        let content: Element<'_, u8> = split(ButtonLook::Secondary, window.chain().is_some());
        window.mount(content, panels(window)).into()
    }

    impl App {
        fn new() -> Self {
            let mut app = Self {
                window: WindowUi::new(),
                renderer: renderer(),
                cache: None,
                cursor: mouse::Cursor::Unavailable,
                app: Vec::new(),
            };
            app.frame();
            app
        }

        fn send(&mut self, events: &[Event]) {
            let mut published = Vec::new();
            {
                let root = view(&self.window);
                let cache = self.cache.take().unwrap_or_default();
                let mut ui = UserInterface::build(root, Size::new(400.0, 200.0), cache, &mut self.renderer);
                for event in events {
                    let _ = ui.update(
                        std::slice::from_ref(event),
                        self.cursor,
                        &mut self.renderer,
                        &mut cosmic::iced::advanced::clipboard::Null,
                        &mut published,
                    );
                }
                self.cache = Some(ui.into_cache());
            }
            for event in published {
                match event {
                    UiEvent::Internal(action) => self.window.apply(action),
                    UiEvent::Application(message) => self.app.push(message),
                }
            }
        }

        fn frame(&mut self) {
            for _ in 0..2 {
                self.send(&[Event::Window(native_window::Event::RedrawRequested(Instant::now()))]);
            }
        }

        fn click(&mut self, at: Point) {
            self.cursor = mouse::Cursor::Available(at);
            self.send(&[Event::Mouse(mouse::Event::CursorMoved { position: at })]);
            self.send(&[Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))]);
            self.send(&[Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))]);
            self.frame();
        }

        fn focus(&mut self, id: widget::Id) {
            let root = view(&self.window);
            let cache = self.cache.take().unwrap_or_default();
            let mut ui = UserInterface::build(root, Size::new(400.0, 200.0), cache, &mut self.renderer);
            ui.operate(&self.renderer, &mut operation::focusable::focus::<()>(id));
            self.cache = Some(ui.into_cache());
            self.frame();
        }

        fn enter(&mut self) {
            let key = keyboard::Key::Named(keyboard::key::Named::Enter);
            self.send(&[Event::Keyboard(keyboard::Event::KeyPressed {
                modified_key: key.clone(),
                key,
                modifiers: keyboard::Modifiers::empty(),
                physical_key: keyboard::key::Physical::Unidentified(
                    keyboard::key::NativeCode::Unidentified,
                ),
                location: keyboard::Location::Standard,
                text: None,
                repeat: false,
            })]);
            self.frame();
        }

        fn open(&self) -> Option<u8> {
            self.window.chain().map(|chain| *chain.root())
        }
    }

    fn menu_centre() -> Point {
        let renderer = renderer();
        let node = layout_of(split(ButtonLook::Secondary, false), &renderer);
        let menu = node.children()[1].bounds();
        Point::new(menu.x + menu.width / 2.0, menu.y + menu.height / 2.0)
    }

    /// A press on the menu part opens its root through the window's chain, a
    /// press on it again closes it, and the action part publishes its own message
    #[test]
    fn pressing_the_menu_part_opens_its_root_and_the_action_part_is_the_action() {
        let mut app = App::new();
        assert_eq!(app.open(), None);
        app.click(menu_centre());
        assert_eq!(app.open(), Some(0));
        app.click(menu_centre());
        assert_eq!(app.open(), None);
        assert!(app.app.is_empty());
        app.click(Point::new(20.0, 16.0));
        assert_eq!(app.app, vec![1]);
        assert_eq!(app.open(), None);
    }

    /// Focused, the menu part opens its root from the keyboard
    #[test]
    fn the_focused_menu_part_opens_its_root_on_enter() {
        let mut app = App::new();
        app.focus(widget::Id::new("split"));
        app.enter();
        assert_eq!(app.open(), Some(0));
    }
}
