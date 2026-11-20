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
    /// `allocate_width` requires, and, in household-widgets' layout, the
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
