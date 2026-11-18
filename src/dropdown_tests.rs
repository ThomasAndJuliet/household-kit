// SPDX-License-Identifier: GPL-3.0-only
//! A dropdown under a window root, through the native runtime: its list is
//! the window's one chain, replaces another open menu, sets the value once
//! when an option is chosen, works from the keyboard, and a disabled option
//! explains itself.

use super::*;
use crate::menu::MenuStyle;
use crate::menu_surface::{row_id, row_key};
use crate::popup::{FocusProbe, Panel};
use crate::window::{Entry, UiEvent, WindowAction, WindowUi};
use cosmic::Renderer;
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::advanced::widget::{Id, Operation, operation};
use cosmic::iced::runtime::{UserInterface, user_interface};
use cosmic::iced::widget as native;
use cosmic::iced::window as native_window;
use cosmic::iced::{Event, Font, Pixels, Point, Rectangle, Size, keyboard, mouse};
use std::hash::{Hash, Hasher};
use std::time::Instant;

#[derive(Clone, Debug, PartialEq)]
enum Root {
    Declared(Declared),
    Menu,
}

impl From<Declared> for Root {
    fn from(declared: Declared) -> Self {
        Self::Declared(declared)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Message {
    Size(usize),
    Elsewhere,
}

type Ev = UiEvent<Root, Message>;

struct Decl(DropdownMenu<Message>);

impl From<DropdownMenu<Message>> for Decl {
    fn from(menu: DropdownMenu<Message>) -> Self {
        Self(menu)
    }
}

impl crate::window::Declaration for Decl {
    fn target(&self) -> u64 {
        let mut hash = std::hash::DefaultHasher::new();
        format!("{:?}", self.0.menu.rows().len()).hash(&mut hash);
        hash.finish()
    }
    fn revision(&self) -> u64 {
        let mut hash = std::hash::DefaultHasher::new();
        self.0.menu.outline().hash(&mut hash);
        hash.finish()
    }
}

fn body(text: String) -> Element<'static, Ev> {
    native::text(text).size(14).into()
}

fn chosen(text: String) -> Element<'static, Ev> {
    native::text(text)
        .size(14)
        .font(cosmic::font::semibold())
        .into()
}

fn icon(name: &'static str) -> cosmic::widget::icon::Handle {
    cosmic::widget::icon::from_name(name).handle()
}

const NS: &str = "size-list";

fn panels<'a>(window: &'a WindowUi<Root>) -> Vec<Panel<'a, Ev>> {
    let Some(request) = window.chain() else {
        return Vec::new();
    };
    match request.root() {
        Root::Declared(declared) => {
            let Some(decl) = request.declaration::<Decl>() else {
                return Vec::new();
            };
            crate::window::menu_surface(
                decl.0.menu.clone().map(UiEvent::Application),
                MenuStyle { icon, body },
                NS.into(),
            )
            .choice_rows(chosen)
            .preferred_width(decl.0.width)
            .panels(
                declared.source().clone(),
                request.path().to_vec(),
                request.on_path(),
                |m| m,
            )
        }
        Root::Menu => crate::window::menu_surface(
            crate::menu::Menu::new().item(crate::menu::MenuItem {
                label: "Elsewhere".into(),
                message: UiEvent::Application(Message::Elsewhere),
                icon: None,
                shortcut: None,
                mark: None,
                disabled_reason: None,
                destructive: false,
            }),
            MenuStyle { icon, body },
            "other".into(),
        )
        .panels(
            Id::new("other"),
            request.path().to_vec(),
            request.on_path(),
            |m| m,
        ),
    }
}

fn view<'w>(window: &'w WindowUi<Root>, selected: usize) -> Element<'w, Ev> {
    let menu = crate::window::trigger(
        "other",
        Root::Menu,
        crate::popup::focus_target(
            "other",
            native::button(native::space().width(60).height(30))
                .padding(0)
                .on_press(Toggle),
        )
        .on_activate(Toggle),
    );
    let sizes: Element<'w, Message> = dropdown::<Message, Root, Decl>(
        ["Small", "Medium", "Large"],
        Some(selected),
        Message::Size,
    )
    .id("sizes")
    .disabled(2, "Not available on this screen")
    .into();
    let content: Element<'w, Message> = native::column(vec![
        native::row(vec![menu, native::space().width(300).into(), sizes])
            .align_y(cosmic::iced::Alignment::Center)
            .into(),
        native::space().height(300).into(),
    ])
    .into();
    window.mount(content, panels(window)).into()
}

struct App {
    window: WindowUi<Root>,
    selected: usize,
    renderer: Renderer,
    cache: Option<user_interface::Cache>,
    cursor: mouse::Cursor,
    app: Vec<Message>,
}

const SIZE: Size = Size::new(500.0, 500.0);

impl App {
    fn new() -> Self {
        let renderer = cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
            Font::default(),
            Pixels(14.0),
            Some("tiny-skia"),
        ))
        .expect("headless renderer");
        let mut app = Self {
            window: WindowUi::new(),
            selected: 0,
            renderer,
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
            let root = view(&self.window, self.selected);
            let cache = self.cache.take().unwrap_or_default();
            let mut ui = UserInterface::build(root, SIZE, cache, &mut self.renderer);
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
                UiEvent::Internal(action) => {
                    let action: WindowAction<Root> = action;
                    self.window.apply(action);
                }
                UiEvent::Application(message) => {
                    // The app sets the value from the message, once per message
                    if let Message::Size(i) = message {
                        self.selected = i;
                    }
                    self.app.push(message);
                }
            }
        }
    }

    fn frame_at(&mut self, now: Instant) {
        for _ in 0..2 {
            self.send(&[Event::Window(native_window::Event::RedrawRequested(now))]);
        }
    }

    fn frame(&mut self) {
        self.frame_at(Instant::now());
    }

    fn visible(&self, id: &Id) -> Option<Rectangle> {
        self.window.anchors().visible(id)
    }

    fn click(&mut self, at: Point) {
        self.cursor = mouse::Cursor::Available(at);
        self.send(&[Event::Mouse(mouse::Event::CursorMoved { position: at })]);
        self.frame();
        self.send(&[Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        ))]);
        self.send(&[Event::Mouse(mouse::Event::ButtonReleased(
            mouse::Button::Left,
        ))]);
        self.frame();
    }

    fn key(&mut self, key: keyboard::Key, modifiers: keyboard::Modifiers) {
        self.send(&[Event::Keyboard(keyboard::Event::KeyPressed {
            modified_key: key.clone(),
            key,
            modifiers,
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            text: None,
            repeat: false,
        })]);
        self.frame();
    }

    fn named(&mut self, key: keyboard::key::Named) {
        self.key(keyboard::Key::Named(key), keyboard::Modifiers::empty());
    }

    fn operate(&mut self, operation: &mut dyn Operation) {
        let root = view(&self.window, self.selected);
        let cache = self.cache.take().unwrap_or_default();
        let mut ui = UserInterface::build(root, SIZE, cache, &mut self.renderer);
        ui.operate(&self.renderer, operation);
        self.cache = Some(ui.into_cache());
    }

    fn focused(&mut self) -> Option<Id> {
        let mut probe = FocusProbe::default();
        self.operate(&mut probe);
        probe.focused
    }

    fn focus(&mut self, id: Id) {
        self.operate(&mut operation::focusable::focus::<()>(id));
        self.frame();
    }

    fn open_root(&self) -> Option<Root> {
        self.window.chain().map(|chain| chain.root().clone())
    }

    fn option(&self, label: &str) -> Rectangle {
        self.visible(&row_id(NS, &[], row_key(label, false, 0)))
            .unwrap_or_else(|| panic!("{label} is listed"))
    }

    /// The dropdown's trigger, in the row after the other trigger and a gap
    /// the other's menu does not reach over
    fn trigger(&self) -> Point {
        let first = self.visible(&Id::new("other")).expect("the other trigger");
        Point::new(first.x + 60.0 + 300.0 + 20.0, first.y + first.height / 2.0)
    }

    fn is_dropdown_open(&self) -> bool {
        matches!(self.open_root(), Some(Root::Declared(_)))
    }
}

#[test]
fn the_trigger_is_as_wide_as_the_widest_option_and_the_list_as_wide_as_the_trigger() {
    let options = |labels: &[&str]| {
        labels
            .iter()
            .map(|label| DropdownOption {
                label: (*label).into(),
                disabled_reason: None,
            })
            .collect::<Vec<_>>()
    };
    let narrow = trigger_width(&options(&["On", "Off"]));
    let wide = trigger_width(&options(&["On", "Off", "Only when the window is in front"]));
    assert!(wide > narrow + 80.0, "{narrow} {wide}");
    let mut app = App::new();
    app.click(app.trigger());
    assert!(app.is_dropdown_open());
    let list = app.window.anchors().surfaces();
    let width = trigger_width(&options(&["Small", "Medium", "Large"]));
    assert!(
        list[0].width >= width - 1.0,
        "the list is at least as wide as its trigger: {list:?} {width}"
    );
}

#[test]
fn a_dropdown_opened_while_a_top_menu_is_open_replaces_it() {
    let mut app = App::new();
    app.click(app.visible(&Id::new("other")).unwrap().center());
    assert_eq!(app.open_root(), Some(Root::Menu));
    app.click(app.trigger());
    assert!(app.is_dropdown_open(), "{:?}", app.open_root());
    assert_eq!(
        app.window.anchors().surfaces().len(),
        1,
        "one chain in the window"
    );
    // And the other way: the top menu replaces the list
    app.click(app.visible(&Id::new("other")).unwrap().center());
    assert_eq!(app.open_root(), Some(Root::Menu));
    assert_eq!(app.window.anchors().surfaces().len(), 1);
}

#[test]
fn choosing_an_option_sets_the_value_once_and_closes_the_list() {
    let mut app = App::new();
    app.click(app.trigger());
    app.click(app.option("Medium").center());
    assert_eq!(app.app, [Message::Size(1)]);
    assert_eq!(app.selected, 1);
    assert!(app.window.chain().is_none());
    // Open again: the chosen option is the selected row, whole and marked
    app.click(app.trigger());
    let outline = app
        .window
        .chain()
        .and_then(|chain| chain.declaration::<Decl>())
        .expect("the list is open")
        .0
        .menu
        .outline();
    assert_eq!(outline[1], "[x] Medium");
    assert!(outline[0].starts_with("[ ] Small"), "{outline:?}");
    app.frame();
    assert_eq!(app.app, [Message::Size(1)], "opening chooses nothing");
}

#[test]
fn the_keyboard_opens_navigates_and_chooses() {
    use keyboard::key::Named;
    let mut app = App::new();
    app.focus(trigger_id("sizes"));
    app.named(Named::Enter);
    assert!(app.is_dropdown_open(), "Enter opens the list");
    assert_eq!(app.window.chain().unwrap().entry(), Entry::Keyboard);
    app.named(Named::ArrowDown);
    assert_eq!(
        app.focused(),
        Some(row_id(NS, &[], row_key("Medium", false, 0)))
    );
    assert!(app.app.is_empty(), "moving chooses nothing");
    app.named(Named::Enter);
    assert_eq!(app.app, [Message::Size(1)]);
    assert!(app.window.chain().is_none());
}

#[test]
fn a_disabled_option_explains_itself_and_cannot_be_chosen() {
    use keyboard::key::Named;
    let mut app = App::new();
    app.click(app.trigger());
    app.named(Named::ArrowDown);
    app.named(Named::ArrowDown);
    let large = row_id(NS, &[], row_key("Large", false, 0));
    assert_eq!(
        app.focused(),
        Some(large.clone()),
        "a disabled row is inspectable"
    );
    app.key(
        keyboard::Key::Character("h".into()),
        keyboard::Modifiers::ALT,
    );
    assert!(
        app.window.anchors().information_visible(&large),
        "Alt+H shows why at once"
    );
    app.named(Named::Enter);
    assert!(app.app.is_empty(), "it does nothing: {:?}", app.app);
}
