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
//! - [`order`]: what a parent orders of a child: a level, prominence,
//!   detail, and kinds a family adds.
//! - [`boxes`]: the box a parent sees: query, order, and the kept plan.
//! - [`allocate`]: sharing a shortage between children.
//! - [`compose`]: rows, columns and lists of boxes.
//! - [`kinds`]: the kinds of thing that occupy space.
//! - [`rules`]: the distance table and the covering table, as data.
//! - [`record`]: the record of occupied space, and what it answers.
//! - [`labels`]: anchors, ordered places, and sets placed by pattern.

pub mod allocate;
pub mod boxes;
pub mod compose;
pub mod geometry;
pub mod identity;
pub mod kinds;
pub mod labels;
pub mod level;
pub mod order;
pub mod record;
pub mod rules;
