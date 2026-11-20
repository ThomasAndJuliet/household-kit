// SPDX-License-Identifier: GPL-3.0-only

//! Explicit chrome around an ordinary immediate child, using its existing plan.
//!
//! The frame's geometry (its padding round its content, measured and
//! planned) is the engine's (`household_layout::Frame`); here are its
//! declaration with a native decorator as its chrome, and its rendering.
use super::*;
use cosmic::iced::Padding;

/// Add declared native chrome without flattening a box into a fixed-size leaf.
/// The decorator must add exactly `padding` around its supplied content; native
/// verification rejects disagreement. Include border/shadow allowance here or
/// in another declared outer frame. All geometry, forms, minima, source facts
/// and requested levels of the immediate child remain part of the same engine.
/// Hidden content omits the entire frame and its spacing. Padding must be
/// finite and nonnegative; any other makes the frame invalid
/// (`household_layout::Child::invalid_unless`).
pub fn framed_child<'a, Message: 'a>(
    content: Child<'a, Message>,
    padding: impl Into<Padding>,
    decorate: impl Fn(Element<'a, Message>) -> Element<'a, Message> + 'a,
) -> Child<'a, Message> {
    let padding = padding.into();
    let decorate: ViewMap<'a, Message> = Rc::new(decorate);
    Child::from_part(Part::frame(content.part, core_padding(padding), decorate))
}

/// Render the frame from its plan, its content with the growth in height it
/// was planned with: the engine's `grow_height_in_effect` for the frame.
pub(super) fn render<'a, Message: 'a>(
    frame: &household_layout::Frame<'a, Native<'a, Message>>,
    level: Level,
    plan: &GeometryPlan,
    agreement: Rc<Cell<bool>>,
    grow_height: f32,
) -> Element<'a, Message> {
    // A frame's own plan places its content first; a plan without a
    // placement was made for another kind of child, and the frame refuses
    // it as a leaf does a plan without its native size.
    let Some(Placement {
        geometry: inner, ..
    }) = plan.children.first()
    else {
        return refused(agreement);
    };
    let native = frame
        .content()
        .render_inheriting(level, inner, agreement.clone(), grow_height);
    let native = Element::new(VerifiedDecoration {
        content: native,
        expected: inner.size,
        agreement: agreement.clone(),
        check_constraints: true,
    });
    Element::new(VerifiedDecoration {
        content: (frame.chrome())(native),
        expected: plan.size,
        agreement,
        check_constraints: false,
    })
}
