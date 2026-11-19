// SPDX-License-Identifier: GPL-3.0-only

//! The box a parent sees, and the kept plan.
//!
//! A parent deals with an immediate child in two ways only, and never sees
//! which form the child chose. It **queries** the child: under given
//! orders and in an offered space, the child answers with its preferred
//! and least sizes, whether it fits, what it could not fit and what its
//! chosen form does not show. It **orders** the child, which yields that
//! child's part of the plan. Queries are pure and may be repeated at any
//! orders. A size depends on the width offered (text wraps), so a box is
//! asked at a space, never once for a fixed pair.
//!
//! The plan is kept data, not a drawing: a tree of [`Node`]s with
//! identities, bounds, the orders each resolved to, and the reasons for
//! what was chosen and rejected. A leaf's node also carries whatever its
//! user needs to draw it, as the type parameter `C`, which the crate never
//! looks inside.

use crate::geometry::{Direction, FIT_EPSILON, Point, Rect, Size};
use crate::identity::{Id, Source};
use crate::level::Level;
use crate::order::{Accepts, Order, Resolved, Unsupported};

/// The maker's request of a box whose maker asks for nothing.
static NO_REQUEST: Order = Order::NONE;

/// A box, as its parent sees it. `C` is what a leaf carries for its user
/// to draw it with.
///
/// An implementor supplies [`measure`](Self::measure) and
/// [`arrange`](Self::arrange) under orders that have already resolved;
/// a parent calls [`query`](Self::query) and [`order`](Self::order),
/// which resolve the parent's order first, so an order the box does not
/// accept is refused before anything is measured.
pub trait LayoutBox<C> {
    /// The box's stable identity.
    fn id(&self) -> &Id;

    /// The field or datum of the input the box draws, if any.
    fn source(&self) -> Option<&Source> {
        None
    }

    /// What the box's maker asks of it. It comes before any parent's
    /// order, kind by kind.
    fn maker(&self) -> &Order {
        &NO_REQUEST
    }

    /// The presentations the box accepts, and its family's default.
    fn accepts(&self) -> &Accepts;

    /// Whether the box takes the level it is ordered exactly, as a leaf
    /// with forms and a list do, rather than distributing it as a ceiling
    /// among its children, as a row, a column and arrangements do.
    /// Whoever holds a ceiling over a box that takes its level exactly
    /// gives it each level from Preferred up and orders it the lowest at
    /// which it fits.
    fn takes_level_exactly(&self) -> bool {
        true
    }

    /// The box's answer under `orders` in `space`. Pure.
    fn measure(&self, orders: &Resolved, space: Size) -> Result<Answer, Unsupported>;

    /// The box's part of the plan, placed at `bounds`, under `orders`.
    /// The same `orders` and the size of `bounds` give the same choices
    /// as [`measure`](Self::measure) at that size.
    fn arrange(&self, orders: &Resolved, bounds: Rect) -> Result<Node<C>, Unsupported>;

    /// The parent's order for this box, resolved against the maker's
    /// request and the family's default.
    fn resolve(&self, parent: &Order) -> Result<Resolved, Unsupported> {
        self.accepts().resolve(self.id(), self.maker(), parent)
    }

    /// Query the box under the parent's `order` in `space`.
    fn query(&self, order: &Order, space: Size) -> Result<Answer, Unsupported> {
        self.measure(&self.resolve(order)?, space)
    }

    /// Order the box: its part of the plan at `bounds`.
    fn order(&self, order: &Order, bounds: Rect) -> Result<Node<C>, Unsupported> {
        self.arrange(&self.resolve(order)?, bounds)
    }
}

/// A box's answer to a query in an offered space.
///
/// `preferred.width` is the width the box would take with room to spare;
/// `least.width` is the narrowest it can be under these orders. Both
/// heights are at the width the box would take in the offered space
/// ([`Answer::taken`] across), since text wraps: `preferred.height` is
/// the height it would like there, `least.height` the least it can be.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Answer {
    pub preferred: Size,
    pub least: Size,
    /// The baseline's distance from the box's top, if it has one.
    pub baseline: Option<f64>,
    /// What could not fit. Empty when the box fits.
    pub short: Vec<Shortfall>,
    /// What the chosen form does not show, reported upward.
    pub unshown: Vec<Unshown>,
}

impl Answer {
    /// The answer of a box that speaks only for itself: `part` falls short
    /// in each direction where its least size exceeds `space`.
    pub fn of(part: &Id, preferred: Size, least: Size, space: Size) -> Self {
        let short = [Direction::Across, Direction::Down]
            .into_iter()
            .filter_map(|direction| {
                Shortfall::of(
                    part,
                    direction,
                    direction.length(least),
                    direction.length(space),
                )
            })
            .collect();
        Self {
            preferred,
            least,
            baseline: None,
            short,
            unshown: Vec::new(),
        }
    }

    pub fn with_baseline(mut self, baseline: f64) -> Self {
        self.baseline = Some(baseline);
        self
    }

    pub fn with_unshown(mut self, unshown: Unshown) -> Self {
        self.unshown.push(unshown);
        self
    }

    /// Whether the box fits the space it was offered.
    pub fn fits(&self) -> bool {
        self.short.is_empty()
    }

    /// The length the box takes in `direction` when `offered` that much:
    /// its preferred length, no more than offered, never below its least.
    pub fn taken(&self, direction: Direction, offered: f64) -> f64 {
        direction
            .length(self.preferred)
            .min(offered)
            .max(direction.length(self.least))
    }

    /// The size the box takes in `space`.
    pub fn taken_size(&self, space: Size) -> Size {
        Size::new(
            self.taken(Direction::Across, space.width),
            self.taken(Direction::Down, space.height),
        )
    }
}

/// What could not fit: the part, the direction, the length it needs and
/// the length it has, so that a refusal can name a size that helps.
#[derive(Clone, Debug, PartialEq)]
pub struct Shortfall {
    pub part: Id,
    pub direction: Direction,
    pub needs: f64,
    pub has: f64,
}

impl Shortfall {
    /// A shortfall of `part` in `direction`, if `needs` exceeds `has` by
    /// more than the fit tolerance.
    pub fn of(part: &Id, direction: Direction, needs: f64, has: f64) -> Option<Self> {
        (needs > has + FIT_EPSILON).then(|| Self {
            part: part.clone(),
            direction,
            needs,
            has,
        })
    }

    /// How much more room the part needs.
    pub fn lacks(&self) -> f64 {
        self.needs - self.has
    }
}

/// Something a box's chosen form does not show, reported to its parent
/// so that the parent can supply what restores it (design §9).
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Unshown {
    /// What is not shown, by its identity.
    pub part: Id,
    pub loss: Loss,
}

/// How a thing is not shown. The apps' two kinds of information loss.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Loss {
    Shortened,
    Omitted,
}

/// One box's part of the kept plan.
#[derive(Clone, Debug, PartialEq)]
pub struct Node<C> {
    pub id: Id,
    pub source: Option<Source>,
    pub bounds: Rect,
    /// The baseline's distance from the top of `bounds`, if it has one.
    pub baseline: Option<f64>,
    /// The orders the box resolved to.
    pub orders: Resolved,
    /// What a leaf's user needs to draw it. `None` for a composition.
    pub content: Option<C>,
    pub children: Vec<Node<C>>,
    /// Why the box's children are as they are, in the order decided.
    pub reasons: Vec<Reason>,
}

impl<C> Node<C> {
    /// A node with no content, no children and no reasons.
    pub fn new(id: Id, orders: Resolved, bounds: Rect) -> Self {
        Self {
            id,
            source: None,
            bounds,
            baseline: None,
            orders,
            content: None,
            children: Vec::new(),
            reasons: Vec::new(),
        }
    }

    /// A leaf's node, carrying `content` for its user.
    pub fn leaf(id: Id, orders: Resolved, bounds: Rect, content: C) -> Self {
        Self {
            content: Some(content),
            ..Self::new(id, orders, bounds)
        }
    }

    pub fn with_source(mut self, source: Option<Source>) -> Self {
        self.source = source;
        self
    }

    pub fn with_baseline(mut self, baseline: Option<f64>) -> Self {
        self.baseline = baseline;
        self
    }

    /// The node of the box `id`: this one or a descendant.
    pub fn find(&self, id: &Id) -> Option<&Node<C>> {
        if &self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(id))
    }

    /// Every node of the tree, this one first, depth first.
    pub fn nodes(&self) -> Vec<&Node<C>> {
        let mut nodes = vec![self];
        for child in &self.children {
            nodes.extend(child.nodes());
        }
        nodes
    }
}

/// A reason for a choice, as data a program can read.
#[derive(Clone, Debug, PartialEq)]
pub enum Reason {
    /// `child` was ordered up from one level to the next, because of what
    /// the parent's query showed.
    OrderedUp {
        child: Id,
        from: Level,
        to: Level,
        showed: Showed,
    },
    /// `child` was asked and left at `level`.
    Kept {
        child: Id,
        level: Level,
        showed: Showed,
        why: Kept,
    },
    /// Nothing more could be ordered up, so the flexible child `child`
    /// was given less than it prefers.
    FlexibleSqueezed { child: Id, preferred: f64, got: f64 },
    /// An arrangement was tried from Preferred and did not fit.
    ArrangementRejected {
        arrangement: Id,
        short: Vec<Shortfall>,
    },
    /// The first arrangement that fits.
    ArrangementTaken { arrangement: Id },
    /// Nothing more could be ordered up and the composition does not fit.
    NoFit { short: Vec<Shortfall> },
}

/// What a parent's queries showed about a child.
#[derive(Clone, Debug, PartialEq)]
pub enum Showed {
    /// The child does not fit the room it got.
    DoesNotFitItsRoom { short: Vec<Shortfall> },
    /// The child takes room that the flexible child `flexible` lacks.
    TakesRoomFlexibleLacks { flexible: Id, lacks: f64 },
    /// The children's least lengths together exceed the room, so no child
    /// has room for its least length.
    NoRoomForTheLeast { needs: f64, has: f64 },
}

/// Why a child that was asked was not ordered up.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kept {
    /// A query showed that its next level would not help.
    NextLevelDoesNotHelp { next: Level },
    /// Its next level is past the ceiling ordered from above, or there is
    /// none.
    AtCeiling,
}

/// Why no plan was made.
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    /// A box does not accept an order. Not a failure to fit.
    Unsupported(Unsupported),
    /// The tree does not fit the space; each shortfall names a size that
    /// helps.
    NoFit { short: Vec<Shortfall> },
}

impl From<Unsupported> for Refusal {
    fn from(unsupported: Unsupported) -> Self {
        Self::Unsupported(unsupported)
    }
}

/// Lay out `root` under `order` in `space`, at the origin: the kept plan,
/// or a refusal. The level `order` resolves to is a ceiling. A root that
/// [takes its level exactly](LayoutBox::takes_level_exactly) is given each
/// level from Preferred up to it and ordered the first at which it fits,
/// so that nothing is ordered more compaction than it needs; any other
/// root distributes the ceiling itself.
pub fn plan<C>(root: &dyn LayoutBox<C>, order: &Order, space: Size) -> Result<Node<C>, Refusal> {
    let ceiling = root.resolve(order)?.level;
    let lowest = if root.takes_level_exactly() {
        Level::Preferred
    } else {
        ceiling
    };
    let mut short = Vec::new();
    for level in Level::ALL
        .into_iter()
        .filter(|level| (lowest..=ceiling).contains(level))
    {
        let at = order.clone().with_level(level);
        let answer = root.query(&at, space)?;
        if answer.fits() {
            let bounds = Rect::from_origin(Point::default(), answer.taken_size(space));
            return Ok(root.order(&at, bounds)?);
        }
        short = answer.short;
    }
    Err(Refusal::NoFit { short })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_falls_short_where_its_least_exceeds_the_space() {
        let part = Id::new("name");
        let answer = Answer::of(
            &part,
            Size::new(80.0, 20.0),
            Size::new(60.0, 20.0),
            Size::new(50.0, 30.0),
        );
        assert!(!answer.fits());
        assert_eq!(
            answer.short,
            vec![Shortfall {
                part,
                direction: Direction::Across,
                needs: 60.0,
                has: 50.0
            }]
        );
        assert_eq!(
            answer.taken_size(Size::new(70.0, 30.0)),
            Size::new(70.0, 20.0)
        );
        assert_eq!(
            answer.taken_size(Size::new(50.0, 30.0)),
            Size::new(60.0, 20.0)
        );
    }

    #[test]
    fn a_node_is_found_by_identity() {
        let mut root: Node<()> = Node::new(Id::new("root"), Resolved::default(), Rect::default());
        let mut middle = Node::new(Id::new("middle"), Resolved::default(), Rect::default());
        middle.children.push(Node::leaf(
            Id::new("leaf"),
            Resolved::default(),
            Rect::new(1.0, 2.0, 3.0, 4.0),
            (),
        ));
        root.children.push(middle);
        assert_eq!(
            root.find(&Id::new("leaf")).map(|node| node.bounds),
            Some(Rect::new(1.0, 2.0, 3.0, 4.0))
        );
        assert!(root.find(&Id::new("nowhere")).is_none());
        assert_eq!(root.nodes().len(), 3);
    }
}
