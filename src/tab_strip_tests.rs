// SPDX-License-Identifier: GPL-3.0-only
//! The user-content tab strip's rules: the pure ones on the geometry and the
//! model, and the strip itself under a window through the native runtime.

use super::*;
use crate::menu::{MenuItem, MenuStyle};
use crate::menu_surface::{row_id, row_key};
use crate::popup::{INFORMATION_DELAY, Panel};
use crate::window::{UiEvent, WindowAction, WindowUi};
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::runtime::{UserInterface, user_interface};
use cosmic::iced::widget as native;
use cosmic::iced::{Font, Pixels, window as native_window};
use std::time::Instant;

// --- Pure rules -------------------------------------------------------------

#[test]
fn centring_is_as_far_as_the_ends_permit() {
    // total 1000 in a 300 viewport: the offset ranges over 0..=700
    assert_eq!(geometry::centre(450.0, 550.0, 1000.0, 300.0), 350.0);
    assert_eq!(
        geometry::centre(0.0, 100.0, 1000.0, 300.0),
        0.0,
        "start end"
    );
    assert_eq!(
        geometry::centre(900.0, 1000.0, 1000.0, 300.0),
        700.0,
        "far end"
    );
    assert_eq!(geometry::centre(0.0, 100.0, 200.0, 300.0), 0.0, "all fit");
}

#[test]
fn revealing_moves_only_enough() {
    // visible 100..400
    assert_eq!(
        geometry::reveal(100.0, 150.0, 250.0, 1000.0, 300.0),
        100.0,
        "already in"
    );
    assert_eq!(
        geometry::reveal(100.0, 380.0, 480.0, 1000.0, 300.0),
        180.0,
        "end flush"
    );
    assert_eq!(
        geometry::reveal(100.0, 30.0, 130.0, 1000.0, 300.0),
        30.0,
        "start flush"
    );
}

#[test]
fn a_gap_is_filled_from_the_end_without_recentring() {
    // Scrolled to the far end of 1000; 100 is closed: the strip slides back
    // by the gap and not further
    assert_eq!(geometry::clamp(700.0, 900.0, 300.0), 600.0);
    assert_eq!(
        geometry::clamp(300.0, 900.0, 300.0),
        300.0,
        "mid strip stays"
    );
}

#[test]
fn hidden_tabs_are_those_wholly_out_of_view() {
    let spans: Vec<(f32, f32)> = (0..10).map(|i| (i as f32 * 100.0, 100.0)).collect();
    // Viewing 250..550: tabs 0, 1 wholly left; 5 partly in (500..600); 6.. right
    assert_eq!(geometry::hidden(&spans, 250.0, 300.0), (2, 6));
    assert_eq!(geometry::hidden(&spans, 0.0, 300.0), (0, 3));
}

fn ids(n: u64) -> Vec<TabId> {
    (0..n).map(TabId).collect()
}

#[test]
fn closing_an_unselected_tab_keeps_the_selection() {
    let mut model = TabStripModel::new(ids(5), Some(TabId(2)));
    assert_eq!(model.close(TabId(0)), Some(TabId(2)));
    assert_eq!(model.close(TabId(4)), Some(TabId(2)));
    assert_eq!(model.order(), &[TabId(1), TabId(2), TabId(3)]);
}

#[test]
fn closing_the_selected_tab_selects_its_right_neighbour_else_its_left() {
    let mut model = TabStripModel::new(ids(4), Some(TabId(1)));
    assert_eq!(model.close(TabId(1)), Some(TabId(2)), "right neighbour");
    model.select_visible(TabId(3));
    assert_eq!(
        model.close(TabId(3)),
        Some(TabId(2)),
        "none right: the left"
    );
    assert_eq!(model.close(TabId(2)), Some(TabId(0)));
    assert_eq!(
        model.close(TabId(0)),
        None,
        "an empty group has no selection"
    );
    assert!(model.order().is_empty());
}

#[test]
fn selection_and_overflow_never_reorder_the_sequence() {
    let mut model = TabStripModel::new(ids(6), Some(TabId(0)));
    model.select_visible(TabId(4));
    model.choose_hidden(TabId(5));
    model.step(-1);
    assert_eq!(model.order(), ids(6).as_slice());
    model.reorder(TabId(5), 0);
    assert_eq!(
        model.order()[0],
        TabId(5),
        "only an explicit reorder moves one"
    );
    assert_eq!(model.selected(), Some(TabId(4)));
}

// --- The strip under a window -----------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Root {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
enum Message {
    Select(TabId),
    Close(TabId),
    Choose(TabId),
    Reorder(TabId, usize),
}

type Ev = UiEvent<Root, Message>;
const NS: &str = "tabs";

fn title<'a>(text: String, strong: bool) -> Child<'a, Message> {
    engine::child(move |_| {
        let mut text = native::text(text.clone()).size(14);
        if strong {
            text = text.font(Font {
                weight: cosmic::iced::font::Weight::Semibold,
                ..Font::default()
            });
        }
        text.into()
    })
}

fn label(tab: TabId) -> String {
    match tab.0 {
        // One character: wants less than the minimum
        1 => "a".into(),
        // Exactly seven zeros: the minimum itself
        2 => "0000000".into(),
        n => format!("Tab {n}"),
    }
}

fn item(tab: TabId) -> MenuItem<Ev> {
    MenuItem {
        label: label(tab),
        message: UiEvent::Application(Message::Choose(tab)),
        icon: None,
        shortcut: None,
        mark: None,
        disabled_reason: None,
        destructive: false,
    }
}

fn icon(name: &'static str) -> cosmic::widget::icon::Handle {
    cosmic::widget::icon::from_name(name).handle()
}

fn body(text: String) -> Element<'static, Ev> {
    native::text(text).size(14).into()
}

fn panels<'w>(window: &'w WindowUi<Root>, model: &TabStripModel) -> Vec<Panel<'w, Ev>> {
    use crate::layout::{Alignment, AnchorCandidate, Side};
    let Some(request) = window.chain() else {
        return Vec::new();
    };
    let root = *request.root();
    let side = if root == Root::Left {
        Overflow::Left
    } else {
        Overflow::Right
    };
    crate::window::menu_surface(
        model.overflow_menu(side, item),
        MenuStyle { icon, body },
        format!("{root:?}"),
    )
    .root_candidates(vec![AnchorCandidate::new(
        Side::Below,
        Alignment::StartBounded,
    )])
    .hover(request.on_hover_path(), request.hover_level())
    .panels(
        TabStrip::<Message, Root>::overflow_id(NS, side),
        request.path().to_vec(),
        request.on_path(),
        |m| m,
    )
}

fn view<'w>(window: &'w WindowUi<Root>, model: &'w TabStripModel) -> Element<'w, Ev> {
    let tabs = model
        .order()
        .iter()
        .map(|&id| Tab {
            id,
            title: Rc::new(move |strong| title(label(id), strong)),
            icon: None,
            identity: None,
            action: format!("Show {}", label(id)),
            preview: Some(engine::child(|_| native::text("preview").into())),
            on_select: Message::Select(id),
            on_close: Some(Message::Close(id)),
        })
        .collect();
    let strip = TabStrip::new(NS, model, tabs, Root::Left, Root::Right)
        .on_reorder(Message::Reorder)
        .view();
    window.mount(strip, panels(window, model)).into()
}

struct App {
    window: WindowUi<Root>,
    model: TabStripModel,
    size: Size,
    renderer: Renderer,
    cache: Option<user_interface::Cache>,
    cursor: mouse::Cursor,
}

impl App {
    fn new(tabs: u64, width: f32) -> Self {
        let renderer = cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
            Font::default(),
            Pixels(14.0),
            Some("tiny-skia"),
        ))
        .expect("headless renderer");
        let mut app = Self {
            window: WindowUi::new(),
            model: TabStripModel::new(ids(tabs), Some(TabId(0))),
            size: Size::new(width, 600.0),
            renderer,
            cache: None,
            cursor: mouse::Cursor::Unavailable,
        };
        app.frame();
        app
    }

    fn send(&mut self, events: &[Event]) {
        let mut published = Vec::new();
        {
            let root = view(&self.window, &self.model);
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
                    let _: &WindowAction<Root> = &action;
                    self.window.apply(action);
                }
                UiEvent::Application(Message::Select(tab)) => self.model.select_visible(tab),
                UiEvent::Application(Message::Close(tab)) => {
                    self.model.close(tab);
                }
                UiEvent::Application(Message::Choose(tab)) => self.model.choose_hidden(tab),
                UiEvent::Application(Message::Reorder(tab, to)) => self.model.reorder(tab, to),
            }
        }
    }

    /// Run an operation (a focus, say) on the window's current tree
    fn operate(&mut self, operation: &mut dyn cosmic::iced::advanced::widget::Operation) {
        let root = view(&self.window, &self.model);
        let cache = self.cache.take().unwrap_or_default();
        let mut ui = UserInterface::build(root, self.size, cache, &mut self.renderer);
        ui.operate(&self.renderer, operation);
        self.cache = Some(ui.into_cache());
        self.frame();
    }

    fn press_key(&mut self, key: cosmic::iced::keyboard::Key) {
        use cosmic::iced::keyboard;
        let event = Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        });
        self.send(&[event]);
        self.frame();
    }

    fn frame_at(&mut self, at: Instant) {
        for _ in 0..2 {
            self.send(&[Event::Window(native_window::Event::RedrawRequested(at))]);
        }
    }

    fn frame(&mut self) {
        self.frame_at(Instant::now());
    }

    fn visible(&self, id: &Id) -> Option<Rectangle> {
        self.window.anchors().visible(id)
    }

    fn tab(&self, tab: u64) -> Option<Rectangle> {
        self.visible(&TabStrip::<Message, Root>::tab_id(NS, TabId(tab)))
    }

    /// The viewport the tabs show through: between the two overflow buttons
    fn lane(&self) -> (f32, f32) {
        // Only the button with something to open is a source; both are as wide
        let button = [Overflow::Left, Overflow::Right]
            .into_iter()
            .find_map(|side| self.visible(&TabStrip::<Message, Root>::overflow_id(NS, side)));
        button.map_or((0.0, self.size.width), |b| {
            (b.width, self.size.width - b.width)
        })
    }

    fn fully_visible(&self, tab: u64) -> bool {
        let (start, end) = self.lane();
        self.tab(tab)
            .is_some_and(|b| b.x >= start - 0.5 && b.x + b.width <= end + 0.5)
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

    /// Where the tab's title is, away from its close control
    fn title_point(&self, tab: u64) -> Point {
        let b = self.tab(tab).expect("tab shown");
        Point::new(b.x + 14.0, b.y + b.height / 2.0)
    }

    fn open(&mut self, side: Overflow) {
        let button = self
            .visible(&TabStrip::<Message, Root>::overflow_id(NS, side))
            .expect("overflow button shown");
        self.click_at(button.center());
    }

    fn rows_in_menu_order(&self, root: Root, tabs: &[TabId]) -> Vec<f32> {
        tabs.iter()
            .map(|tab| {
                self.visible(&menu_row_for(root, *tab))
                    .expect("row shown")
                    .y
            })
            .collect()
    }
}

/// Many tabs in a window far narrower than all of them
const NARROW: f32 = 420.0;

#[test]
fn a_title_has_at_least_seven_characters_of_room() {
    let app = App::new(4, 900.0);
    let (short, seven) = (app.tab(1).unwrap(), app.tab(2).unwrap());
    let ch = crate::menu::measured_width("0", 14.0, cosmic::font::default());
    assert!(
        short.width >= ch * 7.0 + 20.0,
        "a one-character title still gets seven characters: {short:?}"
    );
    assert!(
        (short.width - seven.width).abs() < 0.5,
        "exactly the minimum for both: {short:?} {seven:?}"
    );
}

#[test]
fn tabs_that_do_not_fit_show_overflow_buttons_and_the_rest_do_not() {
    let wide = App::new(4, 900.0);
    assert!(
        wide.visible(&TabStrip::<Message, Root>::overflow_id(NS, Overflow::Left))
            .is_none()
    );
    let narrow = App::new(12, NARROW);
    assert!(
        narrow
            .visible(&TabStrip::<Message, Root>::overflow_id(NS, Overflow::Right))
            .is_some()
    );
    assert!(narrow.tab(0).is_some());
}

#[test]
fn a_press_on_a_visible_tab_selects_it_and_leaves_the_strip_where_it_is() {
    let mut app = App::new(12, NARROW);
    let before: Vec<_> = (0..4).map(|i| app.tab(i)).collect();
    assert!(app.fully_visible(2));
    app.click_at(app.title_point(2));
    assert_eq!(app.model.selected(), Some(TabId(2)));
    let after: Vec<_> = (0..4).map(|i| app.tab(i)).collect();
    assert_eq!(before, after, "the strip is stationary");
    // And back, on a partly visible tab at the edge, still no movement
    let edge = (0..12).find(|&i| app.tab(i).is_some() && !app.fully_visible(i));
    if let Some(edge) = edge {
        let before = app.tab(0);
        app.click_at(app.title_point(edge).min_x_in(app.lane()));
        assert_eq!(app.tab(0), before);
    }
}

trait InLane {
    fn min_x_in(self, lane: (f32, f32)) -> Self;
}
impl InLane for Point {
    fn min_x_in(mut self, lane: (f32, f32)) -> Self {
        self.x = self.x.clamp(lane.0 + 1.0, lane.1 - 1.0);
        self
    }
}

#[test]
fn the_right_menu_lists_hidden_tabs_nearest_first_and_the_left_in_reverse() {
    let mut app = App::new(12, NARROW);
    // Scroll away from the start: choose tab 8 from the right menu
    let right = app.model.hidden(Overflow::Right);
    assert!(right.len() >= 4, "{right:?}");
    assert_eq!(right, {
        let mut sorted = right.clone();
        sorted.sort_by_key(|t| t.0);
        sorted
    });
    assert_eq!(
        right[0].0 as usize + right.len(),
        12,
        "the tail of the strip"
    );
    app.open(Overflow::Right);
    let ys = app.rows_in_menu_order(Root::Right, &right);
    assert!(
        ys.windows(2).all(|w| w[0] < w[1]),
        "nearest first, then further right: {ys:?}"
    );
    // Choose a far one, so that there are hidden tabs on the left
    let far = *right.last().unwrap();
    app.click_at(
        app.visible(&menu_row_for(Root::Right, far))
            .unwrap()
            .center(),
    );
    assert_eq!(app.model.selected(), Some(far));
    let left = app.model.hidden(Overflow::Left);
    assert!(left.len() >= 2, "{left:?}");
    assert!(
        left.windows(2).all(|w| w[0].0 > w[1].0),
        "reverse strip order, nearest first: {left:?}"
    );
    app.open(Overflow::Left);
    let ys = app.rows_in_menu_order(Root::Left, &left);
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "nearest first: {ys:?}");
    assert_eq!(app.model.order(), ids(12).as_slice(), "never reordered");
}

fn menu_row_for(root: Root, tab: TabId) -> Id {
    row_id(&format!("{root:?}"), &[], row_key(&label(tab), false, 0))
}

#[test]
fn choosing_a_hidden_tab_recentres_on_it() {
    let mut app = App::new(12, NARROW);
    let target = TabId(6);
    assert!(app.model.hidden(Overflow::Right).contains(&target));
    app.open(Overflow::Right);
    app.click_at(
        app.visible(&menu_row_for(Root::Right, target))
            .unwrap()
            .center(),
    );
    assert_eq!(app.model.selected(), Some(target));
    let (start, end) = app.lane();
    let tab = app.tab(6).expect("shown");
    let drift = (tab.x + tab.width / 2.0 - (start + end) / 2.0).abs();
    assert!(
        drift < 1.0,
        "centred in the strip: {tab:?} in {start}..{end}"
    );
}

#[test]
fn choosing_a_tab_near_the_end_centres_as_far_as_the_end_permits() {
    let mut app = App::new(12, NARROW);
    app.open(Overflow::Right);
    app.click_at(
        app.visible(&menu_row_for(Root::Right, TabId(11)))
            .unwrap()
            .center(),
    );
    let (_, end) = app.lane();
    let tab = app.tab(11).unwrap();
    assert!(
        (tab.x + tab.width - end).abs() < 1.0,
        "the last tab is flush with the end: {tab:?} {end}"
    );
    assert!(app.model.hidden(Overflow::Right).is_empty());
}

#[test]
fn keyboard_stepping_moves_only_enough_to_show_the_selection() {
    let mut app = App::new(12, NARROW);
    let mut last = 0;
    while app.fully_visible(last + 1) {
        last += 1;
    }
    let first_before = app.tab(0).unwrap().x;
    app.model.step((last + 1) as isize);
    app.frame();
    let step = app.model.selected().unwrap();
    assert!(app.fully_visible(step.0), "revealed");
    let (_, end) = app.lane();
    let tab = app.tab(step.0).unwrap();
    assert!(
        (tab.x + tab.width - end).abs() < 1.0,
        "flush with the edge, no further"
    );
    assert!(app.tab(0).unwrap().x < first_before, "moved");
    // One step back in view: no movement at all
    let before = app.tab(step.0).unwrap();
    app.model.step(-1);
    app.frame();
    assert_eq!(app.tab(step.0), Some(before), "already in view: stationary");
}

#[test]
fn shrinking_the_window_keeps_the_selection_in_view() {
    let mut app = App::new(12, 900.0);
    app.model.select_visible(TabId(5));
    app.frame();
    assert!(app.fully_visible(5));
    app.size = Size::new(NARROW, 600.0);
    app.frame();
    assert!(app.fully_visible(5), "still shown after the resize");
    app.size = Size::new(260.0, 600.0);
    app.frame();
    assert!(app.fully_visible(5), "and again");
}

#[test]
fn closing_in_a_scrolled_strip_fills_the_gap_and_keeps_the_selection() {
    let mut app = App::new(12, NARROW);
    app.open(Overflow::Right);
    app.click_at(
        app.visible(&menu_row_for(Root::Right, TabId(11)))
            .unwrap()
            .center(),
    );
    let (_, end) = app.lane();
    // Close an unselected visible tab: selection stays, the end stays full
    let victim = TabId(9);
    app.model.close(victim);
    app.frame();
    assert_eq!(app.model.selected(), Some(TabId(11)));
    let last = app.tab(11).unwrap();
    assert!(
        (last.x + last.width - end).abs() < 1.0,
        "no empty space at the end"
    );
    // Closing the selected last tab selects the left neighbour (no right one)
    app.model.close(TabId(11));
    app.frame();
    assert_eq!(app.model.selected(), Some(TabId(10)));
    assert!(app.fully_visible(10));
}

#[test]
fn a_drag_reorders_within_the_visible_tabs() {
    let mut app = App::new(12, NARROW);
    let from = app.title_point(3);
    let to = app.title_point(0);
    app.point(from);
    app.send(&[Event::Mouse(mouse::Event::ButtonPressed(
        mouse::Button::Left,
    ))]);
    app.point(Point::new(from.x - 30.0, from.y));
    app.point(to);
    app.send(&[Event::Mouse(mouse::Event::ButtonReleased(
        mouse::Button::Left,
    ))]);
    app.frame();
    assert_eq!(app.model.order()[0], TabId(3));
    assert_eq!(
        app.model.selected(),
        Some(TabId(0)),
        "a drag does not select"
    );
}

#[test]
fn a_tab_has_one_information_card_anchored_at_it_after_the_delay() {
    let mut app = App::new(12, NARROW);
    let id = TabStrip::<Message, Root>::tab_id(NS, TabId(3));
    app.point(app.title_point(3));
    assert!(
        !app.window.anchors().information_visible(&id),
        "not before the delay"
    );
    app.frame_at(Instant::now() + INFORMATION_DELAY);
    assert!(app.window.anchors().information_visible(&id));
    assert_eq!(app.window.anchors().hovered(), Some(id.clone()));
    let tab = app.tab(3).unwrap();
    let anchor = app
        .window
        .anchors()
        .information_anchor(&id)
        .expect("anchored");
    assert!(
        anchor.x >= tab.x
            && anchor.x + anchor.width <= tab.x + tab.width
            && anchor.y >= tab.y
            && anchor.y + anchor.height <= tab.y + tab.height,
        "the card is anchored within the hovered tab: {anchor:?} in {tab:?}"
    );
    // Moving to another tab hands the one card over; it never stacks
    app.point(app.title_point(2));
    app.frame_at(Instant::now() + INFORMATION_DELAY);
    assert!(
        !app.window.anchors().information_visible(&id),
        "one card at a time"
    );
    assert!(
        app.window
            .anchors()
            .information_visible(&TabStrip::<Message, Root>::tab_id(NS, TabId(2)))
    );
}

#[test]
fn closing_through_the_close_control_publishes_the_close() {
    let mut app = App::new(4, 900.0);
    let tab = app.tab(3).unwrap();
    // The close control is at the tab's end
    app.click_at(Point::new(
        tab.x + tab.width - 12.0,
        tab.y + tab.height / 2.0,
    ));
    assert_eq!(app.model.order().len(), 3, "closed");
    assert_eq!(
        app.model.selected(),
        Some(TabId(0)),
        "unselected: selection kept"
    );
}

#[test]
fn tabs_keep_the_tab_bars_measures_equal_widths_and_no_maximum() {
    // COSMIC's tab bar (libcosmic `tab_bar`, as Terminal sets it up): the
    // tabs share the width equally and fill it, 32 high, with no maximum:
    // two tabs in 900 pixels are 450 each
    let two = App::new(2, 900.0);
    let (a, b) = (two.tab(0).unwrap(), two.tab(1).unwrap());
    assert!((a.width - 450.0).abs() < 0.5 && (b.width - 450.0).abs() < 0.5);
    assert!((a.height - 32.0).abs() < 0.5, "{a:?}");
    assert!((b.x - (a.x + a.width)).abs() < 0.5, "no space between tabs");
    let four = App::new(4, 900.0);
    assert!((four.tab(3).unwrap().width - 225.0).abs() < 0.5);
}

#[test]
fn the_wheel_scrolls_an_overflowing_strip_without_selecting() {
    let mut app = App::new(12, NARROW);
    let selected = app.model.selected();
    let first = app.tab(0).unwrap();
    let over = app.title_point(1);
    app.point(over);
    app.send(&[Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Lines { x: 0.0, y: -2.0 },
    })]);
    app.frame();
    let moved = app.tab(0);
    assert!(
        moved.is_none_or(|b| b.x < first.x - 1.0),
        "the strip scrolled sideways: {first:?} then {moved:?}"
    );
    assert_eq!(app.model.selected(), selected, "scrolling selects nothing");
    // The other way brings it back, to the start and no further
    for _ in 0..4 {
        app.send(&[Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: 5.0 },
        })]);
    }
    app.frame();
    assert_eq!(app.tab(0), Some(first));
    // A strip that fits does not scroll
    let mut fits = App::new(3, 900.0);
    let before = fits.tab(0);
    fits.point(fits.title_point(1));
    fits.send(&[Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Lines { x: 0.0, y: -3.0 },
    })]);
    fits.frame();
    assert_eq!(fits.tab(0), before);
}

#[test]
fn left_and_right_move_focus_between_tabs_and_enter_or_space_select() {
    use cosmic::iced::advanced::widget::operation::focusable;
    use cosmic::iced::keyboard::key::Named;
    let mut app = App::new(5, 900.0);
    app.operate(&mut focusable::focus::<()>(
        TabStrip::<Message, Root>::tab_id(NS, TabId(1)),
    ));
    app.press_key(cosmic::iced::keyboard::Key::Named(Named::ArrowRight));
    app.press_key(cosmic::iced::keyboard::Key::Named(Named::ArrowRight));
    app.press_key(cosmic::iced::keyboard::Key::Named(Named::Enter));
    assert_eq!(
        app.model.selected(),
        Some(TabId(3)),
        "Right twice from tab 1 focuses tab 3; Enter selects it"
    );
    app.press_key(cosmic::iced::keyboard::Key::Named(Named::ArrowLeft));
    app.press_key(cosmic::iced::keyboard::Key::Character(" ".into()));
    assert_eq!(app.model.selected(), Some(TabId(2)));
    assert_eq!(app.model.order().len(), 5, "no reordering by keys");
}

#[test]
fn a_hidden_tab_is_not_a_keyboard_stop() {
    use cosmic::iced::advanced::widget::operation::focusable;
    use cosmic::iced::keyboard::key::Named;
    let mut app = App::new(12, NARROW);
    // Focus the last tab in view and step right: it stays on the tabs in view
    let last = (0..12)
        .rev()
        .find(|&i| app.fully_visible(i))
        .expect("some tab in view");
    app.operate(&mut focusable::focus::<()>(
        TabStrip::<Message, Root>::tab_id(NS, TabId(last)),
    ));
    app.press_key(cosmic::iced::keyboard::Key::Named(Named::ArrowRight));
    app.press_key(cosmic::iced::keyboard::Key::Named(Named::ArrowRight));
    app.press_key(cosmic::iced::keyboard::Key::Named(Named::ArrowRight));
    app.press_key(cosmic::iced::keyboard::Key::Named(Named::Enter));
    let chosen = app.model.selected().unwrap().0;
    assert!(
        app.tab(chosen).is_some(),
        "the selected tab is one in view: {chosen}"
    );
}

#[test]
fn an_apps_own_changes_are_taken_and_only_a_selection_that_moved_is_shown() {
    let mut model = TabStripModel::new(ids(6), Some(TabId(0)));
    // What the strip itself selected asks nothing more
    model.select_visible(TabId(2));
    let before = model.seq;
    model.sync(ids(6), Some(TabId(2)));
    assert_eq!(model.seq, before, "no request");
    // A selection made elsewhere is shown
    model.sync(ids(6), Some(TabId(5)));
    assert_eq!(model.seq, before + 1);
    // A tab gone: the order is the app's, the selection kept in it or none
    model.sync(vec![TabId(0), TabId(1)], Some(TabId(5)));
    assert_eq!(model.order(), &[TabId(0), TabId(1)]);
    assert_eq!(model.selected(), None);
}
