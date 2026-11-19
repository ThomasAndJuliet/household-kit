// SPDX-License-Identifier: GPL-3.0-only

//! Rows, columns and lists of immediate children, and a parent's
//! arrangements in its order of preference.
//!
//! A row or column shares a shortage along its direction with
//! [`allocate`], then asks each child again at the room it got. Gaps are
//! children with a range from a least to an ideal, named by what they
//! separate.
//!
//! A level in an order is a ceiling, everywhere: "you may compromise up
//! to here" (semantics A1).
//!
//! - A list orders every member its own level, and passes each member
//!   exactly what it was ordered: "the point is that children are
//!   organised so in the toolbar example the child is the list of quick
//!   action buttons this means they compact together". That is the only
//!   "together". A list takes its level exactly, so whoever holds a
//!   ceiling over it works up from Preferred.
//! - A row or column (A3, second round) first shares the shortage: it
//!   fits if its children's least lengths along fit together and each
//!   child fits the room it gets across, the room short of the preferred
//!   lengths being shared by weight over the children's ranges. A weight
//!   of zero protects a child's preferred length. If it does not fit, then
//!   for each target from Mild up to its ceiling it orders up, one level
//!   at a time, the first child in its asking order that is below the
//!   target and whose next level helps: its least length along shrinks
//!   while the direction is short, or it comes nearer to fitting its own
//!   room across. When it fits, it relaxes: in the reverse of the order in
//!   which children were ordered up, each is tried one level lower and
//!   kept there if the whole still fits. A row or column passes its
//!   children only the level.
//! - [`Arrangements`] tries its arrangements in passes (A4): by default up
//!   to Mild, then up to Strong, then up to AllPermitted, each capped at
//!   its own ceiling; within a pass each arrangement starts again from
//!   Preferred, and the first that fits is taken: "compact mildly, change
//!   arrangement, then compact strongly".
//!
//! A composition's answer is about the space offered (B1, second round):
//! each of its four lengths is for one direction with the other as
//! offered, and its taken size is what it decides there. Ordering it in
//! the same space makes the same decision. A composition remembers its
//! answers while it is unchanged: adding to it, or telling it to
//! [forget](LayoutBox::forget), clears what it remembered (B5).

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use crate::allocate::{AllocationError, Capacity, allocate};
use crate::boxes::{Answer, Kept, LayoutBox, Node, Reason, Shortfall, Showed};
use crate::geometry::{Direction, FIT_EPSILON, Point, Rect, Size};
use crate::identity::{Id, Source};
use crate::level::Level;
use crate::order::{Accepts, Order, Resolved, Unsupported};

/// A child, shared so that several arrangements can hold the same box.
pub type Child<'a, C> = Rc<dyn LayoutBox<C> + 'a>;

/// A size with `along` in `direction` and `across` in the other.
fn size_in(direction: Direction, along: f64, across: f64) -> Size {
    match direction {
        Direction::Across => Size::new(along, across),
        Direction::Down => Size::new(across, along),
    }
}

/// A space as a key: the bits of its two lengths.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct SpaceKey(u64, u64);

impl From<Size> for SpaceKey {
    fn from(size: Size) -> Self {
        Self(size.width.to_bits(), size.height.to_bits())
    }
}

type AskedKey = (usize, Order, SpaceKey);

/// The answers a parent has had from its children. A query is pure, so
/// each child is asked once for each order and space while the parent is
/// unchanged.
#[derive(Default)]
struct Asked(RefCell<HashMap<AskedKey, Result<Answer, Unsupported>>>);

impl Asked {
    fn ask<C>(
        &self,
        i: usize,
        child: &dyn LayoutBox<C>,
        order: &Order,
        space: Size,
    ) -> Result<Answer, Unsupported> {
        let key = (i, order.clone(), SpaceKey::from(space));
        if let Some(answer) = self.0.borrow().get(&key) {
            return answer.clone();
        }
        let answer = child.query(order, space);
        self.0.borrow_mut().insert(key, answer.clone());
        answer
    }

    fn clear(&self) {
        self.0.borrow_mut().clear();
    }
}

/// A box's own answers, by its resolved orders and the space offered.
struct Remembered<T>(RefCell<HashMap<(Resolved, SpaceKey), Rc<T>>>);

impl<T> Default for Remembered<T> {
    fn default() -> Self {
        Self(RefCell::default())
    }
}

impl<T> Remembered<T> {
    fn get_or(
        &self,
        own: &Resolved,
        space: Size,
        make: impl FnOnce() -> Result<T, Unsupported>,
    ) -> Result<Rc<T>, Unsupported> {
        let key = (own.clone(), SpaceKey::from(space));
        if let Some(remembered) = self.0.borrow().get(&key) {
            return Ok(Rc::clone(remembered));
        }
        let made = Rc::new(make()?);
        self.0.borrow_mut().insert(key, Rc::clone(&made));
        Ok(made)
    }

    fn clear(&self) {
        self.0.borrow_mut().clear();
    }
}

/// The length a box would like in `direction`, never below its least.
fn preferred_in(direction: Direction, answer: &Answer) -> f64 {
    direction
        .length(answer.preferred)
        .max(direction.length(answer.least))
}

fn lacks(answer: &Answer) -> f64 {
    answer.short.iter().map(Shortfall::lacks).sum()
}

fn short_in(direction: Direction, answer: &Answer) -> bool {
    answer
        .short
        .iter()
        .any(|short| short.direction == direction)
}

/// Space between children along its composition, and nothing across, with
/// three lengths: a relaxed length, which it prefers (the stylesheet's
/// recommendation); an ideal, the compact distance; and a least. It gives
/// room from relaxed down to its ideal at any level, sharing the shortage
/// by weight like any other child; that is not a change of form and is
/// never recorded as one. Below the level it is squeezed from it keeps
/// its ideal; from that level it may be squeezed to its least (design §5:
/// Strong is "flagged squeezing below an ideal"). The level is the rules'
/// value, Strong unless given.
#[derive(Clone, Debug)]
pub struct Gap {
    id: Id,
    direction: Direction,
    least: f64,
    ideal: f64,
    relaxed: f64,
    squeezed_from: Level,
    accepts: Accepts,
}

impl Gap {
    /// A gap whose relaxed length is its ideal.
    pub fn new(id: Id, direction: Direction, least: f64, ideal: f64) -> Self {
        Self {
            id,
            direction,
            least,
            ideal,
            relaxed: ideal,
            squeezed_from: Level::Strong,
            accepts: Accepts::ordinary(),
        }
    }

    /// The length this gap prefers, from which it gives room down to its
    /// ideal at any level; never below the ideal.
    pub fn relaxed(mut self, relaxed: f64) -> Self {
        self.relaxed = relaxed.max(self.ideal);
        self
    }

    /// The level from which this gap may go below its ideal.
    pub fn squeezed_from(mut self, level: Level) -> Self {
        self.squeezed_from = level;
        self
    }
}

impl<C> LayoutBox<C> for Gap {
    fn id(&self) -> &Id {
        &self.id
    }

    fn accepts(&self) -> &Accepts {
        &self.accepts
    }

    fn measure(&self, orders: &Resolved, space: Size) -> Result<Answer, Unsupported> {
        let least = if orders.level >= self.squeezed_from {
            self.least
        } else {
            self.ideal
        };
        Ok(Answer::of(
            &self.id,
            size_in(self.direction, self.relaxed, 0.0),
            size_in(self.direction, least, 0.0),
            space,
        ))
    }

    fn arrange(&self, orders: &Resolved, at: Point, space: Size) -> Result<Node<C>, Unsupported> {
        let taken = LayoutBox::<C>::measure(self, orders, space)?.taken;
        Ok(Node::new(
            self.id.clone(),
            orders.clone(),
            Rect::from_origin(at, taken),
        ))
    }
}

/// Where a member sits across its composition's direction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Align {
    Start,
    Centre,
    End,
    /// It takes the whole length across: its node is stretched to the
    /// composition's length across.
    Fill,
}

/// A child of a row, column or list, with what its parent orders of it.
pub struct Member<'a, C> {
    pub child: Child<'a, C>,
    /// The parent's order for this child, in every kind but the level; a
    /// row or column sets the level itself. A list passes every member
    /// what the list was ordered and ignores this.
    pub order: Order,
    /// How readily it gives room within its range, from its preferred
    /// length down to its least, as in [`Capacity::weight`]. Zero protects
    /// its preferred length.
    pub weight: f64,
    /// Where it sits across; `None` is the composition's default: a row
    /// centres its members ("boxes in a row are centred on one line"), a
    /// column starts them.
    pub align: Option<Align>,
    /// Its share, by weight, of room beyond the preferred lengths. Zero
    /// does not grow.
    pub grow: f64,
}

impl<'a, C> Member<'a, C> {
    pub fn new(child: Child<'a, C>) -> Self {
        Self {
            child,
            order: Order::NONE,
            weight: 1.0,
            align: None,
            grow: 0.0,
        }
    }

    pub fn with_order(mut self, order: Order) -> Self {
        self.order = order;
        self
    }

    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }

    /// The child that gives and grows, such as a plot: it gives room by
    /// its range at its weight like every member, and takes the surplus
    /// with a growth of one. Nothing else sets it apart; in particular its
    /// siblings are not compacted to spare it.
    pub fn flexible(self) -> Self {
        self.growing(1.0)
    }

    pub fn aligned(mut self, align: Align) -> Self {
        self.align = Some(align);
        self
    }

    /// Marked to grow, with `weight` its share of the surplus.
    pub fn growing(mut self, weight: f64) -> Self {
        self.grow = weight;
        self
    }
}

impl<C> Clone for Member<'_, C> {
    fn clone(&self) -> Self {
        Self {
            child: Rc::clone(&self.child),
            order: self.order.clone(),
            weight: self.weight,
            align: self.align,
            grow: self.grow,
        }
    }
}

/// A gap the composition made, at `index` among its members.
struct GapAt {
    index: usize,
    least: f64,
    ideal: f64,
    relaxed: f64,
}

/// A row, a column or a list of immediate children.
pub struct Stack<'a, C> {
    id: Id,
    source: Option<Source>,
    maker: Order,
    accepts: Accepts,
    direction: Direction,
    /// A list: every member takes the list's own order.
    together: bool,
    members: Vec<Member<'a, C>>,
    asked_first: Vec<Id>,
    gaps: Vec<GapAt>,
    squeezed_from: Level,
    on_baselines: bool,
    asked: Asked,
    decided: Remembered<Outcome>,
    answered: Remembered<Answer>,
}

impl<'a, C> Stack<'a, C> {
    fn new(id: Id, direction: Direction, together: bool) -> Self {
        Self {
            id,
            source: None,
            maker: Order::NONE,
            accepts: Accepts::ordinary(),
            direction,
            together,
            members: Vec::new(),
            asked_first: Vec::new(),
            gaps: Vec::new(),
            squeezed_from: Level::Strong,
            on_baselines: false,
            asked: Asked::default(),
            decided: Remembered::default(),
            answered: Remembered::default(),
        }
    }

    /// Children side by side, left to right.
    pub fn row(id: Id) -> Self {
        Self::new(id, Direction::Across, false)
    }

    /// Children one above another.
    pub fn column(id: Id) -> Self {
        Self::new(id, Direction::Down, false)
    }

    /// Members in `direction` that take one order and change together.
    pub fn list(id: Id, direction: Direction) -> Self {
        Self::new(id, direction, true)
    }

    /// Forget this composition's own answers, not its children's.
    fn forget_own(&self) {
        self.asked.clear();
        self.decided.clear();
        self.answered.clear();
    }

    /// The same composition, changed: what it remembered no longer holds.
    fn changed(self) -> Self {
        self.forget_own();
        self
    }

    pub fn with_source(mut self, source: Source) -> Self {
        self.source = Some(source);
        self
    }

    /// What this composition's maker asks of it. Its level can only lower
    /// the ceiling ordered from above.
    pub fn with_maker(mut self, maker: Order) -> Self {
        self.maker = maker;
        self.changed()
    }

    /// The presentations this composition accepts. A list passes what it
    /// was ordered on to every member, so each member must accept it too.
    pub fn accepting(mut self, accepts: Accepts) -> Self {
        self.accepts = accepts;
        self.changed()
    }

    /// The parent's short list of who is asked first. Children not on it
    /// are asked after, in their order in the composition.
    pub fn asked_first(mut self, ids: impl IntoIterator<Item = Id>) -> Self {
        self.asked_first = ids.into_iter().collect();
        self.changed()
    }

    /// A row whose members that have a baseline sit on one shared
    /// baseline, which the row reports. Members without one keep their
    /// own alignment. A column ignores this.
    pub fn on_baselines(mut self) -> Self {
        self.on_baselines = true;
        self.changed()
    }

    pub fn direction(&self) -> Direction {
        self.direction
    }

    pub fn members(&self) -> &[Member<'a, C>] {
        &self.members
    }
}

impl<'a, C> Stack<'a, C> {
    pub fn child(self, child: Child<'a, C>) -> Self {
        self.member(Member::new(child))
    }

    /// Add a member. What the composition remembered is cleared.
    pub fn member(mut self, member: Member<'a, C>) -> Self {
        self.members.push(member);
        self.name_gaps();
        self.changed()
    }

    /// A gap from `least` to `ideal` along this composition, as the next
    /// child, whose relaxed length is its ideal. It is named by the
    /// children it separates.
    pub fn gap(self, least: f64, ideal: f64) -> Self {
        self.relaxed_gap(least, ideal, ideal)
    }

    /// A gap that prefers `relaxed` and gives room down to `ideal` at any
    /// level, and down to `least` from the squeeze level, as the next
    /// child. It is named by the children it separates.
    pub fn relaxed_gap(mut self, least: f64, ideal: f64, relaxed: f64) -> Self {
        let index = self.members.len();
        let placeholder: Child<'a, C> =
            Rc::new(Gap::new(self.id.child("gap"), self.direction, least, ideal));
        self.members.push(Member::new(placeholder));
        self.gaps.push(GapAt {
            index,
            least,
            ideal,
            relaxed: relaxed.max(ideal),
        });
        self.name_gaps();
        self.changed()
    }

    /// The level from which this composition's gaps may go below their
    /// ideals: the rules' value (Strong unless given).
    pub fn squeezing_from(mut self, level: Level) -> Self {
        self.squeezed_from = level;
        self.name_gaps();
        self.changed()
    }

    /// Make each gap again, named by the nearest children on either side
    /// that are not gaps (`start` and `end` at the ends), so that a gap
    /// keeps its identity when a child is added elsewhere.
    fn name_gaps(&mut self) {
        let names: Vec<Option<String>> = (0..self.members.len())
            .map(|i| {
                (!self.gaps.iter().any(|gap| gap.index == i))
                    .then(|| self.members[i].child.id().as_str().to_owned())
            })
            .collect();
        let mut seen: HashMap<(String, String), usize> = HashMap::new();
        for gap in &self.gaps {
            let before = names[..gap.index]
                .iter()
                .rev()
                .flatten()
                .next()
                .map_or("start", String::as_str);
            let after = names[gap.index + 1..]
                .iter()
                .flatten()
                .next()
                .map_or("end", String::as_str);
            let count = seen
                .entry((before.to_owned(), after.to_owned()))
                .or_insert(0);
            *count += 1;
            let name = if *count == 1 {
                format!("gap[{before}|{after}]")
            } else {
                format!("gap[{before}|{after}]#{count}")
            };
            let made = Gap::new(self.id.child(name), self.direction, gap.least, gap.ideal)
                .relaxed(gap.relaxed)
                .squeezed_from(self.squeezed_from);
            self.members[gap.index].child = Rc::new(made);
        }
    }
}

/// The children's answers under one assignment of levels in one space.
struct Trial {
    orders: Vec<Order>,
    /// Answers with unlimited room along and the room across as offered:
    /// each child's lengths along.
    full: Vec<Answer>,
    /// Each child's room along: what the allocation gave it, with its
    /// share of any surplus if it grows, or, when the floors do not fit,
    /// its floor.
    lengths: Vec<f64>,
    /// Each child's answer at its room. A child given its preferred
    /// length, or any length while the floors do not fit, answers as it
    /// does in `full`.
    rooms: Vec<Answer>,
    /// The space each child was asked in for `rooms`.
    asked_at: Vec<Size>,
    /// What the floors need and the room has, when they do not fit.
    along_short: Option<(f64, f64)>,
}

impl Trial {
    fn fits(&self) -> bool {
        self.along_short.is_none() && self.rooms.iter().all(Answer::fits)
    }
}

/// What a row, column or list decided under one resolved order in one
/// space.
struct Outcome {
    levels: Vec<Level>,
    trial: Trial,
    reasons: Vec<Reason>,
    short: Vec<Shortfall>,
}

fn note(reasons: &mut Vec<Reason>, reason: Reason) {
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

impl<'a, C> Stack<'a, C> {
    fn across(&self) -> Direction {
        self.direction.other()
    }

    fn room(&self, space: Size, along: f64) -> Size {
        size_in(self.direction, along, self.across().length(space))
    }

    /// Unlimited along and as offered across: where a child is asked for
    /// its lengths along, and where this composition's preferred length
    /// along is decided. Asking every child there, whatever the length
    /// along offered, keeps the spaces a tree is asked in few (B5).
    fn whole(&self, space: Size) -> Size {
        self.room(space, f64::INFINITY)
    }

    /// As offered along and unlimited across: where this composition's
    /// preferred length across is decided.
    fn open(&self, space: Size) -> Size {
        size_in(self.direction, self.direction.length(space), f64::INFINITY)
    }

    fn order_for(&self, i: usize, level: Level, own: &Resolved) -> Order {
        if self.together {
            own.ordered.clone().with_level(own.level)
        } else {
            self.members[i].order.clone().with_level(level)
        }
    }

    fn ask(
        &self,
        i: usize,
        level: Level,
        own: &Resolved,
        space: Size,
    ) -> Result<Answer, Unsupported> {
        let order = self.order_for(i, level, own);
        self.asked
            .ask(i, self.members[i].child.as_ref(), &order, space)
    }

    /// The least length along child `i` can be given: its least, or its
    /// preferred length while its weight protects it.
    fn floor(&self, i: usize, full: &Answer) -> f64 {
        if self.members[i].weight == 0.0 {
            preferred_in(self.direction, full)
        } else {
            self.direction.length(full.least)
        }
    }

    fn align_of(&self, i: usize) -> Align {
        self.members[i].align.unwrap_or(match self.direction {
            Direction::Across => Align::Centre,
            Direction::Down => Align::Start,
        })
    }

    fn baseline_row(&self) -> bool {
        self.on_baselines && self.direction == Direction::Across
    }

    /// The order in which children are asked: the short list, then the
    /// rest in their order.
    fn asking_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = self
            .asked_first
            .iter()
            .filter_map(|id| self.members.iter().position(|m| m.child.id() == id))
            .collect();
        for i in 0..self.members.len() {
            if !order.contains(&i) {
                order.push(i);
            }
        }
        order
    }

    /// Share the room along at `levels`, then ask each child at its room.
    fn trial(&self, levels: &[Level], own: &Resolved, space: Size) -> Result<Trial, Unsupported> {
        let d = self.direction;
        let n = self.members.len();
        let orders: Vec<Order> = (0..n).map(|i| self.order_for(i, levels[i], own)).collect();
        let whole = self.whole(space);
        let full = (0..n)
            .map(|i| self.ask(i, levels[i], own, whole))
            .collect::<Result<Vec<_>, _>>()?;
        let available = d.length(space);
        // A gap's relaxed length is for the content as given: once any child
        // has changed form, a gap takes no more than its ideal, so that a
        // gap never lengthens as the room shrinks.
        let compacted = levels.iter().any(|level| *level > Level::Preferred);
        let preferred: Vec<f64> = (0..n)
            .map(|i| {
                let preferred = preferred_in(d, &full[i]);
                match self.gaps.iter().find(|gap| gap.index == i) {
                    Some(gap) if compacted => preferred.min(gap.ideal),
                    _ => preferred,
                }
            })
            .collect();
        let floors: Vec<f64> = (0..n)
            .map(|i| self.floor(i, &full[i]).min(preferred[i]))
            .collect();
        let (mut lengths, along_short) = if available.is_finite() {
            let capacities: Vec<Capacity> = (0..n)
                .map(|i| {
                    Capacity::new(
                        preferred[i],
                        d.length(full[i].least),
                        self.members[i].weight,
                    )
                })
                .collect();
            match allocate(available, &capacities) {
                Ok(lengths) => (lengths, None),
                Err(AllocationError::NoFit {
                    required,
                    available,
                }) => (floors, Some((required, available))),
                // A child answered with lengths that cannot be shared: no
                // length is enough.
                Err(AllocationError::InvalidInput) => (floors, Some((f64::INFINITY, available))),
            }
        } else {
            (preferred.clone(), None)
        };
        let growing: f64 = self.members.iter().map(|member| member.grow).sum();
        if along_short.is_none() && available.is_finite() && growing > 0.0 {
            let surplus = available - lengths.iter().sum::<f64>();
            if surplus > FIT_EPSILON {
                for (length, member) in lengths.iter_mut().zip(&self.members) {
                    *length += surplus * member.grow / growing;
                }
            }
        }
        let asked_at: Vec<Size> = (0..n)
            .map(|i| {
                // Asked at its room whenever the room is not what it would
                // take of itself with unlimited room along.
                if along_short.is_none()
                    && (lengths[i] - preferred_in(d, &full[i])).abs() > FIT_EPSILON
                {
                    self.room(space, lengths[i])
                } else {
                    whole
                }
            })
            .collect();
        let rooms = (0..n)
            .map(|i| {
                if asked_at[i] == whole {
                    Ok(full[i].clone())
                } else {
                    self.ask(i, levels[i], own, asked_at[i])
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Trial {
            orders,
            full,
            lengths,
            rooms,
            asked_at,
            along_short,
        })
    }

    /// The next level above `from`, up to the ceiling, at which child
    /// `i`'s answer with unlimited room along or at `room` changes: a box
    /// need not have a form at every level.
    fn next_changing(
        &self,
        i: usize,
        from: Level,
        own: &Resolved,
        space: Size,
        room: Size,
    ) -> Result<Option<Level>, Unsupported> {
        let whole = self.whole(space);
        let full = self.ask(i, from, own, whole)?;
        let at_room = self.ask(i, from, own, room)?;
        let mut level = from;
        while let Some(next) = level.next().filter(|next| *next <= own.level) {
            if self.ask(i, next, own, whole)? != full || self.ask(i, next, own, room)? != at_room {
                return Ok(Some(next));
            }
            level = next;
        }
        Ok(None)
    }

    /// Whether child `i` at `next` lacks less at `room` than at `from`.
    fn fits_better(
        &self,
        i: usize,
        from: Level,
        next: Level,
        own: &Resolved,
        room: Size,
    ) -> Result<bool, Unsupported> {
        Ok(lacks(&self.ask(i, next, own, room)?)
            < lacks(&self.ask(i, from, own, room)?) - FIT_EPSILON)
    }

    /// Whether child `i` at `next` can be given less room along than at
    /// `from` without falling short across where it did not.
    fn gives_along(
        &self,
        i: usize,
        from: Level,
        next: Level,
        own: &Resolved,
        space: Size,
    ) -> Result<bool, Unsupported> {
        let whole = self.whole(space);
        let (now, then) = (
            self.ask(i, from, own, whole)?,
            self.ask(i, next, own, whole)?,
        );
        if self.floor(i, &then) >= self.floor(i, &now) - FIT_EPSILON {
            return Ok(false);
        }
        let across = self.across();
        Ok(short_in(across, &now) || !short_in(across, &then))
    }

    /// The child to order up towards `target`, with its next level and
    /// what showed it needs it; children passed over go into `reasons`.
    fn pick(
        &self,
        target: Level,
        levels: &[Level],
        own: &Resolved,
        space: Size,
        trial: &Trial,
        reasons: &mut Vec<Reason>,
    ) -> Result<Option<(usize, Level, Showed)>, Unsupported> {
        for i in self.asking_order() {
            if levels[i] >= target {
                continue;
            }
            let room_short = !trial.rooms[i].fits();
            let along_short = trial.along_short.is_some();
            if !room_short && !along_short {
                continue;
            }
            let showed = match trial.along_short {
                Some((needs, has)) if !room_short => Showed::NoRoomForTheLeast { needs, has },
                _ => Showed::DoesNotFitItsRoom {
                    short: trial.rooms[i].short.clone(),
                },
            };
            let child = self.members[i].child.id().clone();
            let room = trial.asked_at[i];
            let Some(next) = self.next_changing(i, levels[i], own, space, room)? else {
                note(
                    reasons,
                    Reason::Kept {
                        child,
                        level: levels[i],
                        showed,
                        why: Kept::AtCeiling,
                    },
                );
                continue;
            };
            if next > target {
                continue;
            }
            let helps = (room_short && self.fits_better(i, levels[i], next, own, room)?)
                || (along_short && self.gives_along(i, levels[i], next, own, space)?);
            if helps {
                return Ok(Some((i, next, showed)));
            }
            note(
                reasons,
                Reason::Kept {
                    child,
                    level: levels[i],
                    showed,
                    why: Kept::NextLevelDoesNotHelp { next },
                },
            );
        }
        Ok(None)
    }

    fn short_of(&self, trial: &Trial) -> Vec<Shortfall> {
        match trial.along_short {
            Some((needs, has)) => vec![Shortfall {
                part: self.id.clone(),
                direction: self.direction,
                needs,
                has,
            }],
            None => trial
                .rooms
                .iter()
                .flat_map(|answer| answer.short.iter().cloned())
                .collect(),
        }
    }

    /// Share, order up, relax (A3, second round).
    fn distribute(&self, own: &Resolved, space: Size) -> Result<Outcome, Unsupported> {
        let n = self.members.len();
        if self.together {
            let levels = vec![own.level; n];
            let trial = self.trial(&levels, own, space)?;
            let short = self.short_of(&trial);
            return Ok(Outcome {
                levels,
                trial,
                reasons: Vec::new(),
                short,
            });
        }
        let mut levels = vec![Level::Preferred; n];
        let mut reasons = Vec::new();
        let mut ups: Vec<(usize, Level, Level)> = Vec::new();
        let mut trial = self.trial(&levels, own, space)?;
        for target in Level::ALL.into_iter().filter(|level| *level <= own.level) {
            while !trial.fits() {
                let Some((i, next, showed)) =
                    self.pick(target, &levels, own, space, &trial, &mut reasons)?
                else {
                    break;
                };
                reasons.push(Reason::OrderedUp {
                    child: self.members[i].child.id().clone(),
                    from: levels[i],
                    to: next,
                    showed,
                });
                ups.push((i, levels[i], next));
                levels[i] = next;
                trial = self.trial(&levels, own, space)?;
            }
            if trial.fits() {
                break;
            }
        }
        if trial.fits() {
            for &(i, from, to) in ups.iter().rev() {
                if levels[i] != to {
                    continue;
                }
                let mut lower = levels.clone();
                lower[i] = from;
                let relaxed = self.trial(&lower, own, space)?;
                if relaxed.fits() {
                    reasons.push(Reason::OrderedDown {
                        child: self.members[i].child.id().clone(),
                        from: to,
                        to: from,
                    });
                    levels = lower;
                    trial = relaxed;
                }
            }
        }
        let short = self.short_of(&trial);
        if !short.is_empty() {
            reasons.push(Reason::NoFit {
                short: short.clone(),
            });
        }
        Ok(Outcome {
            levels,
            trial,
            reasons,
            short,
        })
    }

    fn decided(&self, own: &Resolved, space: Size) -> Result<Rc<Outcome>, Unsupported> {
        self.decided
            .get_or(own, space, || self.distribute(own, space))
    }

    /// The least length along at the length across offered: each child
    /// driven from Preferred as far as the ceiling allows, while its next
    /// level can be given less room along without falling short across, or
    /// falls less short across.
    fn least_along(&self, own: &Resolved, space: Size) -> Result<f64, Unsupported> {
        let across = self.across();
        let whole = self.whole(space);
        let mut total = 0.0;
        for i in 0..self.members.len() {
            let mut level = if self.together {
                own.level
            } else {
                Level::Preferred
            };
            while !self.together
                && let Some(next) = self.next_changing(i, level, own, space, whole)?
            {
                let short_across = short_in(across, &self.ask(i, level, own, whole)?);
                let helps = (short_across && self.fits_better(i, level, next, own, whole)?)
                    || self.gives_along(i, level, next, own, space)?;
                if !helps {
                    break;
                }
                level = next;
            }
            total += self.floor(i, &self.ask(i, level, own, whole)?);
        }
        Ok(total)
    }

    /// How far the members reach across, and the shared baseline of a
    /// row on baselines, with `length` each member's length across.
    fn extent(&self, answers: &[Answer], length: impl Fn(&Answer) -> f64) -> (f64, Option<f64>) {
        let shared = self
            .baseline_row()
            .then(|| {
                answers
                    .iter()
                    .filter_map(|answer| answer.baseline)
                    .reduce(f64::max)
            })
            .flatten();
        let extent = answers
            .iter()
            .enumerate()
            .map(|(i, answer)| match (shared, answer.baseline) {
                (Some(shared), Some(own)) if self.align_of(i) != Align::Fill => {
                    shared - own + length(answer)
                }
                _ => length(answer),
            })
            .fold(0.0, f64::max);
        (extent, shared)
    }

    /// The least length across at the length along offered: from what the
    /// composition decides there with unlimited room across, each child
    /// driven further while its next level is shorter across at its room
    /// and the floors along still fit.
    fn least_across(&self, own: &Resolved, space: Size) -> Result<f64, Unsupported> {
        let across = self.across();
        let open = self.open(space);
        let outcome = self.decided(own, open)?;
        let trial = &outcome.trial;
        let mut answers = trial.rooms.clone();
        if !self.together {
            let whole = self.whole(open);
            let mut levels = outcome.levels.clone();
            let mut floors: Vec<f64> = (0..self.members.len())
                .map(|i| self.floor(i, &trial.full[i]))
                .collect();
            for i in self.asking_order() {
                let room = trial.asked_at[i];
                while let Some(next) = self.next_changing(i, levels[i], own, open, room)? {
                    let at_next = self.ask(i, next, own, room)?;
                    let shorter = across.length(at_next.least)
                        < across.length(answers[i].least) - FIT_EPSILON;
                    let floor = self.floor(i, &self.ask(i, next, own, whole)?);
                    let total: f64 = floors.iter().sum::<f64>() - floors[i] + floor;
                    if !shorter || total > self.direction.length(space) + FIT_EPSILON {
                        break;
                    }
                    levels[i] = next;
                    floors[i] = floor;
                    answers[i] = at_next;
                }
            }
        }
        Ok(self
            .extent(&answers, |answer| across.length(answer.least))
            .0)
    }

    fn answer(&self, own: &Resolved, space: Size) -> Result<Answer, Unsupported> {
        let d = self.direction;
        let across = self.across();
        let outcome = self.decided(own, space)?;
        let trial = &outcome.trial;
        let preferred_along: f64 = self
            .decided(own, self.whole(space))?
            .trial
            .full
            .iter()
            .map(|answer| preferred_in(d, answer))
            .sum();
        let preferred_across = self
            .extent(
                &self.decided(own, self.open(space))?.trial.rooms,
                |answer| across.length(answer.taken),
            )
            .0;
        let least_along = self.least_along(own, space)?;
        let least_across = self.least_across(own, space)?;
        let (taken_across, shared) =
            self.extent(&trial.rooms, |answer| across.length(answer.taken));
        let baseline = match d {
            Direction::Across => shared,
            Direction::Down => trial.rooms.iter().enumerate().find_map(|(i, answer)| {
                answer
                    .baseline
                    .map(|baseline| trial.lengths[..i].iter().sum::<f64>() + baseline)
            }),
        };
        Ok(Answer {
            preferred: size_in(
                d,
                preferred_along.max(least_along),
                preferred_across.max(least_across),
            ),
            least: size_in(d, least_along, least_across),
            taken: size_in(d, trial.lengths.iter().sum(), taken_across),
            baseline,
            short: outcome.short.clone(),
            unshown: trial
                .rooms
                .iter()
                .flat_map(|answer| answer.unshown.iter().cloned())
                .collect(),
        })
    }
}

impl<C> LayoutBox<C> for Stack<'_, C> {
    fn id(&self) -> &Id {
        &self.id
    }

    fn source(&self) -> Option<&Source> {
        self.source.as_ref()
    }

    fn maker(&self) -> &Order {
        &self.maker
    }

    fn accepts(&self) -> &Accepts {
        &self.accepts
    }

    /// A list takes its level exactly; a row or column distributes it.
    fn takes_level_exactly(&self) -> bool {
        self.together
    }

    fn measure(&self, orders: &Resolved, space: Size) -> Result<Answer, Unsupported> {
        Ok((*self
            .answered
            .get_or(orders, space, || self.answer(orders, space))?)
        .clone())
    }

    /// Each child is ordered in the space it was last asked in when the
    /// composition decided, so it makes the choices it answered with.
    fn arrange(&self, orders: &Resolved, at: Point, space: Size) -> Result<Node<C>, Unsupported> {
        let taken = LayoutBox::<C>::measure(self, orders, space)?.taken;
        let outcome = self.decided(orders, space)?;
        let trial = &outcome.trial;
        let d = self.direction;
        let across = self.across();
        let cross = across.length(taken);
        let (_, shared) = self.extent(&trial.rooms, |answer| across.length(answer.taken));
        let mut node = Node::new(
            self.id.clone(),
            orders.clone(),
            Rect::from_origin(at, taken),
        )
        .with_source(self.source.clone());
        let mut offset = 0.0;
        let mut baseline = shared;
        for (i, member) in self.members.iter().enumerate() {
            let answer = &trial.rooms[i];
            let align = self.align_of(i);
            let length_across = across.length(answer.taken);
            let shift = match (shared, answer.baseline) {
                (Some(shared), Some(own)) if align != Align::Fill => shared - own,
                _ => match align {
                    Align::Start | Align::Fill => 0.0,
                    Align::Centre => ((cross - length_across) / 2.0).max(0.0),
                    Align::End => (cross - length_across).max(0.0),
                },
            };
            let place = match d {
                Direction::Across => Point::new(at.x + offset, at.y + shift),
                Direction::Down => Point::new(at.x + shift, at.y + offset),
            };
            let mut child = member
                .child
                .order(&trial.orders[i], place, trial.asked_at[i])?;
            // A member that fills is stretched across, and one that grows
            // along to its room, beyond what it would take of itself.
            let stretch = |bounds: &mut Rect, direction: Direction, length: f64| match direction {
                Direction::Across => bounds.width = length,
                Direction::Down => bounds.height = length,
            };
            if align == Align::Fill {
                stretch(&mut child.bounds, across, cross);
            }
            if member.grow > 0.0 {
                stretch(&mut child.bounds, d, trial.lengths[i]);
            }
            node.children.push(child);
            if d == Direction::Down && baseline.is_none() {
                baseline = answer.baseline.map(|own| offset + own);
            }
            offset += trial.lengths[i];
        }
        debug_assert_eq!(outcome.levels.len(), node.children.len());
        node.baseline = baseline;
        node.reasons = outcome.reasons.clone();
        Ok(node)
    }

    fn forget(&self) {
        self.forget_own();
        for member in &self.members {
            member.child.forget();
        }
    }
}

/// The passes an [`Arrangements`] makes unless given others: up to Mild,
/// then up to Strong, then up to AllPermitted (design §5).
pub const DEFAULT_PASSES: [Level; 3] = [Level::Mild, Level::Strong, Level::AllPermitted];

/// What an [`Arrangements`] decided in one space.
struct Decision {
    /// Each arrangement tried and not taken, with its last answer, in the
    /// order first tried.
    rejected: Vec<(usize, Answer)>,
    /// The first arrangement that fits, at the level it was given.
    chosen: Option<(usize, Level)>,
}

/// A parent's arrangements of its children, in its order of preference,
/// tried in passes (semantics A4). Within a pass each arrangement is
/// given, from Preferred, each level up to the pass's ceiling, and the
/// first that fits is taken: "compact mildly, change arrangement, then
/// compact strongly". An arrangement can be tagged with the value of a
/// family kind it stands for (`direction` = `turned`); an order naming
/// that kind restricts the box to the arrangements so tagged, and the
/// box's node records the tags of the one it chose. The chosen
/// arrangement is passed everything the box was ordered.
///
/// Its answer, for each direction, gives the least length over the
/// arrangements that fit the other direction as offered (B2, second
/// round), and its preferred and taken sizes are those of the
/// arrangement it would choose.
pub struct Arrangements<'a, C> {
    id: Id,
    source: Option<Source>,
    maker: Order,
    accepts: Accepts,
    options: Vec<Child<'a, C>>,
    tags: Vec<BTreeMap<String, String>>,
    passes: Vec<Level>,
    asked: Asked,
    answered: Remembered<Answer>,
}

impl<'a, C> Arrangements<'a, C> {
    pub fn new(id: Id, options: Vec<Child<'a, C>>) -> Self {
        let tags = vec![BTreeMap::new(); options.len()];
        Self {
            id,
            source: None,
            maker: Order::NONE,
            accepts: Accepts::ordinary(),
            options,
            tags,
            passes: DEFAULT_PASSES.to_vec(),
            asked: Asked::default(),
            answered: Remembered::default(),
        }
    }

    fn changed(self) -> Self {
        self.asked.clear();
        self.answered.clear();
        self
    }

    pub fn with_source(mut self, source: Source) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_maker(mut self, maker: Order) -> Self {
        self.maker = maker;
        self.changed()
    }

    pub fn accepting(mut self, accepts: Accepts) -> Self {
        self.accepts = accepts;
        self.changed()
    }

    /// The passes, as the ceiling of each in turn; each is capped at the
    /// box's own ceiling.
    pub fn with_passes(mut self, passes: impl IntoIterator<Item = Level>) -> Self {
        self.passes = passes.into_iter().collect();
        self.changed()
    }

    /// Tag the arrangement `arrangement` as standing for `value` of the
    /// family kind `kind`.
    ///
    /// # Panics
    ///
    /// If no arrangement has that identity.
    pub fn tagged(
        mut self,
        arrangement: &Id,
        kind: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        let k = self
            .options
            .iter()
            .position(|option| option.id() == arrangement)
            .expect("a tag names one of the arrangements");
        self.tags[k].insert(kind.into(), value.into());
        self.changed()
    }

    /// The arrangements the order permits, in order of preference.
    fn eligible(&self, own: &Resolved) -> Result<Vec<usize>, Unsupported> {
        let eligible: Vec<usize> = (0..self.options.len())
            .filter(|k| {
                own.ordered.added.iter().all(|(kind, value)| {
                    !self.tags.iter().any(|tags| tags.contains_key(kind))
                        || self.tags[*k].get(kind) == Some(value)
                })
            })
            .collect();
        if eligible.is_empty() {
            return Err(Unsupported {
                part: self.id.clone(),
                requested: own.presentation.clone(),
            });
        }
        Ok(eligible)
    }

    /// What the chosen arrangement is passed: everything the box was
    /// ordered, at `level`.
    fn order_at(own: &Resolved, level: Level) -> Order {
        own.ordered.clone().with_level(level)
    }

    fn ask(
        &self,
        k: usize,
        level: Level,
        own: &Resolved,
        space: Size,
    ) -> Result<Answer, Unsupported> {
        self.asked.ask(
            k,
            self.options[k].as_ref(),
            &Self::order_at(own, level),
            space,
        )
    }

    /// The levels an arrangement can be given under the passes: each from
    /// Preferred to the highest pass's ceiling for one that takes its
    /// level exactly, that ceiling alone for one that distributes it.
    fn candidates(&self, own: &Resolved) -> Result<Vec<(usize, Level)>, Unsupported> {
        let reach = self
            .passes
            .iter()
            .map(|pass| (*pass).min(own.level))
            .max()
            .unwrap_or(Level::Preferred);
        let mut candidates = Vec::new();
        for k in self.eligible(own)? {
            let lowest = if self.options[k].takes_level_exactly() {
                Level::Preferred
            } else {
                reach
            };
            for level in Level::ALL
                .into_iter()
                .filter(|level| (lowest..=reach).contains(level))
            {
                candidates.push((k, level));
            }
        }
        Ok(candidates)
    }

    fn decide(&self, own: &Resolved, space: Size) -> Result<Decision, Unsupported> {
        let eligible = self.eligible(own)?;
        let mut rejected: Vec<(usize, Answer)> = Vec::new();
        let mut chosen = None;
        let mut covered: Option<Level> = None;
        'passes: for pass in &self.passes {
            let cap = (*pass).min(own.level);
            let levels: Vec<Level> = Level::ALL
                .into_iter()
                .filter(|level| *level <= cap && covered.is_none_or(|covered| *level > covered))
                .collect();
            for &k in &eligible {
                // An arrangement that distributes its level works up by
                // itself under the pass's ceiling.
                let levels = if self.options[k].takes_level_exactly() {
                    &levels[..]
                } else {
                    &levels[levels.len().saturating_sub(1)..]
                };
                for &level in levels {
                    let answer = self.ask(k, level, own, space)?;
                    if answer.fits() {
                        chosen = Some((k, level));
                        break 'passes;
                    }
                    match rejected.iter_mut().find(|(seen, _)| *seen == k) {
                        Some(entry) => entry.1 = answer,
                        None => rejected.push((k, answer)),
                    }
                }
            }
            covered = covered.max(Some(cap));
        }
        if let Some((k, _)) = chosen {
            rejected.retain(|(seen, _)| *seen != k);
        }
        Ok(Decision { rejected, chosen })
    }

    /// The arrangement taken in `space`: the chosen one or, when none
    /// fits, the one that falls least short, as the parent's last resort.
    fn taken(&self, own: &Resolved, space: Size) -> Result<(Decision, usize, Level), Unsupported> {
        let decision = self.decide(own, space)?;
        if let Some((k, level)) = decision.chosen {
            return Ok((decision, k, level));
        }
        let mut best: Option<(usize, Level, f64)> = None;
        for (k, level) in self.candidates(own)? {
            let lacking = lacks(&self.ask(k, level, own, space)?);
            if best.is_none_or(|(_, _, least)| lacking < least - FIT_EPSILON) {
                best = Some((k, level, lacking));
            }
        }
        let (k, level, _) = best.expect("at least one arrangement is eligible");
        Ok((decision, k, level))
    }

    /// The least of `length` over the candidates asked in `space` that do
    /// not fall short in `other`; over every candidate if none fits.
    fn least_over(
        &self,
        own: &Resolved,
        space: Size,
        other: Direction,
        length: impl Fn(&Answer) -> f64,
    ) -> Result<f64, Unsupported> {
        let mut fitting = f64::INFINITY;
        let mut any = f64::INFINITY;
        for (k, level) in self.candidates(own)? {
            let answer = self.ask(k, level, own, space)?;
            any = any.min(length(&answer));
            if !short_in(other, &answer) {
                fitting = fitting.min(length(&answer));
            }
        }
        Ok(if fitting.is_finite() { fitting } else { any })
    }

    fn answer(&self, own: &Resolved, space: Size) -> Result<Answer, Unsupported> {
        let (_, k, level) = self.taken(own, space)?;
        let main = self.ask(k, level, own, space)?;
        let wide = Size::new(f64::INFINITY, space.height);
        let tall = Size::new(space.width, f64::INFINITY);
        let preferred_width = {
            let (_, k, level) = self.taken(own, wide)?;
            self.ask(k, level, own, wide)?.taken.width
        };
        let preferred_height = {
            let (_, k, level) = self.taken(own, tall)?;
            self.ask(k, level, own, tall)?.taken.height
        };
        let least = Size::new(
            self.least_over(own, wide, Direction::Down, |answer| answer.least.width)?,
            self.least_over(own, tall, Direction::Across, |answer| answer.least.height)?,
        );
        Ok(Answer {
            preferred: Size::new(
                preferred_width.max(least.width),
                preferred_height.max(least.height),
            ),
            least,
            ..main
        })
    }
}

impl<C> LayoutBox<C> for Arrangements<'_, C> {
    fn id(&self) -> &Id {
        &self.id
    }

    fn source(&self) -> Option<&Source> {
        self.source.as_ref()
    }

    fn maker(&self) -> &Order {
        &self.maker
    }

    fn accepts(&self) -> &Accepts {
        &self.accepts
    }

    fn takes_level_exactly(&self) -> bool {
        false
    }

    /// The answer of the arrangement it would take, with each least
    /// length the least over the arrangements that fit the other
    /// direction as offered.
    fn measure(&self, orders: &Resolved, space: Size) -> Result<Answer, Unsupported> {
        Ok((*self
            .answered
            .get_or(orders, space, || self.answer(orders, space))?)
        .clone())
    }

    fn arrange(&self, orders: &Resolved, at: Point, space: Size) -> Result<Node<C>, Unsupported> {
        let (decision, k, level) = self.taken(orders, space)?;
        let mut resolved = orders.clone();
        for (kind, value) in &self.tags[k] {
            resolved
                .presentation
                .added
                .insert(kind.clone(), value.clone());
        }
        let child = self.options[k].order(&Self::order_at(orders, level), at, space)?;
        let mut node = Node::new(
            self.id.clone(),
            resolved,
            Rect::from_origin(at, child.bounds.size()),
        )
        .with_source(self.source.clone());
        for (rejected, answer) in &decision.rejected {
            node.reasons.push(Reason::ArrangementRejected {
                arrangement: self.options[*rejected].id().clone(),
                short: answer.short.clone(),
            });
        }
        if decision.chosen.is_some() {
            node.reasons.push(Reason::ArrangementTaken {
                arrangement: self.options[k].id().clone(),
            });
        }
        node.children.push(child);
        Ok(node)
    }

    fn forget(&self) {
        self.asked.clear();
        self.answered.clear();
        for option in &self.options {
            option.forget();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boxes::{Refusal, plan as lay_out};
    use crate::order::{Presentation, Prominence};

    /// A leaf's node at `at`, of the size it takes in `space`.
    fn leaf_node(id: &Id, orders: &Resolved, at: Point, answer: Answer) -> Node<String> {
        Node::leaf(
            id.clone(),
            orders.clone(),
            Rect::from_origin(at, answer.taken),
            id.as_str().to_owned(),
        )
        .with_baseline(answer.baseline)
    }

    /// A leaf with a form at some levels: each `(level, preferred, least)`
    /// holds from that level until the next form. Its content is its name.
    struct Forms {
        id: Id,
        forms: Vec<(Level, Size, Size)>,
        accepts: Accepts,
    }

    impl Forms {
        fn form(&self, level: Level) -> (Size, Size) {
            let (_, preferred, least) = self
                .forms
                .iter()
                .rev()
                .find(|(from, _, _)| *from <= level)
                .expect("a form at Preferred");
            (*preferred, *least)
        }
    }

    impl LayoutBox<String> for Forms {
        fn id(&self) -> &Id {
            &self.id
        }
        fn accepts(&self) -> &Accepts {
            &self.accepts
        }
        fn measure(&self, orders: &Resolved, space: Size) -> Result<Answer, Unsupported> {
            let (preferred, least) = self.form(orders.level);
            Ok(Answer::of(&self.id, preferred, least, space))
        }
        fn arrange(
            &self,
            orders: &Resolved,
            at: Point,
            space: Size,
        ) -> Result<Node<String>, Unsupported> {
            Ok(leaf_node(
                &self.id,
                orders,
                at,
                self.measure(orders, space)?,
            ))
        }
    }

    /// A box of one size at every level.
    fn fixed(id: &str, width: f64, height: f64) -> Rc<Forms> {
        let size = Size::new(width, height);
        Rc::new(Forms {
            id: Id::new(id),
            forms: vec![(Level::Preferred, size, size)],
            accepts: Accepts::ordinary(),
        })
    }

    /// A box with a narrower form from Mild.
    fn compact(id: &str, preferred: f64, mild: f64, height: f64) -> Rc<Forms> {
        let at = |width| Size::new(width, height);
        Rc::new(Forms {
            id: Id::new(id),
            forms: vec![
                (Level::Preferred, at(preferred), at(preferred)),
                (Level::Mild, at(mild), at(mild)),
            ],
            accepts: Accepts::ordinary(),
        })
    }

    /// A box with a continuous range at each of its forms.
    fn ranged(id: &str, forms: Vec<(Level, f64, f64)>, height: f64) -> Rc<Forms> {
        Rc::new(Forms {
            id: Id::new(id),
            forms: forms
                .into_iter()
                .map(|(level, preferred, least)| {
                    (
                        level,
                        Size::new(preferred, height),
                        Size::new(least, height),
                    )
                })
                .collect(),
            accepts: Accepts::ordinary(),
        })
    }

    /// Text that wraps: its height grows as its width shrinks. It answers
    /// as the protocol asks: its least width is the least at which it fits
    /// the offered height.
    struct Wrapping {
        id: Id,
        natural: f64,
        least: f64,
        line: f64,
        accepts: Accepts,
    }

    impl Wrapping {
        fn height_at(&self, width: f64) -> f64 {
            (self.natural / width).ceil() * self.line
        }
    }

    impl LayoutBox<String> for Wrapping {
        fn id(&self) -> &Id {
            &self.id
        }
        fn accepts(&self) -> &Accepts {
            &self.accepts
        }
        fn measure(&self, _: &Resolved, space: Size) -> Result<Answer, Unsupported> {
            let width = self.natural.min(space.width).max(self.least);
            let height = self.height_at(width);
            let lines = (space.height / self.line).floor();
            let least_width = if lines >= 1.0 {
                (self.natural / lines).max(self.least)
            } else {
                self.natural
            };
            Ok(Answer::of(
                &self.id,
                Size::new(self.natural, height),
                Size::new(least_width, height),
                space,
            ))
        }
        fn arrange(
            &self,
            orders: &Resolved,
            at: Point,
            space: Size,
        ) -> Result<Node<String>, Unsupported> {
            Ok(leaf_node(
                &self.id,
                orders,
                at,
                self.measure(orders, space)?,
            ))
        }
    }

    fn wrapping(id: &str, natural: f64, least: f64, line: f64) -> Rc<Wrapping> {
        Rc::new(Wrapping {
            id: Id::new(id),
            natural,
            least,
            line,
            accepts: Accepts::ordinary(),
        })
    }

    /// The second review's text (item 2): 120 wide on one line, or
    /// narrower, down to 40, on three lines 30 high; from Mild a form 30 by
    /// 10.
    struct ReviewText {
        id: Id,
        accepts: Accepts,
    }

    impl LayoutBox<String> for ReviewText {
        fn id(&self) -> &Id {
            &self.id
        }
        fn accepts(&self) -> &Accepts {
            &self.accepts
        }
        fn measure(&self, orders: &Resolved, space: Size) -> Result<Answer, Unsupported> {
            if orders.level >= Level::Mild {
                let size = Size::new(30.0, 10.0);
                return Ok(Answer::of(&self.id, size, size, space));
            }
            let width = space.width.clamp(40.0, 120.0);
            let height = if width >= 120.0 { 10.0 } else { 30.0 };
            Ok(Answer::of(
                &self.id,
                Size::new(120.0, height),
                Size::new(40.0, height),
                space,
            ))
        }
        fn arrange(
            &self,
            orders: &Resolved,
            at: Point,
            space: Size,
        ) -> Result<Node<String>, Unsupported> {
            Ok(leaf_node(
                &self.id,
                orders,
                at,
                self.measure(orders, space)?,
            ))
        }
    }

    /// The second review's row (item 2): `a` 60 wide, 30 from Mild, and
    /// the text `b`.
    fn review_row() -> Stack<'static, String> {
        Stack::row(Id::new("row"))
            .child(compact("a", 60.0, 30.0, 20.0))
            .child(Rc::new(ReviewText {
                id: Id::new("b"),
                accepts: Accepts::ordinary(),
            }))
    }

    fn all_permitted() -> Order {
        Order::NONE.with_level(Level::AllPermitted)
    }

    fn level_of(plan: &Node<String>, id: &str) -> Level {
        plan.find(&Id::new(id)).expect(id).orders.level
    }

    fn bounds_of(plan: &Node<String>, id: &str) -> Rect {
        plan.find(&Id::new(id)).expect(id).bounds
    }

    fn fits(root: &dyn LayoutBox<String>, space: Size) -> bool {
        root.query(&all_permitted(), space).unwrap().fits()
    }

    /// Three buttons that go from 50 to 20 wide at Mild, as one list.
    fn actions() -> Rc<Stack<'static, String>> {
        Rc::new((1..=3).fold(
            Stack::list(Id::new("actions"), Direction::Across),
            |list, n| list.child(compact(&format!("button{n}"), 50.0, 20.0, 20.0)),
        ))
    }

    #[test]
    fn a_lists_members_change_together_and_the_short_list_says_who_gives_first() {
        let toolbar = |first: Vec<Id>| {
            Stack::row(Id::new("toolbar"))
                .child(compact("title", 100.0, 60.0, 20.0))
                .child(actions())
                .asked_first(first)
        };
        // The actions are asked first: they reach Mild together, and the
        // title is then not short of anything.
        let plan = lay_out(
            &toolbar(vec![Id::new("actions")]),
            &all_permitted(),
            Size::new(200.0, 20.0),
        )
        .unwrap();
        for n in 1..=3 {
            let button = format!("button{n}");
            assert_eq!(level_of(&plan, &button), Level::Mild);
            assert_eq!(bounds_of(&plan, &button).width, 20.0);
        }
        assert_eq!(level_of(&plan, "title"), Level::Preferred);
        assert_eq!(
            plan.reasons,
            vec![Reason::OrderedUp {
                child: Id::new("actions"),
                from: Level::Preferred,
                to: Level::Mild,
                showed: Showed::NoRoomForTheLeast {
                    needs: 250.0,
                    has: 200.0
                },
            }]
        );
        assert_eq!(plan.bounds.width, 160.0);
        // Asked in their order, the title gives first, and 60 + 150 is
        // still too wide, so the actions give too; then the title comes
        // back, since 100 + 60 fits.
        let plan = lay_out(&toolbar(vec![]), &all_permitted(), Size::new(200.0, 20.0)).unwrap();
        assert_eq!(level_of(&plan, "title"), Level::Preferred);
        assert_eq!(level_of(&plan, "button1"), Level::Mild);
        assert_eq!(
            plan.reasons.last(),
            Some(&Reason::OrderedDown {
                child: Id::new("title"),
                from: Level::Mild,
                to: Level::Preferred
            })
        );
        // The ceiling ordered from above is never passed.
        let refused = lay_out(
            &toolbar(vec![]),
            &Order::NONE.with_level(Level::Preferred),
            Size::new(200.0, 20.0),
        );
        assert!(matches!(refused, Err(Refusal::NoFit { .. })));
    }

    #[test]
    fn every_child_that_needs_it_reaches_mild_before_any_reaches_strong() {
        // First review, finding 3: the title is unchanged at Mild and 60
        // at Strong; the actions are 150 and 60 at Mild. In 200 the
        // actions at Mild alone fit, so the title is not touched.
        let row = Stack::row(Id::new("toolbar"))
            .child(ranged(
                "title",
                vec![
                    (Level::Preferred, 100.0, 100.0),
                    (Level::Strong, 100.0, 60.0),
                ],
                20.0,
            ))
            .child(actions());
        let plan = lay_out(&row, &all_permitted(), Size::new(200.0, 20.0)).unwrap();
        assert_eq!(level_of(&plan, "title"), Level::Preferred);
        assert_eq!(level_of(&plan, "actions"), Level::Mild);
        assert_eq!(bounds_of(&plan, "actions").width, 60.0);
        // With less room the title is ordered on to Strong.
        let plan = lay_out(&row, &all_permitted(), Size::new(130.0, 20.0)).unwrap();
        assert_eq!(level_of(&plan, "title"), Level::Strong);
        assert_eq!(level_of(&plan, "actions"), Level::Mild);
    }

    #[test]
    fn a_child_ordered_up_comes_back_down_when_the_whole_still_fits() {
        // Second review, specification fault: A (100, 90 from Mild) and B
        // (100, 40 from Mild) in 150. Both are ordered up, then A comes back
        // to Preferred: 100 + 40 fits.
        let row = Stack::row(Id::new("row"))
            .child(compact("a", 100.0, 90.0, 20.0))
            .child(compact("b", 100.0, 40.0, 20.0));
        let plan = lay_out(&row, &all_permitted(), Size::new(150.0, 20.0)).unwrap();
        assert_eq!(level_of(&plan, "a"), Level::Preferred);
        assert_eq!(level_of(&plan, "b"), Level::Mild);
        assert_eq!(plan.bounds.width, 140.0);
        let up = |child: &str, needs: f64| Reason::OrderedUp {
            child: Id::new(child),
            from: Level::Preferred,
            to: Level::Mild,
            showed: Showed::NoRoomForTheLeast { needs, has: 150.0 },
        };
        assert_eq!(
            plan.reasons,
            vec![
                up("a", 200.0),
                up("b", 190.0),
                Reason::OrderedDown {
                    child: Id::new("a"),
                    from: Level::Mild,
                    to: Level::Preferred
                },
            ]
        );
    }

    /// A plot that gives from 400 down to 200, beside an axis title of 60
    /// that is abbreviated to 30 from Mild.
    fn titled_plot() -> Stack<'static, String> {
        Stack::row(Id::new("chart"))
            .child(compact("title", 60.0, 30.0, 20.0))
            .member(
                Member::new(ranged("plot", vec![(Level::Preferred, 400.0, 200.0)], 20.0))
                    .flexible(),
            )
    }

    #[test]
    fn a_plot_gives_room_by_weight_before_a_sibling_changes_form() {
        // Second review, specification fault: in 430 the plot gives 30 and
        // the title keeps its words; it is not abbreviated to save the plot
        // its 400.
        let plan = lay_out(&titled_plot(), &all_permitted(), Size::new(430.0, 20.0)).unwrap();
        assert_eq!(level_of(&plan, "title"), Level::Preferred);
        assert_eq!(bounds_of(&plan, "plot").width, 370.0);
        assert!(plan.reasons.is_empty());
        // How much each gives is its weight times its range: with a legend
        // from 100 to 50 in 500, the plot (rate 200) and the legend (rate
        // 50) share 60 as 48 and 12; at a weight of 4 the legend's rate is
        // 200 too, and they give 30 each.
        let with_legend = |weight: f64| {
            titled_plot().member(
                Member::new(ranged(
                    "legend",
                    vec![(Level::Preferred, 100.0, 50.0)],
                    20.0,
                ))
                .with_weight(weight),
            )
        };
        let widths = |plan: &Node<String>| {
            (
                bounds_of(plan, "plot").width,
                bounds_of(plan, "legend").width,
            )
        };
        let even = lay_out(&with_legend(1.0), &all_permitted(), Size::new(500.0, 20.0)).unwrap();
        let (plot, legend) = widths(&even);
        assert!((plot - 352.0).abs() < 1e-9 && (legend - 88.0).abs() < 1e-9);
        let heavy = lay_out(&with_legend(4.0), &all_permitted(), Size::new(500.0, 20.0)).unwrap();
        let (plot, legend) = widths(&heavy);
        assert!((plot - 370.0).abs() < 1e-9 && (legend - 70.0).abs() < 1e-9);
        assert_eq!(level_of(&heavy, "title"), Level::Preferred);
        // A weight of zero protects the preferred length: the legend keeps
        // 100 and the plot gives all 60.
        let protected =
            lay_out(&with_legend(0.0), &all_permitted(), Size::new(500.0, 20.0)).unwrap();
        assert_eq!(widths(&protected), (340.0, 100.0));
        // The child that gives also grows: in 700 it takes the surplus.
        let wide = lay_out(&titled_plot(), &all_permitted(), Size::new(700.0, 20.0)).unwrap();
        assert_eq!(bounds_of(&wide, "plot"), Rect::new(60.0, 0.0, 640.0, 20.0));
        assert_eq!(wide.bounds.width, 700.0);
    }

    #[test]
    fn a_sibling_changes_form_only_when_the_least_lengths_do_not_fit() {
        // In 260 the least lengths, 60 and 200, just fit.
        let plan = lay_out(&titled_plot(), &all_permitted(), Size::new(260.0, 20.0)).unwrap();
        assert_eq!(level_of(&plan, "title"), Level::Preferred);
        assert_eq!(bounds_of(&plan, "plot").width, 200.0);
        // In 250 they do not, and the title is abbreviated.
        let plan = lay_out(&titled_plot(), &all_permitted(), Size::new(250.0, 20.0)).unwrap();
        assert_eq!(level_of(&plan, "title"), Level::Mild);
        assert_eq!(bounds_of(&plan, "plot").width, 220.0);
        assert_eq!(
            plan.reasons,
            vec![Reason::OrderedUp {
                child: Id::new("title"),
                from: Level::Preferred,
                to: Level::Mild,
                showed: Showed::NoRoomForTheLeast {
                    needs: 260.0,
                    has: 250.0
                },
            }]
        );
    }

    #[test]
    fn a_child_is_not_ordered_into_a_form_that_falls_short_across() {
        // First review, finding 4: `a` is narrower at Mild but too tall for
        // the row. It stays, and `b` gives at Strong instead.
        let row = Stack::row(Id::new("row"))
            .child(Rc::new(Forms {
                id: Id::new("a"),
                forms: vec![
                    (
                        Level::Preferred,
                        Size::new(80.0, 20.0),
                        Size::new(80.0, 20.0),
                    ),
                    (Level::Mild, Size::new(40.0, 40.0), Size::new(40.0, 40.0)),
                ],
                accepts: Accepts::ordinary(),
            }))
            .child(ranged(
                "b",
                vec![(Level::Preferred, 40.0, 40.0), (Level::Strong, 15.0, 15.0)],
                20.0,
            ));
        let plan = lay_out(&row, &all_permitted(), Size::new(100.0, 20.0)).unwrap();
        assert_eq!(level_of(&plan, "a"), Level::Preferred);
        assert_eq!(level_of(&plan, "b"), Level::Strong);
        assert_eq!(bounds_of(&plan, "b"), Rect::new(80.0, 0.0, 15.0, 20.0));
    }

    #[test]
    fn whoever_holds_a_ceiling_over_a_list_works_up_from_preferred() {
        // A list under arrangements, ordered AllPermitted with unlimited
        // room, keeps every member at Preferred; so does a list at the
        // root of a plan.
        let list: Child<'static, String> = actions();
        let parent = Arrangements::new(Id::new("parent"), vec![Rc::clone(&list)]);
        for plan in [
            lay_out(&parent, &all_permitted(), Size::INFINITE).unwrap(),
            lay_out(list.as_ref(), &all_permitted(), Size::INFINITE).unwrap(),
        ] {
            assert_eq!(level_of(&plan, "button2"), Level::Preferred);
            assert_eq!(bounds_of(&plan, "button2").width, 50.0);
        }
        // In 100 they take Mild, and no more.
        let plan = lay_out(&parent, &all_permitted(), Size::new(100.0, 20.0)).unwrap();
        assert_eq!(level_of(&plan, "button2"), Level::Mild);
    }

    /// `a`, a gap and `b` in a row, and the same in a column.
    fn row_and_column() -> (Child<'static, String>, Child<'static, String>) {
        let a: Child<'static, String> = fixed("a", 40.0, 10.0);
        let b: Child<'static, String> = fixed("b", 40.0, 10.0);
        let row: Child<'static, String> = Rc::new(
            Stack::row(Id::new("row"))
                .child(Rc::clone(&a))
                .gap(2.0, 10.0)
                .child(Rc::clone(&b)),
        );
        let column: Child<'static, String> = Rc::new(
            Stack::column(Id::new("column"))
                .child(a)
                .gap(2.0, 10.0)
                .child(b),
        );
        (row, column)
    }

    #[test]
    fn a_column_that_fits_mildly_is_taken_over_a_row_squeezed_at_strong() {
        // In 85 the row fits only with its gap squeezed at Strong; the
        // column fits at Preferred.
        let (row, column) = row_and_column();
        let parent = Arrangements::new(Id::new("parent"), vec![row, column]);
        let plan = lay_out(&parent, &all_permitted(), Size::new(85.0, 40.0)).unwrap();
        assert!(plan.find(&Id::new("row")).is_none());
        assert_eq!(level_of(&plan, "column/gap[a|b]"), Level::Preferred);
        assert_eq!(bounds_of(&plan, "b"), Rect::new(0.0, 20.0, 40.0, 10.0));
        // Too low for the column: the row is squeezed in the Strong pass.
        let plan = lay_out(&parent, &all_permitted(), Size::new(85.0, 20.0)).unwrap();
        assert!(plan.find(&Id::new("column")).is_none());
        assert_eq!(bounds_of(&plan, "row/gap[a|b]").width, 5.0);
        // The passes are data: one pass straight to AllPermitted takes the
        // row.
        let (row, column) = row_and_column();
        let eager = Arrangements::new(Id::new("parent"), vec![row, column])
            .with_passes([Level::AllPermitted]);
        let plan = lay_out(&eager, &all_permitted(), Size::new(85.0, 40.0)).unwrap();
        assert!(plan.find(&Id::new("row")).is_some());
    }

    /// A chart that is `across` (upright) or `up` (turned), with `a` and
    /// `b` 40 by 10.
    fn upright_or_turned() -> Arrangements<'static, String> {
        let upright = Presentation::default().with("direction", "upright");
        let turned = Presentation::default().with("direction", "turned");
        let both = || Accepts::new(upright.clone(), vec![upright.clone(), turned.clone()]);
        let (a, b): (Child<'static, String>, Child<'static, String>) =
            (fixed("a", 40.0, 10.0), fixed("b", 40.0, 10.0));
        let across: Child<'static, String> = Rc::new(
            Stack::row(Id::new("across"))
                .accepting(both())
                .child(Rc::clone(&a))
                .child(Rc::clone(&b)),
        );
        let up: Child<'static, String> = Rc::new(
            Stack::column(Id::new("up"))
                .accepting(both())
                .child(a)
                .child(b),
        );
        Arrangements::new(Id::new("chart"), vec![across, up])
            .accepting(both())
            .tagged(&Id::new("across"), "direction", "upright")
            .tagged(&Id::new("up"), "direction", "turned")
    }

    #[test]
    fn an_order_naming_a_tagged_kind_restricts_the_arrangements_and_reaches_the_chosen() {
        let turned = Presentation::default().with("direction", "turned");
        let chart = upright_or_turned();
        let free = lay_out(&chart, &all_permitted(), Size::INFINITE).unwrap();
        assert!(free.find(&Id::new("across")).is_some());
        let group = all_permitted().with("direction", "turned");
        let plan = lay_out(&chart, &group, Size::INFINITE).unwrap();
        assert!(plan.find(&Id::new("across")).is_none());
        assert_eq!(
            plan.find(&Id::new("up")).unwrap().orders.presentation,
            turned
        );
        // A row or column passes its children only the level.
        assert_eq!(
            plan.find(&Id::new("a")).unwrap().orders.presentation,
            Presentation::default()
        );
    }

    #[test]
    fn the_node_records_the_arrangement_chosen_and_the_kind_it_stands_for() {
        // Second review, specification fault: not ordered a direction, the
        // chart takes the turned arrangement in 50 by 30, and its node says
        // so, not the family's default.
        let plan = lay_out(
            &upright_or_turned(),
            &all_permitted(),
            Size::new(50.0, 30.0),
        )
        .unwrap();
        assert_eq!(plan.children[0].id, Id::new("up"));
        assert_eq!(
            plan.orders.presentation,
            Presentation::default().with("direction", "turned")
        );
        assert_eq!(
            plan.reasons.last(),
            Some(&Reason::ArrangementTaken {
                arrangement: Id::new("up")
            })
        );
        let wide = lay_out(
            &upright_or_turned(),
            &all_permitted(),
            Size::new(100.0, 30.0),
        )
        .unwrap();
        assert_eq!(
            wide.orders.presentation,
            Presentation::default().with("direction", "upright")
        );
    }

    /// A column of `w` (120 by 20, and 80 by 30 from Mild) and `f` (50 by
    /// 20).
    fn w_over_f() -> Stack<'static, String> {
        Stack::column(Id::new("inner"))
            .child(w())
            .child(fixed("f", 50.0, 20.0))
    }

    fn w() -> Rc<Forms> {
        Rc::new(Forms {
            id: Id::new("w"),
            forms: vec![
                (
                    Level::Preferred,
                    Size::new(120.0, 20.0),
                    Size::new(120.0, 20.0),
                ),
                (Level::Mild, Size::new(80.0, 30.0), Size::new(80.0, 30.0)),
            ],
            accepts: Accepts::ordinary(),
        })
    }

    #[test]
    fn a_composition_answers_what_it_would_do_in_the_space_offered() {
        // First review, finding 5: offered width 100, the column needs 50,
        // not 40.
        let answer = w_over_f()
            .query(&all_permitted(), Size::new(100.0, 100.0))
            .unwrap();
        assert_eq!(answer.preferred.height, 50.0);
        assert_eq!(answer.least.height, 50.0);
        assert_eq!(answer.taken, Size::new(80.0, 50.0));
        let outer = Stack::column(Id::new("outer"))
            .child(Rc::new(w_over_f()))
            .child(fixed("y", 50.0, 10.0));
        let plan = lay_out(&outer, &all_permitted(), Size::new(100.0, 60.0)).unwrap();
        assert_eq!(bounds_of(&plan, "y").y, 50.0);
        assert_eq!(level_of(&plan, "w"), Level::Mild);
    }

    /// Every node's identity, level and bounds.
    fn choices(plan: &Node<String>) -> Vec<(Id, Level, Rect)> {
        plan.nodes()
            .into_iter()
            .map(|node| (node.id.clone(), node.orders.level, node.bounds))
            .collect()
    }

    #[test]
    fn a_query_and_the_plan_in_the_same_space_make_the_same_choices() {
        // Second review, item 2: offered 170 by 20, the query keeps `a` at
        // Preferred and orders `b` to Mild, taking 90. The plan is made in
        // the 170 offered, not the 90 taken, and makes the same choices.
        let space = Size::new(170.0, 20.0);
        let plan = lay_out(&review_row(), &all_permitted(), space).unwrap();
        assert_eq!(level_of(&plan, "a"), Level::Preferred);
        assert_eq!(level_of(&plan, "b"), Level::Mild);
        assert_eq!(plan.bounds, Rect::new(0.0, 0.0, 90.0, 20.0));
        let answer = review_row().query(&all_permitted(), space).unwrap();
        assert!(answer.fits());
        assert_eq!(answer.taken, plan.bounds.size());
        // The same for other boxes, each asked and ordered afresh.
        let (row, column) = row_and_column();
        let arrangements: Child<'static, String> =
            Rc::new(Arrangements::new(Id::new("parent"), vec![row, column]));
        type Maker = (Box<dyn Fn() -> Child<'static, String>>, Size);
        let makers: Vec<Maker> = vec![
            (Box::new(|| Rc::new(review_row())), space),
            (Box::new(|| Rc::new(w_over_f())), Size::new(100.0, 100.0)),
            (
                Box::new(move || Rc::clone(&arrangements)),
                Size::new(85.0, 40.0),
            ),
            (
                Box::new(|| {
                    Rc::new(
                        Stack::row(Id::new("toolbar"))
                            .child(compact("title", 100.0, 60.0, 20.0))
                            .child(actions()),
                    )
                }),
                Size::new(170.0, 30.0),
            ),
        ];
        for (make, space) in makers {
            let planned = lay_out(make().as_ref(), &all_permitted(), space).unwrap();
            let at = Order::NONE.with_level(planned.orders.level);
            let fresh = make();
            fresh.forget();
            let ordered = fresh.order(&at, Point::default(), space).unwrap();
            assert_eq!(choices(&planned), choices(&ordered), "{:?}", planned.id);
            fresh.forget();
            let taken = fresh.query(&at, space).unwrap().taken;
            assert_eq!(taken, planned.bounds.size(), "{:?}", planned.id);
        }
    }

    #[test]
    fn a_box_offered_its_least_length_in_one_direction_fits_and_a_little_less_does_not() {
        // Second review, item 3: W (120 by 20, 80 by 30 from Mild) beside F
        // (50 by 20), offered 400 by 100. Its least width at height 100 is
        // 130 and its least height at width 400 is 20.
        let row = Stack::row(Id::new("row"))
            .child(w())
            .child(fixed("f", 50.0, 20.0));
        let answer = row
            .query(&all_permitted(), Size::new(400.0, 100.0))
            .unwrap();
        assert_eq!(answer.least, Size::new(130.0, 20.0));
        assert_eq!(answer.preferred, Size::new(170.0, 20.0));
        assert!(fits(&row, Size::new(130.0, 100.0)));
        assert!(!fits(&row, Size::new(129.5, 100.0)));
        assert!(fits(&row, Size::new(400.0, 20.0)));
        assert!(!fits(&row, Size::new(400.0, 19.5)));
        // The pair is not a size it is promised to fit: W is 80 wide only
        // by being 30 high. At width 130 its least height is 30.
        assert!(!fits(&row, Size::new(130.0, 20.0)));
        let narrow = row
            .query(&all_permitted(), Size::new(130.0, 100.0))
            .unwrap();
        assert_eq!(narrow.least.height, 30.0);
        assert!(fits(&row, Size::new(130.0, 30.0)));
        assert!(!fits(&row, Size::new(130.0, 29.5)));
        // The same along and across a column.
        let column = w_over_f();
        let answer = column
            .query(&all_permitted(), Size::new(100.0, 100.0))
            .unwrap();
        assert_eq!(answer.least, Size::new(80.0, 50.0));
        assert!(fits(&column, Size::new(80.0, 100.0)));
        assert!(!fits(&column, Size::new(79.5, 100.0)));
        assert!(fits(&column, Size::new(100.0, 50.0)));
        assert!(!fits(&column, Size::new(100.0, 49.5)));
    }

    #[test]
    fn a_box_with_arrangements_fits_a_column_by_the_arrangement_it_takes() {
        // Second review, item 1: a row (120 by 20) or a column (60 by 40),
        // in a column offered 200 by 30. The row fits; its least height is
        // the row's, not the column's.
        let choice: Child<'static, String> = Rc::new(Arrangements::new(
            Id::new("choice"),
            vec![fixed("row", 120.0, 20.0), fixed("column", 60.0, 40.0)],
        ));
        let answer = choice
            .query(&Order::NONE, Size::new(200.0, f64::INFINITY))
            .unwrap();
        assert_eq!(answer.least, Size::new(60.0, 20.0));
        assert_eq!(answer.preferred, Size::new(120.0, 20.0));
        let outer = Stack::column(Id::new("outer")).child(choice);
        let plan = lay_out(&outer, &all_permitted(), Size::new(200.0, 30.0)).unwrap();
        assert_eq!(bounds_of(&plan, "row"), Rect::new(0.0, 0.0, 120.0, 20.0));
        assert!(plan.find(&Id::new("column")).is_none());
    }

    #[test]
    fn arrangements_report_each_least_over_the_arrangements_that_fit_the_other_direction() {
        let (a, b): (Child<'static, String>, Child<'static, String>) =
            (fixed("a", 100.0, 20.0), fixed("b", 100.0, 20.0));
        let row: Child<'static, String> = Rc::new(
            Stack::row(Id::new("row"))
                .child(Rc::clone(&a))
                .child(Rc::clone(&b)),
        );
        let column: Child<'static, String> =
            Rc::new(Stack::column(Id::new("column")).child(a).child(b));
        let parent = Arrangements::new(Id::new("parent"), vec![row, column]);
        // Both fit 300 by 50: the least width is the column's, the least
        // height the row's.
        let answer = parent
            .query(&all_permitted(), Size::new(300.0, 50.0))
            .unwrap();
        assert_eq!(answer.preferred, Size::new(200.0, 20.0));
        assert_eq!(answer.least, Size::new(100.0, 20.0));
        // At height 30 only the row fits, so the least width is 200.
        let low = parent
            .query(&all_permitted(), Size::new(300.0, 30.0))
            .unwrap();
        assert_eq!(low.least.width, 200.0);
        assert!(fits(&parent, Size::new(200.0, 30.0)));
        assert!(!fits(&parent, Size::new(199.5, 30.0)));
        // At width 150 only the column fits, so the least height is 40.
        let narrow = parent
            .query(&all_permitted(), Size::new(150.0, 50.0))
            .unwrap();
        assert_eq!(narrow.least.height, 40.0);
        assert!(fits(&parent, Size::new(150.0, 40.0)));
        assert!(!fits(&parent, Size::new(150.0, 39.5)));
    }

    /// A leaf with one size and a baseline.
    struct Lettered {
        id: Id,
        size: Size,
        baseline: f64,
        accepts: Accepts,
    }

    impl LayoutBox<String> for Lettered {
        fn id(&self) -> &Id {
            &self.id
        }
        fn accepts(&self) -> &Accepts {
            &self.accepts
        }
        fn measure(&self, _: &Resolved, space: Size) -> Result<Answer, Unsupported> {
            Ok(Answer::of(&self.id, self.size, self.size, space).with_baseline(self.baseline))
        }
        fn arrange(
            &self,
            orders: &Resolved,
            at: Point,
            space: Size,
        ) -> Result<Node<String>, Unsupported> {
            Ok(leaf_node(
                &self.id,
                orders,
                at,
                self.measure(orders, space)?,
            ))
        }
    }

    fn lettered(id: &str, height: f64, baseline: f64) -> Rc<Lettered> {
        Rc::new(Lettered {
            id: Id::new(id),
            size: Size::new(10.0, height),
            baseline,
            accepts: Accepts::ordinary(),
        })
    }

    #[test]
    fn a_row_centres_its_members_and_a_column_starts_them() {
        let row = Stack::row(Id::new("row"))
            .child(fixed("tall", 10.0, 20.0))
            .child(fixed("centred", 10.0, 10.0))
            .member(Member::new(fixed("top", 10.0, 10.0)).aligned(Align::Start))
            .member(Member::new(fixed("bottom", 10.0, 10.0)).aligned(Align::End))
            .member(
                Member::new(ranged("filled", vec![(Level::Preferred, 10.0, 10.0)], 4.0))
                    .aligned(Align::Fill),
            );
        let plan = lay_out(&row, &Order::NONE, Size::new(100.0, 50.0)).unwrap();
        assert_eq!(plan.bounds.height, 20.0);
        assert_eq!(bounds_of(&plan, "centred").y, 5.0);
        assert_eq!(bounds_of(&plan, "top").y, 0.0);
        assert_eq!(bounds_of(&plan, "bottom").y, 10.0);
        assert_eq!(bounds_of(&plan, "filled"), Rect::new(40.0, 0.0, 10.0, 20.0));
        let column = Stack::column(Id::new("column"))
            .child(fixed("wide", 40.0, 10.0))
            .child(fixed("narrow", 10.0, 10.0));
        let plan = lay_out(&column, &Order::NONE, Size::new(100.0, 50.0)).unwrap();
        assert_eq!(bounds_of(&plan, "narrow").x, 0.0);
    }

    #[test]
    fn a_row_can_share_a_baseline_and_a_column_reports_its_first() {
        let row = Stack::row(Id::new("row"))
            .on_baselines()
            .child(lettered("big", 20.0, 15.0))
            .child(lettered("small", 10.0, 8.0));
        let answer = row.query(&Order::NONE, Size::new(100.0, 50.0)).unwrap();
        assert_eq!(answer.baseline, Some(15.0));
        assert_eq!(answer.preferred.height, 20.0);
        let plan = lay_out(&row, &Order::NONE, Size::new(100.0, 50.0)).unwrap();
        assert_eq!(bounds_of(&plan, "small").y, 7.0);
        assert_eq!(plan.baseline, Some(15.0));
        let column = Stack::column(Id::new("column"))
            .child(fixed("rule", 10.0, 4.0))
            .child(lettered("line", 10.0, 8.0));
        let answer = column.query(&Order::NONE, Size::new(100.0, 50.0)).unwrap();
        assert_eq!(answer.baseline, Some(12.0));
        let plan = lay_out(&column, &Order::NONE, Size::new(100.0, 50.0)).unwrap();
        assert_eq!(plan.baseline, Some(12.0));
    }

    #[test]
    fn surplus_goes_to_the_growing_members_by_weight() {
        let row = |grow: bool| {
            let member = |id: &str, weight: f64| {
                let member = Member::new(ranged(id, vec![(Level::Preferred, 20.0, 10.0)], 10.0));
                if grow { member.growing(weight) } else { member }
            };
            Stack::row(Id::new("row"))
                .member(member("one", 1.0))
                .member(member("three", 3.0))
                .child(fixed("still", 20.0, 10.0))
        };
        let plan = lay_out(&row(true), &Order::NONE, Size::new(100.0, 10.0)).unwrap();
        assert_eq!(bounds_of(&plan, "one").width, 30.0);
        assert_eq!(bounds_of(&plan, "three"), Rect::new(30.0, 0.0, 50.0, 10.0));
        assert_eq!(bounds_of(&plan, "still").x, 80.0);
        // A growing row takes the room offered; its preferred width is
        // still what it would take with unlimited width.
        let answer = row(true)
            .query(&Order::NONE, Size::new(100.0, 10.0))
            .unwrap();
        assert_eq!((answer.taken.width, answer.preferred.width), (100.0, 60.0));
        // Without growing members the surplus is left.
        let plan = lay_out(&row(false), &Order::NONE, Size::new(100.0, 10.0)).unwrap();
        assert_eq!(plan.bounds.width, 60.0);
        assert_eq!(bounds_of(&plan, "still").x, 40.0);
    }

    #[test]
    fn a_list_passes_what_it_was_ordered_and_a_member_keeps_its_family_default() {
        let emphasised = Presentation::new(Prominence::Emphasised, Default::default());
        let member = || {
            Rc::new(Forms {
                id: Id::new("member"),
                forms: vec![(
                    Level::Preferred,
                    Size::new(10.0, 10.0),
                    Size::new(10.0, 10.0),
                )],
                accepts: Accepts::new(
                    emphasised.clone(),
                    vec![emphasised.clone(), Presentation::default()],
                ),
            })
        };
        let list = Stack::list(Id::new("list"), Direction::Across).child(member());
        let plan = lay_out(&list, &Order::NONE, Size::INFINITE).unwrap();
        assert_eq!(
            plan.find(&Id::new("member")).unwrap().orders.presentation,
            emphasised
        );
        let normal = Order::NONE.with_prominence(Prominence::Normal);
        let plan = lay_out(&list, &normal, Size::INFINITE).unwrap();
        assert_eq!(
            plan.find(&Id::new("member")).unwrap().orders.presentation,
            Presentation::default()
        );
    }

    #[test]
    fn a_gap_is_named_by_what_it_separates() {
        let ids = |stack: &Stack<'static, String>| -> Vec<String> {
            stack
                .members()
                .iter()
                .map(|m| m.child.id().as_str().to_owned())
                .collect()
        };
        let short = Stack::row(Id::new("row"))
            .child(fixed("a", 1.0, 1.0))
            .gap(1.0, 2.0)
            .child(fixed("b", 1.0, 1.0));
        let long = Stack::row(Id::new("row"))
            .gap(1.0, 2.0)
            .child(fixed("z", 1.0, 1.0))
            .gap(1.0, 2.0)
            .child(fixed("a", 1.0, 1.0))
            .gap(1.0, 2.0)
            .child(fixed("b", 1.0, 1.0));
        assert_eq!(ids(&short), ["a", "row/gap[a|b]", "b"]);
        assert_eq!(
            ids(&long),
            [
                "row/gap[start|z]",
                "z",
                "row/gap[z|a]",
                "a",
                "row/gap[a|b]",
                "b"
            ]
        );
    }

    #[test]
    fn the_level_a_gap_is_squeezed_from_is_given_to_the_composition() {
        let row = Stack::row(Id::new("row"))
            .child(fixed("a", 40.0, 10.0))
            .gap(2.0, 10.0)
            .child(fixed("b", 40.0, 10.0))
            .squeezing_from(Level::Mild);
        let plan = lay_out(
            &row,
            &Order::NONE.with_level(Level::Mild),
            Size::new(85.0, 10.0),
        )
        .unwrap();
        assert_eq!(bounds_of(&plan, "row/gap[a|b]").width, 5.0);
    }

    /// A leaf that counts its measurements.
    struct Counted {
        inner: Rc<Forms>,
        count: Rc<std::cell::Cell<usize>>,
    }

    impl LayoutBox<String> for Counted {
        fn id(&self) -> &Id {
            self.inner.id()
        }
        fn accepts(&self) -> &Accepts {
            self.inner.accepts()
        }
        fn measure(&self, orders: &Resolved, space: Size) -> Result<Answer, Unsupported> {
            self.count.set(self.count.get() + 1);
            self.inner.measure(orders, space)
        }
        fn arrange(
            &self,
            orders: &Resolved,
            at: Point,
            space: Size,
        ) -> Result<Node<String>, Unsupported> {
            self.inner.arrange(orders, at, space)
        }
    }

    /// Rows and columns in turn, three children each, `depth` deep.
    fn tree(
        depth: usize,
        path: &str,
        count: &Rc<std::cell::Cell<usize>>,
    ) -> Child<'static, String> {
        if depth == 0 {
            return Rc::new(Counted {
                inner: compact(path, 30.0, 20.0, 10.0),
                count: Rc::clone(count),
            });
        }
        let id = Id::new(path);
        let stack = if depth.is_multiple_of(2) {
            Stack::row(id)
        } else {
            Stack::column(id)
        };
        Rc::new((0..3).fold(stack, |stack, n| {
            stack.child(tree(depth - 1, &format!("{path}/{n}"), count))
        }))
    }

    #[test]
    fn a_tree_four_deep_needs_hundreds_of_leaf_measurements() {
        let count = Rc::new(std::cell::Cell::new(0));
        let root = tree(4, "root", &count);
        // Preferred, the tree is 270 by 90; at Mild 180 wide.
        let plan = lay_out(root.as_ref(), &all_permitted(), Size::new(200.0, 100.0)).unwrap();
        assert!(plan.bounds.width <= 200.0);
        assert_eq!(level_of(&plan, "root/0/0/0/0"), Level::Mild);
        let measured = count.get();
        assert!(measured < 1000, "{measured} leaf measurements");
    }

    #[test]
    fn adding_to_a_composition_after_it_was_asked_clears_what_it_remembered() {
        // Second review, item 7: query, add a child, query again in the
        // same space.
        let space = Size::new(200.0, 20.0);
        let row = Stack::row(Id::new("row")).child(fixed("a", 40.0, 10.0));
        assert_eq!(
            row.query(&Order::NONE, space).unwrap().preferred.width,
            40.0
        );
        let row = row.child(fixed("b", 40.0, 10.0));
        assert_eq!(
            row.query(&Order::NONE, space).unwrap().preferred.width,
            80.0
        );
        let row = row.gap(2.0, 10.0).child(fixed("c", 40.0, 10.0));
        assert_eq!(
            row.query(&Order::NONE, space).unwrap().preferred.width,
            130.0
        );
        // Told to forget, a box asks its children again.
        let count = Rc::new(std::cell::Cell::new(0));
        let counted = Stack::column(Id::new("column")).child(Rc::new(Counted {
            inner: fixed("leaf", 10.0, 10.0),
            count: Rc::clone(&count),
        }));
        counted.query(&Order::NONE, space).unwrap();
        let first = count.get();
        counted.query(&Order::NONE, space).unwrap();
        assert_eq!(count.get(), first);
        counted.forget();
        counted.query(&Order::NONE, space).unwrap();
        assert!(count.get() > first);
    }

    #[test]
    fn a_child_is_not_ordered_up_when_its_next_level_would_not_help() {
        let row = Stack::row(Id::new("row"))
            .child(fixed("fixed", 50.0, 20.0))
            .child(compact("compact", 60.0, 30.0, 20.0))
            .asked_first([Id::new("fixed")]);
        let plan = lay_out(&row, &all_permitted(), Size::new(85.0, 20.0)).unwrap();
        assert_eq!(level_of(&plan, "fixed"), Level::Preferred);
        assert_eq!(level_of(&plan, "compact"), Level::Mild);
        let showed = Showed::NoRoomForTheLeast {
            needs: 110.0,
            has: 85.0,
        };
        assert_eq!(
            plan.reasons,
            vec![
                Reason::Kept {
                    child: Id::new("fixed"),
                    level: Level::Preferred,
                    showed: showed.clone(),
                    why: Kept::AtCeiling,
                },
                Reason::OrderedUp {
                    child: Id::new("compact"),
                    from: Level::Preferred,
                    to: Level::Mild,
                    showed,
                },
            ]
        );
        // A form that is no narrower does not help either.
        let wider = Stack::row(Id::new("row")).child(compact("wider", 60.0, 70.0, 20.0));
        let refused = lay_out(&wider, &all_permitted(), Size::new(50.0, 20.0));
        let Err(Refusal::NoFit { .. }) = refused else {
            panic!("expected no fit, got {refused:?}");
        };
        assert!(!fits(&wider, Size::new(50.0, 20.0)));
    }

    #[test]
    fn each_arrangement_starts_from_preferred_and_the_first_that_fits_wins() {
        let a: Child<'static, String> = compact("a", 80.0, 60.0, 20.0);
        let b: Child<'static, String> = fixed("b", 80.0, 20.0);
        let row: Child<'static, String> = Rc::new(
            Stack::row(Id::new("row"))
                .child(Rc::clone(&a))
                .child(Rc::clone(&b)),
        );
        let column: Child<'static, String> =
            Rc::new(Stack::column(Id::new("column")).child(a).child(b));
        let parent = Arrangements::new(Id::new("parent"), vec![row, column]);

        // The row orders `a` up and still does not fit; the column starts
        // again from Preferred and fits.
        let plan = lay_out(&parent, &all_permitted(), Size::new(100.0, 50.0)).unwrap();
        assert!(plan.find(&Id::new("row")).is_none());
        assert!(plan.find(&Id::new("column")).is_some());
        assert_eq!(level_of(&plan, "a"), Level::Preferred);
        assert_eq!(
            plan.find(&Id::new("b")).unwrap().bounds,
            Rect::new(0.0, 20.0, 80.0, 20.0)
        );
        assert!(matches!(
            &plan.reasons[..],
            [
                Reason::ArrangementRejected { arrangement: rejected, .. },
                Reason::ArrangementTaken { arrangement: taken },
            ] if rejected == &Id::new("row") && taken == &Id::new("column")
        ));

        // With room, the first arrangement is taken.
        let wide = lay_out(&parent, &all_permitted(), Size::new(200.0, 50.0)).unwrap();
        assert!(wide.find(&Id::new("row")).is_some());
        assert_eq!(
            wide.reasons,
            vec![Reason::ArrangementTaken {
                arrangement: Id::new("row")
            }]
        );
    }

    #[test]
    fn an_unsupported_order_is_an_error_and_not_a_failure_to_fit() {
        let emphasised = Presentation::new(Prominence::Emphasised, Default::default());
        let list = Stack::list(Id::new("list"), Direction::Across)
            .accepting(Accepts::new(
                Presentation::default(),
                vec![Presentation::default(), emphasised.clone()],
            ))
            .child(fixed("ordinary", 10.0, 10.0));
        let order = Order::NONE.with_prominence(Prominence::Emphasised);
        // Even with every room in the world, it is refused by name.
        let refused = lay_out(&list, &order, Size::INFINITE);
        assert_eq!(
            refused,
            Err(Refusal::Unsupported(Unsupported {
                part: Id::new("ordinary"),
                requested: emphasised.clone(),
            }))
        );
        assert!(
            fixed("leaf", 10.0, 10.0)
                .query(&order, Size::new(1.0, 1.0))
                .is_err()
        );
        // An accepted order that does not fit is a different answer.
        let small = lay_out(&list, &Order::NONE, Size::new(5.0, 5.0));
        assert!(matches!(small, Err(Refusal::NoFit { .. })));
    }

    #[test]
    fn no_fit_names_the_part_the_direction_and_the_size_needed() {
        let row = Stack::row(Id::new("row"))
            .child(wrapping("text", 120.0, 40.0, 10.0))
            .child(fixed("mark", 30.0, 10.0));
        let row_short = |needs: f64, has: f64| {
            Err(Refusal::NoFit {
                short: vec![Shortfall {
                    part: Id::new("row"),
                    direction: Direction::Across,
                    needs,
                    has,
                }],
            })
        };
        // Too narrow: the row names its own least width, 40 + 30.
        assert_eq!(
            lay_out(&row, &all_permitted(), Size::new(50.0, 100.0)),
            row_short(70.0, 50.0)
        );
        // At height 15 the text must be on one line, so the row needs 150.
        assert_eq!(
            lay_out(&row, &all_permitted(), Size::new(100.0, 15.0)),
            row_short(150.0, 100.0)
        );
        // Lower than a line, each child names the height it needs.
        let down = |part: &str| Shortfall {
            part: Id::new(part),
            direction: Direction::Down,
            needs: 10.0,
            has: 5.0,
        };
        assert_eq!(
            lay_out(&row, &all_permitted(), Size::new(200.0, 5.0)),
            Err(Refusal::NoFit {
                short: vec![down("text"), down("mark")]
            })
        );
        // With height for two lines, the text wraps and fits.
        let fits = lay_out(&row, &all_permitted(), Size::new(100.0, 30.0)).unwrap();
        assert_eq!(
            fits.find(&Id::new("text")).unwrap().bounds,
            Rect::new(0.0, 0.0, 70.0, 20.0)
        );
        assert_eq!(fits.find(&Id::new("mark")).unwrap().bounds.x, 70.0);
    }

    #[test]
    fn queries_repeat_and_the_plan_finds_a_node_by_identity() {
        let column = Stack::column(Id::new("column"))
            .child(wrapping("text", 120.0, 40.0, 10.0))
            .gap(2.0, 6.0)
            .child(fixed("mark", 30.0, 10.0));
        let space = Size::new(50.0, 100.0);
        let first = column.query(&all_permitted(), space).unwrap();
        assert_eq!(first, column.query(&all_permitted(), space).unwrap());
        assert_eq!(first.preferred.height, 30.0 + 6.0 + 10.0);
        let plan = lay_out(&column, &all_permitted(), space).unwrap();
        let mark = plan.find(&Id::new("mark")).unwrap();
        assert_eq!(mark.bounds, Rect::new(0.0, 36.0, 30.0, 10.0));
        assert_eq!(mark.content.as_deref(), Some("mark"));
        assert!(plan.find(&Id::new("column/gap[text|mark]")).is_some());
    }

    /// A title 30 high (15 from Mild), a gap relaxed at 24 with an ideal of
    /// 12 and a least of 4, and a plot from 200 high down to 150.
    fn titled_column() -> Stack<'static, String> {
        let tall =
            |preferred: f64, least: f64| (Size::new(100.0, preferred), Size::new(100.0, least));
        let (title, short) = (tall(30.0, 30.0), tall(15.0, 15.0));
        let plot = tall(200.0, 150.0);
        Stack::column(Id::new("chart"))
            .child(Rc::new(Forms {
                id: Id::new("title"),
                forms: vec![
                    (Level::Preferred, title.0, title.1),
                    (Level::Mild, short.0, short.1),
                ],
                accepts: Accepts::ordinary(),
            }))
            .relaxed_gap(4.0, 12.0, 24.0)
            .child(Rc::new(Forms {
                id: Id::new("plot"),
                forms: vec![(Level::Preferred, plot.0, plot.1)],
                accepts: Accepts::ordinary(),
            }))
    }

    #[test]
    fn a_gap_gives_from_relaxed_to_ideal_by_sharing_and_never_lengthens_as_room_shrinks() {
        let gap = |plan: &Node<String>| bounds_of(plan, "chart/gap[title|plot]").height;
        // With room it takes its relaxed 24.
        let roomy = lay_out(&titled_column(), &all_permitted(), Size::new(100.0, 300.0)).unwrap();
        assert_eq!(gap(&roomy), 24.0);
        // Giving room down to its ideal is the shortage being shared, not a
        // change of form: nothing is ordered up and the gap stays Preferred.
        let shared = lay_out(&titled_column(), &all_permitted(), Size::new(100.0, 240.0)).unwrap();
        assert!(gap(&shared) > 12.0 && gap(&shared) < 24.0);
        assert_eq!(level_of(&shared, "chart/gap[title|plot]"), Level::Preferred);
        assert!(shared.reasons.is_empty());
        // As the room shrinks step by step, the gap never gets longer: once
        // the title is abbreviated, the gap keeps no more than its ideal.
        let mut last = f64::INFINITY;
        let mut height = 260.0;
        while height >= 169.0 {
            let plan = lay_out(&titled_column(), &all_permitted(), Size::new(100.0, height))
                .unwrap_or_else(|refusal| panic!("{height}: {refusal:?}"));
            assert!(
                gap(&plan) <= last + 1e-9,
                "{height}: {} after {last}",
                gap(&plan)
            );
            last = gap(&plan);
            height -= 1.0;
        }
        let compact = lay_out(&titled_column(), &all_permitted(), Size::new(100.0, 190.0)).unwrap();
        assert_eq!(level_of(&compact, "title"), Level::Mild);
        assert_eq!(gap(&compact), 12.0);
        // Below 177 it is squeezed towards its least, flagged at Strong.
        let squeezed =
            lay_out(&titled_column(), &all_permitted(), Size::new(100.0, 170.0)).unwrap();
        assert_eq!(level_of(&squeezed, "chart/gap[title|plot]"), Level::Strong);
        assert!(gap(&squeezed) < 12.0);
    }

    /// A leaf that records a reason of its own.
    struct Reasoned;

    impl LayoutBox<String> for Reasoned {
        fn id(&self) -> &Id {
            static ID: std::sync::OnceLock<Id> = std::sync::OnceLock::new();
            ID.get_or_init(|| Id::new("labels"))
        }
        fn accepts(&self) -> &Accepts {
            static ACCEPTS: std::sync::OnceLock<Accepts> = std::sync::OnceLock::new();
            ACCEPTS.get_or_init(Accepts::ordinary)
        }
        fn measure(&self, _: &Resolved, space: Size) -> Result<Answer, Unsupported> {
            let size = Size::new(10.0, 10.0);
            Ok(Answer::of(self.id(), size, size, space))
        }
        fn arrange(
            &self,
            orders: &Resolved,
            at: Point,
            space: Size,
        ) -> Result<Node<String>, Unsupported> {
            let mut node = leaf_node(self.id(), orders, at, self.measure(orders, space)?);
            node.reasons.push(Reason::own(
                "label set",
                "end labels placed beside the lines",
            ));
            Ok(node)
        }
    }

    #[test]
    fn a_box_can_put_its_own_reasons_in_its_node() {
        let row = Stack::row(Id::new("row")).child(Rc::new(Reasoned));
        let plan = lay_out(&row, &Order::NONE, Size::INFINITE).unwrap();
        assert_eq!(
            plan.find(&Id::new("labels")).unwrap().reasons,
            vec![Reason::Own {
                kind: "label set".to_owned(),
                detail: "end labels placed beside the lines".to_owned()
            }]
        );
    }

    /// A measuring service a text box borrows: it measures only what it is
    /// asked, and counts.
    struct Measurer {
        advance: f64,
        asked: std::cell::Cell<usize>,
    }

    /// A text that borrows the service rather than measuring every line
    /// count when it is made.
    struct Borrowing<'s> {
        id: Id,
        text: &'s str,
        measurer: &'s Measurer,
        accepts: Accepts,
    }

    impl LayoutBox<String> for Borrowing<'_> {
        fn id(&self) -> &Id {
            &self.id
        }
        fn accepts(&self) -> &Accepts {
            &self.accepts
        }
        fn measure(&self, _: &Resolved, space: Size) -> Result<Answer, Unsupported> {
            self.measurer.asked.set(self.measurer.asked.get() + 1);
            let size = Size::new(self.text.len() as f64 * self.measurer.advance, 10.0);
            Ok(Answer::of(&self.id, size, size, space))
        }
        fn arrange(
            &self,
            orders: &Resolved,
            at: Point,
            space: Size,
        ) -> Result<Node<String>, Unsupported> {
            Ok(leaf_node(
                &self.id,
                orders,
                at,
                self.measure(orders, space)?,
            ))
        }
    }

    #[test]
    fn a_composition_can_hold_children_that_borrow() {
        let measurer = Measurer {
            advance: 6.0,
            asked: std::cell::Cell::new(0),
        };
        let text = String::from("Revenue");
        let title: Child<'_, String> = Rc::new(Borrowing {
            id: Id::new("title"),
            text: &text,
            measurer: &measurer,
            accepts: Accepts::ordinary(),
        });
        let column = Stack::column(Id::new("column"))
            .child(title)
            .child(fixed("plot", 100.0, 50.0));
        let plan = lay_out(&column, &Order::NONE, Size::new(200.0, 100.0)).unwrap();
        assert_eq!(bounds_of(&plan, "title"), Rect::new(0.0, 0.0, 42.0, 10.0));
        assert!(measurer.asked.get() > 0);
    }

    #[test]
    fn a_gap_keeps_its_ideal_until_strong() {
        let row = || {
            Stack::row(Id::new("row"))
                .child(fixed("a", 40.0, 10.0))
                .gap(2.0, 10.0)
                .child(fixed("b", 40.0, 10.0))
        };
        let space = Size::new(85.0, 10.0);
        let mild = lay_out(&row(), &Order::NONE.with_level(Level::Mild), space);
        assert!(matches!(mild, Err(Refusal::NoFit { .. })));
        let strong = lay_out(&row(), &Order::NONE.with_level(Level::Strong), space).unwrap();
        assert_eq!(
            strong.find(&Id::new("row/gap[a|b]")).unwrap().bounds.width,
            5.0
        );
        assert!(strong.reasons.contains(&Reason::OrderedUp {
            child: Id::new("row/gap[a|b]"),
            from: Level::Preferred,
            to: Level::Strong,
            showed: Showed::NoRoomForTheLeast {
                needs: 90.0,
                has: 85.0
            },
        }));
    }
}
