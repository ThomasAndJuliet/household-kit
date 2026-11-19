// SPDX-License-Identifier: GPL-3.0-only
//! A part with its own stages, placed as a leaf in another layout, must
//! behave in the toolkit-free port (`household_layout::engine`) as this
//! crate's engine (`household_widgets::layout`) makes it behave: there it
//! is a `ResponsiveLevels` inside a native leaf, here a `ResponsiveLevels`
//! made into a `Child`. Each tree is described once and built in both, as
//! in `layout_port_equality`. Over sweeps of available width and height,
//! every outer stage's query and the outer stage chosen must agree:
//! whether it fits, the error when it does not, and every plan's size,
//! minimum height, hidden flag and placements, recursively. At each nested
//! part, the stage the port records it took must be the one the engine's
//! own `ResponsiveLevels` takes in the same room (laid out as the engine
//! lays out a native leaf), with the same plan, or, when none fits, the
//! same error for each of its stages.

use std::collections::BTreeSet;

use cosmic::iced::advanced::Widget;
use cosmic::iced::advanced::layout::Limits;
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::advanced::widget::Tree;
use cosmic::iced::{Font, Pixels, Size};
use cosmic::{Renderer, widget};
use household_layout::engine as port;
use household_layout::geometry as pg;
use household_widgets::layout as eng;

/// Lengths agree when they differ by less than this: the engine works in
/// `f32` and the port in `f64`, and shares of a shortage are divided.
const TOLERANCE: f64 = 1e-3;

fn renderer() -> Renderer {
    cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
        Font::default(),
        Pixels(14.0),
        Some("tiny-skia"),
    ))
    .expect("headless tiny-skia renderer")
}

#[derive(Clone, Copy)]
enum Group {
    Row,
    Column,
    Wrap,
}

#[derive(Clone, Copy)]
enum Modifier {
    Weight(f32),
    MinimumWidth(f32),
}

/// One tree, built in both. Modifiers here add no wrapper, so the plan of
/// a tree has the tree's own shape.
#[derive(Clone)]
enum Spec {
    /// A fixed-size native box, with a width for each level.
    Block([f32; 4], f32),
    /// A declared viewport over a fixed-size box, with its useful minimum.
    Scroll((f32, f32), (f32, f32)),
    Group(Group, f32, Vec<Spec>),
    /// A part with its own stages (level index, tree), and the size of a
    /// fixed-size fallback.
    Nested(Vec<(usize, Spec)>, Option<(f32, f32)>),
    With(Box<Spec>, Vec<Modifier>),
}

fn b(width: f32, height: f32) -> Spec {
    Spec::Block([width; 4], height)
}

fn group(group: Group, spacing: f32, children: Vec<Spec>) -> Spec {
    Spec::Group(group, spacing, children)
}

fn with(spec: Spec, modifiers: &[Modifier]) -> Spec {
    Spec::With(Box::new(spec), modifiers.to_vec())
}

fn ranged(preferred: f32, minimum: f32, height: f32) -> Spec {
    with(b(preferred, height), &[Modifier::MinimumWidth(minimum)])
}

fn index(level: eng::Level) -> usize {
    eng::Level::ALL
        .iter()
        .position(|candidate| *candidate == level)
        .expect("a level")
}

fn port_index(level: port::Level) -> usize {
    port::Level::ALL
        .iter()
        .position(|candidate| *candidate == level)
        .expect("a level")
}

fn engine_levels(
    stages: &[(usize, Spec)],
    fallback: Option<(f32, f32)>,
) -> eng::ResponsiveLevels<'static, ()> {
    let levels = eng::stages(
        stages
            .iter()
            .map(|(level, spec)| eng::Stage::new(eng::Level::ALL[*level], engine(spec)))
            .collect(),
    );
    match fallback {
        Some((width, height)) => {
            levels.fallback(move || widget::Space::new().width(width).height(height).into())
        }
        None => levels,
    }
}

fn engine(spec: &Spec) -> eng::Child<'static, ()> {
    match spec.clone() {
        Spec::Block(widths, height) => eng::child(move |level| {
            widget::Space::new()
                .width(widths[index(level)])
                .height(height)
                .into()
        }),
        Spec::Scroll(size, minimum) => eng::scroll_child(
            move |_| widget::Space::new().width(size.0).height(size.1).into(),
            Size::new(minimum.0, minimum.1),
        ),
        Spec::Group(kind, spacing, children) => {
            let children = children.iter().map(engine).collect();
            match kind {
                Group::Row => eng::row_child(children, spacing),
                Group::Column => eng::column_child(children, spacing),
                Group::Wrap => eng::wrap_child(children, spacing),
            }
        }
        // As an app places it: the stage list as the view of a native leaf.
        Spec::Nested(stages, fallback) => {
            eng::child(move |_| engine_levels(&stages, fallback).into())
        }
        Spec::With(inner, modifiers) => {
            modifiers
                .iter()
                .fold(engine(&inner), |child, modifier| match *modifier {
                    Modifier::Weight(weight) => child.weight(weight),
                    Modifier::MinimumWidth(width) => child.minimum_width(move |_, _| width),
                })
        }
    }
}

/// A fixed-size native box, as iced resolves it: clamped to the room.
struct Block {
    widths: [f64; 4],
    height: f64,
}

impl port::Leaf for Block {
    fn query(&self, level: port::Level) -> port::MeasuredSize {
        let size = self.layout(level, pg::Size::INFINITE);
        port::MeasuredSize {
            preferred: size,
            minimum: size,
        }
    }

    fn layout(&self, level: port::Level, maximum: pg::Size) -> pg::Size {
        pg::Size::new(
            self.widths[port_index(level)].min(maximum.width),
            self.height.min(maximum.height),
        )
    }
}

fn port_levels(
    stages: &[(usize, Spec)],
    fallback: Option<(f32, f32)>,
) -> port::ResponsiveLevels<'static> {
    let levels = port::stages(
        stages
            .iter()
            .map(|(level, spec)| port::Stage::new(port::Level::ALL[*level], ported(spec)))
            .collect(),
    );
    match fallback {
        Some((width, height)) => levels.fallback(move |maximum: pg::Size| {
            pg::Size::new(
                f64::from(width).min(maximum.width),
                f64::from(height).min(maximum.height),
            )
        }),
        None => levels,
    }
}

fn ported(spec: &Spec) -> port::Child<'static> {
    match spec.clone() {
        Spec::Block(widths, height) => port::child(Block {
            widths: widths.map(f64::from),
            height: f64::from(height),
        }),
        Spec::Scroll(size, minimum) => port::scroll_child(
            Block {
                widths: [f64::from(size.0); 4],
                height: f64::from(size.1),
            },
            pg::Size::new(f64::from(minimum.0), f64::from(minimum.1)),
        ),
        Spec::Group(kind, spacing, children) => {
            let children = children.iter().map(ported).collect();
            let spacing = f64::from(spacing);
            match kind {
                Group::Row => port::row_child(children, spacing),
                Group::Column => port::column_child(children, spacing),
                Group::Wrap => port::wrap_child(children, spacing),
            }
        }
        Spec::Nested(stages, fallback) => port::Child::from(port_levels(&stages, fallback)),
        Spec::With(inner, modifiers) => {
            modifiers
                .iter()
                .fold(ported(&inner), |child, modifier| match *modifier {
                    Modifier::Weight(weight) => child.weight(f64::from(weight)),
                    Modifier::MinimumWidth(width) => {
                        child.minimum_width(move |_, _| f64::from(width))
                    }
                })
        }
    }
}

fn close(engine: f32, port: f64) -> bool {
    let engine = f64::from(engine);
    engine == port || (engine.is_nan() && port.is_nan()) || (engine - port).abs() <= TOLERANCE
}

fn same_error(engine: eng::AllocationError, port: port::AllocationError) -> bool {
    match (engine, port) {
        (eng::AllocationError::InvalidInput, port::AllocationError::InvalidInput) => true,
        (
            eng::AllocationError::NoFit {
                required: engine_required,
                available: engine_available,
            },
            port::AllocationError::NoFit {
                required,
                available,
            },
        ) => close(engine_required, required) && close(engine_available, available),
        _ => false,
    }
}

fn same_measurement(engine: eng::MeasuredSize, port: port::MeasuredSize) -> bool {
    close(engine.preferred.width, port.preferred.width)
        && close(engine.preferred.height, port.preferred.height)
        && close(engine.minimum.width, port.minimum.width)
        && close(engine.minimum.height, port.minimum.height)
}

/// What the nested parts chose over a sweep: a stage index, or `None` when
/// none of its stages fitted.
type Seen = BTreeSet<Option<usize>>;

struct Checker {
    renderer: Renderer,
    seen: Seen,
}

impl Checker {
    fn same_plan(
        &mut self,
        spec: &Spec,
        engine: &eng::GeometryPlan,
        port: &port::GeometryPlan,
        path: &str,
    ) -> Result<(), String> {
        let differs = |what: &str, engine: String, port: String| {
            Err(format!("{path}: {what}: engine {engine}, port {port}"))
        };
        if !close(engine.size.width, port.size.width)
            || !close(engine.size.height, port.size.height)
        {
            return differs(
                "size",
                format!("{:?}", engine.size),
                format!("{:?}", port.size),
            );
        }
        if !close(engine.minimum_height(), port.minimum_height()) {
            return differs(
                "minimum height",
                engine.minimum_height().to_string(),
                port.minimum_height().to_string(),
            );
        }
        if engine.hidden != port.hidden {
            return differs("hidden", engine.hidden.to_string(), port.hidden.to_string());
        }
        if engine.children.len() != port.children.len() {
            return differs(
                "placements",
                engine.children.len().to_string(),
                port.children.len().to_string(),
            );
        }
        let mut spec = spec;
        while let Spec::With(inner, _) = spec {
            spec = inner;
        }
        if !matches!(spec, Spec::Nested(..)) && port.selection().is_some() {
            return Err(format!(
                "{path}: the port records a selection on a plain node"
            ));
        }
        match spec {
            Spec::Group(_, _, children) => {
                for (engine, port) in engine.children.iter().zip(&port.children) {
                    let path = format!("{path}/{}", engine.index);
                    if engine.index != port.index {
                        return Err(format!(
                            "{path}: index: engine {}, port {}",
                            engine.index, port.index
                        ));
                    }
                    if !close(engine.position.x, port.position.x)
                        || !close(engine.position.y, port.position.y)
                    {
                        return Err(format!(
                            "{path}: position: engine {:?}, port {:?}",
                            engine.position, port.position
                        ));
                    }
                    self.same_plan(
                        &children[engine.index],
                        &engine.geometry,
                        &port.geometry,
                        &path,
                    )?;
                }
                Ok(())
            }
            Spec::Nested(stages, fallback) => {
                self.same_selection(stages, *fallback, engine.size.width, port, path)
            }
            _ => Ok(()),
        }
    }

    /// The engine's own stage list laid out as the engine lays out a
    /// native leaf at its allocated width (unbounded height, a fill width
    /// compressed only when unbounded), against the port's record.
    fn same_selection(
        &mut self,
        stages: &[(usize, Spec)],
        fallback: Option<(f32, f32)>,
        width: f32,
        port: &port::GeometryPlan,
        path: &str,
    ) -> Result<(), String> {
        let path = format!("{path} (nested)");
        let Some(port_selection) = port.selection() else {
            return Err(format!("{path}: the port records no selection"));
        };
        let mut levels = engine_levels(stages, fallback);
        let mut tree = Tree::empty();
        let maximum = Size::new(width, f32::INFINITY);
        let node = levels.layout(
            &mut tree,
            &self.renderer,
            &Limits::with_compression(Size::ZERO, maximum, Size::new(width.is_infinite(), true)),
        );
        if !close(node.size().width, port.size.width)
            || !close(node.size().height, port.size.height)
        {
            return Err(format!(
                "{path}: box: engine {:?}, port {:?}",
                node.size(),
                port.size
            ));
        }
        let engine_selection = levels.selection().expect("laid out").clone();
        match (&engine_selection, port_selection) {
            (Ok(engine), Ok(port)) => {
                self.seen.insert(Some(port.stage_index));
                if engine.stage_index != port.stage_index
                    || index(engine.level) != port_index(port.level)
                {
                    return Err(format!(
                        "{path}: stage: engine {} at {:?}, port {} at {:?}",
                        engine.stage_index, engine.level, port.stage_index, port.level
                    ));
                }
                self.same_plan(
                    &stages[port.stage_index].1,
                    &engine.geometry,
                    &port.geometry,
                    &format!("{path} stage {}", port.stage_index),
                )
            }
            (Err(engine_none), Err(port_none)) => {
                self.seen.insert(None);
                if engine_none.attempted != port_none.attempted
                    || port_none.failures.len() != stages.len()
                {
                    return Err(format!(
                        "{path}: no fit: engine {engine_none:?}, port {port_none:?}"
                    ));
                }
                // The engine discards each stage's error; its own query in
                // the same room gives it again.
                for ((level, spec), failure) in stages.iter().zip(&port_none.failures) {
                    let error = engine(spec)
                        .query_at(eng::Level::ALL[*level], &self.renderer, maximum)
                        .expect_err("no stage fits");
                    if !same_error(error, failure.error) {
                        return Err(format!(
                            "{path}: stage {} error: engine {error:?}, port {:?}",
                            failure.stage_index, failure.error
                        ));
                    }
                }
                Ok(())
            }
            _ => Err(format!(
                "{path}: fits: engine {}, port {}",
                engine_selection.is_ok(),
                port_selection.is_ok()
            )),
        }
    }
}

fn widths() -> Vec<f32> {
    let mut widths: Vec<f32> = (0..=128).map(|step| step as f32 * 2.5).collect();
    widths.extend([33.3, 97.75, 101.0, 147.5, 148.0, 1000.0, f32::INFINITY]);
    widths
}

const HEIGHTS: [f32; 16] = [
    0.0,
    10.0,
    20.0,
    30.0,
    40.0,
    47.0,
    48.0,
    50.0,
    56.0,
    60.0,
    72.0,
    76.0,
    80.0,
    100.0,
    200.0,
    f32::INFINITY,
];

/// Compare one outer stage list over the whole sweep. Returns what the
/// nested parts chose, and how many sizes no outer stage fitted.
fn compare(name: &str, stages: &[(usize, Spec)]) -> (Seen, usize) {
    let mut checker = Checker {
        renderer: renderer(),
        seen: Seen::new(),
    };
    let engine_stages: Vec<_> = stages
        .iter()
        .map(|(level, spec)| eng::Stage::new(eng::Level::ALL[*level], engine(spec)))
        .collect();
    let port_stages: Vec<_> = stages
        .iter()
        .map(|(level, spec)| port::Stage::new(port::Level::ALL[*level], ported(spec)))
        .collect();
    let mut failures = Vec::new();
    for (engine, port) in engine_stages.iter().zip(&port_stages) {
        for (engine_level, port_level) in eng::Level::ALL.into_iter().zip(port::Level::ALL) {
            let engine = engine.content.query(engine_level, &checker.renderer);
            let port = port.content.query(port_level);
            if !same_measurement(engine, port) {
                failures.push(format!(
                    "{name}: query at {engine_level:?}: engine {engine:?}, port {port:?}"
                ));
            }
        }
    }
    let mut unfitted = 0;
    for width in widths() {
        for height in HEIGHTS {
            let at = format!("{name} at {width} x {height}");
            let available = Size::new(width, height);
            let port_available = pg::Size::new(f64::from(width), f64::from(height));
            for (stage, ((_, spec), (engine, port))) in stages
                .iter()
                .zip(engine_stages.iter().zip(&port_stages))
                .enumerate()
            {
                let engine = engine
                    .content
                    .query_at(engine.level, &checker.renderer, available);
                let port = port.content.query_at(port.level, port_available);
                let path = format!("{at}, stage {stage}");
                match (&engine, &port) {
                    (Ok(engine), Ok(port)) => {
                        if let Err(difference) = checker.same_plan(spec, engine, port, &path) {
                            failures.push(difference);
                        }
                    }
                    (Err(engine), Err(port)) if same_error(*engine, *port) => {}
                    _ => failures.push(format!("{path}: engine {engine:?}, port {port:?}")),
                }
            }
            let engine = eng::choose_stage(&engine_stages, &checker.renderer, available);
            let port = port::choose_stage(&port_stages, port_available);
            match (&engine, &port) {
                (Ok(engine), Ok(port)) => {
                    if engine.stage_index != port.stage_index
                        || index(engine.level) != port_index(port.level)
                    {
                        failures.push(format!(
                            "{at}: chosen stage: engine {}, port {}",
                            engine.stage_index, port.stage_index
                        ));
                    } else if let Err(difference) = checker.same_plan(
                        &stages[port.stage_index].1,
                        &engine.geometry,
                        &port.geometry,
                        &at,
                    ) {
                        failures.push(difference);
                    }
                }
                (Err(engine), Err(port)) => {
                    unfitted += 1;
                    if engine.attempted != port.attempted
                        || port.failures.len() != engine_stages.len()
                    {
                        failures.push(format!("{at}: no fit: engine {engine:?}, port {port:?}"));
                    }
                    for (stage, failure) in engine_stages.iter().zip(&port.failures) {
                        let error = stage
                            .content
                            .query_at(stage.level, &checker.renderer, available)
                            .expect_err("no stage fits");
                        if !same_error(error, failure.error) {
                            failures.push(format!(
                                "{at}: stage {} error: engine {error:?}, port {:?}",
                                failure.stage_index, failure.error
                            ));
                        }
                    }
                }
                _ => failures.push(format!(
                    "{at}: fits: engine {}, port {}",
                    engine.is_ok(),
                    port.is_ok()
                )),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} disagreements, the first:\n{}",
        failures.len(),
        failures
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    (checker.seen, unfitted)
}

/// A label that can shrink from 120 to 60 and a control 80 wide: side by
/// side with a spacing of 8, then stacked with a spacing of 4.
fn setting() -> Spec {
    let parts = || vec![ranged(120.0, 60.0, 20.0), b(80.0, 20.0)];
    Spec::Nested(
        vec![
            (0, group(Group::Row, 8.0, parts())),
            (0, group(Group::Column, 4.0, parts())),
        ],
        None,
    )
}

fn beside_fixed_siblings(nested: Spec) -> Spec {
    group(
        Group::Column,
        6.0,
        vec![b(100.0, 20.0), nested, b(60.0, 10.0)],
    )
}

fn at_every_level(spec: Spec) -> Vec<(usize, Spec)> {
    (0..4).map(|level| (level, spec.clone())).collect()
}

fn set(choices: &[Option<usize>]) -> Seen {
    choices.iter().copied().collect()
}

#[test]
fn a_nested_list_in_a_column_beside_fixed_siblings() {
    // Without a declared minimum the outer column never offers less than
    // the first stage's 208, so the part never stacks.
    let (seen, unfitted) = compare(
        "nested in a column",
        &at_every_level(beside_fixed_siblings(setting())),
    );
    assert_eq!(seen, set(&[Some(0)]));
    assert!(unfitted > 0);
}

#[test]
fn a_nested_list_with_a_declared_minimum_width() {
    let (seen, _) = compare(
        "nested with a minimum",
        &at_every_level(beside_fixed_siblings(with(
            setting(),
            &[Modifier::MinimumWidth(100.0)],
        ))),
    );
    assert_eq!(seen, set(&[Some(0), Some(1)]));
}

#[test]
fn two_nested_lists_in_one_column_each_decide_for_themselves() {
    let three = || vec![b(70.0, 16.0), b(70.0, 16.0), b(70.0, 16.0)];
    let buttons = Spec::Nested(
        vec![
            (0, group(Group::Row, 4.0, three())),
            (0, group(Group::Wrap, 4.0, three())),
        ],
        None,
    );
    let (seen, _) = compare(
        "two nested",
        &at_every_level(group(
            Group::Column,
            6.0,
            vec![
                with(setting(), &[Modifier::MinimumWidth(100.0)]),
                b(40.0, 10.0),
                with(buttons, &[Modifier::MinimumWidth(70.0)]),
            ],
        )),
    );
    assert_eq!(seen, set(&[Some(0), Some(1)]));
}

#[test]
fn a_nested_list_in_a_row_beside_a_child_with_a_range() {
    let (seen, _) = compare(
        "nested in a row",
        &at_every_level(group(
            Group::Row,
            8.0,
            vec![
                ranged(150.0, 50.0, 30.0),
                with(
                    setting(),
                    &[Modifier::MinimumWidth(100.0), Modifier::Weight(2.0)],
                ),
            ],
        )),
    );
    assert_eq!(seen, set(&[Some(0), Some(1)]));
}

#[test]
fn the_outer_level_does_not_choose_the_inner_stage() {
    // The outer list is at Preferred, then Mild; the inner list names its
    // own levels, Preferred and Strong. Its first child is 90 at its own
    // Preferred, under either outer level.
    let inner_parts = || vec![Spec::Block([90.0, 40.0, 40.0, 40.0], 20.0), b(60.0, 20.0)];
    let inner = with(
        Spec::Nested(
            vec![
                (0, group(Group::Row, 6.0, inner_parts())),
                (2, group(Group::Column, 4.0, inner_parts())),
            ],
            None,
        ),
        &[Modifier::MinimumWidth(60.0)],
    );
    let outer = group(
        Group::Column,
        4.0,
        vec![Spec::Block([200.0, 120.0, 120.0, 120.0], 20.0), inner],
    );
    let (seen, _) = compare("outer levels", &[(0, outer.clone()), (1, outer)]);
    assert_eq!(seen, set(&[Some(0), Some(1)]));
}

#[test]
fn a_nested_list_none_of_whose_stages_fit() {
    for fallback in [None, Some((40.0, 12.0)), Some((120.0, 12.0))] {
        let nested = with(
            Spec::Nested(
                vec![
                    (
                        0,
                        group(Group::Row, 0.0, vec![b(50.0, 20.0), b(50.0, 20.0)]),
                    ),
                    (0, b(70.0, 30.0)),
                ],
                fallback,
            ),
            &[Modifier::MinimumWidth(30.0)],
        );
        let (seen, _) = compare(
            "none fit",
            &at_every_level(group(Group::Column, 0.0, vec![b(20.0, 10.0), nested])),
        );
        assert_eq!(seen, set(&[None, Some(0), Some(1)]));
    }
}

#[test]
fn the_worked_example_of_the_port_documentation() {
    let parts = || vec![b(120.0, 20.0), b(80.0, 20.0)];
    let setting = with(
        Spec::Nested(
            vec![
                (0, group(Group::Row, 8.0, parts())),
                (0, group(Group::Column, 4.0, parts())),
            ],
            None,
        ),
        &[Modifier::MinimumWidth(120.0)],
    );
    let (seen, unfitted) = compare(
        "worked example",
        &[(
            0,
            group(
                Group::Column,
                0.0,
                vec![
                    b(100.0, 20.0),
                    Spec::Scroll((0.0, 16.0), (0.0, 8.0)),
                    setting,
                ],
            ),
        )],
    );
    assert_eq!(seen, set(&[Some(0), Some(1)]));
    assert!(unfitted > 0);
}
