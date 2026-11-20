// SPDX-License-Identifier: GPL-3.0-only

//! Sizes, points and rectangles in `f32`, as the native geometry has them,
//! and the checks every size passes before the engine uses it.
//!
//! These are the core's own types, with the fields a toolkit's have, so
//! that nothing here names a toolkit. A toolkit layer converts at its edge
//! (household-widgets converts iced's `Size`, its `MeasuredSize` and iced's
//! `Rectangle`).

use crate::AllocationError;

/// A width and a height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    /// No width and no height.
    pub const ZERO: Self = Self::new(0.0, 0.0);

    /// Unbounded in both dimensions: room without a limit.
    pub const INFINITE: Self = Self::new(f32::INFINITY, f32::INFINITY);

    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// A position: `x` from the left, `y` from the top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A rectangle: its top left corner (`x`, `y`) and its size. Placing a
/// popup or a hover card is geometry over rectangles: the window, the
/// anchor, the region a candidate offers and the bounds placed in it
/// (`attach`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rectangle {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rectangle {
    pub const fn new(position: Point, size: Size) -> Self {
        Self {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        }
    }

    /// A rectangle of `size` at the origin.
    pub const fn with_size(size: Size) -> Self {
        Self::new(Point::new(0.0, 0.0), size)
    }

    /// Its top left corner.
    pub fn position(self) -> Point {
        Point::new(self.x, self.y)
    }

    /// Its width and height.
    pub fn size(self) -> Size {
        Size::new(self.width, self.height)
    }

    /// Where it overlaps `other`, if they overlap with an area: none when
    /// they only touch along an edge or at a corner. The arithmetic is the
    /// toolkit's (iced's `Rectangle::intersection`), so a host's answers
    /// are the same either way.
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);

        let lower_right_x = (self.x + self.width).min(other.x + other.width);
        let lower_right_y = (self.y + self.height).min(other.y + other.height);

        let width = lower_right_x - x;
        let height = lower_right_y - y;

        if width > 0.0 && height > 0.0 {
            Some(Self {
                x,
                y,
                width,
                height,
            })
        } else {
            None
        }
    }
}

/// Preferred and useful minimum geometry of a component at one exact level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measured {
    pub preferred: Size,
    pub minimum: Size,
}

/// Whether a size is one a part can have: both dimensions finite and not
/// negative.
pub fn finite_size(size: Size) -> bool {
    size.width.is_finite() && size.height.is_finite() && size.width >= 0.0 && size.height >= 0.0
}

/// Whether a size is room a part can be offered: neither dimension NaN or
/// negative. Unlike a part's size, room may be unbounded (infinite).
pub fn valid_available(size: Size) -> bool {
    !size.width.is_nan() && !size.height.is_nan() && size.width >= 0.0 && size.height >= 0.0
}

/// A measurement the engine can plan with: both sizes finite and not
/// negative, and the minimum no larger than the preferred size in either
/// dimension. Anything else is `InvalidInput`.
pub fn validate_measurement(size: Measured) -> Result<(), AllocationError> {
    if finite_size(size.preferred)
        && finite_size(size.minimum)
        && size.minimum.width <= size.preferred.width
        && size.minimum.height <= size.preferred.height
    {
        Ok(())
    } else {
        Err(AllocationError::InvalidInput)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_is_finite_and_not_negative_and_room_may_be_unbounded() {
        for bad in [f32::NAN, f32::INFINITY, -1.0] {
            assert!(!finite_size(Size::new(bad, 1.0)));
            assert!(!finite_size(Size::new(1.0, bad)));
        }
        assert!(finite_size(Size::ZERO));
        for bad in [f32::NAN, f32::NEG_INFINITY, -1.0] {
            assert!(!valid_available(Size::new(bad, 1.0)));
            assert!(!valid_available(Size::new(1.0, bad)));
        }
        assert!(valid_available(Size::new(f32::INFINITY, f32::INFINITY)));
    }

    #[test]
    fn a_measurement_is_valid_only_with_its_minimum_within_its_preference() {
        let measured = |preferred: Size, minimum: Size| Measured { preferred, minimum };
        let size = Size::new(10.0, 20.0);
        assert_eq!(validate_measurement(measured(size, size)), Ok(()));
        assert_eq!(validate_measurement(measured(size, Size::ZERO)), Ok(()));
        for (preferred, minimum) in [
            (size, Size::new(11.0, 20.0)),
            (size, Size::new(10.0, 21.0)),
            (Size::new(f32::NAN, 20.0), Size::ZERO),
            (size, Size::new(-1.0, 0.0)),
            (Size::new(10.0, f32::INFINITY), Size::ZERO),
        ] {
            assert_eq!(
                validate_measurement(measured(preferred, minimum)),
                Err(AllocationError::InvalidInput)
            );
        }
    }
}
