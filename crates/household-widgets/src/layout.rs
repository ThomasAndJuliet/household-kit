// SPDX-License-Identifier: GPL-3.0-only

//! Measured native representations, in increasing order of permitted compromise.
//!
//! The view factory must be free of application side effects: it is called for
//! disposable measurements as well as the selected representation. Queries use
//! fresh widget state and cannot consume the runtime's named widget state.
//! Compatible selected widgets retain their native state through ordinary tree
//! reconciliation. Changing widget types discards that local state, including
//! focus and inline overlays; this adapter does not transfer it between controls.

// No path through the layout engine may panic. Every household app lays out
// through it, and the owner's rule is: "this is a user application - we
// should not be paining at all" (panicking). A number the engine cannot use
// makes an invalid child; a plan that does not match the child given it is
// refused; a widget tree or layout without the part it should hold does
// nothing there. So the engine's code (this module and the `layout_*` files,
// which are its children) may not unwrap, expect, index or slice unchecked,
// panic, or leave unreachable, todo or unimplemented paths, and Clippy in CI
// (`-D warnings`) rejects a change that adds one. Tests are exempt: a test
// may unwrap and index, since a panic there is a failing test.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]

use cosmic::iced::advanced::widget::{Operation, Tree, tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use cosmic::iced::{Event, Length, Rectangle, Size, Vector};
use cosmic::{Element, Renderer, Theme, widget};
use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;

#[path = "layout_frame.rs"]
mod frame;
#[path = "layout_information.rs"]
mod information;
#[path = "layout_navigation.rs"]
mod navigation;
pub use frame::framed_child;
pub use navigation::*;
#[path = "layout_presentation.rs"]
mod presentation;
pub use information::{
    AcceptedSourceReport, InformationId, InformationLoss, LossKind, SourceId, SourceReport,
    SourceReports,
};
use information::{SourceInformation, VerifiedBox};
pub use presentation::{
    Detail, Emphasis, Presentation, PresentationFamily, PresentationRequest,
    UnsupportedPresentation,
};

#[path = "layout_popup.rs"]
mod attached;
#[path = "layout_hover.rs"]
mod hover;
pub use attached::{
    Alignment, AnchorCandidate, AttachedPlan, Attachment, PlacementPreference, Side,
    choose_attached, commit_attached,
};
pub(crate) use attached::{
    ChainCard, ChainFailure, RetainedGeometry, choose_attached_avoiding, choose_attached_chain,
    commit_attached_retaining, commit_attached_retaining_avoiding,
};
pub use hover::choose_hover;
pub(crate) use hover::{HoverRetained, commit_hover_retaining};

// The engine's pure geometry lives in `household-layout`, which names no
// toolkit type, so that the charts lay out with the same code. Its values
// and the sharing of a length are this interface's, re-exported at the same
// paths; the rest of this file converts iced's `Size` to and from the
// core's at the edge, and the arithmetic is the core's, written once.
pub(crate) use household_layout::FIT_EPSILON;
pub use household_layout::{AllocationError, Arrangement, Level, WidthCapacity, allocate_width};
use household_layout::{Host, Kind, LeafHeight, Measured, Offer, is_amount};

/// Query representations without retaining the result of any rejected query.
/// `None` explicitly means that none of the permitted representations fits.
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

/// Preferred and useful minimum geometry of a component at one exact level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasuredSize {
    pub preferred: Size,
    pub minimum: Size,
}

// The core measures in its own `Size`, which has iced's two fields; the
// interface keeps iced's. These convert at the edge, number for number.
impl From<MeasuredSize> for Measured {
    fn from(size: MeasuredSize) -> Self {
        Self {
            preferred: core_size(size.preferred),
            minimum: core_size(size.minimum),
        }
    }
}

impl From<Measured> for MeasuredSize {
    fn from(size: Measured) -> Self {
        Self {
            preferred: toolkit_size(size.preferred),
            minimum: toolkit_size(size.minimum),
        }
    }
}

fn core_size(size: Size) -> household_layout::Size {
    household_layout::Size::new(size.width, size.height)
}

fn toolkit_size(size: household_layout::Size) -> Size {
    Size::new(size.width, size.height)
}

/// Geometry proposed by a pure query. No widget state is retained here.
#[derive(Clone, Debug)]
pub struct GeometryPlan {
    pub size: Size,
    pub hidden: bool,
    pub children: Vec<Placement>,
    // Row membership is part of the plan, not recomputed during commit.
    rows: Vec<Vec<usize>>,
    minimum_height: f32,
    native_size: Option<Size>,
    bindings: Vec<(PreparedId, Level)>,
    source_reports: Vec<SourceReport>,
}

impl GeometryPlan {
    /// Useful minimum height after measuring this form at its offered width.
    /// Divider constraints must use this width-dependent result for wrapped
    /// children rather than an unconstrained preferred height.
    pub fn minimum_height(&self) -> f32 {
        self.minimum_height
    }
}

// Geometric equality is useful when checking decorators and pure re-queries.
// Preparation bindings certify a commit, rather than changing its geometry.
impl PartialEq for GeometryPlan {
    fn eq(&self, other: &Self) -> bool {
        self.size == other.size
            && self.hidden == other.hidden
            && self.children == other.children
            && self.rows == other.rows
            && self.minimum_height == other.minimum_height
            && self.native_size == other.native_size
            && self.source_reports == other.source_reports
    }
}

// Keep the allocation alive in every plan, so pointer identity cannot be reused
// while an old plan still exists. Clones of an immutable preparation share it.
#[derive(Clone, Debug)]
struct PreparedId(Arc<()>);

impl PreparedId {
    fn new() -> Self {
        Self(Arc::new(()))
    }
}

impl PartialEq for PreparedId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    /// Index in the declared immediate children (hidden children are omitted).
    pub index: usize,
    pub position: cosmic::iced::Point,
    pub geometry: GeometryPlan,
}

impl GeometryPlan {
    /// The engine's plan as the interface's: the same tree, with iced's
    /// `Size` and `Point`, and no bindings and no reports, which
    /// `Child::bind_geometry` adds. Every number is the engine's own `f32`.
    fn from_core(plan: &household_layout::Plan) -> Self {
        Self {
            size: toolkit_size(plan.size()),
            hidden: plan.is_hidden(),
            children: plan
                .children()
                .iter()
                .map(|placed| Placement {
                    index: placed.index(),
                    position: cosmic::iced::Point::new(placed.position().x, placed.position().y),
                    geometry: Self::from_core(placed.plan()),
                })
                .collect(),
            rows: plan.rows().to_vec(),
            minimum_height: plan.minimum_height(),
            native_size: plan.leaf_size().map(toolkit_size),
            bindings: Vec::new(),
            source_reports: Vec::new(),
        }
    }

    /// A leaf's plan of `size`, with nothing bound, for tests that give a
    /// child a plan it was not planned from.
    #[cfg(test)]
    fn leaf(size: Size) -> Self {
        Self {
            size,
            hidden: false,
            children: Vec::new(),
            rows: Vec::new(),
            minimum_height: size.height,
            native_size: None,
            bindings: Vec::new(),
            source_reports: Vec::new(),
        }
    }

    /// Trial facts about this plan and its immediate-child subplans. These are
    /// never active-source notifications; accepted native traversal publishes
    /// the current facts through `AcceptedSourceReport` instead.
    pub fn reports(&self) -> Vec<&SourceReport> {
        let mut reports: Vec<_> = self.source_reports.iter().collect();
        for child in &self.children {
            reports.extend(child.geometry.reports());
        }
        reports
    }

    pub(crate) fn same_preparation(&self, other: &Self) -> bool {
        self.bindings == other.bindings
            && self.children.len() == other.children.len()
            && self
                .children
                .iter()
                .zip(&other.children)
                .all(|(a, b)| a.index == b.index && a.geometry.same_preparation(&b.geometry))
    }
}

type ViewFactory<'a, Message> = Rc<dyn Fn(Level) -> Element<'a, Message> + 'a>;
type ViewMap<'a, Message> = Rc<dyn Fn(Element<'a, Message>) -> Element<'a, Message> + 'a>;

/// The widgets as the engine's host (`household_layout::Host`): a leaf is a
/// native view, built by its factory at a level; measuring needs the
/// renderer; every part carries the widgets' `Note`; and a frame's chrome is
/// the decorator that draws its padding (`framed_child`).
///
/// The engine's one question, a leaf's size at a level within an offer, is
/// answered by laying the view out natively with fresh state
/// (`measure_element_in`): an offer to take the height it needs is laid out
/// with its height compressed to its own, an offer of a height to be laid
/// out in is laid out in that height. Those are the four measurements the
/// engine makes of a native view: with unbounded room, at a declared
/// minimum width, at a viewport's floor, and at the width allocated while
/// planning.
struct Native<'a, Message>(PhantomData<fn() -> (&'a (), Message)>);

impl<'a, Message> Host for Native<'a, Message> {
    type Leaf = ViewFactory<'a, Message>;
    type Context = Renderer;
    type Note = Note<'a, Message>;
    type Chrome = ViewMap<'a, Message>;

    fn size(
        leaf: &Self::Leaf,
        level: Level,
        context: &Self::Context,
        offer: Offer,
    ) -> household_layout::Size {
        let (room, compress_height) = match offer {
            Offer::NeededHeight(room) => (room, true),
            Offer::OfferedHeight(room) => (room, false),
        };
        core_size(measure_element_in(
            || leaf(level),
            context,
            toolkit_size(room),
            compress_height,
        ))
    }
}

/// A part of the widgets' tree: the engine's part, with the widgets as its
/// host. A composition's children, a frame's content and a family's forms
/// are parts; `Child` is the interface's handle on one.
type Part<'a, Message> = household_layout::Child<'a, Native<'a, Message>>;

/// A queryable leaf or composite. Only the root selects a level. Composites
/// forward that exact level; outward weights never replace internal weights.
///
/// What a child is, how it is measured and how it is planned are the
/// engine's (`household_layout::Child`, its part): its kind, the
/// adjustments to its measurement, and its weights, growth and hiding. This
/// is the interface's handle on that part, and it adds what is the
/// widgets': building views, binding a plan to the child it was planned
/// from and rendering it with verification (`Commit`), and the bookkeeping
/// every part carries in its note (`Note`). Each builder below declares on
/// the part through the engine's builder of the same name, then does the
/// widgets' bookkeeping.
pub struct Child<'a, Message> {
    part: Part<'a, Message>,
}

/// What the widgets keep on every child for themselves, apart from its
/// geometry: the preparation identity that certifies a plan was made from
/// this child (`bind_geometry`), the decorator that styles its rendered
/// content (`map_view`), the presentation it was prepared with
/// (`PresentationFamily::prepare`), the information it reports
/// (`report_information`) and the regions it names (`report_region`).
/// Planning never reads any of it: it is the host's note on the engine's
/// part (`household_layout::Host::Note`).
struct Note<'a, Message> {
    preparation: PreparedId,
    decorator: Option<ViewMap<'a, Message>>,
    presentation: Option<Presentation>,
    information: Option<SourceInformation<'a>>,
    regions: Vec<Region>,
}

/// A child's note before anything is declared: a fresh preparation
/// identity, and nothing else. The engine gives every part it builds this
/// note, the column that `fill_width` and `fixed_dimensions` put a child in
/// included.
impl<Message> Default for Note<'_, Message> {
    fn default() -> Self {
        Self {
            preparation: PreparedId::new(),
            decorator: None,
            presentation: None,
            information: None,
            regions: Vec::new(),
        }
    }
}

// Written by hand, as for `Child`: the decorator is an `Rc`, and a derived
// `Clone` would ask for `Message: Clone`.
impl<Message> Clone for Note<'_, Message> {
    fn clone(&self) -> Self {
        Self {
            preparation: self.preparation.clone(),
            decorator: self.decorator.clone(),
            presentation: self.presentation,
            information: self.information.clone(),
            regions: self.regions.clone(),
        }
    }
}

// Written by hand: the part's views and decorators are `Rc`s, so a clone
// shares the prepared parts and needs no `Message: Clone`.
impl<Message> Clone for Child<'_, Message> {
    fn clone(&self) -> Self {
        Self {
            part: self.part.clone(),
        }
    }
}

impl<'a, Message: 'a> Child<'a, Message> {
    /// The interface's handle on `part`.
    fn from_part(part: Part<'a, Message>) -> Self {
        Self { part }
    }

    /// Declare on the part with the engine's builder, `declare`, then give
    /// the child a fresh preparation identity, as every declaration that
    /// changes what is planned does: a plan made before it was made from
    /// another child.
    fn redeclared(self, declare: impl FnOnce(Part<'a, Message>) -> Part<'a, Message>) -> Self {
        let mut child = Self::from_part(declare(self.part));
        child.refresh_preparation();
        child
    }

    /// Mark this child invalid unless `valid`
    /// (`household_layout::Child::invalid_unless`).
    fn invalid_unless(self, valid: bool) -> Self {
        Self::from_part(self.part.invalid_unless(valid))
    }

    /// The preparation identity in this child's note, which the binding
    /// tests compare with a plan's bindings.
    #[cfg(test)]
    fn preparation(&self) -> &PreparedId {
        &self.part.note().preparation
    }

    /// What this child is, which tests match on.
    #[cfg(test)]
    fn kind(&self) -> &Kind<'a, Native<'a, Message>> {
        self.part.kind()
    }

    fn refresh_preparation(&mut self) {
        self.part.note_mut().preparation = PreparedId::new();
    }

    pub fn presentation(&self) -> Option<Presentation> {
        self.part.note().presentation
    }

    /// Report family-local information loss from prepared immutable data and
    /// this exact candidate geometry. This pure callback runs for trial queries
    /// too: it must not update a hover, registry, application state or perform
    /// work such as parsing, I/O or generation. Rebuild the Child on a content
    /// revision or presentation/dependency change; retain a clone only while
    /// its prepared snapshot is unchanged.
    pub fn report_information(
        mut self,
        source: SourceId,
        revision: u64,
        report: impl Fn(Level, &GeometryPlan) -> Vec<InformationLoss> + 'a,
    ) -> Self {
        self.part.note_mut().information = Some(SourceInformation {
            source,
            revision,
            report: Rc::new(report),
        });
        self.refresh_preparation();
        self
    }

    /// Name an accepted native box for section navigation. Multiple roles may
    /// describe the same box. Rebuilding refreshes the preparation identity.
    pub fn report_region(mut self, region: Region) -> Self {
        self.part.note_mut().regions.push(region);
        self.refresh_preparation();
        self
    }

    /// Bind a finished plan to this child, the child it was planned from, at
    /// the level it was planned at: the one walk that binds plans.
    ///
    /// What binding is. Every node of a plan records which children made it
    /// (`GeometryPlan::bindings`: a child's preparation identity and the
    /// level), and a child that reports information adds its report on the
    /// node it made (`source_reports`), made by its callback from that node.
    /// `render` draws a node only for a child whose binding it carries, so a
    /// plan can be committed only by the children it was planned from, and
    /// only the reports of the plan that is committed are published.
    ///
    /// Why it is apart from planning. Planning is geometry: where each part
    /// goes and whether it fits. Preparation identities and reports belong
    /// to the widgets, not to the geometry, and the planner lives in a
    /// crate that knows nothing of them (household-layout). Binding also
    /// needs only the plan that is kept: planning makes and discards trial
    /// plans (a column plans each child with unbounded height and again at
    /// its share, a wrap plans again in its second pass, a stage that does
    /// not fit is dropped), and those need no binding. So planning
    /// (`household_layout::Child::plan_width`, and the compositions' and the
    /// frame's planning) makes plans with no bindings and no reports, and
    /// `query_at`, the only way into planning from outside, plans, makes the
    /// interface's plan of the engine's (`GeometryPlan::from_core`), and then
    /// calls this once on the plan it returns.
    ///
    /// How the walk pairs a node with the child that made it: by the rule
    /// planning follows, one arm per kind (`Commit::bind`).
    ///
    /// - A composition's placement names the child it was planned from by
    ///   its `index` among the declared children.
    /// - A frame's content made the frame's first (and only) placement.
    /// - The form selected at the level made the very node the family
    ///   returns, so that node is bound twice: by the form, then by the
    ///   family.
    /// - A leaf has nothing below.
    ///
    /// A hidden node was returned at once by the hidden child that was asked
    /// for it (a hidden family, frame or composition returns before asking
    /// what it holds), so it is bound by that child alone and nothing below
    /// it is visited. A placement whose index names no child, or a frame's
    /// plan without its placement, cannot come from the child's own
    /// planning; the walk leaves that part unbound, and `render` refuses it.
    ///
    /// The order is the order planning bound in when it did it as it went:
    /// what is below a node is bound before the node itself, and a family's
    /// form before the family. So each node ends with the same bindings and
    /// reports in the same order, and each report callback is given the
    /// node as it was then: every node below it bound and reported, the
    /// node's earlier bindings and reports in place, and its own binding.
    /// Each node is finished when it is bound: nothing planning does after a
    /// child has returned its plan changes that plan, except forms raising
    /// their form's node to the room when they grow in height, which the
    /// form, given that growth, has already done
    /// (`household_layout::Child::grow_height_in_effect`).
    ///
    /// The one difference from binding as planning went: a report callback
    /// now runs once for each node of the finished plan that its child
    /// made, and no longer also for the trial plans that were discarded or
    /// for plans that failed. A report callback must be pure
    /// (`report_information`), so what it reports is the same.
    fn bind_geometry(&self, level: Level, mut plan: GeometryPlan) -> GeometryPlan {
        self.part.bind(level, &mut plan);
        plan
    }

    pub fn view(&self, level: Level) -> Element<'a, Message> {
        if self.part.is_hidden(level) {
            return widget::space().width(0).height(0).into();
        }
        // Preserve the native leaf's public tree identity for direct consumers
        // such as tab strips. Root stage commits use render/VerifiedLeaf; only
        // composites and explicitly decorated leaves need this fixed-level
        // allocation adapter here.
        let note = self.part.note();
        match self.part.kind() {
            Kind::Leaf(leaf)
                if note.decorator.is_none()
                    && note.presentation.is_none()
                    && note.information.is_none() =>
            {
                (leaf.content())(level)
            }
            Kind::Leaf(_) | Kind::Composition(_) | Kind::Frame(_) | Kind::Forms(_) => {
                Element::new(FixedComponent {
                    child: self.clone(),
                    level,
                    content: widget::space().into(),
                })
            }
        }
    }

    /// The engine's measurement of the part (`household_layout::Child::query`).
    pub fn query(&self, level: Level, renderer: &Renderer) -> MeasuredSize {
        self.part.query(level, renderer).into()
    }

    /// Allocate first, then measure native content at its allocated width with
    /// unbounded height. Thus a clipped height can never certify a fit. A useful
    /// minimum narrower than intrinsic width must be explicitly declared.
    // The engine makes the plan (`household_layout::Child::query_at`);
    // binding it to this child, with the reports, follows once it is
    // finished (`bind_geometry`).
    pub fn query_at(
        &self,
        level: Level,
        renderer: &Renderer,
        available: Size,
    ) -> Result<GeometryPlan, AllocationError> {
        self.part
            .query_at(level, renderer, core_size(available))
            .map(|plan| self.bind_geometry(level, GeometryPlan::from_core(&plan)))
    }

    /// Plan this child in `width` × `height`, as its parent does
    /// (`household_layout::Child::plan_width`): the plan has no bindings
    /// and no reports, which `query_at` adds to the plan it returns
    /// (`bind_geometry`).
    #[cfg(test)]
    fn plan_width(
        &self,
        level: Level,
        renderer: &Renderer,
        width: f32,
        height: f32,
    ) -> Result<GeometryPlan, AllocationError> {
        self.part
            .plan_width(level, renderer, width, height)
            .map(|plan| GeometryPlan::from_core(&plan))
    }

    /// Render this child from `plan`, inheriting no growth in height: the
    /// root, or a composition's child (`Commit::render`).
    fn render(
        &self,
        level: Level,
        plan: &GeometryPlan,
        agreement: Rc<Cell<bool>>,
    ) -> Element<'a, Message> {
        self.part.render(level, plan, agreement)
    }

    /// Reserve the full width allocated by the parent, keeping the inner
    /// component's natural geometry and reports intact. Useful for wrapping
    /// navigation followed by trailing controls: a short final row must not
    /// move those controls into space already allocated to navigation.
    pub fn fill_width(self) -> Self {
        Self::from_part(self.part.fill_width())
    }

    /// Reserve exact dimensions for this prepared component. A dimension below
    /// its measured useful minimum is raised to that minimum; insufficient
    /// available space fails the stage rather than shrinking the request.
    /// Apply this to the expanded form, before declaring a collapsed form.
    /// The inner child remains an ordinary nested composition and keeps its
    /// reports and native state. Values must be finite and nonnegative; any
    /// other makes the child invalid (`invalid_unless`).
    pub fn fixed_dimensions(self, width: Option<f32>, height: Option<f32>) -> Self {
        Self::from_part(self.part.fixed_dimensions(width, height))
    }

    /// Decorate this component's rendered content without changing its geometry.
    /// Useful ranges, structured forms and exact-level queries stay unchanged.
    /// Apply only styling or alignment inside the existing component box: no
    /// padding, size changes or overflow. Native commits verify the decorated
    /// size and reject a disagreeing stage. Factories and decorators must be
    /// free of application side effects. Hidden forms are never decorated.
    pub fn map_view(
        mut self,
        decorate: impl Fn(Element<'a, Message>) -> Element<'a, Message> + 'a,
    ) -> Self {
        let note = self.part.note_mut();
        note.decorator = Some(match note.decorator.take() {
            Some(previous) => Rc::new(move |content| decorate(previous(content))),
            None => Rc::new(decorate),
        });
        self.refresh_preparation();
        self
    }

    /// Finite and nonnegative; any other weight makes the child invalid
    /// (`invalid_unless`), as for `grow` and `grow_height`.
    pub fn weight(self, weight: f32) -> Self {
        self.redeclared(|part| part.weight(weight))
    }

    /// Share surplus only after shrinking has finished.
    pub fn grow(self, grow: f32) -> Self {
        self.redeclared(|part| part.grow(grow))
    }

    /// Share surplus height in a column independently of width growth. Native
    /// content keeps its measured height floor; a declared viewport expands.
    pub fn grow_height(self, grow: f32) -> Self {
        self.redeclared(|part| part.grow_height(grow))
    }

    /// Explicit component permission to omit this form. No layout slot, gap,
    /// event target, overlay or accessibility node is built for hidden children.
    pub fn hidden_when(self, hidden: impl Fn(Level) -> bool + 'a) -> Self {
        self.redeclared(move |part| part.hidden_when(hidden))
    }

    pub fn round_width_up(self) -> Self {
        self.redeclared(|part| part.round_width_up())
    }

    /// Set the desired width independently of the measured useful minimum.
    /// A parent may allocate less down to that minimum; when the minimum is
    /// larger than the requested width it remains authoritative. Geometry is
    /// remeasured at the allocated width, including wrapping and chrome.
    pub fn preferred_width(self, width: f32) -> Self {
        self.redeclared(|part| part.preferred_width(width))
    }

    /// Derive a preferred width from the same prepared data and renderer as
    /// the component, without replacing its independently measured minimum.
    pub fn preferred_width_with(
        self,
        preferred: impl Fn(Level, &Renderer, MeasuredSize) -> f32 + 'a,
    ) -> Self {
        self.redeclared(move |part| {
            part.preferred_width_with(move |level, renderer: &Renderer, measured: Measured| {
                preferred(level, renderer, measured.into())
            })
        })
    }

    /// Declare a finite, nonnegative useful width floor. If native intrinsic
    /// width is smaller (including an all-Fill subtree reporting zero), the
    /// preferred offer is raised to the floor. A native leaf with positive
    /// infinite intrinsic width is probed at this declared floor; only a finite
    /// fitting native result can use the floor as its initial preferred offer.
    /// Native content is measured again
    /// at its allocated width, including the actual height of wrapping text.
    /// The component is responsible for making that form usable.
    pub fn minimum_width(self, minimum: impl Fn(Level, Size) -> f32 + 'a) -> Self {
        self.redeclared(move |part| {
            part.minimum_width(move |level, preferred: household_layout::Size| {
                minimum(level, toolkit_size(preferred))
            })
        })
    }

    /// A useful width floor measured from prepared native content using the
    /// query's renderer. The callback is pure and must capture the same data,
    /// typography and presentation as this component's selected view.
    pub fn minimum_width_with(self, minimum: impl Fn(Level, &Renderer, Size) -> f32 + 'a) -> Self {
        self.redeclared(move |part| {
            part.minimum_width_with(
                move |level, renderer: &Renderer, preferred: household_layout::Size| {
                    minimum(level, renderer, toolkit_size(preferred))
                },
            )
        })
    }
}

/// The widgets' half of the engine, over the engine's parts: binding a
/// finished plan to the parts it was planned from (`bind`), and rendering
/// it, with verification, as native widgets (`render`). The engine plans;
/// these walk the plan and the parts together, one arm per kind, reading
/// the parts through the engine's read access and the widgets' bookkeeping
/// from each part's note.
trait Commit<'a, Message> {
    /// The walk of `Child::bind_geometry`, over a node in place.
    fn bind(&self, level: Level, plan: &mut GeometryPlan);

    /// Render this part from `plan`, inheriting no growth in height: the
    /// root, or a composition's child
    /// (`household_layout::Child::grow_height_in_effect`).
    fn render(
        &self,
        level: Level,
        plan: &GeometryPlan,
        agreement: Rc<Cell<bool>>,
    ) -> Element<'a, Message>;

    /// Render this part from `plan`, with the growth in height the forms or
    /// the frame round it pass it, as it was planned.
    fn render_inheriting(
        &self,
        level: Level,
        plan: &GeometryPlan,
        agreement: Rc<Cell<bool>>,
        inherited_growth: f32,
    ) -> Element<'a, Message>;

    /// Render what this part holds, once its binding has matched.
    fn render_content(
        &self,
        level: Level,
        plan: &GeometryPlan,
        agreement: Rc<Cell<bool>>,
        inherited_growth: f32,
    ) -> Element<'a, Message>;
}

impl<'a, Message: 'a> Commit<'a, Message> for Part<'a, Message> {
    fn bind(&self, level: Level, plan: &mut GeometryPlan) {
        if !plan.hidden {
            match self.kind() {
                Kind::Leaf(_) => {}
                Kind::Composition(composition) => {
                    for placed in &mut plan.children {
                        if let Some(child) = composition.children().get(placed.index) {
                            child.bind(level, &mut placed.geometry);
                        }
                    }
                }
                Kind::Frame(frame) => {
                    if let Some(placed) = plan.children.first_mut() {
                        frame.content().bind(level, &mut placed.geometry);
                    }
                }
                Kind::Forms(forms) => forms.select(level).bind(level, plan),
            }
        }
        plan.bindings.push((self.note().preparation.clone(), level));
        if !plan.hidden
            && let Some(information) = &self.note().information
        {
            let losses = (information.report)(level, plan);
            plan.source_reports.push(SourceReport {
                source: information.source,
                revision: information.revision,
                presentation: self.note().presentation,
                level,
                size: plan.size,
                losses,
                owner: self.note().preparation.clone(),
            });
        }
    }

    fn render(
        &self,
        level: Level,
        plan: &GeometryPlan,
        agreement: Rc<Cell<bool>>,
    ) -> Element<'a, Message> {
        self.render_inheriting(level, plan, agreement, 0.0)
    }

    fn render_inheriting(
        &self,
        level: Level,
        plan: &GeometryPlan,
        agreement: Rc<Cell<bool>>,
        inherited_growth: f32,
    ) -> Element<'a, Message> {
        let binding_matches = plan
            .bindings
            .iter()
            .any(|(id, planned_level)| id == &self.note().preparation && *planned_level == level);
        let content = if !binding_matches || plan.hidden {
            widget::space().width(0).height(0).into()
        } else {
            self.render_content(level, plan, agreement.clone(), inherited_growth)
        };
        let content = match &self.note().decorator {
            Some(_) if !binding_matches || plan.hidden => content,
            Some(decorate) => {
                let content = Element::new(VerifiedDecoration {
                    content,
                    expected: plan.size,
                    agreement: agreement.clone(),
                    check_constraints: true,
                });
                Element::new(VerifiedDecoration {
                    content: decorate(content),
                    expected: plan.size,
                    agreement: agreement.clone(),
                    check_constraints: false,
                })
            }
            None => content,
        };
        Element::new(VerifiedBox {
            content,
            reports: if binding_matches && !plan.hidden {
                plan.source_reports
                    .iter()
                    .filter(|report| report.owner == self.note().preparation)
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            },
            regions: if binding_matches && !plan.hidden {
                self.note().regions.clone()
            } else {
                Vec::new()
            },
            binding_matches,
            agreement,
        })
    }

    fn render_content(
        &self,
        level: Level,
        plan: &GeometryPlan,
        agreement: Rc<Cell<bool>>,
        inherited_growth: f32,
    ) -> Element<'a, Message> {
        let grow_height = self.grow_height_in_effect(inherited_growth);
        // Forms, frames and leaves are rendered here; a composition continues
        // below, where its rows or column are built from the plan's placements.
        let composition = match self.kind() {
            Kind::Forms(forms) => {
                let content =
                    forms
                        .select(level)
                        .render_inheriting(level, plan, agreement, grow_height);
                return if grow_height > 0.0 {
                    widget::container(content)
                        .width(plan.size.width)
                        .height(plan.size.height)
                        .into()
                } else {
                    content
                };
            }
            Kind::Frame(frame) => return frame::render(frame, level, plan, agreement, grow_height),
            Kind::Leaf(leaf) => {
                // A leaf's own plan always records the size its view measured
                // (`household_layout::Plan::leaf_size`). A plan without one
                // was made for another kind of child, so it does not match
                // this leaf: render nothing and refuse the stage, exactly as
                // `render` does for a plan whose binding does not match.
                let Some(expected) = plan.native_size else {
                    return refused(agreement);
                };
                // Laid out as it was planned: an ordinary leaf with unbounded
                // height, compressed to its own; a viewport within the height
                // it was offered (`household_layout::Offer`).
                let (maximum_height, compress_height) = match leaf.height() {
                    LeafHeight::Floor => (f32::INFINITY, true),
                    LeafHeight::Range { .. } => (plan.size.height, false),
                };
                let content = Element::new(VerifiedLeaf {
                    content: (leaf.content())(level),
                    expected,
                    maximum: Size::new(plan.size.width, maximum_height),
                    compress_height,
                    agreement,
                });
                return widget::container(content)
                    .width(plan.size.width)
                    .height(plan.size.height)
                    .into();
            }
            Kind::Composition(composition) => composition,
        };
        // A placement's child, rendered in the slot the plan gives it. The
        // plan was made from this composition, so its placements name its
        // children and its rows name its placements; one that names nothing
        // here belongs to another plan, and that part renders nothing and
        // refuses the stage (`refused`).
        let place = |placed: &Placement| -> Element<'a, Message> {
            let Some(child) = composition.children().get(placed.index) else {
                return refused(agreement.clone());
            };
            widget::container(child.render(level, &placed.geometry, agreement.clone()))
                .width(placed.geometry.size.width)
                .height(placed.geometry.size.height)
                .into()
        };
        // The plan has already omitted hidden forms. Native `with_children`
        // filters zero-sized hints, which would discard valid zero-height
        // horizontal spacers (or zero-width vertical slots). Preserve every
        // explicitly allocated slot with `from_vec` instead.
        let make_row = |indices: &[usize]| -> Element<'a, Message> {
            widget::Row::from_vec(
                indices
                    .iter()
                    .map(|i| match plan.children.get(*i) {
                        Some(placed) => place(placed),
                        None => refused(agreement.clone()),
                    })
                    .collect::<Vec<Element<'a, Message>>>(),
            )
            .spacing(composition.spacing())
            .align_y(if composition.top_aligned() {
                cosmic::iced::Alignment::Start
            } else {
                cosmic::iced::Alignment::Center
            })
            .into()
        };
        let content = match composition.arrangement() {
            Arrangement::Row => make_row(plan.rows.first().map_or(&[], Vec::as_slice)),
            Arrangement::Wrap => widget::Column::from_vec(
                plan.rows
                    .iter()
                    .map(|row| make_row(row))
                    .collect::<Vec<_>>(),
            )
            .spacing(composition.spacing())
            .into(),
            Arrangement::Column => widget::Column::from_vec(
                plan.children
                    .iter()
                    .map(place)
                    .collect::<Vec<Element<'a, Message>>>(),
            )
            .spacing(composition.spacing())
            .into(),
        };
        let sharing = self.sharing();
        if grow_height > 0.0 || sharing.fixed.any() || sharing.reserve_width {
            widget::container(content)
                .width(plan.size.width)
                .height(plan.size.height)
                .into()
        } else {
            content
        }
    }
}

/// What a part renders when the plan it is given does not match it: nothing,
/// in a box that refuses the stage when it is laid out, as `render` does for
/// a plan whose binding does not match (`VerifiedBox` with `binding_matches`
/// false). The stage is rejected, so the next declared stage, or the
/// fallback, is taken; nothing is laid out at a size the plan did not give.
fn refused<'a, Message: 'a>(agreement: Rc<Cell<bool>>) -> Element<'a, Message> {
    Element::new(VerifiedBox {
        content: widget::space().width(0).height(0).into(),
        reports: Vec::new(),
        regions: Vec::new(),
        binding_matches: false,
        agreement,
    })
}

pub fn child<'a, Message: 'a>(
    view: impl Fn(Level) -> Element<'a, Message> + 'a,
) -> Child<'a, Message> {
    let view: ViewFactory<'a, Message> = Rc::new(view);
    Child::from_part(Part::leaf(view))
}

/// A component family's own structured forms, inherited until the next
/// threshold. The first threshold must be Preferred and thresholds strictly
/// increase. Every selected form receives the requested level, so an inherited
/// composite still forwards Mild/Strong rules to its own immediate children.
/// The returned child's outward weights are independent of the form's internals.
/// Thresholds that break either rule make the child invalid (`invalid_unless`).
pub fn forms<'a, Message: 'a>(
    alternatives: Vec<(Level, Child<'a, Message>)>,
) -> Child<'a, Message> {
    let mut alternatives = alternatives.into_iter();
    let Some((first_threshold, first)) = alternatives.next() else {
        // No form at all: there is nothing to stand for the family, so the
        // invalid child is a leaf that shows nothing. Being invalid, it is
        // never measured or planned.
        return child(|_| widget::space().width(0).height(0).into()).invalid_unless(false);
    };
    let later: Vec<_> = alternatives
        .map(|(threshold, form)| (threshold, form.part))
        .collect();
    Child::from_part(Part::forms((first_threshold, first.part), later))
}

/// Declare a native scrolling viewport, including its scrollbar and any other
/// controls in `view`. The declaration is local; no stage invents scrolling.
/// `view` must actually implement scrolling at the offered viewport dimensions.
/// If a Fill viewport has no finite intrinsic preferred dimension, its declared
/// minimum supplies that initial offer after a bounded native probe succeeds.
pub fn scroll_child<'a, Message: 'a>(
    view: impl Fn(Level) -> Element<'a, Message> + 'a,
    minimum: Size,
) -> Child<'a, Message> {
    // The engine's measuring interpreter rejects a bad floor too,
    // but a replaced measurement discards that entry; the mark does not go.
    scroll_child_with_minimum(view, move |_, _| minimum).invalid_unless(finite_size(minimum))
}

/// Declare a scrolling viewport whose useful floor is measured with the same
/// native renderer as its contents. A changed level or typography therefore
/// refreshes both measurement and selected viewport geometry. Nonfinite or
/// negative queried floors are explicit invalid input, never clipped viewports.
///
/// This is the one declaration of a leaf that takes a range of heights
/// (`household_layout::Child::ranged_leaf`, `LeafHeight::Range`); the name
/// is the apps' name for their scrolling areas. The floor first applies to
/// the leaf's measurement, before anything declared on the child after.
pub fn scroll_child_with_minimum<'a, Message: 'a>(
    view: impl Fn(Level) -> Element<'a, Message> + 'a,
    minimum: impl Fn(Level, &Renderer) -> Size + 'a,
) -> Child<'a, Message> {
    let view: ViewFactory<'a, Message> = Rc::new(view);
    Child::from_part(Part::ranged_leaf(
        view,
        move |level, renderer: &Renderer| core_size(minimum(level, renderer)),
    ))
}

pub fn spacer<'a, Message: 'a>(minimum: f32) -> Child<'a, Message> {
    child(|_| widget::Space::new().width(Length::Fill).height(0).into())
        .with_measurement(move |_, _| MeasuredSize {
            preferred: Size::new(minimum, 0.0),
            minimum: Size::new(minimum, 0.0),
        })
        .weight(0.0)
        .grow(1.0)
        .invalid_unless(is_amount(minimum))
}

impl<'a, Message: 'a> Child<'a, Message> {
    /// Measure this child with `query` instead: what its kind measures and
    /// every adjustment declared so far no longer count, as the list shows by
    /// starting again from the replacement. Adjustments declared after this
    /// apply to the replacement.
    pub(crate) fn with_measurement(
        self,
        query: impl Fn(Level, &Renderer) -> MeasuredSize + 'a,
    ) -> Self {
        self.redeclared(move |part| {
            part.with_measurement(move |level, renderer: &Renderer| query(level, renderer).into())
        })
    }
}

pub fn row_child<'a, Message: 'a>(
    children: Vec<Child<'a, Message>>,
    spacing: f32,
) -> Child<'a, Message> {
    Child::from_part(Part::row(parts(children), spacing))
}

/// Arrange a row with each immediate child aligned to its top edge.
pub fn row_child_top<'a, Message: 'a>(
    children: Vec<Child<'a, Message>>,
    spacing: f32,
) -> Child<'a, Message> {
    Child::from_part(Part::row_top(parts(children), spacing))
}

pub fn column_child<'a, Message: 'a>(
    children: Vec<Child<'a, Message>>,
    spacing: f32,
) -> Child<'a, Message> {
    Child::from_part(Part::column(parts(children), spacing))
}

pub fn wrap_child<'a, Message: 'a>(
    children: Vec<Child<'a, Message>>,
    spacing: f32,
) -> Child<'a, Message> {
    Child::from_part(Part::wrap(parts(children), spacing))
}

/// The engine's parts of `children`, in their order.
fn parts<'a, Message>(children: Vec<Child<'a, Message>>) -> Vec<Part<'a, Message>> {
    children.into_iter().map(|child| child.part).collect()
}

/// The core's checks of a size (`household_layout::finite_size`,
/// `household_layout::validate_measurement`), on iced's sizes.
fn finite_size(size: Size) -> bool {
    household_layout::finite_size(core_size(size))
}
#[cfg(test)]
fn validate_measurement(size: MeasuredSize) -> Result<(), AllocationError> {
    household_layout::validate_measurement(size.into())
}

/// One exact level and component arrangement in the parent's ordered policy.
pub struct Stage<'a, Message> {
    pub level: Level,
    pub content: Child<'a, Message>,
}
impl<'a, Message> Stage<'a, Message> {
    pub fn new(level: Level, content: Child<'a, Message>) -> Self {
        Self { level, content }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StagePlan {
    pub stage_index: usize,
    pub level: Level,
    pub geometry: GeometryPlan,
}

/// No declared stage supplied complete fitting geometry. Nothing is implicitly
/// hidden, clipped, scrolled or promoted to a stronger undeclared level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoFit {
    pub attempted: usize,
}

pub fn choose_stage<'a, Message: 'a>(
    stages: &[Stage<'a, Message>],
    renderer: &Renderer,
    available: Size,
) -> Result<StagePlan, NoFit> {
    choose_stage_inner(stages, renderer, available)
}
fn choose_stage_inner<'a, Message: 'a>(
    stages: &[Stage<'a, Message>],
    renderer: &Renderer,
    available: Size,
) -> Result<StagePlan, NoFit> {
    for (stage_index, stage) in stages.iter().enumerate() {
        if let Ok(geometry) = stage.content.query_at(stage.level, renderer, available) {
            return Ok(StagePlan {
                stage_index,
                level: stage.level,
                geometry,
            });
        }
    }
    Err(NoFit {
        attempted: stages.len(),
    })
}

pub fn stages<'a, Message: 'a>(stages: Vec<Stage<'a, Message>>) -> ResponsiveLevels<'a, Message> {
    ResponsiveLevels {
        stages,
        width: Length::Fill,
        height: Length::Shrink,
        content: widget::space().width(0).height(0).into(),
        selection: None,
        fallback: None,
    }
}

/// Compatibility convenience: the same component at successive permitted levels.
pub fn responsive<'a, Message: 'a>(
    max_level: Level,
    view: impl Fn(Level) -> Element<'a, Message> + 'a,
) -> ResponsiveLevels<'a, Message> {
    responsive_child(max_level, child(view))
}

pub fn responsive_child<'a, Message: 'a>(
    max_level: Level,
    child: Child<'a, Message>,
) -> ResponsiveLevels<'a, Message> {
    stages(
        Level::ALL
            .into_iter()
            .take_while(|level| *level <= max_level)
            .map(|level| Stage::new(level, child.clone()))
            .collect(),
    )
}

pub fn row<'a, Message: 'a>(
    max_level: Level,
    children: Vec<Child<'a, Message>>,
    spacing: f32,
) -> ResponsiveLevels<'a, Message> {
    responsive_child(max_level, row_child(children, spacing))
}

pub struct ResponsiveLevels<'a, Message> {
    stages: Vec<Stage<'a, Message>>,
    width: Length,
    height: Length,
    content: Element<'a, Message>,
    selection: Option<Result<StagePlan, NoFit>>,
    fallback: Option<Box<dyn Fn() -> Element<'a, Message> + 'a>>,
}

impl<'a, Message: 'a> ResponsiveLevels<'a, Message> {
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }
    pub fn selection(&self) -> Option<&Result<StagePlan, NoFit>> {
        self.selection.as_ref()
    }
    pub fn fallback(mut self, fallback: impl Fn() -> Element<'a, Message> + 'a) -> Self {
        self.fallback = Some(Box::new(fallback));
        self
    }
}

// A component decorator may contain a whole nested composition. Validate its
// own box before the parent's fixed slot could conceal a changed requirement.
struct VerifiedDecoration<'a, Message> {
    content: Element<'a, Message>,
    expected: Size,
    agreement: Rc<Cell<bool>>,
    check_constraints: bool,
}
impl<Message> Widget<Message, Theme, Renderer> for VerifiedDecoration<'_, Message> {
    forward_widget!(Message, Theme, Renderer; children, diff, size, draw, update, operate, mouse_interaction, overlay, drag_destinations);
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let limits = if self.check_constraints {
            // Padding or a smaller explicit size must not conceal overflow by
            // reducing the raw component's already allocated box.
            if limits.max().width + FIT_EPSILON < self.expected.width
                || limits.max().height + FIT_EPSILON < self.expected.height
            {
                self.agreement.set(false);
            }
            *limits
        } else {
            layout::Limits::with_compression(
                Size::ZERO,
                Size::new(self.expected.width, f32::INFINITY),
                Size::new(true, true),
            )
        };
        let Some(content) = tree.children.first_mut() else {
            return refused_node(&self.agreement);
        };
        let node = self
            .content
            .as_widget_mut()
            .layout(content, renderer, &limits);
        if !finite_size(node.size())
            || (node.size().width - self.expected.width).abs() > FIT_EPSILON
            || (node.size().height - self.expected.height).abs() > FIT_EPSILON
        {
            self.agreement.set(false);
        }
        node
    }
    #[cfg(feature = "a11y")]
    fn a11y_nodes(
        &self,
        layout: Layout<'_>,
        tree: &Tree,
        cursor: mouse::Cursor,
    ) -> iced_accessibility::A11yTree {
        let Some(content) = tree.children.first() else {
            return iced_accessibility::A11yTree::default();
        };
        self.content.as_widget().a11y_nodes(layout, content, cursor)
    }
}

struct VerifiedLeaf<'a, Message> {
    content: Element<'a, Message>,
    expected: Size,
    maximum: Size,
    compress_height: bool,
    agreement: Rc<Cell<bool>>,
}
impl<Message> Widget<Message, Theme, Renderer> for VerifiedLeaf<'_, Message> {
    forward_widget!(Message, Theme, Renderer; children, diff, size, draw, update, operate, mouse_interaction, overlay, drag_destinations);
    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, _: &layout::Limits) -> layout::Node {
        let Some(content) = tree.children.first_mut() else {
            return refused_node(&self.agreement);
        };
        let node = self.content.as_widget_mut().layout(
            content,
            renderer,
            &layout::Limits::with_compression(
                Size::ZERO,
                self.maximum,
                Size::new(self.maximum.width.is_infinite(), self.compress_height),
            ),
        );
        if !finite_size(node.size())
            || (node.size().width - self.expected.width).abs() > FIT_EPSILON
            || (node.size().height - self.expected.height).abs() > FIT_EPSILON
        {
            self.agreement.set(false);
        }
        node
    }
    #[cfg(feature = "a11y")]
    fn a11y_nodes(
        &self,
        layout: Layout<'_>,
        tree: &Tree,
        cursor: mouse::Cursor,
    ) -> iced_accessibility::A11yTree {
        let Some(content) = tree.children.first() else {
            return iced_accessibility::A11yTree::default();
        };
        self.content.as_widget().a11y_nodes(layout, content, cursor)
    }
}

// A standalone fixed-level view uses the same geometry implementation as stages.
struct FixedComponent<'a, Message> {
    child: Child<'a, Message>,
    level: Level,
    content: Element<'a, Message>,
}
impl<'a, Message: 'a> Widget<Message, Theme, Renderer> for FixedComponent<'a, Message> {
    forward_widget!(Message, Theme, Renderer; draw, update, operate, mouse_interaction, overlay, drag_destinations);
    fn diff(&mut self, _: &mut Tree) {}
    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let plan = self.child.query_at(self.level, renderer, limits.max());
        let agreement = Rc::new(Cell::new(true));
        self.content = match plan {
            Ok(plan) => self.child.render(self.level, &plan, agreement.clone()),
            Err(_) => widget::space().width(0).height(0).into(),
        };
        match layout_content(tree, &mut self.content, renderer, limits) {
            Some(node) if agreement.get() => node,
            _ => {
                self.content = widget::space().width(0).height(0).into();
                layout_content(tree, &mut self.content, renderer, limits)
                    .unwrap_or_else(|| layout::Node::new(Size::ZERO))
            }
        }
    }
    #[cfg(feature = "a11y")]
    fn a11y_nodes(
        &self,
        layout: Layout<'_>,
        tree: &Tree,
        cursor: mouse::Cursor,
    ) -> iced_accessibility::A11yTree {
        let Some(content) = tree.children.first() else {
            return iced_accessibility::A11yTree::default();
        };
        self.content.as_widget().a11y_nodes(layout, content, cursor)
    }
}

/// Reconcile a wrapper's tree with its one content element and lay the
/// content out within `limits`. `diff_children` leaves the tree one child
/// per element it is given, so the content's own tree is the first child.
/// Were it missing, the content is not laid out and there is no node; the
/// caller then lays out nothing there, as the engine renders nothing for a
/// plan that does not match.
fn layout_content<Message>(
    tree: &mut Tree,
    content: &mut Element<'_, Message>,
    renderer: &Renderer,
    limits: &layout::Limits,
) -> Option<layout::Node> {
    tree.diff_children(std::slice::from_mut(content));
    let content_tree = tree.children.first_mut()?;
    Some(
        content
            .as_widget_mut()
            .layout(content_tree, renderer, limits),
    )
}

/// What a verification wrapper lays out when its tree has no child for its
/// content: nothing, and the stage it belongs to is refused, as for a plan
/// that does not match (`refused`).
fn refused_node(agreement: &Cell<bool>) -> layout::Node {
    agreement.set(false);
    layout::Node::new(Size::ZERO)
}

// Tree::diff and layout-time diffing (e.g. a nested native Responsive) consult
// this runtime map. A fresh Tree alone would let a discarded probe steal focus
// or popup state from the committed UI. Restore even when a factory panics.
struct NamedStateGuard<F: FnOnce()>(Option<F>);

thread_local! {
    /// How many disposable measurements are under way on this thread
    static SPECULATING: Cell<usize> = const { Cell::new(0) };
}

/// Whether the layout under way is a disposable measurement, rather than
/// the layout presented. Widgets that keep geometry across frames (a
/// virtualised list's item heights, its reading position) read it then,
/// but commit nothing: queries cannot edit retained state.
pub fn is_speculative() -> bool {
    SPECULATING.with(Cell::get) > 0
}

/// Mark a disposable measurement for as long as the guard lives
pub(crate) fn speculate() -> impl Drop {
    SPECULATING.with(|depth| depth.set(depth.get() + 1));
    NamedStateGuard(Some(|| {
        SPECULATING.with(|depth| depth.set(depth.get() - 1))
    }))
}

pub(crate) fn isolate_named_state() -> impl Drop {
    let saved = tree::NAMED.with(|named| std::mem::take(&mut *named.borrow_mut()));
    NamedStateGuard(Some(move || {
        tree::NAMED.with(|named| *named.borrow_mut() = saved)
    }))
}

pub(crate) fn measure_element<'a, Message>(
    view: impl FnOnce() -> Element<'a, Message>,
    renderer: &Renderer,
) -> Size {
    measure_element_in(view, renderer, Size::INFINITE, true)
}

fn measure_element_in<'a, Message>(
    view: impl FnOnce() -> Element<'a, Message>,
    renderer: &Renderer,
    maximum: Size,
    compress_height: bool,
) -> Size {
    // Only an unbounded intrinsic probe compresses Fill widths. At an
    // allocated finite width, native Fill columns must consume that offer.
    let _isolation = isolate_named_state();
    let _speculation = speculate();
    let mut candidate = view();
    let mut tree = Tree::new(candidate.as_widget());
    tree.diff(candidate.as_widget_mut());
    candidate
        .as_widget_mut()
        .layout(
            &mut tree,
            renderer,
            &layout::Limits::with_compression(
                Size::ZERO,
                maximum,
                Size::new(maximum.width.is_infinite(), compress_height),
            ),
        )
        .size()
}

impl<F: FnOnce()> Drop for NamedStateGuard<F> {
    fn drop(&mut self) {
        if let Some(restore) = self.0.take() {
            restore();
        }
    }
}

impl<'view, Message: 'view> Widget<Message, Theme, Renderer> for ResponsiveLevels<'view, Message> {
    fn diff(&mut self, _tree: &mut Tree) {
        // The parent has not allocated width yet. Reconcile the selected
        // representation in layout, like libcosmic's native Responsive.
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let limits = limits.width(self.width).height(self.height);
        let available = limits.max();
        let mut rejected_live_tree = false;
        for (stage_index, stage) in self.stages.iter().enumerate() {
            let Ok(geometry) = stage.content.query_at(stage.level, renderer, available) else {
                continue;
            };
            if rejected_live_tree {
                // A failed native candidate may have a different arrangement.
                // Save named state before reconciling the next shape, just as
                // the native runtime does at a view rebuild boundary.
                let preserved = tree.take_all_named();
                tree::NAMED.with(|named| named.borrow_mut().extend(preserved));
            }
            let agreement = Rc::new(Cell::new(true));
            self.content = stage
                .content
                .render(stage.level, &geometry, agreement.clone());
            // Ordinary native content always retains unbounded height here.
            // Finite-height clipping cannot certify a successful stage.
            let commit_limits = layout::Limits::with_compression(
                Size::ZERO,
                Size::new(geometry.size.width, f32::INFINITY),
                Size::new(true, true),
            );
            // Content that could not be laid out does not agree.
            if let Some(node) = layout_content(tree, &mut self.content, renderer, &commit_limits)
                && agreement.get()
                && finite_size(node.size())
                && (node.size().width - geometry.size.width).abs() <= FIT_EPSILON
                && (node.size().height - geometry.size.height).abs() <= FIT_EPSILON
            {
                self.selection = Some(Ok(StagePlan {
                    stage_index,
                    level: stage.level,
                    geometry,
                }));
                return layout::Node::with_children(
                    limits.resolve(self.width, self.height, node.size()),
                    vec![node],
                );
            }
            // No hook can observe this intermediate candidate. Continue in the
            // parent's declared order, never adding a stronger implicit level.
            rejected_live_tree = true;
        }
        self.selection = Some(Err(NoFit {
            attempted: self.stages.len(),
        }));
        self.content = self.fallback.as_ref().map_or_else(
            || widget::space().width(0).height(0).into(),
            |fallback| fallback(),
        );
        match layout_content(tree, &mut self.content, renderer, &limits.loose()) {
            Some(node) => layout::Node::with_children(
                limits.resolve(self.width, self.height, node.size()),
                vec![node],
            ),
            // Nothing to show: the widget's own box, with no child in it
            // (`child_layout`).
            None => layout::Node::new(limits.resolve(self.width, self.height, Size::ZERO)),
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let (Some(layout), Some(content)) = (child_layout(layout), tree.children.first()) else {
            return;
        };
        self.content
            .as_widget()
            .draw(content, renderer, theme, style, layout, cursor, viewport);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let (Some(layout), Some(content)) = (child_layout(layout), tree.children.first_mut())
        else {
            return;
        };
        self.content.as_widget_mut().update(
            content, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let (Some(layout), Some(content)) = (child_layout(layout), tree.children.first_mut())
        else {
            return;
        };
        self.content
            .as_widget_mut()
            .operate(content, layout, renderer, operation);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let (Some(layout), Some(content)) = (child_layout(layout), tree.children.first()) else {
            return mouse::Interaction::default();
        };
        self.content
            .as_widget()
            .mouse_interaction(content, layout, cursor, viewport, renderer)
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        let layout = child_layout(layout)?;
        let content = tree.children.first_mut()?;
        self.content
            .as_widget_mut()
            .overlay(content, layout, renderer, viewport, translation)
    }

    fn drag_destinations(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        destinations: &mut cosmic::iced::advanced::clipboard::DndDestinationRectangles,
    ) {
        let (Some(layout), Some(content)) = (child_layout(layout), tree.children.first()) else {
            return;
        };
        self.content
            .as_widget()
            .drag_destinations(content, layout, renderer, destinations);
    }

    #[cfg(feature = "a11y")]
    fn a11y_nodes(
        &self,
        layout: Layout<'_>,
        tree: &Tree,
        cursor: mouse::Cursor,
    ) -> iced_accessibility::A11yTree {
        let (Some(layout), Some(content)) = (child_layout(layout), tree.children.first()) else {
            return iced_accessibility::A11yTree::default();
        };
        self.content.as_widget().a11y_nodes(layout, content, cursor)
    }
}

/// The layout of the selected representation (or the fallback): the one
/// child of the node `ResponsiveLevels::layout` returns. Every layout it
/// returns has that child. Given a layout without one, which was not made by
/// this widget's layout, there is nothing of this widget's to draw, update,
/// operate on or point at there, so each caller does nothing and answers
/// with the neutral value (no interaction, no overlay, no accessibility
/// nodes), as the engine renders nothing for a plan that does not match. The
/// same holds for a tree without the content's own tree, the first child
/// `layout_content` leaves in it.
fn child_layout(layout: Layout<'_>) -> Option<Layout<'_>> {
    layout
        .children()
        .next()
        .map(|child| child.with_virtual_offset(layout.virtual_offset()))
}

impl<'a, Message: 'a> From<ResponsiveLevels<'a, Message>> for Element<'a, Message> {
    fn from(responsive: ResponsiveLevels<'a, Message>) -> Self {
        Self::new(responsive)
    }
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "layout_contract_tests.rs"]
mod contract_tests;

#[cfg(test)]
#[path = "layout_engine_tests.rs"]
mod engine_tests;

#[cfg(test)]
#[path = "layout_decorator_tests.rs"]
mod decorator_tests;

#[cfg(test)]
#[path = "layout_input_tests.rs"]
mod input_tests;

#[cfg(test)]
#[path = "layout_presentation_tests.rs"]
mod presentation_tests;

#[cfg(test)]
#[path = "layout_measuring_tests.rs"]
mod measuring_tests;

#[cfg(test)]
#[path = "layout_height_tests.rs"]
mod height_tests;

#[cfg(test)]
#[path = "layout_invalid_tests.rs"]
mod invalid_tests;

#[cfg(test)]
#[path = "layout_mismatch_tests.rs"]
mod mismatch_tests;

#[cfg(test)]
#[path = "layout_arrangement_tests.rs"]
mod arrangement_tests;

#[cfg(test)]
#[path = "layout_test_fixtures.rs"]
mod test_fixtures;

#[cfg(test)]
#[path = "layout_width_dependent_tests.rs"]
mod width_dependent_tests;

#[cfg(test)]
#[path = "layout_overflow_tests.rs"]
mod overflow_tests;

#[cfg(test)]
#[path = "layout_unbounded_tests.rs"]
mod unbounded_tests;

#[cfg(test)]
#[path = "layout_offer_tests.rs"]
mod offer_tests;

#[cfg(test)]
#[path = "layout_tolerance_tests.rs"]
mod tolerance_tests;

#[cfg(test)]
#[path = "layout_decision_tests.rs"]
mod decision_tests;

#[cfg(test)]
#[path = "layout_binding_tests.rs"]
mod binding_tests;

#[cfg(test)]
mod renderer_minimum_tests {
    use super::*;
    use cosmic::iced::advanced::renderer::Headless;
    use cosmic::iced::{Font, Pixels};

    fn renderer() -> Renderer {
        cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
            Font::default(),
            Pixels(14.0),
            Some("tiny-skia"),
        ))
        .expect("headless tiny-skia renderer")
    }

    fn floor(size: f32) -> Child<'static, ()> {
        child(move |_| {
            widget::container(widget::text("More models").size(size))
                .padding([4.0, 12.0])
                .into()
        })
    }

    #[test]
    fn useful_width_uses_actual_native_typography_in_query_and_commit() {
        let renderer = renderer();
        let small = floor(12.0);
        let large = floor(24.0);
        let small_width = small.query(Level::Preferred, &renderer).preferred.width;
        let large_width = large.query(Level::Preferred, &renderer).preferred.width;
        assert!(
            large_width > small_width,
            "the native font determines the useful floor"
        );
        let content: Child<'_, ()> = child(|_| {
            widget::text("A wrapping model label that stays complete")
                .width(Length::Fill)
                .into()
        })
        .minimum_width_with(move |level, renderer, _| {
            if level >= Level::Mild {
                large.query(level, renderer).preferred.width
            } else {
                small.query(level, renderer).preferred.width
            }
        });
        for (level, width) in [(Level::Preferred, small_width), (Level::Mild, large_width)] {
            assert_eq!(content.query(level, &renderer).minimum.width, width);
            assert!(
                content
                    .query_at(level, &renderer, Size::new(width - 1.0, 1000.0))
                    .is_err()
            );
            let mut selected = stages(vec![Stage::new(level, content.clone())]);
            let mut tree = Tree::empty();
            let node = selected.layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(width, 1000.0)),
            );
            assert!(
                selected.selection().unwrap().is_ok(),
                "native commit must verify the measured floor"
            );
            assert_eq!(node.size().width, width);
        }
    }

    #[test]
    fn scrolling_floor_refreshes_with_native_typography_and_rejects_small_offers() {
        let renderer = renderer();
        for font_size in [12.0, 24.0] {
            let probe = floor(font_size);
            let minimum = probe.query(Level::Preferred, &renderer).preferred;
            let content: Child<'_, ()> = scroll_child_with_minimum(
                move |_| {
                    widget::scrollable(widget::text("A model row\n".repeat(50)).size(font_size))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .into()
                },
                move |level, renderer| probe.query(level, renderer).preferred,
            );
            assert_eq!(content.query(Level::Preferred, &renderer).minimum, minimum);
            assert!(
                content
                    .query_at(
                        Level::Preferred,
                        &renderer,
                        Size::new(minimum.width - 1.0, 100.0)
                    )
                    .is_err()
            );
            assert!(
                content
                    .query_at(
                        Level::Preferred,
                        &renderer,
                        Size::new(minimum.width, minimum.height - 1.0)
                    )
                    .is_err()
            );
            let mut selected = stages(vec![Stage::new(Level::Preferred, content)]);
            let mut tree = Tree::empty();
            let node = selected.layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, minimum),
            );
            assert!(selected.selection().unwrap().is_ok());
            assert_eq!(node.size(), minimum);
        }
    }

    #[test]
    fn invalid_measured_floors_do_not_produce_a_plan() {
        let renderer = renderer();
        for invalid in [f32::NAN, f32::INFINITY, -1.0] {
            let width: Child<'_, ()> =
                child(|_| widget::text("Model").into()).minimum_width_with(move |_, _, _| invalid);
            assert_eq!(
                width.query_at(Level::Preferred, &renderer, Size::new(300.0, 300.0)),
                Err(AllocationError::InvalidInput)
            );
            let scrolling: Child<'_, ()> = scroll_child_with_minimum(
                |_| widget::scrollable(widget::text("Model")).into(),
                move |_, _| Size::new(invalid, 32.0),
            );
            assert_eq!(
                scrolling.query_at(Level::Preferred, &renderer, Size::new(300.0, 300.0)),
                Err(AllocationError::InvalidInput)
            );
        }
    }
}
