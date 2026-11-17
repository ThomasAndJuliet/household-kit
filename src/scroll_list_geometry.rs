// SPDX-License-Identifier: GPL-3.0-only

//! A list's items: their order, heights and positions.
//!
//! Each item has a stable key, a revision changed when what it shows
//! changes, and a height: exact once the list has laid it out at the
//! current width, estimated until then ([`super::Estimate`]). Positions come
//! from a [`Positions`] tree of running sums, so one item's height changes
//! in O(log n), and the item at an offset and the offset of an item are
//! found in O(log n).
//!
//! The reader's place is an item, how far into it the view's top is, and
//! the room above it ([`Place`]). The list keeps it where it was across
//! layouts: when an item above the view turns out taller or shorter than
//! estimated, or the text rewraps, or items arrive above, the scroll offset
//! moves by the difference, so what is on screen stays put.

use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Range;

use super::estimate::{Estimate, EstimateClass, TextMetrics};

/// What the list is told of each item when it is brought up to date
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemSpec<K> {
    /// Unique within the list and stable while the item exists
    pub key: K,
    /// Changes whenever what the item shows changes; the item's estimate
    /// is asked for again only then
    pub revision: u64,
    pub class: EstimateClass,
    /// The room above it when another child is before it
    pub gap: f32,
    /// Whether it is shown: a hidden item takes no room and is never built
    pub shown: bool,
}

/// The reader's place: an item, how far into it (past its gap) the view's
/// top is, as a fraction of its height, and the room above it still in
/// view
pub type Place<K> = (K, f32, f32);

#[derive(Clone, Debug)]
pub(crate) struct Item<K> {
    pub key: K,
    /// Its height, the gap above it included: 0 while hidden
    pub height: f32,
    /// Whether `height` was laid out, rather than estimated
    pub exact: bool,
    /// The width `height` is for
    width: f32,
    /// The room above it, which its height includes
    pub gap: f32,
    pub shown: bool,
    class: EstimateClass,
    /// The exact height it had at the width before, kept so that going back
    /// to that width (a drag back and forth, a panel opened and closed)
    /// needs no estimate
    previous: Option<(f32, f32)>,
    revision: u64,
    estimate: Estimate,
    /// Its content's height, as laid out elsewhere at the current width, if
    /// told: an estimate then needs only the room around it
    content: Option<f32>,
}

/// Running sums of the items' heights (a Fenwick tree): a height changed,
/// a prefix summed and the item at an offset found, each in O(log n)
#[derive(Clone, Debug, Default)]
pub(crate) struct Positions {
    /// 1-based partial sums
    tree: Vec<f64>,
    values: Vec<f32>,
}

impl Positions {
    pub fn new(values: Vec<f32>) -> Self {
        let mut tree = vec![0.0; values.len() + 1];
        for (i, value) in values.iter().enumerate() {
            let i = i + 1;
            tree[i] += f64::from(*value);
            let parent = i + (i & i.wrapping_neg());
            if parent < tree.len() {
                tree[parent] += tree[i];
            }
        }
        Self { tree, values }
    }

    pub fn set(&mut self, index: usize, value: f32) {
        let delta = f64::from(value) - f64::from(self.values[index]);
        self.values[index] = value;
        if delta == 0.0 {
            return;
        }
        let mut i = index + 1;
        while i < self.tree.len() {
            self.tree[i] += delta;
            i += i & i.wrapping_neg();
        }
    }

    /// The sum of the first `count` values
    pub fn prefix(&self, count: usize) -> f32 {
        let mut i = count.min(self.values.len());
        let mut sum = 0.0;
        while i > 0 {
            sum += self.tree[i];
            i -= i & i.wrapping_neg();
        }
        sum as f32
    }

    pub fn total(&self) -> f32 {
        self.prefix(self.values.len())
    }

    /// The item `offset` falls in: the one whose span holds it, past any
    /// of no height; the last at or past the end
    pub fn find(&self, offset: f32) -> usize {
        let n = self.values.len();
        if n == 0 {
            return 0;
        }
        let mut position = 0usize;
        let mut remaining = f64::from(offset.max(0.0));
        let mut step = 1usize << (usize::BITS - 1 - n.leading_zeros());
        while step > 0 {
            let next = position + step;
            if next <= n && self.tree[next] <= remaining {
                position = next;
                remaining -= self.tree[next];
            }
            step >>= 1;
        }
        position.min(n - 1)
    }
}

/// What was last laid out: where the view's top was, the place there, and
/// the scrollable's and its content's sizes, to tell later where its
/// offset stands
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Last<K> {
    pub offset: f32,
    pub place: Option<Place<K>>,
    pub bounds: (f32, f32),
    pub content: (f32, f32),
}

/// Counts of what the list did, for tests and benchmarks
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Items given widgets
    pub built: u64,
    /// Items laid out and measured
    pub measured: u64,
    /// Heights estimated
    pub estimated: u64,
    /// Scroll offsets moved to keep the reader's place
    pub kept: u64,
    /// Full passes over the items: syncs that changed something, and new
    /// widths
    pub passes: u64,
}

#[derive(Debug)]
pub(crate) struct Geometry<K> {
    items: Vec<Item<K>>,
    by_key: HashMap<K, usize>,
    positions: Positions,
    /// What the items were last synced from
    synced: Option<u64>,
    width: f32,
    metrics: TextMetrics,
    /// Where the first item's top is in the content: below the padding and
    /// the leading children, as last laid out
    pub top: f32,
    /// Below the last item, to the content's end, as last laid out
    pub bottom: f32,
    /// What the estimate of each class is off by, as learned from those
    /// laid out: a fixed estimate's whole height; a content estimate's
    /// share
    learned: HashMap<EstimateClass, f32>,
    /// The room each class of item takes around its content, as learned
    /// from those laid out whose content height was told
    around: HashMap<EstimateClass, f32>,
    pub last: Option<Last<K>>,
    /// The place to keep through the layout under way
    pending: Option<Place<K>>,
    /// How much heights above the place last shown changed since it was
    /// laid out, outside a layout: a scroll since moves from where that
    /// place is now
    moved: f32,
    pub counts: Counts,
}

impl<K> Default for Geometry<K> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            by_key: HashMap::new(),
            positions: Positions::default(),
            synced: None,
            width: 0.0,
            metrics: TextMetrics::default(),
            top: 0.0,
            bottom: 0.0,
            learned: HashMap::new(),
            around: HashMap::new(),
            last: None,
            pending: None,
            moved: 0.0,
            counts: Counts::default(),
        }
    }
}

/// A fast mixing of what a revision is made of
pub fn mix(hash: u64, value: u64) -> u64 {
    (hash ^ value)
        .wrapping_mul(0x0100_0000_01b3)
        .rotate_left(23)
        ^ (value >> 7)
}

/// The width estimates are made for before any layout
const LIKELY_WIDTH: f32 = 700.0;

impl<K: Copy + Eq + Hash> Geometry<K> {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn item(&self, index: usize) -> &Item<K> {
        &self.items[index]
    }

    pub fn index_of(&self, key: K) -> Option<usize> {
        self.by_key.get(&key).copied()
    }

    /// The items' total height
    pub fn total(&self) -> f32 {
        self.positions.total()
    }

    /// Where item `index`'s top (its gap's top) is, below the first's
    pub fn offset_of(&self, index: usize) -> f32 {
        self.positions.prefix(index)
    }

    /// The content's height, as last laid out, with the items' heights now
    pub fn content_height(&self) -> f32 {
        self.top + self.total() + self.bottom
    }

    /// Forget every item, keeping the counts
    pub fn clear(&mut self) {
        let counts = self.counts;
        *self = Self {
            counts,
            ..Self::default()
        };
    }

    /// Bring the items up to date, unless `revision` says nothing changed
    /// since last time. `spec` describes item `i`; `estimate` is asked only
    /// for items whose revision changed. Without `leading` children above
    /// them, the first shown item has no gap. A pass over the specs,
    /// without allocating, when only content changed; rebuilt when items
    /// came, went or moved
    pub fn sync(
        &mut self,
        revision: u64,
        len: usize,
        leading: bool,
        told: impl Fn(usize) -> ItemSpec<K>,
        estimate: impl Fn(usize) -> Estimate,
    ) {
        if self.synced == Some(revision) {
            return;
        }
        self.synced = Some(revision);
        // In order, so that the first shown is known as it is reached
        let first = std::cell::Cell::new(!leading);
        let spec = |i: usize| {
            let spec = told(i);
            if spec.shown && first.replace(false) {
                ItemSpec { gap: 0.0, ..spec }
            } else {
                spec
            }
        };
        let same_order =
            len == self.items.len() && (0..len).all(|i| self.items[i].key == told(i).key);
        if same_order {
            let mut changed = false;
            for i in 0..len {
                let s = spec(i);
                let item = &self.items[i];
                if item.revision == s.revision
                    && item.class == s.class
                    && item.shown == s.shown
                    && item.gap == s.gap
                {
                    continue;
                }
                changed = true;
                let content = item.revision != s.revision || item.class != s.class;
                let estimate = content.then(|| estimate(i));
                self.update_item(i, s, estimate);
            }
            if changed {
                self.counts.passes += 1;
            }
            return;
        }
        self.counts.passes += 1;
        let mut old: HashMap<K, Item<K>> = std::mem::take(&mut self.items)
            .into_iter()
            .map(|item| (item.key, item))
            .collect();
        let mut items = Vec::with_capacity(len);
        for i in 0..len {
            let s = spec(i);
            let item = match old.remove(&s.key) {
                Some(mut item) if item.revision == s.revision && item.class == s.class => {
                    if item.gap != s.gap {
                        if item.shown {
                            item.height += s.gap - item.gap;
                        }
                        item.gap = s.gap;
                    }
                    if item.shown != s.shown {
                        item.shown = s.shown;
                        item.exact = false;
                        item.previous = None;
                        item.height = if s.shown {
                            self.estimate_of(&item)
                        } else {
                            0.0
                        };
                    }
                    item
                }
                _ => {
                    let mut item = Item {
                        key: s.key,
                        height: 0.0,
                        exact: false,
                        width: self.width,
                        gap: s.gap,
                        shown: s.shown,
                        class: s.class,
                        previous: None,
                        revision: s.revision,
                        estimate: estimate(i),
                        content: None,
                    };
                    if item.shown {
                        item.height = self.estimate_of(&item);
                    }
                    item
                }
            };
            items.push(item);
        }
        self.items = items;
        self.by_key = self
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| (item.key, i))
            .collect();
        debug_assert_eq!(
            self.by_key.len(),
            self.items.len(),
            "every key in a list is unique"
        );
        self.positions = Positions::new(self.items.iter().map(|item| item.height).collect());
    }

    fn update_item(&mut self, i: usize, s: ItemSpec<K>, estimate: Option<Estimate>) {
        let item = &mut self.items[i];
        let old_gap = item.gap;
        if let Some(estimate) = estimate {
            item.estimate = estimate;
            item.content = None;
            item.revision = s.revision;
            item.class = s.class;
            item.exact = false;
            item.previous = None;
        }
        if item.shown != s.shown {
            item.shown = s.shown;
            item.exact = false;
            item.previous = None;
        }
        item.gap = s.gap;
        if item.exact {
            // Only its gap changed
            item.height += s.gap - old_gap;
            if let Some((_, height)) = &mut item.previous {
                *height += s.gap - old_gap;
            }
        }
        let item = &self.items[i];
        let height = if !item.shown {
            0.0
        } else if item.exact {
            item.height
        } else {
            self.counts.estimated += 1;
            self.estimate_of(item)
        };
        self.items[i].height = height;
        self.positions.set(i, height);
    }

    /// The width estimates are made for: the last laid out, or a likely one
    fn estimate_width(&self) -> f32 {
        if self.width > 0.0 {
            self.width
        } else {
            LIKELY_WIDTH
        }
    }

    /// The height `item` is estimated to take at the current width
    fn estimate_of(&self, item: &Item<K>) -> f32 {
        let size = self.metrics.size;
        let height = match &item.estimate {
            Estimate::Fixed { ems } => self.learned.get(&item.class).copied().unwrap_or(ems * size),
            Estimate::Content { around, .. } if item.content.is_some() => {
                let around = self
                    .around
                    .get(&item.class)
                    .copied()
                    .unwrap_or(around * size);
                item.content.unwrap_or_default() + around
            }
            Estimate::Content { summary, around } => {
                let content = self.metrics.estimate(summary, self.estimate_width());
                content * self.learned.get(&item.class).copied().unwrap_or(1.0) + around * size
            }
        };
        height.max(1.0) + item.gap
    }

    /// Lay the items out at `width` with `metrics`: when either changed,
    /// each item takes the height it had at that width if it was the one
    /// before, else an estimate, until it is laid out again
    pub fn set_width(&mut self, width: f32, metrics: TextMetrics) {
        if width == self.width && metrics == self.metrics {
            return;
        }
        let refont = metrics != self.metrics;
        self.width = width;
        self.metrics = metrics;
        self.counts.passes += 1;
        for i in 0..self.items.len() {
            // Its content's height was for the width before
            self.items[i].content = None;
            let item = &self.items[i];
            if !item.shown {
                continue;
            }
            let was = item.exact.then_some((item.width, item.height));
            let back = item.previous.filter(|(at, _)| *at == width && !refont);
            let (height, exact) = match back {
                Some((_, height)) => (height, true),
                None => {
                    self.counts.estimated += 1;
                    (self.estimate_of(item), false)
                }
            };
            let item = &mut self.items[i];
            item.previous = if refont { None } else { was };
            item.height = height;
            item.exact = exact;
            item.width = width;
        }
        self.positions = Positions::new(self.items.iter().map(|item| item.height).collect());
    }

    /// Item `index` was laid out `height` tall, its gap included, at the
    /// current width
    pub fn measured(&mut self, index: usize, height: f32) {
        self.counts.measured += 1;
        let width = self.width;
        let size = self.metrics.size;
        let item = &self.items[index];
        if item.exact && item.width == width && (item.height - height).abs() < 0.01 {
            return;
        }
        // What estimates of its class are off by
        let class = item.class;
        let learned = match &item.estimate {
            Estimate::Fixed { .. } => Some(height - item.gap),
            Estimate::Content { summary, around } => {
                let content = self.metrics.estimate(summary, self.estimate_width());
                (content > 1.0).then(|| {
                    let ratio = ((height - item.gap - around * size) / content).clamp(0.4, 2.5);
                    let old = self.learned.get(&class).copied().unwrap_or(1.0);
                    old + (ratio - old) * 0.2
                })
            }
        };
        if let Some(learned) = learned {
            self.learned.insert(class, learned);
        }
        if let (Some(content), Estimate::Content { .. }) = (item.content, &item.estimate) {
            let around = (height - item.gap - content).max(0.0);
            let old = self.around.get(&class).copied().unwrap_or(around);
            self.around.insert(class, old + (around - old) * 0.3);
        }
        let item = &mut self.items[index];
        item.height = height;
        item.exact = true;
        item.width = width;
        self.positions.set(index, height);
    }

    /// Item `key`'s content is `height` tall, as laid out elsewhere at the
    /// current width: its estimate, while the list has not laid it out, is
    /// that and the room its class takes around it. O(log n)
    pub fn content_height_known(&mut self, key: K, height: f32) {
        let Some(index) = self.index_of(key) else {
            return;
        };
        let item = &mut self.items[index];
        if item.content == Some(height) {
            return;
        }
        item.content = Some(height);
        if !item.shown || item.exact {
            return;
        }
        let item = &self.items[index];
        let estimate = self.estimate_of(item);
        let delta = estimate - item.height;
        self.items[index].height = estimate;
        self.positions.set(index, estimate);
        if let Some((key, ..)) = self.last.and_then(|last| last.place)
            && self.index_of(key).is_some_and(|at| index < at)
        {
            self.moved += delta;
        }
    }

    /// The items to build for a view whose top is at `top` in the content,
    /// `page` tall: those meeting it and a screen either side
    pub fn window(&self, top: f32, page: f32) -> Range<usize> {
        if self.items.is_empty() {
            return 0..0;
        }
        let from = (top - page - self.top).max(0.0);
        let to = top + 2.0 * page - self.top;
        if to < 0.0 {
            return 0..self.positions.find(0.0) + 1;
        }
        let start = self.positions.find(from);
        let end = (self.positions.find(to) + 1).min(self.items.len());
        start..end.max(start)
    }

    /// The place at `y`, in the content: the first item whose content
    /// (past its gap) ends below it
    pub fn place_at(&self, y: f32) -> Option<Place<K>> {
        if self.items.is_empty() || self.total() <= 0.0 {
            return None;
        }
        let relative = y - self.top;
        let mut index = if relative <= 0.0 {
            0
        } else {
            self.positions.find(relative)
        };
        // Past the end of the one it falls in (in its gap's place below)
        while index < self.items.len() {
            let item = &self.items[index];
            let start = self.top + self.positions.prefix(index) + item.gap;
            let height = (item.height - item.gap).max(0.0);
            if item.shown && item.height > 0.0 && start + height > y {
                let fraction = ((y - start) / height.max(1.0)).clamp(0.0, 1.0);
                return Some((item.key, fraction, (start - y).max(0.0)));
            }
            index += 1;
        }
        None
    }

    /// Where a place is in the content now: where the view's top must be to
    /// show it as it was
    pub fn y_of(&self, (key, fraction, above): Place<K>) -> Option<f32> {
        let index = self.index_of(key)?;
        let item = &self.items[index];
        if !item.shown {
            return None;
        }
        let start = self.top + self.positions.prefix(index) + item.gap;
        Some(start + fraction * (item.height - item.gap).max(0.0) - above)
    }

    /// Where item `key`'s top (past its gap) is in the content
    pub fn top_of(&self, key: K) -> Option<f32> {
        let index = self.index_of(key)?;
        Some(self.top + self.positions.prefix(index) + self.items[index].gap)
    }

    /// Where item `key`'s content ends in the content
    pub fn bottom_of(&self, key: K) -> Option<f32> {
        let index = self.index_of(key)?;
        Some(self.top + self.positions.prefix(index + 1))
    }

    /// A layout begins, with the view's top at `offset` as the scrollable
    /// stands, if known: the place to keep through it. Unscrolled since the
    /// last layout, it is the place the view showed then, wherever that
    /// item has gone since; scrolled, the place at the new offset
    pub fn begin_layout(&mut self, offset: Option<f32>) {
        self.pending = match (offset, self.last) {
            (Some(offset), Some(last)) if (offset - last.offset).abs() < 0.5 => last.place,
            // Scrolled: by as much from where the place shown last is now,
            // as heights above it may have changed since
            (Some(offset), Some(_)) => self.place_at(offset + self.moved),
            (Some(offset), None) => self.place_at(offset),
            (None, _) => None,
        };
    }

    /// Where the view's top must be to keep the place, after the layout
    pub fn target(&self) -> Option<f32> {
        self.pending.and_then(|place| self.y_of(place))
    }

    /// The layout is done, with the view's top at `offset`
    pub fn end_layout(&mut self, offset: f32, bounds: (f32, f32), content: (f32, f32)) {
        self.pending = None;
        self.moved = 0.0;
        self.last = Some(Last {
            offset,
            place: self.place_at(offset),
            bounds,
            content,
        });
    }

    /// Keep `place` from the next layout on, rather than what is at the
    /// view's top now. False if nothing was laid out yet to keep it in
    pub fn keep(&mut self, place: Place<K>) -> bool {
        match &mut self.last {
            Some(last) => {
                last.place = Some(place);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::estimate::{ContentSummary, Segment};
    use super::*;

    fn spec(i: usize) -> ItemSpec<u64> {
        ItemSpec {
            key: i as u64,
            revision: i as u64,
            class: EstimateClass(1),
            gap: 10.0,
            shown: true,
        }
    }

    fn prose(chars: u32) -> Estimate {
        Estimate::Content {
            summary: ContentSummary {
                segments: vec![Segment::Prose { chars, scale: 1.0 }],
                indent: 0.0,
            },
            around: 0.0,
        }
    }

    #[test]
    fn running_sums_find_items_and_offsets() {
        let values: Vec<f32> = (0..1000).map(|i| (i % 7) as f32 * 3.0).collect();
        let mut positions = Positions::new(values.clone());
        let mut sum = 0.0f32;
        for (i, value) in values.iter().enumerate() {
            assert!((positions.prefix(i) - sum).abs() < 0.01, "prefix {i}");
            if *value > 0.0 {
                assert_eq!(positions.find(sum), i, "the item at its own top");
                assert_eq!(positions.find(sum + value - 0.5), i, "and inside it");
            }
            sum += value;
        }
        assert_eq!(positions.find(sum + 100.0), 999);
        positions.set(10, 100.0);
        assert!((positions.total() - (sum - values[10] + 100.0)).abs() < 0.01);
        assert_eq!(positions.find(positions.prefix(10) + 50.0), 10);
        assert_eq!(
            positions.find(positions.prefix(11) + 0.1),
            if values[11] > 0.0 { 11 } else { 12 }
        );
    }

    #[test]
    fn heights_change_one_item_at_a_time() {
        let mut geometry = Geometry::default();
        geometry.set_width(600.0, TextMetrics::default());
        geometry.sync(1, 5000, true, spec, |_| prose(400));
        let passes = geometry.counts.passes;
        let before = geometry.offset_of(4000);
        let old = geometry.item(10).height;
        geometry.measured(10, 123.0);
        assert!(geometry.item(10).exact);
        assert!(
            (geometry.offset_of(4000) - before - (123.0 - old)).abs() < 0.05,
            "later items shift by the delta"
        );
        assert_eq!(
            geometry.counts.passes, passes,
            "a measurement is not a pass over the items"
        );
        geometry.sync(1, 5000, true, spec, |_| prose(400));
        assert_eq!(
            geometry.counts.passes, passes,
            "nothing changed: no pass at all"
        );
    }

    #[test]
    fn the_window_covers_a_screen_either_side() {
        let mut geometry = Geometry::default();
        geometry.set_width(600.0, TextMetrics::default());
        geometry.sync(1, 1000, true, spec, |_| prose(200));
        for i in 0..1000 {
            geometry.measured(i, 50.0);
        }
        assert_eq!(
            geometry.window(10_000.0, 500.0),
            190..221,
            "items from 9 500 to 11 000 px"
        );
        assert_eq!(geometry.window(0.0, 500.0), 0..21);
        assert_eq!(geometry.window(1e9, 500.0).end, 1000);
    }

    #[test]
    fn a_place_survives_items_above_changing() {
        let mut geometry = Geometry::default();
        geometry.set_width(600.0, TextMetrics::default());
        geometry.top = 24.0;
        geometry.sync(1, 100, true, spec, |_| prose(200));
        for i in 0..100 {
            geometry.measured(i, 60.0);
        }
        let place = geometry.place_at(24.0 + 60.0 * 40.0 + 10.0 + 25.0).unwrap();
        assert_eq!(place.0, 40);
        assert!((place.1 - 0.5).abs() < 0.01);
        let y = geometry.y_of(place).unwrap();
        geometry.measured(3, 160.0);
        assert!(
            (geometry.y_of(place).unwrap() - y - 100.0).abs() < 0.01,
            "an item above grows: the place moves down by as much"
        );
        let (key, fraction, above) = geometry.place_at(24.0 + 60.0 * 41.0 + 100.0 + 4.0).unwrap();
        assert_eq!(
            (key, fraction),
            (41, 0.0),
            "in the gap above an item: the item"
        );
        assert!((above - 6.0).abs() < 0.01, "and the room above it");
    }

    #[test]
    fn layouts_keep_the_place_unless_scrolled() {
        let mut geometry = Geometry::default();
        geometry.set_width(600.0, TextMetrics::default());
        geometry.sync(1, 100, true, spec, |_| prose(200));
        for i in 0..100 {
            geometry.measured(i, 60.0);
        }
        geometry.end_layout(1000.0, (600.0, 500.0), (600.0, 6000.0));
        geometry.begin_layout(Some(1000.0));
        geometry.measured(2, 100.0);
        assert!(
            (geometry.target().unwrap() - 1040.0).abs() < 0.01,
            "not scrolled: items above grow, the place is kept"
        );
        geometry.end_layout(1040.0, (600.0, 500.0), (600.0, 6040.0));
        geometry.begin_layout(Some(2000.0));
        geometry.measured(50, 70.0);
        assert!(
            (geometry.target().unwrap() - 2000.0).abs() < 0.01,
            "scrolled: an item below the place moves nothing"
        );
    }

    #[test]
    fn going_back_to_a_width_takes_its_heights_back() {
        let mut geometry = Geometry::default();
        geometry.set_width(600.0, TextMetrics::default());
        geometry.sync(1, 3, true, spec, |_| prose(2000));
        geometry.measured(0, 200.0);
        geometry.set_width(300.0, TextMetrics::default());
        assert!(!geometry.item(0).exact);
        geometry.measured(0, 390.0);
        geometry.set_width(600.0, TextMetrics::default());
        assert!(geometry.item(0).exact);
        assert_eq!(geometry.item(0).height, 200.0);
        geometry.set_width(
            600.0,
            TextMetrics {
                size: 18.0,
                ..TextMetrics::default()
            },
        );
        assert!(!geometry.item(0).exact, "a new font forgets them");
    }

    #[test]
    fn hidden_items_take_no_room_and_are_never_the_place() {
        let mut geometry = Geometry::default();
        geometry.set_width(600.0, TextMetrics::default());
        let hidden = |i: usize| ItemSpec {
            shown: i.is_multiple_of(2),
            ..spec(i)
        };
        geometry.sync(1, 10, true, hidden, |_| prose(100));
        for i in (0..10).step_by(2) {
            geometry.measured(i, 40.0);
        }
        assert_eq!(geometry.total(), 200.0);
        assert_eq!(geometry.place_at(45.0).unwrap().0, 2);
        assert_eq!(geometry.window(0.0, 100.0), 0..10);
    }

    #[test]
    fn fixed_estimates_learn_their_class_height() {
        let mut geometry = Geometry::default();
        geometry.set_width(600.0, TextMetrics::default());
        let closed = |i: usize| ItemSpec {
            class: EstimateClass(2),
            ..spec(i)
        };
        geometry.sync(1, 3, true, closed, |_| Estimate::Fixed { ems: 2.0 });
        assert_eq!(
            geometry.item(1).height,
            30.0 + 10.0,
            "two ems of 15 px and the gap"
        );
        geometry.measured(0, 10.0 + 44.0);
        geometry.sync(
            2,
            3,
            true,
            |i| ItemSpec {
                revision: 7,
                ..closed(i)
            },
            |_| Estimate::Fixed { ems: 2.0 },
        );
        assert_eq!(
            geometry.item(1).height,
            44.0 + 10.0,
            "the class's learned height"
        );
    }

    #[test]
    fn a_known_content_height_estimates_with_the_room_around_it() {
        let mut geometry = Geometry::default();
        geometry.set_width(600.0, TextMetrics::default());
        geometry.sync(1, 3, true, spec, |_| Estimate::Content {
            summary: ContentSummary::default(),
            around: 2.0,
        });
        geometry.content_height_known(1, 100.0);
        assert_eq!(
            geometry.item(1).height,
            100.0 + 30.0 + 10.0,
            "content, two ems around it, the gap"
        );
    }
}
