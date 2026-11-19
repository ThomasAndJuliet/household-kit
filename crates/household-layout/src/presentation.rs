// SPDX-License-Identifier: GPL-3.0-only

//! Presentations: how a component is shown, kept apart from compaction.
//!
//! A component has two axes of presentation besides its compaction level:
//! its [`Emphasis`] (normal or emphasised) and its [`Detail`] (concise or
//! verbose). They say what the component is asked to be, not how much room
//! it has, so they never change with the level the engine chooses: an
//! emphasised button is emphasised at `Preferred` and at `Strong` alike.
//! The pair is resolved once, before anything is measured, and the
//! component is then built for that pair.
//!
//! Each axis is resolved on its own, from three places in order: the
//! instance's own request, then its immediate parent's default for its
//! children, then the family's default. So an instance that asks only for
//! verbose detail, inside a parent whose children are emphasised by default,
//! is emphasised and verbose. An explicit `Normal` or `Concise` on the
//! instance overrides an emphasised or verbose default. A parent's default
//! reaches its immediate children only: nothing cascades further down.
//!
//! A [`PresentationFamily`] lists the pairs a kind of component supports,
//! its default among them. When the resolved pair is not supported the
//! answer is an explicit [`UnsupportedPresentation`] naming the pair; the
//! family never silently chooses another. Falling back is the caller's
//! decision.
//!
//! For example, a family whose default is normal and verbose and which
//! supports that pair and normal-concise resolves a request for concise
//! detail alone to normal-concise: the emphasis comes from the family's
//! default. A request for emphasis is unsupported, and says so. (Checked
//! by the test `axes_resolve_instance_then_immediate_then_family`.)
//!
//! This is household-widgets' `layout_presentation.rs` translated item by
//! item with the same names, without `prepare`, which builds a widget;
//! household-widgets' test `layout_port_equality` requires both to resolve
//! every request alike.

/// How strongly a component is shown.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Emphasis {
    #[default]
    Normal,
    Emphasised,
}

/// How much a component says.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Detail {
    #[default]
    Concise,
    Verbose,
}

/// One resolved pair. Presentation is independent of the compaction level.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Presentation {
    pub emphasis: Emphasis,
    pub detail: Detail,
}

/// A request for a presentation, axis by axis: `None` leaves the axis to
/// the immediate parent's default and then the family's.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PresentationRequest {
    pub emphasis: Option<Emphasis>,
    pub detail: Option<Detail>,
}

impl From<Presentation> for PresentationRequest {
    fn from(value: Presentation) -> Self {
        Self {
            emphasis: Some(value.emphasis),
            detail: Some(value.detail),
        }
    }
}

/// A resolved pair the family does not support.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnsupportedPresentation {
    pub requested: Presentation,
}

/// The pairs a kind of component supports, including its default. A
/// legacy component may declare only the presentation it already has.
#[derive(Clone, Debug)]
pub struct PresentationFamily {
    default: Presentation,
    supported: Vec<Presentation>,
}

impl PresentationFamily {
    /// A family with `default`, which must be among `supported`.
    ///
    /// # Panics
    ///
    /// When `default` is not supported.
    pub fn new(default: Presentation, supported: Vec<Presentation>) -> Self {
        assert!(
            supported.contains(&default),
            "family default must be supported"
        );
        Self { default, supported }
    }

    pub fn default_presentation(&self) -> Presentation {
        self.default
    }

    pub fn supported(&self) -> &[Presentation] {
        &self.supported
    }

    /// Resolves each axis separately, from `instance`, then
    /// `immediate_defaults`, then the family's default; the pair must be
    /// supported, or the answer is [`UnsupportedPresentation`].
    pub fn resolve(
        &self,
        instance: PresentationRequest,
        immediate_defaults: PresentationRequest,
    ) -> Result<Presentation, UnsupportedPresentation> {
        let requested = Presentation {
            emphasis: instance
                .emphasis
                .or(immediate_defaults.emphasis)
                .unwrap_or(self.default.emphasis),
            detail: instance
                .detail
                .or(immediate_defaults.detail)
                .unwrap_or(self.default.detail),
        };
        if self.supported.contains(&requested) {
            Ok(requested)
        } else {
            Err(UnsupportedPresentation { requested })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn pair(emphasis: Emphasis, detail: Detail) -> Presentation {
        Presentation { emphasis, detail }
    }

    const NORMAL_CONCISE: Presentation = pair(Emphasis::Normal, Detail::Concise);
    const NORMAL_VERBOSE: Presentation = pair(Emphasis::Normal, Detail::Verbose);
    const EMPHASISED_CONCISE: Presentation = pair(Emphasis::Emphasised, Detail::Concise);
    const EMPHASISED_VERBOSE: Presentation = pair(Emphasis::Emphasised, Detail::Verbose);

    fn all_presentations() -> PresentationFamily {
        PresentationFamily::new(
            NORMAL_CONCISE,
            vec![
                NORMAL_CONCISE,
                EMPHASISED_CONCISE,
                NORMAL_VERBOSE,
                EMPHASISED_VERBOSE,
            ],
        )
    }

    #[test]
    fn axes_resolve_instance_then_immediate_then_family() {
        let family = all_presentations();
        let contextual = PresentationRequest {
            emphasis: Some(Emphasis::Emphasised),
            detail: None,
        };
        let instance = PresentationRequest {
            emphasis: Some(Emphasis::Normal),
            detail: Some(Detail::Verbose),
        };
        // The instance wins on both axes; an explicit Normal overrides an
        // emphasised default.
        assert_eq!(family.resolve(instance, contextual), Ok(NORMAL_VERBOSE));
        // Emphasis from the parent, detail from the family.
        assert_eq!(
            family.resolve(PresentationRequest::default(), contextual),
            Ok(EMPHASISED_CONCISE)
        );
        assert_eq!(
            family.resolve(Default::default(), Default::default()),
            Ok(NORMAL_CONCISE)
        );
        // The module's example: a family whose default is verbose.
        let family = PresentationFamily::new(NORMAL_VERBOSE, vec![NORMAL_VERBOSE, NORMAL_CONCISE]);
        let concise = PresentationRequest {
            detail: Some(Detail::Concise),
            ..Default::default()
        };
        assert_eq!(
            family.resolve(concise, Default::default()),
            Ok(NORMAL_CONCISE)
        );
        let emphasised = PresentationRequest {
            emphasis: Some(Emphasis::Emphasised),
            ..Default::default()
        };
        assert_eq!(
            family.resolve(emphasised, Default::default()),
            Err(UnsupportedPresentation {
                requested: EMPHASISED_VERBOSE
            })
        );
    }

    #[test]
    fn unsupported_is_explicit_and_never_another_pair() {
        let family = PresentationFamily::new(NORMAL_CONCISE, vec![NORMAL_CONCISE]);
        assert_eq!(
            family.resolve(EMPHASISED_VERBOSE.into(), Default::default()),
            Err(UnsupportedPresentation {
                requested: EMPHASISED_VERBOSE
            })
        );
        // A parent's default that the family does not support is reported
        // in the same way, not dropped for the family's default.
        assert_eq!(
            family.resolve(Default::default(), EMPHASISED_CONCISE.into()),
            Err(UnsupportedPresentation {
                requested: EMPHASISED_CONCISE
            })
        );
        assert_eq!(family.default_presentation(), NORMAL_CONCISE);
        assert_eq!(family.supported(), [NORMAL_CONCISE]);
    }

    #[test]
    #[should_panic(expected = "family default must be supported")]
    fn a_family_default_must_be_supported() {
        PresentationFamily::new(NORMAL_VERBOSE, vec![NORMAL_CONCISE]);
    }
}
