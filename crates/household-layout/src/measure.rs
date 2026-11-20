// SPDX-License-Identifier: GPL-3.0-only

//! How a part is measured: its kind's measurement, then the adjustments
//! its builders declared, in order, by one interpreter.

use crate::part::{Child, Kind, Leaf, LeafHeight};
use crate::{FIT_EPSILON, Host, Level, Measured, Offer, Size, WidthCapacity, finite_size};
use std::rc::Rc;

/// A caller's measurement of a child, in place of its own.
pub(crate) type SizeQuery<'a, H> = Rc<dyn Fn(Level, &<H as Host>::Context) -> Measured + 'a>;
/// A caller's preferred width, given the measurement so far.
pub(crate) type PreferredWidth<'a, H> =
    Rc<dyn Fn(Level, &<H as Host>::Context, Measured) -> f32 + 'a>;
/// A caller's useful width floor, given the preferred size so far.
pub(crate) type MinimumWidth<'a, H> = Rc<dyn Fn(Level, &<H as Host>::Context, Size) -> f32 + 'a>;

/// One change a builder declares to how a child is measured. A child keeps
/// these in `Child::adjustments` in the order the builders were called, and
/// `Child::measure_adjusted` applies them in that order to the measurement
/// its kind gives (`Child::base_measurement`).
///
/// Before these were data, each builder took the child's measuring closure
/// and wrapped it in another, and the list below is exactly those closures,
/// each kept as the values it captured:
///
/// - The constructors built the first closure from the kind: a leaf
///   measured itself with unbounded room (`Child::leaf`); forms asked the
///   form selected at the level (`Child::forms`); a composition summed or
///   stacked its children (`Child::row`, `Child::row_top`,
///   `Child::column`, `Child::wrap`, and the column that `fill_width` and
///   `fixed_dimensions` put a child in); a frame added its padding to its
///   content's measurement (`Child::frame`). That is now
///   `base_measurement`, which is a `match` on the kind and nothing else.
/// - `with_measurement` replaced the closure with the caller's
///   (`Replaced`). household-widgets' `spacer` and its inline and formula
///   content use it.
/// - `round_width_up` wrapped it, capturing nothing (`RoundWidthUp`).
/// - `preferred_width_with` wrapped it, capturing the caller's function;
///   `preferred_width` passes a function that returns its number
///   (`PreferredWidth`).
/// - `minimum_width_with` wrapped it, capturing the caller's function and,
///   when the child was a leaf, the leaf to measure at the floor;
///   `minimum_width` passes a function that ignores the context
///   (`MinimumWidth`). The interpreter now reads the leaf from the kind,
///   which never changes after a child is built.
/// - `fixed_dimensions` wrapped the closure of the column it puts the child
///   in, capturing the width and height (`FixedDimensions`).
/// - `ranged_leaf` (household-widgets' `scroll_child_with_minimum`) wrapped
///   the new leaf's closure, capturing the caller's floor and the leaf to
///   measure at it (`ViewportFloor`); `scroll_child` passes a function that
///   returns its size. The leaf now holds both, as its content and its
///   declared range of heights, and the interpreter reads them from it.
/// - `Child::new` received the first closure and `Child`'s `Clone` shared
///   it; they now start and copy the list.
///
/// Nothing else built, replaced or wrapped it. A typical caller declares a
/// leaf's floors, then rounding, then a preferred width, as a menu row does:
/// `child(view).minimum_width_with(..).round_width_up().preferred_width(..)`.
///
/// Why the order matters: each adjustment acts on the measurement as the
/// ones before it left it. Take a leaf that measures 5 wide, given a
/// minimum width of 7.5, then rounded up, then a preferred width of 3. The
/// minimum width makes the minimum 7.5 and raises the preferred width to
/// 7.5; rounding up makes both 8; the preferred width of 3 is below the
/// minimum it finds, so the preferred width stays 8. Rounding up first
/// would have rounded 5 and left both at 7.5: the fraction the rounding was
/// declared to remove. household-widgets' `layout_measuring_tests.rs`
/// builds every pair of adjustments whose order changes the result both
/// ways round.
pub(crate) enum Adjustment<'a, H: Host> {
    /// The caller's measurement in place of everything before it. The kind's
    /// own measurement and the adjustments declared earlier are not consulted:
    /// `with_measurement` empties the list before recording this, so a
    /// replacement is always the first entry, and the interpreter never looks
    /// behind one.
    Replaced(SizeQuery<'a, H>),
    /// Round the preferred and the minimum width up to whole pixels.
    RoundWidthUp,
    /// The caller's preferred width, given the measurement so far. Never
    /// below the minimum width; not a finite nonnegative number: invalid.
    PreferredWidth(PreferredWidth<'a, H>),
    /// The caller's useful width floor, given the preferred size so far. A
    /// leaf with no intrinsic width is measured at the floor.
    MinimumWidth(MinimumWidth<'a, H>),
    /// Exact dimensions, each raised to the minimum measured so far.
    FixedDimensions {
        width: Option<f32>,
        height: Option<f32>,
    },
    /// A leaf's floor when it takes a range of heights
    /// (`LeafHeight::Range`), measured by the caller with the same context,
    /// and the leaf to measure at that floor when it has no intrinsic size.
    /// Both are the leaf's own: this entry holds no copy, only its place in
    /// the list. `ranged_leaf` records it first on the leaf it builds, and
    /// `with_measurement` discards it with the rest of the list while the
    /// leaf still takes a range: its plan is still offered height down to
    /// its floor, but its measurement is the replacement's.
    ViewportFloor,
}

// Written by hand: the callbacks are `Rc`s, and a derived `Clone` would ask
// for `H: Clone`.
impl<H: Host> Clone for Adjustment<'_, H> {
    fn clone(&self) -> Self {
        match self {
            Self::Replaced(query) => Self::Replaced(query.clone()),
            Self::RoundWidthUp => Self::RoundWidthUp,
            Self::PreferredWidth(preferred) => Self::PreferredWidth(preferred.clone()),
            Self::MinimumWidth(minimum) => Self::MinimumWidth(minimum.clone()),
            Self::FixedDimensions { width, height } => Self::FixedDimensions {
                width: *width,
                height: *height,
            },
            Self::ViewportFloor => Self::ViewportFloor,
        }
    }
}

impl<'a, H: Host> Child<'a, H> {
    /// This child's preferred and useful minimum size at `level`: zero when
    /// it is hidden there, NaN when it is invalid, and otherwise its kind's
    /// measurement with its adjustments applied in order.
    pub fn query(&self, level: Level, context: &H::Context) -> Measured {
        if self.is_hidden(level) {
            return Measured {
                preferred: Size::ZERO,
                minimum: Size::ZERO,
            };
        }
        if self.invalid {
            // An invalid child has no valid size (`invalid_unless`).
            let invalid = Size::new(f32::NAN, f32::NAN);
            return Measured {
                preferred: invalid,
                minimum: invalid,
            };
        }
        self.measure_adjusted(&self.adjustments, level, context)
    }

    /// What the child's kind measures at a level, before any adjustment. A
    /// leaf is measured by the host with unbounded room and offers that size
    /// as both its preferred and its minimum; forms measure the form selected
    /// at the level (zero if that form is hidden); a composition combines its
    /// visible children (`Composition::measure`); a frame adds its padding to
    /// its content's measurement (`Frame::measure`).
    fn base_measurement(&self, level: Level, context: &H::Context) -> Measured {
        match &self.kind {
            Kind::Leaf(leaf) => {
                let size = H::size(
                    &leaf.content,
                    level,
                    context,
                    Offer::NeededHeight(Size::INFINITE),
                );
                Measured {
                    preferred: size,
                    minimum: size,
                }
            }
            Kind::Composition(composition) => composition.measure(level, context),
            Kind::Frame(frame) => frame.measure(level, context),
            Kind::Forms(forms) => forms.select(level).query(level, context),
        }
    }

    /// The one interpreter of a child's measuring adjustments: the
    /// measurement after `adjustments` (a prefix of `self.adjustments`) have
    /// been applied, in order, to the kind's base measurement.
    ///
    /// The measurement after a list is its last adjustment applied to the
    /// measurement after the entries before it, and with no entries it is the
    /// base measurement. It is written from the last entry back, rather than
    /// as a loop from the base forward, so that each adjustment takes the
    /// earlier measurement at the moment it needs it and not before: a
    /// replacement never takes it at all (the kind's leaf is not even
    /// measured), and a leaf that takes a range of heights asks the caller
    /// for its floor before the leaf is measured, just as the wrapped
    /// closures did. Every arm does the arithmetic and the validity checks
    /// of the builder that declared it, in the same order, so the result is
    /// the same to the bit.
    fn measure_adjusted(
        &self,
        adjustments: &[Adjustment<'a, H>],
        level: Level,
        context: &H::Context,
    ) -> Measured {
        let Some((adjustment, earlier)) = adjustments.split_last() else {
            return self.base_measurement(level, context);
        };
        let measure_earlier = || self.measure_adjusted(earlier, level, context);
        match adjustment {
            Adjustment::Replaced(query) => query(level, context),
            Adjustment::RoundWidthUp => {
                let mut measured = measure_earlier();
                measured.preferred.width = measured.preferred.width.ceil();
                measured.minimum.width = measured.minimum.width.ceil();
                measured
            }
            Adjustment::PreferredWidth(preferred) => {
                let mut measured = measure_earlier();
                let width = preferred(level, context, measured);
                measured.preferred.width = if width.is_finite() && width >= 0.0 {
                    width.max(measured.minimum.width)
                } else {
                    f32::NAN
                };
                measured
            }
            Adjustment::MinimumWidth(minimum) => {
                let mut measured = measure_earlier();
                // Only a leaf can be measured at the declared floor.
                let native = match &self.kind {
                    Kind::Leaf(leaf) => Some(&leaf.content),
                    Kind::Composition(_) | Kind::Frame(_) | Kind::Forms(_) => None,
                };
                // The leaf to measure at the floor, when it has no intrinsic
                // width.
                let recoverable = native.filter(|_| {
                    measured.preferred.width == f32::INFINITY
                        && measured.preferred.height.is_finite()
                        && measured.preferred.height >= 0.0
                        && measured.minimum.height.is_finite()
                        && measured.minimum.height >= 0.0
                });
                if (measured.preferred.width.is_finite() && measured.preferred.width >= 0.0)
                    || recoverable.is_some()
                {
                    let floor = minimum(level, context, measured.preferred);
                    if floor.is_finite() && floor >= 0.0 {
                        if let Some(native) = recoverable {
                            // Some leaves (native Fill controls) have no
                            // intrinsic width. The component's declared floor
                            // is the finite offer; never substitute an
                            // arbitrary window or probe width.
                            let bounded = H::size(
                                native,
                                level,
                                context,
                                Offer::NeededHeight(Size::new(floor, f32::INFINITY)),
                            );
                            if finite_size(bounded) && bounded.width <= floor + FIT_EPSILON {
                                measured.preferred = Size::new(floor, bounded.height);
                                measured.minimum.width = floor;
                                measured.minimum.height =
                                    measured.minimum.height.min(bounded.height);
                            } else {
                                measured.minimum.width = f32::NAN;
                            }
                        } else {
                            // A leaf (an all-Fill native subtree) can report
                            // intrinsic zero.
                            measured.minimum.width = floor;
                            measured.preferred.width = measured.preferred.width.max(floor);
                        }
                    } else {
                        measured.minimum.width = f32::NAN;
                    }
                } else {
                    measured.minimum.width = f32::NAN;
                }
                measured
            }
            Adjustment::FixedDimensions { width, height } => {
                let mut measured = measure_earlier();
                if let Some(width) = *width {
                    let width = width.max(measured.minimum.width);
                    measured.minimum.width = width;
                    measured.preferred.width = width;
                }
                if let Some(height) = *height {
                    let height = height.max(measured.minimum.height);
                    measured.minimum.height = height;
                    measured.preferred.height = height;
                }
                measured
            }
            Adjustment::ViewportFloor => {
                // The floor and the leaf are the leaf's declaration. Only
                // `ranged_leaf` records this entry, on the leaf it builds,
                // and a child's kind never changes, so the entry is always
                // on such a leaf; were it on anything else, there would be
                // no floor to apply.
                let Kind::Leaf(Leaf {
                    content,
                    height: LeafHeight::Range { minimum },
                }) = &self.kind
                else {
                    return measure_earlier();
                };
                let minimum = minimum(level, context);
                let measured = measure_earlier();
                if !finite_size(minimum) {
                    return Measured {
                        preferred: Size::new(f32::NAN, f32::NAN),
                        minimum,
                    };
                }
                let invalid = |value: f32| value.is_nan() || value < 0.0;
                if invalid(measured.preferred.width) || invalid(measured.preferred.height) {
                    return Measured {
                        preferred: measured.preferred,
                        minimum,
                    };
                }
                let mut preferred = measured.preferred;
                if preferred.width.is_infinite() || preferred.height.is_infinite() {
                    // This component explicitly declares a range of heights
                    // (a native scrolling viewport). Validate its whole
                    // leaf, including controls, at that finite minimum.
                    let bounded = H::size(content, level, context, Offer::OfferedHeight(minimum));
                    if !finite_size(bounded)
                        || bounded.width > minimum.width + FIT_EPSILON
                        || bounded.height > minimum.height + FIT_EPSILON
                    {
                        return Measured {
                            preferred: Size::new(f32::NAN, f32::NAN),
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
                Measured {
                    preferred: Size::new(
                        preferred.width.max(minimum.width),
                        preferred.height.max(minimum.height),
                    ),
                    minimum,
                }
            }
        }
    }

    /// This child's capacity for width, from its measurement at a level: its
    /// preferred and minimum widths, and its weight.
    pub(crate) fn width_capacity(&self, measured: Measured) -> WidthCapacity {
        WidthCapacity {
            preferred: measured.preferred.width,
            minimum: measured.minimum.width,
            weight: self.sharing.weight,
        }
    }
}
