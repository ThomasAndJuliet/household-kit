// SPDX-License-Identifier: GPL-3.0-only

//! The plan of a part, and planning a part in a room: where each part goes,
//! and whether it all fits.

use crate::part::{Child, Kind, LeafHeight};
use crate::{
    AllocationError, FIT_EPSILON, Host, Level, Offer, Point, Size, allocate_one, finite_size,
    valid_available, validate_measurement,
};

/// Where a part and everything in it goes, at one level and in one room.
///
/// A plan is geometry only. It holds nothing of the host's: a host that
/// keeps more on a plan (household-widgets binds each node to the part
/// that made it, with that part's reports) makes its own plan from this
/// one, the same tree.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub(crate) size: Size,
    pub(crate) hidden: bool,
    pub(crate) children: Vec<Placed>,
    // Row membership is part of the plan, not recomputed by the host.
    pub(crate) rows: Vec<Vec<usize>>,
    pub(crate) minimum_height: f32,
    pub(crate) leaf_size: Option<Size>,
}

/// A part's plan, where its parent puts it.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub(crate) index: usize,
    pub(crate) position: Point,
    pub(crate) plan: Plan,
}

impl Plan {
    pub(crate) fn leaf(size: Size) -> Self {
        Self {
            size,
            hidden: false,
            children: Vec::new(),
            rows: Vec::new(),
            minimum_height: size.height,
            leaf_size: None,
        }
    }

    pub(crate) fn hidden() -> Self {
        Self {
            hidden: true,
            ..Self::leaf(Size::ZERO)
        }
    }

    /// The part's size: the room it takes.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Whether the part is hidden at the level: it takes no room, and
    /// nothing below it was planned.
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    /// The parts placed in this one, in order: a composition's visible
    /// children, or a frame's content. Forms place nothing: the family's
    /// plan is its selected form's.
    pub fn children(&self) -> &[Placed] {
        &self.children
    }

    /// A row's or a wrap's rows, each the indices in `children` of its
    /// placements, in order; empty for a column or anything else.
    pub fn rows(&self) -> &[Vec<usize>] {
        &self.rows
    }

    /// Useful minimum height after measuring this form at its offered width.
    /// Divider constraints must use this width-dependent result for wrapped
    /// children rather than an unconstrained preferred height.
    pub fn minimum_height(&self) -> f32 {
        self.minimum_height
    }

    /// For a leaf's plan, the size the host gave the leaf in its offer
    /// (`Host::size`), which a host lays the leaf out to again when it
    /// draws the plan; for any other part, none.
    pub fn leaf_size(&self) -> Option<Size> {
        self.leaf_size
    }
}

impl Placed {
    /// The part's index among its parent's declared children (hidden
    /// children are omitted from the placements, so an index may be
    /// skipped); a frame's content is its 0.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Where the part goes: its top left corner, from its parent's.
    pub fn position(&self) -> Point {
        self.position
    }

    /// The part's own plan.
    pub fn plan(&self) -> &Plan {
        &self.plan
    }
}

impl<'a, H: Host> Child<'a, H> {
    /// Allocate first, then measure leaves at their allocated width with
    /// unbounded height. Thus a clipped height can never certify a fit. A
    /// useful minimum narrower than intrinsic width must be explicitly
    /// declared.
    // Planning makes the plan; a host that binds it to the parts it came
    // from does so once it is finished, on the plan this returns.
    pub fn query_at(
        &self,
        level: Level,
        context: &H::Context,
        available: Size,
    ) -> Result<Plan, AllocationError> {
        if !valid_available(available) {
            return Err(AllocationError::InvalidInput);
        }
        if self.is_hidden(level) {
            return Ok(Plan::hidden());
        }
        let measured = self.query(level, context);
        validate_measurement(measured)?;
        let allocated = allocate_one(
            available.width,
            self.width_capacity(measured),
            self.sharing.grow,
        )?;
        let width = if self.has_composition(level) && available.width.is_finite() {
            available.width
        } else {
            allocated
        };
        self.plan_width(level, context, width, available.height)
    }

    /// Plan this child in `width` × `height`, as its parent does. A child
    /// planned here inherits no growth in height: it is the root, or a
    /// composition's child, which a composition plans by its own
    /// (`grow_height_in_effect`).
    pub fn plan_width(
        &self,
        level: Level,
        context: &H::Context,
        width: f32,
        height: f32,
    ) -> Result<Plan, AllocationError> {
        self.plan_inheriting(level, context, width, height, 0.0)
    }

    /// The growth in height in effect for this child, when the forms or the
    /// frame round it pass it `inherited`: the inherited growth when it is
    /// above zero, else the child's own. Planning (`plan_inheriting`) and
    /// a host drawing the plan both read it here, so they agree.
    ///
    /// How far an inherited growth reaches. Forms pass the growth in effect
    /// for the family to the form they select, and a frame passes its own to
    /// its content; a form or content that is itself forms or a frame passes
    /// on the growth in effect for it in turn, so a family's or a frame's
    /// growth above zero reaches through any chain of forms and frames to
    /// the first child that is neither. A composition does not pass it on:
    /// its children are planned as a composition's children (`plan_width`),
    /// and a column shares its height among them by each one's own
    /// `grow_height` (`Composition::plan_column`). An inherited growth of
    /// zero leaves the child its own.
    ///
    /// (Before this was an argument, forms and frames planned and rendered
    /// a clone of the form or the content with its `grow_height` overwritten
    /// when theirs was above zero. The clone shared the child's note, so
    /// household-widgets bound its plans as the child's, as it does now.)
    pub fn grow_height_in_effect(&self, inherited: f32) -> f32 {
        if inherited > 0.0 {
            inherited
        } else {
            self.sharing.grow_height
        }
    }

    /// Plan this child in `width` × `height`, with the growth in height the
    /// forms or the frame round it pass it (`grow_height_in_effect`).
    pub(crate) fn plan_inheriting(
        &self,
        level: Level,
        context: &H::Context,
        width: f32,
        height: f32,
        inherited_growth: f32,
    ) -> Result<Plan, AllocationError> {
        if self.is_hidden(level) {
            return Ok(Plan::hidden());
        }
        if self.invalid {
            // An invalid child has no plan (`invalid_unless`).
            return Err(AllocationError::InvalidInput);
        }
        let fixed = self.sharing.fixed.sizes(|| self.query(level, context));
        let width = match fixed.width {
            Some(requested) => {
                if width + FIT_EPSILON < requested {
                    return Err(AllocationError::NoFit {
                        required: requested,
                        available: width,
                    });
                }
                requested
            }
            None => width,
        };
        let height = match fixed.height {
            Some(requested) => {
                if height + FIT_EPSILON < requested {
                    return Err(AllocationError::NoFit {
                        required: requested,
                        available: height,
                    });
                }
                requested
            }
            None => height,
        };
        let grow_height = self.grow_height_in_effect(inherited_growth);
        let mut plan = match &self.kind {
            Kind::Forms(forms) => {
                forms
                    .select(level)
                    .plan_inheriting(level, context, width, height, grow_height)?
            }
            Kind::Composition(composition) => composition.plan(level, context, width, height)?,
            Kind::Frame(frame) => frame.plan(level, context, width, height, grow_height)?,
            Kind::Leaf(leaf) => {
                // How the leaf takes height, decided once: the height it is
                // offered, how it is to use that height (`Offer`), and, for
                // a leaf that takes a range of heights, the floor its plan
                // records. An ordinary leaf is offered unbounded height and
                // takes the height it needs, which its plan keeps as both
                // its height and its floor. A leaf that takes a range is
                // offered a height between its floor and the room (all of
                // the room when it grows) and must fit in it as offered.
                let (offered_height, offer, viewport_floor): (f32, fn(Size) -> Offer, _) =
                    match &leaf.height {
                        LeafHeight::Floor => (f32::INFINITY, Offer::NeededHeight, None),
                        LeafHeight::Range { minimum: floor } => {
                            let minimum = floor(level, context);
                            let measured = self.query(level, context);
                            let offered = if grow_height > 0.0 && height.is_finite() {
                                height.max(minimum.height)
                            } else {
                                measured.preferred.height.min(height).max(minimum.height)
                            };
                            (offered, Offer::OfferedHeight, Some(floor))
                        }
                    };
                let size = H::size(
                    &leaf.content,
                    level,
                    context,
                    offer(Size::new(width, offered_height)),
                );
                if !finite_size(size) {
                    return Err(AllocationError::InvalidInput);
                }
                if size.width > width + FIT_EPSILON {
                    return Err(AllocationError::NoFit {
                        required: size.width,
                        available: width,
                    });
                }
                // Never true for an ordinary leaf: its size is finite here and
                // its offer unbounded.
                if size.height > offered_height + FIT_EPSILON {
                    return Err(AllocationError::NoFit {
                        required: size.height,
                        available: offered_height,
                    });
                }
                // The parent reserves the allocated width even for shrink content.
                let mut plan = Plan::leaf(Size::new(width, size.height));
                plan.leaf_size = Some(size);
                // The floor is asked for again after the leaf is measured,
                // as it always has been.
                if let Some(floor) = viewport_floor {
                    plan.size.height = offered_height;
                    plan.minimum_height = floor(level, context).height;
                }
                plan
            }
        };
        // For forms, `plan` is the node their form made, and nothing below
        // changes it. A forms child never has a fixed dimension or a reserved
        // width: `fixed_dimensions` and `fill_width` put the child they are
        // given in a column, whose node is another. And the form was planned
        // in this same width and height, with the family's growth in height
        // when that is above zero, so it has already raised its height to
        // the room and passed both fit checks below. That is why a host that
        // binds the finished plan (household-widgets' `bind_geometry`) can
        // give the form's report the node it would have been given here.
        if fixed.width.is_some() || self.sharing.reserve_width {
            plan.size.width = width;
        }
        if (grow_height > 0.0 || fixed.height.is_some()) && height.is_finite() {
            plan.size.height = plan.size.height.max(height);
        }
        if let Some(requested) = fixed.height {
            plan.minimum_height = plan.minimum_height.max(requested);
        }
        if plan.size.width > width + FIT_EPSILON {
            return Err(AllocationError::NoFit {
                required: plan.size.width,
                available: width,
            });
        }
        if plan.size.height > height + FIT_EPSILON {
            return Err(AllocationError::NoFit {
                required: plan.size.height,
                available: height,
            });
        }
        Ok(plan)
    }
}
