// SPDX-License-Identifier: GPL-3.0-only
//! The scrollable list through the actual native runtime and renderer:
//! what a virtualised list builds, the reading position it keeps, reveal
//! of items not built, and measurement that commits nothing.

use cosmic::iced::advanced::widget::operation::Scrollable;
use cosmic::iced::advanced::widget::operation::{self, Focusable};
use cosmic::iced::advanced::widget::{Id, Operation};
use cosmic::iced::advanced::{clipboard, renderer::Headless};
use cosmic::iced::keyboard;
use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
use cosmic::iced::widget::scrollable::AbsoluteOffset;
use cosmic::iced::{Font, Length, Pixels, Point, Rectangle, Size, Task, Vector, mouse};
use cosmic::{Element, Renderer, widget};
use household_widgets::layout;
use household_widgets::scroll_list::{
    Axis, Estimate, EstimateClass, ItemSpec, ListAction, ListChild, ListState, Reveal, scroll_list,
};

#[derive(Clone, Debug)]
enum Message {
    List(ListAction),
    Pressed,
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
    /// The window's size, `PAGE` tall unless a test changes it
    size: Size,
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
            size: Size::new(600.0, PAGE),
        }
    }

    /// Lay `content` out in the window, run `operation` on it,
    /// and deliver what it sends to `state`
    fn show(
        &mut self,
        state: &mut ListState<u64>,
        content: impl FnOnce(&ListState<u64>) -> Element<'_, Message>,
        operation: Option<&mut dyn Operation>,
    ) {
        drop(self.send(state, content, operation, &[]));
    }

    /// Lay `content` out, run `operation` on it, deliver `events` and then
    /// a redraw, apply the list's scroll reports to `state`, and return
    /// what else the list reported
    fn send(
        &mut self,
        state: &mut ListState<u64>,
        content: impl FnOnce(&ListState<u64>) -> Element<'_, Message>,
        operation: Option<&mut dyn Operation>,
        events: &[cosmic::iced::Event],
    ) -> Vec<ListAction> {
        let messages = self.messages(state, content, operation, events);
        let mut reported = Vec::new();
        for message in messages {
            match message {
                Message::List(ListAction::Scrolled(viewport)) => {
                    drop(state.apply::<()>(ListAction::Scrolled(viewport)))
                }
                Message::List(action) => reported.push(action),
                Message::Pressed => {}
            }
        }
        reported
    }

    /// As [`Window::send`], but return every message without applying
    /// any, as the runtime does before it updates the app
    fn messages(
        &mut self,
        state: &ListState<u64>,
        content: impl FnOnce(&ListState<u64>) -> Element<'_, Message>,
        operation: Option<&mut dyn Operation>,
        events: &[cosmic::iced::Event],
    ) -> Vec<Message> {
        let mut messages = Vec::new();
        {
            let root = widget::container(content(state))
                .width(Length::Fill)
                .height(Length::Fill);
            let cache = std::mem::take(&mut self.cache);
            let mut ui = UserInterface::build(root, self.size, cache, &mut self.renderer);
            if let Some(operation) = operation {
                ui.operate(&self.renderer, operation);
            }
            for event in events {
                let _ = ui.update(
                    std::slice::from_ref(event),
                    mouse::Cursor::Available(Point::new(10.0, 10.0)),
                    &mut self.renderer,
                    &mut Board,
                    &mut messages,
                );
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
        messages
    }

    /// Run the widget operations `task` asks for, one at a time, each in a
    /// frame of its own, as the runtime does once it has rebuilt the view;
    /// how many there were
    fn run(
        &mut self,
        state: &mut ListState<u64>,
        content: impl Fn(&ListState<u64>) -> Element<'_, Message>,
        task: Task<()>,
    ) -> usize {
        use cosmic::iced::futures::StreamExt;
        let Some(mut stream) = cosmic::iced::runtime::task::into_stream(task) else {
            return 0;
        };
        let mut ran = 0;
        while let Some(action) = cosmic::iced::futures::executor::block_on(stream.next()) {
            if let cosmic::iced::runtime::Action::Widget(mut operation) = action {
                self.show(state, &content, Some(operation.as_mut()));
                ran += 1;
            }
        }
        ran
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
        navigable: true,
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
        navigable: true,
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

fn row_id(i: usize) -> Id {
    Id::new(format!("row-{i}"))
}

/// `count` rows of 40 px, each a focusable button
fn rows<'a>(state: &'a ListState<u64>, count: usize, virtualised: bool) -> Element<'a, Message> {
    rows_navigable(state, count, virtualised, true)
}

fn rows_navigable<'a>(
    state: &'a ListState<u64>,
    count: usize,
    virtualised: bool,
    navigable: bool,
) -> Element<'a, Message> {
    let spec = move |i: usize| ItemSpec {
        key: i as u64,
        revision: 1,
        class: EstimateClass(0),
        gap: 0.0,
        shown: true,
        navigable,
    };
    scroll_list(state)
        .virtualised(virtualised)
        .items(
            1,
            count,
            spec,
            |_| Estimate::Fixed { ems: 40.0 / 15.0 },
            |i| {
                ListChild::new(
                    widget::button::custom(widget::Space::new().width(Length::Fill).height(40))
                        .id(row_id(i))
                        .on_press(Message::Pressed),
                )
            },
        )
        .on_action(Message::List)
        .into()
}

fn named(key: keyboard::key::Named, modifiers: keyboard::Modifiers) -> cosmic::iced::Event {
    cosmic::iced::Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(key),
        modified_key: keyboard::Key::Named(key),
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    })
}

/// The id of the focused widget, if any
#[derive(Default)]
struct Focused(Option<Id>);

impl Operation for Focused {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn focusable(&mut self, id: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
        if state.is_focused() {
            self.0 = id.cloned();
        }
    }
}

fn focused(
    window: &mut Window,
    state: &mut ListState<u64>,
    count: usize,
    virtualised: bool,
) -> Option<Id> {
    let mut look = Focused::default();
    window.show(
        state,
        |state| rows(state, count, virtualised),
        Some(&mut look),
    );
    look.0
}

/// Focus row `i` as a click or Tab into it does
fn focus_row(
    window: &mut Window,
    state: &mut ListState<u64>,
    count: usize,
    virtualised: bool,
    i: usize,
) {
    let mut focus = operation::focusable::focus(row_id(i));
    window.show(
        state,
        |state| rows(state, count, virtualised),
        Some(&mut focus),
    );
    window.show(state, |state| rows(state, count, virtualised), None);
}

#[test]
fn arrow_keys_move_the_target_and_its_focus() {
    use keyboard::key::Named;
    let mut window = Window::new();
    let mut state = ListState::new();
    window.show(&mut state, |state| rows(state, 50, false), None);
    focus_row(&mut window, &mut state, 50, false, 3);
    assert_eq!(
        state.target(),
        Some(3),
        "focusing a row makes it the target"
    );
    let none = keyboard::Modifiers::default();
    let reported = window.send(
        &mut state,
        |state| rows(state, 50, false),
        None,
        &[named(Named::ArrowDown, none)],
    );
    assert!(reported.is_empty());
    assert_eq!(state.target(), Some(4));
    assert_eq!(
        focused(&mut window, &mut state, 50, false),
        Some(row_id(4)),
        "the focus follows the target"
    );
    focus_row(&mut window, &mut state, 50, false, 0);
    let reported = window.send(
        &mut state,
        |state| rows(state, 50, false),
        None,
        &[named(Named::ArrowUp, none)],
    );
    assert!(
        matches!(reported.as_slice(), [ListAction::PastStart]),
        "up from the first row: {reported:?}"
    );
    assert_eq!(state.target(), Some(0), "the list stays where it was");
    let reported = window.send(
        &mut state,
        |state| rows(state, 50, false),
        None,
        &[named(Named::Tab, none)],
    );
    assert!(
        matches!(reported.as_slice(), [ListAction::Leave { forward: true }]),
        "Tab leaves the list: {reported:?}"
    );
}

#[test]
fn end_reaches_a_row_not_built_and_focuses_it_once_built() {
    use keyboard::key::Named;
    let mut window = Window::new();
    let mut state = ListState::new();
    window.show(&mut state, |state| rows(state, 1000, true), None);
    focus_row(&mut window, &mut state, 1000, true, 0);
    assert!(
        !state.measured_keys().contains(&999),
        "the last row is not built"
    );
    let none = keyboard::Modifiers::default();
    drop(window.send(
        &mut state,
        |state| rows(state, 1000, true),
        None,
        &[named(Named::End, none)],
    ));
    assert_eq!(state.target(), Some(999));
    window.show(&mut state, |state| rows(state, 1000, true), None);
    assert_eq!(
        focused(&mut window, &mut state, 1000, true),
        Some(row_id(999)),
        "focused once built"
    );
    let end = state.start_of(999).unwrap() + 40.0;
    assert!(
        (state.view_start() + PAGE - end).abs() < 1.0,
        "the view ends with it: {} + {PAGE} for {end}; content {}, stands {:?}, measured {}",
        state.view_start(),
        state.content_extent(),
        state.stands(),
        state.measured_keys().len()
    );
}

#[test]
fn the_target_stays_built_and_focused_when_scrolled_away() {
    let mut window = Window::new();
    let mut state = ListState::new();
    window.show(&mut state, |state| rows(state, 1000, true), None);
    focus_row(&mut window, &mut state, 1000, true, 2);
    let mut away = Offset {
        id: state.id().clone(),
        y: None,
        scroll_to: Some(30_000.0),
    };
    window.show(&mut state, |state| rows(state, 1000, true), Some(&mut away));
    window.show(&mut state, |state| rows(state, 1000, true), None);
    assert!(
        state.view_start() > 20_000.0,
        "scrolled far away: {}",
        state.view_start()
    );
    assert_eq!(
        focused(&mut window, &mut state, 1000, true),
        Some(row_id(2)),
        "its focus survives"
    );
}

#[test]
fn a_list_without_targets_leaves_the_keys_to_its_items() {
    use keyboard::key::Named;
    let mut window = Window::new();
    let mut state = ListState::new();
    fn view(state: &ListState<u64>) -> Element<'_, Message> {
        rows_navigable(state, 20, false, false)
    }
    window.show(&mut state, view, None);
    let mut focus = operation::focusable::focus(row_id(3));
    window.show(&mut state, view, Some(&mut focus));
    let none = keyboard::Modifiers::default();
    let reported = window.send(
        &mut state,
        view,
        None,
        &[named(Named::Tab, none), named(Named::ArrowDown, none)],
    );
    assert!(reported.is_empty(), "nothing taken: {reported:?}");
    assert_eq!(state.target(), None, "no target in a list of none");
}

// Following the end. A list that follows its end stays there as its content
// and view change, in the layout where they change, whatever the order in
// which its scroll reports and tasks arrive. The harness drops every task
// `ListState::apply` returns, so nothing here can rely on one.

/// A wheel tick: `y` below zero scrolls down, `x` below zero to the right
fn wheel(x: f32, y: f32) -> cosmic::iced::Event {
    cosmic::iced::Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Pixels { x, y },
    })
}

fn along(axis: Axis, v: Vector) -> f32 {
    match axis {
        Axis::Vertical => v.y,
        Axis::Horizontal => v.x,
    }
}

fn extent(axis: Axis, size: Size) -> f32 {
    match axis {
        Axis::Vertical => size.height,
        Axis::Horizontal => size.width,
    }
}

/// Where the scrollable `id` stands along `axis` and where its end is,
/// after the layout it runs in; optionally moving it first, as the
/// scrollable operations do
struct Look {
    id: Id,
    axis: Axis,
    found: Option<(f32, f32)>,
    scroll_to: Option<f32>,
}

impl Look {
    fn at(state: &ListState<u64>) -> Self {
        Self {
            id: state.id().clone(),
            axis: state.axis(),
            found: None,
            scroll_to: None,
        }
    }

    fn scroll_to(state: &ListState<u64>, offset: f32) -> Self {
        Self {
            scroll_to: Some(offset),
            ..Self::at(state)
        }
    }

    /// The offset and the end it was found at
    fn stood(&self) -> (f32, f32) {
        self.found.expect("the list's scrollable")
    }
}

impl Operation for Look {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn container(&mut self, _id: Option<&Id>, _bounds: Rectangle) {}
    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        translation: Vector,
        state: &mut dyn Scrollable,
    ) {
        if id != Some(&self.id) {
            return;
        }
        let axis = self.axis;
        let end = (extent(axis, content.size()) - extent(axis, bounds.size())).max(0.0);
        self.found = Some((along(axis, translation), end));
        let main = |main: f32| match axis {
            Axis::Vertical => (None, Some(main)),
            Axis::Horizontal => (Some(main), None),
        };
        if let Some(offset) = self.scroll_to {
            let (x, y) = main(offset);
            state.scroll_to(AbsoluteOffset { x, y });
        }
    }
}

/// The list stands at its end and follows it
#[track_caller]
fn assert_pinned(state: &ListState<u64>, look: &Look, why: &str) {
    let (offset, end) = look.stood();
    assert!(
        (offset - end).abs() <= 1.0,
        "{why}: the view stands at {offset}, its end is at {end}"
    );
    assert!(state.follows_end(), "{why}: the list follows its end");
}

/// The list stands at `at` and does not follow its end
#[track_caller]
fn assert_stays(state: &ListState<u64>, look: &Look, at: f32, why: &str) {
    let (offset, end) = look.stood();
    assert!(
        (offset - at).abs() <= 1.0,
        "{why}: the view stands at {offset}, not {at} (end {end})"
    );
    assert!(
        !state.follows_end(),
        "{why}: the list does not follow its end"
    );
}

/// `count` items of 50 px a gap of 10 apart behind a leading line of 24:
/// 50 of them are 3024 px, so the end of a view `PAGE` tall is at 2524
fn convo(state: &ListState<u64>, count: usize) -> Element<'_, Message> {
    list(state, count, 1, false, |_| 50.0)
}

/// A list that follows its end, laid out with `count` items and standing
/// there
fn following(window: &mut Window, count: usize) -> ListState<u64> {
    let mut state = ListState::following_end();
    window.show(&mut state, |state| convo(state, count), None);
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, count), Some(&mut look));
    assert_pinned(&state, &look, "to begin with");
    state
}

#[test]
fn a_wheel_tick_at_the_end_then_growth_stays_at_the_new_end() {
    let mut window = Window::new();
    let mut state = following(&mut window, 50);
    // The tick unpins the scrollable to an absolute offset at its end, and
    // reports a user scroll there; whatever task that returns never runs
    drop(window.send(
        &mut state,
        |state| convo(state, 50),
        None,
        &[wheel(0.0, -50.0)],
    ));
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 60), Some(&mut look));
    assert_pinned(&state, &look, "ten items arrived");
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 70), Some(&mut look));
    assert_pinned(&state, &look, "and ten more");
}

#[test]
fn a_wheel_tick_at_the_end_then_a_shorter_view_stays_at_the_end() {
    let mut window = Window::new();
    let mut state = following(&mut window, 50);
    drop(window.send(
        &mut state,
        |state| convo(state, 50),
        None,
        &[wheel(0.0, -50.0)],
    ));
    // Something below the list, a composer, took 100 px of its view
    window.size.height = PAGE - 100.0;
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 50), Some(&mut look));
    assert_pinned(&state, &look, "the view got shorter");
}

#[test]
fn a_wheel_tick_at_the_end_then_shrinking_stays_at_the_end() {
    let mut window = Window::new();
    let mut state = following(&mut window, 50);
    drop(window.send(
        &mut state,
        |state| convo(state, 50),
        None,
        &[wheel(0.0, -50.0)],
    ));
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 40), Some(&mut look));
    assert_pinned(&state, &look, "ten items went");
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 50), Some(&mut look));
    assert_pinned(&state, &look, "and came back");
}

#[test]
fn a_move_to_the_middle_is_kept_as_items_arrive() {
    let mut window = Window::new();
    let mut state = following(&mut window, 50);
    // What ListState::scroll_to does, and then the operation its task runs
    drop(state.scroll_to::<()>(1000.0));
    let mut to = Look::scroll_to(&state, 1000.0);
    window.show(&mut state, |state| convo(state, 50), Some(&mut to));
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 60), Some(&mut look));
    assert_stays(&state, &look, 1000.0, "ten items arrived below the view");
    let place = state.place().expect("a place once moved");
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 70), Some(&mut look));
    assert_stays(&state, &look, 1000.0, "and ten more");
    assert_eq!(
        state.place(),
        Some(place),
        "the same item at the view's start"
    );
}

#[test]
fn a_raw_move_to_the_middle_while_following_stops_following() {
    let mut window = Window::new();
    let mut state = following(&mut window, 50);
    // The operation alone, the list's state not told
    let mut to = Look::scroll_to(&state, 1000.0);
    window.show(&mut state, |state| convo(state, 50), Some(&mut to));
    assert!(
        !state.follows_end(),
        "a move away from the end, not by the user"
    );
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 60), Some(&mut look));
    assert_stays(&state, &look, 1000.0, "ten items arrived below the view");
}

#[test]
fn a_wheel_tick_up_is_not_undone_by_a_layout_that_still_follows() {
    let mut window = Window::new();
    let mut state = following(&mut window, 50);
    let end = 24.0 + 50.0 * 60.0 - PAGE;
    // The tick's report is not applied yet, so the next view is built from
    // a state that still follows, as the runtime may build it
    let reports = window.messages(&state, |state| convo(state, 50), None, &[wheel(0.0, 50.0)]);
    assert!(state.follows_end(), "not told of the tick yet");
    let mut look = Look::at(&state);
    let later = window.messages(&state, |state| convo(state, 60), Some(&mut look), &[]);
    let (offset, _) = look.stood();
    assert!(
        (offset - (end - 50.0)).abs() <= 1.0,
        "the view stays 50 px up from where its end was: {offset}"
    );
    for message in reports.into_iter().chain(later) {
        if let Message::List(action) = message {
            drop(state.apply::<()>(action));
        }
    }
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 70), Some(&mut look));
    assert_stays(&state, &look, end - 50.0, "once told of the tick");
}

/// A list at 2100, not following: a page forward from there (450) passes
/// the end at 2524
fn reading_at_2100(window: &mut Window) -> ListState<u64> {
    let mut state = ListState::new();
    window.show(&mut state, |state| convo(state, 50), None);
    let mut to = Look::scroll_to(&state, 2100.0);
    window.show(&mut state, |state| convo(state, 50), Some(&mut to));
    window.show(&mut state, |state| convo(state, 50), None);
    assert!(!state.follows_end());
    state
}

#[test]
fn a_page_forward_to_the_end_follows_it_through_growth() {
    let mut window = Window::new();
    let mut state = reading_at_2100(&mut window);
    let task = state.scroll_pages::<()>(1.0);
    assert_eq!(
        window.run(&mut state, |state| convo(state, 50), task),
        1,
        "one operation"
    );
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 60), Some(&mut look));
    assert_pinned(&state, &look, "paged to the end, then ten items arrived");
}

/// The runtime lays the view out again before it runs the page's
/// operation: output that arrives with the key is laid out first
#[test]
fn a_page_forward_to_the_end_follows_output_laid_out_before_its_scroll() {
    let mut window = Window::new();
    let mut state = reading_at_2100(&mut window);
    let task = state.scroll_pages::<()>(1.0);
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 60), Some(&mut look));
    assert_stays(&state, &look, 2100.0, "ten items laid out before the page");
    window.run(&mut state, |state| convo(state, 60), task);
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 60), Some(&mut look));
    assert_pinned(&state, &look, "then the page reached the scrollable");
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 70), Some(&mut look));
    assert_pinned(&state, &look, "and ten more items arrived");
}

/// A page whose operation never runs leaves the list as it was, restored
/// where it stood in a new widget tree (a tab switched away and back)
#[test]
fn a_lost_page_forward_changes_nothing() {
    let mut window = Window::new();
    let mut state = reading_at_2100(&mut window);
    drop(state.scroll_pages::<()>(1.0));
    assert!(
        !state.follows_end(),
        "not following before the page arrives"
    );
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 60), Some(&mut look));
    assert_stays(&state, &look, 2100.0, "ten items arrived");
    let mut elsewhere = Window::new();
    let mut look = Look::at(&state);
    elsewhere.show(&mut state, |state| convo(state, 60), Some(&mut look));
    assert_stays(&state, &look, 2100.0, "restored in a new widget tree");
}

/// A scroll up between the key and its operation is not undone: the page
/// then scrolls by a page from there, short of the end
#[test]
fn a_page_forward_after_a_scroll_up_scrolls_by_a_page() {
    let mut window = Window::new();
    let mut state = reading_at_2100(&mut window);
    let task = state.scroll_pages::<()>(1.0);
    drop(window.send(
        &mut state,
        |state| convo(state, 50),
        None,
        &[wheel(0.0, 50.0)],
    ));
    window.run(&mut state, |state| convo(state, 50), task);
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 60), Some(&mut look));
    assert_stays(&state, &look, 2050.0 + 450.0, "a page on from 2050");
}

#[test]
fn a_page_forward_short_of_the_end_does_not_follow_it() {
    let mut window = Window::new();
    let mut state = ListState::new();
    window.show(&mut state, |state| convo(state, 50), None);
    let mut to = Look::scroll_to(&state, 1000.0);
    window.show(&mut state, |state| convo(state, 50), Some(&mut to));
    window.show(&mut state, |state| convo(state, 50), None);
    let task = state.scroll_pages::<()>(1.0);
    assert!(!state.follows_end(), "1450 is short of the end at 2524");
    window.run(&mut state, |state| convo(state, 50), task);
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 60), Some(&mut look));
    assert_stays(&state, &look, 1450.0, "ten items arrived below the view");
}

#[test]
fn only_a_move_away_from_the_end_stops_following() {
    let mut window = Window::new();
    let mut state = reading_at_2100(&mut window);
    // Asked to follow the end again: it follows from now on, though the
    // snap has not reached the scrollable yet
    let task = state.follow_end::<()>();
    assert!(state.follows_end());
    // Items arrive first: the layout reports new content under the same
    // offset, which is not a move
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 51), Some(&mut look));
    assert!(
        (look.stood().0 - 2100.0).abs() <= 1.0,
        "the offset is unchanged: {:?}",
        look.stood()
    );
    assert!(
        state.follows_end(),
        "a layout alone does not stop following"
    );
    window.run(&mut state, |state| convo(state, 51), task);
    let mut look = Look::at(&state);
    window.show(&mut state, |state| convo(state, 51), Some(&mut look));
    assert_pinned(&state, &look, "once the snap arrived");
    // A move away from the end, not by the user, does
    let mut to = Look::scroll_to(&state, 1000.0);
    window.show(&mut state, |state| convo(state, 51), Some(&mut to));
    assert!(!state.follows_end(), "moved away from the end");
}

/// Items 50.25 px tall: the content is 3036.5 px, and its end at 2536.5
/// is between whole pixels, where the scrollable's offset is rounded
fn fractional(state: &ListState<u64>) -> Element<'_, Message> {
    list(state, 50, 2, false, |_| 50.25)
}

#[test]
fn a_wheel_tick_up_of_less_than_a_pixel_is_not_undone_by_a_layout_that_still_follows() {
    let mut window = Window::new();
    let mut state = ListState::following_end();
    window.show(&mut state, fractional, None);
    let mut look = Look::at(&state);
    window.show(&mut state, fractional, Some(&mut look));
    assert_pinned(&state, &look, "to begin with");
    // 0.8 px up: rounded, the offset still looks as if at the end
    let reports = window.messages(&state, fractional, None, &[wheel(0.0, 0.8)]);
    let later = window.messages(&state, fractional, None, &[]);
    for message in reports.into_iter().chain(later) {
        if let Message::List(action) = message {
            drop(state.apply::<()>(action));
        }
    }
    let mut look = Look::at(&state);
    window.show(&mut state, fractional, Some(&mut look));
    let (offset, end) = look.stood();
    assert!(!state.follows_end(), "scrolled up, by however little");
    assert!(offset < end, "not pinned again: {offset} of {end}");
}
#[test]
fn a_horizontal_list_follows_its_end_along_its_width() {
    let mut window = Window::new();
    let mut state = ListState::along(Axis::Horizontal);
    drop(state.follow_end::<()>());
    window.show(&mut state, |state| row(state, 50, |_| 80.0), None);
    let mut look = Look::at(&state);
    window.show(
        &mut state,
        |state| row(state, 50, |_| 80.0),
        Some(&mut look),
    );
    assert_pinned(&state, &look, "to begin with");
    drop(window.send(
        &mut state,
        |state| row(state, 50, |_| 80.0),
        None,
        &[wheel(-50.0, 0.0)],
    ));
    let mut look = Look::at(&state);
    window.show(
        &mut state,
        |state| row(state, 60, |_| 80.0),
        Some(&mut look),
    );
    assert_pinned(&state, &look, "a tick at the end, then ten items arrived");
    // A move to the middle is kept
    drop(state.scroll_to::<()>(1000.0));
    let mut to = Look::scroll_to(&state, 1000.0);
    window.show(&mut state, |state| row(state, 60, |_| 80.0), Some(&mut to));
    let mut look = Look::at(&state);
    window.show(
        &mut state,
        |state| row(state, 70, |_| 80.0),
        Some(&mut look),
    );
    assert_stays(&state, &look, 1000.0, "ten items arrived past the view");
}

#[test]
fn a_virtualised_list_follows_its_end_as_new_items_are_measured() {
    let mut window = Window::new();
    let mut state = ListState::following_end();
    // Items from 200 on are 120 px, not the 50 their estimate says
    fn view(state: &ListState<u64>, count: usize) -> Element<'_, Message> {
        list(
            state,
            count,
            2,
            true,
            |i| if i >= 200 { 120.0 } else { 50.0 },
        )
    }
    window.show(&mut state, |state| view(state, 200), None);
    let mut look = Look::at(&state);
    window.show(&mut state, |state| view(state, 200), Some(&mut look));
    assert_pinned(&state, &look, "to begin with");
    drop(window.send(
        &mut state,
        |state| view(state, 200),
        None,
        &[wheel(0.0, -50.0)],
    ));
    let mut look = Look::at(&state);
    window.show(&mut state, |state| view(state, 210), Some(&mut look));
    assert_pinned(&state, &look, "ten taller items arrived");
    assert!(
        state.measured_keys().contains(&209),
        "the last item is built and measured"
    );
    let exact = 24.0 + 200.0 * 60.0 + 10.0 * 130.0;
    assert!(
        (state.content_extent() - exact).abs() < 0.5,
        "its exact extent counts: {}",
        state.content_extent()
    );
}
