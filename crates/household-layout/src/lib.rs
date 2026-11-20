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
//! leaves ([`Host`]). It holds what a part is, how it is measured, and how a
//! row, a column, a wrap, a frame and forms are planned:
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
