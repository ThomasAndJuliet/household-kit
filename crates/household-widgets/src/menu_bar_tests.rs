// SPDX-License-Identifier: GPL-3.0-only
//! A menu bar under a window root, through the native runtime: the
//! conventions libcosmic's responsive menu bar gave (a press on a title opens
//! its menu and a press on the open title closes it; the pointer moving onto
//! another title while a menu is open switches to it; Escape closes the
//! innermost menu; a press elsewhere closes it), and those of a menu bar it
//! did not have (Left and Right between menus, focusable titles opening from
//! the keyboard), in both forms the engine chooses from the titles' widths.

use super::*;
use crate::menu::{Menu, MenuItem, MenuStyle};
use crate::menu_surface::row_key;
use crate::popup::{FocusProbe, Panel};
use crate::window::{UiEvent, WindowAction, WindowUi};
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::advanced::widget::operation;
use cosmic::iced::runtime::{UserInterface, user_interface};
use cosmic::iced::widget as native;
use cosmic::iced::{Font, Pixels, Point, keyboard, window as native_window};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Root {
    File,
    Edit,
    Help,
    /// The three-bar menu of all of them
    All,
    /// A menu of another area of the window
    Other,
}

const MENUS: [Root; 3] = [Root::File, Root::Edit, Root::Help];
const NS: &str = "bar";

#[derive(Clone, Debug, PartialEq)]
enum Message {
    Chose(&'static str),
}

type Ev = UiEvent<Root, Message>;

fn title_body(text: String) -> Element<'static, Toggle> {
    native::text(text).size(14).into()
}

fn body(text: String) -> Element<'static, Ev> {
    native::text(text).size(14).into()
}

fn icon(name: &'static str) -> cosmic::widget::icon::Handle {
    cosmic::widget::icon::from_name(name).handle()
}

fn item(label: &'static str) -> MenuItem<Ev> {
    MenuItem {
        label: label.into(),
        message: UiEvent::Application(Message::Chose(label)),
        icon: None,
        shortcut: None,
        mark: None,
        disabled_reason: None,
        destructive: false,
        detail: None,
    }
}

fn menu(root: Root) -> Menu<Ev> {
    match root {
        Root::File => Menu::new()
            .item(item("New"))
            .folder("Recent", Menu::new().item(item("One")).item(item("Two"))),
        Root::Edit => Menu::new().item(item("Copy")).item(item("Paste")),
        Root::Help => Menu::new().item(item("About")),
        Root::All => Menu::new()
            .folder("File", menu(Root::File))
            .folder("Edit", menu(Root::Edit))
            .folder("Help", menu(Root::Help)),
        Root::Other => Menu::new().item(item("Elsewhere")),
    }
}

fn source(root: Root) -> Id {
    match root {
        Root::All => Id::new("bar-all"),
        Root::Other => Id::new("other"),
        menu => MenuBar::<Root>::title_id(NS, MENUS.iter().position(|m| *m == menu).unwrap()),
    }
}

/// The app's panels: the open root's menu below its title, with Left and
/// Right to the bar's neighbours
fn panels(window: &WindowUi<Root>) -> Vec<Panel<'_, Ev>> {
    use crate::layout::{Alignment, AnchorCandidate, Side};
    let Some(request) = window.chain() else {
        return Vec::new();
    };
    let root = *request.root();
    let mut panels =
        crate::window::menu_surface(menu(root), MenuStyle { icon, body }, format!("{root:?}"))
            .root_candidates(vec![AnchorCandidate::new(
                Side::Below,
                Alignment::StartBounded,
            )])
            .hover(request.on_hover_path(), request.hover_level())
            .panels(
                source(root),
                request.path().to_vec(),
                request.on_path(),
                |m| m,
            );
    if let Some((previous, next)) = MenuBar::lateral(&MENUS, &request) {
        let first = panels.remove(0).lateral(previous, next);
        panels.insert(0, first);
    }
    panels
}

fn view<'w>(window: &'w WindowUi<Root>) -> Element<'w, Ev> {
    let open = window.chain().map(|chain| *chain.root());
    let collapsed = crate::layout::child(|_| {
        crate::window::first_menu(
            Root::All,
            crate::window::trigger(
                "bar-all",
                Root::All,
                crate::popup::focus_target(
                    "bar-all",
                    native::button(native::space().width(32).height(32))
                        .padding(0)
                        .on_press(Toggle),
                )
                .on_activate(Toggle),
            ),
        )
    });
    let bar = MenuBar::new(
        NS,
        MENUS
            .iter()
            .map(|root| (format!("{root:?}"), *root))
            .collect(),
        title_body,
    )
    .open(open)
    .view(collapsed);
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
    let content: Element<'w, Message> = native::column(vec![
        native::row(vec![bar]).into(),
        native::space().height(200).into(),
        other,
    ])
    .into();
    window.mount(content, panels(window)).into()
}

struct App {
    window: WindowUi<Root>,
    size: Size,
    renderer: Renderer,
    cache: Option<user_interface::Cache>,
    cursor: mouse::Cursor,
    app: Vec<Message>,
    actions: Vec<WindowAction<Root>>,
}

impl App {
    fn new(width: f32) -> Self {
        let renderer = cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
            Font::default(),
            Pixels(14.0),
            Some("tiny-skia"),
        ))
        .expect("headless renderer");
        let mut app = Self {
            window: WindowUi::new(),
            size: Size::new(width, 400.0),
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
            let root = view(&self.window);
            let cache = self.cache.take().unwrap_or_default();
            let mut ui = UserInterface::build(root, self.size, cache, &mut self.renderer);
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
        self.send(&[Event::Window(native_window::Event::RedrawRequested(
            Instant::now(),
        ))]);
        // A chain opened or switched by that frame lays out once more
        self.send(&[Event::Window(native_window::Event::RedrawRequested(
            Instant::now(),
        ))]);
    }

    fn visible(&self, id: &Id) -> Option<Rectangle> {
        self.window.anchors().visible(id)
    }

    fn title(&self, index: usize) -> Rectangle {
        self.visible(&MenuBar::<Root>::title_id(NS, index))
            .unwrap_or_else(|| panic!("title {index} is shown"))
    }

    fn point(&mut self, at: Point) {
        self.cursor = mouse::Cursor::Available(at);
        self.send(&[Event::Mouse(mouse::Event::CursorMoved { position: at })]);
        self.frame();
    }

    fn click_at(&mut self, at: Point) {
        self.point(at);
        self.send(&[Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        ))]);
        self.send(&[Event::Mouse(mouse::Event::ButtonReleased(
            mouse::Button::Left,
        ))]);
        self.frame();
    }

    /// The pointer at `at`, resting there longer than a submenu's delay
    fn rest(&mut self, at: Point) {
        self.point(at);
        self.send(&[Event::Window(native_window::Event::RedrawRequested(
            Instant::now() + crate::menu_surface::SUBMENU_DELAY * 2,
        ))]);
        self.frame();
    }

    fn row(&self, menu: Root, label: &str) -> Rectangle {
        self.visible(&crate::menu_surface::row_id(
            &format!("{menu:?}"),
            &[],
            row_key(label, label == "Recent", 0),
        ))
        .unwrap_or_else(|| panic!("{label} is shown"))
    }

    fn named(&mut self, key: keyboard::key::Named) {
        self.send(&[Event::Keyboard(keyboard::Event::KeyPressed {
            modified_key: keyboard::Key::Named(key),
            key: keyboard::Key::Named(key),
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

    fn operate(&mut self, operation: &mut dyn Operation) {
        let root = view(&self.window);
        let cache = self.cache.take().unwrap_or_default();
        let mut ui = UserInterface::build(root, self.size, cache, &mut self.renderer);
        ui.operate(&self.renderer, operation);
        self.cache = Some(ui.into_cache());
    }

    fn focus(&mut self, id: Id) {
        self.operate(&mut operation::focusable::focus::<()>(id));
        self.frame();
    }

    fn focused(&mut self) -> Option<Id> {
        let mut probe = FocusProbe::default();
        self.operate(&mut probe);
        probe.focused
    }

    fn open(&self) -> Option<Root> {
        self.window.chain().map(|chain| *chain.root())
    }

    fn surfaces(&self) -> Vec<Rectangle> {
        self.window.anchors().surfaces()
    }
}

/// Wide enough for the titles
const WIDE: f32 = 600.0;

#[test]
fn a_press_on_a_title_opens_its_menu_below_it_and_a_press_on_it_again_closes_it() {
    let mut app = App::new(WIDE);
    assert!(
        app.visible(&Id::new("bar-all")).is_none(),
        "titles, not the three-bar menu, where they fit"
    );
    let file = app.title(0);
    app.click_at(file.center());
    assert_eq!(app.open(), Some(Root::File));
    let surfaces = app.surfaces();
    assert_eq!(surfaces.len(), 1);
    assert!(
        surfaces[0].y >= file.y + file.height - 1.0 && (surfaces[0].x - file.x).abs() < 12.0,
        "the menu hangs below its title, at its start: {surfaces:?} under {file:?}"
    );
    app.click_at(file.center());
    assert_eq!(
        app.open(),
        None,
        "a press on the open menu's title closes it"
    );
    assert!(app.surfaces().is_empty());
}

#[test]
fn the_pointer_moving_onto_another_title_while_a_menu_is_open_switches_to_it() {
    let mut app = App::new(WIDE);
    // With no menu open, passing over the titles opens nothing
    app.point(app.title(1).center());
    assert_eq!(app.open(), None);
    app.click_at(app.title(0).center());
    assert_eq!(app.open(), Some(Root::File));
    app.point(app.title(1).center());
    assert_eq!(app.open(), Some(Root::Edit), "moving onto Edit opens Edit");
    let edit = app.title(1);
    let surfaces = app.surfaces();
    assert_eq!(surfaces.len(), 1, "one menu: {surfaces:?}");
    assert!(
        surfaces[0].x >= edit.x - 12.0,
        "under Edit: {surfaces:?} {edit:?}"
    );
    app.point(app.title(2).center());
    assert_eq!(app.open(), Some(Root::Help));
    // Back over the open title, and on into its menu, nothing changes
    let sessions = app.actions.len();
    app.point(app.title(2).center());
    let menu = app.surfaces()[0];
    app.point(menu.center());
    assert_eq!(app.open(), Some(Root::Help));
    assert_eq!(
        app.actions.len(),
        sessions,
        "no action while on the open menu's title or in it"
    );
}

#[test]
fn another_areas_menu_does_not_switch_to_a_title_the_pointer_passes() {
    let mut app = App::new(WIDE);
    app.click_at(app.visible(&Id::new("other")).unwrap().center());
    assert_eq!(app.open(), Some(Root::Other));
    app.point(app.title(0).center());
    assert_eq!(
        app.open(),
        Some(Root::Other),
        "only the bar's own menus switch by pointing"
    );
    // A press on a title still opens its menu in that one's place
    app.click_at(app.title(0).center());
    assert_eq!(app.open(), Some(Root::File));
}

#[test]
fn escape_closes_the_innermost_menu_and_focus_returns_to_the_title() {
    let mut app = App::new(WIDE);
    app.focus(MenuBar::<Root>::title_id(NS, 0));
    app.named(keyboard::key::Named::Enter);
    assert_eq!(
        app.open(),
        Some(Root::File),
        "Enter on a focused title opens its menu"
    );
    // Down to Recent, and Right opens its submenu, not the next menu
    app.named(keyboard::key::Named::ArrowDown);
    app.named(keyboard::key::Named::ArrowRight);
    assert_eq!(app.open(), Some(Root::File));
    assert_eq!(
        app.window.chain().unwrap().path(),
        [row_key("Recent", true, 0)]
    );
    app.named(keyboard::key::Named::Escape);
    assert_eq!(
        app.open(),
        Some(Root::File),
        "Escape closes the submenu first"
    );
    assert!(app.window.chain().unwrap().path().is_empty());
    app.named(keyboard::key::Named::Escape);
    assert_eq!(app.open(), None);
    app.frame();
    assert_eq!(
        app.focused(),
        Some(MenuBar::<Root>::title_id(NS, 0)),
        "focus is back on File's title"
    );
}

#[test]
fn left_and_right_move_between_the_bars_menus_and_wrap_at_its_ends() {
    let mut app = App::new(WIDE);
    app.focus(MenuBar::<Root>::title_id(NS, 0));
    app.named(keyboard::key::Named::Enter);
    assert_eq!(app.open(), Some(Root::File));
    app.named(keyboard::key::Named::ArrowRight);
    assert_eq!(
        app.open(),
        Some(Root::Edit),
        "Right on a plain row: the next menu"
    );
    let target = app.focused();
    assert!(
        target
            .as_ref()
            .is_some_and(|id| format!("{id:?}").contains("Edit:menu")),
        "the keyboard's target is Edit's first row: {target:?}"
    );
    app.named(keyboard::key::Named::ArrowRight);
    assert_eq!(app.open(), Some(Root::Help));
    app.named(keyboard::key::Named::ArrowRight);
    assert_eq!(app.open(), Some(Root::File), "wrapping after the last");
    app.named(keyboard::key::Named::ArrowLeft);
    assert_eq!(app.open(), Some(Root::Help), "and before the first");
    // Left inside a submenu closes it rather than moving
    app.named(keyboard::key::Named::ArrowRight);
    app.named(keyboard::key::Named::ArrowDown);
    app.named(keyboard::key::Named::ArrowRight);
    assert_eq!(app.window.chain().unwrap().path().len(), 1);
    app.named(keyboard::key::Named::ArrowLeft);
    assert_eq!(app.open(), Some(Root::File));
    assert!(app.window.chain().unwrap().path().is_empty());
    assert!(app.app.is_empty(), "nothing was activated");
}

#[test]
fn a_press_outside_closes_the_menu_and_is_kept_from_what_is_under_it() {
    let mut app = App::new(WIDE);
    app.click_at(app.title(1).center());
    assert_eq!(app.open(), Some(Root::Edit));
    app.click_at(Point::new(WIDE - 10.0, 390.0));
    assert_eq!(app.open(), None);
}

#[test]
fn where_the_titles_do_not_fit_one_three_bar_menu_holds_them_all() {
    let mut app = App::new(110.0);
    assert!(
        app.visible(&MenuBar::<Root>::title_id(NS, 0)).is_none(),
        "no title where they do not all fit"
    );
    let all = app
        .visible(&Id::new("bar-all"))
        .expect("the three-bar button");
    app.click_at(all.center());
    assert_eq!(app.open(), Some(Root::All));
    // Its menus are submenus: neither Left nor Right leaves it
    app.named(keyboard::key::Named::ArrowLeft);
    assert_eq!(app.open(), Some(Root::All));
    // Wide again: the titles, never both
    let mut wide = App::new(WIDE);
    assert!(wide.visible(&Id::new("bar-all")).is_none());
    wide.frame();
}

#[test]
fn resting_on_a_submenus_row_opens_it_beside_the_menu_and_resting_elsewhere_closes_it() {
    let mut app = App::new(WIDE);
    app.click_at(app.title(0).center());
    let recent = app.row(Root::File, "Recent");
    // Passing over it opens nothing (no redraw: under load a frame can
    // come later than the delay)
    app.cursor = mouse::Cursor::Available(recent.center());
    app.send(&[Event::Mouse(mouse::Event::CursorMoved {
        position: recent.center(),
    })]);
    assert!(app.window.chain().unwrap().path().is_empty(), "not at once");
    app.rest(recent.center());
    assert_eq!(
        app.window.chain().unwrap().path(),
        [row_key("Recent", true, 0)],
        "after the delay its submenu opens"
    );
    let surfaces = app.surfaces();
    assert_eq!(surfaces.len(), 2, "beside the menu: {surfaces:?}");
    assert!(
        surfaces[1].x > surfaces[0].x + surfaces[0].width / 2.0,
        "a card of its own, beside: {surfaces:?}"
    );
    // A click on the row it opened keeps it open
    app.click_at(recent.center());
    assert_eq!(app.window.chain().unwrap().path().len(), 1);
    assert!(app.app.is_empty());
    // Resting on another row of the menu closes it again
    app.rest(app.row(Root::File, "New").center());
    assert!(app.window.chain().unwrap().path().is_empty());
    assert_eq!(app.surfaces().len(), 1);
    assert!(app.app.is_empty(), "resting activates nothing");
}

#[test]
fn resting_never_opens_a_submenu_inline_where_there_is_no_room_beside() {
    let mut app = App::new(110.0);
    let all = app.visible(&Id::new("bar-all")).unwrap();
    app.click_at(all.center());
    assert_eq!(app.open(), Some(Root::All));
    let file = app
        .visible(&crate::menu_surface::row_id(
            "All",
            &[],
            row_key("File", true, 0),
        ))
        .expect("File's row");
    app.rest(file.center());
    assert_eq!(app.open(), Some(Root::All), "the menu stays open");
    assert!(
        app.window.chain().unwrap().path().is_empty(),
        "and its submenu, with no room beside, does not open inline"
    );
    assert_eq!(app.surfaces().len(), 1);
    // A click does expand it inline
    app.click_at(file.center());
    assert_eq!(
        app.window.chain().unwrap().path(),
        [row_key("File", true, 0)]
    );
    assert_eq!(app.surfaces().len(), 1, "inline, in the same card");
    // And resting on another row leaves the inline expansion alone
    let help = app
        .visible(&crate::menu_surface::row_id(
            "All",
            &[],
            row_key("Help", true, 0),
        ))
        .expect("Help's row");
    app.rest(help.center());
    assert_eq!(
        app.window.chain().unwrap().path(),
        [row_key("File", true, 0)],
        "an inline expansion closes only explicitly"
    );
}

#[test]
fn f10_opens_the_first_menu_from_the_keyboard_and_closes_it() {
    let mut app = App::new(WIDE);
    app.named(keyboard::key::Named::F10);
    assert_eq!(app.open(), Some(Root::File));
    let chain = app.window.chain().unwrap();
    assert_eq!(chain.entry(), Entry::Keyboard);
    let target = app.focused();
    assert!(
        target
            .as_ref()
            .is_some_and(|id| format!("{id:?}").contains("File:menu")),
        "its first row is the keyboard's target: {target:?}"
    );
    app.named(keyboard::key::Named::F10);
    assert_eq!(app.open(), None, "F10 again closes it");
    // Where the titles do not fit, the three-bar menu is the first
    let mut narrow = App::new(110.0);
    narrow.named(keyboard::key::Named::F10);
    assert_eq!(narrow.open(), Some(Root::All));
}

/// Pin (Standard density): a title is its text in 12 px at the sides and
/// `space_l` (32) tall (was 27.6: 4 above and below its 19.6 line)
#[test]
fn pin_standard_density_title_geometry() {
    let app = App::new(900.0);
    let title = app.title(0);
    let text = crate::menu::measured_width("File", 14.0, cosmic::font::default());
    assert!((title.width - (text + 24.0)).abs() < 0.01, "{title:?}");
    assert!((title.height - 32.0).abs() < 0.01, "{title:?}");
}

/// At Compact density a title is padded by the Compact steps: 8 at the sides,
/// and it is `space_l` (24) tall
#[test]
fn a_title_follows_the_density() {
    let compact = cosmic::cosmic_theme::Spacing::from(cosmic::cosmic_theme::Density::Compact);
    let padding = title_padding(&compact);
    assert_eq!((padding.top, padding.left), (0.0, 8.0));
    assert_eq!(crate::spacing::control_height(&compact), 24.0);
    assert_eq!(
        title_padding(&cosmic::cosmic_theme::Spacing::default()).left,
        12.0
    );
}
