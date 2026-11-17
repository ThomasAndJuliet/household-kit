// SPDX-License-Identifier: GPL-3.0-only
//! The window owner through the native runtime: independent areas mounted
//! under one root, with real events, the messages fed back to the window as
//! an app would, and assertions on its state, messages and geometry.

use super::*;
use crate::focus_regions::Group;
use crate::layout::{Alignment, AnchorCandidate, Attachment, Level, Side, Stage, child};
use crate::list_navigation::Axis;
use crate::popup::{FocusProbe, FocusTarget, INFORMATION_DELAY, focus_target};
use crate::scroll_list::{Estimate, EstimateClass, ItemSpec, ListAction, ListChild, ListState};
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::advanced::widget::operation;
use cosmic::iced::event::Status;
use cosmic::iced::runtime::{UserInterface, user_interface};
use cosmic::iced::widget as native;
use cosmic::iced::{Font, Pixels, Point, keyboard, window as native_window};
use cosmic::widget;
use std::time::Instant;

#[derive(Clone, Debug, PartialEq)]
enum Root {
    A,
    B,
}

#[derive(Clone, Debug)]
enum Message {
    Chose(&'static str),
    Pressed,
    Typed,
    List(ListAction),
}

type Ev = UiEvent<Root, Message>;

/// What the app shows, besides its window state
struct Scene {
    a_trigger: bool,
    a_info: bool,
    editor: native::text_editor::Content<Renderer>,
    list: ListState<u64>,
    protected: bool,
}

impl Scene {
    fn new() -> Self {
        Self {
            a_trigger: true,
            a_info: true,
            editor: native::text_editor::Content::new(),
            list: ListState::new(),
            protected: false,
        }
    }
}

fn help<'w, M: 'w>() -> crate::layout::Child<'w, M> {
    child(|_| native::text("Help").into())
}

fn below() -> Vec<AnchorCandidate> {
    vec![AnchorCandidate::new(Side::Below, Alignment::StartBounded)]
}

/// A control in an area, with information
fn info<'w>(id: &'static str) -> Element<'w, Message> {
    hover_box(
        id,
        focus_target(id, native::space().width(120).height(30)),
        help(),
        below(),
    )
}

/// A trigger of a provided root
fn opener<'w>(id: &'static str, root: Root) -> Element<'w, Message> {
    trigger(
        id,
        root,
        focus_target(
            id,
            native::button(native::space().width(60).height(30))
                .padding(0)
                .on_press(Toggle),
        )
        .on_activate(Toggle),
    )
}

/// The app's panels for the open root
fn panels(window: &WindowUi<Root>) -> Vec<Panel<'_, Ev>> {
    let Some(request) = window.chain() else {
        return Vec::new();
    };
    let (source, row) = match request.root() {
        Root::A => ("a", "a-row"),
        Root::B => ("b", "b-row"),
    };
    let chose = UiEvent::Application(Message::Chose(row));
    let action = chose.clone();
    vec![
        Panel::new(
            Attachment::below(Rectangle::default()),
            vec![Stage::new(
                Level::Preferred,
                child(move |_| {
                    focus_target(
                        row,
                        native::button(native::space().width(120).height(40))
                            .padding(0)
                            .on_press(action.clone()),
                    )
                    .on_activate(action.clone())
                    .into()
                }),
            )],
        )
        .source(source)
        .focus_first()
        .focus_targets(vec![FocusTarget::new(row).activate(chose)]),
    ]
}

fn row_id(i: usize) -> Id {
    Id::new(format!("row-{i}"))
}

fn row_help(i: usize) -> Id {
    Id::new(format!("row-help-{i}"))
}

/// Three areas built independently: A (a trigger and an informing
/// control), B (an editor with information, a trigger and a tool, in focus
/// regions) and a list whose rows inform
fn view<'w>(window: &'w WindowUi<Root>, scene: &'w Scene) -> Element<'w, Ev> {
    let space = || -> Element<'w, Message> { native::space().width(60).height(30).into() };
    let area_a = native::row(vec![
        if scene.a_trigger {
            opener("a", Root::A)
        } else {
            space()
        },
        native::space().width(10).into(),
        if scene.a_info {
            info("a-info")
        } else {
            space()
        },
    ]);
    let editor = hover_box(
        "b-editor-help",
        native::text_editor(&scene.editor)
            .id("b-editor")
            .on_action(|_| Message::Typed)
            .height(60),
        help(),
        below(),
    );
    let tools = native::row(vec![
        opener("b", Root::B),
        focus_target(
            "b-tool",
            native::button(native::space().width(60).height(30))
                .padding(0)
                .on_press(Message::Pressed),
        )
        .into(),
    ]);
    let area_b = focus_regions(
        native::column(vec![editor, tools.into()]),
        Id::new("b-editor"),
        vec![Group::new(
            Axis::Horizontal,
            vec![Id::new("b"), Id::new("b-tool")],
        )],
    );
    let spec = |i: usize| ItemSpec {
        key: i as u64,
        revision: 1,
        class: EstimateClass(0),
        gap: 0.0,
        shown: true,
        navigable: true,
    };
    let list = crate::scroll_list::scroll_list(&scene.list)
        .items(
            1,
            20,
            spec,
            |_| Estimate::Fixed { ems: 40.0 / 15.0 },
            |i| {
                ListChild::new(hover_box(
                    row_help(i),
                    widget::button::custom(native::space().width(Length::Fill).height(40))
                        .id(row_id(i))
                        .on_press(Message::Pressed),
                    help(),
                    vec![AnchorCandidate::new(Side::Left, Alignment::StartBounded)],
                ))
            },
        )
        .on_action(Message::List);
    let content = native::row(vec![
        native::container(area_a).width(200).into(),
        native::container(area_b).width(200).into(),
        native::container(list).width(200).height(200).into(),
    ]);
    window
        .mount(content, panels(window))
        .protected_keys(scene.protected)
        .into()
}

/// An app with one window, delivering real events and feeding the
/// window's own actions back to it
struct App {
    window: WindowUi<Root>,
    scene: Scene,
    renderer: Renderer,
    cache: Option<user_interface::Cache>,
    cursor: mouse::Cursor,
    app: Vec<Message>,
    actions: Vec<WindowAction<Root>>,
}

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
            scene: Scene::new(),
            renderer,
            cache: None,
            cursor: mouse::Cursor::Unavailable,
            app: Vec::new(),
            actions: Vec::new(),
        };
        app.frame(Instant::now());
        app
    }

    /// Deliver `events` to a view built afresh, as the runtime does after
    /// each update, then apply what the window and the list published
    fn send(&mut self, events: &[Event]) -> Vec<Status> {
        let mut published = Vec::new();
        let mut statuses = Vec::new();
        {
            let root = view(&self.window, &self.scene);
            let cache = self.cache.take().unwrap_or_default();
            let mut ui =
                UserInterface::build(root, Size::new(600.0, 400.0), cache, &mut self.renderer);
            for event in events {
                let (_, status) = ui.update(
                    std::slice::from_ref(event),
                    self.cursor,
                    &mut self.renderer,
                    &mut cosmic::iced::advanced::clipboard::Null,
                    &mut published,
                );
                statuses.extend(status);
            }
            self.cache = Some(ui.into_cache());
        }
        assert!(current().is_none(), "no window stays current");
        for event in published {
            match event {
                UiEvent::Internal(action) => {
                    self.actions.push(action.clone());
                    self.window.apply(action);
                }
                UiEvent::Application(Message::List(ListAction::Scrolled(viewport))) => {
                    drop(self.scene.list.apply::<()>(ListAction::Scrolled(viewport)));
                }
                // The editor's own actions are not under test
                UiEvent::Application(Message::Typed) => {}
                UiEvent::Application(message) => self.app.push(message),
            }
        }
        statuses
    }

    fn frame(&mut self, at: Instant) {
        self.send(&[Event::Window(native_window::Event::RedrawRequested(at))]);
    }

    fn visible(&self, id: &str) -> Rectangle {
        self.window
            .anchors()
            .visible(&Id::new(id.to_owned()))
            .unwrap_or_else(|| panic!("{id} is visible"))
    }

    fn point(&mut self, at: Point) {
        self.cursor = mouse::Cursor::Available(at);
        self.send(&[Event::Mouse(mouse::Event::CursorMoved { position: at })]);
    }

    /// A press on source `id` and, after the app updated, its release
    fn click(&mut self, id: &str) {
        let at = self.visible(id).center();
        self.click_at(at);
    }

    fn click_at(&mut self, at: Point) {
        self.point(at);
        self.send(&[Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        ))]);
        self.send(&[Event::Mouse(mouse::Event::ButtonReleased(
            mouse::Button::Left,
        ))]);
        self.frame(Instant::now());
    }

    fn key(&mut self, key: keyboard::Key, modifiers: keyboard::Modifiers) -> Vec<Status> {
        let statuses = self.send(&[Event::Keyboard(keyboard::Event::KeyPressed {
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
        self.frame(Instant::now());
        statuses
    }

    fn named(&mut self, key: keyboard::key::Named) -> Vec<Status> {
        self.key(keyboard::Key::Named(key), keyboard::Modifiers::empty())
    }

    fn operate(&mut self, operation: &mut dyn Operation) {
        let root = view(&self.window, &self.scene);
        let cache = self.cache.take().unwrap_or_default();
        let mut ui = UserInterface::build(root, Size::new(600.0, 400.0), cache, &mut self.renderer);
        ui.operate(&self.renderer, operation);
        self.cache = Some(ui.into_cache());
    }

    fn focus(&mut self, id: &str) {
        self.operate(&mut operation::focusable::focus::<()>(Id::new(
            id.to_owned(),
        )));
        self.frame(Instant::now());
    }

    fn focused(&mut self) -> Option<Id> {
        let mut probe = FocusProbe::default();
        self.operate(&mut probe);
        probe.focused
    }

    /// Let the information delay pass
    fn wait(&mut self) {
        self.frame(Instant::now() + INFORMATION_DELAY);
    }

    fn informs(&self, id: &str) -> bool {
        self.window
            .anchors()
            .information_visible(&Id::new(id.to_owned()))
    }

    fn open_root(&self) -> Option<Root> {
        self.window.chain().map(|chain| chain.root().clone())
    }

    fn is_open(&self, id: &str) -> bool {
        self.window.anchors().is_open(&Id::new(id.to_owned()))
    }
}

#[test]
fn opening_a_root_in_one_area_replaces_the_other_areas_chain() {
    let mut app = App::new();
    app.click("a");
    assert_eq!(app.open_root(), Some(Root::A));
    assert!(app.is_open("a"));
    let surfaces = app.window.anchors().surfaces();
    assert_eq!(surfaces.len(), 1);
    let a = app.visible("a");
    assert!(
        surfaces[0].y >= a.y + a.height - 1.0 && surfaces[0].x < a.x + a.width,
        "A's menu hangs below its trigger: {surfaces:?} under {a:?}"
    );
    // One press on B's trigger, in another area, closes A's chain and opens
    // B's: one chain in the window
    app.click("b");
    assert_eq!(app.open_root(), Some(Root::B));
    assert!(app.is_open("b") && !app.is_open("a"));
    let surfaces = app.window.anchors().surfaces();
    let b = app.visible("b");
    assert_eq!(surfaces.len(), 1, "one chain: {surfaces:?}");
    assert!(
        surfaces[0].y >= b.y + b.height - 1.0 && surfaces[0].x >= b.x - 1.0,
        "B's menu hangs below B's trigger: {surfaces:?} under {b:?}"
    );
    // Its own trigger closes it
    app.click("b");
    assert_eq!(app.open_root(), None);
    assert!(app.window.anchors().surfaces().is_empty());
}

#[test]
fn activating_a_row_closes_the_chain_before_routing_its_action() {
    let mut app = App::new();
    app.click("a");
    app.actions.clear();
    let menu = app.window.anchors().surfaces()[0];
    app.click_at(menu.center());
    assert_eq!(app.open_root(), None, "activation closes the chain");
    assert!(
        matches!(
            app.actions.as_slice(),
            [WindowAction(Internal::Activated { .. })]
        ),
        "the window learns of the activation: {:?}",
        app.actions
    );
    assert!(
        matches!(app.app.as_slice(), [Message::Chose("a-row")]),
        "and the row's action reaches the app: {:?}",
        app.app
    );
    // From the keyboard too: Enter on the first row
    app.app.clear();
    app.window.open(Root::B, Entry::Keyboard);
    app.frame(Instant::now());
    assert_eq!(app.focused(), Some(Id::new("b-row")));
    app.named(keyboard::key::Named::Enter);
    assert_eq!(app.open_root(), None);
    assert!(matches!(app.app.as_slice(), [Message::Chose("b-row")]));
}

#[test]
fn an_exposed_control_informs_while_another_areas_menu_is_open() {
    let mut app = App::new();
    app.click("b");
    assert_eq!(app.open_root(), Some(Root::B));
    let menu = app.window.anchors().surfaces();
    let control = app.visible("a-info");
    assert!(
        menu.iter()
            .all(|surface| surface.intersection(&control).is_none()),
        "A's control is exposed"
    );
    app.point(control.center());
    assert_eq!(app.window.anchors().hovered(), Some(Id::new("a-info")));
    app.wait();
    assert!(app.informs("a-info"), "its information shows");
    assert_eq!(app.open_root(), Some(Root::B), "and B's menu stays");
    assert_eq!(app.window.anchors().surfaces(), menu, "where it was");
}

#[test]
fn removing_a_source_cancels_its_pending_information_and_its_chain() {
    let mut app = App::new();
    let control = app.visible("a-info");
    app.point(control.center());
    assert_eq!(app.window.anchors().hovered(), Some(Id::new("a-info")));
    // The control goes before the delay passes
    app.scene.a_info = false;
    app.frame(Instant::now());
    assert_eq!(app.window.anchors().hovered(), None);
    app.wait();
    assert!(!app.informs("a-info"), "nothing shows for a removed source");
    // A chain whose trigger goes closes, as having no fit
    app.click("a");
    assert_eq!(app.open_root(), Some(Root::A));
    app.actions.clear();
    app.scene.a_trigger = false;
    app.frame(Instant::now());
    app.frame(Instant::now());
    assert_eq!(app.open_root(), None);
    assert!(
        app.actions
            .iter()
            .any(|action| matches!(action, WindowAction(Internal::NoFit { depth: 0, .. }))),
        "{:?}",
        app.actions
    );
}

/// A pointer-owned card on A's control
fn pointer_card(app: &mut App) {
    let control = app.visible("a-info");
    app.point(control.center());
    app.wait();
    assert!(app.informs("a-info"));
}

#[test]
fn escape_in_an_editor_goes_to_the_editor_and_a_pointer_card_stays() {
    let mut app = App::new();
    app.focus("b-editor");
    assert_eq!(app.focused(), Some(Id::new("b-editor")));
    pointer_card(&mut app);
    app.named(keyboard::key::Named::Escape);
    assert!(app.informs("a-info"), "the pointer's card stays");
    assert_eq!(app.focused(), None, "the editor took its Escape");
    // Focus on nothing in particular: Escape now dismisses the card
    app.named(keyboard::key::Named::Escape);
    assert!(!app.informs("a-info"));
}

#[test]
fn escape_on_an_ordinary_control_dismisses_a_pointer_card() {
    let mut app = App::new();
    app.focus("b-tool");
    pointer_card(&mut app);
    let statuses = app.named(keyboard::key::Named::Escape);
    assert!(!app.informs("a-info"));
    assert_eq!(statuses, [Status::Captured]);
    assert_eq!(app.focused(), Some(Id::new("b-tool")), "focus stays");
    // Suppressed for the unchanged target
    app.wait();
    assert!(!app.informs("a-info"));
}

#[test]
fn escape_dismisses_a_keyboard_card_before_the_editor() {
    let mut app = App::new();
    app.focus("b-tool");
    // F6 takes the keyboard to the editor, whose information follows
    app.named(keyboard::key::Named::F6);
    assert_eq!(app.focused(), Some(Id::new("b-editor")));
    assert_eq!(
        app.window.anchors().hovered(),
        Some(Id::new("b-editor-help"))
    );
    app.wait();
    assert!(app.informs("b-editor-help"));
    app.named(keyboard::key::Named::Escape);
    assert!(!app.informs("b-editor-help"), "the card goes first");
    assert_eq!(
        app.focused(),
        Some(Id::new("b-editor")),
        "and the editor never saw the Escape"
    );
}

#[test]
fn escape_with_a_menu_open_dismisses_information_then_the_menu() {
    let mut app = App::new();
    app.click("a");
    pointer_card(&mut app);
    app.named(keyboard::key::Named::Escape);
    assert!(!app.informs("a-info"));
    assert_eq!(app.open_root(), Some(Root::A), "information first");
    app.named(keyboard::key::Named::Escape);
    assert_eq!(app.open_root(), None);
    assert_eq!(
        app.focused(),
        Some(Id::new("a")),
        "focus returns to the trigger"
    );
}

#[test]
fn a_wheel_outside_the_chain_dismisses_it_and_is_consumed() {
    let mut app = App::new();
    app.click("a");
    let menu = app.window.anchors().surfaces()[0];
    // Inside the menu, the wheel is the menu's
    app.point(menu.center());
    let wheel = Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 },
    });
    app.send(std::slice::from_ref(&wheel));
    assert_eq!(app.open_root(), Some(Root::A));
    // Over the list, it closes the menu and the list does not scroll
    let list = app.visible("row-help-2");
    app.point(list.center());
    let start = app.scene.list.view_start();
    let statuses = app.send(std::slice::from_ref(&wheel));
    assert_eq!(statuses, [Status::Captured]);
    assert_eq!(app.open_root(), None);
    app.frame(Instant::now());
    assert_eq!(app.scene.list.view_start(), start, "the wheel was consumed");
}

#[test]
fn a_lists_keyboard_target_works_under_the_root_and_its_information_follows() {
    use keyboard::key::Named;
    let mut app = App::new();
    app.focus("row-2");
    assert_eq!(app.scene.list.target(), Some(2));
    app.named(Named::ArrowDown);
    assert_eq!(app.scene.list.target(), Some(3), "arrows are the list's");
    assert_eq!(app.focused(), Some(row_id(3)));
    assert_eq!(app.window.anchors().hovered(), Some(row_help(3)));
    // Page Down moves the target a page on; information follows it
    app.named(Named::PageDown);
    let target = app.scene.list.target().expect("a target");
    assert!(target > 3, "a page on: {target}");
    assert_eq!(app.focused(), Some(row_id(target as usize)));
    assert_eq!(
        app.window.anchors().hovered(),
        Some(row_help(target as usize))
    );
    app.wait();
    assert!(app.informs(&format!("row-help-{target}")));
    // Escape dismisses the keyboard's card and keeps the target
    app.named(Named::Escape);
    assert!(!app.informs(&format!("row-help-{target}")));
    assert_eq!(app.scene.list.target(), Some(target));
    app.named(Named::PageUp);
    let back = app.scene.list.target().expect("a target");
    assert!(back < target);
    assert_eq!(
        app.window.anchors().hovered(),
        Some(row_help(back as usize))
    );
    // Tab leaves the list: the app's, not the root's
    app.app.clear();
    let statuses = app.named(Named::Tab);
    assert_eq!(statuses, [Status::Captured]);
    assert!(
        matches!(
            app.app.as_slice(),
            [Message::List(ListAction::Leave { forward: true })]
        ),
        "{:?}",
        app.app
    );
    // With a menu open, arrows drive the menu, not the list behind it
    app.focus(&format!("row-{back}"));
    app.window.open(Root::A, Entry::Keyboard);
    app.frame(Instant::now());
    app.named(Named::ArrowDown);
    assert_eq!(app.scene.list.target(), Some(back));
    assert_eq!(app.focused(), Some(Id::new("a-row")));
}

#[test]
fn protected_keys_leave_escape_and_alt_h_to_the_protected_input() {
    let mut app = App::new();
    app.focus("b-editor");
    pointer_card(&mut app);
    app.scene.protected = true;
    let statuses = app.named(keyboard::key::Named::Escape);
    assert_eq!(statuses, [Status::Ignored], "Escape is left to the page");
    assert!(app.informs("a-info"));
    let statuses = app.key(
        keyboard::Key::Character("h".into()),
        keyboard::Modifiers::ALT,
    );
    assert_eq!(statuses, [Status::Ignored], "and Alt+H");
    app.scene.protected = false;
    // Escape left the editor and dismissed the pointer card
    app.named(keyboard::key::Named::Escape);
    assert!(!app.informs("a-info"));
    app.focus("b-editor");
    let statuses = app.key(
        keyboard::Key::Character("h".into()),
        keyboard::Modifiers::ALT,
    );
    assert_eq!(statuses, [Status::Captured]);
}

#[test]
fn two_windows_have_independent_owners() {
    let mut main = App::new();
    let mut other = App::new();
    main.click("a");
    other.frame(Instant::now());
    assert_eq!(main.open_root(), Some(Root::A));
    assert_eq!(other.open_root(), None, "a chain belongs to its window");
    // Information in the other window, while main's menu stays
    pointer_card(&mut other);
    assert!(other.informs("a-info"));
    assert_eq!(main.window.anchors().hovered(), None);
    main.frame(Instant::now());
    assert_eq!(main.open_root(), Some(Root::A));
    // Closing one window's chain leaves the other's information
    main.named(keyboard::key::Named::Escape);
    assert_eq!(main.open_root(), None);
    assert!(other.informs("a-info"));
}

#[test]
fn an_earlier_sessions_actions_are_ignored() {
    let mut window: WindowUi<Root> = WindowUi::new();
    window.open(Root::A, Entry::Pointer);
    let stale = window.chain().unwrap().on_path::<()>()(vec![7]);
    let UiEvent::Internal(stale) = stale else {
        unreachable!()
    };
    window.open(Root::B, Entry::Pointer);
    window.apply(stale);
    assert_eq!(window.chain().unwrap().path(), &[] as &[usize]);
    let expand = window.chain().unwrap().expand::<()>(3, true);
    let UiEvent::Internal(expand) = expand else {
        unreachable!()
    };
    window.apply(expand.clone());
    assert!(window.chain().unwrap().is_expanded(3));
    window.close();
    window.open(Root::B, Entry::Pointer);
    window.apply(expand);
    assert!(
        !window.chain().unwrap().is_expanded(3),
        "a disclosure belongs to its session"
    );
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "outside any window root")]
fn a_declaration_outside_any_window_root_is_a_construction_error() {
    let mut renderer = cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
        Font::default(),
        Pixels(14.0),
        Some("tiny-skia"),
    ))
    .expect("headless renderer");
    let content: Element<'_, Message> = info("stray");
    let mut ui = UserInterface::build(
        content,
        Size::new(200.0, 100.0),
        user_interface::Cache::new(),
        &mut renderer,
    );
    ui.update(
        &[Event::Window(native_window::Event::RedrawRequested(
            Instant::now(),
        ))],
        mouse::Cursor::Unavailable,
        &mut renderer,
        &mut cosmic::iced::advanced::clipboard::Null,
        &mut Vec::new(),
    );
}
