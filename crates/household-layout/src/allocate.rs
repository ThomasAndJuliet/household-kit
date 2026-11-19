// SPDX-License-Identifier: GPL-3.0-only

//! Sharing a shortage between children, and the first form that fits.
//!
//! Both are household-widgets' engine (`allocate_width` and
//! `first_fitting` in its `layout.rs`), ported to `f64` with the same
//! behaviour. A length here runs in either direction: a row shares width,
//! a column shares height.

use crate::geometry::{FIT_EPSILON, Size};
use crate::level::Level;

/// The first level, up to `ceiling`, whose size fits `available`; `None`
/// says plainly that no permitted form fits. A size that is not a number,
/// infinite or negative never fits. No rejected query is retained.
pub fn first_fitting(
    ceiling: Level,
    available: Size,
    measure: impl Fn(Level) -> Size,
) -> Option<Level> {
    Level::ALL
        .into_iter()
        .take_while(|level| *level <= ceiling)
        .find(|level| {
            let size = measure(*level);
            size.width.is_finite() && size.height.is_finite() && size.is_valid() && size.fits(available)
        })
}

/// A child's length along the shared direction: the length it prefers,
/// its least length, and its willingness to shrink.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capacity {
    pub preferred: f64,
    pub least: f64,
    /// Higher weights shrink faster; zero protects the preferred length.
    pub weight: f64,
}

impl Capacity {
    pub const fn new(preferred: f64, least: f64, weight: f64) -> Self {
        Self {
            preferred,
            least,
            weight,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AllocationError {
    /// A length that is not a finite number, a negative one, or a least
    /// above the preferred.
    InvalidInput,
    /// The least lengths and the protected preferred lengths together
    /// exceed the room.
    NoFit { required: f64, available: f64 },
}

/// Share a shortage at each child's original rate,
/// `weight * (preferred - least)`, clamping at the least lengths and
/// sharing what remains again at the remaining children's original rates.
/// A zero weight protects the preferred length. Surplus room is left
/// unallocated, and no level is changed here.
pub fn allocate(available: f64, capacities: &[Capacity]) -> Result<Vec<f64>, AllocationError> {
    if !available.is_finite()
        || available < 0.0
        || capacities.iter().any(|capacity| {
            !capacity.preferred.is_finite()
                || !capacity.least.is_finite()
                || !capacity.weight.is_finite()
                || capacity.least < 0.0
                || capacity.preferred < capacity.least
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
                capacity.least
            }
        })
        .sum();
    if required > available + FIT_EPSILON {
        return Err(AllocationError::NoFit {
            required,
            available,
        });
    }
    let mut lengths: Vec<f64> = capacities.iter().map(|capacity| capacity.preferred).collect();
    let rates: Vec<f64> = capacities
        .iter()
        .map(|capacity| capacity.weight * (capacity.preferred - capacity.least))
        .collect();
    let mut shortage = (lengths.iter().sum::<f64>() - available).max(0.0);
    while shortage > 0.0 {
        let total_rate: f64 = capacities
            .iter()
            .enumerate()
            .filter(|(i, capacity)| lengths[*i] > capacity.least)
            .map(|(i, _)| rates[i])
            .sum();
        if total_rate == 0.0 {
            break;
        }
        let mut removed = 0.0;
        let mut clamped = false;
        for (i, capacity) in capacities.iter().enumerate() {
            let room = lengths[i] - capacity.least;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_fit_respects_preference_bounds_height_and_explicit_failure() {
        let measured = |level| match level {
            Level::Preferred | Level::Mild => Size::new(160.0, 48.0),
            Level::Strong => Size::new(100.0, 40.0),
            Level::AllPermitted => Size::new(60.0, 40.0),
        };
        assert_eq!(
            first_fitting(Level::AllPermitted, Size::new(160.0, 48.0), measured),
            Some(Level::Preferred)
        );
        assert_eq!(
            first_fitting(Level::AllPermitted, Size::new(160.0, 40.0), measured),
            Some(Level::Strong)
        );
        assert_eq!(
            first_fitting(Level::Mild, Size::new(100.0, 48.0), measured),
            None
        );
        assert_eq!(
            first_fitting(Level::AllPermitted, Size::new(59.0, 48.0), measured),
            None
        );
        assert_eq!(
            first_fitting(Level::AllPermitted, Size::INFINITE, |_| Size::new(
                f64::NAN,
                0.0
            )),
            None
        );
    }

    fn capacity(preferred: f64, least: f64, weight: f64) -> Capacity {
        Capacity::new(preferred, least, weight)
    }

    #[test]
    fn capacity_sharing_uses_original_rates_clamps_and_protects_zero_weight() {
        let children = [
            capacity(100.0, 80.0, 3.0),
            capacity(100.0, 0.0, 1.0),
            capacity(40.0, 0.0, 0.0),
        ];
        // Original rates are 60 and 100. At 160 the first child reaches its
        // least, leaving the remaining shortage entirely to the second.
        assert_eq!(allocate(160.0, &children), Ok(vec![80.0, 40.0, 40.0]));
        assert_eq!(allocate(300.0, &children), Ok(vec![100.0, 100.0, 40.0]));
        assert_eq!(
            allocate(119.0, &children),
            Err(AllocationError::NoFit {
                required: 120.0,
                available: 119.0
            })
        );
        let weighted =
            allocate(150.0, &[capacity(100.0, 0.0, 2.0), capacity(100.0, 0.0, 1.0)]).unwrap();
        assert!((weighted.iter().sum::<f64>() - 150.0).abs() < 0.001);
        assert!(((100.0 - weighted[0]) / (100.0 - weighted[1]) - 2.0).abs() < 0.001);
        assert_eq!(allocate(0.0, &[]), Ok(vec![]));
        assert_eq!(
            allocate(10.0, &[capacity(1.0, 2.0, 1.0)]),
            Err(AllocationError::InvalidInput)
        );
    }

    #[test]
    fn the_fit_tolerance_is_the_crates() {
        // A total a hair over the room, within FIT_EPSILON, still fits.
        assert!(allocate(99.9995, &[capacity(100.0, 100.0, 1.0)]).is_ok());
        assert!(allocate(99.99, &[capacity(100.0, 100.0, 1.0)]).is_err());
    }
}
