// SPDX-License-Identifier: GPL-3.0-only

//! A host of plain boxes, as `tests/engine.rs` has, whose parts carry a
//! name as their note, so that a test can say which part a failure names
//! and which part a walk visits.

// Each test file uses its own share of these.
#![allow(dead_code)]

use household_layout::{Child, Host, Level, Offer, Padding, Size};

/// A leaf of the test host: its size as a function of what it is offered.
#[derive(Clone, Copy)]
pub enum Shape {
    /// `height` high, as wide as the room; no width when the room is
    /// unbounded, as a native box that fills its width has.
    Fill { height: f32 },
    /// Always `width` × `height`, whatever it is offered.
    Fixed { width: f32, height: f32 },
    /// A scrolling area `width` wide whose content is `height` high: it
    /// needs `height`, and laid out in a height it takes at most that.
    Scrolling { width: f32, height: f32 },
    /// 10 × 10 in unbounded room, and no size at all at a finite width: a
    /// leaf that measures but cannot be planned.
    Broken,
}

/// A host whose leaves are shapes and whose parts carry a name.
pub struct Boxes;

impl Host for Boxes {
    type Leaf = Shape;
    type Context = ();
    type Note = &'static str;
    type Chrome = ();

    fn size(leaf: &Shape, _: Level, _: &(), offer: Offer) -> Size {
        let (room, offered) = match offer {
            Offer::NeededHeight(room) => (room, false),
            Offer::OfferedHeight(room) => (room, true),
        };
        match *leaf {
            Shape::Fill { height } => {
                let width = if room.width.is_finite() {
                    room.width
                } else {
                    0.0
                };
                Size::new(width, height)
            }
            Shape::Fixed { width, height } => Size::new(width, height),
            Shape::Scrolling { width, height } => {
                if offered {
                    Size::new(width, height.min(room.height))
                } else {
                    Size::new(width, height)
                }
            }
            Shape::Broken => {
                if room.width.is_finite() {
                    Size::new(f32::NAN, f32::NAN)
                } else {
                    Size::new(10.0, 10.0)
                }
            }
        }
    }
}

pub type Part = Child<'static, Boxes>;

/// `part` with `name` as its note.
pub fn named(name: &'static str, mut part: Part) -> Part {
    *part.note_mut() = name;
    part
}

/// A box `height` high that fills the width it is given, preferring
/// `preferred` and usable down to `minimum`.
pub fn part(preferred: f32, minimum: f32, height: f32) -> Part {
    Child::leaf(Shape::Fill { height })
        .minimum_width(move |_, _| minimum)
        .preferred_width(preferred)
}

/// A box of one size, whatever it is offered.
pub fn fixed(width: f32, height: f32) -> Part {
    Child::leaf(Shape::Fixed { width, height })
}

/// The viewport of household-widgets' tests: 80 wide, 60 high, with a floor
/// of 10.
pub fn viewport() -> Part {
    Child::ranged_leaf(
        Shape::Scrolling {
            width: 80.0,
            height: 60.0,
        },
        |_, _| Size::new(80.0, 10.0),
    )
}

/// A leaf that declares a range of heights, 80 wide with a floor of 10,
/// but is 60 high however little height it is offered: offered less, it
/// does not fit.
pub fn stubborn() -> Part {
    Child::ranged_leaf(
        Shape::Fixed {
            width: 80.0,
            height: 60.0,
        },
        |_, _| Size::new(80.0, 10.0),
    )
}

pub fn all_round(padding: f32) -> Padding {
    Padding {
        top: padding,
        right: padding,
        bottom: padding,
        left: padding,
    }
}

pub fn no_padding() -> Padding {
    all_round(0.0)
}
