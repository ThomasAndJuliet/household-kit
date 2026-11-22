// SPDX-License-Identifier: GPL-3.0-only

//! The dropdown's trigger and a menu bar's title are the shared button's
//! Plain look (Juliet, 22 Nov 2026): drawn here, headless, with the test
//! fonts. At rest they draw only their label (and the trigger's caret): no
//! fill, no border. Under the pointer the Plain wash is inside the pill, the
//! keyboard-target outline is inside the edge, and a title's text is in the
//! header's colour.
//!
//! [`sheet`], run on request (`--ignored`), prints the pictures of the
//! controls at rest and hovered as the RGBA of each, base64, for a script to
//! assemble (a sandboxed build has no other way out).

use crate::dropdown::{DropdownOption, trigger, trigger_width};
use crate::menu_bar::title;
use crate::window::{Toggle, UiEvent, WindowUi};
use cosmic::iced::advanced::clipboard;
use cosmic::iced::advanced::renderer::Headless as _;
use cosmic::iced::advanced::renderer::Style;
use cosmic::iced::advanced::widget::{Id, Operation, operation};
use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
use cosmic::iced::{Color, Length, Point, Rectangle, Size, mouse};
use cosmic::widget;
use cosmic::{Element, Theme};

const W: u32 = 160;
const H: u32 = 48;
const GROUND: Color = Color::from_rgb(0.5, 0.5, 0.5);

fn the_trigger() -> Element<'static, ()> {
    // Measured in the test fonts, as drawn
    let _ = crate::test_fonts::renderer();
    let options = [DropdownOption {
        label: "Medium".into(),
        disabled_reason: None,
    }];
    trigger(
        "Medium".into(),
        Length::Fixed(trigger_width(&options)),
        None,
    )
    .map(|_| ())
}

fn body(text: String) -> Element<'static, Toggle> {
    widget::text(text).size(14).into()
}

fn a_title() -> Element<'static, ()> {
    a_title_open(false)
}

fn a_title_open(open: bool) -> Element<'static, ()> {
    // A title is a trigger of the window's chain, so it is mounted in one
    let window: &'static WindowUi<u8> = Box::leak(Box::new(WindowUi::new()));
    let title: Element<'static, ()> =
        title::<(), u8>(Id::new("t"), 0, "File".into(), open, false, body);
    let mounted: Element<'static, UiEvent<u8, ()>> = window.mount(title, Vec::new()).into();
    mounted.map(|_| ())
}

struct FocusFirst(bool);

impl Operation for FocusFirst {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }
    fn focusable(&mut self, _: Option<&Id>, _: Rectangle, focus: &mut dyn operation::Focusable) {
        if !self.0 {
            focus.focus();
            self.0 = true;
        }
    }
}

struct Picture {
    pixels: Vec<[u8; 4]>,
    bounds: Rectangle,
}

impl Picture {
    fn at(&self, x: f32, y: f32) -> [u8; 4] {
        self.pixels[(y as usize) * W as usize + x as usize]
    }
}

/// `element` centred in a mid-grey canvas, the pointer on `pointer` (a point
/// of the box, relative to it) if given, the keyboard's focus on it if
/// `focus`, and the header's colour `icon` (the body's is dark)
fn draw(
    element: Element<'static, ()>,
    pointer: Option<Point>,
    focus: bool,
    icon: Color,
    scale: f32,
) -> Picture {
    let mut renderer = crate::test_fonts::renderer().expect("native renderer");
    let canvas = Size::new(W as f32, H as f32);
    let centred: Element<'static, ()> = widget::container(element).center(Length::Fill).into();
    let mut ui = UserInterface::build(centred, canvas, Cache::default(), &mut renderer);
    let mut find = Bounds(None);
    ui.operate(&renderer, &mut find);
    let bounds = find.0.unwrap_or(Rectangle::with_size(canvas));
    let cursor = match pointer {
        Some(p) => mouse::Cursor::Available(Point::new(bounds.x + p.x, bounds.y + p.y)),
        None => mouse::Cursor::Available(Point::new(1.0, 1.0)),
    };
    if focus {
        ui.operate(&renderer, &mut FocusFirst(false));
    }
    let mut messages = Vec::new();
    let _ = ui.update(
        &[cosmic::iced::Event::Mouse(mouse::Event::CursorMoved {
            position: cursor.position().unwrap_or_default(),
        })],
        cursor,
        &mut renderer,
        &mut clipboard::Null,
        &mut messages,
    );
    ui.draw(
        &mut renderer,
        &Theme::default(),
        &Style {
            icon_color: icon,
            text_color: body_colour(),
            ..Default::default()
        },
        cursor,
    );
    let pixels = renderer.screenshot(
        Size::new((W as f32 * scale) as u32, (H as f32 * scale) as u32),
        scale,
        GROUND,
    );
    Picture {
        pixels: pixels.as_chunks::<4>().0.to_vec(),
        bounds,
    }
}

/// The bounds of the first focusable
struct Bounds(Option<Rectangle>);

impl Operation for Bounds {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }
    fn focusable(&mut self, _: Option<&Id>, bounds: Rectangle, _: &mut dyn operation::Focusable) {
        self.0.get_or_insert(bounds);
    }
}

fn is_near(pixel: [u8; 4], rgb: [u8; 3], slack: u8) -> bool {
    (0..3).all(|n| pixel[n].abs_diff(rgb[n]) <= slack)
}

/// The colour a window's body text is drawn in
fn body_colour() -> Color {
    Theme::default().cosmic().on_bg_color().into()
}

fn ground() -> [u8; 3] {
    [128, 128, 128]
}

/// Nothing but the label and the caret at rest: the box's top rows and its
/// left room, where neither is drawn, are the ground (no fill, no border),
/// and the box is `space_l` tall
fn at_rest_only_the_label_is_drawn(element: fn() -> Element<'static, ()>, what: &str) {
    let picture = draw(element(), None, false, Color::BLACK, 1.0);
    let b = picture.bounds;
    assert_eq!(b.height, 32.0, "{what}");
    for y in 0..4 {
        for x in 0..b.width as usize {
            let p = picture.at(b.x + x as f32, b.y + y as f32);
            assert!(is_near(p, ground(), 0), "{what}: {x},{y} is {p:?}");
        }
    }
    for y in 0..b.height as usize {
        for x in 0..14 {
            let p = picture.at(b.x + x as f32, b.y + y as f32);
            assert!(is_near(p, ground(), 0), "{what}: {x},{y} is {p:?}");
        }
    }
}

#[test]
fn the_trigger_at_rest_draws_only_its_label_and_caret() {
    at_rest_only_the_label_is_drawn(the_trigger, "trigger");
}

#[test]
fn a_title_at_rest_draws_only_its_label() {
    at_rest_only_the_label_is_drawn(a_title, "title");
}

/// The hover wash of the Plain look over `ground`, 8-bit
fn wash() -> [u8; 3] {
    let theme = Theme::default();
    let wash = crate::button::paint(
        crate::button::ButtonLook::Plain,
        false,
        None,
        crate::button::State::Hovered,
        &theme,
    )
    .background
    .expect("a wash");
    let mix = |w: f32, g: f32| ((w * wash.a + g * (1.0 - wash.a)) * 255.0).round() as u8;
    [mix(wash.r, 0.5), mix(wash.g, 0.5), mix(wash.b, 0.5)]
}

/// Under the pointer: the Plain wash fills the pill (a point inside its
/// left end), and its corner, outside the pill's curve, is the ground
fn hovered_is_the_wash_inside_the_pill(element: fn() -> Element<'static, ()>, what: &str) {
    let picture = draw(
        element(),
        Some(Point::new(20.0, 16.0)),
        false,
        Color::BLACK,
        1.0,
    );
    let b = picture.bounds;
    let inside = picture.at(b.x + 6.0, b.center_y());
    assert!(
        is_near(inside, wash(), 2),
        "{what}: {inside:?} want {:?}",
        wash()
    );
    let corner = picture.at(b.x, b.y);
    assert!(
        is_near(corner, ground(), 0),
        "{what}: the corner is outside the pill: {corner:?}"
    );
}

#[test]
fn the_trigger_under_the_pointer_is_the_plain_wash_in_its_pill() {
    hovered_is_the_wash_inside_the_pill(the_trigger, "trigger");
}

#[test]
fn a_title_under_the_pointer_is_the_plain_wash_in_its_pill() {
    hovered_is_the_wash_inside_the_pill(a_title, "title");
}

/// While its menu is open a title keeps the wash, the pointer elsewhere
#[test]
fn an_open_titles_wash_is_kept_with_the_pointer_away() {
    let picture = draw(a_title_open(true), None, false, Color::BLACK, 1.0);
    let b = picture.bounds;
    let inside = picture.at(b.x + 6.0, b.center_y());
    assert!(is_near(inside, wash(), 2), "{inside:?} want {:?}", wash());
}

/// Focused: the household's keyboard-target outline on the box's edge,
/// inside it, nothing outside
fn focus_is_the_outline_inside_the_edge(element: fn() -> Element<'static, ()>, what: &str) {
    let rest = draw(element(), None, false, Color::BLACK, 1.0);
    let focused = draw(element(), None, true, Color::BLACK, 1.0);
    let b = focused.bounds;
    assert_eq!(b, rest.bounds, "{what}: the same box");
    assert_ne!(rest.pixels, focused.pixels, "{what}: focus shows");
    let dark = crate::menu_parts::RowContour::Rounded
        .target_border(&Theme::default())
        .color;
    let dark = [
        (dark.r * 255.0).round() as u8,
        (dark.g * 255.0).round() as u8,
        (dark.b * 255.0).round() as u8,
    ];
    // The box is centred on a half pixel, so the stroke on its left edge
    // is shared by two pixels: one of them is clearly not the ground
    let row = b.center_y();
    let edge = [
        focused.at(b.x.floor(), row),
        focused.at(b.x.floor() + 1.0, row),
    ];
    assert!(
        edge.iter().any(|p| (0..3).all(|n| p[n].abs_diff(128) > 30)),
        "{what}: the stroke is on the edge: {edge:?} (target {dark:?})"
    );
    for (n, pixel) in focused.pixels.iter().enumerate() {
        let (x, y) = ((n % W as usize) as f32, (n / W as usize) as f32);
        // A pixel wholly outside the box
        if x + 1.0 <= b.x || x >= b.x + b.width || y + 1.0 <= b.y || y >= b.y + b.height {
            assert!(
                is_near(*pixel, ground(), 0),
                "{what}: {x},{y} outside: {pixel:?}"
            );
        }
    }
}

#[test]
fn the_trigger_focused_wears_the_outline_inside_its_edge() {
    focus_is_the_outline_inside_the_edge(the_trigger, "trigger");
}

#[test]
fn a_title_focused_wears_the_outline_inside_its_edge() {
    focus_is_the_outline_inside_the_edge(a_title, "title");
}

/// A title's text is in the header's colour: with a red icon colour
/// given, the label's pixels are red and none is in the body text's colour
#[test]
fn a_title_is_drawn_in_the_headers_colour() {
    let red = Color::from_rgb(0.9, 0.1, 0.2);
    let picture = draw(a_title(), None, false, red, 1.0);
    let reddish = picture
        .pixels
        .iter()
        .filter(|p| p[0] > p[1].saturating_add(80))
        .count();
    let body = body_colour();
    let body = [body.r, body.g, body.b].map(|c| (c * 255.0).round() as u8);
    let in_body_colour = picture
        .pixels
        .iter()
        .filter(|p| is_near(**p, body, 12))
        .count();
    assert!(reddish > 20, "{reddish}");
    assert_eq!(in_body_colour, 0);
}

/// Pictures for the before and after sheet: each control at rest and
/// hovered, at 2x, as `NAME WIDTH HEIGHT BASE64` lines
#[test]
#[ignore = "prints the pictures of the before and after sheet"]
fn sheet() {
    use std::fmt::Write as _;
    let header = Color::from_rgb(0.0, 0.32, 0.4);
    let mut out = String::new();
    for (name, element, color) in [
        (
            "dropdown",
            the_trigger as fn() -> Element<'static, ()>,
            body_colour(),
        ),
        ("title", a_title, header),
    ] {
        for (state, pointer) in [("rest", None), ("hover", Some(Point::new(20.0, 16.0)))] {
            let picture = draw(element(), pointer, false, color, 2.0);
            let bytes: Vec<u8> = picture.pixels.iter().flatten().copied().collect();
            let _ = writeln!(
                out,
                "SHEET {name}-{state} {} {} {}",
                W * 2,
                H * 2,
                base64(&bytes)
            );
        }
    }
    println!("{out}");
}

fn base64(bytes: &[u8]) -> String {
    const SET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(SET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}
