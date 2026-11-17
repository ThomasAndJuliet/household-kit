// SPDX-License-Identifier: GPL-3.0-only
//! The scrollable list through the actual native runtime and renderer:
//! what a virtualised list builds, the reading position it keeps, reveal
//! of items not built, and measurement that commits nothing.

use cosmic::iced::advanced::widget::operation::Scrollable;
use cosmic::iced::advanced::widget::{Id, Operation};
use cosmic::iced::advanced::{clipboard, renderer::Headless};
use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
use cosmic::iced::widget::scrollable::AbsoluteOffset;
use cosmic::iced::{Font, Length, Pixels, Point, Rectangle, Size, Vector, mouse};
use cosmic::{Element, Renderer, widget};
use household_widgets::layout;
use household_widgets::scroll_list::{
    Axis, Estimate, EstimateClass, ItemSpec, ListAction, ListChild, ListState, Reveal, scroll_list,
};

#[derive(Clone, Debug)]
enum Message {
    List(ListAction),
}

struct Board;
impl clipboard::Clipboard for Board {
    fn read(&self, _: clipboard::Kind) -> Option<String> {
        None
    }
    fn write(&mut self, _: clipboard::Kind, _: String) {}
}

const PAGE: f32 = 500.0;

struct Window {
    renderer: Renderer,
    cache: Cache,
}

impl Window {
    fn new() -> Self {
        let renderer = cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
            Font::default(),
            Pixels(14.0),
            Some("tiny-skia"),
        ))
        .expect("the actual tiny-skia renderer");
        Self {
            renderer,
            cache: Cache::default(),
        }
    }

    /// Lay `content` out in a window `PAGE` tall, run `operation` on it,
    /// and deliver what it sends to `state`
    fn show(
        &mut self,
        state: &mut ListState<u64>,
        content: impl FnOnce(&ListState<u64>) -> Element<'_, Message>,
        operation: Option<&mut dyn Operation>,
    ) {
        let mut messages = Vec::new();
        {
            let root = widget::container(content(state))
                .width(Length::Fill)
                .height(Length::Fill);
            let cache = std::mem::take(&mut self.cache);
            let mut ui =
                UserInterface::build(root, Size::new(600.0, PAGE), cache, &mut self.renderer);
            if let Some(operation) = operation {
                ui.operate(&self.renderer, operation);
            }
            // A redraw reports the viewport, as the runtime's does
            let _ = ui.update(
                &[cosmic::iced::Event::Window(
                    cosmic::iced::window::Event::RedrawRequested(std::time::Instant::now()),
                )],
                mouse::Cursor::Available(Point::new(10.0, 10.0)),
                &mut self.renderer,
                &mut Board,
                &mut messages,
            );
            self.cache = ui.into_cache();
        }
        for Message::List(action) in messages {
            drop(state.apply::<()>(action));
        }
    }
}

/// Where the scrollable `id`'s view stands, or a new offset for it
struct Offset {
    id: Id,
    y: Option<f32>,
    scroll_to: Option<f32>,
}

impl Operation for Offset {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn container(&mut self, _id: Option<&Id>, _bounds: Rectangle) {}
    fn scrollable(
        &mut self,
        id: Option<&Id>,
        _bounds: Rectangle,
        _content: Rectangle,
        translation: Vector,
        state: &mut dyn Scrollable,
    ) {
        if id == Some(&self.id) {
            self.y = Some(translation.y);
            if let Some(y) = self.scroll_to {
                state.scroll_to(AbsoluteOffset {
                    x: None,
                    y: Some(y),
                });
            }
        }
    }
}

/// `count` items, item `i` `height(i)` tall, behind a leading line. At
/// list revision `revision`, the items whose height is not 50 changed
fn list<'a>(
    state: &'a ListState<u64>,
    count: usize,
    revision: u64,
    virtualised: bool,
    height: impl Fn(usize) -> f32 + 'a,
) -> Element<'a, Message> {
    let spec = |i: usize| ItemSpec {
        key: i as u64,
        revision: if height(i) == 50.0 { 1 } else { revision },
        class: EstimateClass(0),
        gap: 10.0,
        shown: true,
    };
    scroll_list(state)
        .virtualised(virtualised)
        .spacing(10.0)
        .leading(widget::Space::new().width(Length::Fill).height(24))
        .items(
            revision,
            count,
            spec,
            |_| Estimate::Fixed { ems: 50.0 / 15.0 },
            |i| ListChild::new(widget::Space::new().width(Length::Fill).height(height(i))),
        )
        .on_action(Message::List)
        .into()
}

#[test]
fn a_virtualised_list_builds_only_what_is_near_the_view() {
    let mut window = Window::new();
    let mut state = ListState::new();
    window.show(
        &mut state,
        |state| list(state, 1000, 1, true, |_| 50.0),
        None,
    );
    window.show(
        &mut state,
        |state| list(state, 1000, 1, true, |_| 50.0),
        None,
    );
    let built = state.counts().built;
    assert!(
        built < 2 * 40,
        "about three pages of items each time, not all 1000: {built}"
    );
    assert!(
        state.content_extent() > 1000.0 * 40.0,
        "the content is as tall as every item, built or not"
    );
}

#[test]
fn an_ordinary_list_builds_every_item_and_measures_it() {
    let mut window = Window::new();
    let mut state = ListState::new();
    window.show(
        &mut state,
        |state| list(state, 100, 1, false, |_| 50.0),
        None,
    );
    assert_eq!(state.counts().built, 100);
    assert_eq!(state.measured_keys().len(), 100);
    // 24 leading, then 100 items of 50 with a gap of 10 above each
    assert!(
        (state.content_extent() - (24.0 + 100.0 * 60.0)).abs() < 0.5,
        "{}",
        state.content_extent()
    );
}

#[test]
fn the_reading_position_holds_when_items_above_grow() {
    let mut window = Window::new();
    let mut state = ListState::new();
    window.show(
        &mut state,
        |state| list(state, 200, 1, true, |_| 50.0),
        None,
    );
    let id = state.id().clone();
    let mut to = Offset {
        id: id.clone(),
        y: None,
        scroll_to: Some(3000.0),
    };
    window.show(
        &mut state,
        |state| list(state, 200, 1, true, |_| 50.0),
        Some(&mut to),
    );
    window.show(
        &mut state,
        |state| list(state, 200, 1, true, |_| 50.0),
        None,
    );
    let (place, ..) = state.place().expect("a place once scrolled");
    let before = state.start_of(place).unwrap();
    // Items 42 to 46, above the view and built, turn out 100 px taller
    let mut look = Offset {
        id: id.clone(),
        y: None,
        scroll_to: None,
    };
    window.show(
        &mut state,
        |state| {
            list(state, 200, 2, true, |i| {
                if (42..47).contains(&i) { 150.0 } else { 50.0 }
            })
        },
        Some(&mut look),
    );
    let mut look = Offset {
        id,
        y: None,
        scroll_to: None,
    };
    window.show(
        &mut state,
        |state| {
            list(state, 200, 2, true, |i| {
                if (42..47).contains(&i) { 150.0 } else { 50.0 }
            })
        },
        Some(&mut look),
    );
    let after = state.start_of(place).unwrap();
    assert!(
        (after - before - 500.0).abs() < 0.5,
        "the place moved down by what grew above it: {before} → {after}"
    );
    let y = look.y.expect("the scrollable");
    assert!(
        (y - 3000.0 - (after - before)).abs() < 1.0,
        "the view followed it, so the same item stands at its top: {y}"
    );
}

#[test]
fn reveal_brings_an_item_not_built_into_view() {
    let mut window = Window::new();
    let mut state = ListState::new();
    window.show(
        &mut state,
        |state| list(state, 1000, 1, true, |_| 50.0),
        None,
    );
    assert!(
        !state.measured_keys().contains(&700),
        "far below the view, item 700 is not built"
    );
    drop(
        state
            .reveal::<()>(700, Reveal::Start { margin: 16.0 })
            .expect("item 700 is in the list"),
    );
    let mut look = Offset {
        id: state.id().clone(),
        y: None,
        scroll_to: None,
    };
    window.show(
        &mut state,
        |state| list(state, 1000, 1, true, |_| 50.0),
        None,
    );
    window.show(
        &mut state,
        |state| list(state, 1000, 1, true, |_| 50.0),
        Some(&mut look),
    );
    assert!(state.measured_keys().contains(&700), "built once revealed");
    let top = state.start_of(700).unwrap();
    let y = look.y.expect("the scrollable");
    assert!(
        (y - (top - 16.0)).abs() < 1.0,
        "its top 16 px below the view's: {y} for {top}"
    );
}

#[test]
fn a_disposable_measurement_commits_nothing() {
    let window = Window::new();
    let state = ListState::new();
    {
        let child = layout::scroll_child(
            |_| list(&state, 50, 1, true, |_| 80.0),
            Size::new(200.0, 300.0),
        );
        let _ = child.query(layout::Level::Preferred, &window.renderer);
        let _ = child.query_at(
            layout::Level::Preferred,
            &window.renderer,
            Size::new(240.0, 300.0),
        );
    }
    let counts = state.counts();
    assert_eq!(
        (counts.measured, counts.kept),
        (0, 0),
        "no heights measured, no place kept"
    );
    assert!(state.measured_keys().is_empty());
}

/// `count` buttons-like items along a horizontal list, item `i` `width(i)`
/// wide
fn row<'a>(
    state: &'a ListState<u64>,
    count: usize,
    width: impl Fn(usize) -> f32 + 'a,
) -> Element<'a, Message> {
    let spec = |i: usize| ItemSpec {
        key: i as u64,
        revision: 1,
        class: EstimateClass(0),
        gap: 8.0,
        shown: true,
    };
    scroll_list(state)
        .virtualised(true)
        .items(
            1,
            count,
            spec,
            |_| Estimate::Fixed { ems: 80.0 / 15.0 },
            |i| ListChild::new(widget::Space::new().width(width(i)).height(Length::Fill)),
        )
        .on_action(Message::List)
        .into()
}

#[test]
fn a_horizontal_list_builds_and_reveals_along_its_width() {
    let mut window = Window::new();
    let mut state = ListState::along(Axis::Horizontal);
    window.show(&mut state, |state| row(state, 500, |_| 80.0), None);
    window.show(&mut state, |state| row(state, 500, |_| 80.0), None);
    assert!(
        state.counts().built < 2 * 30,
        "about three widths of 600 px of items each time, not all 500: {}",
        state.counts().built
    );
    // 500 items of 80 with a gap of 8 before all but the first
    assert!(
        (state.content_extent() - (500.0 * 88.0 - 8.0)).abs() < 0.5,
        "the content is as wide as every item: {}",
        state.content_extent()
    );
    drop(
        state
            .reveal::<()>(400, Reveal::Start { margin: 0.0 })
            .expect("item 400"),
    );
    let mut look = Offset {
        id: state.id().clone(),
        y: None,
        scroll_to: None,
    };
    window.show(&mut state, |state| row(state, 500, |_| 80.0), None);
    window.show(
        &mut state,
        |state| row(state, 500, |_| 80.0),
        Some(&mut look),
    );
    assert!(state.measured_keys().contains(&400), "built once revealed");
    let start = state.start_of(400).unwrap();
    assert!(
        (state.view_start() - start).abs() < 1.0,
        "its start at the view's start: {} for {start}",
        state.view_start()
    );
}
