// SPDX-License-Identifier: GPL-3.0-only

//! What a parent orders of a child.
//!
//! "There are multiple traits so prominent is also something that can be
//! ordered." An order has independent kinds: the compaction level,
//! prominence, detail, and kinds a family of boxes adds (a chart's
//! direction is the first; this crate does not know what a chart is).
//!
//! Each kind resolves on its own, as household-widgets'
//! `PresentationFamily::resolve` does: the maker's request for that kind,
//! then the parent's order for that child, then the family's default. A
//! box says which combinations it accepts. An order it does not accept is
//! an explicit [`Unsupported`] error: it never silently selects another
//! form, and it is not the same as not fitting. Orders resolve before
//! anything is measured, and the same resolution is drawn.

use std::collections::BTreeMap;
use std::fmt;

use crate::identity::Id;
use crate::level::Level;

/// The apps' emphasis; in charts, primary or secondary.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Prominence {
    #[default]
    Normal,
    Emphasised,
}

/// The apps' concise or verbose.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Detail {
    #[default]
    Concise,
    Verbose,
}

/// Every kind but the level, resolved: the presentation a box is drawn
/// in. Kinds a family adds are named by that family, each with one value
/// (for example `direction` = `turned`); the crate never interprets them.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct Presentation {
    pub prominence: Prominence,
    pub detail: Detail,
    pub added: BTreeMap<String, String>,
}

impl Presentation {
    pub fn new(prominence: Prominence, detail: Detail) -> Self {
        Self {
            prominence,
            detail,
            added: BTreeMap::new(),
        }
    }

    /// This presentation with a family's kind set to `value`.
    pub fn with(mut self, kind: impl Into<String>, value: impl Into<String>) -> Self {
        self.added.insert(kind.into(), value.into());
        self
    }
}

/// A request for some kinds and not others: what a box's maker asks of
/// it, or what a parent orders of one child. A kind left `None` (or a
/// family kind left out) falls through to the next source.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Order {
    pub level: Option<Level>,
    pub prominence: Option<Prominence>,
    pub detail: Option<Detail>,
    pub added: BTreeMap<String, String>,
}

impl Order {
    /// An order that asks for nothing.
    pub const NONE: Self = Self {
        level: None,
        prominence: None,
        detail: None,
        added: BTreeMap::new(),
    };

    pub fn with_level(mut self, level: Level) -> Self {
        self.level = Some(level);
        self
    }

    pub fn with_prominence(mut self, prominence: Prominence) -> Self {
        self.prominence = Some(prominence);
        self
    }

    pub fn with_detail(mut self, detail: Detail) -> Self {
        self.detail = Some(detail);
        self
    }

    /// This order with a family's kind set to `value`.
    pub fn with(mut self, kind: impl Into<String>, value: impl Into<String>) -> Self {
        self.added.insert(kind.into(), value.into());
        self
    }
}

impl From<&Presentation> for Order {
    fn from(presentation: &Presentation) -> Self {
        Self {
            level: None,
            prominence: Some(presentation.prominence),
            detail: Some(presentation.detail),
            added: presentation.added.clone(),
        }
    }
}

/// Every kind resolved: what a box is measured and drawn under. The level
/// is a ceiling: the most compromise its parent permits.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct Resolved {
    pub level: Level,
    pub presentation: Presentation,
}

impl From<&Resolved> for Order {
    /// The order that passes every kind of `resolved` on, as a list does
    /// to each member.
    fn from(resolved: &Resolved) -> Self {
        Order::from(&resolved.presentation).with_level(resolved.level)
    }
}

/// An order a box does not accept. It is an error of the order, not a
/// failure to fit, and no other form is chosen in its place: the caller
/// declares any alternative.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unsupported {
    /// The box that was ordered.
    pub part: Id,
    /// The presentation the order resolved to.
    pub requested: Presentation,
}

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} does not accept {:?}, {:?}",
            self.part.as_str(),
            self.requested.prominence,
            self.requested.detail
        )?;
        for (kind, value) in &self.requested.added {
            write!(f, ", {kind} = {value}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Unsupported {}

/// Which presentations a box accepts, as an explicit list that includes
/// its family's default. Every box accepts every level: a box that has no
/// form of its own at a level keeps the form below it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Accepts {
    default: Presentation,
    supported: Vec<Presentation>,
}

impl Accepts {
    /// # Panics
    ///
    /// If `default` is not among `supported`.
    pub fn new(default: Presentation, supported: Vec<Presentation>) -> Self {
        assert!(
            supported.contains(&default),
            "a family's default must be accepted"
        );
        Self { default, supported }
    }

    /// Only `presentation`, which is also the default.
    pub fn only(presentation: Presentation) -> Self {
        Self::new(presentation.clone(), vec![presentation])
    }

    /// Only the ordinary presentation: normal and concise, with no kind a
    /// family adds.
    pub fn ordinary() -> Self {
        Self::only(Presentation::default())
    }

    pub fn default_presentation(&self) -> &Presentation {
        &self.default
    }

    pub fn supported(&self) -> &[Presentation] {
        &self.supported
    }

    /// Resolve each kind separately: the maker's request, then the
    /// parent's order, then the family's default (for the level,
    /// [`Level::Preferred`]). A presentation not in the list is
    /// [`Unsupported`], naming `part`.
    pub fn resolve(
        &self,
        part: &Id,
        maker: &Order,
        parent: &Order,
    ) -> Result<Resolved, Unsupported> {
        let mut added = self.default.added.clone();
        for (kind, value) in parent.added.iter().chain(&maker.added) {
            added.insert(kind.clone(), value.clone());
        }
        let requested = Presentation {
            prominence: maker
                .prominence
                .or(parent.prominence)
                .unwrap_or(self.default.prominence),
            detail: maker
                .detail
                .or(parent.detail)
                .unwrap_or(self.default.detail),
            added,
        };
        if !self.supported.contains(&requested) {
            return Err(Unsupported {
                part: part.clone(),
                requested,
            });
        }
        Ok(Resolved {
            level: maker.level.or(parent.level).unwrap_or_default(),
            presentation: requested,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_pair() -> Accepts {
        let default = Presentation::default();
        Accepts::new(
            default.clone(),
            vec![
                default,
                Presentation::new(Prominence::Emphasised, Detail::Concise),
                Presentation::new(Prominence::Normal, Detail::Verbose),
                Presentation::new(Prominence::Emphasised, Detail::Verbose),
            ],
        )
    }

    fn part() -> Id {
        Id::new("part")
    }

    #[test]
    fn kinds_resolve_maker_then_parent_then_family() {
        let accepts = every_pair();
        let parent = Order::NONE.with_prominence(Prominence::Emphasised);
        let maker = Order::NONE
            .with_prominence(Prominence::Normal)
            .with_detail(Detail::Verbose);
        assert_eq!(
            accepts
                .resolve(&part(), &maker, &parent)
                .unwrap()
                .presentation,
            Presentation::new(Prominence::Normal, Detail::Verbose)
        );
        assert_eq!(
            accepts
                .resolve(&part(), &Order::NONE, &parent)
                .unwrap()
                .presentation,
            Presentation::new(Prominence::Emphasised, Detail::Concise)
        );
        assert_eq!(
            accepts
                .resolve(&part(), &Order::NONE, &Order::NONE)
                .unwrap(),
            Resolved::default()
        );
        // A family can have a different ordinary presentation in either
        // kind, and an explicit request overrides it.
        let verbose = Presentation::new(Prominence::Normal, Detail::Verbose);
        let accepts = Accepts::new(verbose.clone(), vec![verbose, Presentation::default()]);
        assert_eq!(
            accepts
                .resolve(
                    &part(),
                    &Order::NONE.with_detail(Detail::Concise),
                    &Order::NONE
                )
                .unwrap()
                .presentation,
            Presentation::default()
        );
    }

    #[test]
    fn the_level_resolves_like_every_other_kind() {
        let accepts = Accepts::ordinary();
        let parent = Order::NONE.with_level(Level::Strong);
        assert_eq!(
            accepts
                .resolve(&part(), &Order::NONE, &parent)
                .unwrap()
                .level,
            Level::Strong
        );
        let pinned = Order::NONE.with_level(Level::Preferred);
        assert_eq!(
            accepts.resolve(&part(), &pinned, &parent).unwrap().level,
            Level::Preferred
        );
    }

    #[test]
    fn a_kind_a_family_adds_resolves_on_its_own() {
        let upright = Presentation::default().with("direction", "upright");
        let turned = Presentation::default().with("direction", "turned");
        let accepts = Accepts::new(upright.clone(), vec![upright.clone(), turned.clone()]);
        assert_eq!(
            accepts
                .resolve(&part(), &Order::NONE, &Order::NONE)
                .unwrap()
                .presentation,
            upright
        );
        let group = Order::NONE.with("direction", "turned");
        assert_eq!(
            accepts
                .resolve(&part(), &Order::NONE, &group)
                .unwrap()
                .presentation,
            turned
        );
        let maker = Order::NONE.with("direction", "upright");
        assert_eq!(
            accepts
                .resolve(&part(), &maker, &group)
                .unwrap()
                .presentation,
            upright
        );
        // A box that does not know the kind refuses it by name.
        let refused = Accepts::ordinary().resolve(&part(), &Order::NONE, &group);
        assert_eq!(
            refused,
            Err(Unsupported {
                part: part(),
                requested: Presentation::default().with("direction", "turned"),
            })
        );
    }

    #[test]
    fn an_unsupported_order_is_explicit_and_names_the_part() {
        let accepts = Accepts::ordinary();
        let requested = Presentation::new(Prominence::Emphasised, Detail::Verbose);
        let result = accepts.resolve(&part(), &Order::from(&requested), &Order::NONE);
        assert_eq!(
            result,
            Err(Unsupported {
                part: part(),
                requested
            })
        );
        assert!(result.unwrap_err().to_string().contains("part"));
    }
}
