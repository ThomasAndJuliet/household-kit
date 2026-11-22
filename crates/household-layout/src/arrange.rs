// SPDX-License-Identifier: GPL-3.0-only

//! How a row, a column and a wrap are measured and planned.

use crate::part::{Child, Composition};
use crate::plan::{Failure, Placed, Plan, no_fit};
use crate::{
    Arrangement, FIT_EPSILON, Host, Level, Measured, Point, Size, WidthCapacity, allocate_one,
    allocate_with_growth, validate_measurement,
};

/// A visible child of a composition, as planning carries it: its index among
/// the declared children (which its placement records), the child, and its
/// measurement at the level.
type Visible<'c, 'a, H> = (usize, &'c Child<'a, H>, Measured);

/// A planned row's height (its tallest placement's) and its floor of height
/// (its highest placement floor): what a wrap shares a shortage of height
/// with between its rows.
struct RowExtent {
    height: f32,
    minimum_height: f32,
}

impl<'a, H: Host> Composition<'a, H> {
    /// What the arrangement measures from its visible children, each
    /// measured once at the level. A child's minimum width here is the width
    /// it cannot go below (`WidthCapacity::least`). Spacing goes between
    /// visible children only.
    ///
    /// One function with an arm per arrangement, not three: the arms share
    /// the measured children and the gap, and each arm is whole on its own.
    /// The row and the wrap prefer the same size; they differ in the
    /// minimum width, which for a row counts every gap and for a wrap counts
    /// none.
    pub(crate) fn measure(&self, level: Level, context: &H::Context) -> Measured {
        let measured: Vec<_> = self
            .children
            .iter()
            .filter(|child| !child.is_hidden(level))
            .map(|child| (child, child.query(level, context)))
            .collect();
        let gap = self.spacing * measured.len().saturating_sub(1) as f32;
        let mut result = Measured {
            preferred: Size::ZERO,
            minimum: Size::ZERO,
        };
        match self.arrangement {
            // Side by side on one line: the widths and the gaps add up, in
            // the minimum as in the preferred; the height is the tallest.
            Arrangement::Row => {
                for (child, size) in &measured {
                    let least = child.width_capacity(*size).least();
                    result.preferred.width += size.preferred.width;
                    result.preferred.height = result.preferred.height.max(size.preferred.height);
                    result.minimum.width += least;
                    result.minimum.height = result.minimum.height.max(size.minimum.height);
                }
                result.preferred.width += gap;
                result.minimum.width += gap;
            }
            // Preferred on one line, as a row. At its minimum width every
            // child has a row of its own, so that width is the widest floor,
            // with no gap. The minimum height is a bound, the tallest child's
            // floor, not the height of the rows at that width: `plan_wrap`
            // measures rows.
            Arrangement::Wrap => {
                for (child, size) in &measured {
                    let least = child.width_capacity(*size).least();
                    result.preferred.width += size.preferred.width;
                    result.preferred.height = result.preferred.height.max(size.preferred.height);
                    result.minimum.width = result.minimum.width.max(least);
                    result.minimum.height = result.minimum.height.max(size.minimum.height);
                }
                result.preferred.width += gap;
            }
            // Stacked: the width is the widest; the heights and the gaps add
            // up, in the minimum as in the preferred.
            Arrangement::Column => {
                for (child, size) in &measured {
                    let least = child.width_capacity(*size).least();
                    result.preferred.width = result.preferred.width.max(size.preferred.width);
                    result.minimum.width = result.minimum.width.max(least);
                    result.preferred.height += size.preferred.height;
                    result.minimum.height += size.minimum.height;
                }
                result.preferred.height += gap;
                result.minimum.height += gap;
            }
        }
        result
    }

    /// Plan the arrangement in `width` × `height`. Each arrangement has a
    /// function of its own, over the helpers below for what they share.
    ///
    /// Which part a failure is about (`Failure`). A check on one child (its
    /// measurement is not a valid size; its floor is wider than the room)
    /// is that child's, and so is whatever fails while the child is planned,
    /// which comes up with the child's index in front. A check on the
    /// children together (the gaps are wider than the room; the room cannot
    /// hold every child's floor, as `allocate_with_growth` finds for a row's
    /// widths, a column's heights or a wrap's rows) is the composition's
    /// own, with an empty path.
    pub(crate) fn plan(
        &self,
        level: Level,
        context: &H::Context,
        width: f32,
        height: f32,
    ) -> Result<Plan, Failure> {
        match self.arrangement {
            Arrangement::Column => self.plan_column(level, context, width, height),
            Arrangement::Row => self.plan_row(level, context, width, height),
            Arrangement::Wrap => self.plan_wrap(level, context, width, height),
        }
    }

    /// The visible children, in their declared order, each with its index
    /// among the declared children and its measurement at the level. Every
    /// one is measured first, and then each measurement is checked in order,
    /// so the first that is not a valid size fails the plan with
    /// `InvalidInput` before anything is allocated: that child's failure.
    fn measure_visible(
        &self,
        level: Level,
        context: &H::Context,
    ) -> Result<Vec<Visible<'_, 'a, H>>, Failure> {
        let visible: Vec<_> = self
            .children
            .iter()
            .enumerate()
            .filter(|(_, child)| !child.is_hidden(level))
            .map(|(index, child)| (index, child, child.query(level, context)))
            .collect();
        for (index, _, size) in &visible {
            validate_measurement(*size).map_err(|error| Failure::from(error).in_child(*index))?;
        }
        Ok(visible)
    }

    /// A column stacks its visible children from the top, with the spacing
    /// between them, each at the left.
    ///
    /// The gaps come first: wider than the room's height, they fail the
    /// column before anything is planned. Then width: each child is
    /// allocated the column's whole width on its own, by its capacity and
    /// its growth (`allocate_one`), and is planned at that width with
    /// unbounded height. Its plan therefore gives the height it needs at
    /// that width, and its floor of height, which is below its height only
    /// for a leaf that takes a range of heights or a plan that holds one
    /// (`LeafHeight`).
    ///
    /// Then height: the room's height less the gaps is shared among the
    /// children as a length with growth (`allocate_with_growth`). Each child
    /// offers its planned height down to its floor at its own weight, so a
    /// weight of zero protects a preferred height here as it does a width,
    /// and takes a share of a surplus by its `grow_height`. An ordinary
    /// child's height is its floor, so a shortage falls on the leaves that
    /// take a range of heights alone, and with none to take it the column
    /// does not fit. A child given a height other than the one it was
    /// planned at is planned again at it. The column is as wide as its
    /// widest child, and its floor of height is its gaps and every child's
    /// floor.
    fn plan_column(
        &self,
        level: Level,
        context: &H::Context,
        width: f32,
        height: f32,
    ) -> Result<Plan, Failure> {
        let visible = self.measure_visible(level, context)?;
        let mut result = Plan::leaf(Size::ZERO);
        let gap = self.spacing * visible.len().saturating_sub(1) as f32;
        if gap > height {
            return Err(no_fit(gap, height));
        }
        let mut proposals = Vec::new();
        for (index, child, measured) in visible {
            // The child's width alone: its floor against the column's width.
            let allocated = allocate_one(width, child.width_capacity(measured), child.sharing.grow)
                .map_err(|error| Failure::from(error).in_child(index))?;
            let geometry = child
                .plan_blaming(level, context, allocated, f32::INFINITY)
                .map_err(|failure| failure.in_child(index))?;
            proposals.push((index, child, allocated, geometry));
        }
        // The children's heights together against the room: a shortage here
        // is the column's own (`Failure`).
        let heights = allocate_with_growth(
            height - gap,
            &proposals
                .iter()
                .map(|(_, child, _, plan)| WidthCapacity {
                    preferred: plan.size.height,
                    minimum: plan.minimum_height,
                    weight: child.sharing.weight,
                })
                .collect::<Vec<_>>(),
            &proposals
                .iter()
                .map(|(_, child, _, _)| child.sharing.grow_height)
                .collect::<Vec<_>>(),
        )?;
        result.minimum_height = gap;
        for ((index, child, allocated, proposal), allocated_height) in
            proposals.into_iter().zip(heights)
        {
            let geometry = if (allocated_height - proposal.size.height).abs() > FIT_EPSILON {
                child
                    .plan_blaming(level, context, allocated, allocated_height)
                    .map_err(|failure| failure.in_child(index))?
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
            result.children.push(Placed {
                index,
                position,
                plan: geometry,
            });
        }
        Ok(result)
    }

    /// A row sets its visible children side by side on one line from the
    /// left, with the spacing between them, each centred on the row's
    /// height or, for `Child::row_top`, aligned to its top.
    ///
    /// Width first. A child's floor is the width it cannot go below
    /// (`WidthCapacity::least`); a floor wider than the row fails it, child
    /// by child in order, before anything is allocated. Then the row's width
    /// less its gaps is shared
    /// among the children by their capacities: a shortage by weight, a
    /// surplus by growth (`place_row`).
    ///
    /// Then height: each child is planned at its width within the room's
    /// height, so a leaf that takes a range of heights takes at once what
    /// the row has, and a child taller than the row fails it there. This is
    /// where a row and a wrap differ: a row has one line, so the room's
    /// height is the line's, and there is no shortage left to share
    /// afterwards. The row is as high as its tallest child, and its floor of
    /// height is its highest floor.
    fn plan_row(
        &self,
        level: Level,
        context: &H::Context,
        width: f32,
        height: f32,
    ) -> Result<Plan, Failure> {
        let visible = self.measure_visible(level, context)?;
        for (index, child, measured) in &visible {
            let minimum = child.width_capacity(*measured).least();
            if minimum > width + FIT_EPSILON {
                return Err(no_fit(minimum, width).in_child(*index));
            }
        }
        let mut result = Plan::leaf(Size::ZERO);
        if !visible.is_empty() {
            self.place_row(level, context, visible, width, height, &mut result)?;
            // The spacing `place_row` adds after a row.
            result.size.height -= self.spacing;
            result.minimum_height -= self.spacing;
        }
        Ok(result)
    }

    /// A wrap fills rows from the left and starts a new row below when the
    /// next child does not fit beside the ones before it, with the spacing
    /// between children and between rows. A wrap is never top-aligned (only
    /// `Child::row_top` declares that), so its children are centred on their
    /// row's height.
    ///
    /// Width first. The children are packed into rows in order by their
    /// floors, the width each cannot go below (`WidthCapacity::least`): a
    /// child whose floor, after a gap, would take its row past the room
    /// starts the next row, and a child whose floor alone is wider than the
    /// room fails the wrap. Each
    /// row is then given the whole width afresh and shared among its
    /// children as a row's is (`place_row`).
    ///
    /// Then height, in a second pass. A wrap's children are planned with
    /// unbounded height, unlike a row's, because no row's height is known
    /// until every row is planned. If the rows stacked are taller than the
    /// room, the wrap is shared as a column of atomic rows: the room less the
    /// gaps between rows is shared among the rows, each offering its height
    /// down to its floor (`RowExtent`) at weight 1 and with no growth, so a
    /// shortage falls on the rows that hold a leaf that takes a range of
    /// heights. A placement taller than its row's new height is planned
    /// again at that height, and each placement is centred on it. The plan's
    /// floor of height stays the one the rows were first planned with.
    fn plan_wrap(
        &self,
        level: Level,
        context: &H::Context,
        width: f32,
        height: f32,
    ) -> Result<Plan, Failure> {
        let visible = self.measure_visible(level, context)?;
        // `row` is the row being filled, with `used` its width so far; it
        // joins `rows` when the next child does not fit beside it, and after
        // the last child. A row is never empty in `rows`.
        let mut rows: Vec<Vec<Visible<'_, 'a, H>>> = Vec::new();
        let mut row: Vec<Visible<'_, 'a, H>> = Vec::new();
        let mut used = 0.0;
        for (index, child, measured) in visible {
            let minimum = child.width_capacity(measured).least();
            if minimum > width + FIT_EPSILON {
                return Err(no_fit(minimum, width).in_child(index));
            }
            if !row.is_empty() && used + self.spacing + minimum > width + FIT_EPSILON {
                rows.push(std::mem::take(&mut row));
                used = 0.0;
            }
            if !row.is_empty() {
                used += self.spacing;
            }
            used += minimum;
            row.push((index, child, measured));
        }
        if !row.is_empty() {
            rows.push(row);
        }
        // The child each placement was planned from, in the placements'
        // order, and each row's height and floor, for the second pass.
        let mut result = Plan::leaf(Size::ZERO);
        let mut planned: Vec<&Child<'a, H>> = Vec::new();
        let mut row_extents: Vec<WidthCapacity> = Vec::new();
        for row in rows {
            planned.extend(row.iter().map(|(_, child, _)| *child));
            let extent = self.place_row(level, context, row, width, f32::INFINITY, &mut result)?;
            row_extents.push(WidthCapacity {
                preferred: extent.height,
                minimum: extent.minimum_height,
                weight: 1.0,
            });
        }
        if !result.rows.is_empty() {
            // The spacing `place_row` adds after the last row.
            result.size.height -= self.spacing;
            result.minimum_height -= self.spacing;
        }
        if result.size.height > height + FIT_EPSILON {
            let gap = self.spacing * result.rows.len().saturating_sub(1) as f32;
            if gap > height {
                return Err(no_fit(gap, height));
            }
            // No placement's plan has changed since `row_extents` was
            // recorded. The rows together against the room: a shortage here
            // is the wrap's own.
            let heights =
                allocate_with_growth(height - gap, &row_extents, &vec![0.0; row_extents.len()])?;
            let mut y = 0.0;
            // The rows are consecutive runs of the placements, in order, so
            // each row takes the next placements, as many as it holds, with
            // the children they were planned from.
            let mut placements = result.children.iter_mut().zip(planned);
            for (row, row_height) in result.rows.iter().zip(heights) {
                for (placed, child) in placements.by_ref().take(row.len()) {
                    if placed.plan.size.height > row_height + FIT_EPSILON {
                        let index = placed.index;
                        placed.plan = child
                            .plan_blaming(level, context, placed.plan.size.width, row_height)
                            .map_err(|failure| failure.in_child(index))?;
                    }
                    placed.position.y = y + (row_height - placed.plan.size.height) / 2.0;
                }
                y += row_height + self.spacing;
            }
            result.size.height = (y - self.spacing).max(0.0);
        }
        Ok(result)
    }

    /// Place one row of children (never empty) below the rows already in
    /// `result`, as a row and each of a wrap's rows are placed.
    ///
    /// The row's gaps come first: wider than `width`, they fail it. The rest
    /// of `width` is shared among the children by their capacities, a
    /// shortage by weight and a surplus by growth (`allocate_with_growth`),
    /// and each child is planned at its width within `offered_height` (the
    /// caller says which height a row offers; see `plan_row` and
    /// `plan_wrap`). The children are set side by side from the left, at the
    /// top of the row, then each is moved down to the middle of the row's
    /// height unless the composition is top-aligned.
    ///
    /// The row joins `result.rows` as the indices of its placements; the
    /// plan is as wide as its widest row, and its height and floor of height
    /// grow by the row's and by one spacing after it, which the caller takes
    /// off after its last row. Returns the row's height and floor.
    fn place_row(
        &self,
        level: Level,
        context: &H::Context,
        row: Vec<Visible<'_, 'a, H>>,
        width: f32,
        offered_height: f32,
        result: &mut Plan,
    ) -> Result<RowExtent, Failure> {
        let gap = self.spacing * row.len().saturating_sub(1) as f32;
        if gap > width {
            return Err(no_fit(gap, width));
        }
        let capacities: Vec<_> = row
            .iter()
            .map(|(_, child, measured)| child.width_capacity(*measured))
            .collect();
        let grows: Vec<_> = row.iter().map(|(_, child, _)| child.sharing.grow).collect();
        // The row's children together against its width: a shortage here is
        // the composition's own.
        let widths = allocate_with_growth(width - gap, &capacities, &grows)?;
        let start = result.children.len();
        let mut row_width = 0.0;
        let mut row_height: f32 = 0.0;
        let mut row_minimum_height: f32 = 0.0;
        for ((index, child, _), allocated) in row.into_iter().zip(widths) {
            let geometry = child
                .plan_blaming(level, context, allocated, offered_height)
                .map_err(|failure| failure.in_child(index))?;
            if result.children.len() > start {
                row_width += self.spacing;
            }
            let position = Point::new(row_width, result.size.height);
            row_width += geometry.size.width;
            row_height = row_height.max(geometry.size.height);
            row_minimum_height = row_minimum_height.max(geometry.minimum_height);
            result.children.push(Placed {
                index,
                position,
                plan: geometry,
            });
        }
        // The row's placements are the ones pushed since `start`.
        for placed in result.children.iter_mut().skip(start) {
            if !self.top_aligned {
                placed.position.y += (row_height - placed.plan.size.height) / 2.0;
            }
        }
        result.rows.push((start..result.children.len()).collect());
        result.size.width = result.size.width.max(row_width);
        result.size.height += row_height + self.spacing;
        result.minimum_height += row_minimum_height + self.spacing;
        Ok(RowExtent {
            height: row_height,
            minimum_height: row_minimum_height,
        })
    }
}
