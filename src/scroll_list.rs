// SPDX-License-Identifier: GPL-3.0-only

//! A scrollable list of boxes, and its optional virtualisation.
//!
//! [`scroll_list`] declares one list along an [`Axis`]: some leading
//! children (a loading line, a "Load earlier" button), the items, and some
//! trailing children (a status line), each a gap after the child before
//! it. Along the main axis the list's extent grows with its content; across
//! it, its parent gives its size (a vertical list's width, a horizontal
//! list's height). The list owns the scrollable, its viewport and the
//! reader's place in it; the app supplies the items' keys and gaps, a child
//! for each item, and what each item holds, for an estimate of its extent
//! before it is laid out ([`Estimate`]).
//!
//! An ordinary list builds every item. A [`ScrollList::virtualised`] list
//! builds only the items that meet the viewport and a screen either side
//! of it; every other item is room of its known or estimated extent, so the
//! scrollable's content is as long as the whole list, and nothing but the
//! items near the view is laid out, diffed, drawn or sent events. Both are
//! the same contract: the same keys, children, reveal and reading position.
//!
//! The list keeps the reader's place across layouts: when items before the
//! view turn out larger or smaller than their estimates, rewrap at a new
//! cross size, or arrive, it moves the scroll offset by the difference, in the
//! same layout, so no frame shows the jump. Children are diffed by key, so
//! an item's widgets keep their state as the window slides.
//!
//! [`ListState`] is retained by the app for the list's logical lifetime
//! (a conversation, a panel's list), not the widget's, and it outlives
//! offscreen eviction and tab switches: several lists can share one widget
//! tree position, as tabs do, and each restores its own scroll position.
//! The app forwards [`ListAction`]s to [`ListState::apply`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Range;
use std::rc::Rc;

use cosmic::iced::advanced::widget::operation::Scrollable;
use cosmic::iced::advanced::widget::{Id, Operation, Tree, tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use cosmic::iced::widget::scrollable::{
    self, AbsoluteOffset, Direction, RelativeOffset, Scrollbar, Viewport,
};
use cosmic::iced::{Event, Length, Padding, Point, Rectangle, Size, Task, Vector};
use cosmic::{Element, Renderer, Theme, widget};

#[path = "scroll_list_estimate.rs"]
mod estimate;
#[path = "scroll_list_geometry.rs"]
mod geometry;

pub use estimate::{ContentSummary, Estimate, EstimateClass, MONO_SCALE, Segment, TextMetrics};
use geometry::Geometry;
pub use geometry::{Counts, ItemSpec, Last, Place, mix};

use crate::scroll::{SCROLLBAR_GAP, SCROLLBAR_WIDTH};

pub use crate::list_navigation::Axis;

/// The main axis's measure of sizes, offsets and points: along a vertical
/// list's height, a horizontal list's width
trait Along {
    fn main(self, size: Size) -> f32;
    fn cross(self, size: Size) -> f32;
    fn size(self, main: f32, cross: f32) -> Size;
    fn point(self, main: f32, cross: f32) -> Point;
    fn of(self, offset: Vector) -> f32;
    fn to(self, main: f32) -> AbsoluteOffset<Option<f32>>;
    fn by(self, main: f32) -> AbsoluteOffset;
    fn fraction(self, fraction: f32) -> RelativeOffset<Option<f32>>;
    fn end(self) -> scrollable::Position;
}

impl Along for Axis {
    fn main(self, size: Size) -> f32 {
        match self {
            Axis::Vertical => size.height,
            Axis::Horizontal => size.width,
        }
    }
    fn cross(self, size: Size) -> f32 {
        match self {
            Axis::Vertical => size.width,
            Axis::Horizontal => size.height,
        }
    }
    fn size(self, main: f32, cross: f32) -> Size {
        match self {
            Axis::Vertical => Size::new(cross, main),
            Axis::Horizontal => Size::new(main, cross),
        }
    }
    fn point(self, main: f32, cross: f32) -> Point {
        match self {
            Axis::Vertical => Point::new(cross, main),
            Axis::Horizontal => Point::new(main, cross),
        }
    }
    fn of(self, offset: Vector) -> f32 {
        match self {
            Axis::Vertical => offset.y,
            Axis::Horizontal => offset.x,
        }
    }
    fn to(self, main: f32) -> AbsoluteOffset<Option<f32>> {
        match self {
            Axis::Vertical => AbsoluteOffset {
                x: None,
                y: Some(main),
            },
            Axis::Horizontal => AbsoluteOffset {
                x: Some(main),
                y: None,
            },
        }
    }
    fn by(self, main: f32) -> AbsoluteOffset {
        match self {
            Axis::Vertical => AbsoluteOffset { x: 0.0, y: main },
            Axis::Horizontal => AbsoluteOffset { x: main, y: 0.0 },
        }
    }
    fn fraction(self, fraction: f32) -> RelativeOffset<Option<f32>> {
        match self {
            Axis::Vertical => RelativeOffset {
                x: None,
                y: Some(fraction),
            },
            Axis::Horizontal => RelativeOffset {
                x: Some(fraction),
                y: None,
            },
        }
    }
    fn end(self) -> scrollable::Position {
        match self {
            Axis::Vertical => RelativeOffset { x: 0.0, y: 1.0 }.into(),
            Axis::Horizontal => RelativeOffset { x: 1.0, y: 0.0 }.into(),
        }
    }
}

/// What a list's scrollable reports, for [`ListState::apply`]
#[derive(Clone, Debug)]
pub enum ListAction {
    /// The view scrolled, by the user or by a layout
    Scrolled(Viewport),
}

/// Where a revealed item is brought to
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reveal {
    /// Its start `margin` after the view's start, as a jump to it does
    Start { margin: f32 },
    /// Just into view, moving no more than needed, as keyboard
    /// navigation does; not at all if it is in view already
    Nearest,
}

/// The retained state of one list: where its items are, how far it is
/// scrolled and whether it follows its end
#[derive(Debug)]
pub struct ListState<K> {
    geometry: Rc<RefCell<Geometry<K>>>,
    id: Id,
    axis: Axis,
    /// Restored when the list's widget tree position is reused by another
    /// list, as tabs reuse theirs
    position: scrollable::Position,
    follow: bool,
    offset: Option<f32>,
    page: f32,
    content: f32,
    /// The reader's place while not following the end, as last scrolled
    place: Option<Place<K>>,
}

impl<K: Copy + Eq + Hash> Default for ListState<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Copy + Eq + Hash> ListState<K> {
    /// A vertical list at its start
    pub fn new() -> Self {
        Self {
            geometry: Rc::default(),
            id: Id::unique(),
            axis: Axis::Vertical,
            position: AbsoluteOffset::default().into(),
            follow: false,
            offset: None,
            page: 0.0,
            content: 0.0,
            place: None,
        }
    }

    /// A list along `axis`, at its start
    pub fn along(axis: Axis) -> Self {
        Self {
            axis,
            ..Self::new()
        }
    }

    /// A vertical list that follows its end as items arrive, as a
    /// conversation does
    pub fn following_end() -> Self {
        let mut state = Self::new();
        state.follow = true;
        state.position = state.axis.end();
        state
    }

    /// The list's main axis
    pub fn axis(&self) -> Axis {
        self.axis
    }

    /// The scrollable's id, for operations the list does not offer yet
    pub fn id(&self) -> &Id {
        &self.id
    }

    /// Bring the list's state up to date with what its scrollable reported.
    /// The task returned pins a deliberate scroll to the end there
    pub fn apply<T: Send + 'static>(&mut self, action: ListAction) -> Task<T> {
        match action {
            ListAction::Scrolled(viewport) => self.scrolled(&viewport),
        }
    }

    fn scrolled<T: Send + 'static>(&mut self, viewport: &Viewport) -> Task<T> {
        let axis = self.axis;
        let bounds = axis.main(viewport.bounds().size());
        let content = axis.main(viewport.content_bounds().size());
        let offset = axis.of(Vector::new(
            viewport.absolute_offset().x,
            viewport.absolute_offset().y,
        ));
        // When the layout changes under the reader, who did not scroll, the
        // list has kept their place already, in the same layout
        let at_end = offset + bounds >= content - 0.5;
        // Only a vertical scrollable keeps a pin to its end as content grows
        let pinned = match axis {
            Axis::Vertical => viewport.is_pinned_to_end(),
            Axis::Horizontal => self.follow && at_end,
        };
        self.follow = pinned || (viewport.is_user_scroll() && at_end);
        self.position = if self.follow {
            axis.end()
        } else {
            viewport.position()
        };
        self.page = bounds;
        self.content = content;
        self.offset = Some(offset);
        if !self.follow
            && let Some(place) = self.geometry.borrow().place_at(offset)
        {
            self.place = Some(place);
        }
        if viewport.is_user_scroll() && at_end && !pinned {
            return self.pin_to_end();
        }
        Task::none()
    }

    /// Show the list from its start, not following its end, as a fresh view
    /// of earlier items does
    pub fn show_start(&mut self) {
        self.follow = false;
        self.position = AbsoluteOffset::default().into();
        self.offset = Some(0.0);
    }

    /// The scroll position restored when the list's widget is rebuilt
    pub fn position(&self) -> scrollable::Position {
        self.position
    }

    /// Whether the list follows its end as items arrive
    pub fn follows_end(&self) -> bool {
        self.follow
    }

    /// Stop following the end, as a deliberate move elsewhere does, keeping
    /// the view where it is
    pub fn stop_following(&mut self) {
        self.follow = false;
    }

    /// Scroll so that the content's `offset` is at the view's start, as
    /// dragging a stand-in for the scrollbar does. Stops following the end
    pub fn scroll_to<T: Send + 'static>(&mut self, offset: f32) -> Task<T> {
        self.follow = false;
        self.offset = Some(offset.max(0.0));
        scrollable::scroll_to(self.id.clone(), self.axis.to(offset.max(0.0)))
    }

    /// Scroll `fraction` of the way along, as a jump to somewhere the list
    /// cannot place by key (an item not loaded yet) does. Stops following
    /// the end
    pub fn scroll_to_fraction<T: Send + 'static>(&mut self, fraction: f32) -> Task<T> {
        self.follow = false;
        scrollable::snap_to(
            self.id.clone(),
            self.axis.fraction(fraction.clamp(0.0, 1.0)),
        )
    }

    /// Follow the end again and go there
    pub fn follow_end<T: Send + 'static>(&mut self) -> Task<T> {
        self.follow = true;
        self.position = self.axis.end();
        scrollable::snap_to(self.id.clone(), self.axis.fraction(1.0))
    }

    /// Pin a deliberate return to the end, only if the user has not
    /// scrolled away again by the time this reaches the scrollable
    fn pin_to_end<T: Send + 'static>(&self) -> Task<T> {
        struct Pin(Id, Axis);
        impl<T> Operation<T> for Pin {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
                operate(self);
            }
            fn scrollable(
                &mut self,
                id: Option<&Id>,
                bounds: Rectangle,
                content: Rectangle,
                translation: Vector,
                state: &mut dyn Scrollable,
            ) {
                let axis = self.1;
                if id == Some(&self.0)
                    && axis.of(translation) + axis.main(bounds.size())
                        >= axis.main(content.size()) - 0.5
                {
                    state.snap_to(axis.fraction(1.0));
                }
            }
        }
        cosmic::iced::runtime::task::widget(Pin(self.id.clone(), self.axis))
    }

    /// Scroll by `pages` of the view, keeping a little of the last page in
    /// sight to read on from; forward to the end follows it again
    pub fn scroll_pages<T: Send + 'static>(&mut self, pages: f32) -> Task<T> {
        if pages < 0.0 {
            self.follow = false;
        }
        let page = if self.page > 0.0 {
            self.page * 0.9
        } else {
            400.0
        };
        let scroll = scrollable::scroll_by(self.id.clone(), self.axis.by(pages * page));
        if pages > 0.0 {
            scroll.chain(self.pin_to_end())
        } else {
            scroll
        }
    }

    /// Bring item `key` into view, built or not, and keep it there as the
    /// items around it are laid out and their extents corrected. Stops
    /// following the end. None if the list has no such item shown
    pub fn reveal<T: Send + 'static>(&mut self, key: K, reveal: Reveal) -> Option<Task<T>> {
        let mut geometry = self.geometry.borrow_mut();
        let start = geometry.start_of(key)?;
        let offset = match reveal {
            Reveal::Start { margin } => (start - margin).max(0.0),
            Reveal::Nearest => {
                let end = geometry.end_of(key)?;
                let view = self.view_start_in(&geometry);
                if start < view {
                    start
                } else if end > view + self.page_extent() {
                    (end - self.page_extent()).max(0.0)
                } else {
                    return Some(Task::none());
                }
            }
        };
        self.follow = false;
        self.offset = Some(offset);
        // Laid out before: the list goes there itself at its next layout
        // and keeps this place from then on, rather than what was at its
        // start
        if geometry.keep((key, 0.0, start - offset)) {
            return Some(Task::none());
        }
        drop(geometry);
        Some(scrollable::scroll_to(self.id.clone(), self.axis.to(offset)))
    }

    /// Where the view's start stands, as last scrolled or at the end
    pub fn view_start(&self) -> f32 {
        self.view_start_in(&self.geometry.borrow())
    }

    fn view_start_in(&self, geometry: &Geometry<K>) -> f32 {
        match self.offset {
            Some(start) if !self.follow => start,
            _ => (geometry.content_extent() - self.page_extent()).max(0.0),
        }
    }

    /// The view's extent, as last scrolled, or a likely one before then
    pub fn page_extent(&self) -> f32 {
        if self.page > 0.0 { self.page } else { 1000.0 }
    }

    /// The items a virtualised list builds: those meeting the view and a
    /// screen either side of it
    pub fn window(&self) -> Range<usize> {
        let geometry = self.geometry.borrow();
        geometry.window(self.view_start_in(&geometry), self.page_extent())
    }

    /// Whether item `index` meets the view, rather than only being near it
    pub fn on_screen(&self, index: usize) -> bool {
        let geometry = self.geometry.borrow();
        if index >= geometry.len() || !geometry.item(index).shown {
            return false;
        }
        let view = self.view_start_in(&geometry);
        let at = geometry.lead + geometry.offset_of(index);
        at + geometry.item(index).extent >= view && at <= view + self.page_extent()
    }

    /// The reader's place, as last scrolled while not following the end
    pub fn place(&self) -> Option<Place<K>> {
        self.place
    }

    /// The place at `offset` in the content
    pub fn place_at(&self, offset: f32) -> Option<Place<K>> {
        self.geometry.borrow().place_at(offset)
    }

    /// Where item `key`'s start, past its gap, is in the content
    pub fn start_of(&self, key: K) -> Option<f32> {
        self.geometry.borrow().start_of(key)
    }

    /// Item `key`'s index, among all the items synced
    pub fn index_of(&self, key: K) -> Option<usize> {
        self.geometry.borrow().index_of(key)
    }

    /// The content's extent: its items' known and estimated extents and
    /// the room around them
    pub fn content_extent(&self) -> f32 {
        let geometry = self.geometry.borrow();
        if geometry.last.is_some() || self.content <= 0.0 {
            geometry.content_extent()
        } else {
            self.content
        }
    }

    /// The view as last laid out: its start, its content's extent and its
    /// own, or as last scrolled before the first layout
    pub fn stands(&self) -> (f32, f32, f32) {
        match self.geometry.borrow().last {
            Some(last) => {
                let main = |(width, height): (f32, f32)| self.axis.main(Size::new(width, height));
                (last.offset, main(last.content), main(last.bounds))
            }
            None => (self.offset.unwrap_or_default(), self.content, self.page),
        }
    }

    /// Item `key`'s content was laid out `extent` long elsewhere at the
    /// list's cross size (by a worker that shapes text): a better estimate
    /// until the list lays the item out itself
    pub fn content_extent_known(&self, key: K, extent: f32) {
        self.geometry.borrow_mut().content_extent_known(key, extent);
    }

    /// Forget every item, as when what the list shows is unloaded
    pub fn clear(&self) {
        self.geometry.borrow_mut().clear();
    }

    /// Whether any item takes room
    pub fn any_shown(&self) -> bool {
        self.geometry.borrow().total() > 0.0
    }

    /// The keys of the items laid out at the current cross size, for tests
    pub fn measured_keys(&self) -> Vec<K> {
        let geometry = self.geometry.borrow();
        (0..geometry.len())
            .map(|i| geometry.item(i))
            .filter(|item| item.shown && item.exact)
            .map(|item| item.key)
            .collect()
    }

    /// What the list did, for tests and benchmarks
    pub fn counts(&self) -> Counts {
        self.geometry.borrow().counts
    }
}

/// An item's child, as the app builds it
pub struct ListChild<'a, Message> {
    element: Element<'a, Message>,
    provisional: bool,
}

impl<'a, Message> ListChild<'a, Message> {
    pub fn new(element: impl Into<Element<'a, Message>>) -> Self {
        Self {
            element: element.into(),
            provisional: false,
        }
    }

    /// A child standing in for content still being prepared (text a worker
    /// has not shaped yet): room is kept at the item's estimated extent,
    /// not at the extent of what stands in for it meanwhile
    pub fn provisional(element: impl Into<Element<'a, Message>>) -> Self {
        Self {
            element: element.into(),
            provisional: true,
        }
    }
}

/// How the list's scrollbar shows
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollbarLane {
    /// In its own lane beside the content, as every list's is by default
    #[default]
    Lane,
    /// None: something else stands in for it, as a conversation's jump
    /// rail does
    Hidden,
}

/// Declare a scrollable list, along the axis of `state`, its retained
/// state. Add its leading children, then its [`ScrollList::items`], then
/// its trailing children, in that order
pub fn scroll_list<'a, K: Copy + Eq + Hash + 'static, Message: 'a>(
    state: &'a ListState<K>,
) -> ScrollList<'a, K, Message> {
    ScrollList {
        state,
        virtualised: false,
        spacing: 0.0,
        padding: Padding::ZERO,
        max_cross: f32::INFINITY,
        metrics: TextMetrics::default(),
        scrollbar: ScrollbarLane::Lane,
        children: Vec::new(),
        keys: Vec::new(),
        roles: Vec::new(),
        items_added: false,
        content: None,
        on_action: None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Key<K> {
    Leading(usize),
    Item(K),
    Trailing(usize),
}

#[derive(Clone, Copy, Debug)]
enum Role {
    /// Before the items, with the gap before it
    Leading(f32),
    /// The item at this index, and whether it is provisional: then its
    /// extent stays the estimate
    Item(usize, bool),
    /// After the items, with the gap before it
    Trailing(f32),
}

type ContentMap<'a, Message> = Box<dyn FnOnce(Element<'a, Message>) -> Element<'a, Message> + 'a>;

/// A list being declared; see [`scroll_list`]
pub struct ScrollList<'a, K, Message> {
    state: &'a ListState<K>,
    virtualised: bool,
    spacing: f32,
    padding: Padding,
    max_cross: f32,
    metrics: TextMetrics,
    scrollbar: ScrollbarLane,
    children: Vec<Element<'a, Message>>,
    keys: Vec<Key<K>>,
    roles: Vec<Role>,
    items_added: bool,
    content: Option<ContentMap<'a, Message>>,
    on_action: Option<Rc<dyn Fn(ListAction) -> Message + 'a>>,
}

impl<'a, K: Copy + Eq + Hash + 'static, Message: 'a> ScrollList<'a, K, Message> {
    /// Build only the items near the view (`true`), or every item
    pub fn virtualised(mut self, virtualised: bool) -> Self {
        self.virtualised = virtualised;
        self
    }

    /// The gap before each leading and trailing child but the list's first
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    /// The list's largest cross size, padding included (a vertical list's
    /// widest); it stands at the start of the content's cross axis unless
    /// [`Self::map_content`] places it
    pub fn max_cross(mut self, max_cross: f32) -> Self {
        self.max_cross = max_cross;
        self
    }

    /// The text measures item estimates are made with
    pub fn metrics(mut self, metrics: TextMetrics) -> Self {
        self.metrics = metrics;
        self
    }

    pub fn scrollbar(mut self, scrollbar: ScrollbarLane) -> Self {
        self.scrollbar = scrollbar;
        self
    }

    /// What the list's scrollable reports, as the app's message
    pub fn on_action(mut self, on_action: impl Fn(ListAction) -> Message + 'a) -> Self {
        self.on_action = Some(Rc::new(on_action));
        self
    }

    /// Place or decorate the column inside the scrollable: centred, on a
    /// page's colour, under a context menu of its free space
    pub fn map_content(
        mut self,
        map: impl FnOnce(Element<'a, Message>) -> Element<'a, Message> + 'a,
    ) -> Self {
        self.content = Some(Box::new(map));
        self
    }

    /// Whether anything is before the next child
    fn anything_before(&self) -> bool {
        !self.children.is_empty() || (self.items_added && self.state.any_shown())
    }

    /// A child before the items
    pub fn leading(mut self, element: impl Into<Element<'a, Message>>) -> Self {
        debug_assert!(!self.items_added, "leading children come before the items");
        let gap = if self.anything_before() {
            self.spacing
        } else {
            0.0
        };
        let key = Key::Leading(self.children.len());
        self.push(key, Role::Leading(gap), element.into());
        self
    }

    /// The items: `len` of them, item `i` described by `spec(i)`, its
    /// estimate by `estimate(i)` (asked only when its revision changed) and
    /// its child by `child(i)` (asked only for items that are built: every
    /// shown item, or those near the view when virtualised). Nothing is
    /// recomputed while `revision` stays the same. The first shown item has
    /// no gap before it when nothing leads the list
    pub fn items(
        mut self,
        revision: u64,
        len: usize,
        spec: impl Fn(usize) -> ItemSpec<K>,
        estimate: impl Fn(usize) -> Estimate,
        child: impl Fn(usize) -> ListChild<'a, Message>,
    ) -> Self {
        debug_assert!(!self.items_added, "a list has one run of items");
        let leading = !self.children.is_empty();
        let revision = mix(mix(revision, len as u64), u64::from(leading));
        self.state
            .geometry
            .borrow_mut()
            .sync(revision, len, leading, spec, estimate);
        let window = if self.virtualised {
            self.state.window()
        } else {
            0..len
        };
        let mut built = 0;
        for index in window {
            if !self.state.geometry.borrow().item(index).shown {
                continue;
            }
            built += 1;
            let key = self.state.geometry.borrow().item(index).key;
            let ListChild {
                element,
                provisional,
            } = child(index);
            self.push(Key::Item(key), Role::Item(index, provisional), element);
        }
        self.state.geometry.borrow_mut().counts.built += built;
        self.items_added = true;
        self
    }

    /// A child after the items
    pub fn trailing(mut self, element: impl Into<Element<'a, Message>>) -> Self {
        let gap = if self.anything_before() {
            self.spacing
        } else {
            0.0
        };
        let key = Key::Trailing(self.children.len());
        self.push(key, Role::Trailing(gap), element.into());
        self
    }

    fn push(&mut self, key: Key<K>, role: Role, element: Element<'a, Message>) {
        self.keys.push(key);
        self.roles.push(role);
        self.children.push(element);
    }
}

impl<'a, K: Copy + Eq + Hash + 'static, Message: 'a> From<ScrollList<'a, K, Message>>
    for Element<'a, Message>
{
    fn from(list: ScrollList<'a, K, Message>) -> Self {
        let state = list.state;
        let axis = state.axis;
        let column = Element::new(Column {
            geometry: state.geometry.clone(),
            axis,
            metrics: list.metrics,
            padding: list.padding,
            max_cross: list.max_cross,
            children: list.children,
            keys: list.keys,
            roles: list.roles,
        });
        let content = match list.content {
            Some(map) => map(column),
            None => column,
        };
        let scrollbar = match list.scrollbar {
            ScrollbarLane::Lane => Scrollbar::new()
                .width(SCROLLBAR_WIDTH)
                .scroller_width(SCROLLBAR_WIDTH)
                .spacing(SCROLLBAR_GAP),
            ScrollbarLane::Hidden => Scrollbar::hidden(),
        };
        let direction = match axis {
            Axis::Vertical => Direction::Vertical(scrollbar),
            Axis::Horizontal => Direction::Horizontal(scrollbar),
        };
        let mut scroll = widget::scrollable(content)
            .direction(direction)
            .id(state.id.clone())
            .initial_position(state.position)
            .height(Length::Fill)
            .width(Length::Fill);
        if let Some(on_action) = list.on_action {
            scroll = scroll.on_scroll(move |viewport| on_action(ListAction::Scrolled(viewport)));
        }
        Element::new(Anchored {
            content: scroll.into(),
            id: state.id.clone(),
            axis,
            geometry: state.geometry.clone(),
            follow: state.follow,
        })
    }
}

#[derive(Default)]
struct ColumnState<K> {
    keys: Vec<Key<K>>,
}

/// The list's children along its axis: leading, the items built,
/// trailing. Every item not built is room of its known or estimated extent
struct Column<'a, K, Message> {
    geometry: Rc<RefCell<Geometry<K>>>,
    axis: Axis,
    metrics: TextMetrics,
    padding: Padding,
    max_cross: f32,
    children: Vec<Element<'a, Message>>,
    keys: Vec<Key<K>>,
    roles: Vec<Role>,
}

impl<K: Copy + Eq + Hash + 'static, Message> Widget<Message, Theme, Renderer>
    for Column<'_, K, Message>
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<ColumnState<K>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(ColumnState {
            keys: self.keys.clone(),
        })
    }

    fn children(&self) -> Vec<Tree> {
        self.children
            .iter()
            .map(|child| Tree::new(child.as_widget()))
            .collect()
    }

    fn diff(&mut self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<ColumnState<K>>();
        if state.keys == self.keys {
            tree.diff_children(&mut self.children);
            return;
        }
        // By key: an item's widgets keep their state wherever it moves.
        // Through the tree's own diff, which makes the state afresh where
        // the widget under a key changed kind, and gives named widgets back
        // the state the runtime set aside for them
        let mut old: HashMap<Key<K>, Tree> = state
            .keys
            .iter()
            .copied()
            .zip(std::mem::take(&mut tree.children))
            .collect();
        tree.children = self
            .children
            .iter_mut()
            .zip(&self.keys)
            .map(|(child, key)| {
                let mut child_tree = old
                    .remove(key)
                    .unwrap_or_else(|| Tree::new(child.as_widget()));
                child_tree.diff(child.as_widget_mut());
                child_tree
            })
            .collect();
        state.keys.clone_from(&self.keys);
    }

    fn size(&self) -> Size<Length> {
        match self.axis {
            Axis::Vertical => Size::new(Length::Fill, Length::Shrink),
            Axis::Horizontal => Size::new(Length::Shrink, Length::Fill),
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let axis = self.axis;
        let padding = self.padding;
        // Padding at the main axis's start and end, and across it
        let (lead_pad, trail_pad, cross_start, cross_pad) = match axis {
            Axis::Vertical => (
                padding.top,
                padding.bottom,
                padding.left,
                padding.left + padding.right,
            ),
            Axis::Horizontal => (
                padding.left,
                padding.right,
                padding.top,
                padding.top + padding.bottom,
            ),
        };
        let size = self.size();
        let limits = match axis {
            Axis::Vertical => limits.max_width(self.max_cross),
            Axis::Horizontal => limits.max_height(self.max_cross),
        }
        .width(size.width)
        .height(size.height);
        let cross = axis.cross(limits.max());
        let inner = (cross - cross_pad).max(0.0);
        // As a column or row lays its children out: the cross size given,
        // the main axis unbounded and compressed as the scrollable asks
        let compression = limits.compression();
        let child_limits = match axis {
            Axis::Vertical => layout::Limits::with_compression(
                Size::ZERO,
                Size::new(inner, f32::INFINITY),
                Size::new(false, compression.height),
            ),
            Axis::Horizontal => layout::Limits::with_compression(
                Size::ZERO,
                Size::new(f32::INFINITY, inner),
                Size::new(compression.width, false),
            ),
        };
        // A disposable measurement reads the geometry but commits nothing
        // to it: the retained extents are those of the layout presented
        let commit = !crate::layout::is_speculative();
        let mut geometry = self.geometry.borrow_mut();
        if commit {
            geometry.set_cross(inner, self.metrics);
        }
        let mut at = lead_pad;
        // Where the first item's start is: after the leading children
        let mut items_lead = None;
        let mut after_items = false;
        let mut nodes = Vec::with_capacity(self.children.len());
        for ((child, child_tree), role) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(&self.roles)
        {
            let node = child
                .as_widget_mut()
                .layout(child_tree, renderer, &child_limits);
            let extent = axis.main(node.size());
            match *role {
                Role::Leading(gap) => {
                    nodes.push(node.move_to(axis.point(at + gap, cross_start)));
                    at += gap + extent;
                }
                Role::Item(index, provisional) => {
                    let lead = *items_lead.get_or_insert(at);
                    let gap = geometry.item(index).gap;
                    if commit && !provisional {
                        geometry.measured(index, gap + extent);
                    }
                    nodes.push(
                        node.move_to(
                            axis.point(lead + geometry.offset_of(index) + gap, cross_start),
                        ),
                    );
                }
                Role::Trailing(gap) => {
                    if !after_items {
                        at = *items_lead.get_or_insert(at) + geometry.total();
                        after_items = true;
                    }
                    nodes.push(node.move_to(axis.point(at + gap, cross_start)));
                    at += gap + extent;
                }
            }
        }
        let items_lead = items_lead.unwrap_or(at);
        if !after_items {
            at = items_lead + geometry.total();
        }
        let end = at + trail_pad;
        if commit {
            geometry.lead = items_lead;
            geometry.trail = end - (items_lead + geometry.total());
        }
        let size = limits.resolve(size.width, size.height, axis.size(end, cross));
        layout::Node::with_children(size, nodes)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((child, state), child_layout) in self
                .children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                child.as_widget_mut().operate(
                    state,
                    child_layout.with_virtual_offset(layout.virtual_offset()),
                    renderer,
                    operation,
                );
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
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        for ((child, state), child_layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().update(
                state,
                event,
                child_layout.with_virtual_offset(layout.virtual_offset()),
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
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
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), child_layout)| {
                child.as_widget().mouse_interaction(
                    state,
                    child_layout.with_virtual_offset(layout.virtual_offset()),
                    cursor,
                    viewport,
                    renderer,
                )
            })
            .max()
            .unwrap_or_default()
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
        if layout.bounds().intersection(viewport).is_none() {
            return;
        }
        for ((child, state), child_layout) in self
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .filter(|(_, child_layout)| child_layout.bounds().intersects(viewport))
        {
            child.as_widget().draw(
                state,
                renderer,
                theme,
                style,
                child_layout.with_virtual_offset(layout.virtual_offset()),
                cursor,
                viewport,
            );
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }

    fn drag_destinations(
        &self,
        state: &Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        dnd_rectangles: &mut cosmic::iced::advanced::clipboard::DndDestinationRectangles,
    ) {
        for ((child, state), child_layout) in self
            .children
            .iter()
            .zip(&state.children)
            .zip(layout.children())
        {
            child
                .as_widget()
                .drag_destinations(state, child_layout, renderer, dnd_rectangles);
        }
    }
}

/// The scrollable `id`'s offset, bounds and content, and a new offset to
/// give it along `axis`; nothing inside it is visited
struct Probe {
    id: Id,
    axis: Axis,
    found: Option<(Rectangle, Rectangle, Vector)>,
    scroll_to: Option<f32>,
}

impl Operation for Probe {
    fn traverse(&mut self, _operate: &mut dyn FnMut(&mut dyn Operation)) {}

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        translation: Vector,
        state: &mut dyn Scrollable,
    ) {
        if id == Some(&self.id) {
            self.found = Some((bounds, content, translation));
            if let Some(offset) = self.scroll_to {
                state.scroll_to(self.axis.to(offset));
            }
        }
    }
}

/// The list's scrollable, keeping the reader's place in its items across
/// layouts unless it follows the end
struct Anchored<'a, K, Message> {
    content: Element<'a, Message>,
    id: Id,
    axis: Axis,
    geometry: Rc<RefCell<Geometry<K>>>,
    follow: bool,
}

impl<K: Copy + Eq + Hash, Message> Anchored<'_, K, Message> {
    fn probe(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        node: &layout::Node,
        scroll_to: Option<f32>,
    ) -> Option<(Rectangle, Rectangle, Vector)> {
        let mut probe = Probe {
            id: self.id.clone(),
            axis: self.axis,
            found: None,
            scroll_to,
        };
        self.content
            .as_widget_mut()
            .operate(tree, Layout::new(node), renderer, &mut probe);
        probe.found
    }
}

impl<K: Copy + Eq + Hash, Message> Widget<Message, Theme, Renderer> for Anchored<'_, K, Message> {
    forward_widget!(Message, Theme, Renderer; children, diff, size, size_hint, draw, update, operate, a11y_nodes, mouse_interaction, overlay, drag_destinations);

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        // A disposable measurement neither scrolls nor moves the place
        if crate::layout::is_speculative() {
            return self
                .content
                .as_widget_mut()
                .layout(&mut tree.children[0], renderer, limits);
        }
        let axis = self.axis;
        // Where the scrollable's start stands now, against the layout it had
        let last = self.geometry.borrow().last;
        let before = last.and_then(|last| {
            let node = layout::Node::with_children(
                Size::new(last.bounds.0, last.bounds.1),
                vec![layout::Node::new(Size::new(last.content.0, last.content.1))],
            );
            self.probe(&mut tree.children[0], renderer, &node, None)
                .map(|(_, _, translation)| axis.of(translation))
        });
        self.geometry.borrow_mut().begin_layout(before);
        let node = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        let Some((bounds, content, translation)) =
            self.probe(&mut tree.children[0], renderer, &node, None)
        else {
            return node;
        };
        let mut offset = axis.of(translation);
        let target = self.geometry.borrow().target();
        if !self.follow
            && let Some(target) = target
        {
            let most = (axis.main(content.size()) - axis.main(bounds.size())).max(0.0);
            let target = target.clamp(0.0, most);
            if (target - offset).abs() > 0.5 {
                self.probe(&mut tree.children[0], renderer, &node, Some(target));
                self.geometry.borrow_mut().counts.kept += 1;
                offset = target;
            }
        }
        self.geometry.borrow_mut().end_layout(
            offset,
            (bounds.width, bounds.height),
            (content.width, content.height),
        );
        node
    }
}
