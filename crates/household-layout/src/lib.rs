// SPDX-License-Identifier: GPL-3.0-only

//! Arrangement for the household's apps, charts and diagrams.
//!
//! This crate decides where things go. It draws nothing and measures
//! nothing, and it names no type of iced, libcosmic or figkit: the caller
//! measures its own content and answers with numbers, in `f64`.
//!
//! It has two parts, used one after the other.
//!
//! **The engine** ([`engine`]) arranges boxes. A window or a chart is a
//! tree of boxes in rows, columns and wraps, and each component lists, in
//! order, the arrangements it accepts as room runs short: a row with full
//! labels, then a row of icons, then a wrap, for example. The engine takes
//! the first that fits the room given and says where every box goes, or,
//! when none fits, what failed at each. It is the household apps' own
//! layout engine, ported without the toolkit and with the same names, so a
//! chart lays out as a window does; household-widgets' test
//! `layout_port_equality` holds the two to the same answers. The port adds
//! only an identity on boxes and plan nodes, what failed at each stage
//! when nothing fits, and a box's report of what its form does not show.
//! How a component is presented (its emphasis and detail) is resolved
//! before it is laid out and never changes with the room
//! ([`presentation`]). The long comment at the top of [`engine`] explains
//! how the engine is meant to work, with worked examples.
//!
//! **The record and the labels** ([`record`], [`labels`]) place the
//! things that must not collide inside a box once its size is known: a
//! chart's value labels, end labels and notes. The record keeps what has
//! been drawn, with its kind and its owner, and judges a candidate place
//! against it by two tables of data ([`rules`]): what may stand on what,
//! and how far apart things must stand. Label sets are placed by pattern:
//! one place for every member, tried in the caller's order, the first
//! pattern that every member can take winning. A set can say before
//! anything is placed how much room its pattern needs, so that the chart
//! can reserve it when it lays out.
//!
//! A caller uses them together in three steps:
//!
//! 1. Lay out with the engine: build the tree of boxes from leaves it
//!    measures, with the room each label pattern needs counted in, and
//!    take the first stage that fits ([`engine::choose_stage`]).
//! 2. Draw into the boxes the plan gives, and enter what was drawn in a
//!    [`record::Record`]: bars, lines, gridlines, each with its identity
//!    and owner.
//! 3. Place each label set against the record ([`labels::place_set`]),
//!    most important first. Each placed set enters the record, so later
//!    sets are judged against it; nothing placed is moved. A set with no
//!    pattern left says which member stopped each pattern and why, and the
//!    chart takes its next stage or form.
//!
//! Rules and numbers are data: the distance table, the covering table and
//! the order of drawing are plain values whose [`Default`] holds the agreed
//! numbers, and nowhere else does. The design is Juliet's
//! (`layout-design.md` in the design conversation of 19 November 2026);
//! her decision of that day is to work within the apps' engine before
//! assuming an extension.
//!
//! - [`engine`]: the apps' layout engine, with leaves the caller measures
//!   in place of widgets.
//! - [`presentation`]: emphasis and detail, resolved apart from compaction.
//! - [`level`]: the four compaction levels.
//! - [`geometry`]: points, sizes and rectangles.
//! - [`identity`]: a box's stable identity and the input field it draws.
//! - [`kinds`]: the kinds of thing that occupy space, and roles.
//! - [`rules`]: the distance table, the covering table and the order of
//!   drawing.
//! - [`record`]: the record of occupied space, and what it answers.
//! - [`halo`]: where a line is cut round a text standing in it, what a
//!   caller enters for a line, and how a cut is judged.
//! - [`labels`]: anchors, and label sets placed by pattern.

pub mod engine;
pub mod geometry;
pub mod halo;
pub mod identity;
pub mod kinds;
pub mod labels;
pub mod level;
pub mod presentation;
pub mod record;
pub mod rules;
