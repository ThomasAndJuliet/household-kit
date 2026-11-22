// SPDX-License-Identifier: GPL-3.0-only

//! Stages: a short list of (level, part) pairs a host declares in order,
//! and the first of them that fits.
//!
//! A host does not ask for one plan at one level. It declares what it
//! would rather show and what it can live with: the same part at
//! successive levels, or different parts (a toolbar, then the toolbar with
//! its labels hidden, then a menu button), each with the level it is
//! planned at. The engine plans them in the declared order and keeps the
//! first that fits ([`choose_stage`]); a stage that does not fit is
//! dropped with nothing kept of it. Nothing is promoted to a level the host
//! did not declare, and when no stage fits the answer says so ([`NoFit`]),
//! so the host shows its own fallback rather than something clipped. A
//! host that tells its reader why asks [`choose_stage_blaming`] instead,
//! which chooses the same stage and, when none fits, gives what each stage
//! lacked and the part it lacked it in.
//!
//! A popup or a hover card chooses among the same stages, together with
//! where it is attached ([`crate::attach`], [`crate::hover`]).
//!
//! How a list of stages reaches the engine. A host keeps its stages in its
//! own type, with its own handle on each part (household-widgets'
//! `Stage { level, content }` holds the widgets' `Child`, a public struct
//! the apps build). The engine reads a stage through [`AsStage`]: its level
//! and a reference to its part. So it chooses over the host's own slice of
//! stages, with no copy of a part and no list built for the purpose, and it
//! can hand a stage of that slice back to the host as the host's own (a
//! hover card commits each region it tries through the host:
//! [`crate::hover::place_hover`]). [`Stage`] is the engine's own stage,
//! for a host with nothing more to keep on one.

use crate::{Child, Failure, Host, Level, Plan, Size};

/// No declared stage supplied complete fitting geometry. Nothing is implicitly
/// hidden, clipped, scrolled or promoted to a stronger undeclared level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoFit {
    pub attempted: usize,
}

/// A stage as the engine reads it: the level it is planned at and its part.
///
/// The host is a parameter of the trait, so that a host may implement it
/// for its own public stage with a host type it keeps private: an impl
/// whose header names a private type is private itself. Every function
/// over stages infers the host from the one impl the stage has.
pub trait AsStage<'a, H: Host> {
    /// The one level the stage's part is planned at.
    fn level(&self) -> Level;

    /// The stage's part.
    fn part(&self) -> &Child<'a, H>;
}

/// One exact level and part in a host's ordered list of stages.
pub struct Stage<'a, H: Host> {
    pub level: Level,
    pub part: Child<'a, H>,
}

impl<'a, H: Host> Stage<'a, H> {
    pub fn new(level: Level, part: Child<'a, H>) -> Self {
        Self { level, part }
    }
}

impl<'a, H: Host> AsStage<'a, H> for Stage<'a, H> {
    fn level(&self) -> Level {
        self.level
    }

    fn part(&self) -> &Child<'a, H> {
        &self.part
    }
}

/// The stage chosen: its index in the declared list, its level, and its
/// part's plan there.
#[derive(Clone, Debug, PartialEq)]
pub struct StagePlan {
    pub stage_index: usize,
    pub level: Level,
    pub plan: Plan,
}

/// The first stage, in the declared order, whose part fits in `available`
/// at its level (`Child::query_at`), with its plan; `NoFit` with the number
/// of stages tried when none does.
pub fn choose_stage<'a, H: Host, S: AsStage<'a, H>>(
    stages: &[S],
    context: &H::Context,
    available: Size,
) -> Result<StagePlan, NoFit> {
    for (stage_index, stage) in stages.iter().enumerate() {
        if let Ok(plan) = stage.part().query_at(stage.level(), context, available) {
            return Ok(StagePlan {
                stage_index,
                level: stage.level(),
                plan,
            });
        }
    }
    Err(NoFit {
        attempted: stages.len(),
    })
}

/// The first stage that fits, as `choose_stage` chooses it; and when none
/// does, what each stage lacked: one [`Failure`] per stage tried, in the
/// declared order (`Child::query_at_blaming`), whose path starts at that
/// stage's part.
///
/// The stages are planned exactly as `choose_stage` plans them, so the
/// plan chosen is the same, and so is everything asked of the host on the
/// way. The difference is only what is kept of a stage that does not fit:
/// `choose_stage` drops its failure and counts it (`NoFit`); this keeps
/// it, so a host can say which part of each stage could not be fitted
/// (the charts do, in the words they give their reader). The failures are
/// collected as the stages fail, so a stage that fits after some that did
/// not has cost their failures too.
pub fn choose_stage_blaming<'a, H: Host, S: AsStage<'a, H>>(
    stages: &[S],
    context: &H::Context,
    available: Size,
) -> Result<StagePlan, Vec<Failure>> {
    let mut failures = Vec::new();
    for (stage_index, stage) in stages.iter().enumerate() {
        match stage
            .part()
            .query_at_blaming(stage.level(), context, available)
        {
            Ok(plan) => {
                return Ok(StagePlan {
                    stage_index,
                    level: stage.level(),
                    plan,
                });
            }
            Err(failure) => failures.push(failure),
        }
    }
    Err(failures)
}
