// SPDX-License-Identifier: GPL-3.0-only

//! The household apps' layout engine, without a toolkit.
//!
//! # What the engine is for
//!
//! The engine decides where the boxes of a window or a chart go when the
//! room they are given changes. It answers one question for a tree of
//! boxes and an available size: which of the arrangements the caller has
//! declared fits, and where every box goes in it. A window and a chart
//! that use it adapt in the same way, by the same rules.
//!
//! What it deliberately does not do matters as much. It draws nothing and
//! measures nothing: the caller measures its own content (see [`Leaf`]).
//! It never invents hiding, clipping or scrolling: a box is hidden only
//! where the box itself declares it may be ([`Child::hidden_when`]), and
//! a box gives up height only where it declares a viewport. It does not
//! decide child by child which box should compact: every box is asked at
//! the same level, so boxes compact together. And it has no scores: it
//! does not weigh one layout against another, it takes the first declared
//! stage that fits.
//!
//! # The model
//!
//! A layout is a tree of [`Child`]ren. A leaf is the caller's content; a
//! composite arranges its immediate children in a row, a column or a wrap
//! (rows that break onto new lines), with a fixed spacing between them.
//!
//! A [`Level`] says how much compromise is permitted: `Preferred`, `Mild`,
//! `Strong` or `AllPermitted`, in increasing order. Each child changes
//! with the level in two kinds of way.
//!
//! - **Continuous change, within one level.** At one exact level a child
//!   has a preferred size and a useful minimum ([`MeasuredSize`]). Either
//!   may depend on the level. Between those two widths the child can take
//!   any width a parent gives it; a shortage of room is shared out between
//!   siblings by weight (see *Sharing a shortage* below), and a surplus is
//!   shared by `grow`.
//! - **Discrete change, between levels.** A child can declare a different
//!   form from a given level ([`forms`]): for example a labelled button at
//!   `Preferred` and an icon from `Mild`. A form carries on to every
//!   higher level until the next level that declares another.
//!
//! # One level for everyone
//!
//! Only the root selects the level. Every composite passes that exact
//! level on to all of its children, which pass it on to theirs; no child
//! picks a level for itself and no parent schedules its children one at a
//! time. So when a toolbar is asked at `Mild`, every button in it takes its
//! `Mild` form at once. Which things compact together is decided by how
//! the tree is organised and by the forms each child declares, not by a
//! search over who should give way.
//!
//! # Stages
//!
//! A component states its own choices as a short ordered list of
//! [`Stage`]s, each one level and one tree, and [`choose_stage`] tries
//! them in order and takes the first that fits. The list is the simple
//! way to say what this component cares about most when room runs short,
//! and that can differ from one component to another: one toolbar may
//! rather wrap onto a second line than lose its button labels, while
//! another may rather keep to one line. Each parent writes its own list
//! for its own priorities.
//!
//! For example: a row at `Preferred`; a row at `Mild`; a wrap at `Mild`;
//! a wrap at `Strong`. That list compacts before it rearranges. Putting
//! the wrap at `Preferred` before the row at `Mild` would rearrange first
//! and keep the full buttons longer. Both are only examples of orders a
//! list can express: the engine itself has no built-in preference between
//! rearranging and compacting, and follows whichever order it is given.
//!
//! A stage fits when its whole tree can be laid out within the available
//! width and height: every child at least its useful minimum wide, every
//! leaf no wider than the width it was given, and the total height no more
//! than the height available, each to within [`FIT_EPSILON`]. Each stage
//! starts again from the preferred sizes at its own level; nothing carried
//! over from a stage that failed. So a compact form that frees room lets
//! its siblings grow back towards their preferred widths. If no stage
//! fits, the answer is [`NoFit`]; nothing is hidden, clipped or compacted
//! further than the stages declare.
//!
//! # A part with its own stages
//!
//! A part of a layout can be a root of its own: it holds its own list of
//! stages ([`stages`]) and takes the first that fits the room its parent
//! gives it, as [`choose_stage`] does for the whole layout, whatever the
//! rest of the layout chooses. This is the engine's `ResponsiveLevels`
//! placed in another layout as an ordinary leaf; here a
//! [`ResponsiveLevels`] becomes a [`Child`] with `Child::from` or `into()`.
//!
//! Use it for a choice that depends only on that part's own room. A
//! settings row puts its label, preview and control side by side, then the
//! preview below, then everything stacked, by the width the row itself is
//! given. A chart's category names go onto two lines when the axis they
//! label is short of width, while the gap under the figure's title answers
//! to the figure's height. Written as stages of the whole page or figure,
//! each of these choices would be tied to every other choice in that list:
//! the names would wrap only together with whatever else that stage
//! compacts.
//!
//! The outer layout sees an ordinary leaf, which it measures and lays out
//! as it does any other:
//!
//! - **Measured**, the part lays itself out in unbounded room. The first of
//!   its stages that fits there gives its preferred size, and its minimum
//!   is the same, as for any leaf. So unless the caller declares a
//!   [`Child::minimum_width`], the outer layout never gives it less width
//!   than its first stage prefers, and in that width the first stage
//!   fits: its later stages are never reached. A part that should
//!   rearrange when short of width declares the least width it can work
//!   in.
//! - **Laid out** at the width its parent allocates, it chooses its stage
//!   there. Like any ordinary leaf it chooses with unbounded height, so its
//!   choice follows its width alone, and in a column its height is a
//!   floor. Its box fills the width it is given and is as high as the
//!   chosen stage.
//! - **When none of its stages fits**, it shows the caller's fallback
//!   ([`ResponsiveLevels::fallback`]) or an empty box, as wide as it was
//!   given and as high as the fallback (nothing, for none). To the outer
//!   layout that is still a leaf that fits: the failure does not make the
//!   outer stage fail.
//!
//! The outer level does not reach inside. An outer stage asks the part at
//! its own level, as it asks every child, but the part does not use it:
//! each of its stages names its own level, and the same width gives the
//! same inner choice under an outer stage at `Preferred` or at `Mild`.
//!
//! The outer plan carries the inner choice. The node of the part has a
//! [`GeometryPlan::selection`]: the stage it took, at its level, with that
//! stage's plan (relative to the node's top left corner), or, when none
//! fitted, the [`NoFit`] that says what each stage lacked.
//!
//! # How one layout is worked out
//!
//! Width comes first. Content such as text that wraps is taller when it is
//! narrower, so a child's height is only known once its width is: the
//! engine settles every width, then lays each leaf out at its width with
//! unbounded height, so that a clipped height can never pass for a fit.
//!
//! - **A row** shares its width, less the spacing, between its children
//!   (see *Sharing a shortage*), then lays each one out at its width. Its
//!   height is that of its tallest child, and each shorter child is centred
//!   vertically in it, unless the row is top-aligned ([`row_child_top`]).
//! - **A column** gives each child its own share of the column's width
//!   (each child alone, as if it were the only one), and lays it out there,
//!   which gives the child's height. The heights are then fitted to the
//!   column's height. An ordinary child's height is a floor: content laid
//!   out at its width needs that height, and taking any away would clip
//!   it. Only a child that declares vertical capacity can give height:
//!   a viewport ([`scroll_child`], [`scroll_child_with_minimum`]), which
//!   says the least height at which it still works, or a composite that
//!   holds one. A chart's gap that
//!   may shrink declares itself in the same way. The heights are shared
//!   between such children by the same rule as widths, using each child's
//!   `weight`, and surplus height goes to children that `grow_height`.
//! - **A wrap** packs its children into rows greedily by their useful
//!   minimum widths: a child goes on the current row if its minimum, with
//!   the spacing, still fits, and starts a new row otherwise. Each row then
//!   shares the full width afresh, as a row does. The rows stack downwards.
//!   If they are too tall, the wrap shares its height between the rows in
//!   the way a column does, so again only viewports can give height.
//!
//! Some children have a part to play beyond their content:
//!
//! - A **hidden** child ([`Child::hidden_when`]) takes no room and no
//!   spacing, and has no placement; a composite all of whose children are
//!   hidden is hidden itself. The placements of the others keep their
//!   index among the declared children.
//! - A **frame** ([`framed_child`]) adds chrome around a child. The chrome
//!   is counted inside the fit: the frame's minimum and preferred sizes
//!   are the child's plus the padding (its preferred width stands for its
//!   minimum if its weight is zero), and the child is laid out in what
//!   is left when the padding is taken away.
//! - A **spacer** ([`spacer`]) is an empty leaf with a fixed minimum width
//!   and a weight of zero that takes surplus width (`grow` of 1): it pushes
//!   what follows it to the far end of a row.
//! - **Growth**: after any shortage has been shared, a surplus is given to
//!   the children with a positive `grow`, in proportion to it. Without any,
//!   the surplus is left empty at the end of the row.
//!
//! # Sharing a shortage
//!
//! When children's preferred widths add up to more than the room,
//! [`allocate_width`] takes the shortage from them at a rate of
//! `weight × (preferred − minimum)`. A child that reaches its minimum is
//! held there, and what remains of the shortage is shared again between
//! the others at their original rates. A weight of zero protects the
//! preferred width: such a child never shrinks, so wherever the engine
//! adds up minima it counts that child's preferred width as its minimum.
//! If the minima (and the protected preferred widths) add up to more than
//! the room, nothing fits.
//!
//! For example, three children of preferred widths 100, 100 and 40, minima
//! 80, 0 and 0, and weights 3, 1 and 0, in 160. The shortage is 80. The
//! rates are 3 × 20 = 60 and 1 × 100 = 100; the third child is protected.
//! The first share takes 30 from the first child, which can give only 20
//! and stops at 80, and 50 from the second, which goes to 50. Ten remain;
//! only the second can give, so it takes all ten and ends at 40. The widths
//! are 80, 40 and 40. In 119 nothing fits: the minima need 80 + 0 + 40 =
//! 120. (Checked by the test
//! `capacity_sharing_uses_original_rates_clamps_and_protects_zero_weight`.)
//!
//! # A worked example: a toolbar
//!
//! A toolbar row with a spacing of 8 holds a title, which prefers 160 and
//! is useful down to 60; three buttons, 90 wide with their labels and 32
//! as icons, with the icon form from `Mild`; and a primary action 100 wide
//! with a weight of zero, so that it keeps its label. Every box is 32
//! high. The stages are a row at `Preferred`, a row at `Mild` and a wrap
//! at `Mild`, and the height available is 80.
//!
//! At `Preferred` the row prefers 160 + 3 × 90 + 100 + 4 × 8 = 562 and
//! needs at least 60 + 270 + 100 + 32 = 462. At `Mild` it prefers 160 +
//! 3 × 32 + 100 + 32 = 388 and needs 288.
//!
//! - In 600, the `Preferred` row fits as it is, 562 wide; the rest is
//!   left empty.
//! - In 500, the `Preferred` row is 62 short. Only the title can give, so
//!   it takes the whole shortage and is 98 wide; the buttons keep their
//!   labels.
//! - In 400, the `Preferred` row cannot fit (it needs 462), so the `Mild`
//!   row is tried from its own preferred sizes: all three buttons become
//!   icons together, and the title is back at 160.
//! - In 300, the `Mild` row is 88 short; the title gives it and is 72.
//! - In 250, neither row fits (the `Mild` one needs 288), and the wrap is
//!   tried. Packed by minima, the title and the three icons take 60 + 3 ×
//!   (8 + 32) = 180, and the primary action would make 288, so it starts a
//!   second row. On the first row the title shares out the 226 left after
//!   the spacing and is 130 wide. The wrap is 32 + 8 + 32 = 72 high, and
//!   the primary action is placed at (0, 40).
//! - In 250 by 60, no stage fits: the wrap's two rows need 64 of the 52
//!   left once the spacing between them is taken off.
//! - In 90, no stage fits: even the wrap needs the primary action's 100.
//!
//! (Checked by the test `worked_example_the_toolbar`.)
//!
//! # A worked example: a chart's vertical gap
//!
//! A chart's column holds a title 20 high, a gap, and a plot 100 high,
//! with no spacing. The gap is 16 at its recommended size and 8 at its
//! compact size. It declares a viewport whose least height is 8 at
//! `Preferred` and `Mild`, so that at those levels it runs from 16 down to
//! 8. At `Strong` it starts at its compact size, 8, and may be squeezed
//! to 4. The stages are the column at `Preferred` and at `Strong`.
//!
//! - In a height of 140, the `Preferred` column fits as it is: 20 + 16 +
//!   100 = 136.
//! - In 130 it is 6 short. The title and the plot are floors, so the gap
//!   gives all 6 and is 10.
//! - In 128 the gap is at its compact size, 8.
//! - In 125 the `Preferred` column needs 128 and does not fit. At
//!   `Strong` the gap starts at 8, the column is 3 short, and the gap is 5.
//! - In 120 nothing fits: `Strong` needs 20 + 4 + 100 = 124.
//!
//! Declared as an ordinary leaf instead, the gap would be a floor like the
//! title, and the `Preferred` column would not fit below 136. (Checked by
//! the test `worked_example_a_charts_vertical_gap`.)
//!
//! # A worked example: a part with its own stages
//!
//! A setting has a label 120 wide and a control 80 wide, both 20 high. Its
//! own stages, both at `Preferred`, are the two side by side with a spacing
//! of 8, which needs 208, and the two stacked with a spacing of 4, which is
//! 120 wide and 44 high. It declares a minimum width of 120. It sits in an
//! outer column with no spacing, below a title 100 wide and 20 high and a
//! gap that is 16 high and may shrink to 8 (a viewport, as in the chart
//! above). The outer column is the only outer stage.
//!
//! Measured, the setting is 208 by 20 (its first stage in unbounded room),
//! with a minimum width of 120 as declared. The outer column prefers 208
//! by 20 + 16 + 20 = 56.
//!
//! - In 240 by 60, the setting is given 208, its preferred width. Its
//!   first stage fits, and it is 20 high; the column is 56 high, with the
//!   gap at 16.
//! - In 240 by 50, the column is 6 short of height. The setting's height
//!   is a floor and its choice does not change: it is still side by side.
//!   The gap gives the 6 and is 10.
//! - In 160 by 80, the setting is given 160. Side by side it needs 120 +
//!   8 + 80 = 208, since neither part can be narrower, so it stacks: its
//!   box is 160 wide and 44 high. The column needs 20 + 16 + 44 = 80 and
//!   fits as it is. What the side-by-side stage lacked is in the setting's
//!   own record, not the outer one.
//! - In 160 by 76, both are short: the setting stacks because of the width
//!   and the gap gives 4 and is 12 because of the height.
//! - In 160 by 71 the outer column does not fit: its floors and the gap's
//!   least height need 20 + 8 + 44 = 72.
//! - In 110 the outer column does not fit either: the setting's declared
//!   minimum is 120. Without that declaration it would not fit below 208,
//!   and the setting would never stack.
//!
//! (Checked by the test `worked_example_a_part_with_its_own_stages`.)
//!
//! # What a caller supplies and gets back
//!
//! The caller supplies the leaves. A [`Leaf`] answers two questions, both
//! pure: its preferred size and useful minimum at a level, and its size
//! when laid out within a given width and height limit at a level. Around
//! the leaves the caller builds the tree with this module's builders and
//! declares its stages.
//!
//! It gets back a [`StagePlan`]: which stage was taken, at which level,
//! and a [`GeometryPlan`], whose size, and whose placements of each child
//! (its index, position relative to its parent and own plan, recursively)
//! say where everything goes. Or it gets [`NoFit`], with what failed at
//! each stage. A single child can also be asked directly with
//! [`Child::query`] and [`Child::query_at`].
//!
//! # Where it comes from
//!
//! The engine is household-widgets' `layout.rs` (with `layout_frame.rs`),
//! the layout of the household's apps, which follows the levels contract
//! of 8 November 2026. This module is a translation of it, item by item,
//! with the engine's names; where the engine measures a native widget,
//! this module asks a [`Leaf`]. What is about live widgets is left out:
//! views and their elements, the verified commit, speculative measuring,
//! native state, decorators, popups, hovers, navigation regions and
//! presentations (whose resolution is ported on its own, in
//! [`crate::presentation`]). Of `ResponsiveLevels` only what it does as a
//! leaf of another layout is kept, with its default width and height (fill
//! the width, shrink to the height); its other widths and heights, and the
//! conveniences `responsive` and `row`, are left out. household-widgets'
//! tests `layout_port_equality` and `layout_port_nested_equality` build
//! trees in both and require the same results over sweeps of width and
//! height.
//!
//! # Three additions
//!
//! None of these changes a choice the engine makes.
//!
//! - **Identities.** A child may carry an [`Id`], and the [`Source`] in
//!   the input it draws ([`Child::identity`]); its node in the plan carries
//!   them, and [`GeometryPlan::find`] finds a node by identity. A chart
//!   needs to know which box is its title and which its legend; a window
//!   knows its widgets by other means.
//! - **What failed.** When no stage fits, [`NoFit`] lists, for each stage,
//!   the engine's own error ([`AllocationError`]), which says how much
//!   room was required and how much was available, and the identity of the
//!   child it came from where one is known. The engine discards this; a
//!   chart needs it to say which size would help. In the same way the node
//!   of a part with its own stages carries the choice it made
//!   ([`GeometryPlan::selection`]), which the engine keeps on its widget.
//! - **What a form does not show.** A child may report which information
//!   its planned form shortens or omits ([`Child::report_information`]);
//!   the reports are gathered in the plan ([`GeometryPlan::reports`]) as
//!   the engine gathers its source reports, so that what is left out can be
//!   shown some other way.

use std::rc::Rc;

use crate::geometry::{FIT_EPSILON, Point, Size};
use crate::identity::{Id, Source};
/// The four levels of permitted compromise, in increasing order: the
/// engine's `Level`.
pub use crate::level::Level;

/// The first level, up to `max_level`, whose measured size fits
/// `available`; `None` when none does. The engine's `first_fitting`: a
/// helper for a single box with a size per level, which [`choose_stage`]
/// does not use. As in the engine, the fit is exact, with no tolerance, and
/// a size that is not finite or is negative never fits.
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

/// Why an allocation or a plan failed: the engine's `AllocationError`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AllocationError {
    /// A length that is not a finite, nonnegative number, or a minimum
    /// above its preferred.
    InvalidInput,
    /// The minima and the protected preferred lengths together need
    /// `required`, more than the `available` room.
    NoFit { required: f64, available: f64 },
}

/// One child's claim on a shared length, for [`allocate_width`]: the
/// engine's `WidthCapacity`. A column uses it for heights too.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WidthCapacity {
    /// The length the child takes when there is room.
    pub preferred: f64,
    /// The least length at which the child is still useful.
    pub minimum: f64,
    /// Higher weights shrink faster; zero protects the preferred width.
    pub weight: f64,
}

/// Share a shortage of `available` between children: the engine's
/// `allocate_width`. Each child gives at its original rate,
/// `weight * (preferred - minimum)`; a child that reaches its minimum is held
/// there and the rest of the shortage is shared again at the others'
/// original rates. A weight of zero protects the preferred length. When
/// there is room, every child gets its preferred length and the surplus is
/// left unallocated (growth is the caller's next step). `NoFit` when the
/// minima and protected preferred lengths exceed `available` by more than
/// [`FIT_EPSILON`]. The module documentation works an example.
pub fn allocate_width(
    available: f64,
    capacities: &[WidthCapacity],
) -> Result<Vec<f64>, AllocationError> {
    if !available.is_finite()
        || available < 0.0
        || capacities.iter().any(|capacity| {
            !capacity.preferred.is_finite()
                || !capacity.minimum.is_finite()
                || !capacity.weight.is_finite()
                || capacity.minimum < 0.0
                || capacity.preferred < capacity.minimum
                || capacity.weight < 0.0
        })
    {
        return Err(AllocationError::InvalidInput);
    }
    let required: f64 = capacities
        .iter()
        .map(|capacity| {
            if capacity.weight == 0.0 {
                capacity.preferred
            } else {
                capacity.minimum
            }
        })
        .sum();
    if required > available + FIT_EPSILON {
        return Err(AllocationError::NoFit {
            required,
            available,
        });
    }
    let mut lengths: Vec<f64> = capacities
        .iter()
        .map(|capacity| capacity.preferred)
        .collect();
    let rates: Vec<f64> = capacities
        .iter()
        .map(|capacity| capacity.weight * (capacity.preferred - capacity.minimum))
        .collect();
    let mut shortage = (lengths.iter().sum::<f64>() - available).max(0.0);
    while shortage > 0.0 {
        let total_rate: f64 = capacities
            .iter()
            .enumerate()
            .filter(|(i, capacity)| lengths[*i] > capacity.minimum)
            .map(|(i, _)| rates[i])
            .sum();
        if total_rate == 0.0 {
            break;
        }
        let mut removed = 0.0;
        let mut clamped = false;
        for (i, capacity) in capacities.iter().enumerate() {
            let room = lengths[i] - capacity.minimum;
            if room <= 0.0 || rates[i] == 0.0 {
                continue;
            }
            let share = shortage * rates[i] / total_rate;
            let reduction = share.min(room);
            lengths[i] -= reduction;
            removed += reduction;
            clamped |= share >= room;
        }
        if !clamped || removed == 0.0 {
            break;
        }
        shortage = (shortage - removed).max(0.0);
    }
    Ok(lengths)
}

/// A child's preferred size and useful minimum at one exact level: the
/// engine's `MeasuredSize`. A parent never gives a child less width than
/// its minimum, and gives it its preferred width when there is room.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasuredSize {
    /// The size the child takes when there is room.
    pub preferred: Size,
    /// The least size at which the child is still useful at this level.
    pub minimum: Size,
}

/// The caller's content: what the engine measures as a native widget. Both
/// answers must be pure, since they are asked for every stage tried, not
/// only the one taken, and may be asked several times.
pub trait Leaf {
    /// The preferred size and useful minimum at one exact level: the
    /// engine's intrinsic measurement of a native view. For content that
    /// does not change with its width, both are its size.
    fn query(&self, level: Level) -> MeasuredSize;

    /// The size when laid out within `maximum` at `level`: the engine's
    /// native measurement at an allocated width. An infinite height means
    /// unbounded (the leaf takes its own height); a finite one is a
    /// declared viewport's offered height. A size wider or taller than
    /// `maximum` does not fit.
    fn layout(&self, level: Level, maximum: Size) -> Size;
}

/// How a composite arranges its immediate children: the engine's
/// `Arrangement`. Each child is placed whole; a wrap never breaks a nested
/// composite across rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arrangement {
    /// Side by side, sharing the width ([`row_child`], [`row_child_top`]).
    Row,
    /// One below another, each with the width it asks for ([`column_child`]).
    Column,
    /// Side by side, starting a new row when the next child's minimum no
    /// longer fits ([`wrap_child`]).
    Wrap,
}

/// How a form loses a piece of information: the engine's `LossKind`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LossKind {
    /// Shown in part, such as a label cut short.
    Shortened,
    /// Not shown at all.
    Omitted,
}

/// One piece of information a form shortens or omits: the engine's
/// `InformationLoss`, with the information named by an [`Id`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InformationLoss {
    /// The information, as the reporting child names it.
    pub identity: Id,
    /// Whether it is shortened or omitted.
    pub kind: LossKind,
}

/// A child's report on its planned form ([`Child::report_information`]),
/// one of the port's additions; the engine's `SourceReport` without the
/// revision and presentation that certify its native commit. An empty
/// `losses` says plainly that the form shows everything.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceReport {
    /// The reporting child's identity, if it has one.
    pub id: Option<Id>,
    /// The input the reporting child draws, if it names one.
    pub source: Option<Source>,
    /// The level the form was planned at.
    pub level: Level,
    /// The size the form was planned at.
    pub size: Size,
    /// What the form shortens or omits.
    pub losses: Vec<InformationLoss>,
}

/// Where a child and everything inside it go: the engine's `GeometryPlan`.
/// A plan is computed afresh by each query and changes nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct GeometryPlan {
    /// The child's size. For a leaf, the width is the width it was given,
    /// even when the leaf itself is narrower ([`Self::native_size`]).
    pub size: Size,
    /// Whether the child is hidden at this level; a hidden plan is empty.
    pub hidden: bool,
    /// The placements of the visible immediate children, in declared
    /// order; a frame has one, its content.
    pub children: Vec<Placement>,
    // Row membership is part of the plan, not recomputed during commit.
    rows: Vec<Vec<usize>>,
    minimum_height: f64,
    native_size: Option<Size>,
    identities: Vec<(Id, Option<Source>)>,
    source_reports: Vec<SourceReport>,
    selection: Option<Rc<Result<StagePlan, NoFit>>>,
}

/// One immediate child's place in its parent's plan: the engine's
/// `Placement`.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    /// Index in the declared immediate children (hidden children are omitted).
    pub index: usize,
    /// The child's top left corner, relative to its parent's.
    pub position: Point,
    /// The child's own plan.
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
            selection: None,
        }
    }

    fn hidden() -> Self {
        Self {
            hidden: true,
            ..Self::leaf(Size::new(0.0, 0.0))
        }
    }

    /// The least height this plan could be given at its width: its size's
    /// height for ordinary content, less for a declared viewport, and for a
    /// composite the sum (column) or the tallest (row) of its children's,
    /// with the spacing. The engine's `minimum_height`; a column shares its
    /// height down to these. Unlike a wrap's measured minimum, this one is
    /// worked out at the width actually given.
    pub fn minimum_height(&self) -> f64 {
        self.minimum_height
    }

    /// The placements in each row of a row or a wrap, as indices into
    /// `children`; empty for a column or a leaf. Private in the engine,
    /// where only its commit reads it.
    pub fn rows(&self) -> &[Vec<usize>] {
        &self.rows
    }

    /// For a leaf, the size the leaf answered at its offered space; the
    /// plan's own width is the allocated width. Private in the engine,
    /// where its commit checks the native widget against it.
    pub fn native_size(&self) -> Option<Size> {
        self.native_size
    }

    /// For the node of a part with its own stages ([`ResponsiveLevels`]),
    /// the choice it made in the room this plan gave it: the stage taken,
    /// with its level and its plan, whose positions are relative to this
    /// node's top left corner; or, when none of its stages fitted, what
    /// each lacked ([`NoFit`]), while this node holds the fallback's size.
    /// `None` for every other node. The engine's
    /// `ResponsiveLevels::selection`, which the engine keeps on the widget
    /// rather than in the outer plan; part of the port's addition *what
    /// failed*, and it changes no choice. [`Self::find`] and
    /// [`Self::reports`] do not look inside it.
    pub fn selection(&self) -> Option<&Result<StagePlan, NoFit>> {
        self.selection.as_deref()
    }

    /// The identities of the children whose node this is: a form's own
    /// identity first, then that of the child that selected it. One of the
    /// port's additions.
    pub fn identities(&self) -> &[(Id, Option<Source>)] {
        &self.identities
    }

    /// The node carrying `id` ([`Child::identity`]), with its top left
    /// corner relative to this plan's; searched depth first, in placement
    /// order. One of the port's additions.
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

    /// The information-loss reports of this node and every node inside it,
    /// in that order ([`Child::report_information`]); the engine's
    /// `reports`.
    pub fn reports(&self) -> Vec<&SourceReport> {
        let mut reports: Vec<_> = self.source_reports.iter().collect();
        for child in &self.children {
            reports.extend(child.geometry.reports());
        }
        reports
    }
}

/// The chrome a frame adds on each side of its content ([`framed_child`]):
/// the engine's padding, which is iced's `Padding`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Padding {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Padding {
    /// Padding with each side given, in the order top, right, bottom, left.
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

/// The same padding on every side.
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

/// A node of the layout tree: a leaf, a composite, a family of forms or a
/// frame, with how it behaves towards its parent (weight, growth, hiding,
/// fixed dimensions). The engine's `Child`. Build one with [`child`],
/// [`row_child`], [`column_child`], [`wrap_child`], [`forms`],
/// [`framed_child`], [`scroll_child`] or [`spacer`], then adjust it with
/// the methods below, each of which returns the changed child.
///
/// Only the root selects a level; a composite asks every child at the
/// level it was asked at. A child's `weight` and `grow` concern its share
/// in its parent; they never change how a composite shares its own room
/// between its children. A child is cheap to clone: its parts are shared.
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
    // A part with its own stages: `native` lays it out, and this records
    // the choice it made there in the plan.
    nested: Option<Rc<ResponsiveLevels<'a>>>,
}

impl<'a> Child<'a> {
    fn new(native: Option<Rc<dyn Leaf + 'a>>, query: SizeQuery<'a>) -> Self {
        Self {
            nested: None,
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
    /// there is one; its node in every plan carries them
    /// ([`GeometryPlan::find`]), and failures inside it are attributed to it
    /// unless a child inside it has an identity of its own
    /// ([`StageFailure::child`]). One of the port's additions; it changes
    /// no choice. A wrapper made later by [`Self::fill_width`] or
    /// [`Self::fixed_dimensions`] does not inherit it.
    pub fn identity(mut self, id: Id, source: Option<Source>) -> Self {
        self.identity = Some((id, source));
        self
    }

    /// Report what this form does not show, from the level and the plan
    /// just made for it: the engine's `report_information`, with the losses
    /// as plain data. The callback runs for every plan made, including
    /// those of stages that are not taken, so it must be pure. Its report
    /// is kept in the plan ([`GeometryPlan::reports`]); a hidden child
    /// makes none.
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

    /// This child's preferred size and useful minimum at `level`, as its
    /// parent sees them: the engine's `query`. A hidden child measures zero.
    /// A composite adds its children up: a row sums widths and takes the
    /// tallest height, a column the reverse, each with its spacing.
    pub fn query(&self, level: Level) -> MeasuredSize {
        if self.is_hidden(level) {
            return MeasuredSize {
                preferred: Size::new(0.0, 0.0),
                minimum: Size::new(0.0, 0.0),
            };
        }
        (self.query)(level)
    }

    /// Plan this child as a root at `level` in `available`: the engine's
    /// `query_at`, and what [`choose_stage`] asks of each stage. The width
    /// is allocated first (a composite takes the whole available width, a
    /// leaf its share of it), then leaves are laid out at their widths with
    /// unbounded height, so a clipped height can never pass for a fit. An
    /// infinite available width gives every child its preferred width. A
    /// useful minimum narrower than the intrinsic width must be declared
    /// ([`Self::minimum_width`]).
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
        // Width is settled before height: content that wraps is taller when
        // narrower, so its height is only known once its width is. A
        // composite takes the whole width offered and shares it out itself;
        // a leaf takes its own share of it.
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
            // Ordinary content is laid out at its width with unbounded
            // height, so the height it needs is what it reports; a clipped
            // height could otherwise pass for a fit. Only a declared
            // viewport is given a finite height.
            let maximum = Size::new(width, viewport_height.unwrap_or(f64::INFINITY));
            // A part with its own stages chooses one in this room; its
            // choice is kept in the plan. The level is not passed on: its
            // own stages name theirs.
            let (size, selection) = match &self.nested {
                Some(nested) => {
                    let (size, selection) = nested.layout(maximum);
                    (size, Some(Rc::new(selection)))
                }
                None => (native.layout(level, maximum), None),
            };
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
            plan.selection = selection;
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
    /// component's natural geometry and reports intact: the engine's
    /// `fill_width`. The child is wrapped in a one-child column that keeps
    /// its weight and height growth, grows by 1 (its own `grow` is not
    /// kept) and is always as wide as it is given. Useful for wrapping
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

    /// Reserve exact dimensions for this component: the engine's
    /// `fixed_dimensions`. A dimension below its
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

    /// How readily this child gives width in a shortage, relative to its
    /// siblings (default 1); zero means never, and then its preferred width
    /// counts as its minimum. In a column the same weight shares a shortage
    /// of height between viewports. The engine's `weight`.
    pub fn weight(mut self, weight: f64) -> Self {
        assert!(weight.is_finite() && weight >= 0.0);
        self.weight = weight;
        self
    }

    /// This child's share of surplus width (default 0): once any shortage
    /// is shared, room left over is divided between siblings in proportion
    /// to `grow`. The engine's `grow`.
    pub fn grow(mut self, grow: f64) -> Self {
        assert!(grow.is_finite() && grow >= 0.0);
        self.grow = grow;
        self
    }

    /// This child's share of surplus height in a column (default 0),
    /// independent of width growth: the engine's `grow_height`. A growing
    /// child's plan is as tall as the height it is given; a declared
    /// viewport is laid out at that height, while a leaf's own height
    /// stays a floor.
    pub fn grow_height(mut self, grow: f64) -> Self {
        assert!(grow.is_finite() && grow >= 0.0);
        self.grow_height = grow;
        self
    }

    /// Permit this child to be left out at the levels where `hidden` is
    /// true: the engine's `hidden_when`. A hidden child has no placement and
    /// costs no width or spacing. This is the only way a child is hidden.
    pub fn hidden_when(mut self, hidden: impl Fn(Level) -> bool + 'a) -> Self {
        self.hidden = Rc::new(hidden);
        self
    }

    /// Round this child's preferred and minimum widths up to whole units,
    /// so that fractional content does not shift its neighbours by a
    /// fraction: the engine's `round_width_up`.
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

    /// Set the desired width independently of the measured useful minimum:
    /// the engine's `preferred_width`.
    /// A parent may allocate less down to that minimum; when the minimum is
    /// larger than the requested width it remains authoritative. Geometry is
    /// laid out again at the allocated width.
    pub fn preferred_width(self, width: f64) -> Self {
        self.preferred_width_with(move |_, _| width)
    }

    /// Set the preferred width from the level and the measurement, without
    /// changing the minimum: the engine's `preferred_width_with`. A
    /// preferred width below the minimum is raised to it; one that is not a
    /// finite, nonnegative number makes the child invalid input.
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

    /// Declare a finite, nonnegative useful width floor from the level and
    /// the measured preferred size: the engine's `minimum_width_with`. A
    /// floor that is not a finite, nonnegative number makes the child
    /// invalid input. If the intrinsic
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

/// A leaf holding the caller's content: the engine's `child`, with a
/// [`Leaf`] in place of a view. Its measurement is the leaf's own
/// [`Leaf::query`]; it is laid out with [`Leaf::layout`] at the width its
/// parent gives it, and fails if the leaf answers wider than that.
pub fn child<'a>(leaf: impl Leaf + 'a) -> Child<'a> {
    let leaf: Rc<dyn Leaf + 'a> = Rc::new(leaf);
    let measure = leaf.clone();
    Child::new(Some(leaf), Rc::new(move |level| measure.query(level)))
}

/// A child that is one of several forms, chosen by level: the engine's
/// `forms`. Each form is given from a level and is
/// inherited until the next
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

/// A leaf that declares vertical capacity, down to `minimum`: the engine's
/// `scroll_child`. In a window it is a scrolling viewport; in a chart it
/// can be any box that works at less than its preferred height, such as a
/// gap. It is the only kind of leaf a column or a wrap can take height
/// from, and it is laid out at the height it is given, which the leaf must
/// fit within. If the leaf has no finite intrinsic size, `minimum` stands
/// for it once the leaf has been laid out within it successfully.
pub fn scroll_child<'a>(leaf: impl Leaf + 'a, minimum: Size) -> Child<'a> {
    assert!(finite_size(minimum));
    scroll_child_with_minimum(leaf, move |_| minimum)
}

/// [`scroll_child`] with a minimum that depends on the level: the engine's
/// `scroll_child_with_minimum`. A minimum that is not finite or is
/// negative makes the child invalid input; it is never clipped.
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

/// An empty leaf at least `minimum` wide, with no height, that never
/// shrinks (weight 0) and takes surplus width (grow 1): the engine's
/// `spacer`. Between two groups in a row it pushes the second to the end.
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

/// Put `padding` around `content`, counting it inside the fit: the
/// engine's `framed_child`, without its decorator. The frame measures the
/// content plus the padding (the content's preferred width standing for
/// its minimum if its weight is zero) and lays the content out in what is
/// left when the padding is taken away, placing it at the padding's top
/// left corner. The content stays a full child of the engine: its forms,
/// minima and level are unchanged. If the content is hidden, so is the
/// whole frame.
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

/// Children side by side with `spacing` between them, each centred
/// vertically in the row: the engine's `row_child`. The row's width is
/// shared between the children by weight, surplus by growth.
pub fn row_child(children: Vec<Child<'_>>, spacing: f64) -> Child<'_> {
    compose(children, spacing, Arrangement::Row)
}

/// A [`row_child`] whose children are aligned to its top edge rather than
/// centred: the engine's `row_child_top`.
pub fn row_child_top(children: Vec<Child<'_>>, spacing: f64) -> Child<'_> {
    compose_aligned(children, spacing, Arrangement::Row, true)
}

/// Children one below another with `spacing` between them: the engine's
/// `column_child`. Each child gets the width it asks for, up to the
/// column's (all of it if the child grows); heights are floors except
/// where a child declares capacity.
pub fn column_child(children: Vec<Child<'_>>, spacing: f64) -> Child<'_> {
    compose(children, spacing, Arrangement::Column)
}

/// Children in rows that break onto new lines, with `spacing` between
/// children and between rows: the engine's `wrap_child`. Rows are packed
/// greedily by the children's minimum widths; each row then shares the
/// whole width as a row does.
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
            // A child of weight zero never gives width in a shortage, so the
            // least it can take is its preferred width.
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
                        // A height bound, not the height at that width: how
                        // the children fall into rows, and so how tall the
                        // wrap is, depends on the width, which a measurement
                        // does not know. plan() measures the rows.
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
            // As in measure(): weight zero protects the preferred width.
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

/// One entry in the root's ordered policy: a tree to try at one exact
/// level. The engine's `Stage`.
pub struct Stage<'a> {
    /// The level every child in `content` is asked at.
    pub level: Level,
    /// The tree, usually with the arrangement this stage tries.
    pub content: Child<'a>,
}

impl<'a> Stage<'a> {
    /// A stage trying `content` at `level`.
    pub fn new(level: Level, content: Child<'a>) -> Self {
        Self { level, content }
    }
}

/// The stage taken and its plan: the engine's `StagePlan`.
#[derive(Clone, Debug, PartialEq)]
pub struct StagePlan {
    /// The stage's position in the list given to [`choose_stage`].
    pub stage_index: usize,
    /// The stage's level.
    pub level: Level,
    /// The plan of the stage's tree.
    pub geometry: GeometryPlan,
}

/// Why one stage did not fit: the engine's error, and the nearest
/// identified child it came from, if any child on the way had an identity.
#[derive(Clone, Debug, PartialEq)]
pub struct StageFailure {
    /// The stage's position in the list given to [`choose_stage`].
    pub stage_index: usize,
    /// The stage's level.
    pub level: Level,
    /// The first failure met in the stage, as the engine's query returns
    /// it.
    pub error: AllocationError,
    /// The identity of the child the failure came from: the child itself
    /// if it has one, otherwise the nearest enclosing child that does. A
    /// failure of a composite's own sharing (its children's minima do not
    /// fit) is the composite's.
    pub child: Option<Id>,
}

/// No declared stage supplied complete fitting geometry. Nothing is implicitly
/// hidden, clipped, scrolled or promoted to a stronger undeclared level.
/// The engine's `NoFit`, with what failed at each stage added.
#[derive(Clone, Debug, PartialEq)]
pub struct NoFit {
    /// How many stages were tried: all of them.
    pub attempted: usize,
    /// One entry per stage, in the order tried.
    pub failures: Vec<StageFailure>,
}

/// Take the first stage whose tree fits `available`: the engine's
/// `choose_stage`. Each stage is planned from scratch at its own level
/// ([`Child::query_at`]), so nothing from a stage that failed carries over.
pub fn choose_stage(stages: &[Stage<'_>], available: Size) -> Result<StagePlan, NoFit> {
    let mut failures = Vec::new();
    for (stage_index, stage) in stages.iter().enumerate() {
        // Every stage is measured and allocated afresh at its own level,
        // from its preferred sizes: a stage that compacts some children
        // frees room for others to grow back, and nothing from a stage that
        // failed is kept.
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

type Fallback<'a> = Rc<dyn Fn(Size) -> Size + 'a>;

/// A part with its own ordered list of stages, which it chooses among from
/// the room it is given: the engine's `ResponsiveLevels`, without its
/// widget. Made by [`stages`] or [`responsive_child`]; laid out directly
/// with [`Self::layout`], or placed in another layout as a leaf with
/// `Child::from` (or `into()`), as the engine places it with `child`. The
/// module documentation's *A part with its own stages* says what the
/// outer layout sees of it, and works an example.
pub struct ResponsiveLevels<'a> {
    stages: Vec<Stage<'a>>,
    fallback: Option<Fallback<'a>>,
}

/// A part that chooses among `stages`, in order, by its own room: the
/// engine's `stages`. With no fallback, a room where none fits gets an
/// empty box.
pub fn stages(stages: Vec<Stage<'_>>) -> ResponsiveLevels<'_> {
    ResponsiveLevels {
        stages,
        fallback: None,
    }
}

/// The same child at each level from `Preferred` up to `max_level`, as
/// that many stages: the engine's `responsive_child`.
pub fn responsive_child(max_level: Level, child: Child<'_>) -> ResponsiveLevels<'_> {
    stages(
        Level::ALL
            .into_iter()
            .take_while(|level| *level <= max_level)
            .map(|level| Stage::new(level, child.clone()))
            .collect(),
    )
}

impl<'a> ResponsiveLevels<'a> {
    /// What to show when no stage fits: the engine's `fallback`, with a
    /// function giving the fallback's size within a maximum (an infinite
    /// width meaning unbounded) in place of its view. The fallback takes
    /// the place of the stages; it is not checked against the room.
    pub fn fallback(mut self, fallback: impl Fn(Size) -> Size + 'a) -> Self {
        self.fallback = Some(Rc::new(fallback));
        self
    }

    /// Choose a stage within `maximum` and say how large the part is: the
    /// engine's `ResponsiveLevels::layout` with its default width and
    /// height (fill the width, shrink to the height), and its
    /// `selection`, without the native commit and its agreement check.
    ///
    /// The choice is [`choose_stage`] in `maximum`. The size is the chosen
    /// stage's, or, when none fits, the fallback's laid out within
    /// `maximum` (nothing for no fallback); then its width is the whole of
    /// a finite `maximum.width` (an unbounded one keeps the content's
    /// width), and its height is the content's, no more than
    /// `maximum.height`.
    pub fn layout(&self, maximum: Size) -> (Size, Result<StagePlan, NoFit>) {
        let selection = choose_stage(&self.stages, maximum);
        let content = match &selection {
            Ok(chosen) => chosen.geometry.size,
            Err(_) => self
                .fallback
                .as_ref()
                .map_or(Size::new(0.0, 0.0), |fallback| fallback(maximum)),
        };
        // iced's `Limits::resolve` for a fill width and a shrink height.
        // Only an unbounded width compresses a fill width to its content.
        let width = if maximum.width.is_infinite() {
            content.width.max(0.0)
        } else {
            maximum.width
        };
        let height = content.height.min(maximum.height).max(0.0);
        (Size::new(width, height), selection)
    }
}

/// The engine's native leaf around a `ResponsiveLevels`. Its intrinsic
/// size, at any level, is its layout in unbounded room; the level it is
/// asked at is not passed on.
struct Nested<'a>(Rc<ResponsiveLevels<'a>>);

impl Leaf for Nested<'_> {
    fn query(&self, _: Level) -> MeasuredSize {
        let (size, _) = self.0.layout(Size::INFINITE);
        MeasuredSize {
            preferred: size,
            minimum: size,
        }
    }

    fn layout(&self, _: Level, maximum: Size) -> Size {
        self.0.layout(maximum).0
    }
}

/// A part with its own stages as a leaf of another layout: the engine's
/// `child(move |_| stages(..).into())`. Its preferred size and minimum are
/// its size in unbounded room (the first stage that fits there), unless a
/// [`Child::minimum_width`] is declared, as for any leaf; laid out, it
/// chooses its stage in the width it is given, and its plan carries that
/// choice ([`GeometryPlan::selection`]).
impl<'a> From<ResponsiveLevels<'a>> for Child<'a> {
    fn from(levels: ResponsiveLevels<'a>) -> Self {
        let levels = Rc::new(levels);
        let mut result = child(Nested(levels.clone()));
        result.nested = Some(levels);
        result
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
