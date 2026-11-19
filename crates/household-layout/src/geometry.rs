// SPDX-License-Identifier: GPL-3.0-only

//! Points, sizes and rectangles, in `f64`. The y axis runs downwards, as
//! on a page and in a window.

/// How far two lengths may differ and still count as equal when a fit is
/// tested. household-widgets tests a fit to the same tolerance.
pub const FIT_EPSILON: f64 = 0.001;

/// A position.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// A width and a height.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    /// No limit in either direction.
    pub const INFINITE: Self = Self::new(f64::INFINITY, f64::INFINITY);

    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    /// Whether both lengths are numbers that are not negative; infinite
    /// lengths are allowed, since a space may be unlimited.
    pub fn is_valid(self) -> bool {
        !self.width.is_nan() && !self.height.is_nan() && self.width >= 0.0 && self.height >= 0.0
    }

    /// Whether `self` fits in `space`, to [`FIT_EPSILON`].
    pub fn fits(self, space: Size) -> bool {
        self.width <= space.width + FIT_EPSILON && self.height <= space.height + FIT_EPSILON
    }
}

/// A rectangle with its top left corner at (`x`, `y`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn from_origin(origin: Point, size: Size) -> Self {
        Self::new(origin.x, origin.y, size.width, size.height)
    }

    pub fn origin(self) -> Point {
        Point::new(self.x, self.y)
    }

    pub fn size(self) -> Size {
        Size::new(self.width, self.height)
    }

    pub fn right(self) -> f64 {
        self.x + self.width
    }

    pub fn bottom(self) -> f64 {
        self.y + self.height
    }

    pub fn centre(self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Whether every length is finite and neither side is negative.
    pub fn is_valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width >= 0.0
            && self.height >= 0.0
    }

    /// Whether `other` lies wholly inside `self`, to [`FIT_EPSILON`].
    pub fn contains(self, other: Rect) -> bool {
        other.x >= self.x - FIT_EPSILON
            && other.y >= self.y - FIT_EPSILON
            && other.right() <= self.right() + FIT_EPSILON
            && other.bottom() <= self.bottom() + FIT_EPSILON
    }

    /// Whether `point` lies inside `self` or on its edge.
    pub fn holds(self, point: Point) -> bool {
        point.x >= self.x
            && point.x <= self.right()
            && point.y >= self.y
            && point.y <= self.bottom()
    }

    /// The overlap of two rectangles, if it has any area.
    pub fn intersection(self, other: Rect) -> Option<Rect> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        (right > x && bottom > y).then(|| Rect::new(x, y, right - x, bottom - y))
    }

    /// The shortest distance between two rectangles' edges: zero where
    /// they touch or overlap.
    pub fn gap(self, other: Rect) -> f64 {
        let dx = (other.x - self.right())
            .max(self.x - other.right())
            .max(0.0);
        let dy = (other.y - self.bottom())
            .max(self.y - other.bottom())
            .max(0.0);
        dx.hypot(dy)
    }

    /// `self` grown by `by` on every side (shrunk where `by` is negative,
    /// never below no size).
    pub fn grown(self, by: f64) -> Rect {
        let width = (self.width + 2.0 * by).max(0.0);
        let height = (self.height + 2.0 * by).max(0.0);
        Rect::new(
            self.centre().x - width / 2.0,
            self.centre().y - height / 2.0,
            width,
            height,
        )
    }

    pub fn translated(self, by: Point) -> Rect {
        Rect::new(self.x + by.x, self.y + by.y, self.width, self.height)
    }
}

/// One of a box's two directions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Left to right.
    Across,
    /// Top to bottom on the page.
    Down,
}

impl Direction {
    pub fn other(self) -> Self {
        match self {
            Self::Across => Self::Down,
            Self::Down => Self::Across,
        }
    }

    /// A size's length in this direction.
    pub fn length(self, size: Size) -> f64 {
        match self {
            Self::Across => size.width,
            Self::Down => size.height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gap_is_measured_between_the_nearest_edges() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert_eq!(a.gap(Rect::new(13.0, 0.0, 5.0, 5.0)), 3.0);
        assert_eq!(a.gap(Rect::new(13.0, 14.0, 5.0, 5.0)), 5.0);
        assert_eq!(a.gap(Rect::new(5.0, 5.0, 10.0, 10.0)), 0.0);
    }

    #[test]
    fn an_intersection_needs_area() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert_eq!(a.intersection(Rect::new(10.0, 0.0, 5.0, 5.0)), None);
        assert_eq!(
            a.intersection(Rect::new(8.0, 8.0, 5.0, 5.0)),
            Some(Rect::new(8.0, 8.0, 2.0, 2.0))
        );
    }

    #[test]
    fn growing_keeps_the_centre() {
        let grown = Rect::new(10.0, 10.0, 4.0, 2.0).grown(1.0);
        assert_eq!(grown, Rect::new(9.0, 9.0, 6.0, 4.0));
        assert_eq!(
            Rect::new(0.0, 0.0, 2.0, 2.0).grown(-5.0).size(),
            Size::new(0.0, 0.0)
        );
    }
}
