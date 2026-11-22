// SPDX-License-Identifier: GPL-3.0-only

//! A frame: a fixed padding round one part, drawn by the host's chrome,
//! planned from the part's own plan.

use crate::part::Child;
use crate::plan::{Failure, Placed, Plan, no_fit};
use crate::{FIT_EPSILON, Host, Level, Measured, Point, Size};

/// Space round a part's four edges.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Padding {
    /// The padding across: left and right.
    pub fn x(self) -> f32 {
        self.left + self.right
    }

    /// The padding down: top and bottom.
    pub fn y(self) -> f32 {
        self.top + self.bottom
    }
}

/// A fixed padding round one child (`Child::frame`), and what the host
/// draws in it.
pub struct Frame<'a, H: Host> {
    pub(crate) content: Child<'a, H>,
    pub(crate) padding: Padding,
    pub(crate) chrome: H::Chrome,
}

impl<'a, H: Host> Frame<'a, H> {
    /// The child the frame holds.
    pub fn content(&self) -> &Child<'a, H> {
        &self.content
    }

    /// The padding round the content.
    pub fn padding(&self) -> Padding {
        self.padding
    }

    /// What the host draws the padding with.
    pub fn chrome(&self) -> &H::Chrome {
        &self.chrome
    }

    /// The content's measurement with the padding added round it. The
    /// frame's minimum width adds the padding to the width the content
    /// cannot go below (`WidthCapacity::least`): for content of weight zero,
    /// its preferred width, not its minimum.
    pub(crate) fn measure(&self, level: Level, context: &H::Context) -> Measured {
        let inner = self.content.query(level, context);
        let chrome = Size::new(self.padding.x(), self.padding.y());
        Measured {
            preferred: Size::new(
                inner.preferred.width + chrome.width,
                inner.preferred.height + chrome.height,
            ),
            minimum: Size::new(
                self.content.width_capacity(inner).least() + chrome.width,
                inner.minimum.height + chrome.height,
            ),
        }
    }

    /// Plan the frame in `width` × `height`: its content in what the
    /// padding leaves. `grow_height` is the growth in height in effect for
    /// the frame, which its content inherits
    /// (`Child::grow_height_in_effect`).
    ///
    /// A room smaller than the padding is the frame's own failure; a
    /// failure planning the content comes up with the content's step, 0, in
    /// front (`Failure`).
    pub(crate) fn plan(
        &self,
        level: Level,
        context: &H::Context,
        width: f32,
        height: f32,
        grow_height: f32,
    ) -> Result<Plan, Failure> {
        let horizontal = self.padding.x();
        let vertical = self.padding.y();
        if width + FIT_EPSILON < horizontal {
            return Err(no_fit(horizontal, width));
        }
        if height + FIT_EPSILON < vertical {
            return Err(no_fit(vertical, height));
        }
        let geometry = self
            .content
            .plan_inheriting(
                level,
                context,
                (width - horizontal).max(0.0),
                (height - vertical).max(0.0),
                grow_height,
            )
            .map_err(|failure| failure.in_child(0))?;
        let mut result = Plan::leaf(Size::new(
            geometry.size.width + horizontal,
            geometry.size.height + vertical,
        ));
        result.minimum_height = geometry.minimum_height + vertical;
        result.children.push(Placed {
            index: 0,
            position: Point::new(self.padding.left, self.padding.top),
            plan: geometry,
        });
        Ok(result)
    }
}
