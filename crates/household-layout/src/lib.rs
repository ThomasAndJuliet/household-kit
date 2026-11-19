// SPDX-License-Identifier: GPL-3.0-only

//! Arrangement for the household's apps, charts and diagrams.
//!
//! This crate holds only arrangement: boxes that a parent queries and
//! orders, compaction levels, the sharing of a shortage, anchors with
//! ordered places, and a record of occupied space. It draws nothing and
//! measures nothing, and it names no type of iced, libcosmic or figkit:
//! a leaf answers with numbers, and its user supplies the leaves.
//!
//! The design is Juliet's, agreed 19 November 2026 (`layout-design.md`
//! in the design conversation). Charts and diagrams use the crate first;
//! household-widgets' layout moves onto it later.
//!
//! - [`geometry`]: points, sizes and rectangles, in `f64`.
//! - [`identity`]: a box's stable identity and the input field it draws.
//! - [`level`]: the four compaction levels.
//! - [`kinds`]: the kinds of thing that occupy space.
//! - [`rules`]: the distance table and the covering table, as data.
//! - [`record`]: the record of occupied space, and what it answers.
//! - [`labels`]: anchors, ordered places, and sets placed by pattern.
//! - [`engine`]: the apps' layout engine (household-widgets' `layout.rs`),
//!   ported as it is, with leaves the caller measures in place of widgets.
//!
//! Juliet's decision of 19 November 2026 is to work within that engine
//! before assuming an extension: [`engine`] is the same engine, with the
//! same names, so that a chart lays out as a window does. It adds only an
//! identity on children and plan nodes, what failed at each stage when
//! nothing fits, and a child's report of what its form does not show.

pub mod engine;
pub mod geometry;
pub mod identity;
pub mod kinds;
pub mod labels;
pub mod level;
pub mod presentation;
pub mod record;
pub mod rules;
