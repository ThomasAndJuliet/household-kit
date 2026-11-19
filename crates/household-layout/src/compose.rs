// SPDX-License-Identifier: GPL-3.0-only

//! Rows, columns and lists of immediate children, and a parent's
//! arrangements in its order of preference.
//!
//! A row or column shares a shortage along its direction with
//! [`allocate`], then asks each child for its size across at the room it
//! got. Gaps are children with a range from a least to an ideal, named by
//! what they separate.
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
//! - A row or column works up level by level (A3). For each target from
//!   Mild up to its ceiling it tries the children at their current
//!   levels, and if they do not fit it orders up the first child, in its
//!   own asking order with flexible children last, that is below the
//!   target, whose next level (the next at which its answer changes) is
//!   not past the target, and that the queries show needs it: it does not
//!   fit its own room, or the composition is short along its direction
//!   and its next level takes less room there without leaving it short
//!   across. So every child that needs it reaches Mild before any child
//!   reaches Strong, and a child that fits and takes no room that is short
//!   is never ordered up. A flexible child keeps its preferred length
//!   while the others can still give; only then is it squeezed. A row or
//!   column passes its children only the level.
//! - [`Arrangements`] tries its arrangements in passes (A4): by default up
//!   to Mild, then up to Strong, then up to AllPermitted, each capped at
//!   its own ceiling; within a pass each arrangement starts again from
//!   Preferred, and the first that fits is taken: "compact mildly, change
//!   arrangement, then compact strongly".
//!
//! A composition's answer describes what it would do in the space
//! offered (B1): its preferred size comes from the levels its
//! distribution reaches there, and its least length in each direction is
//! what it reaches when driven as far as its ceiling allows, with the
//! other direction as offered. A composition remembers its answers and
//! its children's, since a query is pure (B5).

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use crate::allocate::{AllocationError, Capacity, allocate};
use crate::boxes::{Answer, Kept, LayoutBox, Node, Reason, Shortfall, Showed};
use crate::geometry::{Direction, FIT_EPSILON, Rect, Size};
use crate::identity::{Id, Source};
use crate::level::Level;
use crate::order::{Accepts, Order, Resolved, Unsupported};

/// A child, shared so that several arrangements can hold the same box.
pub type Child<C> = Rc<dyn LayoutBox<C>>;

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
/// each child is asked once for each order and space.
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

/// Space between children: a range from a least to an ideal length along
/// its composition, and nothing across. Every ideal distance is kept
/// below the level it is squeezed from; from that level a gap may be
/// squeezed to its least (design §5: Strong is "flagged squeezing below
/// an ideal"). The level is the rules' value, Strong unless given.
#[derive(Clone, Debug)]
pub struct Gap {
    id: Id,
    direction: Direction,
    least: f64,
    ideal: f64,
    squeezed_from: Level,
    accepts: Accepts,
}

impl Gap {
    pub fn new(id: Id, direction: Direction, least: f64, ideal: f64) -> Self {
        Self {
            id,
            direction,
            least,
            ideal,
            squeezed_from: Level::Strong,
            accepts: Accepts::ordinary(),
        }
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
            size_in(self.direction, self.ideal, 0.0),
            size_in(self.direction, least, 0.0),
            space,
        ))
    }

    fn arrange(&self, orders: &Resolved, bounds: Rect) -> Result<Node<C>, Unsupported> {
        Ok(Node::new(self.id.clone(), orders.clone(), bounds))
    }
}

/// Where a member sits across its composition's direction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Align {
    Start,
    Centre,
    End,
    /// It takes the whole length across.
    Fill,
}

/// A child of a row, column or list, with what its parent orders of it.
pub struct Member<C> {
    pub child: Child<C>,
    /// The parent's order for this child, in every kind but the level; a
    /// row or column sets the level itself. A list passes every member
    /// what the list was ordered and ignores this.
    pub order: Order,
    /// How readily it shrinks, as in [`Capacity::weight`].
    pub weight: f64,
    /// In a row or column, a flexible child keeps its preferred length
    /// until the others have given what their levels allow, and its own
    /// levels are tried last. A list's members change together, so a list
    /// has no flexible member.
    pub flexible: bool,
    /// Where it sits across; `None` is the composition's default: a row
    /// centres its members ("boxes in a row are centred on one line"), a
    /// column starts them.
    pub align: Option<Align>,
    /// Its share, by weight, of room beyond the preferred lengths. Zero
    /// does not grow.
    pub grow: f64,
}

impl<C> Member<C> {
    pub fn new(child: Child<C>) -> Self {
        Self {
            child,
            order: Order::NONE,
            weight: 1.0,
            flexible: false,
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

    pub fn flexible(mut self) -> Self {
        self.flexible = true;
        self
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

impl<C> Clone for Member<C> {
    fn clone(&self) -> Self {
        Self {
            child: Rc::clone(&self.child),
            order: self.order.clone(),
            weight: self.weight,
            flexible: self.flexible,
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
}

/// What a composition decided under one resolved order in one space.
struct Measured {
    outcome: Outcome,
    answer: Answer,
}

/// A row, a column or a list of immediate children.
pub struct Stack<C> {
    id: Id,
    source: Option<Source>,
    maker: Order,
    accepts: Accepts,
    direction: Direction,
    /// A list: every member takes the list's own order.
    together: bool,
    members: Vec<Member<C>>,
    asked_first: Vec<Id>,
    gaps: Vec<GapAt>,
    squeezed_from: Level,
    on_baselines: bool,
    asked: Asked,
    measured: RefCell<HashMap<(Resolved, SpaceKey), Rc<Measured>>>,
}

impl<C> Stack<C> {
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
            measured: RefCell::default(),
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

    pub fn with_source(mut self, source: Source) -> Self {
        self.source = Some(source);
        self
    }

    /// What this composition's maker asks of it. Its level can only lower
    /// the ceiling ordered from above.
    pub fn with_maker(mut self, maker: Order) -> Self {
        self.maker = maker;
        self
    }

    /// The presentations this composition accepts. A list passes what it
    /// was ordered on to every member, so each member must accept it too.
    pub fn accepting(mut self, accepts: Accepts) -> Self {
        self.accepts = accepts;
        self
    }

    /// The parent's short list of who is asked first. Children not on it
    /// are asked after, in their order in the composition, and flexible
    /// children after all the rest.
    pub fn asked_first(mut self, ids: impl IntoIterator<Item = Id>) -> Self {
        self.asked_first = ids.into_iter().collect();
        self
    }

    /// A row whose members that have a baseline sit on one shared
    /// baseline, which the row reports. Members without one keep their
    /// own alignment. A column ignores this.
    pub fn on_baselines(mut self) -> Self {
        self.on_baselines = true;
        self
    }

    pub fn direction(&self) -> Direction {
        self.direction
    }

    pub fn members(&self) -> &[Member<C>] {
        &self.members
    }
}

impl<C: 'static> Stack<C> {
    pub fn child(self, child: Child<C>) -> Self {
        self.member(Member::new(child))
    }

    pub fn member(mut self, member: Member<C>) -> Self {
        self.members.push(member);
        self.name_gaps();
        self
    }

    /// A gap from `least` to `ideal` along this composition, as the next
    /// child. It is named by the children it separates.
    pub fn gap(mut self, least: f64, ideal: f64) -> Self {
        let index = self.members.len();
        let placeholder: Child<C> =
            Rc::new(Gap::new(self.id.child("gap"), self.direction, least, ideal));
        self.members.push(Member::new(placeholder));
        self.gaps.push(GapAt {
            index,
            least,
            ideal,
        });
        self.name_gaps();
        self
    }

    /// The level from which this composition's gaps may go below their
    /// ideals: the rules' value (Strong unless given).
    pub fn squeezing_from(mut self, level: Level) -> Self {
        self.squeezed_from = level;
        self.name_gaps();
        self
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
                .squeezed_from(self.squeezed_from);
            self.members[gap.index].child = Rc::new(made);
        }
    }
}

/// The children's answers under one assignment of levels.
struct Trial {
    orders: Vec<Order>,
    /// Answers in the composition's whole space: the lengths along.
    full: Vec<Answer>,
    /// Each child's room along: what the allocation gave it or, when the
    /// floors do not fit, its floor (its least length, or its preferred
    /// length while it is a protected flexible child).
    lengths: Vec<f64>,
    /// Answers at each child's room. While the floors do not fit there is
    /// no room to speak of, and a child given at least its preferred
    /// length answers as it does with room to spare: these are its answers
    /// in the whole space.
    rooms: Vec<Answer>,
    /// The room each child was asked at, when it was asked at one.
    asked_at: Vec<Option<Size>>,
    /// What the floors need and the room has, when they do not fit.
    along_short: Option<(f64, f64)>,
    /// What the least lengths alone need and the room has, when even
    /// they do not fit.
    least_short: Option<(f64, f64)>,
}

impl Trial {
    fn fits(&self) -> bool {
        self.along_short.is_none() && self.rooms.iter().all(Answer::fits)
    }
}

/// The distribution's result.
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

impl<C> Stack<C> {
    fn across(&self) -> Direction {
        self.direction.other()
    }

    fn room(&self, space: Size, along: f64) -> Size {
        size_in(self.direction, along, self.across().length(space))
    }

    /// The space a child is asked in for its lengths along: unlimited
    /// along, since a preferred length is the length with room to spare
    /// and a least length the least it can reach, and as offered across.
    /// Asking every child there, whatever the length along offered, keeps
    /// the spaces a tree is asked in few (semantics B5).
    fn whole(&self, space: Size) -> Size {
        self.room(space, f64::INFINITY)
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
    /// rest in their order, with flexible children after all of them.
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
        let (mut fixed, flexible): (Vec<usize>, Vec<usize>) =
            order.into_iter().partition(|i| !self.members[*i].flexible);
        fixed.extend(flexible);
        fixed
    }

    fn trial(
        &self,
        levels: &[Level],
        own: &Resolved,
        space: Size,
        protect: bool,
    ) -> Result<Trial, Unsupported> {
        let d = self.direction;
        let n = self.members.len();
        let orders: Vec<Order> = (0..n).map(|i| self.order_for(i, levels[i], own)).collect();
        let full = (0..n)
            .map(|i| self.ask(i, levels[i], own, self.whole(space)))
            .collect::<Result<Vec<_>, _>>()?;
        let available = d.length(space);
        let protected = |i: usize| protect && self.members[i].flexible;
        let floors: Vec<f64> = full
            .iter()
            .enumerate()
            .map(|(i, answer)| {
                if protected(i) {
                    preferred_in(d, answer)
                } else {
                    d.length(answer.least)
                }
            })
            .collect();
        let least_total: f64 = full.iter().map(|answer| d.length(answer.least)).sum();
        let least_short =
            (least_total > available + FIT_EPSILON).then_some((least_total, available));
        let (lengths, along_short) = if available.is_finite() {
            let capacities: Vec<Capacity> = full
                .iter()
                .enumerate()
                .map(|(i, answer)| {
                    let weight = if protected(i) {
                        0.0
                    } else {
                        self.members[i].weight
                    };
                    Capacity::new(preferred_in(d, answer), d.length(answer.least), weight)
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
            (
                full.iter().map(|answer| preferred_in(d, answer)).collect(),
                None,
            )
        };
        let asked_at: Vec<Option<Size>> = (0..n)
            .map(|i| {
                (along_short.is_none() && lengths[i] < preferred_in(d, &full[i]) - FIT_EPSILON)
                    .then(|| self.room(space, lengths[i]))
            })
            .collect();
        let rooms = (0..n)
            .map(|i| match asked_at[i] {
                Some(room) => self.ask(i, levels[i], own, room),
                None => Ok(full[i].clone()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Trial {
            orders,
            full,
            lengths,
            rooms,
            asked_at,
            along_short,
            least_short,
        })
    }

    /// The next level above `from`, up to the ceiling, at which child
    /// `i`'s answer in the whole space or at its room changes: a box need
    /// not have a form at every level.
    fn next_changing(
        &self,
        i: usize,
        from: Level,
        own: &Resolved,
        space: Size,
        room: Option<Size>,
    ) -> Result<Option<Level>, Unsupported> {
        let full = self.ask(i, from, own, self.whole(space))?;
        let at_room = room.map(|room| self.ask(i, from, own, room)).transpose()?;
        let mut level = from;
        while let Some(next) = level.next().filter(|next| *next <= own.level) {
            if self.ask(i, next, own, self.whole(space))? != full
                || room.map(|room| self.ask(i, next, own, room)).transpose()? != at_room
            {
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

    /// Whether child `i` at `next` takes less room along than at `from`
    /// without falling short across, in the whole space, where it did not.
    fn gives_along(
        &self,
        i: usize,
        from: Level,
        next: Level,
        own: &Resolved,
        space: Size,
    ) -> Result<bool, Unsupported> {
        let d = self.direction;
        let current = d.length(self.ask(i, from, own, self.whole(space))?.least);
        let least = d.length(self.ask(i, next, own, self.whole(space))?.least);
        if least >= current - FIT_EPSILON {
            return Ok(false);
        }
        let across = self.across();
        let was_short = short_in(across, &self.ask(i, from, own, self.whole(space))?);
        let is_short = short_in(across, &self.ask(i, next, own, self.whole(space))?);
        Ok(was_short || !is_short)
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
        let flexible = self
            .members
            .iter()
            .find(|member| member.flexible)
            .map(|member| member.child.id().clone());
        for i in self.asking_order() {
            if levels[i] >= target {
                continue;
            }
            let member = &self.members[i];
            let room_short = !trial.rooms[i].fits();
            let along_short = if member.flexible {
                trial.least_short.is_some()
            } else {
                trial.along_short.is_some()
            };
            if !room_short && !along_short {
                continue;
            }
            let showed = if room_short {
                Showed::DoesNotFitItsRoom {
                    short: trial.rooms[i].short.clone(),
                }
            } else {
                match (trial.least_short, &flexible, trial.along_short) {
                    (None, Some(flexible), Some((needs, has))) => Showed::TakesRoomFlexibleLacks {
                        flexible: flexible.clone(),
                        lacks: needs - has,
                    },
                    (Some((needs, has)), _, _) | (None, _, Some((needs, has))) => {
                        Showed::NoRoomForTheLeast { needs, has }
                    }
                    (None, _, None) => {
                        unreachable!("a child is asked only when something is short")
                    }
                }
            };
            let child = member.child.id().clone();
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
            let helps = (room_short
                && self.fits_better(
                    i,
                    levels[i],
                    next,
                    own,
                    room.unwrap_or_else(|| self.whole(space)),
                )?)
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

    fn distribute(&self, own: &Resolved, space: Size) -> Result<Outcome, Unsupported> {
        let n = self.members.len();
        if self.together {
            let levels = vec![own.level; n];
            let trial = self.trial(&levels, own, space, false)?;
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
        for target in Level::ALL.into_iter().filter(|level| *level <= own.level) {
            loop {
                let trial = self.trial(&levels, own, space, true)?;
                if trial.fits() {
                    return Ok(Outcome {
                        levels,
                        trial,
                        reasons,
                        short: Vec::new(),
                    });
                }
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
                levels[i] = next;
            }
        }
        // Nothing more can be ordered up: the flexible children give room
        // too.
        let trial = self.trial(&levels, own, space, false)?;
        if trial.along_short.is_none() {
            for (i, member) in self.members.iter().enumerate() {
                let preferred = preferred_in(self.direction, &trial.full[i]);
                if member.flexible && trial.lengths[i] < preferred - FIT_EPSILON {
                    reasons.push(Reason::FlexibleSqueezed {
                        child: member.child.id().clone(),
                        preferred,
                        got: trial.lengths[i],
                    });
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

    /// The least length along: each child driven from Preferred as far as
    /// the ceiling allows, while its next level takes less room along
    /// without falling short across, or falls less short across.
    fn least_along(&self, own: &Resolved, space: Size) -> Result<f64, Unsupported> {
        let d = self.direction;
        let across = self.across();
        let mut total = 0.0;
        for i in 0..self.members.len() {
            let whole = self.whole(space);
            let mut level = Level::Preferred;
            while let Some(next) = self.next_changing(i, level, own, space, None)? {
                let short_across = short_in(across, &self.ask(i, level, own, whole)?);
                let helps = (short_across && self.fits_better(i, level, next, own, whole)?)
                    || self.gives_along(i, level, next, own, space)?;
                if !helps {
                    break;
                }
                level = next;
            }
            total += d.length(self.ask(i, level, own, self.whole(space))?.least);
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

    /// The least length across: from the levels reached, each child
    /// driven while its next level is shorter across at its room and the
    /// least lengths along still fit.
    fn least_across(
        &self,
        own: &Resolved,
        space: Size,
        outcome: &Outcome,
    ) -> Result<f64, Unsupported> {
        let d = self.direction;
        let across = self.across();
        let trial = &outcome.trial;
        let mut levels = outcome.levels.clone();
        let mut along: Vec<f64> = trial
            .full
            .iter()
            .map(|answer| d.length(answer.least))
            .collect();
        let mut answers = trial.rooms.clone();
        for i in self.asking_order() {
            let room = trial.asked_at[i];
            let at = room.unwrap_or_else(|| self.whole(space));
            while let Some(next) = self.next_changing(i, levels[i], own, space, room)? {
                let at_next = self.ask(i, next, own, at)?;
                let narrower =
                    across.length(at_next.least) < across.length(answers[i].least) - FIT_EPSILON;
                let along_next = d.length(self.ask(i, next, own, self.whole(space))?.least);
                let total: f64 = along.iter().sum::<f64>() - along[i] + along_next;
                if !narrower || total > d.length(space) + FIT_EPSILON {
                    break;
                }
                levels[i] = next;
                along[i] = along_next;
                answers[i] = at_next;
            }
        }
        Ok(self
            .extent(&answers, |answer| across.length(answer.least))
            .0)
    }

    fn measured(&self, own: &Resolved, space: Size) -> Result<Rc<Measured>, Unsupported> {
        let key = (own.clone(), SpaceKey::from(space));
        if let Some(measured) = self.measured.borrow().get(&key) {
            return Ok(Rc::clone(measured));
        }
        let outcome = self.distribute(own, space)?;
        let answer = self.answer(own, space, &outcome)?;
        let measured = Rc::new(Measured { outcome, answer });
        self.measured.borrow_mut().insert(key, Rc::clone(&measured));
        Ok(measured)
    }

    fn answer(
        &self,
        own: &Resolved,
        space: Size,
        outcome: &Outcome,
    ) -> Result<Answer, Unsupported> {
        let d = self.direction;
        let across = self.across();
        let trial = &outcome.trial;
        let available = d.length(space);
        let mut preferred_along: f64 = trial
            .full
            .iter()
            .map(|answer| preferred_in(d, answer))
            .sum();
        if available.is_finite() && self.members.iter().any(|member| member.grow > 0.0) {
            preferred_along = preferred_along.max(available);
        }
        let (least_along, least_across) = if self.together {
            (
                trial.full.iter().map(|answer| d.length(answer.least)).sum(),
                self.extent(&trial.rooms, |answer| across.length(answer.least))
                    .0,
            )
        } else {
            (
                self.least_along(own, space)?,
                self.least_across(own, space, outcome)?,
            )
        };
        let offered_across = across.length(space);
        let (preferred_across, shared) =
            self.extent(&trial.rooms, |answer| answer.taken(across, offered_across));
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

impl<C> LayoutBox<C> for Stack<C> {
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
        Ok(self.measured(orders, space)?.answer.clone())
    }

    fn arrange(&self, orders: &Resolved, bounds: Rect) -> Result<Node<C>, Unsupported> {
        let space = bounds.size();
        let measured = self.measured(orders, space)?;
        let outcome = &measured.outcome;
        let trial = &outcome.trial;
        let d = self.direction;
        let across = self.across();
        let cross = across.length(space);
        let mut lengths = trial.lengths.clone();
        let available = d.length(space);
        let growing: f64 = self.members.iter().map(|member| member.grow).sum();
        if trial.along_short.is_none() && available.is_finite() && growing > 0.0 {
            let surplus = available - lengths.iter().sum::<f64>();
            if surplus > FIT_EPSILON {
                for (length, member) in lengths.iter_mut().zip(&self.members) {
                    *length += surplus * member.grow / growing;
                }
            }
        }
        let answers = (0..self.members.len())
            .map(|i| {
                if lengths[i] == trial.lengths[i] {
                    Ok(trial.rooms[i].clone())
                } else {
                    self.ask(i, outcome.levels[i], orders, self.room(space, lengths[i]))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (_, shared) = self.extent(&answers, |answer| answer.taken(across, cross));
        let mut node =
            Node::new(self.id.clone(), orders.clone(), bounds).with_source(self.source.clone());
        let mut offset = 0.0;
        let mut baseline = shared;
        for (i, member) in self.members.iter().enumerate() {
            let answer = &answers[i];
            let align = self.align_of(i);
            let length_across = match align {
                Align::Fill => cross,
                _ => answer.taken(across, cross),
            };
            let at = match (shared, answer.baseline) {
                (Some(shared), Some(own)) if align != Align::Fill => shared - own,
                _ => match align {
                    Align::Start | Align::Fill => 0.0,
                    Align::Centre => ((cross - length_across) / 2.0).max(0.0),
                    Align::End => (cross - length_across).max(0.0),
                },
            };
            let size = size_in(d, lengths[i], length_across);
            let (x, y) = match d {
                Direction::Across => (bounds.x + offset, bounds.y + at),
                Direction::Down => (bounds.x + at, bounds.y + offset),
            };
            node.children.push(
                member
                    .child
                    .order(&trial.orders[i], Rect::new(x, y, size.width, size.height))?,
            );
            if d == Direction::Down && baseline.is_none() {
                baseline = answer.baseline.map(|own| offset + own);
            }
            offset += lengths[i];
        }
        debug_assert_eq!(outcome.levels.len(), node.children.len());
        node.baseline = baseline;
        node.reasons = outcome.reasons.clone();
        Ok(node)
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
    /// The arrangement whose least size the box reports: the narrowest
    /// whose least height fits, at the level it was given.
    narrowest: (usize, Level),
}

/// A parent's arrangements of its children, in its order of preference,
/// tried in passes (semantics A4). Within a pass each arrangement is
/// given, from Preferred, each level up to the pass's ceiling, and the
/// first that fits is taken: "compact mildly, change arrangement, then
/// compact strongly". An arrangement can be tagged with the value of a
/// family kind it stands for (`direction` = `turned`); an order naming
/// that kind restricts the box to the arrangements so tagged. The chosen
/// arrangement is passed everything the box was ordered.
pub struct Arrangements<C> {
    id: Id,
    source: Option<Source>,
    maker: Order,
    accepts: Accepts,
    options: Vec<Child<C>>,
    tags: Vec<BTreeMap<String, String>>,
    passes: Vec<Level>,
    asked: Asked,
}

impl<C> Arrangements<C> {
    pub fn new(id: Id, options: Vec<Child<C>>) -> Self {
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
        }
    }

    pub fn with_source(mut self, source: Source) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_maker(mut self, maker: Order) -> Self {
        self.maker = maker;
        self
    }

    pub fn accepting(mut self, accepts: Accepts) -> Self {
        self.accepts = accepts;
        self
    }

    /// The passes, as the ceiling of each in turn; each is capped at the
    /// box's own ceiling.
    pub fn with_passes(mut self, passes: impl IntoIterator<Item = Level>) -> Self {
        self.passes = passes.into_iter().collect();
        self
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
        self
    }

    /// The arrangements the order permits, in order of preference.
    fn eligible(&self, own: &Resolved) -> Vec<usize> {
        (0..self.options.len())
            .filter(|k| {
                own.ordered.added.iter().all(|(kind, value)| {
                    !self.tags.iter().any(|tags| tags.contains_key(kind))
                        || self.tags[*k].get(kind) == Some(value)
                })
            })
            .collect()
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

    fn decide(&self, own: &Resolved, space: Size) -> Result<Decision, Unsupported> {
        let eligible = self.eligible(own);
        if eligible.is_empty() {
            return Err(Unsupported {
                part: self.id.clone(),
                requested: own.presentation.clone(),
            });
        }
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
        let reach = self
            .passes
            .iter()
            .map(|pass| (*pass).min(own.level))
            .max()
            .unwrap_or(Level::Preferred);
        let mut candidates = Vec::new();
        for &k in &eligible {
            let lowest = if self.options[k].takes_level_exactly() {
                Level::Preferred
            } else {
                reach
            };
            for level in Level::ALL
                .into_iter()
                .filter(|level| (lowest..=reach).contains(level))
            {
                candidates.push((k, level, self.ask(k, level, own, space)?.least));
            }
        }
        let height_fits = |least: &Size| least.height <= space.height + FIT_EPSILON;
        let any_fits = candidates.iter().any(|(_, _, least)| height_fits(least));
        let narrowest = candidates
            .into_iter()
            .filter(|(_, _, least)| !any_fits || height_fits(least))
            .reduce(|best, next| {
                let narrower = next.2.width < best.2.width - FIT_EPSILON
                    || ((next.2.width - best.2.width).abs() <= FIT_EPSILON
                        && next.2.height < best.2.height - FIT_EPSILON);
                if narrower { next } else { best }
            })
            .map(|(k, level, _)| (k, level))
            .expect("at least one arrangement is eligible");
        Ok(Decision {
            rejected,
            chosen,
            narrowest,
        })
    }
}

impl<C> LayoutBox<C> for Arrangements<C> {
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

    /// The chosen arrangement's answer, with the least size of the one
    /// arrangement that can be narrowest (B2: a least width and a least
    /// height that belong to one arrangement). When none fits, the
    /// narrowest's answer, since it is the parent's last resort.
    fn measure(&self, orders: &Resolved, space: Size) -> Result<Answer, Unsupported> {
        let decision = self.decide(orders, space)?;
        let (k, level) = decision.narrowest;
        let narrowest = self.ask(k, level, orders, space)?;
        Ok(match decision.chosen {
            Some((k, level)) => Answer {
                least: narrowest.least,
                ..self.ask(k, level, orders, space)?
            },
            None => narrowest,
        })
    }

    fn arrange(&self, orders: &Resolved, bounds: Rect) -> Result<Node<C>, Unsupported> {
        let decision = self.decide(orders, bounds.size())?;
        let mut node =
            Node::new(self.id.clone(), orders.clone(), bounds).with_source(self.source.clone());
        for (k, answer) in &decision.rejected {
            node.reasons.push(Reason::ArrangementRejected {
                arrangement: self.options[*k].id().clone(),
                short: answer.short.clone(),
            });
        }
        let (k, level) = match decision.chosen {
            Some(chosen) => {
                node.reasons.push(Reason::ArrangementTaken {
                    arrangement: self.options[chosen.0].id().clone(),
                });
                chosen
            }
            None => decision.narrowest,
        };
        let size = self
            .ask(k, level, orders, bounds.size())?
            .taken_size(bounds.size());
        let rect = Rect::new(bounds.x, bounds.y, size.width, size.height);
        node.children
            .push(self.options[k].order(&Self::order_at(orders, level), rect)?);
        Ok(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boxes::{Refusal, plan as lay_out};
    use crate::order::{Presentation, Prominence};

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
        fn arrange(&self, orders: &Resolved, bounds: Rect) -> Result<Node<String>, Unsupported> {
            Ok(Node::leaf(
                self.id.clone(),
                orders.clone(),
                bounds,
                self.id.as_str().to_owned(),
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

    /// A box with a continuous range and, from Mild, a narrower one.
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

    /// Text that wraps: its height grows as its width shrinks.
    struct Wrapping {
        id: Id,
        natural: f64,
        least: f64,
        line: f64,
        accepts: Accepts,
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
            let height = (self.natural / width).ceil() * self.line;
            Ok(Answer::of(
                &self.id,
                Size::new(self.natural, height),
                Size::new(self.least, height),
                space,
            ))
        }
        fn arrange(&self, orders: &Resolved, bounds: Rect) -> Result<Node<String>, Unsupported> {
            Ok(Node::leaf(
                self.id.clone(),
                orders.clone(),
                bounds,
                self.id.as_str().to_owned(),
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

    fn all_permitted() -> Order {
        Order::NONE.with_level(Level::AllPermitted)
    }

    fn level_of(plan: &Node<String>, id: &str) -> Level {
        plan.find(&Id::new(id)).expect(id).orders.level
    }

    fn bounds_of(plan: &Node<String>, id: &str) -> Rect {
        plan.find(&Id::new(id)).expect(id).bounds
    }

    /// Three buttons that go from 50 to 20 wide at Mild, as one list.
    fn actions() -> Rc<Stack<String>> {
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
                // The plan arranges the row at the 160 it takes, and the
                // reasons are the arrangement's.
                showed: Showed::NoRoomForTheLeast {
                    needs: 250.0,
                    has: 160.0
                },
            }]
        );
        assert_eq!(plan.bounds.width, 160.0);
        // Asked in their order, the title gives first, and 60 + 150 is
        // still too wide, so the actions give too.
        let plan = lay_out(&toolbar(vec![]), &all_permitted(), Size::new(200.0, 20.0)).unwrap();
        assert_eq!(level_of(&plan, "title"), Level::Mild);
        assert_eq!(level_of(&plan, "button1"), Level::Mild);
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
        // Review finding 3: the title is unchanged at Mild and 60 at
        // Strong; the actions are 150 and 60 at Mild. In 200 the actions
        // at Mild alone fit, so the title is not touched, although it is
        // asked first.
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
    fn a_flexible_childs_own_levels_are_tried_after_the_others() {
        // Review finding 4: fixed 50 beside a flexible plot whose least is
        // 150 at Preferred and 80 at Mild, in 180: 50 + 80 fits.
        let row = Stack::row(Id::new("chart"))
            .child(fixed("fixed", 50.0, 20.0))
            .member(
                Member::new(ranged(
                    "plot",
                    vec![(Level::Preferred, 150.0, 150.0), (Level::Mild, 150.0, 80.0)],
                    20.0,
                ))
                .flexible(),
            );
        let plan = lay_out(&row, &all_permitted(), Size::new(180.0, 20.0)).unwrap();
        assert_eq!(level_of(&plan, "plot"), Level::Mild);
        assert_eq!(bounds_of(&plan, "plot").width, 130.0);
        assert!(plan.reasons.contains(&Reason::OrderedUp {
            child: Id::new("plot"),
            from: Level::Preferred,
            to: Level::Mild,
            showed: Showed::NoRoomForTheLeast {
                needs: 200.0,
                has: 180.0
            },
        }));
        assert!(plan.reasons.contains(&Reason::FlexibleSqueezed {
            child: Id::new("plot"),
            preferred: 150.0,
            got: 130.0
        }));
    }

    #[test]
    fn several_flexible_children_keep_their_preferred_lengths_and_then_share_the_squeeze() {
        let row = || {
            let plot = |id: &str| {
                Member::new(ranged(id, vec![(Level::Preferred, 100.0, 40.0)], 20.0)).flexible()
            };
            Stack::row(Id::new("chart"))
                .member(plot("one"))
                .child(ranged(
                    "label",
                    vec![(Level::Preferred, 60.0, 50.0), (Level::Mild, 30.0, 30.0)],
                    10.0,
                ))
                .member(plot("two"))
        };
        let width = |plan: &Node<String>, id: &str| bounds_of(plan, id).width;
        let plan = lay_out(&row(), &all_permitted(), Size::new(230.0, 20.0)).unwrap();
        assert_eq!(level_of(&plan, "label"), Level::Mild);
        assert_eq!((width(&plan, "one"), width(&plan, "two")), (100.0, 100.0));
        let plan = lay_out(&row(), &all_permitted(), Size::new(200.0, 20.0)).unwrap();
        assert_eq!((width(&plan, "one"), width(&plan, "two")), (85.0, 85.0));
        for id in ["one", "two"] {
            assert!(plan.reasons.contains(&Reason::FlexibleSqueezed {
                child: Id::new(id),
                preferred: 100.0,
                got: 85.0
            }));
        }
    }

    #[test]
    fn a_child_is_not_ordered_into_a_form_that_falls_short_across() {
        // Review finding 4, second half: `a` is narrower at Mild but too
        // tall for the row. It stays, and `b` gives at Strong instead.
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
        // Review finding 1: a list under arrangements, ordered AllPermitted
        // with unlimited room, keeps every member at Preferred; so does a
        // list at the root of a plan.
        let list: Child<String> = actions();
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
    fn row_and_column() -> (Child<String>, Child<String>) {
        let a: Child<String> = fixed("a", 40.0, 10.0);
        let b: Child<String> = fixed("b", 40.0, 10.0);
        let row: Child<String> = Rc::new(
            Stack::row(Id::new("row"))
                .child(Rc::clone(&a))
                .gap(2.0, 10.0)
                .child(Rc::clone(&b)),
        );
        let column: Child<String> = Rc::new(
            Stack::column(Id::new("column"))
                .child(a)
                .gap(2.0, 10.0)
                .child(b),
        );
        (row, column)
    }

    #[test]
    fn a_column_that_fits_mildly_is_taken_over_a_row_squeezed_at_strong() {
        // Review finding 2: in 85 the row fits only with its gap squeezed
        // at Strong; the column fits at Preferred.
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

    #[test]
    fn an_order_naming_a_tagged_kind_restricts_the_arrangements_and_reaches_the_chosen() {
        // Review finding 2: "direction = turned" reaches the chart.
        let upright = Presentation::default().with("direction", "upright");
        let turned = Presentation::default().with("direction", "turned");
        let both = || Accepts::new(upright.clone(), vec![upright.clone(), turned.clone()]);
        let (a, b): (Child<String>, Child<String>) =
            (fixed("a", 40.0, 10.0), fixed("b", 40.0, 10.0));
        let across: Child<String> = Rc::new(
            Stack::row(Id::new("across"))
                .accepting(both())
                .child(Rc::clone(&a))
                .child(Rc::clone(&b)),
        );
        let up: Child<String> = Rc::new(
            Stack::column(Id::new("up"))
                .accepting(both())
                .child(a)
                .child(b),
        );
        let chart = Arrangements::new(Id::new("chart"), vec![across, up])
            .accepting(both())
            .tagged(&Id::new("across"), "direction", "upright")
            .tagged(&Id::new("up"), "direction", "turned");
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

    /// A column of `w` (120 by 20, and 80 by 30 from Mild) and `f` (50 by
    /// 20).
    fn w_over_f() -> Stack<String> {
        Stack::column(Id::new("inner"))
            .child(Rc::new(Forms {
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
            }))
            .child(fixed("f", 50.0, 20.0))
    }

    #[test]
    fn a_composition_answers_what_it_would_do_in_the_space_offered() {
        // Review finding 5: offered width 100, the column needs 50, not 40.
        let answer = w_over_f()
            .query(&all_permitted(), Size::new(100.0, 100.0))
            .unwrap();
        assert_eq!(answer.preferred.height, 50.0);
        assert_eq!(answer.least.height, 50.0);
        // In a parent, the answer of 40 ended in a false no fit.
        let outer = Stack::column(Id::new("outer"))
            .child(Rc::new(w_over_f()))
            .child(fixed("y", 50.0, 10.0));
        let plan = lay_out(&outer, &all_permitted(), Size::new(100.0, 60.0)).unwrap();
        assert_eq!(bounds_of(&plan, "y").y, 50.0);
        assert_eq!(level_of(&plan, "w"), Level::Mild);
    }

    /// Every node's identity and level.
    fn choices(plan: &Node<String>) -> Vec<(Id, Level)> {
        plan.nodes()
            .into_iter()
            .map(|node| (node.id.clone(), node.orders.level))
            .collect()
    }

    #[test]
    fn arranging_at_the_answered_size_makes_the_querys_choices() {
        let (row, column) = row_and_column();
        let boxes: Vec<(Child<String>, Size)> = vec![
            (Rc::new(w_over_f()), Size::new(100.0, 100.0)),
            (
                Rc::new(Arrangements::new(Id::new("parent"), vec![row, column])),
                Size::new(85.0, 40.0),
            ),
            (
                Rc::new(
                    Stack::row(Id::new("toolbar"))
                        .child(compact("title", 100.0, 60.0, 20.0))
                        .child(actions()),
                ),
                Size::new(170.0, 30.0),
            ),
        ];
        for (root, space) in boxes {
            let plan = lay_out(root.as_ref(), &all_permitted(), space).unwrap();
            let at = Order::NONE.with_level(plan.orders.level);
            let in_space = root
                .order(&at, Rect::from_origin(Default::default(), space))
                .unwrap();
            assert_eq!(choices(&plan), choices(&in_space), "{}", root.id().as_str());
        }
    }

    #[test]
    fn a_box_offered_its_least_size_fits_and_offered_less_does_not() {
        let fits = |root: &dyn LayoutBox<String>, space| {
            root.query(&all_permitted(), space).unwrap().fits()
        };
        // Along a column, at the width offered.
        let column = w_over_f();
        let least = column
            .query(&all_permitted(), Size::new(100.0, 100.0))
            .unwrap()
            .least;
        assert!(fits(&column, Size::new(100.0, least.height)));
        assert!(!fits(&column, Size::new(100.0, least.height - 0.5)));
        // Across the column: W can be 80 wide.
        assert_eq!(least.width, 80.0);
        assert!(fits(&column, Size::new(least.width, 100.0)));
        assert!(!fits(&column, Size::new(least.width - 0.5, 100.0)));
        // Along a row with a flexible child (review finding 4's row).
        let row = Stack::row(Id::new("chart"))
            .child(fixed("fixed", 50.0, 20.0))
            .member(
                Member::new(ranged(
                    "plot",
                    vec![(Level::Preferred, 150.0, 150.0), (Level::Mild, 150.0, 80.0)],
                    20.0,
                ))
                .flexible(),
            );
        let least = row
            .query(&all_permitted(), Size::new(400.0, 20.0))
            .unwrap()
            .least;
        assert_eq!(least, Size::new(130.0, 20.0));
        assert!(fits(&row, least));
        assert!(!fits(&row, Size::new(least.width - 0.5, 20.0)));
        assert!(!fits(&row, Size::new(least.width, 19.5)));
    }

    #[test]
    fn arrangements_report_a_least_size_of_one_arrangement() {
        let (a, b): (Child<String>, Child<String>) =
            (fixed("a", 100.0, 20.0), fixed("b", 100.0, 20.0));
        let row: Child<String> = Rc::new(
            Stack::row(Id::new("row"))
                .child(Rc::clone(&a))
                .child(Rc::clone(&b)),
        );
        let column: Child<String> = Rc::new(Stack::column(Id::new("column")).child(a).child(b));
        let parent = Arrangements::new(Id::new("parent"), vec![row, column]);
        let answer = parent
            .query(&all_permitted(), Size::new(300.0, 50.0))
            .unwrap();
        assert_eq!(answer.preferred, Size::new(200.0, 20.0));
        assert_eq!(answer.least, Size::new(100.0, 40.0));
        assert!(parent.query(&all_permitted(), answer.least).unwrap().fits());
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
        fn arrange(&self, orders: &Resolved, bounds: Rect) -> Result<Node<String>, Unsupported> {
            Ok(Node::leaf(
                self.id.clone(),
                orders.clone(),
                bounds,
                self.id.as_str().to_owned(),
            )
            .with_baseline(Some(self.baseline)))
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
        let ids = |stack: &Stack<String>| -> Vec<String> {
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
        fn arrange(&self, orders: &Resolved, bounds: Rect) -> Result<Node<String>, Unsupported> {
            self.inner.arrange(orders, bounds)
        }
    }

    /// Rows and columns in turn, three children each, `depth` deep.
    fn tree(depth: usize, path: &str, count: &Rc<std::cell::Cell<usize>>) -> Child<String> {
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
    fn the_flexible_child_keeps_its_preferred_size_until_the_others_have_given_theirs() {
        let row = || {
            Stack::row(Id::new("chart"))
                .member(
                    Member::new(ranged("plot", vec![(Level::Preferred, 100.0, 40.0)], 20.0))
                        .flexible(),
                )
                .child(ranged(
                    "label",
                    vec![(Level::Preferred, 60.0, 50.0), (Level::Mild, 30.0, 30.0)],
                    10.0,
                ))
        };
        let width = |plan: &Node<String>, id: &str| plan.find(&Id::new(id)).unwrap().bounds.width;

        // The label gives what its level allows before anything else.
        let wide = lay_out(&row(), &all_permitted(), Size::new(150.0, 20.0)).unwrap();
        assert_eq!((width(&wide, "plot"), width(&wide, "label")), (100.0, 50.0));
        assert_eq!(level_of(&wide, "label"), Level::Preferred);
        assert!(wide.reasons.is_empty());

        // Then it is ordered up, and the plot still has its preferred width.
        let middle = lay_out(&row(), &all_permitted(), Size::new(130.0, 20.0)).unwrap();
        assert_eq!(
            (width(&middle, "plot"), width(&middle, "label")),
            (100.0, 30.0)
        );
        assert_eq!(
            middle.reasons,
            vec![Reason::OrderedUp {
                child: Id::new("label"),
                from: Level::Preferred,
                to: Level::Mild,
                showed: Showed::TakesRoomFlexibleLacks {
                    flexible: Id::new("plot"),
                    lacks: 20.0
                },
            }]
        );

        // Only when nothing more can be ordered up does the plot shrink.
        let narrow = lay_out(&row(), &all_permitted(), Size::new(100.0, 20.0)).unwrap();
        assert_eq!(
            (width(&narrow, "plot"), width(&narrow, "label")),
            (70.0, 30.0)
        );
        assert!(narrow.reasons.contains(&Reason::FlexibleSqueezed {
            child: Id::new("plot"),
            preferred: 100.0,
            got: 70.0
        }));
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
        // The row takes 80 of the 85, and is arranged there.
        let showed = Showed::NoRoomForTheLeast {
            needs: 110.0,
            has: 80.0,
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
        let answer = wider
            .query(&all_permitted(), Size::new(50.0, 20.0))
            .unwrap();
        assert!(!answer.fits());
    }

    #[test]
    fn each_arrangement_starts_from_preferred_and_the_first_that_fits_wins() {
        let a: Child<String> = compact("a", 80.0, 60.0, 20.0);
        let b: Child<String> = fixed("b", 80.0, 20.0);
        let row: Child<String> = Rc::new(
            Stack::row(Id::new("row"))
                .child(Rc::clone(&a))
                .child(Rc::clone(&b)),
        );
        let column: Child<String> = Rc::new(Stack::column(Id::new("column")).child(a).child(b));
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
        // Too low: the text, at the 70 it gets, wraps to two lines.
        assert_eq!(
            lay_out(&row, &all_permitted(), Size::new(100.0, 15.0)),
            Err(Refusal::NoFit {
                short: vec![Shortfall {
                    part: Id::new("text"),
                    direction: Direction::Down,
                    needs: 20.0,
                    has: 15.0
                }]
            })
        );
        // Too narrow: the row names its own least width.
        assert_eq!(
            lay_out(&row, &all_permitted(), Size::new(50.0, 100.0)),
            Err(Refusal::NoFit {
                short: vec![Shortfall {
                    part: Id::new("row"),
                    direction: Direction::Across,
                    needs: 70.0,
                    has: 50.0
                }]
            })
        );
        // With height to spare, the text wraps and fits.
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
