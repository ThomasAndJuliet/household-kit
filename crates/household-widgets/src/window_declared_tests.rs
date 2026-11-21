// SPDX-License-Identifier: GPL-3.0-only
//! Declared roots under a window root, through the native runtime: a row's
//! right-click menu and a button's menu are the window's one chain, opened
//! at the pointer (or, from the keyboard, at the focused control), built
//! from what their source declares now, and acting on the target they were
//! opened on.

use super::*;
use crate::menu::{Menu, MenuItem, MenuStyle};
use crate::popup::{FocusProbe, Panel};
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::advanced::widget::operation;
use cosmic::iced::runtime::{UserInterface, user_interface};
use cosmic::iced::widget as native;
use cosmic::iced::{Font, Pixels, Point, keyboard, window as native_window};
use std::hash::{Hash, Hasher};
use std::time::Instant;

#[derive(Clone, Debug, PartialEq)]
enum Root {
    /// A menu a source declares
    Declared(Declared),
    /// A click menu of another area
    Other,
}

impl From<Declared> for Root {
    fn from(declared: Declared) -> Self {
        Self::Declared(declared)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Message {
    Chose(String),
}

type Ev = UiEvent<Root, Message>;

/// A row's menu: its actions name the row; `label` is what Copy says
struct RowMenu {
    row: &'static str,
    label: String,
}

impl Declaration for RowMenu {
    fn target(&self) -> u64 {
        let mut hash = std::hash::DefaultHasher::new();
        self.row.hash(&mut hash);
        hash.finish()
    }
    fn revision(&self) -> u64 {
        let mut hash = std::hash::DefaultHasher::new();
        (self.row, &self.label).hash(&mut hash);
        hash.finish()
    }
}

fn body(text: String) -> Element<'static, Ev> {
    native::text(text).size(14).into()
}

fn icon(name: &'static str) -> cosmic::widget::icon::Handle {
    cosmic::widget::icon::from_name(name).handle()
}

fn item(label: String, row: &str) -> MenuItem<Ev> {
    MenuItem {
        message: UiEvent::Application(Message::Chose(format!("{label} {row}"))),
        label,
        icon: None,
        shortcut: None,
        mark: None,
        disabled_reason: None,
        destructive: false,
        detail: None,
    }
}

/// The window's state as the app has it
#[derive(Default)]
struct Rows {
    /// The two rows in the other order
    swapped: bool,
    /// The rows keep their place as their identity, rather than a key
    placed: bool,
    /// What row A's Copy says now
    label: Option<String>,
}

fn panels<'a>(window: &'a WindowUi<Root>) -> Vec<Panel<'a, Ev>> {
    let Some(request) = window.chain() else {
        return Vec::new();
    };
    match request.root() {
        Root::Declared(declared) => {
            let Some(menu) = request.declaration::<RowMenu>() else {
                return Vec::new();
            };
            let content = Menu::new()
                .item(item(menu.label.clone(), menu.row))
                .item(item("Rename".into(), menu.row));
            crate::window::menu_surface(content, MenuStyle { icon, body }, "row-menu".into())
                .root_region(declared.invocation().region)
                .panels(
                    declared.source().clone(),
                    request.path().to_vec(),
                    request.on_path(),
                    |m| m,
                )
        }
        Root::Other => crate::window::menu_surface(
            Menu::new().item(item("Elsewhere".into(), "other")),
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

fn row<'a>(rows: &Rows, name: &'static str) -> Element<'a, Message> {
    let label = match (name, &rows.label) {
        ("A", Some(label)) => label.clone(),
        _ => "Copy".into(),
    };
    let content = native::row(vec![
        native::text(name).size(14).width(Length::Fill).into(),
        Element::new(Claimer {
            content: native::space().width(40).height(20).into(),
        }),
        crate::popup::focus_target(
            format!("button-{name}"),
            native::button(native::space().width(30).height(20)).padding(0),
        )
        .into(),
    ])
    .width(300)
    .height(40);
    let source = (!rows.placed).then(|| Id::new(format!("row-{name}")));
    crate::window::context_menu::<Message, Root, RowMenu>(
        source,
        RowMenu { row: name, label },
        content,
    )
}

fn view<'w>(window: &'w WindowUi<Root>, rows: &Rows) -> Element<'w, Ev> {
    let other = crate::window::trigger(
        "other",
        Root::Other,
        crate::popup::focus_target(
            "other",
            native::button(native::space().width(60).height(30))
                .padding(0)
                .on_press(Toggle),
        )
        .on_activate(Toggle),
    );
    let (first, second) = if rows.swapped { ("B", "A") } else { ("A", "B") };
    // An area whose menu the app provides, of whatever its state says
    let picked = crate::window::context_root(
        "picked",
        |invocation: Invocation| (invocation.entry == Entry::Pointer).then_some(Root::Other),
        native::space().width(300).height(40),
    );
    let content: Element<'w, Message> = native::column(vec![
        other,
        row(rows, first),
        row(rows, second),
        picked,
        native::space().height(260).into(),
    ])
    .into();
    window.mount(content, panels(window)).into()
}

/// A marker that takes a right press for its surface's menu, telling the
/// app what was right-clicked (as a conversation's items do)
struct Claimer<'a> {
    content: Element<'a, Message>,
}

impl Widget<Message, Theme, Renderer> for Claimer<'_> {
    forward_widget!(Message, Theme, Renderer; children, diff, size, layout, draw, operate, mouse_interaction, overlay);
    fn update(
        &mut self,
        _tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
        ) && cursor.is_over(layout.bounds())
        {
            shell.publish(Message::Chose("marked".into()));
            shell.capture_event();
            crate::window::claim_right_press();
        }
    }
}

struct App {
    window: WindowUi<Root>,
    rows: Rows,
    renderer: Renderer,
    cache: Option<user_interface::Cache>,
    cursor: mouse::Cursor,
    app: Vec<Message>,
    actions: Vec<WindowAction<Root>>,
}

const SIZE: Size = Size::new(600.0, 500.0);

impl App {
    fn new(rows: Rows) -> Self {
        let renderer = cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
            Font::default(),
            Pixels(14.0),
            Some("tiny-skia"),
        ))
        .expect("headless renderer");
        let mut app = Self {
            window: WindowUi::new(),
            rows,
            renderer,
            cache: None,
            cursor: mouse::Cursor::Unavailable,
            app: Vec::new(),
            actions: Vec::new(),
        };
        app.frame();
        app
    }

    fn send(&mut self, events: &[Event]) {
        let mut published = Vec::new();
        {
            let root = view(&self.window, &self.rows);
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
                    self.actions.push(action.clone());
                    self.window.apply(action);
                }
                UiEvent::Application(message) => self.app.push(message),
            }
        }
    }

    fn frame(&mut self) {
        for _ in 0..2 {
            self.send(&[Event::Window(native_window::Event::RedrawRequested(
                Instant::now(),
            ))]);
        }
    }

    fn visible(&self, id: &Id) -> Option<Rectangle> {
        self.window.anchors().visible(id)
    }

    fn point(&mut self, at: Point) {
        self.cursor = mouse::Cursor::Available(at);
        self.send(&[Event::Mouse(mouse::Event::CursorMoved { position: at })]);
        self.frame();
    }

    fn press(&mut self, at: Point, button: mouse::Button) {
        self.point(at);
        self.send(&[Event::Mouse(mouse::Event::ButtonPressed(button))]);
        self.send(&[Event::Mouse(mouse::Event::ButtonReleased(button))]);
        self.frame();
    }

    fn key(&mut self, key: keyboard::key::Named, modifiers: keyboard::Modifiers) {
        self.send(&[Event::Keyboard(keyboard::Event::KeyPressed {
            modified_key: keyboard::Key::Named(key),
            key: keyboard::Key::Named(key),
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

    fn operate(&mut self, operation: &mut dyn Operation) {
        let root = view(&self.window, &self.rows);
        let cache = self.cache.take().unwrap_or_default();
        let mut ui = UserInterface::build(root, SIZE, cache, &mut self.renderer);
        ui.operate(&self.renderer, operation);
        self.cache = Some(ui.into_cache());
    }

    fn focus(&mut self, id: Id) {
        self.operate(&mut operation::focusable::focus::<()>(id));
        self.frame();
    }

    fn focused_bounds(&mut self) -> Option<Rectangle> {
        let mut probe = FocusProbe::default();
        self.operate(&mut probe);
        probe.focused_bounds()
    }

    fn declared(&self) -> Option<Declared> {
        match self.window.chain()?.root() {
            Root::Declared(declared) => Some(declared.clone()),
            Root::Other => None,
        }
    }

    fn surfaces(&self) -> Vec<Rectangle> {
        self.window.anchors().surfaces()
    }

    /// The open menu's row labelled `label`
    fn menu_row(&self, label: &str) -> Rectangle {
        self.visible(&crate::menu_surface::row_id(
            "row-menu",
            &[],
            crate::menu_surface::row_key(label, false, 0),
        ))
        .unwrap_or_else(|| panic!("the menu's {label} is shown"))
    }
}

fn row_bounds(app: &App, name: &str) -> Rectangle {
    app.visible(&Id::new(format!("row-{name}")))
        .unwrap_or_else(|| panic!("row {name} is shown"))
}

#[test]
fn a_right_press_opens_the_rows_menu_at_the_pointer_in_place_of_an_open_menu() {
    let mut app = App::new(Rows::default());
    app.press(
        app.visible(&Id::new("other")).unwrap().center(),
        mouse::Button::Left,
    );
    assert_eq!(
        app.window.chain().map(|chain| chain.root().clone()),
        Some(Root::Other)
    );
    let a = row_bounds(&app, "A");
    let at = Point::new(a.x + 120.0, a.y + 15.0);
    app.press(at, mouse::Button::Right);
    let declared = app
        .declared()
        .expect("one right press opens the row's menu");
    assert_eq!(declared.source(), &Id::new("row-A"));
    assert_eq!(declared.invocation().entry, Entry::Pointer);
    let surfaces = app.surfaces();
    assert_eq!(surfaces.len(), 1, "one chain in the window: {surfaces:?}");
    // Its card's top-left corner is the pointer (the surface is its paint
    // box, the shadow round it)
    let inset = crate::popup_surface::shadow_padding();
    assert!(
        (surfaces[0].x + inset.left - at.x).abs() <= 1.5
            && (surfaces[0].y + inset.top - at.y).abs() <= 1.5,
        "the menu opens at the pointer: {surfaces:?} at {at:?}"
    );
    // A right press elsewhere on the row opens it there instead
    let again = Point::new(a.x + 20.0, a.y + 30.0);
    app.press(again, mouse::Button::Right);
    let surfaces = app.surfaces();
    assert_eq!(surfaces.len(), 1);
    assert!(
        (surfaces[0].x - again.x).abs() <= 12.0,
        "{surfaces:?} at {again:?}"
    );
    // Choosing an item acts on the row it was opened on, and closes it
    app.press(app.menu_row("Copy").center(), mouse::Button::Left);
    assert_eq!(app.app, [Message::Chose("Copy A".into())]);
    assert!(app.window.chain().is_none());
}

#[test]
fn the_menu_keeps_to_the_row_it_was_opened_on_when_the_rows_move() {
    let mut app = App::new(Rows::default());
    let a = row_bounds(&app, "A");
    app.press(a.center(), mouse::Button::Right);
    assert!(app.declared().is_some());
    // B comes first now: the menu follows A
    app.rows.swapped = true;
    app.frame();
    let moved = row_bounds(&app, "A");
    assert!(moved.y > a.y, "A has moved down");
    let surfaces = app.surfaces();
    assert!(
        surfaces[0].y >= moved.y,
        "the menu follows A: {surfaces:?} {moved:?}"
    );
    app.press(app.menu_row("Rename").center(), mouse::Button::Left);
    assert_eq!(app.app, [Message::Chose("Rename A".into())]);
}

#[test]
fn a_menu_whose_place_another_target_takes_closes_rather_than_act_on_it() {
    let mut app = App::new(Rows {
        placed: true,
        ..Rows::default()
    });
    let first = app.visible(&Id::new("other")).unwrap();
    let a = Rectangle::new(
        Point::new(first.x, first.y + first.height),
        Size::new(300.0, 40.0),
    );
    app.press(a.center(), mouse::Button::Right);
    assert!(
        app.declared().is_some(),
        "a row without a key has a menu too"
    );
    app.rows.swapped = true;
    app.frame();
    app.frame();
    assert!(
        app.window.chain().is_none(),
        "B in A's place: A's menu closed rather than act on B"
    );
    assert!(app.app.is_empty());
}

#[test]
fn shift_f10_opens_the_menu_of_the_focused_control_at_it() {
    let mut app = App::new(Rows::default());
    app.focus(Id::new("button-B"));
    let button = app.focused_bounds().expect("B's button has the focus");
    app.key(keyboard::key::Named::F10, keyboard::Modifiers::SHIFT);
    let declared = app
        .declared()
        .expect("Shift+F10 opens the focused row's menu");
    assert_eq!(declared.source(), &Id::new("row-B"));
    assert_eq!(declared.invocation().entry, Entry::Keyboard);
    let surfaces = app.surfaces();
    assert!(
        // The surfaces are paint boxes, their shadow round the card
        (surfaces[0].y - (button.y + button.height)).abs() <= 12.0
            && (surfaces[0].x - button.x).abs() <= 12.0,
        "below the focused control: {surfaces:?} {button:?}"
    );
    app.key(keyboard::key::Named::Escape, keyboard::Modifiers::empty());
    assert!(app.window.chain().is_none());
    // The Menu key too
    app.focus(Id::new("button-A"));
    app.key(
        keyboard::key::Named::ContextMenu,
        keyboard::Modifiers::empty(),
    );
    assert_eq!(
        app.declared().map(|d| d.source().clone()),
        Some(Id::new("row-A"))
    );
}

#[test]
fn an_open_menu_shows_what_its_source_declares_now() {
    let mut app = App::new(Rows::default());
    app.press(row_bounds(&app, "A").center(), mouse::Button::Right);
    let _ = app.menu_row("Copy");
    app.rows.label = Some("Copy all".into());
    app.frame();
    app.frame();
    assert!(app.declared().is_some(), "still open");
    let _ = app.menu_row("Copy all");
    app.press(app.menu_row("Copy all").center(), mouse::Button::Left);
    assert_eq!(app.app, [Message::Chose("Copy all A".into())]);
}

#[test]
fn a_press_a_marker_inside_takes_still_opens_the_surrounding_menu() {
    let mut app = App::new(Rows::default());
    let b = row_bounds(&app, "B");
    // The marker sits after the row's name, before its button
    let marker = Point::new(b.x + 300.0 - 30.0 - 20.0, b.y + 10.0);
    app.press(marker, mouse::Button::Right);
    assert!(
        app.app.contains(&Message::Chose("marked".into())),
        "the marker told the app"
    );
    assert_eq!(
        app.declared().map(|declared| declared.source().clone()),
        Some(Id::new("row-B")),
        "and the row's menu opened for it"
    );
}

#[test]
fn a_provided_context_root_opens_what_the_app_says_at_the_pointer() {
    let mut app = App::new(Rows::default());
    let picked = app.visible(&Id::new("picked")).expect("the area");
    app.press(picked.center(), mouse::Button::Right);
    assert_eq!(
        app.window.chain().map(|chain| chain.root().clone()),
        Some(Root::Other)
    );
    assert_eq!(
        app.window.chain().map(|chain| chain.entry()),
        Some(Entry::Pointer)
    );
}
