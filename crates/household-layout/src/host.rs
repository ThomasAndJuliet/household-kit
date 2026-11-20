// SPDX-License-Identifier: GPL-3.0-only

//! The host: whoever owns the leaves and draws the result. The engine is
//! generic over it, and asks it one question.

use crate::{Level, Size};

/// Whoever owns the leaves of a tree of parts and draws the plan the engine
/// makes of it: household-widgets now, whose leaves are native views, and
/// the charts later, whose leaves are text and marks.
///
/// The engine knows a leaf only through the one question `size` answers,
/// and keeps on every part, for the host, what the host declares there
/// (`Note`) and on every frame what draws the frame (`Chrome`). It reads
/// neither: they are the host's, and the host reads them back from the
/// parts when it draws a plan (`Child::note`, `Frame::chrome`).
///
/// One trait with four associated types, rather than a leaf trait and
/// loose parameters, so that every type of the engine has one parameter
/// (`Child<'a, H>`), and a frame's chrome is part of the frame by its type.
pub trait Host {
    /// What a leaf is to the host (the widgets: a view factory).
    type Leaf;
    /// What measuring a leaf needs from outside (the widgets: the
    /// renderer). Every measuring callback a caller declares is given it
    /// too.
    type Context: ?Sized;
    /// What the host keeps on every part for itself. The core stores it and
    /// never reads it; a part the engine makes itself (the column that
    /// `Child::fill_width` and `Child::fixed_dimensions` put a part in)
    /// starts with the note's `Default`.
    type Note;
    /// What the host keeps on every frame (the widgets: what draws the
    /// padding).
    type Chrome;

    /// The one question the core asks: this leaf's size at this level within
    /// this offer.
    ///
    /// The size must be the leaf's own, laid out as the offer says. A size
    /// wider than the offer's width, or taller than a height the leaf was
    /// to be laid out in, does not fit, and the engine says so; a size that
    /// is not finite and not negative is invalid input. The engine asks this
    /// for disposable measurements as well as for the plan it keeps, so
    /// answering must not change anything.
    fn size(leaf: &Self::Leaf, level: Level, context: &Self::Context, offer: Offer) -> Size;
}

/// The room a leaf is offered, and how it is to use the height. Either
/// dimension of the room may be infinite: unbounded.
///
/// The engine asks with `NeededHeight` for every leaf except one that takes
/// a range of heights while it is planned (`LeafHeight::Range`): a leaf's
/// height is what it needs at its width, and that height is a floor. A
/// leaf that takes a range of heights is planned with `OfferedHeight`, in a
/// height between its floor and the room.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Offer {
    /// Take the height the leaf needs, within the room: the height is the
    /// leaf's own, however much the room has.
    NeededHeight(Size),
    /// Be laid out in the height offered: the room's height is the height
    /// the leaf is to work in.
    OfferedHeight(Size),
}
