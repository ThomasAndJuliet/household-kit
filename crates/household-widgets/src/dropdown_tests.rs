// SPDX-License-Identifier: GPL-3.0-only
//! A dropdown under a window root, through the native runtime: its list is
//! the window's one chain, replaces another open menu, sets the value once
//! when an option is chosen, works from the keyboard, and a disabled option
//! explains itself. The same list can reveal additional choices in place.

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
                decl.0.rendered(&request, |message| message),
                MenuStyle { icon, body },
                NS.into(),
            )
            .dropdown_rows(chosen)
            .preferred_width(decl.0.width)
            .initial_target(decl.0.selected)
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

#[derive(Default)]
struct Picker {
    icons: bool,
    reveal: Option<usize>,
    locked: bool,
    text_size: Option<f32>,
    width: Option<f32>,
}

fn view<'w>(window: &'w WindowUi<Root>, selected: usize, picker: &Picker) -> Element<'w, Ev> {
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
    let mut sizes = dropdown::<Message, Root, Decl>(
        ["Small", "Medium", "Large"],
        Some(selected),
        Message::Size,
    )
    .id("sizes")
    .disabled(2, "Not available on this screen")
    .locked(picker.locked);
    if picker.icons {
        for index in 0..3 {
            sizes = sizes.option_icon(index, icon("view-list-symbolic"));
        }
    }
    if let Some(visible) = picker.reveal {
        sizes = sizes.reveal_after(visible, "More sizes");
    }
    if let Some(size) = picker.text_size {
        sizes = sizes.text_size(size);
    }
    if let Some(width) = picker.width {
        sizes = sizes.width(width);
    }
    let content: Element<'w, Message> = native::column(vec![
        native::row(vec![menu, native::space().width(300).into(), sizes.into()])
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
    picker: Picker,
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
            picker: Picker::default(),
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
            let root = view(&self.window, self.selected, &self.picker);
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
        let root = view(&self.window, self.selected, &self.picker);
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
        self.visible_option(label)
            .unwrap_or_else(|| panic!("{label} is listed"))
    }

    fn visible_option(&self, label: &str) -> Option<Rectangle> {
        self.visible(&row_id(NS, &[], row_key(label, false, 0)))
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
    // libcosmic's dropdown: 8 px padding round a 14 px label in 1.2 line height
    let face = app.visible(&Id::new("sizes")).expect("the trigger");
    assert!((face.height - (14.0 * 1.2 + 16.0)).abs() < 0.5, "{face:?}");
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
    app.selected = 1;
    app.frame();
    app.focus(trigger_id("sizes"));
    app.named(Named::Enter);
    assert!(app.is_dropdown_open(), "Enter opens the list");
    assert_eq!(app.window.chain().unwrap().entry(), Entry::Keyboard);
    let option = |label: &str| Some(row_id(NS, &[], row_key(label, false, 0)));
    assert_eq!(
        app.focused(),
        option("Medium"),
        "the keyboard's target starts on the selected option"
    );
    app.named(Named::ArrowUp);
    assert_eq!(app.focused(), option("Small"));
    assert!(app.app.is_empty(), "moving chooses nothing");
    app.named(Named::Enter);
    assert_eq!(app.app, [Message::Size(0)]);
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

#[test]
fn preferred_width_includes_hidden_icons_and_ignores_width_overrides() {
    let sizes = dropdown::<Message, Root, Decl>(["Small", "Extra large"], Some(0), Message::Size);
    let text_width = sizes.preferred_width();
    let decorated = sizes
        .option_icon(1, icon("view-list-symbolic"))
        .reveal_after(1, "More sizes")
        .width(100.0);
    let icon_width = ICON_SIZE + f32::from(cosmic::theme::spacing().space_xxs);
    assert_eq!(decorated.preferred_width(), text_width + icon_width);
}

#[test]
fn text_size_controls_the_trigger_height_and_preferred_width() {
    let mut app = App::new();
    let source = Id::new("sizes");
    let default = app.visible(&source).unwrap();
    let text_size = 18.0;
    app.picker.text_size = Some(text_size);
    app.frame();

    let large = app.visible(&source).unwrap();
    let sizes =
        dropdown::<Message, Root, Decl>(["Small", "Medium", "Large"], Some(0), Message::Size)
            .text_size(text_size);
    assert_eq!(large.width, sizes.preferred_width());
    assert!(large.width > default.width);
    assert!((large.height - (1.2 * text_size + 2.0 * PADDING)).abs() < 0.5);
}

#[test]
fn a_narrow_icon_face_gives_the_label_room_and_keeps_the_caret_inside() {
    let app = App::new();
    let mut face = Face::new(
        "Medium".into(),
        Some(icon("view-list-symbolic")),
        Length::Fixed(160.0),
        TEXT_SIZE,
        Some(Toggle),
    );
    let prefix_width = face.prefix_width;
    let mut tree = Tree::empty();
    let mut layout_at = |width| {
        let limits = layout::Limits::new(Size::ZERO, Size::new(width, 100.0));
        let node = face.layout(&mut tree, &app.renderer, &limits);
        (
            Layout::new(&node).bounds(),
            face.parts(Layout::new(&node).bounds()),
        )
    };

    let (normal, parts) = layout_at(200.0);
    let icon = parts.leading.unwrap();
    assert_eq!(icon.x, normal.x + PADDING);
    assert_eq!(parts.label.x - (icon.x + icon.width), icon_gap());

    let (narrow, parts) = layout_at(CARET + prefix_width);
    assert!(parts.leading.is_none());
    assert!(parts.label.width >= prefix_width);
    assert!(parts.label.x < narrow.x + PADDING);
    let caret = parts.caret.unwrap();
    assert!(caret.x >= parts.label.x + parts.label.width);
    assert!(caret.x + caret.width <= narrow.x + narrow.width);

    let (tiny, parts) = layout_at(12.0);
    assert_eq!(tiny.width, 12.0);
    assert!(parts.leading.is_none() && parts.caret.is_none());
    assert!(parts.label.width > 0.0 && parts.label.width <= tiny.width);
}

#[test]
fn the_caret_waits_for_a_readable_prefix_at_each_text_size() {
    for text_size in [TEXT_SIZE, 18.0] {
        let prefix_width = measured_width("MM…", text_size, cosmic::font::default()).ceil();
        let breakpoint = CARET + prefix_width;
        for label in ["Default", "On"] {
            let face = Face::new(label.into(), None, Length::Shrink, text_size, Some(Toggle));
            let at = |width| face.parts(Rectangle::with_size(Size::new(width, 40.0)));
            assert!(at(breakpoint - 1.0).caret.is_none());
            assert!(at(breakpoint).caret.is_some());
        }
        let face = Face::new(
            "A longer choice".into(),
            None,
            Length::Shrink,
            text_size,
            Some(Toggle),
        );
        let parts = face.parts(Rectangle::with_size(Size::new(breakpoint, 40.0)));
        assert!(parts.label.width >= prefix_width);
    }
}

#[test]
fn a_text_trigger_captures_the_left_press_and_passes_through_the_release() {
    use cosmic::iced::event::Status;
    let mut app = App::new();
    let face = Face::new("Small".into(), None, 100.0.into(), TEXT_SIZE, Some(Toggle));
    let mut ui = UserInterface::build(
        trigger(face, None),
        SIZE,
        Default::default(),
        &mut app.renderer,
    );
    let mut messages = Vec::new();
    let (_, statuses) = ui.update(
        &[
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ],
        mouse::Cursor::Available(Point::new(10.0, 10.0)),
        &mut app.renderer,
        &mut cosmic::iced::advanced::clipboard::Null,
        &mut messages,
    );
    assert_eq!(statuses, [Status::Captured, Status::Ignored]);
    assert_eq!(messages, [Toggle]);
}

#[test]
fn icon_choices_measure_the_theme_gap_and_keep_the_width_across_selections() {
    let mut app = App::new();
    let source = Id::new("sizes");
    let text = app.visible(&source).unwrap();
    app.picker.icons = true;
    app.frame();
    let with_icon = app.visible(&source).unwrap();
    let expected_increase = ICON_SIZE + f32::from(cosmic::theme::spacing().space_xxs);
    assert!((with_icon.width - text.width - expected_increase).abs() < 0.5);
    assert_eq!(with_icon.height, text.height);

    app.click(app.trigger());
    let request = app.window.chain().unwrap();
    let list = request.declaration::<Decl>().unwrap();
    assert_eq!(list.0.width, with_icon.width);
    assert!(
        list.0
            .menu
            .rows()
            .iter()
            .all(|row| { matches!(row, Row::Item(item) if item.icon.is_some()) })
    );
    app.click(app.option("Medium").center());
    assert_eq!(app.app, [Message::Size(1)]);
    assert_eq!(app.visible(&source).unwrap().width, with_icon.width);

    app.picker.width = Some(128.0);
    app.frame();
    app.click(app.trigger());
    let request = app.window.chain().unwrap();
    let list = request.declaration::<Decl>().unwrap();
    assert_eq!(app.visible(&source).unwrap().width, 128.0);
    assert_eq!(list.0.width, 128.0);
    assert_eq!(list.0.selected, Some(1));
}

#[test]
fn disclosure_replaces_its_row_in_the_same_list_and_resets_on_reopen() {
    use keyboard::key::Named;
    let mut app = App::new();
    app.picker.reveal = Some(1);
    app.frame();
    app.click(app.trigger());
    let root = app.open_root();
    assert!(app.visible_option("Medium").is_none());
    let request = app.window.chain().unwrap();
    let list = request.declaration::<Decl>().unwrap();
    let [Row::Item(_), Row::Folder(label, hidden)] = list.0.menu.rows() else {
        panic!("the declaration includes the choices behind the disclosure");
    };
    assert_eq!(label, "More sizes");
    assert!(matches!(
        hidden.rows(),
        [Row::Item(medium), Row::Item(large)]
            if medium.message == Message::Size(1) && large.message == Message::Size(2)
    ));

    let disclosure = app.option("More sizes").center();
    app.cursor = mouse::Cursor::Available(disclosure);
    app.send(&[Event::Mouse(mouse::Event::CursorMoved {
        position: disclosure,
    })]);
    app.frame();
    assert!(app.visible_option("Medium").is_none());
    app.click(disclosure);
    assert_eq!(app.open_root(), root);
    assert_eq!(app.window.anchors().surfaces().len(), 1);
    assert!(app.visible_option("More sizes").is_none());
    assert!(app.option("Small").y < app.option("Medium").y);
    assert!(app.option("Medium").y < app.option("Large").y);
    assert_eq!(app.selected, 0);
    assert!(app.app.is_empty());

    app.named(Named::Escape);
    assert!(app.window.chain().is_none());
    app.focus(trigger_id("sizes"));
    app.key(
        keyboard::Key::Character(" ".into()),
        keyboard::Modifiers::empty(),
    );
    let target = |label: &str| Some(row_id(NS, &[], row_key(label, false, 0)));
    assert_eq!(app.focused(), target("Small"));
    assert!(app.visible_option("Medium").is_none());
    app.named(Named::End);
    assert_eq!(app.focused(), target("More sizes"));
    app.named(Named::Enter);
    assert_eq!(app.window.anchors().surfaces().len(), 1);
    assert_eq!(app.focused(), target("Medium"));
    assert!(app.app.is_empty());
    app.named(Named::End);
    assert_eq!(app.focused(), target("Large"));
    app.named(Named::Enter);
    assert!(app.app.is_empty());
    app.named(Named::ArrowUp);
    assert_eq!(app.focused(), target("Medium"));
    app.named(Named::Enter);
    assert_eq!(app.app, [Message::Size(1)]);
    assert!(app.window.chain().is_none());
}

#[test]
fn locking_an_open_dropdown_removes_its_menu_and_activation() {
    use keyboard::key::Named;
    let mut app = App::new();
    let trigger = app.trigger();
    app.click(trigger);
    assert!(app.is_dropdown_open());
    app.picker.locked = true;
    app.frame();
    assert!(app.window.chain().is_none());
    app.click(trigger);
    app.focus(trigger_id("sizes"));
    assert_ne!(app.focused(), Some(trigger_id("sizes")));
    app.named(Named::Enter);
    assert!(app.window.chain().is_none());
    assert!(app.app.is_empty());
}
