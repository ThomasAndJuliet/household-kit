// SPDX-License-Identifier: GPL-3.0-only

//! Explicit chrome around an ordinary immediate child, using its existing plan.
use super::*;
use cosmic::iced::Padding;

pub(super) struct Frame<'a, Message> {
    pub content: Child<'a, Message>,
    padding: Padding,
    decorate: ViewMap<'a, Message>,
}

/// Add declared native chrome without flattening a box into a fixed-size leaf.
/// The decorator must add exactly `padding` around its supplied content; native
/// verification rejects disagreement. Include border/shadow allowance here or
/// in another declared outer frame. All geometry, forms, minima, source facts
/// and requested levels of the immediate child remain part of the same engine.
/// Hidden content omits the entire frame and its spacing. Padding must be
/// finite and nonnegative; any other makes the frame invalid
/// (`Child::invalid_unless`).
pub fn framed_child<'a, Message: 'a>(
    content: Child<'a, Message>,
    padding: impl Into<Padding>,
    decorate: impl Fn(Element<'a, Message>) -> Element<'a, Message> + 'a,
) -> Child<'a, Message> {
    let padding = padding.into();
    let valid = [padding.left, padding.right, padding.top, padding.bottom]
        .into_iter()
        .all(is_amount);
    Child::new(Kind::Frame(Rc::new(Frame {
        content,
        padding,
        decorate: Rc::new(decorate),
    })))
    .invalid_unless(valid)
}

impl<'a, Message: 'a> Frame<'a, Message> {
    /// The content's measurement with the padding added round it. The
    /// frame's minimum width adds the padding to the width the content
    /// cannot go below (`WidthCapacity::least`): for content of weight zero,
    /// its preferred width, not its minimum.
    pub fn measure(&self, level: Level, renderer: &Renderer) -> MeasuredSize {
        let inner = self.content.query(level, renderer);
        let chrome = Size::new(self.padding.x(), self.padding.y());
        MeasuredSize {
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

    pub fn plan(
        &self,
        level: Level,
        renderer: &Renderer,
        width: f32,
        height: f32,
        grow_height: f32,
    ) -> Result<GeometryPlan, AllocationError> {
        let horizontal = self.padding.x();
        let vertical = self.padding.y();
        if width + FIT_EPSILON < horizontal {
            return Err(AllocationError::NoFit {
                required: horizontal,
                available: width,
            });
        }
        if height + FIT_EPSILON < vertical {
            return Err(AllocationError::NoFit {
                required: vertical,
                available: height,
            });
        }
        let mut content = self.content.clone();
        if grow_height > 0.0 {
            content.grow_height = grow_height;
        }
        let geometry = content.plan_width(
            level,
            renderer,
            (width - horizontal).max(0.0),
            (height - vertical).max(0.0),
        )?;
        let mut result = GeometryPlan::leaf(Size::new(
            geometry.size.width + horizontal,
            geometry.size.height + vertical,
        ));
        result.minimum_height = geometry.minimum_height + vertical;
        result.children.push(Placement {
            index: 0,
            position: cosmic::iced::Point::new(self.padding.left, self.padding.top),
            geometry,
        });
        Ok(result)
    }

    pub fn render(
        &self,
        level: Level,
        plan: &GeometryPlan,
        agreement: Rc<Cell<bool>>,
        grow_height: f32,
    ) -> Element<'a, Message> {
        // A frame's own plan places its content first (`plan`); a plan
        // without a placement was made for another kind of child, and the
        // frame refuses it as a leaf does a plan without its native size.
        let Some(Placement {
            geometry: inner, ..
        }) = plan.children.first()
        else {
            return refused(agreement);
        };
        let mut content = self.content.clone();
        if grow_height > 0.0 {
            content.grow_height = grow_height;
        }
        let native = content.render(level, inner, agreement.clone());
        let native = Element::new(VerifiedDecoration {
            content: native,
            expected: inner.size,
            agreement: agreement.clone(),
            check_constraints: true,
        });
        Element::new(VerifiedDecoration {
            content: (self.decorate)(native),
            expected: plan.size,
            agreement,
            check_constraints: false,
        })
    }
}
