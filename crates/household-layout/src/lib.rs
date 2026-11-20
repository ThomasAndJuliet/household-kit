// SPDX-License-Identifier: GPL-3.0-only

//! Arrangement for the household's apps, charts and diagrams: the apps'
//! layout engine without a toolkit.
//!
//! The household apps lay out through one engine (household-widgets'
//! `layout`): given a tree of parts, one level and a room, where does each
//! part go, and does it all fit? This crate is that engine's pure geometry,
//! moved out of the widgets so that the charts can lay out with the very
//! same code. It draws nothing, measures nothing and names no toolkit type;
//! the widgets keep their interface and delegate here, as the host of the
//! leaves ([`Host`]).
//!
//! # How the engine works
//!
//! **The host.** The engine does not know what a leaf is. Whoever owns the
//! leaves and draws the result is the host, and the engine asks it one
//! question: this leaf's size, at this level, within this offer
//! ([`Host::size`]). An offer is a room, either dimension of which may be
//! unbounded, and how the leaf is to use its height: take the height it
//! needs, or be laid out in the height offered ([`Offer`]). The host also
//! keeps a note of its own on every part and the chrome of every frame; the
//! engine carries them and never reads them. household-widgets is a host
//! whose leaves are native views, measured by laying them out; a chart is
//! one whose leaves are text and marks.
//!
//! **A part** ([`Child`]) is one of four kinds ([`Kind`]): a leaf of the
//! host's; a composition, whose children are set out in a row, a column or
//! a wrap; a frame, a fixed padding round one child; or forms, a family's
//! alternatives, one per level. Whatever its kind, a part declares how it
//! shares room with the parts beside it ([`Sharing`]: its weight, its
//! growth in width and in height, whether it reserves the width it is
//! allocated, which dimensions are fixed), when it is hidden, and any
//! adjustments to its measurement. It is built by a constructor for its
//! kind and declared by builders; nothing outside the engine writes to it.
//!
//! **A level** is how much a part may compromise: `Preferred`, `Mild`,
//! `Strong`, `AllPermitted`. Only the root is given a level; every part
//! below is measured and planned at that same level. A part may be hidden
//! at a level, and then it takes no room and no gap. Forms choose by the
//! level: each form holds from its threshold until the next, so at any
//! level exactly one form stands for the family. The host tries a list of
//! (level, tree) stages in order and keeps the first that fits.
//!
//! **Measuring** gives a part's preferred size and its useful minimum
//! ([`Measured`]). A part's kind gives the first measurement: a leaf is
//! asked its size in unbounded room, and that size is both its preferred
//! size and its minimum; a composition adds up or stacks its visible
//! children; a frame adds its padding to its content; forms measure the
//! form selected. Then the adjustments its builders declared are applied,
//! one after another in the order they were declared: a floor of width, a
//! preferred width, rounding up, fixed dimensions, or a measurement that
//! replaces everything before it. The order is visible and it matters: each
//! acts on what the ones before it left. A hidden part measures zero, and an
//! invalid one (a builder was given NaN, an infinite or a negative number,
//! or forms out of order) measures NaN, which fails every plan that
//! includes it with [`AllocationError::InvalidInput`]; the engine never
//! panics and never quietly repairs a number.
//!
//! **Width first, height second.** A plan is made from the top. The root
//! is allocated its width from the room, and a composition shares its width
//! among its children by what each prefers and can work in
//! ([`WidthCapacity`], [`allocate_with_growth`]). Only then is each leaf
//! asked its size at the width it was allocated, with unbounded height, so
//! the height it gets is the height it needs at that width: a paragraph is
//! taller when narrower, and the plan has it so. That height is a floor:
//! the leaf is never offered less, because a leaf laid out in less height
//! than it needs is clipped, and a clipped height must never pass for a
//! fit. The one exception is a leaf declared to take a range of heights
//! ([`LeafHeight::Range`]: a scrolling area), which declares its own floor
//! below its height and is offered a height between the two.
//!
//! **A floor** is what a part cannot go below. In width it is the minimum
//! measured, or the preferred width when the part's weight is zero
//! ([`WidthCapacity::least`]); in height it is what a leaf needs at its
//! width, or a ranged leaf's declared floor. A plan records its floor of
//! height ([`Plan::minimum_height`]), so a parent knows how far it can be
//! squeezed.
//!
//! **A shortage and a surplus.** When the room is narrower than the
//! children prefer, the shortage is taken from them in proportion to
//! `weight × (preferred − minimum)`, none below its floor; a weight of zero
//! keeps its preferred width. When the room is wider, the surplus goes to
//! the children that grow, in proportion to their growth, and stays empty
//! when none does. A row has one line, so it offers each child its own
//! height. A column shares its height the same way, each child offering its
//! planned height down to its floor of height at its weight, and growing by
//! its `grow_height`: so a shortage of height falls on the ranged leaves
//! alone, and with none the column does not fit. A wrap packs its children
//! into rows by their floors, shares each row's width as a row does, and,
//! if the rows are taller than the room, shares the height between the
//! rows as a column of rows. A frame plans its content in what its padding
//! leaves. Forms plan their selected form in the family's place; their
//! growth in height, and a frame's, passes to what they hold.
//!
//! **Does it fit?** Every comparison of a size with its room allows
//! [`FIT_EPSILON`]. A part that does not fit fails the whole plan with
//! [`AllocationError::NoFit`], saying what it required and what there was,
//! and the host tries its next stage.
//!
//! **A plan** ([`Plan`]) is the tree of sizes and positions: each part's
//! size, its placements (index among the declared children, position, own
//! plan), a row's or a wrap's rows, its floor of height, and for a leaf the
//! size the host gave it. It holds nothing of the host's; a host that keeps
//! more (the widgets bind every node to the part that made it) makes its own
//! plan from this one.
//!
//! # A worked example
//!
//! A host whose leaves are boxes of a fixed size and paragraphs that are
//! 120 wide on one line of 10, and narrower take as many lines of 10 as
//! they need: two from 60 wide, three from 40. A row of an icon 20 square,
//! which may not shrink (weight zero), and a paragraph usable down to 30
//! wide, 10 apart, in a room 70 wide: the row prefers 150, so 80 must go,
//! and all of it comes
//! from the paragraph, whose rate is 1 × (120 − 30) against the icon's
//! zero. Of the 60 the gap leaves, the icon keeps 20 and the paragraph
//! gets 40, at which it takes three lines. The row is as high as its
//! tallest part, and the icon is centred on it.
//!
//! ```
//! use household_layout::{Child, Host, Level, Offer, Point, Size};
//!
//! enum Shape {
//!     Fixed(Size),
//!     Paragraph,
//! }
//!
//! struct Boxes;
//!
//! impl Host for Boxes {
//!     type Leaf = Shape;
//!     type Context = ();
//!     type Note = ();
//!     type Chrome = ();
//!
//!     fn size(leaf: &Shape, _: Level, _: &(), offer: Offer) -> Size {
//!         let (Offer::NeededHeight(room) | Offer::OfferedHeight(room)) = offer;
//!         match leaf {
//!             Shape::Fixed(size) => *size,
//!             Shape::Paragraph => {
//!                 let width = room.width.min(120.0);
//!                 Size::new(width, 10.0 * (120.0 / width).ceil())
//!             }
//!         }
//!     }
//! }
//!
//! let icon = Child::<Boxes>::leaf(Shape::Fixed(Size::new(20.0, 20.0))).weight(0.0);
//! let text = Child::leaf(Shape::Paragraph).minimum_width(|_, _| 30.0);
//! let row = Child::row(vec![icon, text], 10.0);
//!
//! let measured = row.query(Level::Preferred, &());
//! assert_eq!(measured.preferred, Size::new(150.0, 20.0));
//! assert_eq!(measured.minimum, Size::new(60.0, 20.0));
//!
//! let plan = row
//!     .query_at(Level::Preferred, &(), Size::new(70.0, 100.0))
//!     .unwrap();
//! assert_eq!(plan.size(), Size::new(70.0, 30.0));
//! let placed: Vec<_> = plan
//!     .children()
//!     .iter()
//!     .map(|placed| (placed.index(), placed.position(), placed.plan().size()))
//!     .collect();
//! assert_eq!(
//!     placed,
//!     [
//!         (0, Point::new(0.0, 5.0), Size::new(20.0, 20.0)),
//!         (1, Point::new(30.0, 0.0), Size::new(40.0, 30.0)),
//!     ]
//! );
//! assert_eq!(plan.rows(), [vec![0, 1]]);
//! assert_eq!(plan.minimum_height(), 30.0);
//!
//! // The row cannot be narrower than the icon, the paragraph's floor and
//! // the gap: 20 + 30 + 10.
//! assert_eq!(
//!     row.query_at(Level::Preferred, &(), Size::new(50.0, 100.0)),
//!     Err(household_layout::AllocationError::NoFit {
//!         required: 60.0,
//!         available: 50.0,
//!     }),
//! );
//! ```
//!
//! # What the crate holds
//!
//! - [`host`]: [`Host`], the one seam: whoever owns the leaves, and the one
//!   question the engine asks of a leaf, its size within an [`Offer`].
//! - [`part`]: [`Child`], a part: its [`Kind`] (a [`Leaf`], a
//!   [`Composition`], a [`Frame`] or [`Forms`]), how it shares room
//!   ([`Sharing`]), and the builders and constructors that declare it.
//!   How a part is measured is in a private module (`measure`): its kind's
//!   measurement, then the adjustments its builders declared, in order.
//! - [`plan`]: [`Plan`], where each part goes, and planning a part in a
//!   room (`Child::query_at`, `Child::plan_width`). How a row, a column and
//!   a wrap are measured and planned is in a private module (`arrange`).
//! - [`frame`]: [`Frame`], a [`Padding`] round one part, measured and
//!   planned.
//! - [`level`]: [`Level`], how much a part may compromise, and
//!   [`Arrangement`], how a part sets out its immediate parts (row, column
//!   or wrap).
//! - [`size`]: [`Size`], [`Point`] and [`Measured`] (a preferred and a
//!   useful minimum size), and the checks a size passes before the engine
//!   uses it ([`finite_size`], [`valid_available`],
//!   [`validate_measurement`]).
//! - [`allocation`]: [`WidthCapacity`] and the sharing of a length among
//!   parts ([`allocate_width`], [`allocate_with_growth`], [`allocate_one`]),
//!   its errors ([`AllocationError`]) and the tolerance of every fit
//!   ([`FIT_EPSILON`]).
//!
//! It also holds what places things inside a box once the box's size is
//! known, reached by its module paths rather than from the root:
//!
//! - [`geometry`]: points, sizes and rectangles, in `f64`.
//! - [`identity`]: a box's stable identity, and the field or datum of the
//!   input that it draws.
//! - [`kinds`]: the kinds of thing that occupy space, and roles.
//! - [`rules`]: the distance table and the covering table, as data.
//! - [`record`]: the record of occupied space, and what it answers.
//! - [`halo`]: where a line is cut round a text standing in it.
//! - [`labels`]: anchors, and sets of labels placed by pattern.
//! - [`leaders`]: a label joined to what it names by a connector.
//!
//! [`engine`] and [`presentation`] are an earlier port of the apps' engine,
//! kept only until the charts move onto the core.

// No path through the layout engine may panic. Every household app lays out
// through it, and the owner's rule is: "this is a user application - we
// should not be paining at all" (panicking). A number the engine cannot use
// is `AllocationError::InvalidInput`, never a crash. So this crate may not
// unwrap, expect, index or slice unchecked, panic, or leave unreachable,
// todo or unimplemented paths, and Clippy in CI (`-D warnings`) rejects a
// change that adds one. Tests are exempt: a test may unwrap and index, since
// a panic there is a failing test.
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]

pub mod allocation;
mod arrange;
pub mod frame;
pub mod host;
pub mod level;
mod measure;
pub mod part;
pub mod plan;
pub mod size;

// The earlier port of the apps' engine, which the charts still lay out
// with. It is not repaired, only deleted once the charts move onto the core,
// so it is exempt from the rule above; the allowance goes with it.
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented,
    clippy::indexing_slicing
)]
pub mod engine;
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented,
    clippy::indexing_slicing
)]
pub mod presentation;

pub mod geometry;
pub mod halo;
pub mod identity;
pub mod kinds;
pub mod labels;
pub mod leaders;
pub mod record;
pub mod rules;

pub use allocation::{
    AllocationError, FIT_EPSILON, WidthCapacity, allocate_one, allocate_width, allocate_with_growth,
};
pub use frame::{Frame, Padding};
pub use host::{Host, Offer};
pub use level::{Arrangement, Level};
pub use part::{
    Child, Composition, Fixed, Forms, Kind, Leaf, LeafHeight, MinimumQuery, Sharing, is_amount,
};
pub use plan::{Placed, Plan};
pub use size::{Measured, Point, Size, finite_size, valid_available, validate_measurement};
