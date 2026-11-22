// SPDX-License-Identifier: GPL-3.0-only
//! The "show the rest" row of a menu ([`Row::More`]): hidden until expanded,
//! expanded in place without closing or widening the menu, keyed uniquely.
use super::*;
use crate::menu::MenuItem;
use cosmic::iced::advanced::{
    mouse,
    widget::{Operation, operation},
};
use cosmic::iced::runtime::{UserInterface, user_interface};
use cosmic::iced::{Event, keyboard};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
enum Message {
    Path(Vec<usize>),
    Close(usize),
    Action(&'static str),
    Dismiss,
    NoFit(usize),
    Expand(usize, bool),
}

fn renderer() -> cosmic::Renderer {
    crate::test_fonts::renderer().expect("native renderer")
}
fn item(label: &'static str) -> MenuItem<Message> {
    MenuItem::new(label, Message::Action(label))
}
fn icon(name: &'static str) -> widget::icon::Handle {
    widget::icon::from_name(name).handle()
}
fn body(text: String) -> Element<'static, Message> {
    widget::text(text).size(14).into()
}
const LONG: &str = "A hidden choice with a label much longer than the shown rows";

/// The newest model of each family, and the rest behind a More
fn models() -> Menu<Message> {
    Menu::new()
        .item(item("Opus 5.5"))
        .item(item("Sonnet 5.5"))
        .more(
            "Show older models",
            Menu::new().item(item("Opus 5.1")).item(item(LONG)),
        )
}
fn more_expansion(path: &[usize]) -> usize {
    expansion_key(path, more_key(None, "Show older models", 0))
}
fn surface(menu: Menu<Message>, anchors: &Anchors, expanded: &[usize]) -> MenuSurface<Message> {
    MenuSurface::new(
        menu,
        MenuStyle { icon, body },
        anchors.clone(),
        "more".into(),
    )
    .disclosures(
        expanded.iter().copied().collect::<BTreeSet<_>>(),
        |row, open| Message::Expand(row, open),
    )
}
fn view(
    anchors: &Anchors,
    menu: Menu<Message>,
    path: Vec<usize>,
    expanded: &[usize],
) -> crate::popup::Host<'static, Message> {
    let trigger = anchors.trigger("more-source", widget::space().width(40).height(20));
    let base = widget::column(vec![widget::space().height(Length::Fill).into(), trigger])
        .height(Length::Fill);
    let panels = surface(menu, anchors, expanded)
        .root_candidates(vec![AnchorCandidate::new(
            Side::Above,
            Alignment::StartBounded,
        )])
        .panels(Id::new("more-source"), path, Message::Path, |message| {
            message
        });
    crate::popup::host(base)
        .anchors(anchors.clone())
        .chain(panels)
        .on_close(Message::Close)
        .on_dismiss(Message::Dismiss)
        .on_no_fit(Message::NoFit)
}
#[derive(Default)]
struct Probe {
    focused: Vec<Id>,
    rows: Vec<Id>,
    texts: Vec<String>,
}
impl Operation for Probe {
    fn text(&mut self, _: Option<&Id>, _: Rectangle, text: &str) {
        self.texts.push(text.to_owned());
    }
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }
    fn focusable(&mut self, id: Option<&Id>, _: Rectangle, state: &mut dyn operation::Focusable) {
        if let Some(id) = id {
            self.rows.push(id.clone());
            if state.is_focused() {
                self.focused.push(id.clone());
            }
        }
    }
}
const WINDOW: Size = Size::new(900.0, 600.0);
fn inspect(
    anchors: &Anchors,
    menu: Menu<Message>,
    expanded: &[usize],
    cache: user_interface::Cache,
    renderer: &mut cosmic::Renderer,
) -> (user_interface::Cache, Probe) {
    let mut ui = UserInterface::build(
        view(anchors, menu, vec![], expanded),
        WINDOW,
        cache,
        renderer,
    );
    let mut probe = Probe::default();
    ui.operate(renderer, &mut probe);
    (ui.into_cache(), probe)
}
fn press(
    anchors: &Anchors,
    menu: Menu<Message>,
    expanded: &[usize],
    cache: user_interface::Cache,
    renderer: &mut cosmic::Renderer,
    key: keyboard::Key,
) -> (user_interface::Cache, Vec<Message>) {
    let mut ui = UserInterface::build(
        view(anchors, menu, vec![], expanded),
        WINDOW,
        cache,
        renderer,
    );
    let mut messages = Vec::new();
    ui.update(
        &[Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Enter),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        })],
        mouse::Cursor::Unavailable,
        renderer,
        &mut cosmic::iced::advanced::clipboard::Null,
        &mut messages,
    );
    (ui.into_cache(), messages)
}
fn shows(probe: &Probe, text: &str) -> bool {
    probe.texts.iter().any(|shown| shown == text)
}
/// Whether the menu has the More row labelled `label`: its label is rich
/// text, which the text operation does not report, so it is found by its row
fn has_more_row(probe: &Probe, label: &str) -> bool {
    probe
        .rows
        .contains(&row_id("more", &[], more_key(None, label, 0)))
}
/// The root panel's preferred size when measured
fn measure(menu: Menu<Message>, expanded: &[usize], renderer: &cosmic::Renderer) -> Size {
    surface(menu, &Anchors::default(), expanded)
        .panels(Id::new("more-source"), vec![], Message::Path, |m| m)
        .remove(0)
        .stages
        .remove(0)
        .content
        .query(Level::Preferred, renderer)
        .preferred
}

#[test]
fn hidden_rows_are_not_shown_until_the_more_row_is_expanded() {
    let mut renderer = renderer();
    let anchors = Anchors::default();
    let (_, collapsed) = inspect(&anchors, models(), &[], Default::default(), &mut renderer);
    assert!(shows(&collapsed, "Opus 5.5") && shows(&collapsed, "Sonnet 5.5"));
    assert!(has_more_row(&collapsed, "Show older models"));
    assert!(!shows(&collapsed, "Opus 5.1") && !shows(&collapsed, LONG));
    let anchors = Anchors::default();
    let open = [more_expansion(&[])];
    let (_, expanded) = inspect(&anchors, models(), &open, Default::default(), &mut renderer);
    assert!(shows(&expanded, "Opus 5.5") && shows(&expanded, "Opus 5.1") && shows(&expanded, LONG));
    assert!(
        !has_more_row(&expanded, "Show older models"),
        "an expanded More row is removed, not kept as a header"
    );
}

#[test]
fn a_menu_without_disclosure_state_shows_every_row() {
    let anchors = Anchors::default();
    let mut renderer = renderer();
    let panels = MenuSurface::new(
        models(),
        MenuStyle { icon, body },
        anchors.clone(),
        "more".into(),
    )
    .panels(Id::new("more-source"), vec![], Message::Path, |m| m);
    let trigger = anchors.trigger("more-source", widget::space().width(40).height(20));
    let base = widget::column(vec![widget::space().height(Length::Fill).into(), trigger])
        .height(Length::Fill);
    let host = crate::popup::host(base)
        .anchors(anchors)
        .chain(panels)
        .on_close(Message::Close);
    let mut ui = UserInterface::build(host, WINDOW, Default::default(), &mut renderer);
    let mut probe = Probe::default();
    ui.operate(&renderer, &mut probe);
    assert!(shows(&probe, "Opus 5.1") && !has_more_row(&probe, "Show older models"));
}

#[test]
fn expanding_changes_the_height_and_never_the_width() {
    let renderer = renderer();
    let collapsed = measure(models(), &[], &renderer);
    let expanded = measure(models(), &[more_expansion(&[])], &renderer);
    assert_eq!(
        collapsed.width, expanded.width,
        "the collapsed card already allows for the longest hidden label"
    );
    assert!(
        expanded.height > collapsed.height,
        "two rows replace the one More row: {} vs {}",
        expanded.height,
        collapsed.height
    );
    let without = measure(
        Menu::new().item(item("Opus 5.5")).item(item("Sonnet 5.5")),
        &[],
        &renderer,
    );
    assert!(
        collapsed.width > without.width,
        "the hidden long label widens the card from the start"
    );
    assert!(models().width() > crate::menu::MENU_MIN_WIDTH);
}

#[test]
fn activating_more_expands_in_place_keeps_the_menu_open_and_focuses_the_first_revealed_row() {
    let mut renderer = renderer();
    let anchors = Anchors::default();
    // Collapsed with the More row first, so it is the keyboard's target
    let menu = || {
        Menu::new().more(
            "Show older models",
            Menu::new().item(item("Opus 5.1")).item(item("Opus 4")),
        )
    };
    let more_id = row_id("more", &[], more_key(None, "Show older models", 0));
    let (cache, probe) = inspect(&anchors, menu(), &[], Default::default(), &mut renderer);
    assert_eq!(probe.focused, [more_id], "More is the first target");
    drop(cache);
    for key in [
        keyboard::Key::Named(keyboard::key::Named::Enter),
        keyboard::Key::Character(" ".into()),
    ] {
        let (cache, _) = inspect(&anchors, menu(), &[], Default::default(), &mut renderer);
        let (_, messages) = press(&anchors, menu(), &[], cache, &mut renderer, key.clone());
        assert_eq!(
            messages,
            [Message::Expand(more_expansion(&[]), true)],
            "{key:?} expands and does not close or act"
        );
    }
    // The window applies it and rebuilds with the state
    let (cache, _) = inspect(&anchors, menu(), &[], Default::default(), &mut renderer);
    let (_, probe) = inspect(
        &anchors,
        menu(),
        &[more_expansion(&[])],
        cache,
        &mut renderer,
    );
    assert_eq!(
        probe.focused,
        [row_id("more", &[], row_key("Opus 5.1", false, 0))],
        "focus goes to the first revealed row"
    );
}

#[test]
fn more_keys_are_unique_across_kinds_labels_submenus_and_nested_disclosures() {
    let rows = Menu::<Message>::new()
        .item(item("More"))
        .folder("More", Menu::new())
        .more("More", Menu::new())
        .more("More", Menu::new());
    let shown = level(&rows, &[], &|_| false, false);
    let unique: BTreeSet<_> = shown.keys.iter().collect();
    assert_eq!(unique.len(), 4, "{:?}", shown.keys);
    // The same More in different menus is a different disclosure
    let key = more_key(None, "More", 0);
    let sub = row_key("Options", true, 0);
    assert_ne!(expansion_key(&[], key), expansion_key(&[sub], key));
    assert_ne!(
        expansion_key(&[sub], key),
        expansion_key(&[sub, row_key("Deeper", true, 0)], key)
    );
    // A More among the rows a More reveals is not its parent
    assert_ne!(more_key(None, "More", 0), more_key(Some(key), "More", 0));
    // The expansion of the root's More leaves a same-named More in a submenu closed
    let menu = Menu::<Message>::new()
        .more("More", Menu::new().item(item("Root rest")))
        .folder(
            "Options",
            Menu::new()
                .item(item("Shown"))
                .more("More", Menu::new().item(item("Submenu rest"))),
        );
    let expanded: BTreeSet<_> = [expansion_key(&[], key)].into();
    let root = level(&menu, &[], &|k| expanded.contains(&k), false);
    assert_eq!(root.menu.outline()[0], "Root rest");
    let Some(Row::Folder(_, submenu)) = menu.rows().get(1) else {
        unreachable!()
    };
    let inner = level(submenu, &[sub], &|k| expanded.contains(&k), false);
    assert_eq!(inner.menu.outline(), ["Shown", "More …", "  Submenu rest"]);
    assert!(
        !inner.menu.outline().contains(&"Submenu rest".to_owned()),
        "the submenu's More stays closed"
    );
}

#[test]
fn revealed_rows_join_the_group_where_the_more_row_was() {
    let menu = Menu::<Message>::new()
        .item(item("First"))
        .more(
            "Rest",
            Menu::new()
                .item(item("Second"))
                .divider()
                .item(item("Third")),
        )
        .divider()
        .item(item("Last"));
    let key = more_key(None, "Rest", 0);
    let shown = level(&menu, &[], &|k| k == expansion_key(&[], key), false);
    assert_eq!(
        shown.menu.outline(),
        ["First", "Second", "—", "Third", "—", "Last"]
    );
    assert_eq!(shown.keys.len(), 6);
}
