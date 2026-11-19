// SPDX-License-Identifier: GPL-3.0-only

//! Rows, columns and lists of immediate children, and a parent's
//! arrangements in its order of preference.
//!
//! A row or column shares a shortage along its direction with
//! [`allocate`], then asks each child for its size across at the room it
//! got. Gaps are children with a range from a least to an ideal.
//!
//! A list passes its order to every member: "the point is that children
//! are organised so in the toolbar example the child is the list of quick
//! action buttons this means they compact together". That is the only
//! "together".
//!
//! Distribution of the level (design §5). A level ordered from above is a
//! ceiling. Every child of a row or column starts at Preferred. The
//! parent's own short list says which child is asked first. When the
//! composition does not fit, the parent's queries show which child does
//! not fit its own room, or which child is taking room that a child
//! marked flexible lacks. That child is ordered up one level, never past
//! the ceiling, and only if a query shows the next level helps. A child
//! that fits and takes no such room is not ordered up. This repeats until
//! the composition fits or nothing more can be ordered up, and then the
//! composition reports no fit with what was short. Every choice goes into
//! the plan's reasons.

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

/// Space between children: a range from a least to an ideal length along
/// its composition, and nothing across. Every ideal distance is kept at
/// Preferred and Mild; from Strong a gap may be squeezed to its least
/// (design §5: Strong is "flagged squeezing below an ideal").
#[derive(Clone, Debug)]
pub struct Gap {
    id: Id,
    direction: Direction,
    least: f64,
    ideal: f64,
    accepts: Accepts,
}

impl Gap {
    /// The level from which a gap may go below its ideal.
    pub const SQUEEZED_FROM: Level = Level::Strong;

    pub fn new(id: Id, direction: Direction, least: f64, ideal: f64) -> Self {
        Self {
            id,
            direction,
            least,
            ideal,
            accepts: Accepts::ordinary(),
        }
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
        let least = if orders.level >= Self::SQUEEZED_FROM {
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

/// A child of a row, column or list, with what its parent orders of it.
pub struct Member<C> {
    pub child: Child<C>,
    /// The parent's order for this child, in every kind but the level;
    /// a row or column sets the level itself, a list passes its own.
    pub order: Order,
    /// How readily it shrinks, as in [`Capacity::weight`].
    pub weight: f64,
    /// In a row or column, a flexible child keeps its preferred length
    /// until the others have given what their levels allow. A list's
    /// members change together, so a list has no flexible member.
    pub flexible: bool,
}

impl<C> Member<C> {
    pub fn new(child: Child<C>) -> Self {
        Self {
            child,
            order: Order::NONE,
            weight: 1.0,
            flexible: false,
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
}

impl<C> Clone for Member<C> {
    fn clone(&self) -> Self {
        Self {
            child: Rc::clone(&self.child),
            order: self.order.clone(),
            weight: self.weight,
            flexible: self.flexible,
        }
    }
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
    gaps: usize,
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
            gaps: 0,
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

    /// What this composition's maker asks of it.
    pub fn with_maker(mut self, maker: Order) -> Self {
        self.maker = maker;
        self
    }

    /// The presentations this composition accepts. A list passes what it
    /// resolves to on to every member, so each member must accept it too.
    pub fn accepting(mut self, accepts: Accepts) -> Self {
        self.accepts = accepts;
        self
    }

    pub fn child(self, child: Child<C>) -> Self {
        self.member(Member::new(child))
    }

    pub fn member(mut self, member: Member<C>) -> Self {
        self.members.push(member);
        self
    }

    /// The parent's short list of who is asked first. Children not on it
    /// are asked after, in their order in the composition.
    pub fn asked_first(mut self, ids: impl IntoIterator<Item = Id>) -> Self {
        self.asked_first = ids.into_iter().collect();
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
    /// A gap from `least` to `ideal` along this composition, as the next
    /// child.
    pub fn gap(mut self, least: f64, ideal: f64) -> Self {
        self.gaps += 1;
        let id = self.id.child(format!("gap{}", self.gaps));
        let gap: Child<C> = Rc::new(Gap::new(id, self.direction, least, ideal));
        self.member(Member::new(gap))
    }
}

/// The children's answers under one assignment of levels.
struct Trial {
    orders: Vec<Order>,
    /// Answers in the composition's whole space: the lengths along.
    full: Vec<Answer>,
    /// Lengths along, when the least lengths fit.
    lengths: Option<Vec<f64>>,
    /// Answers at each child's room, when there are lengths.
    rooms: Vec<Answer>,
    /// What the least lengths need and the room has, when they do not fit.
    along_short: Option<(f64, f64)>,
    /// Whether the flexible children kept their preferred lengths.
    protected: bool,
}

impl Trial {
    fn fits(&self) -> bool {
        self.along_short.is_none() && self.rooms.iter().all(Answer::fits)
    }

    /// The answer that places child `i`.
    fn placing(&self, i: usize) -> &Answer {
        self.rooms.get(i).unwrap_or(&self.full[i])
    }
}

/// The distribution's result.
struct Outcome {
    levels: Vec<Level>,
    trial: Trial,
    reasons: Vec<Reason>,
    short: Vec<Shortfall>,
}

fn lacks(answer: &Answer) -> f64 {
    answer.short.iter().map(Shortfall::lacks).sum()
}

impl<C> Stack<C> {
    fn across(&self) -> Direction {
        self.direction.other()
    }

    fn room(&self, space: Size, along: f64) -> Size {
        size_in(self.direction, along, self.across().length(space))
    }

    fn order_for(&self, i: usize, level: Level, own: &Resolved) -> Order {
        if self.together {
            Order::from(own)
        } else {
            self.members[i].order.clone().with_level(level)
        }
    }

    /// The order in which children are asked.
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

    fn trial(
        &self,
        levels: &[Level],
        own: &Resolved,
        space: Size,
        protect: bool,
    ) -> Result<Trial, Unsupported> {
        let d = self.direction;
        let orders: Vec<Order> = (0..self.members.len())
            .map(|i| self.order_for(i, levels[i], own))
            .collect();
        let full = self
            .members
            .iter()
            .zip(&orders)
            .map(|(member, order)| member.child.query(order, space))
            .collect::<Result<Vec<_>, _>>()?;
        let available = d.length(space);
        let allocated = if available.is_finite() {
            let capacities: Vec<Capacity> = full
                .iter()
                .zip(&self.members)
                .map(|(answer, member)| {
                    let least = d.length(answer.least);
                    let weight = if protect && member.flexible {
                        0.0
                    } else {
                        member.weight
                    };
                    Capacity::new(d.length(answer.preferred).max(least), least, weight)
                })
                .collect();
            allocate(available, &capacities)
        } else {
            Ok(full
                .iter()
                .map(|answer| d.length(answer.preferred).max(d.length(answer.least)))
                .collect())
        };
        let mut trial = Trial {
            orders,
            full,
            lengths: None,
            rooms: Vec::new(),
            along_short: None,
            protected: protect,
        };
        match allocated {
            Ok(lengths) => {
                trial.rooms = self
                    .members
                    .iter()
                    .zip(&trial.orders)
                    .zip(&lengths)
                    .map(|((member, order), length)| {
                        member.child.query(order, self.room(space, *length))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                trial.lengths = Some(lengths);
            }
            Err(AllocationError::NoFit {
                required,
                available,
            }) => trial.along_short = Some((required, available)),
            // A child answered with lengths that cannot be shared: no
            // length is enough.
            Err(AllocationError::InvalidInput) => {
                trial.along_short = Some((f64::INFINITY, available));
            }
        }
        Ok(trial)
    }

    /// The children the queries show, in asking order, with what showed.
    fn asked(&self, trial: &Trial) -> Vec<(usize, Showed)> {
        let order = self.asking_order();
        if let Some((needs, has)) = trial.along_short {
            let flexible = self
                .members
                .iter()
                .find(|member| member.flexible)
                .filter(|_| trial.protected);
            let showed = match flexible {
                Some(member) => Showed::TakesRoomFlexibleLacks {
                    flexible: member.child.id().clone(),
                    lacks: needs - has,
                },
                None => Showed::NoRoomForTheLeast { needs, has },
            };
            order
                .into_iter()
                .filter(|i| !self.members[*i].flexible || flexible.is_none())
                .map(|i| (i, showed.clone()))
                .collect()
        } else {
            order
                .into_iter()
                .filter(|i| !trial.rooms[*i].fits())
                .map(|i| {
                    let short = trial.rooms[i].short.clone();
                    (i, Showed::DoesNotFitItsRoom { short })
                })
                .collect()
        }
    }

    /// The level child `i` would be ordered up to, or why it is kept. A
    /// level at which the child answers exactly as before is the same
    /// form (a box need not have a form at every level), so the next
    /// level is the next one at which its answer changes.
    fn next_level(
        &self,
        i: usize,
        from: Level,
        own: &Resolved,
        space: Size,
        trial: &Trial,
        showed: &Showed,
    ) -> Result<Result<Level, Kept>, Unsupported> {
        let d = self.direction;
        let member = &self.members[i];
        let room = trial
            .lengths
            .as_ref()
            .map(|lengths| self.room(space, lengths[i]));
        let current_full = &trial.full[i];
        let current_room = trial.rooms.get(i);
        let mut level = from;
        let mut unchanged = None;
        while let Some(next) = level.next().filter(|next| *next <= own.level) {
            let order = self.order_for(i, next, own);
            let full = member.child.query(&order, space)?;
            let at_room = room
                .map(|room| member.child.query(&order, room))
                .transpose()?;
            if &full == current_full && at_room.as_ref() == current_room {
                unchanged.get_or_insert(next);
                level = next;
                continue;
            }
            let gives_room = d.length(full.least) < d.length(current_full.least) - FIT_EPSILON;
            let helps = match (showed, &at_room, current_room) {
                (Showed::DoesNotFitItsRoom { .. }, Some(at_room), Some(current)) => {
                    gives_room || lacks(at_room) < lacks(current) - FIT_EPSILON
                }
                _ => gives_room,
            };
            return Ok(if helps {
                Ok(next)
            } else {
                Err(Kept::NextLevelDoesNotHelp { next })
            });
        }
        Ok(Err(match unchanged {
            Some(next) => Kept::NextLevelDoesNotHelp { next },
            None => Kept::AtCeiling,
        }))
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
            let mut raised = false;
            for (i, showed) in self.asked(&trial) {
                let child = self.members[i].child.id().clone();
                match self.next_level(i, levels[i], own, space, &trial, &showed)? {
                    Ok(next) => {
                        reasons.push(Reason::OrderedUp {
                            child,
                            from: levels[i],
                            to: next,
                            showed,
                        });
                        levels[i] = next;
                        raised = true;
                        break;
                    }
                    Err(why) => {
                        let kept = Reason::Kept {
                            child,
                            level: levels[i],
                            showed,
                            why,
                        };
                        if !reasons.contains(&kept) {
                            reasons.push(kept);
                        }
                    }
                }
            }
            if raised {
                continue;
            }
            // Nothing more can be ordered up: the flexible children give
            // room too.
            let trial = self.trial(&levels, own, space, false)?;
            if let Some(lengths) = &trial.lengths {
                for (i, member) in self.members.iter().enumerate() {
                    let preferred = self.direction.length(trial.full[i].preferred);
                    if member.flexible && lengths[i] < preferred - FIT_EPSILON {
                        reasons.push(Reason::FlexibleSqueezed {
                            child: member.child.id().clone(),
                            preferred,
                            got: lengths[i],
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
            return Ok(Outcome {
                levels,
                trial,
                reasons,
                short,
            });
        }
    }

    /// The least length along that the children can reach under the
    /// ceiling, and the length along they prefer at Preferred.
    fn range_along(&self, own: &Resolved, space: Size) -> Result<(f64, f64), Unsupported> {
        let d = self.direction;
        let mut preferred = 0.0;
        let mut least = 0.0;
        for (i, member) in self.members.iter().enumerate() {
            let levels: Vec<Level> = if self.together {
                vec![own.level]
            } else {
                Level::ALL
                    .into_iter()
                    .take_while(|level| *level <= own.level)
                    .collect()
            };
            let mut child_least = f64::INFINITY;
            for (k, level) in levels.into_iter().enumerate() {
                let answer = member.child.query(&self.order_for(i, level, own), space)?;
                if k == 0 {
                    preferred += d.length(answer.preferred).max(d.length(answer.least));
                }
                child_least = child_least.min(d.length(answer.least));
            }
            least += child_least;
        }
        Ok((preferred.max(least), least))
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

    fn measure(&self, orders: &Resolved, space: Size) -> Result<Answer, Unsupported> {
        let outcome = self.distribute(orders, space)?;
        let (preferred_along, least_along) = self.range_along(orders, space)?;
        let across = self.across();
        let offered_across = across.length(space);
        let answers: Vec<&Answer> = (0..self.members.len())
            .map(|i| outcome.trial.placing(i))
            .collect();
        let preferred_across = answers
            .iter()
            .map(|answer| answer.taken(across, offered_across))
            .fold(0.0, f64::max);
        let least_across = answers
            .iter()
            .map(|answer| across.length(answer.least))
            .fold(0.0, f64::max);
        Ok(Answer {
            preferred: size_in(self.direction, preferred_along, preferred_across),
            least: size_in(self.direction, least_along, least_across),
            baseline: None,
            short: outcome.short,
            unshown: answers
                .iter()
                .flat_map(|answer| answer.unshown.iter().cloned())
                .collect(),
        })
    }

    fn arrange(&self, orders: &Resolved, bounds: Rect) -> Result<Node<C>, Unsupported> {
        let space = bounds.size();
        let outcome = self.distribute(orders, space)?;
        let d = self.direction;
        let across = self.across();
        let trial = &outcome.trial;
        let mut node =
            Node::new(self.id.clone(), orders.clone(), bounds).with_source(self.source.clone());
        let mut offset = 0.0;
        for (i, member) in self.members.iter().enumerate() {
            let answer = trial.placing(i);
            let along = match &trial.lengths {
                Some(lengths) => lengths[i],
                None => d.length(answer.least),
            };
            let size = size_in(d, along, answer.taken(across, across.length(space)));
            let origin = match d {
                Direction::Across => (bounds.x + offset, bounds.y),
                Direction::Down => (bounds.x, bounds.y + offset),
            };
            let rect = Rect::new(origin.0, origin.1, size.width, size.height);
            node.children
                .push(member.child.order(&trial.orders[i], rect)?);
            offset += along;
        }
        debug_assert_eq!(outcome.levels.len(), node.children.len());
        node.reasons = outcome.reasons;
        Ok(node)
    }
}

/// A parent's arrangements of its children, in its order of preference.
/// Each is tried from Preferred again, under the ceiling ordered from
/// above, and the first that fits is taken.
pub struct Arrangements<C> {
    id: Id,
    source: Option<Source>,
    maker: Order,
    accepts: Accepts,
    options: Vec<Child<C>>,
}

impl<C> Arrangements<C> {
    pub fn new(id: Id, options: Vec<Child<C>>) -> Self {
        Self {
            id,
            source: None,
            maker: Order::NONE,
            accepts: Accepts::ordinary(),
            options,
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

    /// Each arrangement is given only the ceiling.
    fn option_order(orders: &Resolved) -> Order {
        Order::NONE.with_level(orders.level)
    }

    /// Every arrangement's answer, and the first that fits.
    fn choose(
        &self,
        orders: &Resolved,
        space: Size,
    ) -> Result<(Vec<Answer>, Option<usize>), Unsupported> {
        let order = Self::option_order(orders);
        let answers = self
            .options
            .iter()
            .map(|option| option.query(&order, space))
            .collect::<Result<Vec<_>, _>>()?;
        let chosen = answers.iter().position(Answer::fits);
        Ok((answers, chosen))
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

    /// The chosen arrangement's answer, with the least size any
    /// arrangement can reach. When none fits: the first's preferred size,
    /// and the last's shortfalls, since it is the parent's last resort.
    fn measure(&self, orders: &Resolved, space: Size) -> Result<Answer, Unsupported> {
        let (answers, chosen) = self.choose(orders, space)?;
        let Some(last) = answers.last() else {
            return Ok(Answer::of(
                &self.id,
                Size::default(),
                Size::default(),
                space,
            ));
        };
        let least = answers.iter().fold(Size::INFINITE, |least, answer| {
            Size::new(
                least.width.min(answer.least.width),
                least.height.min(answer.least.height),
            )
        });
        let mut answer = match chosen {
            Some(k) => answers[k].clone(),
            None => Answer {
                preferred: answers[0].preferred,
                ..last.clone()
            },
        };
        answer.least = least;
        Ok(answer)
    }

    fn arrange(&self, orders: &Resolved, bounds: Rect) -> Result<Node<C>, Unsupported> {
        let (answers, chosen) = self.choose(orders, bounds.size())?;
        let mut node =
            Node::new(self.id.clone(), orders.clone(), bounds).with_source(self.source.clone());
        let taken = chosen.unwrap_or(answers.len().saturating_sub(1));
        for (k, (option, answer)) in self.options.iter().zip(&answers).enumerate() {
            let arrangement = option.id().clone();
            if k == taken && chosen.is_some() {
                node.reasons.push(Reason::ArrangementTaken { arrangement });
            } else if k <= taken {
                node.reasons.push(Reason::ArrangementRejected {
                    arrangement,
                    short: answer.short.clone(),
                });
            }
        }
        if let Some(option) = self.options.get(taken) {
            let size = answers[taken].taken_size(bounds.size());
            let rect = Rect::new(bounds.x, bounds.y, size.width, size.height);
            node.children
                .push(option.order(&Self::option_order(orders), rect)?);
        }
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

    #[test]
    fn a_lists_members_change_together_and_a_sibling_that_fits_stays() {
        let buttons = (1..=3).fold(
            Stack::list(Id::new("actions"), Direction::Across),
            |list, n| list.child(compact(&format!("button{n}"), 50.0, 20.0, 20.0)),
        );
        let toolbar = Stack::row(Id::new("toolbar"))
            .child(compact("title", 100.0, 60.0, 20.0))
            .child(Rc::new(buttons))
            .asked_first([Id::new("actions"), Id::new("title")]);
        let plan = lay_out(&toolbar, &all_permitted(), Size::new(200.0, 20.0)).unwrap();
        for n in 1..=3 {
            assert_eq!(level_of(&plan, &format!("button{n}")), Level::Mild);
            assert_eq!(
                plan.find(&Id::new(format!("button{n}")))
                    .unwrap()
                    .bounds
                    .width,
                20.0
            );
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
        // The ceiling ordered from above is never passed.
        let refused = lay_out(
            &toolbar,
            &Order::NONE.with_level(Level::Preferred),
            Size::new(200.0, 20.0),
        );
        assert!(matches!(refused, Err(Refusal::NoFit { .. })));
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
                    why: Kept::NextLevelDoesNotHelp { next: Level::Mild },
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
        assert!(plan.find(&Id::new("column/gap1")).is_some());
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
        assert_eq!(strong.find(&Id::new("row/gap1")).unwrap().bounds.width, 5.0);
        assert!(strong.reasons.contains(&Reason::OrderedUp {
            child: Id::new("row/gap1"),
            from: Level::Preferred,
            to: Level::Strong,
            showed: Showed::NoRoomForTheLeast {
                needs: 90.0,
                has: 85.0
            },
        }));
    }
}
