// SPDX-License-Identifier: GPL-3.0-only

//! What a part is: its kind, the rules it declares, and the builders that
//! declare them.

use crate::frame::{Frame, Padding};
use crate::measure::{Adjustment, MinimumWidth, PreferredWidth, SizeQuery};
use crate::{Arrangement, Host, Level, Measured, Size};
use std::rc::Rc;

/// A composition: immediate children in a row, column or wrap, planned
/// together (`Child::row`, `Child::row_top`, `Child::column`,
/// `Child::wrap`, and the wrappers `Child::fill_width` and
/// `Child::fixed_dimensions`, which put one child in a column).
pub struct Composition<'a, H: Host> {
    pub(crate) children: Vec<Child<'a, H>>,
    pub(crate) arrangement: Arrangement,
    pub(crate) spacing: f32,
    pub(crate) top_aligned: bool,
}

impl<'a, H: Host> Composition<'a, H> {
    /// The declared children, hidden or not, in their order. A placement of
    /// a composition's plan names its child by its index here.
    pub fn children(&self) -> &[Child<'a, H>] {
        &self.children
    }

    /// How the children are set out.
    pub fn arrangement(&self) -> Arrangement {
        self.arrangement
    }

    /// The spacing between visible children, and between a wrap's rows.
    pub fn spacing(&self) -> f32 {
        self.spacing
    }

    /// Whether a row aligns each child to its top edge (`Child::row_top`)
    /// rather than centring it on the row's height.
    pub fn top_aligned(&self) -> bool {
        self.top_aligned
    }
}

/// A component family's forms (`Child::forms`): the first form, which
/// holds from `Preferred`, and the later ones, each from its threshold.
///
/// The first form is a field of its own, so the form selected at a level
/// always exists: it is the last later form whose threshold the level has
/// reached, or else the first. Forms built from thresholds that do not start
/// at `Preferred` or do not strictly increase make an invalid child
/// (`Child::invalid_unless`), which is never measured or planned, so the
/// selection over such thresholds only decides where that child is hidden.
pub struct Forms<'a, H: Host> {
    first: Child<'a, H>,
    later: Vec<(Level, Child<'a, H>)>,
}

impl<'a, H: Host> Forms<'a, H> {
    /// The form that stands for the family at `level`.
    pub fn select(&self, level: Level) -> &Child<'a, H> {
        self.later
            .iter()
            .rev()
            .find(|(threshold, _)| *threshold <= level)
            .map_or(&self.first, |(_, form)| form)
    }
}

/// What a child is: exactly one of four kinds, each holding only what that
/// kind needs.
///
/// - `Leaf`: the host's leaf and how it takes height (`Leaf`). The engine
///   knows the leaf only through the host's one question (`Host::size`),
///   which it asks both for disposable measurements and for the plan it
///   keeps (`Child::leaf`, `Child::ranged_leaf`).
/// - `Composition`: immediate children in a row, column or wrap, planned
///   together (`Child::row`, `Child::row_top`, `Child::column`,
///   `Child::wrap`, and the wrappers `Child::fill_width` and
///   `Child::fixed_dimensions`, which put one child in a column).
/// - `Frame`: a fixed padding round one child, drawn by the host's chrome
///   (`Child::frame`).
/// - `Forms`: a component family's own alternatives, one per level threshold;
///   the form selected at a level is planned and drawn in the family's
///   place (`Child::forms`).
///
/// Every question whose answer depends on the kind (is it hidden, how is it
/// measured, how is it planned, and in the host how it is drawn) is one
/// `match` on this enum with no wildcard arm, so a new kind cannot compile
/// until each of those questions has an answer for it. A child cannot be two
/// kinds at once, and only a leaf has a host's leaf: nothing has to check
/// which optional part is set, and no placeholder leaf stands in for the
/// kinds without one.
pub enum Kind<'a, H: Host> {
    Leaf(Leaf<'a, H>),
    Composition(Rc<Composition<'a, H>>),
    Frame(Rc<Frame<'a, H>>),
    Forms(Rc<Forms<'a, H>>),
}

/// The host's leaf, and how it takes height.
pub struct Leaf<'a, H: Host> {
    pub(crate) content: H::Leaf,
    pub(crate) height: LeafHeight<'a, H>,
}

impl<'a, H: Host> Leaf<'a, H> {
    /// The host's leaf, as the host built it.
    pub fn content(&self) -> &H::Leaf {
        &self.content
    }

    /// How the leaf takes height.
    pub fn height(&self) -> &LeafHeight<'a, H> {
        &self.height
    }
}

/// A leaf's floor of size, measured by the caller with the host's context.
pub type MinimumQuery<'a, H> = Rc<dyn Fn(Level, &<H as Host>::Context) -> Size + 'a>;

/// How a leaf takes height: the engine's rule for height, declared where it
/// applies.
///
/// A leaf's height is measured at the width it is allocated, with unbounded
/// room, and that height is a hard floor: offered less, the leaf does not
/// fit. Only a leaf that takes a range of heights is offered a finite
/// height, and only down to the floor it declares. The reason is that a
/// leaf laid out in less height than it needs is clipped, and a clipped
/// height must never pass for a fit; a leaf that takes a range of heights
/// is one built to work in less height (a native viewport scrolls), and
/// says how little.
///
/// A column shares a shortage of height only among the parts whose plans
/// record a floor below their height, which only such a leaf's plan, or a
/// plan containing one, does (`Composition::plan_column`); a wrap does the
/// same between its rows (`Composition::plan_wrap`).
///
/// household-widgets declares such a leaf as a scrolling viewport
/// (`scroll_child`, `scroll_child_with_minimum`), because that is how the
/// apps declare their scrolling areas; what it declares is a leaf that
/// takes a range of heights (`Child::ranged_leaf`).
pub enum LeafHeight<'a, H: Host> {
    /// An ordinary leaf: its measured height is a floor, and the leaf is
    /// planned at exactly that height (`Child::leaf`).
    Floor,
    /// A leaf that works at any height from its floor up, and is offered
    /// one in that range. `minimum` is its floor, measured by the caller
    /// with the same context as the leaf (`Child::ranged_leaf`).
    Range { minimum: MinimumQuery<'a, H> },
}

// Written by hand: the floor is an `Rc`, and a derived `Clone` would ask
// for `H: Clone`.
impl<H: Host> Clone for LeafHeight<'_, H> {
    fn clone(&self) -> Self {
        match self {
            Self::Floor => Self::Floor,
            Self::Range { minimum } => Self::Range {
                minimum: minimum.clone(),
            },
        }
    }
}

// Written by hand, as for `Kind`: a derived `Clone` would ask for
// `H: Clone`; the host's leaf is cloned as the host clones it.
impl<H: Host> Clone for Leaf<'_, H>
where
    H::Leaf: Clone,
{
    fn clone(&self) -> Self {
        Self {
            content: self.content.clone(),
            height: self.height.clone(),
        }
    }
}

// Written by hand, as for `Child`: every variant but the leaf holds an
// `Rc`, so a clone shares the prepared parts and needs no `H: Clone`.
impl<H: Host> Clone for Kind<'_, H>
where
    H::Leaf: Clone,
{
    fn clone(&self) -> Self {
        match self {
            Self::Leaf(leaf) => Self::Leaf(leaf.clone()),
            Self::Composition(composition) => Self::Composition(composition.clone()),
            Self::Frame(frame) => Self::Frame(frame.clone()),
            Self::Forms(forms) => Self::Forms(forms.clone()),
        }
    }
}

/// Which of a child's dimensions are fixed (`Child::fixed_dimensions`).
///
/// The numbers a caller gives are not kept here: they go into the child's
/// measurement (`Adjustment::FixedDimensions`), which raises each to the
/// content's minimum. What the planner fixes is the child's measured minimum
/// in each fixed dimension, every adjustment included, and it obtains those
/// sizes in one step (`Fixed::sizes`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fixed {
    pub width: bool,
    pub height: bool,
}

/// The size of each fixed dimension at one level; `None` where the dimension
/// is not fixed.
#[derive(Clone, Copy)]
pub(crate) struct FixedSizes {
    pub(crate) width: Option<f32>,
    pub(crate) height: Option<f32>,
}

impl Fixed {
    /// The fixed sizes, taken from one measurement of the child, which is made
    /// only when a dimension is fixed.
    pub(crate) fn sizes(self, measure: impl FnOnce() -> Measured) -> FixedSizes {
        if !self.width && !self.height {
            return FixedSizes {
                width: None,
                height: None,
            };
        }
        let minimum = measure().minimum;
        FixedSizes {
            width: self.width.then_some(minimum.width),
            height: self.height.then_some(minimum.height),
        }
    }

    /// Whether either dimension is fixed.
    pub fn any(self) -> bool {
        self.width || self.height
    }
}

/// How a part takes part in sharing room with the parts beside it: the
/// plain numbers it declares, as it declared them (`Child::sharing`).
///
/// - `weight`: how readily it gives width up in a shortage, and in a column
///   height (`WidthCapacity::weight`); zero protects its preferred size.
/// - `grow`: its share of a surplus of width, after shrinking has finished.
/// - `grow_height`: its share of a surplus of height in a column; forms
///   and a frame pass theirs on (`Child::grow_height_in_effect`).
/// - `reserve_width`: it takes the whole width its parent allocates, not
///   only the width its content needs (`Child::fill_width`).
/// - `fixed`: which of its dimensions are fixed (`Child::fixed_dimensions`).
///
/// A copy: the builders are the only way to change what a part declares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sharing {
    pub weight: f32,
    pub grow: f32,
    pub grow_height: f32,
    pub reserve_width: bool,
    pub fixed: Fixed,
}

/// A queryable leaf or composite. Only the root selects a level. Composites
/// forward that exact level; outward weights never replace internal weights.
///
/// A child is its `kind` (what it is) together with what any kind may carry:
/// the adjustments to its measurement and the other declarations a caller
/// adds with the builder methods below (weights and growth, hiding), and the
/// host's note. Every child is built by `Child::new` from its kind alone,
/// with nothing else declared yet.
///
/// How a child is measured (`query`): a hidden child measures zero.
/// Otherwise its kind gives the unadjusted measurement (`base_measurement`),
/// and the adjustments the builders declared are applied to it one after
/// another in the order they were declared (`measure_adjusted`). The list
/// can be read to see what a child is; the order is the order of the calls,
/// and it matters, because each adjustment acts on what the ones before it
/// left (see `Adjustment` for an example).
pub struct Child<'a, H: Host> {
    pub(crate) kind: Kind<'a, H>,
    pub(crate) adjustments: Vec<Adjustment<'a, H>>,
    pub(crate) hidden: Rc<dyn Fn(Level) -> bool + 'a>,
    pub(crate) sharing: Sharing,
    /// A builder was given a number it cannot use (`invalid_unless`).
    pub(crate) invalid: bool,
    pub(crate) note: H::Note,
}

/// Whether a number a caller gives as a length, a weight or a growth factor
/// can be used: finite and not negative.
pub fn is_amount(value: f32) -> bool {
    value.is_finite() && value >= 0.0
}

impl<H: Host> Clone for Child<'_, H>
where
    H::Leaf: Clone,
    H::Note: Clone,
{
    fn clone(&self) -> Self {
        Self {
            kind: self.kind.clone(),
            adjustments: self.adjustments.clone(),
            hidden: self.hidden.clone(),
            sharing: self.sharing,
            invalid: self.invalid,
            note: self.note.clone(),
        }
    }
}

impl<'a, H: Host> Child<'a, H> {
    /// What this child is.
    pub fn kind(&self) -> &Kind<'a, H> {
        &self.kind
    }

    /// What the host keeps on this child.
    pub fn note(&self) -> &H::Note {
        &self.note
    }

    /// What the host keeps on this child, to change it. Nothing the engine
    /// plans with is in it.
    pub fn note_mut(&mut self) -> &mut H::Note {
        &mut self.note
    }

    /// How this child takes part in sharing room, as declared.
    pub fn sharing(&self) -> Sharing {
        self.sharing
    }

    /// Mark this child invalid unless `valid`: the one way a child is marked
    /// invalid.
    ///
    /// An invalid child is one whose builder was given something it cannot
    /// use: a weight, growth factor, spacing, padding, spacer width, viewport
    /// floor or fixed dimension that is NaN, infinite or negative
    /// (`is_amount`), or forms whose thresholds do not start at `Preferred`
    /// or do not strictly increase. It is still built, it keeps the number it
    /// was given, and later builders keep the mark (it is never cleared, and
    /// the wrappers `fill_width` and `fixed_dimensions` take it on). A host
    /// marks a child itself for a number only it can check.
    ///
    /// An invalid child has no valid size. Measuring it (`query`) gives NaN
    /// in every dimension: that is the one place the mark becomes an invalid
    /// measurement, and every measurement that includes it is checked as any
    /// measurement is (`validate_measurement`). Planning it (`plan_width`)
    /// fails at once, so a plan cannot pass through it on a path that does
    /// not measure it first. Every query and plan that includes it therefore
    /// fails with `AllocationError::InvalidInput`, as for a bad leaf
    /// measurement: that stage does not fit, the next declared stage is
    /// tried, and with none left the caller's fallback shows. Where it is
    /// hidden at a level it takes no part there, as any hidden child, and the
    /// rest lays out.
    ///
    /// Not a panic, because these numbers come from drags and measurements
    /// while an app runs, and a user application must never go down for one.
    /// Not a clamp or a default, because a number the engine made usable
    /// would lay out something the caller never declared, and the mistake
    /// would never show. The bad number is kept rather than replaced, so a
    /// path that reached it would compute with it and fail at the engine's
    /// own checks, not succeed quietly.
    pub fn invalid_unless(mut self, valid: bool) -> Self {
        self.invalid |= !valid;
        self
    }

    /// Whether this child, at `level`, has a composition to plan: it is one,
    /// or the frame's content or the form selected is.
    pub(crate) fn has_composition(&self, level: Level) -> bool {
        match &self.kind {
            Kind::Leaf(_) => false,
            Kind::Composition(_) => true,
            Kind::Frame(frame) => frame.content.has_composition(level),
            Kind::Forms(forms) => forms.select(level).has_composition(level),
        }
    }

    /// Whether this child takes no part at `level`: it is declared hidden
    /// there, or everything it holds is.
    pub fn is_hidden(&self, level: Level) -> bool {
        (self.hidden)(level)
            || match &self.kind {
                Kind::Leaf(_) => false,
                Kind::Composition(composition) => composition
                    .children
                    .iter()
                    .all(|child| child.is_hidden(level)),
                Kind::Frame(frame) => frame.content.is_hidden(level),
                Kind::Forms(forms) => forms.select(level).is_hidden(level),
            }
    }

    /// Finite and nonnegative; any other weight makes the child invalid
    /// (`invalid_unless`), as for `grow` and `grow_height`.
    pub fn weight(mut self, weight: f32) -> Self {
        self.sharing.weight = weight;
        self.invalid_unless(is_amount(weight))
    }

    /// Share surplus only after shrinking has finished.
    pub fn grow(mut self, grow: f32) -> Self {
        self.sharing.grow = grow;
        self.invalid_unless(is_amount(grow))
    }

    /// Share surplus height in a column independently of width growth. A
    /// leaf keeps its measured height floor; a leaf that takes a range of
    /// heights expands.
    pub fn grow_height(mut self, grow: f32) -> Self {
        self.sharing.grow_height = grow;
        self.invalid_unless(is_amount(grow))
    }

    /// Explicit component permission to omit this form. A hidden child takes
    /// no part: no slot, no gap.
    pub fn hidden_when(mut self, hidden: impl Fn(Level) -> bool + 'a) -> Self {
        self.hidden = Rc::new(hidden);
        self
    }

    /// Round the preferred and the minimum width up to whole pixels.
    pub fn round_width_up(mut self) -> Self {
        self.adjustments.push(Adjustment::RoundWidthUp);
        self
    }

    /// Set the desired width independently of the measured useful minimum.
    /// A parent may allocate less down to that minimum; when the minimum is
    /// larger than the requested width it remains authoritative. Geometry is
    /// remeasured at the allocated width.
    pub fn preferred_width(self, width: f32) -> Self {
        self.preferred_width_with(move |_, _, _| width)
    }

    /// Derive a preferred width from the same prepared data and context as
    /// the component, without replacing its independently measured minimum.
    pub fn preferred_width_with(
        mut self,
        preferred: impl Fn(Level, &H::Context, Measured) -> f32 + 'a,
    ) -> Self {
        let preferred: PreferredWidth<'a, H> = Rc::new(preferred);
        self.adjustments.push(Adjustment::PreferredWidth(preferred));
        self
    }

    /// Declare a finite, nonnegative useful width floor. If the measured
    /// width is smaller (a leaf may measure zero), the preferred width is
    /// raised to the floor. A leaf with positive infinite width is measured
    /// at this declared floor; only a finite fitting result can use the
    /// floor as its initial preferred width. A leaf is measured again at its
    /// allocated width.
    pub fn minimum_width(self, minimum: impl Fn(Level, Size) -> f32 + 'a) -> Self {
        self.minimum_width_with(move |level, _, preferred| minimum(level, preferred))
    }

    /// A useful width floor measured with the query's context. The callback
    /// is pure and must capture the same data as this component's leaf.
    pub fn minimum_width_with(
        mut self,
        minimum: impl Fn(Level, &H::Context, Size) -> f32 + 'a,
    ) -> Self {
        let minimum: MinimumWidth<'a, H> = Rc::new(minimum);
        self.adjustments.push(Adjustment::MinimumWidth(minimum));
        self
    }

    /// Measure this child with `query` instead: what its kind measures and
    /// every adjustment declared so far no longer count, as the list shows by
    /// starting again from the replacement. Adjustments declared after this
    /// apply to the replacement.
    pub fn with_measurement(mut self, query: impl Fn(Level, &H::Context) -> Measured + 'a) -> Self {
        let query: SizeQuery<'a, H> = Rc::new(query);
        self.adjustments.clear();
        self.adjustments.push(Adjustment::Replaced(query));
        self
    }
}

/// The constructors, and the wrappers that make a new part: each new part
/// starts with the host's default note.
impl<'a, H: Host> Child<'a, H>
where
    H::Note: Default,
{
    /// The one place a child is built. The public constructors choose the
    /// kind, which also says how the child is measured and, for a leaf, how
    /// it takes height; everything a caller may declare afterwards starts
    /// here at its neutral value: no measuring adjustments, visible,
    /// weight 1 and no growth, no reserved or fixed dimensions, valid, and
    /// the host's default note.
    fn new(kind: Kind<'a, H>) -> Self {
        Self {
            kind,
            adjustments: Vec::new(),
            hidden: Rc::new(|_| false),
            sharing: Sharing {
                weight: 1.0,
                grow: 0.0,
                grow_height: 0.0,
                reserve_width: false,
                fixed: Fixed::default(),
            },
            invalid: false,
            note: H::Note::default(),
        }
    }

    /// A leaf whose height is a floor (`LeafHeight::Floor`).
    pub fn leaf(content: H::Leaf) -> Self {
        Self::new(Kind::Leaf(Leaf {
            content,
            height: LeafHeight::Floor,
        }))
    }

    /// A leaf that takes a range of heights, from the floor `minimum`
    /// measures up (`LeafHeight::Range`). If the leaf has no finite
    /// preferred dimension, its floor supplies it, once the leaf measured
    /// at its floor fits there.
    ///
    /// The floor first applies to the leaf's measurement
    /// (`Adjustment::ViewportFloor`), before anything declared on the child
    /// after.
    pub fn ranged_leaf(
        content: H::Leaf,
        minimum: impl Fn(Level, &H::Context) -> Size + 'a,
    ) -> Self {
        let minimum: MinimumQuery<'a, H> = Rc::new(minimum);
        let mut child = Self::new(Kind::Leaf(Leaf {
            content,
            height: LeafHeight::Range { minimum },
        }));
        child.adjustments.push(Adjustment::ViewportFloor);
        child
    }

    /// A component family's own structured forms, inherited until the next
    /// threshold: `first` with its threshold, then the `later` ones. The
    /// first threshold must be Preferred and thresholds strictly increase.
    /// Every selected form receives the requested level, so an inherited
    /// composite still forwards Mild/Strong rules to its own immediate
    /// children. The returned child's outward weights are independent of the
    /// form's internals. Thresholds that break either rule make the child
    /// invalid (`invalid_unless`).
    pub fn forms(first: (Level, Self), later: Vec<(Level, Self)>) -> Self {
        let (first_threshold, first) = first;
        let increasing = later
            .iter()
            .try_fold(first_threshold, |previous, (threshold, _)| {
                (previous < *threshold).then_some(*threshold)
            })
            .is_some();
        Self::new(Kind::Forms(Rc::new(Forms { first, later })))
            .invalid_unless(first_threshold == Level::Preferred && increasing)
    }

    /// Children side by side on one line, each centred on the row's height.
    pub fn row(children: Vec<Self>, spacing: f32) -> Self {
        Self::compose(children, spacing, Arrangement::Row)
    }

    /// A row with each immediate child aligned to its top edge.
    pub fn row_top(children: Vec<Self>, spacing: f32) -> Self {
        Self::compose_aligned(children, spacing, Arrangement::Row, true)
    }

    /// Children stacked from the top.
    pub fn column(children: Vec<Self>, spacing: f32) -> Self {
        Self::compose(children, spacing, Arrangement::Column)
    }

    /// Children in rows, a new row below when the next does not fit.
    pub fn wrap(children: Vec<Self>, spacing: f32) -> Self {
        Self::compose(children, spacing, Arrangement::Wrap)
    }

    fn compose(children: Vec<Self>, spacing: f32, arrangement: Arrangement) -> Self {
        Self::compose_aligned(children, spacing, arrangement, false)
    }

    fn compose_aligned(
        children: Vec<Self>,
        spacing: f32,
        arrangement: Arrangement,
        top_aligned: bool,
    ) -> Self {
        let composition = Rc::new(Composition {
            children,
            arrangement,
            spacing,
            top_aligned,
        });
        Self::new(Kind::Composition(composition)).invalid_unless(is_amount(spacing))
    }

    /// A fixed padding round `content`, drawn by the host's `chrome`. All
    /// geometry, forms, minima and requested levels of the content remain
    /// part of the same engine. Hidden content omits the entire frame and
    /// its spacing. Padding must be finite and nonnegative; any other makes
    /// the frame invalid (`invalid_unless`).
    pub fn frame(content: Self, padding: Padding, chrome: H::Chrome) -> Self {
        let valid = [padding.left, padding.right, padding.top, padding.bottom]
            .into_iter()
            .all(is_amount);
        Self::new(Kind::Frame(Rc::new(Frame {
            content,
            padding,
            chrome,
        })))
        .invalid_unless(valid)
    }

    /// Reserve the full width allocated by the parent, keeping the inner
    /// component's natural geometry intact. Useful for wrapping navigation
    /// followed by trailing controls: a short final row must not move those
    /// controls into space already allocated to navigation.
    pub fn fill_width(self) -> Self {
        let weight = self.sharing.weight;
        let grow_height = self.sharing.grow_height;
        let invalid = self.invalid;
        let mut result = Self::column(vec![self], 0.0);
        result.sharing.weight = weight;
        result.sharing.grow_height = grow_height;
        result.sharing.reserve_width = true;
        result.sharing.grow = 1.0;
        result.invalid_unless(!invalid)
    }

    /// Reserve exact dimensions for this prepared component. A dimension below
    /// its measured useful minimum is raised to that minimum; insufficient
    /// available space fails the stage rather than shrinking the request.
    /// Apply this to the expanded form, before declaring a collapsed form.
    /// The inner child remains an ordinary nested composition. Values must
    /// be finite and nonnegative; any other makes the child invalid
    /// (`invalid_unless`).
    pub fn fixed_dimensions(self, width: Option<f32>, height: Option<f32>) -> Self {
        if width.is_none() && height.is_none() {
            return self;
        }
        // A wrapper keeps the inner child's measured geometry intact.
        let weight = self.sharing.weight;
        let grow = self.sharing.grow;
        let grow_height = self.sharing.grow_height;
        let invalid = self.invalid;
        let mut result = Self::column(vec![self], 0.0);
        result.sharing.weight = weight;
        result.sharing.grow = grow;
        result.sharing.grow_height = grow_height;
        // The numbers go into the measurement; the planner needs only which
        // dimensions they fix (`Fixed`).
        result.sharing.fixed = Fixed {
            width: width.is_some(),
            height: height.is_some(),
        };
        result
            .adjustments
            .push(Adjustment::FixedDimensions { width, height });
        let valid = !invalid && [width, height].into_iter().flatten().all(is_amount);
        result.invalid_unless(valid)
    }
}
