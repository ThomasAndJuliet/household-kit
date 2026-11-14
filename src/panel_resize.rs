// SPDX-License-Identifier: GPL-3.0-only

//! Dimension requests for a shared pane or panel divider.
//!
//! This is interaction state, not layout: callers provide accepted dimensions
//! and measured minima from the existing engine. Only the two participants in
//! the boundary are returned. The same scalar operations apply on either axis.

/// Cerea's barrier constants, from `src/barrier.rs` in Cerea: its pressure
/// threshold, decay time and maximum give, in logical pixels and seconds.
const BREAK_PRESSURE: f32 = 120.0;
const PRESSURE_LEAK: f32 = 0.35;
const GIVE: f32 = 12.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Dimension {
    #[default]
    Automatic,
    Manual(f32),
}

/// The requests belonging to the two neighbours, in divider order.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Requests {
    pub before: Dimension,
    pub after: Dimension,
}

/// Accepted dimensions and measured minima, excluding the divider itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Boundary {
    pub before: f32,
    pub after: f32,
    pub before_min: f32,
    pub after_min: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeError {
    InvalidGeometry,
    NoFit,
}

impl Boundary {
    fn limits(self) -> Result<(f32, f32), ResizeError> {
        let values = [self.before, self.after, self.before_min, self.after_min];
        if values.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return Err(ResizeError::InvalidGeometry);
        }
        let total = self.before + self.after;
        if !total.is_finite() {
            return Err(ResizeError::InvalidGeometry);
        }
        if self.before_min + self.after_min > total
            || self.before < self.before_min
            || self.after < self.after_min
        {
            return Err(ResizeError::NoFit);
        }
        Ok((self.before_min - self.before, self.after - self.after_min))
    }
}

/// A drag snapshot. The caller owns the requests and commits the result on
/// release; dropping or cancelling this value restores the original requests.
#[derive(Clone, Debug)]
pub struct Drag {
    original: Requests,
    boundary: Boundary,
    min_delta: f32,
    max_delta: f32,
    pointer: f32,
    pressure: f32,
    direction: f32,
    manual: bool,
    moved: bool,
    applied: f32,
}

/// A transient preview and the requests to commit on release. Before a
/// breakthrough the resisted preview does not turn automatic sizing manual.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragUpdate {
    pub before: f32,
    pub after: f32,
    pub requests: Requests,
    pub manual: bool,
}

impl Drag {
    pub fn begin(original: Requests, boundary: Boundary) -> Result<Self, ResizeError> {
        let (min_delta, max_delta) = boundary.limits()?;
        Ok(Self {
            original,
            boundary,
            min_delta,
            max_delta,
            pointer: 0.0,
            pressure: 0.0,
            direction: 0.0,
            manual: matches!(original.before, Dimension::Manual(_))
                || matches!(original.after, Dimension::Manual(_)),
            moved: false,
            applied: 0.0,
        })
    }

    /// Add pointer movement and elapsed seconds. Negative movement reduces the
    /// first neighbour. Nonfinite input is ignored. Reversing a pending tug
    /// releases its pressure before building pressure in the other direction.
    pub fn step(&mut self, delta: f32, elapsed: f32) -> DragUpdate {
        if !delta.is_finite() || !elapsed.is_finite() || elapsed < 0.0 {
            return self.current();
        }
        self.pressure *= (-elapsed / PRESSURE_LEAK).exp();
        if self.manual {
            self.pointer += delta;
            self.applied = self.pointer.clamp(self.min_delta, self.max_delta);
            self.moved |= self.applied != 0.0;
        } else if delta != 0.0 {
            let sign = delta.signum();
            if sign != self.direction {
                self.pressure = 0.0;
                self.direction = sign;
            }
            self.pressure += delta.abs();
            let give = GIVE * (1.0 - (-self.pressure / BREAK_PRESSURE).exp());
            if self.pressure >= BREAK_PRESSURE {
                self.manual = true;
                self.pointer = sign * (give + self.pressure - BREAK_PRESSURE);
                self.applied = self.pointer.clamp(self.min_delta, self.max_delta);
                self.moved = self.applied != 0.0;
            } else {
                self.applied = (sign * give).clamp(self.min_delta, self.max_delta);
            }
        } else {
            self.applied =
                (self.direction * GIVE * (1.0 - (-self.pressure / BREAK_PRESSURE).exp()))
                    .clamp(self.min_delta, self.max_delta);
        }
        self.current()
    }

    pub fn current(&self) -> DragUpdate {
        let before = self.boundary.before + self.applied;
        let after = self.boundary.after - self.applied;
        let requests = if self.manual && self.moved {
            Requests {
                before: Dimension::Manual(before),
                after: Dimension::Manual(after),
            }
        } else {
            self.original
        };
        DragUpdate {
            before,
            after,
            requests,
            manual: self.manual,
        }
    }

    pub fn cancel(self) -> Requests {
        self.original
    }

    pub fn finish(self) -> Requests {
        self.current().requests
    }
}

/// The constraint supplied to ordinary layout. A manual request that cannot
/// fit asks the existing engine for a declared collapsed form; it is never
/// silently reduced. The caller still owns the saved request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ordinary {
    Automatic,
    Expanded(f32),
    Collapse,
    InvalidGeometry,
}

pub fn ordinary(request: Dimension, minimum: f32, available: f32) -> Ordinary {
    if [minimum, available]
        .iter()
        .any(|x| !x.is_finite() || *x < 0.0)
    {
        return Ordinary::InvalidGeometry;
    }
    match request {
        Dimension::Automatic => Ordinary::Automatic,
        Dimension::Manual(value) if !value.is_finite() || value < 0.0 => Ordinary::InvalidGeometry,
        Dimension::Manual(value) if value.max(minimum) > available => Ordinary::Collapse,
        Dimension::Manual(value) => Ordinary::Expanded(value.max(minimum)),
    }
}

/// Resolve an explicitly reopened manual dimension without changing its saved
/// request. Ordinary layout must use its discrete collapsed form on no fit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reopen {
    Expanded(f32),
    NoFit,
    InvalidGeometry,
}

pub fn reopen(requested: f32, minimum: f32, available: f32) -> Reopen {
    if [requested, minimum, available]
        .iter()
        .any(|x| !x.is_finite() || *x < 0.0)
    {
        return Reopen::InvalidGeometry;
    }
    if minimum > available {
        return Reopen::NoFit;
    }
    Reopen::Expanded(requested.max(minimum).min(available))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn boundary() -> Boundary {
        Boundary {
            before: 200.0,
            after: 200.0,
            before_min: 80.0,
            after_min: 100.0,
        }
    }
    #[test]
    fn automatic_tug_resists_then_commits_dimensions() {
        let mut d = Drag::begin(Requests::default(), boundary()).unwrap();
        let u = d.step(20.0, 0.0);
        assert!(!u.manual && u.before > 200.0 && u.before < 212.0);
        assert_eq!(u.requests, Requests::default());
        let u = d.step(100.0, 0.0);
        assert!(u.manual);
        assert_eq!(
            u.requests,
            Requests {
                before: Dimension::Manual(u.before),
                after: Dimension::Manual(u.after)
            }
        );
        assert_eq!(d.finish(), u.requests);
    }
    #[test]
    fn minima_clamp_both_directions_and_no_fit_is_reported() {
        let manual = Requests {
            before: Dimension::Manual(200.0),
            after: Dimension::Manual(200.0),
        };
        let mut d = Drag::begin(manual, boundary()).unwrap();
        assert_eq!(d.step(1000.0, 0.0).after, 100.0);
        assert_eq!(d.step(-2000.0, 0.0).before, 80.0);
        assert_eq!(
            Drag::begin(
                manual,
                Boundary {
                    before_min: 250.0,
                    after_min: 200.0,
                    ..boundary()
                }
            )
            .unwrap_err(),
            ResizeError::NoFit
        );
    }
    #[test]
    fn reversal_cancellation_and_decay() {
        let initial = Requests::default();
        let mut d = Drag::begin(initial, boundary()).unwrap();
        d.step(100.0, 0.0);
        assert!(!d.step(-30.0, 0.0).manual);
        assert!(d.current().before < 200.0);
        let previous = d.current().before;
        assert!(d.step(0.0, 0.35).before > previous);
        assert_eq!(d.cancel(), initial);
    }
    #[test]
    fn mirrored_axis_is_symmetric_and_reopen_keeps_request() {
        let mut a = Drag::begin(Requests::default(), boundary()).unwrap();
        let mut b = Drag::begin(
            Requests::default(),
            Boundary {
                before: 200.0,
                after: 200.0,
                before_min: 100.0,
                after_min: 80.0,
            },
        )
        .unwrap();
        let x = a.step(150.0, 0.0);
        let y = b.step(-150.0, 0.0);
        assert_eq!((x.before, x.after), (y.after, y.before));
        let saved = 250.0;
        assert_eq!(
            ordinary(Dimension::Manual(saved), 80.0, 150.0),
            Ordinary::Collapse
        );
        assert_eq!(
            ordinary(Dimension::Manual(saved), 80.0, 300.0),
            Ordinary::Expanded(saved)
        );
        assert_eq!(reopen(saved, 80.0, 150.0), Reopen::Expanded(150.0));
        assert_eq!(reopen(saved, 80.0, 300.0), Reopen::Expanded(saved));
        assert_eq!(reopen(saved, 80.0, 70.0), Reopen::NoFit);
    }
}
