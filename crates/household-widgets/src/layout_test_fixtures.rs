// SPDX-License-Identifier: GPL-3.0-only

//! What the tests of the engine's geometry share: native leaves whose size is
//! whatever function of their offer a test needs, and the numbers at the fit
//! tolerance.
//!
//! A `Shaped` widget lays out at a size computed from the largest size it is
//! offered (`layout::Limits::max`), so a test states its leaf as arithmetic:
//! a paragraph that takes more lines when narrower (`wrapping`), a leaf of a
//! fixed size that overflows a smaller offer (`sized`), a control with no
//! intrinsic width (`unbounded`). The engine measures a leaf by offering it
//! unbounded room, and plans it by offering it the width it is allocated and
//! unbounded height, or, for a viewport, the height it is offered.

use super::*;
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::{Font, Pixels};

pub(super) fn renderer() -> Renderer {
    cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
        Font::default(),
        Pixels(14.0),
        Some("tiny-skia"),
    ))
    .expect("headless tiny-skia renderer")
}

type Shape = Rc<dyn Fn(Size) -> Size>;

/// A native widget whose size is its shape applied to the largest size it is
/// offered.
struct Shaped(Shape);

impl Widget<(), Theme, Renderer> for Shaped {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::Node::new((self.0)(limits.max()))
    }
    fn draw(
        &self,
        _: &Tree,
        _: &mut Renderer,
        _: &Theme,
        _: &renderer::Style,
        _: Layout<'_>,
        _: mouse::Cursor,
        _: &Rectangle,
    ) {
    }
}

/// A view factory for a native view that takes `shape(offer)`.
pub(super) fn view_of(
    shape: impl Fn(Size) -> Size + 'static,
) -> impl Fn(Level) -> Element<'static, ()> {
    let shape: Shape = Rc::new(shape);
    move |_| Element::new(Shaped(shape.clone()))
}

/// A leaf whose native view takes `shape(offer)`.
pub(super) fn shaped(shape: impl Fn(Size) -> Size + 'static) -> Child<'static, ()> {
    child(view_of(shape))
}

/// A box `height` high that fills the width it is given (none, when the
/// width is unbounded), preferring `preferred` and usable down to `minimum`:
/// the part of `layout_arrangement_tests.rs`.
pub(super) fn part(preferred: f32, minimum: f32, height: f32) -> Child<'static, ()> {
    shaped(move |offer| {
        let width = if offer.width.is_finite() {
            offer.width
        } else {
            0.0
        };
        Size::new(width, height)
    })
    .minimum_width(move |_, _| minimum)
    .preferred_width(preferred)
}

/// The size of a paragraph laid out at most `width` wide: 120 wide on one
/// line of 10, and narrower, as many lines as 120 needs: 2 lines at 60 to
/// 119 wide, 3 at 40 to 59, 4 at 30 to 39.
pub(super) fn paragraph(width: f32) -> Size {
    let width = width.min(120.0);
    Size::new(width, 10.0 * (120.0 / width).ceil())
}

/// Content whose height depends on its width: the paragraph, usable from 30
/// wide. It measures one line, 120 × 10.
pub(super) fn wrapping() -> Child<'static, ()> {
    shaped(|offer| paragraph(offer.width)).minimum_width(|_, _| 30.0)
}

/// What the child measures at the preferred level.
pub(super) fn measure(child: &Child<'static, ()>) -> MeasuredSize {
    child.query(Level::Preferred, &renderer())
}

/// The child queried at the preferred level in a room `width` × `height`,
/// as a caller asks: the root's own allocation answers first.
pub(super) fn query_at(
    child: &Child<'static, ()>,
    width: f32,
    height: f32,
) -> Result<GeometryPlan, AllocationError> {
    child.query_at(Level::Preferred, &renderer(), Size::new(width, height))
}

/// Each placement as (declared index, [x, y], [width, height]).
pub(super) fn placed(plan: &GeometryPlan) -> Vec<(usize, [f32; 2], [f32; 2])> {
    plan.children
        .iter()
        .map(|placed| {
            (
                placed.index,
                [placed.position.x, placed.position.y],
                [placed.geometry.size.width, placed.geometry.size.height],
            )
        })
        .collect()
}

pub(super) fn no_fit(required: f32, available: f32) -> AllocationError {
    AllocationError::NoFit {
        required,
        available,
    }
}
