// SPDX-License-Identifier: GPL-3.0-only

//! Arrangement for the household's apps, charts and diagrams: the apps'
//! layout engine without a toolkit.
//!
//! The household apps lay out through one engine (household-widgets'
//! `layout`): given a tree of parts, one level and a room, where does each
//! part go, and does it all fit? This crate is that engine's pure geometry,
//! moved out of the widgets so that the charts can lay out with the very
//! same code. It draws nothing, measures nothing and names no toolkit type;
//! the widgets keep their interface and delegate here. It is being moved in
//! parts, and holds so far the values the engine works in and the sharing
//! of a length:
//!
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
pub mod level;
pub mod size;

pub use allocation::{
    AllocationError, FIT_EPSILON, WidthCapacity, allocate_one, allocate_width, allocate_with_growth,
};
pub use level::{Arrangement, Level};
pub use size::{Measured, Point, Size, finite_size, valid_available, validate_measurement};
