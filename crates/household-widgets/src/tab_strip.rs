// SPDX-License-Identifier: GPL-3.0-only

//! The user-content tab strip: tabs the user opens, closes and reorders,
//! as against the fixed sections of [`tabs`](crate::tabs). The rules are
//! those Juliet agreed on 15 November (SECTION-NAVIGATION.md, "User-content
//! tabs"):
//!
//! - a title has at least seven characters' worth of the UI font's space
//!   ([`MINIMUM_TITLE_CHARS`]), excluding icon, close control and padding;
//! - the sequence is stable: selection and overflow never reorder tabs, and
//!   pressing a visible tab leaves the strip where it is;
//! - when the tabs do not all fit, a LEFT and a RIGHT overflow button each
//!   open a menu of the tabs hidden on that side, nearest first (the left
//!   menu is the strip in reverse). The menus are the window's own: the
//!   buttons are [`trigger`](crate::window::trigger)s of roots the app
//!   provides panels for, built with [`TabStripModel::overflow_menu`];
//! - choosing a hidden tab recentres on it as far as the ends permit;
//!   resizing and keyboard stepping move only enough to keep the selection
//!   visible; closing leaves the strip where it is, the gap filled from the
//!   end;
//! - a tab's one information card shows at the normal delay, anchored to the
//!   tab: the action explanation, then the preview.
//!
//! Tabs have COSMIC's tab bar's measures (libcosmic's `tab_bar`, as
//! Terminal sets it up): they share the strip's width equally, each as wide
//! as the strip is divided by the tabs, never narrower than the seven
//! characters and the tab's other contents need ([`MINIMUM_TITLE_CHARS`]),
//! with COSMIC's `space_s` (16) before the icon and after the title (eight
//! after a close control) and `space_xxs` (8) between parts. COSMIC's tab bar has no maximum
//! width: the 250 pixels it is built with is only the measure of a tab
//! given its natural width, which a bar filling its width never uses.
//!
//! The title is a box the app supplies ([`Tab::title`], a layout
//! [`Child`]); the strip only gives it room, and the box reports any
//! truncation. The mouse wheel over an overflowing strip scrolls it
//! sideways without selecting; Left and Right move the keyboard focus
//! among the tabs in view, Enter and Space select. Dragging a tab reorders it among the visible tabs; dragging
//! beyond them is not designed yet and does nothing.

use crate::button::Button;
use crate::layout::{self as engine, AnchorCandidate, Child, Level};
use crate::menu::{Menu, MenuItem};
use crate::spacing::Kind;
use crate::tooltip::Tip;
use crate::window::{Toggle, hover_box, trigger};
use cosmic::iced::advanced::renderer::Renderer as _;
use cosmic::iced::advanced::widget::{Id, Operation, Tree, tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use cosmic::iced::{Border, Color, Event, Length, Padding, Point, Rectangle, Size, Vector};
use cosmic::{Element, Renderer, Theme, widget};
use std::cell::Cell;
use std::rc::Rc;

/// The least a title is given, in characters' worth of the UI font
pub const MINIMUM_TITLE_CHARS: f32 = 7.0;
const ICON_SIZE: f32 = 16.0;
/// Pointer travel before a press on a tab is a drag: the 25 pixels Terminal
/// gave libcosmic's tab bar
const DRAG_THRESHOLD: f32 = 25.0;
/// The selected tab's line, and the thin line under the others, as COSMIC's
/// tab bar draws them (its `active_width` of 4 and 1)
const UNDERLINE: f32 = 4.0;
const THIN_LINE: f32 = 1.0;
/// How far one notch of a wheel scrolls the strip
const WHEEL_LINE: f32 = 48.0;

/// A tab's identity, the app's and stable across reordering
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TabId(pub u64);

/// Which end of the strip an overflow menu is for
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Overflow {
    Left,
    Right,
}

// --- Geometry: the rules, as pure functions ---------------------------------

/// Pure strip geometry, in pixels along the strip. `total` is the tabs'
/// whole width, `viewport` the width they show through.
pub mod geometry {
    /// The furthest the strip scrolls
    pub fn max_offset(total: f32, viewport: f32) -> f32 {
        (total - viewport).max(0.0)
    }

    /// `offset` brought within the ends: this is what fills a gap left by a
    /// closed tab or a wider window, without recentring on anything
    pub fn clamp(offset: f32, total: f32, viewport: f32) -> f32 {
        offset.clamp(0.0, max_offset(total, viewport))
    }

    /// The offset that centres the tab `start..end`, as far as the ends permit
    pub fn centre(start: f32, end: f32, total: f32, viewport: f32) -> f32 {
        clamp((start + end) / 2.0 - viewport / 2.0, total, viewport)
    }

    /// The least movement from `offset` that shows the tab `start..end` in
    /// full (its start, if it is wider than the viewport)
    pub fn reveal(offset: f32, start: f32, end: f32, total: f32, viewport: f32) -> f32 {
        let moved = if start < offset || end - start > viewport {
            start
        } else if end > offset + viewport {
            end - viewport
        } else {
            offset
        };
        clamp(moved, total, viewport)
    }

    /// How many leading tabs lie wholly out of view, and the index of the
    /// first trailing tab wholly out of view (the count when none is). A
    /// tab partly in view is visible: it can be pressed.
    pub fn hidden(starts_and_widths: &[(f32, f32)], offset: f32, viewport: f32) -> (usize, usize) {
        const EPS: f32 = 0.5;
        let left = starts_and_widths
            .iter()
            .take_while(|(start, width)| start + width <= offset + EPS)
            .count();
        let right = starts_and_widths
            .iter()
            .position(|(start, _)| *start >= offset + viewport - EPS)
            .map_or(starts_and_widths.len(), |at| at.max(left));
        (left, right)
    }
}

// --- The model --------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Wanted {
    Centre,
    Reveal,
}

#[derive(Clone, Copy, Debug)]
struct Request {
    seq: u64,
    wanted: Wanted,
    tab: TabId,
}

/// The tabs' order and selection, which the app owns. The scroll position
/// belongs to the strip widget; the model asks it to centre on or reveal a
/// tab, and keeps what the strip last showed so that the overflow menus list
/// what is hidden.
#[derive(Clone)]
pub struct TabStripModel {
    order: Vec<TabId>,
    selected: Option<TabId>,
    request: Option<Request>,
    seq: u64,
    shown: Rc<Cell<(usize, usize)>>,
}

impl Default for TabStripModel {
    fn default() -> Self {
        Self::new(Vec::new(), None)
    }
}

impl TabStripModel {
    pub fn new(order: Vec<TabId>, selected: Option<TabId>) -> Self {
        let selected = selected.filter(|id| order.contains(id));
        let count = order.len();
        let mut model = Self {
            order,
            selected,
            request: None,
            seq: 0,
            shown: Rc::new(Cell::new((0, count))),
        };
        if let Some(tab) = model.selected {
            model.ask(Wanted::Reveal, tab);
        }
        model
    }

    pub fn order(&self) -> &[TabId] {
        &self.order
    }

    pub fn selected(&self) -> Option<TabId> {
        self.selected
    }

    fn ask(&mut self, wanted: Wanted, tab: TabId) {
        self.seq += 1;
        self.request = Some(Request {
            seq: self.seq,
            wanted,
            tab,
        });
    }

    /// A press on a visible tab: selects it and leaves the strip where it is
    pub fn select_visible(&mut self, tab: TabId) {
        if self.order.contains(&tab) {
            self.selected = Some(tab);
        }
    }

    /// A tab chosen from an overflow menu: selected, and the strip recentres
    /// on it as far as its ends permit
    pub fn choose_hidden(&mut self, tab: TabId) {
        if self.order.contains(&tab) {
            self.selected = Some(tab);
            self.ask(Wanted::Centre, tab);
        }
    }

    /// Keyboard stepping to the neighbour `delta` places away (no wrap):
    /// the strip moves only enough to show it in full
    pub fn step(&mut self, delta: isize) -> Option<TabId> {
        let at = self
            .order
            .iter()
            .position(|id| Some(*id) == self.selected)?;
        let to = at
            .checked_add_signed(delta)?
            .min(self.order.len().saturating_sub(1));
        let tab = self.order[to];
        self.selected = Some(tab);
        self.ask(Wanted::Reveal, tab);
        Some(tab)
    }

    /// Take the order and selection an app keeps itself (its own tab model),
    /// for one that changes them where the strip cannot see. A selection
    /// that moved is shown in full, as keyboard stepping shows it; one the
    /// strip itself made ([`Self::select_visible`], [`Self::choose_hidden`])
    /// has already moved nothing and asks nothing more. The strip is
    /// not otherwise moved: a closed tab's gap is filled from the end.
    pub fn sync(&mut self, order: Vec<TabId>, selected: Option<TabId>) {
        self.order = order;
        let selected = selected.filter(|id| self.order.contains(id));
        if selected != self.selected {
            self.selected = selected;
            if let Some(tab) = selected {
                self.ask(Wanted::Reveal, tab);
            }
        }
    }

    /// Add a tab at `index` (the end if beyond it). A selected new tab is
    /// shown in full.
    pub fn insert(&mut self, index: usize, tab: TabId, select: bool) {
        if self.order.contains(&tab) {
            return;
        }
        self.order.insert(index.min(self.order.len()), tab);
        if select || self.selected.is_none() {
            self.selected = Some(tab);
            self.ask(Wanted::Reveal, tab);
        }
    }

    /// Close a tab. An unselected one leaves the selection alone; the
    /// selected one gives it to its right neighbour, else its left. The
    /// strip does not recentre: the gap is filled from the end. Returns the
    /// selection afterwards.
    pub fn close(&mut self, tab: TabId) -> Option<TabId> {
        let Some(at) = self.order.iter().position(|id| *id == tab) else {
            return self.selected;
        };
        self.order.remove(at);
        if self.selected == Some(tab) {
            self.selected = self
                .order
                .get(at)
                .or_else(|| at.checked_sub(1).and_then(|left| self.order.get(left)))
                .copied();
            if let Some(next) = self.selected {
                self.ask(Wanted::Reveal, next);
            }
        }
        self.selected
    }

    /// Move `tab` to `index` of the sequence (the sequence otherwise
    /// never changes). The selection stays on the same tab.
    pub fn reorder(&mut self, tab: TabId, index: usize) {
        if let Some(at) = self.order.iter().position(|id| *id == tab) {
            let tab = self.order.remove(at);
            self.order.insert(index.min(self.order.len()), tab);
        }
    }

    /// The tabs wholly out of view on `side`, nearest to the strip's view
    /// first: the left list is the strip in reverse
    pub fn hidden(&self, side: Overflow) -> Vec<TabId> {
        let (left, right) = self.shown.get();
        let (left, right) = (left.min(self.order.len()), right.min(self.order.len()));
        match side {
            Overflow::Left => self.order[..left].iter().rev().copied().collect(),
            Overflow::Right => self.order[right..].to_vec(),
        }
    }

    /// The menu of `side`'s hidden tabs, nearest first, in the window's
    /// shared menu form. `item` makes each row (its message should call
    /// [`Self::choose_hidden`]).
    pub fn overflow_menu<M>(&self, side: Overflow, item: impl Fn(TabId) -> MenuItem<M>) -> Menu<M> {
        self.hidden(side)
            .into_iter()
            .fold(Menu::new(), |menu, tab| menu.item(item(tab)))
    }
}

// --- The tab and the strip --------------------------------------------------

/// One tab as the app describes it
pub struct Tab<'a, M> {
    pub id: TabId,
    /// The title box, built for `emphasised` (selected or hovered; the
    /// app's semibold). The strip gives it room: at least seven characters.
    pub title: Rc<dyn Fn(bool) -> Child<'a, M> + 'a>,
    /// The tab's icon, 16 pixels as the app makes it (its colours its own)
    pub icon: Option<widget::icon::Icon>,
    /// The selected underline's colour (the tab's identity), else the accent
    pub identity: Option<Color>,
    /// What pressing the tab does, for its information card
    pub action: String,
    /// The preview shown after the action explanation
    pub preview: Option<Child<'a, M>>,
    pub on_select: M,
    pub on_close: Option<M>,
}

type Decorate<'a, M> = Rc<dyn Fn(TabId, Element<'a, M>) -> Element<'a, M> + 'a>;

/// The strip. Build one each view.
pub struct TabStrip<'a, M, R> {
    namespace: String,
    model: TabStripModel,
    tabs: Vec<Tab<'a, M>>,
    roots: (R, R),
    on_reorder: Option<Rc<dyn Fn(TabId, usize) -> M + 'a>>,
    decorate: Option<Decorate<'a, M>>,
    title_size: f32,
    /// Set by the app; otherwise a control's height, `space_l`
    height: Option<f32>,
    editor: Option<Id>,
}

impl<'a, M: Clone + 'static, R: Clone + 'static> TabStrip<'a, M, R> {
    /// `tabs` in the model's order; `left` and `right` are the roots of the
    /// window's chain the overflow buttons open. Ids are made under
    /// `namespace`.
    pub fn new(
        namespace: impl Into<String>,
        model: &TabStripModel,
        tabs: Vec<Tab<'a, M>>,
        left: R,
        right: R,
    ) -> Self {
        debug_assert!(
            tabs.iter()
                .map(|tab| tab.id)
                .eq(model.order.iter().copied()),
            "the tabs are the model's, in its order"
        );
        Self {
            namespace: namespace.into(),
            model: model.clone(),
            tabs,
            roots: (left, right),
            on_reorder: None,
            decorate: None,
            title_size: 14.0,
            height: None,
            editor: None,
        }
    }

    /// The id of a tab, its information card's anchor and its focus target
    pub fn tab_id(namespace: &str, tab: TabId) -> Id {
        Id::new(format!("{namespace}:tab:{}", tab.0))
    }

    /// The id of an overflow button: its menu's root panel anchors at it
    pub fn overflow_id(namespace: &str, side: Overflow) -> Id {
        Id::new(format!("{namespace}:tab-overflow:{side:?}"))
    }

    /// What a drag of a tab onto another visible tab publishes: the tab and
    /// the index it takes in the sequence (then call [`TabStripModel::reorder`])
    pub fn on_reorder(mut self, on_reorder: impl Fn(TabId, usize) -> M + 'a) -> Self {
        self.on_reorder = Some(Rc::new(on_reorder));
        self
    }

    /// Wrap each tab (the app's context menu, detach gesture); the tab's own
    /// information and selection stay inside it
    pub fn decorate(
        mut self,
        decorate: impl Fn(TabId, Element<'a, M>) -> Element<'a, M> + 'a,
    ) -> Self {
        self.decorate = Some(Rc::new(decorate));
        self
    }

    /// The titles' font size (the app's body size, 14 by default), which
    /// the minimum allocation is measured in
    pub fn title_size(mut self, size: f32) -> Self {
        assert!(size.is_finite() && size > 0.0);
        self.title_size = size;
        self
    }

    /// The control F6 moves to from the tabs (and back from): the content
    /// the tabs are for. Without one, F6 has nowhere to go from the tabs.
    pub fn editor(mut self, editor: Id) -> Self {
        self.editor = Some(editor);
        self
    }

    /// The tabs' height (`space_l`, as COSMIC's tab bar has it, by default)
    pub fn tab_height(mut self, height: f32) -> Self {
        assert!(height.is_finite() && height > 0.0);
        self.height = Some(height);
        self
    }

    pub fn view(self) -> Element<'a, M> {
        self.view_in(&cosmic::theme::spacing())
    }

    /// [`view`](Self::view) with the given spacing steps: the tab's
    /// [`Tab`](Kind::Tab) inset and gap, and the control height
    fn view_in(self, spacing: &cosmic::cosmic_theme::Spacing) -> Element<'a, M> {
        let height = self
            .height
            .unwrap_or_else(|| crate::spacing::control_height(spacing));
        let ns = self.namespace.clone();
        let ch = crate::menu::measured_width("0", self.title_size, cosmic::font::default());
        let minimum = ch * MINIMUM_TITLE_CHARS;
        // The space before a tab's content, and after it where it has no close
        // control, and between its parts. A close control is a nested control
        // sized by its tab: it sits one gap from the tab's end
        let (inset, gap) = (Kind::Tab.inset(spacing).left, Kind::Tab.gap(spacing));
        let close_end = gap;
        let mut minimums = Vec::new();
        let selected_id = self.model.selected;
        let line_colour = self
            .tabs
            .iter()
            .find(|tab| Some(tab.id) == selected_id)
            .and_then(|tab| tab.identity);
        let mut ids = Vec::new();
        let mut selects = Vec::new();
        let mut elements = Vec::new();
        for tab in self.tabs {
            let id = Self::tab_id(&ns, tab.id);
            let selected = selected_id == Some(tab.id);
            let hovered = Rc::new(Cell::new(false));
            let title = Emphasis {
                normal: (tab.title)(false).view(Level::Preferred),
                strong: (tab.title)(true).view(Level::Preferred),
                minimum,
                selected,
                hovered: hovered.clone(),
            };
            let mut parts: Vec<Element<'a, M>> = Vec::new();
            let (mut least, mut end) = (inset + minimum, inset);
            if let Some(icon) = tab.icon {
                parts.push(icon.into());
                least += ICON_SIZE + gap;
            }
            parts.push(Element::new(title));
            if let Some(close) = tab.on_close {
                let mut button =
                    widget::button::icon(widget::icon::from_name("window-close-symbolic").handle())
                        .icon_size(ICON_SIZE as u16)
                        .padding(0)
                        .on_press(close);
                {
                    // The control is in its tab's content colour, as its
                    // icon is
                    let colour = line_colour;
                    let style = move |theme: &Theme| {
                        let colour = content_colour(theme, selected, colour);
                        widget::button::Style {
                            icon_color: Some(colour),
                            text_color: Some(colour),
                            ..widget::button::Style::new()
                        }
                    };
                    button = button.class(cosmic::theme::Button::Custom {
                        active: Box::new(move |_, theme| style(theme)),
                        disabled: Box::new(move |theme| style(theme)),
                        hovered: Box::new(move |_, theme| style(theme)),
                        pressed: Box::new(move |_, theme| style(theme)),
                    });
                }
                parts.push(button.into());
                least += ICON_SIZE + gap;
                end = close_end;
            }
            least += end;
            minimums.push(least);
            let body = widget::container(
                widget::row::with_children(parts)
                    .spacing(gap)
                    .width(Length::Fill)
                    .height(height)
                    .align_y(cosmic::iced::Alignment::Center),
            )
            .width(Length::Fill)
            .height(height)
            .align_y(cosmic::iced::alignment::Vertical::Center)
            .padding(Padding {
                top: 0.0,
                bottom: 0.0,
                left: inset,
                right: end,
            });
            let wash = Wash {
                content: body.into(),
                selected,
                line: line_colour,
                hovered,
            };
            let activate = tab.on_select.clone();
            let focusable =
                crate::popup::focus_target(id.clone(), Element::new(wash)).on_activate(activate);
            let mut card = vec![engine::child({
                let action = tab.action.clone();
                move |_| widget::text(action.clone()).into()
            })];
            card.extend(tab.preview);
            let informed = hover_box(
                id.clone(),
                focusable,
                engine::column_child(card, Kind::InformationCard.gap(spacing)),
                vec![AnchorCandidate::new(
                    engine::Side::Below,
                    engine::Alignment::CenterBounded,
                )],
            );
            let element = match &self.decorate {
                Some(decorate) => decorate(tab.id, informed),
                None => informed,
            };
            ids.push(tab.id);
            selects.push(tab.on_select);
            elements.push(element);
        }
        let on_button = |side: Overflow, root: R| -> (Element<'a, M>, Element<'a, M>) {
            let (name, tip) = if side == Overflow::Left {
                ("go-previous-symbolic", "Tabs hidden on the left")
            } else {
                ("go-next-symbolic", "Tabs hidden on the right")
            };
            // The shared button's Icon look: a `space_l` square, as every
            // control is tall, its name in a tooltip
            fn button<'b, N: Clone + 'static>(
                name: &str,
                tip: &'static str,
                press: Option<N>,
            ) -> Element<'b, N> {
                Button::icon(
                    widget::icon::icon(widget::icon::from_name(name).handle()),
                    tip,
                    Tip::new(tip, move || widget::text(tip).into()),
                )
                .on_press_maybe(press)
                .into()
            }
            let id = Self::overflow_id(&ns, side);
            let on = trigger(
                id.clone(),
                root,
                crate::popup::focus_target(id.clone(), button(name, tip, Some(Toggle)))
                    .on_activate(Toggle),
            );
            let off: Element<'a, M> = button(name, tip, None);
            (on, off)
        };
        let (left_on, left_off) = on_button(Overflow::Left, self.roots.0);
        let (right_on, right_off) = on_button(Overflow::Right, self.roots.1);
        let count = elements.len();
        elements.extend([left_on, left_off, right_on, right_off]);
        let focus_ids: Vec<Id> = ids.iter().map(|tab| Self::tab_id(&ns, *tab)).collect();
        let editor = self
            .editor
            .unwrap_or_else(|| Id::new(format!("{ns}:tab-strip-editor")));
        let strip = Element::new(Strip {
            children: elements,
            count,
            minimums,
            ids: ids.clone(),
            selects,
            selected: selected_id.and_then(|id| self.model.order.iter().position(|o| *o == id)),
            request: self.model.request,
            shown: self.model.shown.clone(),
            on_reorder: self.on_reorder,
        });
        // Left and Right among the tabs in view, as the shared focus regions
        // have them
        crate::window::focus_regions(
            strip,
            editor,
            vec![crate::focus_regions::Group::new(
                crate::list_navigation::Axis::Horizontal,
                focus_ids,
            )],
        )
    }
}

// --- The strip widget -------------------------------------------------------

struct Strip<'a, M> {
    /// The tabs, then left-on, left-off, right-on, right-off
    children: Vec<Element<'a, M>>,
    count: usize,
    /// The least width each tab is given
    minimums: Vec<f32>,
    ids: Vec<TabId>,
    selects: Vec<M>,
    selected: Option<usize>,
    request: Option<Request>,
    shown: Rc<Cell<(usize, usize)>>,
    on_reorder: Option<Rc<dyn Fn(TabId, usize) -> M + 'a>>,
}

#[derive(Default)]
struct State {
    offset: f32,
    handled: u64,
    viewport_width: f32,
    /// The viewport's left edge within the strip, and its width
    lane: (f32, f32),
    /// Tabs' starts and widths along the strip (not scrolled)
    spans: Vec<(f32, f32)>,
    press: Option<Press>,
}

struct Press {
    tab: usize,
    origin: Point,
    dragging: bool,
}

impl<M> Strip<'_, M> {
    fn buttons(&self, state: &State) -> (usize, usize) {
        // Indices of the active left and right buttons among the children
        let (left, right) = self.hidden_counts(state);
        let base = self.count;
        (
            base + usize::from(left == 0),
            base + 2 + usize::from(right >= self.count),
        )
    }

    fn hidden_counts(&self, state: &State) -> (usize, usize) {
        geometry::hidden(&state.spans, state.offset, state.lane.1)
    }

    fn overflowing(&self, state: &State) -> bool {
        state.lane.0 > 0.0 || state.lane.1 < state.spans.last().map_or(0.0, |(s, w)| s + w)
    }

    /// The visible part of tab `index` on screen
    fn tab_bounds(&self, state: &State, origin: Point, index: usize) -> Rectangle {
        let (start, width) = state.spans[index];
        Rectangle::new(
            Point::new(origin.x + state.lane.0 - state.offset + start, origin.y),
            Size::new(width, 0.0),
        )
    }
}

impl<M: Clone + 'static> Widget<M, Theme, Renderer> for Strip<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.children);
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let available = limits.max().width;
        let loose = layout::Limits::new(Size::ZERO, Size::new(f32::INFINITY, limits.max().height));
        // The tabs share the width equally, none narrower than it needs
        let least = self.minimums.iter().copied().fold(0.0_f32, f32::max);
        let count = self.count.max(1) as f32;
        let shared = if available.is_finite() {
            available / count
        } else {
            0.0
        };
        let tab_width = shared.max(least);
        let exact = layout::Limits::new(
            Size::new(tab_width, 0.0),
            Size::new(tab_width, limits.max().height),
        );
        let mut nodes = Vec::new();
        let mut spans = Vec::new();
        let (mut start, mut height) = (0.0_f32, 0.0_f32);
        for (index, child) in self.children.iter_mut().enumerate().take(self.count) {
            let node = child
                .as_widget_mut()
                .layout(&mut tree.children[index], renderer, &exact);
            spans.push((start, node.size().width));
            start += node.size().width;
            height = height.max(node.size().height);
            nodes.push(node);
        }
        let total = start;
        // The four buttons: two are shown, by what is hidden
        let mut button_nodes = Vec::new();
        for index in self.count..self.count + 4 {
            button_nodes.push(self.children[index].as_widget_mut().layout(
                &mut tree.children[index],
                renderer,
                &loose,
            ));
        }
        let button = button_nodes[0].size().width;
        height = height.max(button_nodes[0].size().height);
        let overflow = available.is_finite() && total > available + 0.5;
        let (lane_start, lane_width) = if overflow {
            (button, (available - 2.0 * button).max(0.0))
        } else {
            (0.0, total)
        };
        let state = tree.state.downcast_mut::<State>();
        // Where the strip is scrolled to
        let range = self
            .selected
            .and_then(|at| spans.get(at).map(|(start, width)| (*start, start + width)));
        let fresh = self
            .request
            .filter(|request| request.seq != state.handled)
            .and_then(|request| {
                let at = self.ids.iter().position(|id| *id == request.tab)?;
                Some((request.seq, request.wanted, spans[at]))
            });
        if let Some((seq, wanted, (start, width))) = fresh {
            state.handled = seq;
            let end = start + width;
            state.offset = match wanted {
                Wanted::Centre => geometry::centre(start, end, total, lane_width),
                Wanted::Reveal => geometry::reveal(state.offset, start, end, total, lane_width),
            };
        } else if (lane_width - state.viewport_width).abs() > 0.5
            && let Some((start, end)) = range
        {
            // Resizing moves only enough to keep the selection in view
            state.offset = geometry::reveal(state.offset, start, end, total, lane_width);
        }
        state.offset = geometry::clamp(state.offset, total, lane_width);
        state.viewport_width = lane_width;
        state.lane = (lane_start, lane_width);
        state.spans = spans.clone();
        let (left, right) = geometry::hidden(&spans, state.offset, lane_width);
        if !crate::layout::is_speculative() {
            self.shown.set((left, right));
        }
        let (on_left, on_right) = (
            self.count + usize::from(left == 0),
            self.count + 2 + usize::from(right >= self.count),
        );
        let offset = state.offset;
        let mut children = Vec::new();
        for (node, (start, _)) in nodes.into_iter().zip(&spans) {
            children.push(node.move_to(Point::new(lane_start - offset + start, 0.0)));
        }
        for (index, node) in button_nodes.into_iter().enumerate() {
            let active = self.count + index == on_left || self.count + index == on_right;
            let node = if overflow && active {
                let x = if index < 2 {
                    0.0
                } else {
                    lane_start + lane_width
                };
                node.move_to(Point::new(x, 0.0))
            } else {
                layout::Node::new(Size::ZERO)
            };
            children.push(node);
        }
        let width = if overflow { available } else { total };
        layout::Node::with_children(Size::new(width, height), children)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        let state = tree.state.downcast_ref::<State>();
        // Only the tabs in view, whole or in part, can be reached by key
        let (hidden_left, hidden_right) = self.hidden_counts(state);
        let overflowing = self.overflowing(state);
        let count = self.count;
        operation.traverse(&mut |operation| {
            for (index, ((child, tree), layout)) in self
                .children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
                .enumerate()
            {
                let hidden =
                    overflowing && index < count && (index < hidden_left || index >= hidden_right);
                if layout.bounds().width > 0.0 && !hidden {
                    child
                        .as_widget_mut()
                        .operate(tree, layout, renderer, operation);
                }
            }
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let (lane_start, lane_width) = tree.state.downcast_ref::<State>().lane;
        let lane = Rectangle::new(
            Point::new(bounds.x + lane_start, bounds.y),
            Size::new(lane_width, bounds.height),
        );
        let clipped = lane.intersection(viewport).unwrap_or_default();
        let overflowing = self.overflowing(tree.state.downcast_ref::<State>());
        let (left_on, right_on) = self.buttons(tree.state.downcast_ref::<State>());
        let masked = |cursor: mouse::Cursor, region: Rectangle| match cursor.position() {
            Some(at) if region.contains(at) => cursor,
            _ => mouse::Cursor::Unavailable,
        };
        for (index, ((child, tree), layout)) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
            .enumerate()
        {
            let (cursor, viewport) = if index < self.count {
                (masked(cursor, clipped), &clipped)
            } else if overflowing && (index == left_on || index == right_on) {
                (cursor, viewport)
            } else {
                continue;
            };
            child.as_widget_mut().update(
                tree, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
        }
        let state = tree.state.downcast_mut::<State>();
        if let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event
            && overflowing
            && !shell.is_event_captured()
            && cursor.is_over(bounds)
        {
            // The wheel scrolls the strip sideways, selecting nothing
            let (x, y) = match *delta {
                mouse::ScrollDelta::Lines { x, y } => (x * WHEEL_LINE, y * WHEEL_LINE),
                mouse::ScrollDelta::Pixels { x, y } => (x, y),
            };
            let total = state
                .spans
                .last()
                .map_or(0.0, |(start, width)| start + width);
            let moved = geometry::clamp(state.offset - (x + y), total, state.lane.1);
            if (moved - state.offset).abs() > f32::EPSILON {
                state.offset = moved;
                shell.invalidate_layout();
                shell.request_redraw();
            }
            shell.capture_event();
            return;
        }
        if matches!(
            event,
            Event::Dnd(cosmic::iced::clipboard::dnd::DndEvent::Source(
                cosmic::iced::clipboard::dnd::SourceEvent::Finished
                    | cosmic::iced::clipboard::dnd::SourceEvent::Cancelled
            ))
        ) {
            // A drag the app lifted out of the strip has ended
            state.press = None;
        }
        let over = |state: &State, at: Point| {
            if !clipped.contains(at) {
                return None;
            }
            (0..self.count).find(|&index| {
                let tab = self.tab_bounds(state, bounds.position(), index);
                at.x >= tab.x && at.x < tab.x + tab.width
            })
        };
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if !shell.is_event_captured() =>
            {
                if let Some(at) = cursor.position()
                    && let Some(tab) = over(state, at)
                {
                    state.press = Some(Press {
                        tab,
                        origin: at,
                        dragging: false,
                    });
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(press) = &mut state.press
                    && !press.dragging
                    && position.distance(press.origin) >= DRAG_THRESHOLD
                {
                    press.dragging = self.on_reorder.is_some();
                }
                if state.press.is_some() {
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(press) = state.press.take() {
                    let target = cursor.position().and_then(|at| over(state, at));
                    if press.dragging {
                        // Within the visible tabs only
                        if let (Some(target), Some(reorder)) = (target, &self.on_reorder)
                            && target != press.tab
                        {
                            shell.publish(reorder(self.ids[press.tab], target));
                        }
                        shell.capture_event();
                    } else if target == Some(press.tab) {
                        // A press on a visible tab: the strip stays where it is
                        shell.publish(self.selects[press.tab].clone());
                        shell.capture_event();
                    }
                    shell.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let lane = Rectangle::new(
            Point::new(bounds.x + state.lane.0, bounds.y),
            Size::new(state.lane.1, bounds.height),
        );
        let Some(clip) = lane.intersection(viewport) else {
            return;
        };
        let cursor_in = match cursor.position() {
            Some(at) if clip.contains(at) => cursor,
            _ => mouse::Cursor::Unavailable,
        };
        renderer.with_layer(clip, |renderer| {
            for index in 0..self.count {
                let layout = layout.children().nth(index).unwrap();
                self.children[index].as_widget().draw(
                    &tree.children[index],
                    renderer,
                    theme,
                    style,
                    layout,
                    cursor_in,
                    &clip,
                );
            }
        });
        if let Some(press) = state.press.as_ref().filter(|press| press.dragging)
            && let Some(at) = cursor.position()
            && let Some(target) = (0..self.count).find(|&index| {
                let tab = self.tab_bounds(state, bounds.position(), index);
                clip.contains(at) && at.x >= tab.x && at.x < tab.x + tab.width
            })
            && target != press.tab
        {
            let tab = self.tab_bounds(state, bounds.position(), target);
            let x = if target > press.tab {
                tab.x + tab.width
            } else {
                tab.x
            };
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle::new(
                        Point::new(x - 1.0, bounds.y),
                        Size::new(2.0, bounds.height),
                    ),
                    ..Default::default()
                },
                Color::from(theme.cosmic().accent_color()),
            );
        }
        if self.overflowing(state) {
            let (left, right) = self.buttons(state);
            for index in [left, right] {
                self.children[index].as_widget().draw(
                    &tree.children[index],
                    renderer,
                    theme,
                    style,
                    layout.children().nth(index).unwrap(),
                    cursor,
                    viewport,
                );
            }
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if state.press.as_ref().is_some_and(|press| press.dragging) {
            return mouse::Interaction::Grabbing;
        }
        let (left, right) = self.buttons(state);
        let overflowing = self.overflowing(state);
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .enumerate()
            .filter(|(index, _)| {
                *index < self.count || (overflowing && (*index == left || *index == right))
            })
            .map(|(_, ((child, tree), layout))| {
                child
                    .as_widget()
                    .mouse_interaction(tree, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        let state = tree.state.downcast_ref::<State>();
        let (left, right) = self.buttons(state);
        let overflowing = self.overflowing(state);
        let count = self.count;
        let overlays: Vec<_> = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
            .enumerate()
            .filter(|(index, _)| {
                *index < count || (overflowing && (*index == left || *index == right))
            })
            .filter_map(|(_, ((child, state), layout))| {
                child
                    .as_widget_mut()
                    .overlay(state, layout, renderer, viewport, translation)
            })
            .collect();
        (!overlays.is_empty()).then(|| overlay::Group::with_children(overlays).overlay())
    }
}

// --- A tab's parts ----------------------------------------------------------

/// What a tab's icon and close control are drawn in: its colour for the
/// selected tab, the container's text for the others, as COSMIC's tab bar
fn content_colour(theme: &Theme, selected: bool, identity: Option<Color>) -> Color {
    if selected {
        identity.unwrap_or_else(|| Color::from(theme.cosmic().accent_text_color()))
    } else {
        Color::from(theme.current_container().component.on)
    }
}

/// A tab's wash and underline around its content. Hover washes replace each
/// other: only the tab under the pointer has one, and it is the same wash
/// selected or not. The selected tab has its identity-coloured underline.
struct Wash<'a, M> {
    content: Element<'a, M>,
    selected: bool,
    /// The colour of the lines: the selected tab's identity, the strip's
    /// accent for them all as COSMIC's tab bar draws them
    line: Option<Color>,
    hovered: Rc<Cell<bool>>,
}

impl<M> Widget<M, Theme, Renderer> for Wash<'_, M> {
    forward_widget!(M, Theme, Renderer; children, diff, size, layout, operate, overlay, drag_destinations, a11y_nodes);
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let over = cursor.is_over(layout.bounds());
        if over != self.hovered.replace(over) {
            shell.request_redraw();
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let inner = self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        );
        if inner == mouse::Interaction::None && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            inner
        }
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let over = cursor.is_over(bounds);
        self.hovered.set(over);
        // As COSMIC's tab bar draws a tab: its line first, under the wash;
        // the wash (a fifth of the neutral for the selected tab, a third for
        // the hovered one, which replaces the selected's rather than adding
        // to it), top corners rounded; then its content
        let thick = if self.selected { UNDERLINE } else { THIN_LINE };
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle::new(
                    Point::new(bounds.x, bounds.y + bounds.height - thick),
                    Size::new(bounds.width, thick),
                ),
                ..Default::default()
            },
            self.line
                .unwrap_or_else(|| Color::from(theme.cosmic().accent_text_color())),
        );
        let cosmic = theme.cosmic();
        let radius = cosmic.corner_radii.radius_s;
        let corners = cosmic::iced::border::Radius::from([radius[0], radius[1], 0.0, 0.0]);
        let neutral = Color::from(cosmic.palette.neutral_5);
        let alpha = if over {
            Some(0.3)
        } else {
            self.selected.then_some(0.2)
        };
        if let Some(alpha) = alpha {
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    border: Border {
                        radius: corners,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                Color {
                    a: alpha,
                    ..neutral
                },
            );
        }
        let colour = content_colour(theme, self.selected, self.line);
        let style = renderer::Style {
            icon_color: colour,
            text_color: colour,
            ..*style
        };
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            &style,
            layout,
            cursor,
            viewport,
        );
    }
}

/// A title's room: all that its tab gives it, and never less than the
/// minimum. Its box is laid out in both weights at that width, so that
/// hovering never reflows the strip; the selected or hovered tab shows the
/// emphasised one.
struct Emphasis<'a, M> {
    normal: Element<'a, M>,
    strong: Element<'a, M>,
    minimum: f32,
    selected: bool,
    hovered: Rc<Cell<bool>>,
}

impl<M> Emphasis<'_, M> {
    fn emphasised(&self) -> bool {
        self.selected || self.hovered.get()
    }
}

impl<M> Widget<M, Theme, Renderer> for Emphasis<'_, M> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.normal), Tree::new(&self.strong)]
    }
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut [&mut self.normal, &mut self.strong]);
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let offered = limits.max().width;
        let width = if offered.is_finite() {
            offered.max(self.minimum)
        } else {
            self.minimum
        };
        let exact = layout::Limits::new(Size::new(width, 0.0), Size::new(width, f32::INFINITY));
        let strong = self
            .strong
            .as_widget_mut()
            .layout(&mut tree.children[1], renderer, &exact);
        let normal = self
            .normal
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, &exact);
        // An even height, its text at the top: centred in a tab, it sits as
        // libcosmic's tab bar sets a title, whole pixels from the top
        let height = (strong.size().height.max(normal.size().height) / 2.0).ceil() * 2.0;
        layout::Node::with_children(Size::new(width, height), vec![normal, strong])
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let mut layouts = layout.children();
        let (normal, strong) = (layouts.next().unwrap(), layouts.next().unwrap());
        let (element, tree, layout) = if self.emphasised() {
            (&mut self.strong, &mut tree.children[1], strong)
        } else {
            (&mut self.normal, &mut tree.children[0], normal)
        };
        element
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let mut layouts = layout.children();
        let (normal, strong) = (layouts.next().unwrap(), layouts.next().unwrap());
        let (element, tree, layout) = if self.emphasised() {
            (&mut self.strong, &mut tree.children[1], strong)
        } else {
            (&mut self.normal, &mut tree.children[0], normal)
        };
        element.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let mut layouts = layout.children();
        let (normal, strong) = (layouts.next().unwrap(), layouts.next().unwrap());
        let (element, tree, layout) = if self.emphasised() {
            (&self.strong, &tree.children[1], strong)
        } else {
            (&self.normal, &tree.children[0], normal)
        };
        element
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }
}

#[cfg(test)]
#[path = "tab_strip_tests.rs"]
mod tests;
