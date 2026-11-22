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

/// Why a plan failed, and which part the failure is about: the engine's
/// error ([`AllocationError`]), with the path from the part that was asked
/// down to that part (`Child::query_at_blaming`).
///
/// **The path.** One step for each placement on the way down, by the rule a
/// plan's placements follow ([`Placed::index`]): a composition's child is
/// its index among the composition's declared children, hidden or not, and
/// a frame's content is 0. Forms add no step: a family and the form it
/// selects at the level make one node, which the form plans in the
/// family's place. An empty path is the part that was asked. A host turns
/// a path back into the part with `Child::part_at`.
///
/// **Blame.** Each check that can fail a plan is about one part, and the
/// failure names that part. A check is made by the part being planned, on
/// its own size or on one of its children's: whether a child's measurement
/// is a valid size, or whether a child's floor fits the width of a row, a
/// wrap or a column, is about that child; whether the gaps fit, or whether
/// a row's or a column's children together fit the room
/// (`allocate_with_growth`), is about the composition itself; whether a
/// leaf fits its offer, a frame its padding, or a part its fixed size or
/// its room, is about that part. A failure arises with an empty path at the
/// part that made the check, and each composition and frame it passes up
/// through, on receiving it from a child it was planning, puts that child's
/// step in front. So a failure is blamed where it was found, not where its
/// cause began: a part whose measurement is not a valid size because
/// something inside it is invalid is blamed at the first check that sees
/// the measurement, which for the part asked is its own (`query_at` checks
/// the root's measurement before planning anything). And because a
/// composition's floor of width is made of its children's floors, a room
/// too narrow for a part deep inside is usually found by the first part
/// above it whose floor is checked against that room, often the part asked.
///
/// **Cost.** On success, none: planning carries a `Result<Plan, Failure>`
/// where it carried a `Result<Plan, AllocationError>`, and nothing about a
/// path is made until a check fails. On a failure, the path: an empty path
/// allocates nothing, and each step put in front of it may (the first
/// allocates; the path is as long as the part is deep). A stage that does
/// not fit is the ordinary case in the apps, which try their stages in
/// order, so that is what a stage that does not fit costs beyond what it
/// cost before: a few words of memory for each part it is nested in.
///
/// The public planning entries `Child::query_at` and `Child::plan_width`
/// return this failure's error and nothing else, so what they return, and
/// when, is what it was before failures had paths.
///
/// **An example.** A row of two boxes, no gap between them: an icon that
/// keeps its 40 (weight zero), and a label that prefers 120 and cannot go
/// below 80. Each would fit a room 100 wide on its own; together they need
/// 40 + 80 = 120, which is the row's floor of width. In a room of 100 the
/// row is checked against the room before either child is planned, so the
/// failure is the row's: 120 required, 100 available, and an empty path.
/// It is not the label's, though the label is the part that cannot give
/// way: nothing about the label alone fails. In a room of 130 the row fits
/// in width (the icon 40, the label 90), but the room is only 8 high, and
/// the first child planned in it, the icon, 10 high, is too tall: 10
/// required, 8 available, at path `[0]`.
///
/// ```
/// use household_layout::{AllocationError, Child, Host, Level, Offer, Size};
///
/// /// A box of its own height, as wide as the room it is given, or as wide
/// /// as it would like in unbounded room.
/// struct Boxes;
///
/// impl Host for Boxes {
///     type Leaf = Size;
///     type Context = ();
///     type Note = &'static str;
///     type Chrome = ();
///
///     fn size(leaf: &Size, _: Level, _: &(), offer: Offer) -> Size {
///         let (Offer::NeededHeight(room) | Offer::OfferedHeight(room)) = offer;
///         let width = if room.width.is_finite() { room.width } else { leaf.width };
///         Size::new(width, leaf.height)
///     }
/// }
///
/// let named = |name, mut part: Child<'static, Boxes>| {
///     *part.note_mut() = name;
///     part
/// };
/// let icon = named("icon", Child::leaf(Size::new(40.0, 10.0)).weight(0.0));
/// let label = named(
///     "label",
///     Child::leaf(Size::new(120.0, 10.0)).minimum_width(|_, _| 80.0),
/// );
/// let row = named("row", Child::row(vec![icon, label], 0.0));
///
/// let failure = row
///     .query_at_blaming(Level::Preferred, &(), Size::new(100.0, 50.0))
///     .unwrap_err();
/// assert_eq!(
///     failure.error(),
///     AllocationError::NoFit { required: 120.0, available: 100.0 },
/// );
/// assert!(failure.path().is_empty());
/// let part = row.part_at(Level::Preferred, failure.path()).unwrap();
/// assert_eq!(*part.note(), "row");
/// // `query_at` fails with the same error, without the part.
/// assert_eq!(
///     row.query_at(Level::Preferred, &(), Size::new(100.0, 50.0)),
///     Err(failure.error()),
/// );
///
/// let failure = row
///     .query_at_blaming(Level::Preferred, &(), Size::new(130.0, 8.0))
///     .unwrap_err();
/// assert_eq!(
///     failure.error(),
///     AllocationError::NoFit { required: 10.0, available: 8.0 },
/// );
/// assert_eq!(failure.path(), [0]);
/// let part = row.part_at(Level::Preferred, failure.path()).unwrap();
/// assert_eq!(*part.note(), "icon");
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
    error: AllocationError,
    path: Vec<usize>,
}

impl Failure {
    /// The engine's error: what was required and what there was, or that
    /// the input was invalid.
    pub fn error(&self) -> AllocationError {
        self.error
    }

    /// The path from the part that was asked to the part the failure is
    /// about; empty for the part asked.
    pub fn path(&self) -> &[usize] {
        &self.path
    }

    /// The error alone, as `Child::query_at` returns it.
    pub fn into_error(self) -> AllocationError {
        self.error
    }

    /// This failure, received from the child at `step` (its index among a
    /// composition's declared children, or 0 for a frame's content), as the
    /// part planning that child passes it up: the child's step in front.
    pub(crate) fn in_child(mut self, step: usize) -> Self {
        self.path.insert(0, step);
        self
    }
}

/// A failure of the part being planned itself: the error, with an empty
/// path. Every check that fails with an `AllocationError` makes one so,
/// the `?` on the engine's own checks included.
impl From<AllocationError> for Failure {
    fn from(error: AllocationError) -> Self {
        Self {
            error,
            path: Vec::new(),
        }
    }
}

/// A failure of the part making the check: `required` does not fit in
/// `available`. A check about one of the part's children gives the child's
/// step to it (`Failure::in_child`).
pub(crate) fn no_fit(required: f32, available: f32) -> Failure {
    AllocationError::NoFit {
        required,
        available,
    }
    .into()
}

impl<'a, H: Host> Child<'a, H> {
    /// Allocate first, then measure leaves at their allocated width with
    /// unbounded height. Thus a clipped height can never certify a fit. A
    /// useful minimum narrower than intrinsic width must be explicitly
    /// declared.
    ///
    /// The error is the one `query_at_blaming` finds, without the part it
    /// is about.
    // Planning makes the plan; a host that binds it to the parts it came
    // from does so once it is finished, on the plan this returns.
    pub fn query_at(
        &self,
        level: Level,
        context: &H::Context,
        available: Size,
    ) -> Result<Plan, AllocationError> {
        self.query_at_blaming(level, context, available)
            .map_err(Failure::into_error)
    }

    /// Plan this child at `level` in `available`, as `query_at` does, and
    /// when it does not fit, say which part the failure is about
    /// ([`Failure`]): the same plan, or the same error with its path.
    ///
    /// The room, the root's measurement and the root's floor of width are
    /// checked here, before anything is planned, and each is about the
    /// part asked: a room that is not a size, a measurement that is not a
    /// valid size, a floor wider than the room.
    pub fn query_at_blaming(
        &self,
        level: Level,
        context: &H::Context,
        available: Size,
    ) -> Result<Plan, Failure> {
        if !valid_available(available) {
            return Err(AllocationError::InvalidInput.into());
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
        self.plan_blaming(level, context, width, available.height)
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
        self.plan_blaming(level, context, width, height)
            .map_err(Failure::into_error)
    }

    /// `plan_width` with the failure's path: how a composition plans its
    /// children, and puts the child's step in front of what fails there.
    pub(crate) fn plan_blaming(
        &self,
        level: Level,
        context: &H::Context,
        width: f32,
        height: f32,
    ) -> Result<Plan, Failure> {
        self.plan_inheriting(level, context, width, height, 0.0)
    }

    /// The part at `path` below this one at `level`: the part a failure's
    /// path names (`Failure::path`), or a node's in a walk of a plan
    /// (`Child::walk`). An empty path is this part; a step that names no
    /// part (an index past a composition's children, a step other than 0
    /// into a frame, any step into a leaf) gives none.
    ///
    /// Descending is by the rule plans follow: a step into a composition is
    /// its child at that index among the declared children, a step into a
    /// frame is its content, and a step into forms is taken in the form
    /// selected at `level` (through forms of forms, to the first that is
    /// not), since the family adds no step of its own. Nothing here asks
    /// whether a part is hidden: a path from a failure or a walk never
    /// passes through a hidden part.
    ///
    /// The part at the end of the path is the one its parent placed there.
    /// Where that is a family, it is the family, not its form: the two make
    /// one node, and the family's form is one step of reading away
    /// (`Kind::Forms`, `Forms::select`), while from the form there is no
    /// way back to the family. A host whose note is on a family finds it
    /// here; one whose note is on the form looks inside.
    ///
    /// When the part named carries no note of use to the host, the nearest
    /// part that does is the one to name: inside it first, through the
    /// forms it holds to the form selected, which made the same node; then
    /// up the path, the part at the path one step shorter and the forms
    /// it holds, and so on to this part (an empty path). The charts name
    /// the nearest part that has an identity, and with none at all the
    /// failure is the whole tree's.
    pub fn part_at(&self, level: Level, path: &[usize]) -> Option<&Self> {
        let mut part = self;
        for &step in path {
            let mut holder = part;
            while let Kind::Forms(forms) = &holder.kind {
                holder = forms.select(level);
            }
            part = match &holder.kind {
                Kind::Composition(composition) => composition.children.get(step)?,
                Kind::Frame(frame) if step == 0 => &frame.content,
                Kind::Frame(_) | Kind::Leaf(_) | Kind::Forms(_) => return None,
            };
        }
        Some(part)
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
    ///
    /// Every check made here is about this child, so each fails with an
    /// empty path (`Failure`): its mark of invalid, its fixed size, a leaf's
    /// size against its offer, and its plan against its room. What fails
    /// below comes up with its path as it was made: a composition's and a
    /// frame's planning give it their child's step, and forms add none, the
    /// form being planned in the family's place.
    pub(crate) fn plan_inheriting(
        &self,
        level: Level,
        context: &H::Context,
        width: f32,
        height: f32,
        inherited_growth: f32,
    ) -> Result<Plan, Failure> {
        if self.is_hidden(level) {
            return Ok(Plan::hidden());
        }
        if self.invalid {
            // An invalid child has no plan (`invalid_unless`).
            return Err(AllocationError::InvalidInput.into());
        }
        let fixed = self.sharing.fixed.sizes(|| self.query(level, context));
        let width = match fixed.width {
            Some(requested) => {
                if width + FIT_EPSILON < requested {
                    return Err(no_fit(requested, width));
                }
                requested
            }
            None => width,
        };
        let height = match fixed.height {
            Some(requested) => {
                if height + FIT_EPSILON < requested {
                    return Err(no_fit(requested, height));
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
                    return Err(AllocationError::InvalidInput.into());
                }
                if size.width > width + FIT_EPSILON {
                    return Err(no_fit(size.width, width));
                }
                // Never true for an ordinary leaf: its size is finite here and
                // its offer unbounded.
                if size.height > offered_height + FIT_EPSILON {
                    return Err(no_fit(size.height, offered_height));
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
            return Err(no_fit(plan.size.width, width));
        }
        if plan.size.height > height + FIT_EPSILON {
            return Err(no_fit(plan.size.height, height));
        }
        Ok(plan)
    }
}
