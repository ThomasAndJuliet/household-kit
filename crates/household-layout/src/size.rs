// SPDX-License-Identifier: GPL-3.0-only

//! Sizes and points in `f32`, as the native geometry has them, and the
//! checks every size passes before the engine uses it.
//!
//! These are the core's own types, with the fields a toolkit's have, so
//! that nothing here names a toolkit. A toolkit layer converts at its edge
//! (household-widgets converts iced's `Size` and its `MeasuredSize`).

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
