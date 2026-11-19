// SPDX-License-Identifier: GPL-3.0-only

//! The household apps' layout engine, without a toolkit.
//!
//! This is household-widgets' engine (`layout.rs` and `layout_frame.rs`
//! there) translated item by item, with the engine's names, so that a chart
//! lays out exactly as a window does. Where the engine measures a native
//! widget, the port asks a [`Leaf`] the same two questions: its preferred
//! size and useful minimum at one exact level, and its size when laid out
//! in an offered space. Everything about live widgets is left out: views,
//! the verified commit, speculative measuring, native state, decorators,
//! popups, hovers, navigation regions and presentations.
//!
//! In short, as the engine works: at one exact level a component has a
//! preferred size and a useful minimum ([`MeasuredSize`]); a shortage is
//! shared by [`allocate_width`] and surplus by `grow`. Between levels the
//! change is discrete ([`forms`]). Only the root selects a level, and a
//! composite forwards that exact level to its children. A policy is a list
//! of [`Stage`]s, and [`choose_stage`] takes the first that fits. Widths
//! come first; each child is then planned at its width, which gives its
//! height.
//!
//! Three things are added, none of which changes a choice the engine
//! makes: a child may carry an [`Id`] and a [`Source`], which its node in
//! the plan carries ([`GeometryPlan::find`]); when no stage fits, [`NoFit`]
//! says for each stage what failed and in which identified child; and a
//! child may report what its form does not show
//! ([`Child::report_information`]), gathered as the engine gathers its
//! source reports.

use std::rc::Rc;

pub use crate::allocate::AllocationError;
use crate::allocate::{Capacity, allocate};
use crate::geometry::{FIT_EPSILON, Point, Size};
use crate::identity::{Id, Source};
pub use crate::level::Level;

/// Query representations without retaining the result of any rejected
/// query. `None` explicitly means that none of the permitted representations
/// fits. As in the engine, the fit here is exact, with no tolerance.
pub fn first_fitting(
    max_level: Level,
    available: Size,
    measure: impl Fn(Level) -> Size,
) -> Option<Level> {
    Level::ALL
        .into_iter()
        .take_while(|level| *level <= max_level)
        .find(|level| {
            let size = measure(*level);
            size.width.is_finite()
                && size.height.is_finite()
                && size.width >= 0.0
                && size.height >= 0.0
                && size.width <= available.width
                && size.height <= available.height
        })
}

/// Width a child prefers, its lower bound, and its willingness to shrink.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WidthCapacity {
    pub preferred: f64,
    pub minimum: f64,
    /// Higher weights shrink faster; zero protects the preferred width.
    pub weight: f64,
}

/// Share a shortage using each child's original `weight * (preferred-minimum)`
/// rate, clamping minima and redistributing at the remaining original rates.
/// Surplus space is left unallocated. No representation levels are scheduled.
pub fn allocate_width(
    available: f64,
    capacities: &[WidthCapacity],
) -> Result<Vec<f64>, AllocationError> {
    let capacities: Vec<_> = capacities
        .iter()
        .map(|capacity| Capacity::new(capacity.preferred, capacity.minimum, capacity.weight))
        .collect();
    allocate(available, &capacities)
}

/// Preferred and useful minimum geometry of a component at one exact level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasuredSize {
    pub preferred: Size,
    pub minimum: Size,
}

/// What the engine measures natively, supplied by the caller. Both answers
/// must be pure: they are asked for trial stages as well as the chosen one.
pub trait Leaf {
    /// The preferred size and useful minimum at one exact level.
    fn query(&self, level: Level) -> MeasuredSize;

    /// The size when laid out within `maximum` at `level`: the engine's
    /// native measurement at an allocated width. An infinite height means
    /// unbounded (the leaf takes its own height); a finite one is a
    /// declared viewport's offered height. A size wider or taller than
    /// `maximum` does not fit.
    fn layout(&self, level: Level, maximum: Size) -> Size;
}

/// An arrangement applies only to immediate children, which remain atomic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arrangement {
    Row,
    Column,
    Wrap,
}

/// What a child's form does not show.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LossKind {
    Shortened,
    Omitted,
}

/// One piece of information a form shortens or omits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InformationLoss {
    pub identity: Id,
    pub kind: LossKind,
}

/// A child's report on its planned form, with the child's identity. An
/// empty `losses` says plainly that the form shows everything.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceReport {
    pub id: Option<Id>,
    pub source: Option<Source>,
    pub level: Level,
    pub size: Size,
    pub losses: Vec<InformationLoss>,
}

/// Geometry proposed by a pure query.
#[derive(Clone, Debug, PartialEq)]
pub struct GeometryPlan {
    pub size: Size,
    pub hidden: bool,
    pub children: Vec<Placement>,
    // Row membership is part of the plan, not recomputed during commit.
    rows: Vec<Vec<usize>>,
    minimum_height: f64,
    native_size: Option<Size>,
    identities: Vec<(Id, Option<Source>)>,
    source_reports: Vec<SourceReport>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    /// Index in the declared immediate children (hidden children are omitted).
    pub index: usize,
    pub position: Point,
    pub geometry: GeometryPlan,
}

impl GeometryPlan {
    fn leaf(size: Size) -> Self {
        Self {
            size,
            hidden: false,
            children: Vec::new(),
            rows: Vec::new(),
            minimum_height: size.height,
            native_size: None,
            identities: Vec::new(),
            source_reports: Vec::new(),
        }
    }

    fn hidden() -> Self {
        Self {
            hidden: true,
            ..Self::leaf(Size::new(0.0, 0.0))
        }
    }

    /// Useful minimum height after measuring this form at its offered width.
    /// Divider constraints must use this width-dependent result for wrapped
    /// children rather than an unconstrained preferred height.
    pub fn minimum_height(&self) -> f64 {
        self.minimum_height
    }

    /// The placements in each row of a row or a wrap, as indices into
    /// `children`; empty for a column or a leaf.
    pub fn rows(&self) -> &[Vec<usize>] {
        &self.rows
    }

    /// For a leaf, the size the leaf answered at its offered space; the
    /// plan's own width is the allocated width.
    pub fn native_size(&self) -> Option<Size> {
        self.native_size
    }

    /// The identities of the children whose node this is: a form's own
    /// identity first, then that of the child that selected it.
    pub fn identities(&self) -> &[(Id, Option<Source>)] {
        &self.identities
    }

    /// The node carrying `id`, with its position relative to this plan.
    pub fn find(&self, id: &Id) -> Option<(Point, &GeometryPlan)> {
        if self.identities.iter().any(|(own, _)| own == id) {
            return Some((Point::new(0.0, 0.0), self));
        }
        self.children.iter().find_map(|placed| {
            placed.geometry.find(id).map(|(position, plan)| {
                (
                    Point::new(
                        placed.position.x + position.x,
                        placed.position.y + position.y,
                    ),
                    plan,
                )
            })
        })
    }

    /// Facts about this plan and its immediate-child subplans, recursively.
    pub fn reports(&self) -> Vec<&SourceReport> {
        let mut reports: Vec<_> = self.source_reports.iter().collect();
        for child in &self.children {
            reports.extend(child.geometry.reports());
        }
        reports
    }
}

/// Space added around a framed child: its chrome.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Padding {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Padding {
    pub const fn new(top: f64, right: f64, bottom: f64, left: f64) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// The horizontal chrome, left and right.
    pub fn x(self) -> f64 {
        self.left + self.right
    }

    /// The vertical chrome, top and bottom.
    pub fn y(self) -> f64 {
        self.top + self.bottom
    }
}

impl From<f64> for Padding {
    fn from(all: f64) -> Self {
        Self::new(all, all, all, all)
    }
}

/// `[vertical, horizontal]`, as the engine's padding.
impl From<[f64; 2]> for Padding {
    fn from([vertical, horizontal]: [f64; 2]) -> Self {
        Self::new(vertical, horizontal, vertical, horizontal)
    }
}

/// `[top, right, bottom, left]`.
impl From<[f64; 4]> for Padding {
    fn from([top, right, bottom, left]: [f64; 4]) -> Self {
        Self::new(top, right, bottom, left)
    }
}

struct Composition<'a> {
    children: Vec<Child<'a>>,
    arrangement: Arrangement,
    spacing: f64,
    top_aligned: bool,
}

struct Frame<'a> {
    content: Child<'a>,
    padding: Padding,
}

type SizeQuery<'a> = Rc<dyn Fn(Level) -> MeasuredSize + 'a>;
type MinimumQuery<'a> = Rc<dyn Fn(Level) -> Size + 'a>;
type InformationQuery<'a> = Rc<dyn Fn(Level, &GeometryPlan) -> Vec<InformationLoss> + 'a>;
type Forms<'a> = Rc<Vec<(Level, Child<'a>)>>;

/// What failed while planning, and the nearest identified child it came
/// from: the child itself, or the nearest enclosing child with an identity.
#[derive(Clone, Debug, PartialEq)]
struct Failure {
    error: AllocationError,
    child: Option<Id>,
}

impl From<AllocationError> for Failure {
    fn from(error: AllocationError) -> Self {
        Self { error, child: None }
    }
}

fn no_fit(required: f64, available: f64) -> Failure {
    AllocationError::NoFit {
        required,
        available,
    }
    .into()
}

/// A queryable leaf or composite. Only the root selects a level. Composites
/// forward that exact level; outward weights never replace internal weights.
#[derive(Clone)]
pub struct Child<'a> {
    native: Option<Rc<dyn Leaf + 'a>>,
    query: SizeQuery<'a>,
    composition: Option<Rc<Composition<'a>>>,
    frame: Option<Rc<Frame<'a>>>,
    forms: Option<Forms<'a>>,
    hidden: Rc<dyn Fn(Level) -> bool + 'a>,
    viewport: Option<MinimumQuery<'a>>,
    weight: f64,
    grow: f64,
    grow_height: f64,
    reserve_width: bool,
    fixed_width: Option<f64>,
    fixed_height: Option<f64>,
    identity: Option<(Id, Option<Source>)>,
    information: Option<InformationQuery<'a>>,
}

impl<'a> Child<'a> {
    fn new(native: Option<Rc<dyn Leaf + 'a>>, query: SizeQuery<'a>) -> Self {
        Self {
            native,
            query,
            composition: None,
            frame: None,
            forms: None,
            hidden: Rc::new(|_| false),
            viewport: None,
            weight: 1.0,
            grow: 0.0,
            grow_height: 0.0,
            reserve_width: false,
            fixed_width: None,
            fixed_height: None,
            identity: None,
            information: None,
        }
    }

    /// Give this child an identity, and the field of the input it draws if
    /// there is one; its node in every plan carries them.
    pub fn identity(mut self, id: Id, source: Option<Source>) -> Self {
        self.identity = Some((id, source));
        self
    }

    /// Report what this form does not show, from this exact candidate
    /// geometry. The callback is pure and runs for trial queries too.
    pub fn report_information(
        mut self,
        report: impl Fn(Level, &GeometryPlan) -> Vec<InformationLoss> + 'a,
    ) -> Self {
        self.information = Some(Rc::new(report));
        self
    }

    fn bind_geometry(&self, level: Level, mut plan: GeometryPlan) -> GeometryPlan {
        if let Some(identity) = &self.identity {
            plan.identities.push(identity.clone());
        }
        if !plan.hidden
            && let Some(report) = &self.information
        {
            let losses = report(level, &plan);
            plan.source_reports.push(SourceReport {
                id: self.identity.as_ref().map(|(id, _)| id.clone()),
                source: self
                    .identity
                    .as_ref()
                    .and_then(|(_, source)| source.clone()),
                level,
                size: plan.size,
                losses,
            });
        }
        plan
    }

    fn attribute(&self, mut failure: Failure) -> Failure {
        if failure.child.is_none() {
            failure.child = self.identity.as_ref().map(|(id, _)| id.clone());
        }
        failure
    }

    fn selected_form(&self, level: Level) -> Option<&Self> {
        self.forms.as_ref().map(|forms| select_form(forms, level))
    }

    fn has_composition(&self, level: Level) -> bool {
        self.composition.is_some()
            || self
                .frame
                .as_ref()
                .is_some_and(|frame| frame.content.has_composition(level))
            || self
                .selected_form(level)
                .is_some_and(|form| form.has_composition(level))
    }

    fn is_hidden(&self, level: Level) -> bool {
        (self.hidden)(level)
            || self
                .selected_form(level)
                .is_some_and(|form| form.is_hidden(level))
            || self.composition.as_ref().is_some_and(|composition| {
                composition
                    .children
                    .iter()
                    .all(|child| child.is_hidden(level))
            })
            || self
                .frame
                .as_ref()
                .is_some_and(|frame| frame.content.is_hidden(level))
    }

    pub fn query(&self, level: Level) -> MeasuredSize {
        if self.is_hidden(level) {
            return MeasuredSize {
                preferred: Size::new(0.0, 0.0),
                minimum: Size::new(0.0, 0.0),
            };
        }
        (self.query)(level)
    }

    /// Allocate first, then lay out leaves at their allocated width with
    /// unbounded height. Thus a clipped height can never certify a fit. A useful
    /// minimum narrower than intrinsic width must be explicitly declared.
    pub fn query_at(&self, level: Level, available: Size) -> Result<GeometryPlan, AllocationError> {
        self.try_query_at(level, available)
            .map_err(|failure| failure.error)
    }

    fn try_query_at(&self, level: Level, available: Size) -> Result<GeometryPlan, Failure> {
        self.query_at_inner(level, available)
            .map_err(|failure| self.attribute(failure))
    }

    fn query_at_inner(&self, level: Level, available: Size) -> Result<GeometryPlan, Failure> {
        if !valid_available(available) {
            return Err(AllocationError::InvalidInput.into());
        }
        if self.is_hidden(level) {
            return Ok(self.bind_geometry(level, GeometryPlan::hidden()));
        }
        let measured = self.query(level);
        validate_measurement(measured)?;
        let widths = allocate_with_growth(
            available.width,
            &[WidthCapacity {
                preferred: measured.preferred.width,
                minimum: measured.minimum.width,
                weight: self.weight,
            }],
            &[self.grow],
        )?;
        let width = if self.has_composition(level) && available.width.is_finite() {
            available.width
        } else {
            widths[0]
        };
        self.plan_width(level, width, available.height)
    }

    fn plan_width(&self, level: Level, width: f64, height: f64) -> Result<GeometryPlan, Failure> {
        self.plan_width_inner(level, width, height)
            .map_err(|failure| self.attribute(failure))
    }

    fn plan_width_inner(
        &self,
        level: Level,
        width: f64,
        height: f64,
    ) -> Result<GeometryPlan, Failure> {
        if self.is_hidden(level) {
            return Ok(self.bind_geometry(level, GeometryPlan::hidden()));
        }
        let constrained = if self.fixed_width.is_some() || self.fixed_height.is_some() {
            Some(self.query(level))
        } else {
            None
        };
        let width = if self.fixed_width.is_some() {
            let requested = constrained.expect("fixed measurement").minimum.width;
            if width + FIT_EPSILON < requested {
                return Err(no_fit(requested, width));
            }
            requested
        } else {
            width
        };
        let height = if self.fixed_height.is_some() {
            let requested = constrained.expect("fixed measurement").minimum.height;
            if height + FIT_EPSILON < requested {
                return Err(no_fit(requested, height));
            }
            requested
        } else {
            height
        };
        let mut plan = if let Some(form) = self.selected_form(level) {
            let mut selected = form.clone();
            if self.grow_height > 0.0 {
                selected.grow_height = self.grow_height;
            }
            selected.plan_width(level, width, height)?
        } else if let Some(composition) = &self.composition {
            composition.plan(level, width, height)?
        } else if let Some(frame) = &self.frame {
            frame.plan(level, width, height, self.grow_height)?
        } else {
            let native = self.native.as_ref().expect("a native leaf");
            let viewport_height = self.viewport.as_ref().map(|minimum| {
                let minimum = minimum(level);
                let measured = self.query(level);
                if self.grow_height > 0.0 && height.is_finite() {
                    height.max(minimum.height)
                } else {
                    measured.preferred.height.min(height).max(minimum.height)
                }
            });
            let size = native.layout(
                level,
                Size::new(width, viewport_height.unwrap_or(f64::INFINITY)),
            );
            if !finite_size(size) {
                return Err(AllocationError::InvalidInput.into());
            }
            if size.width > width + FIT_EPSILON {
                return Err(no_fit(size.width, width));
            }
            if let Some(offered_height) = viewport_height
                && size.height > offered_height + FIT_EPSILON
            {
                return Err(no_fit(size.height, offered_height));
            }
            // The parent reserves the allocated width even for shrink content.
            let mut plan = GeometryPlan::leaf(Size::new(width, size.height));
            plan.native_size = Some(size);
            if let Some(minimum) = &self.viewport {
                let minimum = minimum(level);
                plan.size.height = viewport_height.expect("declared viewport height");
                plan.minimum_height = minimum.height;
            }
            plan
        };
        if self.fixed_width.is_some() || self.reserve_width {
            plan.size.width = width;
        }
        if (self.grow_height > 0.0 || self.fixed_height.is_some()) && height.is_finite() {
            plan.size.height = plan.size.height.max(height);
        }
        if self.fixed_height.is_some() {
            plan.minimum_height = plan
                .minimum_height
                .max(constrained.expect("fixed measurement").minimum.height);
        }
        if plan.size.width > width + FIT_EPSILON {
            return Err(no_fit(plan.size.width, width));
        }
        if plan.size.height > height + FIT_EPSILON {
            return Err(no_fit(plan.size.height, height));
        }
        Ok(self.bind_geometry(level, plan))
    }

    /// Reserve the full width allocated by the parent, keeping the inner
    /// component's natural geometry and reports intact. Useful for wrapping
    /// navigation followed by trailing controls: a short final row must not
    /// move those controls into space already allocated to navigation.
    pub fn fill_width(self) -> Self {
        let weight = self.weight;
        let grow_height = self.grow_height;
        let mut result = column_child(vec![self], 0.0);
        result.weight = weight;
        result.grow_height = grow_height;
        result.reserve_width = true;
        result.grow = 1.0;
        result
    }

    /// Reserve exact dimensions for this component. A dimension below its
    /// measured useful minimum is raised to that minimum; insufficient
    /// available space fails the stage rather than shrinking the request.
    /// Apply this to the expanded form, before declaring a collapsed form.
    /// The inner child remains an ordinary nested composition and keeps its
    /// identity and reports. Values must be finite and nonnegative.
    pub fn fixed_dimensions(self, width: Option<f64>, height: Option<f64>) -> Self {
        assert!(
            [width, height]
                .into_iter()
                .flatten()
                .all(|n| n.is_finite() && n >= 0.0)
        );
        if width.is_none() && height.is_none() {
            return self;
        }
        // A wrapper keeps the inner child's measured geometry intact.
        let weight = self.weight;
        let grow = self.grow;
        let grow_height = self.grow_height;
        let mut result = column_child(vec![self], 0.0);
        result.weight = weight;
        result.grow = grow;
        result.grow_height = grow_height;
        result.fixed_width = width;
        result.fixed_height = height;
        let query = result.query.clone();
        result.query = Rc::new(move |level| {
            let mut measured = query(level);
            if let Some(width) = width {
                let width = width.max(measured.minimum.width);
                measured.minimum.width = width;
                measured.preferred.width = width;
            }
            if let Some(height) = height {
                let height = height.max(measured.minimum.height);
                measured.minimum.height = height;
                measured.preferred.height = height;
            }
            measured
        });
        result
    }

    pub fn weight(mut self, weight: f64) -> Self {
        assert!(weight.is_finite() && weight >= 0.0);
        self.weight = weight;
        self
    }

    /// Share surplus only after shrinking has finished.
    pub fn grow(mut self, grow: f64) -> Self {
        assert!(grow.is_finite() && grow >= 0.0);
        self.grow = grow;
        self
    }

    /// Share surplus height in a column independently of width growth. A
    /// leaf keeps its laid-out height floor; a declared viewport expands.
    pub fn grow_height(mut self, grow: f64) -> Self {
        assert!(grow.is_finite() && grow >= 0.0);
        self.grow_height = grow;
        self
    }

    /// Explicit component permission to omit this form. No layout slot or
    /// gap is planned for hidden children.
    pub fn hidden_when(mut self, hidden: impl Fn(Level) -> bool + 'a) -> Self {
        self.hidden = Rc::new(hidden);
        self
    }

    pub fn round_width_up(mut self) -> Self {
        let query = self.query.clone();
        self.query = Rc::new(move |level| {
            let mut measured = query(level);
            measured.preferred.width = measured.preferred.width.ceil();
            measured.minimum.width = measured.minimum.width.ceil();
            measured
        });
        self
    }

    /// Set the desired width independently of the measured useful minimum.
    /// A parent may allocate less down to that minimum; when the minimum is
    /// larger than the requested width it remains authoritative. Geometry is
    /// laid out again at the allocated width.
    pub fn preferred_width(self, width: f64) -> Self {
        self.preferred_width_with(move |_, _| width)
    }

    /// Derive a preferred width from the same measurement, without
    /// replacing the independently measured minimum.
    pub fn preferred_width_with(
        mut self,
        preferred: impl Fn(Level, MeasuredSize) -> f64 + 'a,
    ) -> Self {
        let query = self.query.clone();
        self.query = Rc::new(move |level| {
            let mut measured = query(level);
            let width = preferred(level, measured);
            measured.preferred.width = if width.is_finite() && width >= 0.0 {
                width.max(measured.minimum.width)
            } else {
                f64::NAN
            };
            measured
        });
        self
    }

    /// Declare a finite, nonnegative useful width floor; the engine's
    /// `minimum_width`, which differs from [`Self::minimum_width_with`]
    /// only by the renderer the engine passes to the latter.
    pub fn minimum_width(self, minimum: impl Fn(Level, Size) -> f64 + 'a) -> Self {
        self.minimum_width_with(minimum)
    }

    /// Declare a finite, nonnegative useful width floor. If the intrinsic
    /// width is smaller, the preferred offer is raised to the floor. A leaf
    /// with positive infinite intrinsic width is laid out at this declared
    /// floor; only a finite fitting result can use the floor as its initial
    /// preferred offer. The leaf is laid out again at its allocated width.
    pub fn minimum_width_with(mut self, minimum: impl Fn(Level, Size) -> f64 + 'a) -> Self {
        let query = self.query.clone();
        let native = if self.composition.is_none() && self.forms.is_none() && self.frame.is_none() {
            self.native.clone()
        } else {
            None
        };
        self.query = Rc::new(move |level| {
            let mut measured = query(level);
            let recoverable = measured.preferred.width == f64::INFINITY
                && measured.preferred.height.is_finite()
                && measured.preferred.height >= 0.0
                && measured.minimum.height.is_finite()
                && measured.minimum.height >= 0.0
                && native.is_some();
            if (measured.preferred.width.is_finite() && measured.preferred.width >= 0.0)
                || recoverable
            {
                let floor = minimum(level, measured.preferred);
                if floor.is_finite() && floor >= 0.0 {
                    if recoverable {
                        // A leaf with no intrinsic width: the declared floor is
                        // the finite offer, never an arbitrary probe width.
                        let bounded = native
                            .as_ref()
                            .expect("native leaf")
                            .layout(level, Size::new(floor, f64::INFINITY));
                        if finite_size(bounded) && bounded.width <= floor + FIT_EPSILON {
                            measured.preferred = Size::new(floor, bounded.height);
                            measured.minimum.width = floor;
                            measured.minimum.height = measured.minimum.height.min(bounded.height);
                        } else {
                            measured.minimum.width = f64::NAN;
                        }
                    } else {
                        // A leaf can report an intrinsic width of zero.
                        measured.minimum.width = floor;
                        measured.preferred.width = measured.preferred.width.max(floor);
                    }
                } else {
                    measured.minimum.width = f64::NAN;
                }
            } else {
                measured.minimum.width = f64::NAN;
            }
            measured
        });
        self
    }

    fn with_measurement(mut self, query: impl Fn(Level) -> MeasuredSize + 'a) -> Self {
        self.query = Rc::new(query);
        self
    }
}

/// A leaf: the caller's content, measured and laid out by the caller.
pub fn child<'a>(leaf: impl Leaf + 'a) -> Child<'a> {
    let leaf: Rc<dyn Leaf + 'a> = Rc::new(leaf);
    let measure = leaf.clone();
    Child::new(Some(leaf), Rc::new(move |level| measure.query(level)))
}

/// A component family's own structured forms, inherited until the next
/// threshold. The first threshold must be Preferred and thresholds strictly
/// increase. Every selected form receives the requested level, so an inherited
/// composite still forwards Mild/Strong rules to its own immediate children.
/// The returned child's outward weights are independent of the form's internals.
pub fn forms<'a>(alternatives: Vec<(Level, Child<'a>)>) -> Child<'a> {
    assert!(
        alternatives
            .first()
            .is_some_and(|(level, _)| *level == Level::Preferred)
    );
    assert!(alternatives.windows(2).all(|pair| pair[0].0 < pair[1].0));
    let alternatives = Rc::new(alternatives);
    let query_forms = alternatives.clone();
    let mut result = Child::new(
        None,
        Rc::new(move |level| select_form(&query_forms, level).query(level)),
    );
    result.forms = Some(alternatives);
    result
}

fn select_form<'a, 'forms>(
    alternatives: &'forms [(Level, Child<'a>)],
    level: Level,
) -> &'forms Child<'a> {
    &alternatives
        .iter()
        .rev()
        .find(|(threshold, _)| *threshold <= level)
        .expect("Preferred form is always inherited")
        .1
}

/// Declare a scrolling viewport: the leaf must show its content within the
/// offered viewport height. The declaration is local; no stage invents
/// scrolling. If the leaf has no finite intrinsic preferred dimension, the
/// declared minimum supplies that initial offer after a bounded layout
/// succeeds.
pub fn scroll_child<'a>(leaf: impl Leaf + 'a, minimum: Size) -> Child<'a> {
    assert!(finite_size(minimum));
    scroll_child_with_minimum(leaf, move |_| minimum)
}

/// Declare a scrolling viewport whose useful floor depends on the level.
/// Nonfinite or negative floors are explicit invalid input, never clipped
/// viewports.
pub fn scroll_child_with_minimum<'a>(
    leaf: impl Leaf + 'a,
    minimum: impl Fn(Level) -> Size + 'a,
) -> Child<'a> {
    let minimum: MinimumQuery<'a> = Rc::new(minimum);
    let viewport = minimum.clone();
    let mut child = child(leaf);
    let query = child.query.clone();
    let native = child.native.clone().expect("a native leaf");
    child.query = Rc::new(move |level| {
        let minimum = minimum(level);
        let measured = query(level);
        if !finite_size(minimum) {
            return MeasuredSize {
                preferred: Size::new(f64::NAN, f64::NAN),
                minimum,
            };
        }
        let invalid = |value: f64| value.is_nan() || value < 0.0;
        if invalid(measured.preferred.width) || invalid(measured.preferred.height) {
            return MeasuredSize {
                preferred: measured.preferred,
                minimum,
            };
        }
        let mut preferred = measured.preferred;
        if preferred.width.is_infinite() || preferred.height.is_infinite() {
            // Validate the whole viewport at that finite minimum.
            let bounded = native.layout(level, minimum);
            if !finite_size(bounded)
                || bounded.width > minimum.width + FIT_EPSILON
                || bounded.height > minimum.height + FIT_EPSILON
            {
                return MeasuredSize {
                    preferred: Size::new(f64::NAN, f64::NAN),
                    minimum,
                };
            }
            if preferred.width.is_infinite() {
                preferred.width = minimum.width;
            }
            if preferred.height.is_infinite() {
                preferred.height = minimum.height;
            }
        }
        MeasuredSize {
            preferred: Size::new(
                preferred.width.max(minimum.width),
                preferred.height.max(minimum.height),
            ),
            minimum,
        }
    });
    child.viewport = Some(viewport);
    child
}

/// The engine's spacer leaf: as wide as it is offered, no height.
struct Space;

impl Leaf for Space {
    fn query(&self, _: Level) -> MeasuredSize {
        MeasuredSize {
            preferred: Size::new(0.0, 0.0),
            minimum: Size::new(0.0, 0.0),
        }
    }

    fn layout(&self, _: Level, maximum: Size) -> Size {
        // A fill width takes a finite offer; unbounded, it has none.
        Size::new(
            if maximum.width.is_finite() {
                maximum.width
            } else {
                0.0
            },
            0.0,
        )
    }
}

pub fn spacer<'a>(minimum: f64) -> Child<'a> {
    assert!(minimum.is_finite() && minimum >= 0.0);
    child(Space)
        .with_measurement(move |_| MeasuredSize {
            preferred: Size::new(minimum, 0.0),
            minimum: Size::new(minimum, 0.0),
        })
        .weight(0.0)
        .grow(1.0)
}

/// Add declared chrome without flattening a box into a fixed-size leaf.
/// All geometry, forms, minima and requested levels of the immediate child
/// remain part of the same engine. Hidden content omits the entire frame
/// and its spacing.
pub fn framed_child<'a>(content: Child<'a>, padding: impl Into<Padding>) -> Child<'a> {
    let padding = padding.into();
    assert!(
        [padding.left, padding.right, padding.top, padding.bottom]
            .iter()
            .all(|value| value.is_finite() && *value >= 0.0)
    );
    let frame = Rc::new(Frame { content, padding });
    let measure = frame.clone();
    let mut result = child(Space);
    result.query = Rc::new(move |level| {
        let inner = measure.content.query(level);
        let chrome = Size::new(measure.padding.x(), measure.padding.y());
        MeasuredSize {
            preferred: Size::new(
                inner.preferred.width + chrome.width,
                inner.preferred.height + chrome.height,
            ),
            minimum: Size::new(
                if measure.content.weight == 0.0 {
                    inner.preferred.width
                } else {
                    inner.minimum.width
                } + chrome.width,
                inner.minimum.height + chrome.height,
            ),
        }
    });
    result.frame = Some(frame);
    result
}

impl Frame<'_> {
    fn plan(
        &self,
        level: Level,
        width: f64,
        height: f64,
        grow_height: f64,
    ) -> Result<GeometryPlan, Failure> {
        let horizontal = self.padding.x();
        let vertical = self.padding.y();
        if width + FIT_EPSILON < horizontal {
            return Err(no_fit(horizontal, width));
        }
        if height + FIT_EPSILON < vertical {
            return Err(no_fit(vertical, height));
        }
        let mut content = self.content.clone();
        if grow_height > 0.0 {
            content.grow_height = grow_height;
        }
        let geometry = content.plan_width(
            level,
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
            position: Point::new(self.padding.left, self.padding.top),
            geometry,
        });
        Ok(result)
    }
}

pub fn row_child(children: Vec<Child<'_>>, spacing: f64) -> Child<'_> {
    compose(children, spacing, Arrangement::Row)
}

/// Arrange a row with each immediate child aligned to its top edge.
pub fn row_child_top(children: Vec<Child<'_>>, spacing: f64) -> Child<'_> {
    compose_aligned(children, spacing, Arrangement::Row, true)
}

pub fn column_child(children: Vec<Child<'_>>, spacing: f64) -> Child<'_> {
    compose(children, spacing, Arrangement::Column)
}

pub fn wrap_child(children: Vec<Child<'_>>, spacing: f64) -> Child<'_> {
    compose(children, spacing, Arrangement::Wrap)
}

fn compose(children: Vec<Child<'_>>, spacing: f64, arrangement: Arrangement) -> Child<'_> {
    compose_aligned(children, spacing, arrangement, false)
}

fn compose_aligned(
    children: Vec<Child<'_>>,
    spacing: f64,
    arrangement: Arrangement,
    top_aligned: bool,
) -> Child<'_> {
    assert!(spacing.is_finite() && spacing >= 0.0);
    let composition = Rc::new(Composition {
        children,
        arrangement,
        spacing,
        top_aligned,
    });
    let query = composition.clone();
    let mut result = Child::new(None, Rc::new(move |level| query.measure(level)));
    result.composition = Some(composition);
    result
}

fn finite_size(size: Size) -> bool {
    size.width.is_finite() && size.height.is_finite() && size.width >= 0.0 && size.height >= 0.0
}

fn valid_available(size: Size) -> bool {
    !size.width.is_nan() && !size.height.is_nan() && size.width >= 0.0 && size.height >= 0.0
}

fn validate_measurement(size: MeasuredSize) -> Result<(), AllocationError> {
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

fn allocate_with_growth(
    available: f64,
    capacities: &[WidthCapacity],
    grows: &[f64],
) -> Result<Vec<f64>, AllocationError> {
    let mut widths = if available.is_infinite() {
        if capacities
            .iter()
            .any(|capacity| !finite_size(Size::new(capacity.preferred, capacity.minimum)))
        {
            return Err(AllocationError::InvalidInput);
        }
        capacities
            .iter()
            .map(|capacity| capacity.preferred)
            .collect()
    } else {
        allocate_width(available, capacities)?
    };
    let total: f64 = grows.iter().sum();
    if available.is_finite() && total > 0.0 {
        let surplus = (available - widths.iter().sum::<f64>()).max(0.0);
        for (width, grow) in widths.iter_mut().zip(grows) {
            *width += surplus * grow / total;
        }
    }
    Ok(widths)
}

impl Composition<'_> {
    fn measure(&self, level: Level) -> MeasuredSize {
        let measured: Vec<_> = self
            .children
            .iter()
            .filter(|child| !child.is_hidden(level))
            .map(|child| (child, child.query(level)))
            .collect();
        let gap = self.spacing * measured.len().saturating_sub(1) as f64;
        let mut result = MeasuredSize {
            preferred: Size::new(0.0, 0.0),
            minimum: Size::new(0.0, 0.0),
        };
        for (child, size) in &measured {
            let minimum_width = if child.weight == 0.0 {
                size.preferred.width
            } else {
                size.minimum.width
            };
            match self.arrangement {
                Arrangement::Row | Arrangement::Wrap => {
                    result.preferred.width += size.preferred.width;
                    result.preferred.height = result.preferred.height.max(size.preferred.height);
                    if self.arrangement == Arrangement::Row {
                        result.minimum.width += minimum_width;
                        result.minimum.height = result.minimum.height.max(size.minimum.height);
                    } else {
                        result.minimum.width = result.minimum.width.max(minimum_width);
                        // A height bound, not the height at that width. plan() measures rows.
                        result.minimum.height = result.minimum.height.max(size.minimum.height);
                    }
                }
                Arrangement::Column => {
                    result.preferred.width = result.preferred.width.max(size.preferred.width);
                    result.minimum.width = result.minimum.width.max(minimum_width);
                    result.preferred.height += size.preferred.height;
                    result.minimum.height += size.minimum.height;
                }
            }
        }
        match self.arrangement {
            Arrangement::Row => {
                result.preferred.width += gap;
                result.minimum.width += gap;
            }
            Arrangement::Wrap => result.preferred.width += gap,
            Arrangement::Column => {
                result.preferred.height += gap;
                result.minimum.height += gap;
            }
        }
        result
    }

    fn plan(&self, level: Level, width: f64, height: f64) -> Result<GeometryPlan, Failure> {
        let active: Vec<_> = self
            .children
            .iter()
            .enumerate()
            .filter(|(_, child)| !child.is_hidden(level))
            .map(|(index, child)| (index, child.query(level)))
            .collect();
        for (_, size) in &active {
            validate_measurement(*size)?;
        }
        let capacity = |index: usize, measured: MeasuredSize| WidthCapacity {
            preferred: measured.preferred.width,
            minimum: measured.minimum.width,
            weight: self.children[index].weight,
        };
        let mut result = GeometryPlan::leaf(Size::new(0.0, 0.0));
        if self.arrangement == Arrangement::Column {
            let gap = self.spacing * active.len().saturating_sub(1) as f64;
            if gap > height {
                return Err(no_fit(gap, height));
            }
            let mut proposals = Vec::new();
            for (index, measured) in active {
                let child = &self.children[index];
                let allocated =
                    allocate_with_growth(width, &[capacity(index, measured)], &[child.grow])?[0];
                let geometry = child.plan_width(level, allocated, f64::INFINITY)?;
                proposals.push((index, allocated, geometry));
            }
            // Width-dependent ordinary heights are hard floors. Only explicitly
            // declared viewports (possibly nested) offer vertical capacity.
            let heights = allocate_with_growth(
                height - gap,
                &proposals
                    .iter()
                    .map(|(index, _, plan)| WidthCapacity {
                        preferred: plan.size.height,
                        minimum: plan.minimum_height,
                        weight: self.children[*index].weight,
                    })
                    .collect::<Vec<_>>(),
                &proposals
                    .iter()
                    .map(|(index, _, _)| self.children[*index].grow_height)
                    .collect::<Vec<_>>(),
            )?;
            result.minimum_height = gap;
            for ((index, allocated, proposal), allocated_height) in
                proposals.into_iter().zip(heights)
            {
                let geometry = if (allocated_height - proposal.size.height).abs() > FIT_EPSILON {
                    self.children[index].plan_width(level, allocated, allocated_height)?
                } else {
                    proposal
                };
                if !result.children.is_empty() {
                    result.size.height += self.spacing;
                }
                let position = Point::new(0.0, result.size.height);
                result.size.height += geometry.size.height;
                result.size.width = result.size.width.max(geometry.size.width);
                result.minimum_height += geometry.minimum_height;
                result.children.push(Placement {
                    index,
                    position,
                    geometry,
                });
            }
            return Ok(result);
        }
        // Pack using useful minima, including zero-weight protection. Each new
        // row gets a fresh allocation from that stage's preferred dimensions.
        let mut rows: Vec<Vec<(usize, MeasuredSize)>> = Vec::new();
        let mut used = 0.0;
        for (index, measured) in active {
            let child = &self.children[index];
            let minimum = if child.weight == 0.0 {
                measured.preferred.width
            } else {
                measured.minimum.width
            };
            if minimum > width + FIT_EPSILON {
                return Err(no_fit(minimum, width));
            }
            let extra = if rows.last().is_some_and(|row| !row.is_empty()) {
                self.spacing
            } else {
                0.0
            };
            if self.arrangement == Arrangement::Wrap && used + extra + minimum > width + FIT_EPSILON
            {
                rows.push(Vec::new());
                used = 0.0;
            }
            if rows.is_empty() {
                rows.push(Vec::new());
            }
            let row = rows.last_mut().expect("a row");
            if !row.is_empty() {
                used += self.spacing;
            }
            used += minimum;
            row.push((index, measured));
        }
        for row in rows {
            let gap = self.spacing * row.len().saturating_sub(1) as f64;
            if gap > width {
                return Err(no_fit(gap, width));
            }
            let capacities: Vec<_> = row
                .iter()
                .map(|(index, measured)| capacity(*index, *measured))
                .collect();
            let grows: Vec<_> = row
                .iter()
                .map(|(index, _)| self.children[*index].grow)
                .collect();
            let widths = allocate_with_growth(width - gap, &capacities, &grows)?;
            let start = result.children.len();
            let mut row_width = 0.0;
            let mut row_height: f64 = 0.0;
            let mut row_minimum_height: f64 = 0.0;
            for ((index, _), allocated) in row.into_iter().zip(widths) {
                let geometry = self.children[index].plan_width(
                    level,
                    allocated,
                    if self.arrangement == Arrangement::Row {
                        height
                    } else {
                        f64::INFINITY
                    },
                )?;
                if result.children.len() > start {
                    row_width += self.spacing;
                }
                let position = Point::new(row_width, result.size.height);
                row_width += geometry.size.width;
                row_height = row_height.max(geometry.size.height);
                row_minimum_height = row_minimum_height.max(geometry.minimum_height);
                result.children.push(Placement {
                    index,
                    position,
                    geometry,
                });
            }
            for placed in &mut result.children[start..] {
                if !self.top_aligned {
                    placed.position.y += (row_height - placed.geometry.size.height) / 2.0;
                }
            }
            result.rows.push((start..result.children.len()).collect());
            result.size.width = result.size.width.max(row_width);
            result.size.height += row_height + self.spacing;
            result.minimum_height += row_minimum_height + self.spacing;
        }
        if !result.rows.is_empty() {
            result.size.height -= self.spacing;
            result.minimum_height -= self.spacing;
        }
        // A wrap is a column of atomic rows. Share a vertical shortage only
        // where those rows contain declared viewport capacity.
        if self.arrangement == Arrangement::Wrap && result.size.height > height + FIT_EPSILON {
            let gap = self.spacing * result.rows.len().saturating_sub(1) as f64;
            if gap > height {
                return Err(no_fit(gap, height));
            }
            let capacities: Vec<_> = result
                .rows
                .iter()
                .map(|row| WidthCapacity {
                    preferred: row
                        .iter()
                        .map(|i| result.children[*i].geometry.size.height)
                        .fold(0.0, f64::max),
                    minimum: row
                        .iter()
                        .map(|i| result.children[*i].geometry.minimum_height)
                        .fold(0.0, f64::max),
                    weight: 1.0,
                })
                .collect();
            let heights =
                allocate_with_growth(height - gap, &capacities, &vec![0.0; capacities.len()])?;
            let mut y = 0.0;
            for (row, row_height) in result.rows.iter().zip(heights) {
                for i in row {
                    let placed = &mut result.children[*i];
                    if placed.geometry.size.height > row_height + FIT_EPSILON {
                        placed.geometry = self.children[placed.index].plan_width(
                            level,
                            placed.geometry.size.width,
                            row_height,
                        )?;
                    }
                    placed.position.y = y + (row_height - placed.geometry.size.height) / 2.0;
                }
                y += row_height + self.spacing;
            }
            result.size.height = (y - self.spacing).max(0.0);
        }
        Ok(result)
    }
}

/// One exact level and component arrangement in the parent's ordered policy.
pub struct Stage<'a> {
    pub level: Level,
    pub content: Child<'a>,
}

impl<'a> Stage<'a> {
    pub fn new(level: Level, content: Child<'a>) -> Self {
        Self { level, content }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StagePlan {
    pub stage_index: usize,
    pub level: Level,
    pub geometry: GeometryPlan,
}

/// Why one stage did not fit: the engine's error, and the nearest
/// identified child it came from, if any child on the way had an identity.
#[derive(Clone, Debug, PartialEq)]
pub struct StageFailure {
    pub stage_index: usize,
    pub level: Level,
    pub error: AllocationError,
    pub child: Option<Id>,
}

/// No declared stage supplied complete fitting geometry. Nothing is implicitly
/// hidden, clipped, scrolled or promoted to a stronger undeclared level.
#[derive(Clone, Debug, PartialEq)]
pub struct NoFit {
    pub attempted: usize,
    /// One entry per stage, in the order tried.
    pub failures: Vec<StageFailure>,
}

pub fn choose_stage(stages: &[Stage<'_>], available: Size) -> Result<StagePlan, NoFit> {
    let mut failures = Vec::new();
    for (stage_index, stage) in stages.iter().enumerate() {
        match stage.content.try_query_at(stage.level, available) {
            Ok(geometry) => {
                return Ok(StagePlan {
                    stage_index,
                    level: stage.level,
                    geometry,
                });
            }
            Err(failure) => failures.push(StageFailure {
                stage_index,
                level: stage.level,
                error: failure.error,
                child: failure.child,
            }),
        }
    }
    Err(NoFit {
        attempted: stages.len(),
        failures,
    })
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
