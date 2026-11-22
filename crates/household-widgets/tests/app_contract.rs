// SPDX-License-Identifier: GPL-3.0-only
//! App-boundary probes against the actual exported native components.
//! These are not a mock implementation of the proposed ui module.

#[path = "../src/test_fonts.rs"]
mod test_fonts;

use std::cell::Cell;

use cosmic::iced::advanced::{clipboard, widget::Id};
use cosmic::iced::event::Status;
use cosmic::iced::keyboard::{
    self, Key, Location, Modifiers,
    key::{Code, Physical},
};
use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
use cosmic::iced::{Event, Length, Point, Size, mouse};
use cosmic::{Element, Renderer, widget};
use household_widgets::{ShortTextBox, layout, short_text_box};

#[derive(Clone, Debug, PartialEq)]
enum Message {
    Edit {
        owner: u64,
        view_revision: u64,
        action: short_text_box::Action,
    },
    Submit(u64),
}

#[derive(Default)]
struct Board;
impl clipboard::Clipboard for Board {
    fn read(&self, _: clipboard::Kind) -> Option<String> {
        None
    }
    fn write(&mut self, _: clipboard::Kind, _: String) {}
}

struct NativeWindow {
    renderer: Renderer,
    cache: Cache,
}
impl NativeWindow {
    fn new() -> Self {
        let renderer = crate::test_fonts::renderer().expect("the actual tiny-skia renderer");
        Self {
            renderer,
            cache: Cache::default(),
        }
    }

    fn send<'a>(
        &mut self,
        content: Element<'a, Message>,
        width: f32,
        events: &[Event],
    ) -> (Vec<Message>, Vec<Status>) {
        let root = widget::container(content)
            .width(Length::Fill)
            .height(Length::Fill);
        let cache = std::mem::take(&mut self.cache);
        let mut ui = UserInterface::build(root, Size::new(width, 100.0), cache, &mut self.renderer);
        let mut messages = Vec::new();
        let mut statuses = Vec::new();
        let mut board = Board;
        for event in events {
            let (_, result) = ui.update(
                std::slice::from_ref(event),
                mouse::Cursor::Available(Point::new(120.0, 16.0)),
                &mut self.renderer,
                &mut board,
                &mut messages,
            );
            statuses.extend(result);
        }
        self.cache = ui.into_cache();
        (messages, statuses)
    }
}

/// The model, placeholder and callback state are borrowed, not cloned or made
/// 'static. The message owns its logical owner and rendered-snapshot revision.
fn input<'a>(
    state: &'a ShortTextBox,
    placeholder: &'a str,
    owner: u64,
    view_revision: u64,
    dispatched: &'a Cell<usize>,
) -> layout::Child<'a, Message> {
    layout::child(move |_| {
        short_text_box::text(state, placeholder, move |action| {
            dispatched.set(dispatched.get() + 1);
            Message::Edit {
                owner,
                view_revision,
                action,
            }
        })
        .id(Id::from(format!("contract:{owner}:draft")))
        .on_submit(Message::Submit(owner))
        .width(Length::Fill)
        .into()
    })
    .minimum_width(|_, _| 160.0)
    .grow(1.0)
}

fn element<'a>(
    state: &'a ShortTextBox,
    label: &'a str,
    owner: u64,
    revision: u64,
    dispatched: &'a Cell<usize>,
) -> Element<'a, Message> {
    layout::responsive_child(
        layout::Level::Preferred,
        input(state, label, owner, revision, dispatched),
    )
    .into()
}

fn click() -> Vec<Event> {
    vec![
        Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(120.0, 16.0),
        }),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ]
}

fn key(letter: &str, code: Code, modifiers: Modifiers) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Character(letter.into()),
        modified_key: Key::Character(letter.into()),
        physical_key: Physical::Code(code),
        location: Location::Standard,
        modifiers,
        text: (!modifiers.control()).then(|| letter.into()),
        repeat: false,
    })
}

fn apply(state: &mut ShortTextBox, owner: u64, messages: Vec<Message>) -> Vec<String> {
    let mut values = Vec::new();
    for message in messages {
        if let Message::Edit {
            owner: target,
            action,
            ..
        } = message
        {
            assert_eq!(target, owner, "the handler resolves the captured owner");
            state.perform(action);
            values.push(state.value().to_owned());
        }
    }
    values
}

#[test]
fn borrowed_app_data_and_callbacks_survive_pure_native_queries() {
    let window = NativeWindow::new();
    let state: ShortTextBox = ShortTextBox::starting_with("retained draft");
    let title = String::from("A title borrowed from app data");
    let placeholder = String::from("Search this document");
    let dispatched = Cell::new(0);
    let body = layout::column_child(
        vec![
            // This is component-author code: the text family's useful minimum
            // belongs here, rather than making every app predict string widths.
            layout::child(|_| widget::text(title.as_str()).into())
                .minimum_width(|_, preferred| preferred.width.min(160.0)),
            input(&state, &placeholder, 11, 5, &dispatched),
        ],
        8.0,
    );
    for width in [180.0, 240.0, 420.0] {
        let plan = body.query_at(
            layout::Level::Preferred,
            &window.renderer,
            Size::new(width, 150.0),
        );
        assert!(plan.is_ok(), "native query at {width}: {plan:?}");
    }
    assert!(
        body.query_at(
            layout::Level::Preferred,
            &window.renderer,
            Size::new(100.0, 8.0)
        )
        .is_err()
    );
    assert_eq!(state.value(), "retained draft");
    assert_eq!(
        dispatched.get(),
        0,
        "measurement must not invoke app actions"
    );
}

#[test]
fn an_input_action_keeps_its_owner_when_current_selection_changes() {
    let mut window = NativeWindow::new();
    let mut state: ShortTextBox = ShortTextBox::new();
    let label = String::from("Answer for operation A");
    let selected = Cell::new(11_u64);
    let dispatched = Cell::new(0);
    window.send(element(&state, &label, 11, 1, &dispatched), 320.0, &click());

    // The operation's card stays open while the app selection moves elsewhere.
    let view = element(&state, &label, 11, 1, &dispatched);
    selected.set(22);
    let (messages, statuses) =
        window.send(view, 320.0, &[key("x", Code::KeyX, Modifiers::empty())]);
    assert_eq!(selected.get(), 22);
    assert!(statuses.contains(&Status::Captured));
    assert!(matches!(
        messages.as_slice(),
        [Message::Edit {
            owner: 11,
            view_revision: 1,
            ..
        }]
    ));
    apply(&mut state, 11, messages);
    assert_eq!(state.value(), "x");
}

#[test]
fn native_input_can_emit_several_edits_from_one_rendered_snapshot() {
    let mut window = NativeWindow::new();
    let mut state: ShortTextBox = ShortTextBox::new();
    let dispatched = Cell::new(0);
    window.send(element(&state, "", 11, 7, &dispatched), 320.0, &click());
    let events = [
        key("a", Code::KeyA, Modifiers::empty()),
        key("b", Code::KeyB, Modifiers::empty()),
    ];
    let (messages, _) = window.send(element(&state, "", 11, 7, &dispatched), 320.0, &events);
    assert_eq!(messages.len(), 2);
    assert!(messages.iter().all(|message| matches!(
        message,
        Message::Edit {
            view_revision: 7,
            ..
        }
    )));
    let values = apply(&mut state, 11, messages);
    assert_eq!(
        values,
        ["a", "ab"],
        "a view revision is not a one-event lock"
    );
}

#[test]
fn measurement_and_resize_preserve_native_selection_ime_and_undo() {
    let mut window = NativeWindow::new();
    let mut state: ShortTextBox = ShortTextBox::starting_with("original draft");
    let dispatched = Cell::new(0);
    window.send(element(&state, "", 11, 1, &dispatched), 320.0, &click());
    window.send(
        element(&state, "", 11, 1, &dispatched),
        320.0,
        &[key("a", Code::KeyA, Modifiers::CTRL)],
    );

    let query = input(&state, "", 11, 1, &dispatched);
    for width in [180.0, 420.0] {
        assert!(
            query
                .query_at(
                    layout::Level::Preferred,
                    &window.renderer,
                    Size::new(width, 100.0)
                )
                .is_ok()
        );
    }
    drop(query);
    window.send(element(&state, "", 11, 1, &dispatched), 180.0, &[]);
    let commit = Event::InputMethod(cosmic::iced::advanced::input_method::Event::Commit(
        "日本".into(),
    ));
    let (messages, _) = window.send(element(&state, "", 11, 1, &dispatched), 420.0, &[commit]);
    apply(&mut state, 11, messages);
    assert_eq!(
        state.value(),
        "日本",
        "the pre-query selection must still be replaced"
    );

    let (messages, _) = window.send(
        element(&state, "", 11, 2, &dispatched),
        420.0,
        &[key("z", Code::KeyZ, Modifiers::CTRL)],
    );
    apply(&mut state, 11, messages);
    assert_eq!(state.value(), "original draft");
}
