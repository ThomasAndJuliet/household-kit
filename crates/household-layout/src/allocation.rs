// SPDX-License-Identifier: GPL-3.0-only

//! Sharing a length among parts: what each part prefers, what it can work
//! in, and how readily it gives length up.

use crate::{Size, finite_size};

/// The tolerance within which a size fits its room: a size at most this much
/// larger than its room still fits it.
///
/// Native geometry uses f32. Summing neighbouring requested dimensions in
/// f64 can exceed their f32 total by a few ULPs after a divider drag, so an
/// exact comparison would refuse a fit that is exact on screen. Every check
/// of a fit (allocation here, and a toolkit's query and commit) uses this
/// one tolerance, so that none is stricter than another.
pub const FIT_EPSILON: f32 = 0.001;

/// Width a child prefers, its lower bound, and its willingness to shrink.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WidthCapacity {
    pub preferred: f32,
    pub minimum: f32,
    /// Higher weights shrink faster; zero protects the preferred width.
    pub weight: f32,
}

impl WidthCapacity {
    /// The width this capacity cannot be given less than: its minimum, or
    /// its preferred width when its weight is zero.
    ///
    /// This is the one statement of the rule "a weight of zero protects the
    /// preferred width". A weight says how fast a part gives width up in a
    /// shortage, and a part that gives none up offers its preferred width as
    /// its minimum. Every place that needs that width asks here: what
    /// `allocate_width` requires, and, in planning, the
    /// minimum width a composition and a frame measure
    /// (`Composition::measure`, `Frame::measure`), and the floor by which a
    /// row and a wrap check and pack their children (`Composition::plan_row`,
    /// `Composition::plan_wrap`).
    ///
    /// The name says width because the apps' interface does
    /// (`WidthCapacity`, `allocate_width`), but nothing here is particular
    /// to width: a column shares height with the same capacities, so there
    /// it protects a preferred height (`Composition::plan_column`), and a
    /// wrap shares height between its rows with them.
    ///
    /// Two parts that prefer 100 and can work in 40: the one of weight 1
    /// can be given 40, the one of weight 0 no less than its 100. Together
    /// they need 140, which is what `allocate_width` requires of the room.
    ///
    /// ```
    /// use household_layout::{AllocationError, WidthCapacity, allocate_width};
    ///
    /// let yielding = WidthCapacity { preferred: 100.0, minimum: 40.0, weight: 1.0 };
    /// let protected = WidthCapacity { weight: 0.0, ..yielding };
    /// assert_eq!(yielding.least(), 40.0);
    /// assert_eq!(protected.least(), 100.0);
    ///
    /// assert_eq!(
    ///     allocate_width(130.0, &[yielding, protected]),
    ///     Err(AllocationError::NoFit { required: 140.0, available: 130.0 }),
    /// );
    /// ```
    pub fn least(self) -> f32 {
        if self.weight == 0.0 {
            self.preferred
        } else {
            self.minimum
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AllocationError {
    InvalidInput,
    /// The minima and zero-weight preferred widths exceed the available width.
    NoFit {
        required: f32,
        available: f32,
    },
}

/// Share a shortage using each child's original `weight * (preferred-minimum)`
/// rate, clamping minima and redistributing at the remaining original rates.
/// Surplus space is left unallocated. No representation levels are scheduled.
///
/// This is how parts set side by side share a room too narrow for all of
/// them at their preferred widths: a row's children, and a column's
/// children in height (see `WidthCapacity::least`). It returns one width per
/// capacity, in their order. Each part starts at its preferred width, and
/// the shortage (their total less the room) is taken from them in
/// proportion to their rates, `weight × (preferred − minimum)`: a part
/// gives width up faster the higher its weight and the more it has to give.
/// A part that would be taken below its minimum stops there, and what it
/// could not give is shared again among the rest at their own rates. A
/// part of weight zero has a rate of zero and keeps its preferred width.
/// The room must hold every part's `least`, within `FIT_EPSILON`, or the
/// answer is `NoFit` with what was required; a room or a capacity that is
/// not a usable number is `InvalidInput`. With room to spare, every part
/// has its preferred width and the rest of the room is left over:
/// `allocate_with_growth` shares it.
///
/// A shortage shared by rate. Two parts prefer 100; the first can work in
/// 40 at weight 1 (rate 60), the second in 80 at weight 2 (rate 40). In
/// 160 the shortage is 40, which they give up as 60 : 40, that is 24 and
/// 16:
///
/// ```
/// use household_layout::{WidthCapacity, allocate_width};
///
/// let capacities = [
///     WidthCapacity { preferred: 100.0, minimum: 40.0, weight: 1.0 },
///     WidthCapacity { preferred: 100.0, minimum: 80.0, weight: 2.0 },
/// ];
/// assert_eq!(allocate_width(160.0, &capacities), Ok(vec![76.0, 84.0]));
/// ```
///
/// A part clamped at its minimum, and the rest shared again. The first part
/// can give 20 at weight 3 (rate 60), the second 100 at weight 1 (rate
/// 100). In 120 the shortage is 80, of which the first part's share is
/// 30: more than its 20, so it stops at its minimum of 80 and the second
/// gives 50. The 10 the first part could not give is shared again among the
/// parts still above their minimum, here the second alone, which ends at 40:
///
/// ```
/// use household_layout::{WidthCapacity, allocate_width};
///
/// let capacities = [
///     WidthCapacity { preferred: 100.0, minimum: 80.0, weight: 3.0 },
///     WidthCapacity { preferred: 100.0, minimum: 0.0, weight: 1.0 },
/// ];
/// assert_eq!(allocate_width(120.0, &capacities), Ok(vec![80.0, 40.0]));
/// ```
///
/// A weight of zero protects the preferred width. Of two parts that prefer
/// 100 and can work in 40, the one of weight 0 keeps its 100 and the other
/// gives up the whole shortage of 50:
///
/// ```
/// use household_layout::{WidthCapacity, allocate_width};
///
/// let capacities = [
///     WidthCapacity { preferred: 100.0, minimum: 40.0, weight: 0.0 },
///     WidthCapacity { preferred: 100.0, minimum: 40.0, weight: 1.0 },
/// ];
/// assert_eq!(allocate_width(150.0, &capacities), Ok(vec![100.0, 50.0]));
/// ```
pub fn allocate_width(
    available: f32,
    capacities: &[WidthCapacity],
) -> Result<Vec<f32>, AllocationError> {
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
        .map(|capacity| f64::from(capacity.least()))
        .sum();
    // Native geometry uses f32. Summing neighbouring requested dimensions in
    // f64 can exceed their f32 total by a few ULPs after a divider drag. Use
    // the same fit tolerance as query/commit, not a stricter hidden threshold.
    if required > f64::from(available) + f64::from(FIT_EPSILON) {
        return Err(AllocationError::NoFit {
            required: required as f32,
            available,
        });
    }
    let mut widths: Vec<f64> = capacities
        .iter()
        .map(|capacity| f64::from(capacity.preferred))
        .collect();
    let rates: Vec<f64> = capacities
        .iter()
        .map(|capacity| {
            f64::from(capacity.weight) * f64::from(capacity.preferred - capacity.minimum)
        })
        .collect();
    let mut shortage = (widths.iter().sum::<f64>() - f64::from(available)).max(0.0);
    // `widths` and `rates` hold one entry per capacity, in its order: each
    // pass walks the three together.
    while shortage > 0.0 {
        let total_rate: f64 = widths
            .iter()
            .zip(&rates)
            .zip(capacities)
            .filter(|((width, _), capacity)| **width > f64::from(capacity.minimum))
            .map(|((_, rate), _)| *rate)
            .sum();
        if total_rate == 0.0 {
            break;
        }
        let mut removed = 0.0;
        let mut clamped = false;
        for ((width, rate), capacity) in widths.iter_mut().zip(&rates).zip(capacities) {
            let room = *width - f64::from(capacity.minimum);
            if room <= 0.0 || *rate == 0.0 {
                continue;
            }
            let share = shortage * rate / total_rate;
            let reduction = share.min(room);
            *width -= reduction;
            removed += reduction;
            clamped |= share >= room;
        }
        if !clamped || removed == 0.0 {
            break;
        }
        shortage = (shortage - removed).max(0.0);
    }
    Ok(widths.into_iter().map(|width| width as f32).collect())
}

/// Share a length as `allocate_width` does, then share any surplus by
/// growth; in unbounded room, give every part its preferred width.
///
/// This is what the engine allocates with: a row's widths, a column's
/// widths (one child at a time, `allocate_one`) and heights, and the
/// heights of a wrap's rows. `grows` holds one growth factor per capacity,
/// in their order. In a shortage the answer is `allocate_width`'s, since
/// there is nothing left over to grow into. With room to spare, the
/// surplus (the room less the widths allocated) is shared among the parts
/// in proportion to their growth; when no part grows, it stays unallocated.
/// Unbounded room (infinite) is how a part is asked what it would take
/// without a limit: every part is given its preferred width and nothing
/// grows, as there is no surplus to share; a capacity whose preferred or
/// minimum width is not finite and not negative is `InvalidInput` there.
///
/// A surplus shared by growth. Two parts that prefer 60 and 40, growing
/// 1 : 3, in 200: the surplus of 100 goes 25 to the first and 75 to the
/// second:
///
/// ```
/// use household_layout::{WidthCapacity, allocate_with_growth};
///
/// let capacities = [
///     WidthCapacity { preferred: 60.0, minimum: 20.0, weight: 1.0 },
///     WidthCapacity { preferred: 40.0, minimum: 20.0, weight: 1.0 },
/// ];
/// assert_eq!(
///     allocate_with_growth(200.0, &capacities, &[1.0, 3.0]),
///     Ok(vec![85.0, 115.0]),
/// );
/// ```
///
/// Unbounded room gives the preferred widths, whatever the growth:
///
/// ```
/// use household_layout::{WidthCapacity, allocate_with_growth};
///
/// let capacities = [
///     WidthCapacity { preferred: 60.0, minimum: 20.0, weight: 1.0 },
///     WidthCapacity { preferred: 40.0, minimum: 20.0, weight: 1.0 },
/// ];
/// assert_eq!(
///     allocate_with_growth(f32::INFINITY, &capacities, &[1.0, 3.0]),
///     Ok(vec![60.0, 40.0]),
/// );
/// ```
pub fn allocate_with_growth(
    available: f32,
    capacities: &[WidthCapacity],
    grows: &[f32],
) -> Result<Vec<f32>, AllocationError> {
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
    let total: f64 = grows.iter().map(|grow| f64::from(*grow)).sum();
    if available.is_finite() && total > 0.0 {
        let surplus = (f64::from(available)
            - widths.iter().map(|width| f64::from(*width)).sum::<f64>())
        .max(0.0);
        for (width, grow) in widths.iter_mut().zip(grows) {
            *width += (surplus * f64::from(*grow) / total) as f32;
        }
    }
    Ok(widths)
}

/// The width allocated to one child alone, by the same rule as to several
/// (`allocate_with_growth`), which returns one width per capacity. Were it
/// to return none, the child would have no width, and its plan fails as for
/// invalid input rather than laying out at a width nobody allocated.
pub fn allocate_one(
    available: f32,
    capacity: WidthCapacity,
    grow: f32,
) -> Result<f32, AllocationError> {
    allocate_with_growth(available, &[capacity], &[grow])?
        .first()
        .copied()
        .ok_or(AllocationError::InvalidInput)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capacity(preferred: f32, minimum: f32, weight: f32) -> WidthCapacity {
        WidthCapacity {
            preferred,
            minimum,
            weight,
        }
    }

    const GOOD: WidthCapacity = WidthCapacity {
        preferred: 100.0,
        minimum: 40.0,
        weight: 1.0,
    };

    #[test]
    fn every_kind_of_bad_room_is_invalid_input() {
        for room in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0] {
            assert_eq!(
                allocate_width(room, &[GOOD]),
                Err(AllocationError::InvalidInput),
                "room {room}"
            );
        }
        // Unbounded room is a question allocate_with_growth answers; a room
        // that is not a number is not.
        assert_eq!(
            allocate_with_growth(f32::NAN, &[GOOD], &[0.0]),
            Err(AllocationError::InvalidInput)
        );
        assert_eq!(
            allocate_with_growth(-1.0, &[GOOD], &[0.0]),
            Err(AllocationError::InvalidInput)
        );
    }

    #[test]
    fn every_kind_of_bad_capacity_is_invalid_input() {
        let bad = [
            ("preferred NaN", capacity(f32::NAN, 40.0, 1.0)),
            ("preferred infinite", capacity(f32::INFINITY, 40.0, 1.0)),
            ("minimum NaN", capacity(100.0, f32::NAN, 1.0)),
            ("minimum infinite", capacity(100.0, f32::INFINITY, 1.0)),
            ("minimum negative", capacity(100.0, -1.0, 1.0)),
            ("preferred below minimum", capacity(30.0, 40.0, 1.0)),
            ("weight NaN", capacity(100.0, 40.0, f32::NAN)),
            ("weight infinite", capacity(100.0, 40.0, f32::INFINITY)),
            ("weight negative", capacity(100.0, 40.0, -1.0)),
        ];
        for (what, bad) in bad {
            // A bad capacity spoils the allocation wherever it stands.
            assert_eq!(
                allocate_width(1000.0, &[GOOD, bad]),
                Err(AllocationError::InvalidInput),
                "{what}"
            );
            assert_eq!(
                allocate_with_growth(1000.0, &[GOOD, bad], &[0.0, 0.0]),
                Err(AllocationError::InvalidInput),
                "{what}"
            );
            assert_eq!(
                allocate_one(1000.0, bad, 0.0),
                Err(AllocationError::InvalidInput),
                "{what}"
            );
        }
        // In unbounded room only the widths are checked, finite and not
        // negative.
        for bad in [
            capacity(f32::NAN, 40.0, 1.0),
            capacity(f32::INFINITY, 40.0, 1.0),
            capacity(100.0, f32::NAN, 1.0),
            capacity(100.0, -1.0, 1.0),
        ] {
            assert_eq!(
                allocate_with_growth(f32::INFINITY, &[GOOD, bad], &[0.0, 0.0]),
                Err(AllocationError::InvalidInput)
            );
        }
    }

    #[test]
    fn no_fit_says_what_was_required_and_what_there_was() {
        // Each part's least: 40 for weight 1, 100 for weight 0.
        let capacities = [GOOD, capacity(100.0, 40.0, 0.0), GOOD];
        let expected = Err(AllocationError::NoFit {
            required: 180.0,
            available: 179.0,
        });
        assert_eq!(allocate_width(179.0, &capacities), expected);
        assert_eq!(
            allocate_with_growth(179.0, &capacities, &[1.0, 1.0, 1.0]),
            expected
        );
        assert_eq!(
            allocate_one(39.0, GOOD, 0.0),
            Err(AllocationError::NoFit {
                required: 40.0,
                available: 39.0,
            })
        );
        // Exactly the least fits, each part at its least.
        assert_eq!(
            allocate_width(180.0, &capacities),
            Ok(vec![40.0, 100.0, 40.0])
        );
    }

    #[test]
    fn a_requirement_fits_within_the_tolerance_and_not_beyond_it() {
        let available = 100.0;
        let inside = available + FIT_EPSILON / 2.0;
        let outside = available + 2.0 * FIT_EPSILON;
        let fixed = |width: f32| capacity(width, width, 1.0);
        // Just inside: it fits, and the part keeps its width, as it cannot
        // give any up.
        assert_eq!(
            allocate_width(available, &[fixed(inside)]),
            Ok(vec![inside])
        );
        // Just outside: it does not.
        assert_eq!(
            allocate_width(available, &[fixed(outside)]),
            Err(AllocationError::NoFit {
                required: outside,
                available,
            })
        );
    }

    #[test]
    fn nothing_to_share_is_an_empty_allocation() {
        assert_eq!(allocate_width(0.0, &[]), Ok(vec![]));
        assert_eq!(allocate_with_growth(f32::INFINITY, &[], &[]), Ok(vec![]));
    }
}
