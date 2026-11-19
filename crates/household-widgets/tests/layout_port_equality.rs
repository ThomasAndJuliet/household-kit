// SPDX-License-Identifier: GPL-3.0-only
//! The toolkit-free port of the layout engine (`household_layout::engine`)
//! must make every choice this crate's engine (`household_widgets::layout`)
//! makes. Each tree below is described once and built twice: in the engine
//! from fixed-size native boxes measured by the headless tiny-skia renderer,
//! and in the port from leaves given the same numbers. Over sweeps of
//! available width and height, every stage's query and the stage chosen
//! must agree: whether it fits, the error when it does not, and the plan's
//! size, minimum height, hidden flags, and every placement's index,
//! position and size, recursively.

use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::{Font, Length, Padding, Pixels, Size};
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
    RowTop,
    Column,
    Wrap,
}

#[derive(Clone, Copy)]
enum Modifier {
    Weight(f32),
    Grow(f32),
    GrowHeight(f32),
    /// Hidden at this level index and above.
    HiddenFrom(usize),
    /// Preferred width at each level index.
    PreferredWidth([f32; 4]),
    /// Useful minimum width at each level index.
    MinimumWidth([f32; 4]),
    Fixed(Option<f32>, Option<f32>),
    FillWidth,
    RoundUp,
}

/// One tree, built in both.
#[derive(Clone)]
enum Spec {
    /// A fixed-size native box, with a width for each level.
    Block([f32; 4], f32),
    /// A native box that fills the width it is offered.
    Fill(f32),
    /// A declared viewport over a fixed-size box, with its useful minimum.
    Scroll((f32, f32), (f32, f32)),
    Spacer(f32),
    Group(Group, f32, Vec<Spec>),
    /// Forms from level indices.
    Forms(Vec<(usize, Spec)>),
    /// Chrome: top, right, bottom, left.
    Frame([f32; 4], Box<Spec>),
    With(Box<Spec>, Vec<Modifier>),
}

fn b(width: f32, height: f32) -> Spec {
    Spec::Block([width; 4], height)
}

fn at_levels(widths: [f32; 4], height: f32) -> Spec {
    Spec::Block(widths, height)
}

fn group(group: Group, spacing: f32, children: Vec<Spec>) -> Spec {
    Spec::Group(group, spacing, children)
}

fn with(spec: Spec, modifiers: &[Modifier]) -> Spec {
    Spec::With(Box::new(spec), modifiers.to_vec())
}

fn ranged(preferred: f32, minimum: f32, height: f32) -> Spec {
    with(
        b(preferred, height),
        &[Modifier::MinimumWidth([minimum; 4])],
    )
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

fn engine(spec: &Spec) -> eng::Child<'static, ()> {
    match spec.clone() {
        Spec::Block(widths, height) => eng::child(move |level| {
            widget::Space::new()
                .width(widths[index(level)])
                .height(height)
                .into()
        }),
        Spec::Fill(height) => eng::child(move |_| {
            widget::Space::new()
                .width(Length::Fill)
                .height(height)
                .into()
        }),
        Spec::Scroll(size, minimum) => eng::scroll_child(
            move |_| widget::Space::new().width(size.0).height(size.1).into(),
            Size::new(minimum.0, minimum.1),
        ),
        Spec::Spacer(minimum) => eng::spacer(minimum),
        Spec::Group(kind, spacing, children) => {
            let children = children.iter().map(engine).collect();
            match kind {
                Group::Row => eng::row_child(children, spacing),
                Group::RowTop => eng::row_child_top(children, spacing),
                Group::Column => eng::column_child(children, spacing),
                Group::Wrap => eng::wrap_child(children, spacing),
            }
        }
        Spec::Forms(alternatives) => eng::forms(
            alternatives
                .iter()
                .map(|(level, form)| (eng::Level::ALL[*level], engine(form)))
                .collect(),
        ),
        Spec::Frame([top, right, bottom, left], content) => {
            let padding = Padding {
                top,
                right,
                bottom,
                left,
            };
            eng::framed_child(engine(&content), padding, move |content| {
                widget::container(content).padding(padding).into()
            })
        }
        Spec::With(inner, modifiers) => {
            modifiers
                .iter()
                .fold(engine(&inner), |child, modifier| match *modifier {
                    Modifier::Weight(weight) => child.weight(weight),
                    Modifier::Grow(grow) => child.grow(grow),
                    Modifier::GrowHeight(grow) => child.grow_height(grow),
                    Modifier::HiddenFrom(from) => {
                        child.hidden_when(move |level| index(level) >= from)
                    }
                    Modifier::PreferredWidth(widths) => {
                        child.preferred_width_with(move |level, _, _| widths[index(level)])
                    }
                    Modifier::MinimumWidth(widths) => {
                        child.minimum_width(move |level, _| widths[index(level)])
                    }
                    Modifier::Fixed(width, height) => child.fixed_dimensions(width, height),
                    Modifier::FillWidth => child.fill_width(),
                    Modifier::RoundUp => child.round_width_up(),
                })
        }
    }
}

/// A length as a native fixed-size box resolves it (iced's
/// `Limits::resolve`): a fixed length is clamped to the room offered; a
/// fill length takes a finite offer and, unbounded, compresses to nothing.
#[derive(Clone, Copy)]
enum Len {
    Fixed(f64),
    Fill,
}

impl Len {
    fn resolve(self, maximum: f64) -> f64 {
        match self {
            Self::Fixed(length) => length.min(maximum),
            Self::Fill if maximum.is_finite() => maximum,
            Self::Fill => 0.0,
        }
    }
}

struct Block {
    widths: [Len; 4],
    height: Len,
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
            self.widths[port_index(level)].resolve(maximum.width),
            self.height.resolve(maximum.height),
        )
    }
}

fn fixed_block(widths: [f32; 4], height: f32) -> Block {
    Block {
        widths: widths.map(|width| Len::Fixed(f64::from(width))),
        height: Len::Fixed(f64::from(height)),
    }
}

fn ported(spec: &Spec) -> port::Child<'static> {
    match spec.clone() {
        Spec::Block(widths, height) => port::child(fixed_block(widths, height)),
        Spec::Fill(height) => port::child(Block {
            widths: [Len::Fill; 4],
            height: Len::Fixed(f64::from(height)),
        }),
        Spec::Scroll(size, minimum) => port::scroll_child(
            fixed_block([size.0; 4], size.1),
            pg::Size::new(f64::from(minimum.0), f64::from(minimum.1)),
        ),
        Spec::Spacer(minimum) => port::spacer(f64::from(minimum)),
        Spec::Group(kind, spacing, children) => {
            let children = children.iter().map(ported).collect();
            let spacing = f64::from(spacing);
            match kind {
                Group::Row => port::row_child(children, spacing),
                Group::RowTop => port::row_child_top(children, spacing),
                Group::Column => port::column_child(children, spacing),
                Group::Wrap => port::wrap_child(children, spacing),
            }
        }
        Spec::Forms(alternatives) => port::forms(
            alternatives
                .iter()
                .map(|(level, form)| (port::Level::ALL[*level], ported(form)))
                .collect(),
        ),
        Spec::Frame(padding, content) => {
            port::framed_child(ported(&content), padding.map(f64::from))
        }
        Spec::With(inner, modifiers) => {
            modifiers
                .iter()
                .fold(ported(&inner), |child, modifier| match *modifier {
                    Modifier::Weight(weight) => child.weight(f64::from(weight)),
                    Modifier::Grow(grow) => child.grow(f64::from(grow)),
                    Modifier::GrowHeight(grow) => child.grow_height(f64::from(grow)),
                    Modifier::HiddenFrom(from) => {
                        child.hidden_when(move |level| port_index(level) >= from)
                    }
                    Modifier::PreferredWidth(widths) => child
                        .preferred_width_with(move |level, _| f64::from(widths[port_index(level)])),
                    Modifier::MinimumWidth(widths) => {
                        child.minimum_width(move |level, _| f64::from(widths[port_index(level)]))
                    }
                    Modifier::Fixed(width, height) => {
                        child.fixed_dimensions(width.map(f64::from), height.map(f64::from))
                    }
                    Modifier::FillWidth => child.fill_width(),
                    Modifier::RoundUp => child.round_width_up(),
                })
        }
    }
}

fn close(engine: f32, port: f64) -> bool {
    let engine = f64::from(engine);
    engine == port || (engine.is_nan() && port.is_nan()) || (engine - port).abs() <= TOLERANCE
}

fn same_plan(
    engine: &eng::GeometryPlan,
    port: &port::GeometryPlan,
    path: &str,
) -> Result<(), String> {
    let differs = |what: &str, engine: String, port: String| {
        Err(format!("{path}: {what}: engine {engine}, port {port}"))
    };
    if !close(engine.size.width, port.size.width) || !close(engine.size.height, port.size.height) {
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
    for (engine, port) in engine.children.iter().zip(&port.children) {
        let path = format!("{path}/{}", engine.index);
        if engine.index != port.index {
            return Err(format!(
                "{path}: index: engine {}, port {}",
                engine.index, port.index
            ));
        }
        if !close(engine.position.x, port.position.x) || !close(engine.position.y, port.position.y)
        {
            return Err(format!(
                "{path}: position: engine {:?}, port {:?}",
                engine.position, port.position
            ));
        }
        same_plan(&engine.geometry, &port.geometry, &path)?;
    }
    Ok(())
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

fn widths() -> Vec<f32> {
    let mut widths: Vec<f32> = (0..=144).map(|step| step as f32 * 2.5).collect();
    widths.extend([33.3, 97.75, 101.0, 1000.0, f32::INFINITY]);
    widths
}

const HEIGHTS: [f32; 20] = [
    0.0,
    9.0,
    10.0,
    19.5,
    20.0,
    24.0,
    25.0,
    30.0,
    39.0,
    40.0,
    45.0,
    50.0,
    60.0,
    64.0,
    75.0,
    80.0,
    100.0,
    120.0,
    200.0,
    f32::INFINITY,
];

/// Compare one stage list over the whole sweep; return the number of sizes
/// compared and how many of them no stage fitted.
fn compare(name: &str, stages: &[(usize, Spec)]) -> (usize, usize) {
    let renderer = renderer();
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
            let engine = engine.content.query(engine_level, &renderer);
            let port = port.content.query(port_level);
            if !same_measurement(engine, port) {
                failures.push(format!(
                    "{name}: query at {engine_level:?}: engine {engine:?}, port {port:?}"
                ));
            }
        }
    }
    let mut sizes = 0;
    let mut unfitted = 0;
    for width in widths() {
        for height in HEIGHTS {
            sizes += 1;
            let at = format!("{name} at {width} x {height}");
            let available = Size::new(width, height);
            let port_available = pg::Size::new(f64::from(width), f64::from(height));
            for (stage, (engine, port)) in engine_stages.iter().zip(&port_stages).enumerate() {
                let engine = engine.content.query_at(engine.level, &renderer, available);
                let port = port.content.query_at(port.level, port_available);
                let path = format!("{at}, stage {stage}");
                match (&engine, &port) {
                    (Ok(engine), Ok(port)) => {
                        if let Err(difference) = same_plan(engine, port, &path) {
                            failures.push(difference);
                        }
                    }
                    (Err(engine), Err(port)) if same_error(*engine, *port) => {}
                    _ => failures.push(format!("{path}: engine {engine:?}, port {port:?}")),
                }
            }
            let engine = eng::choose_stage(&engine_stages, &renderer, available);
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
                    } else if let Err(difference) = same_plan(&engine.geometry, &port.geometry, &at)
                    {
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
                    // The engine discards each stage's error; its own query
                    // gives it again, and the port must report that error.
                    for (stage, failure) in engine_stages.iter().zip(&port.failures) {
                        let error = stage
                            .content
                            .query_at(stage.level, &renderer, available)
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
    (sizes, unfitted)
}

/// The same tree at every level, as a root offering each in turn.
fn at_every_level(spec: Spec) -> Vec<(usize, Spec)> {
    (0..4).map(|level| (level, spec.clone())).collect()
}

const MILD: usize = 1;
const STRONG: usize = 2;

fn leaves_with_ranges() -> Vec<Spec> {
    vec![
        ranged(120.0, 40.0, 20.0),
        with(ranged(80.0, 60.0, 30.0), &[Modifier::Weight(2.0)]),
        with(ranged(60.0, 20.0, 10.0), &[Modifier::Weight(0.5)]),
    ]
}

#[test]
fn a_row_of_leaves_with_ranges_and_weights() {
    compare(
        "row",
        &at_every_level(group(Group::Row, 8.0, leaves_with_ranges())),
    );
}

#[test]
fn a_column_of_leaves_with_ranges_and_weights() {
    compare(
        "column",
        &at_every_level(group(Group::Column, 6.0, leaves_with_ranges())),
    );
}

#[test]
fn a_wrap_of_leaves_with_ranges_and_weights() {
    compare(
        "wrap",
        &at_every_level(group(Group::Wrap, 4.0, leaves_with_ranges())),
    );
}

#[test]
fn a_zero_weight_child_keeps_its_preferred_width() {
    compare(
        "zero weight",
        &at_every_level(group(
            Group::Row,
            5.0,
            vec![
                ranged(100.0, 40.0, 20.0),
                with(ranged(50.0, 10.0, 20.0), &[Modifier::Weight(0.0)]),
                with(ranged(70.0, 30.0, 20.0), &[Modifier::Weight(3.0)]),
            ],
        )),
    );
}

#[test]
fn growing_children_and_spacers_share_surplus() {
    compare(
        "growth",
        &at_every_level(group(
            Group::Row,
            4.0,
            vec![
                b(40.0, 20.0),
                Spec::Spacer(8.0),
                with(ranged(60.0, 20.0, 20.0), &[Modifier::Grow(2.0)]),
                with(Spec::Spacer(0.0), &[Modifier::Grow(3.0)]),
                with(Spec::Fill(16.0), &[Modifier::MinimumWidth([30.0; 4])]),
                with(b(10.25, 20.0), &[Modifier::RoundUp]),
            ],
        )),
    );
}

#[test]
fn rows_nested_in_columns_and_columns_in_rows() {
    let row = |first: f32| {
        group(
            Group::Row,
            4.0,
            vec![ranged(first, first / 2.0, 20.0), b(30.0, 16.0)],
        )
    };
    compare(
        "rows in a column",
        &at_every_level(group(
            Group::Column,
            6.0,
            vec![row(120.0), row(80.0), ranged(150.0, 50.0, 12.0)],
        )),
    );
    let column = |first: f32| {
        group(
            Group::Column,
            3.0,
            vec![ranged(first, 30.0, 20.0), b(40.0, 10.0)],
        )
    };
    compare(
        "columns in a row",
        &at_every_level(group(
            Group::Row,
            10.0,
            vec![
                column(90.0),
                with(column(140.0), &[Modifier::Weight(2.0)]),
                b(20.0, 50.0),
            ],
        )),
    );
}

#[test]
fn forms_change_at_mild_and_strong_with_a_nested_composition() {
    let family = Spec::Forms(vec![
        (
            0,
            group(
                Group::Row,
                4.0,
                vec![ranged(100.0, 60.0, 20.0), b(40.0, 20.0)],
            ),
        ),
        (
            MILD,
            group(
                Group::Row,
                4.0,
                vec![
                    with(
                        b(60.0, 20.0),
                        &[Modifier::MinimumWidth([60.0, 30.0, 30.0, 30.0])],
                    ),
                    b(24.0, 20.0),
                ],
            ),
        ),
        (STRONG, b(24.0, 20.0)),
    ]);
    compare(
        "forms",
        &at_every_level(group(
            Group::Row,
            8.0,
            vec![
                with(family, &[Modifier::Weight(3.0), Modifier::GrowHeight(1.0)]),
                ranged(50.0, 20.0, 20.0),
                at_levels([40.0, 40.0, 30.0, 20.0], 24.0),
            ],
        )),
    );
}

#[test]
fn children_hidden_from_a_level_cost_nothing() {
    compare(
        "hidden",
        &at_every_level(group(
            Group::Row,
            5.0,
            vec![
                b(40.0, 20.0),
                with(b(30.0, 20.0), &[Modifier::HiddenFrom(MILD)]),
                ranged(60.0, 20.0, 20.0),
                group(
                    Group::Row,
                    8.0,
                    vec![with(b(30.0, 20.0), &[Modifier::HiddenFrom(STRONG)])],
                ),
                with(b(25.0, 30.0), &[Modifier::HiddenFrom(0)]),
            ],
        )),
    );
}

#[test]
fn fixed_dimensions_and_reserved_widths() {
    compare(
        "fixed",
        &at_every_level(group(
            Group::Row,
            6.0,
            vec![
                with(
                    ranged(40.0, 20.0, 20.0),
                    &[Modifier::Fixed(Some(70.0), None)],
                ),
                ranged(80.0, 30.0, 20.0),
                with(
                    group(
                        Group::Column,
                        2.0,
                        vec![b(30.0, 20.0), ranged(45.0, 25.0, 10.0)],
                    ),
                    &[Modifier::Fixed(Some(50.0), Some(60.0))],
                ),
                with(b(20.0, 10.0), &[Modifier::Fixed(Some(10.0), Some(5.0))]),
            ],
        )),
    );
    compare(
        "fill width",
        &at_every_level(group(
            Group::Wrap,
            4.0,
            vec![
                with(
                    group(
                        Group::Wrap,
                        4.0,
                        vec![b(50.0, 20.0), b(60.0, 20.0), b(40.0, 20.0)],
                    ),
                    &[Modifier::FillWidth],
                ),
                b(30.0, 20.0),
            ],
        )),
    );
    compare(
        "fixed height in a column",
        &at_every_level(group(
            Group::Column,
            4.0,
            vec![
                b(60.0, 20.0),
                with(b(60.0, 20.0), &[Modifier::Fixed(None, Some(35.0))]),
                with(b(60.0, 15.0), &[Modifier::GrowHeight(1.0)]),
            ],
        )),
    );
}

#[test]
fn a_declared_viewport_gives_height_in_a_short_column() {
    compare(
        "viewport",
        &at_every_level(group(
            Group::Column,
            5.0,
            vec![
                b(80.0, 20.0),
                Spec::Scroll((300.0, 300.0), (60.0, 40.0)),
                b(80.0, 10.0),
            ],
        )),
    );
    compare(
        "growing viewport",
        &at_every_level(group(
            Group::Column,
            5.0,
            vec![
                b(80.0, 20.0),
                with(
                    Spec::Scroll((80.0, 20.0), (60.0, 10.0)),
                    &[Modifier::GrowHeight(1.0)],
                ),
                with(
                    Spec::Scroll((90.0, 100.0), (40.0, 30.0)),
                    &[Modifier::Weight(2.0)],
                ),
            ],
        )),
    );
    compare(
        "viewport in a wrap",
        &at_every_level(group(
            Group::Wrap,
            4.0,
            vec![
                b(60.0, 20.0),
                Spec::Scroll((200.0, 200.0), (50.0, 30.0)),
                b(60.0, 20.0),
                group(
                    Group::Column,
                    2.0,
                    vec![b(40.0, 10.0), Spec::Scroll((40.0, 80.0), (40.0, 20.0))],
                ),
            ],
        )),
    );
}

#[test]
fn rows_centre_their_children_unless_top_aligned() {
    let children = || {
        vec![
            b(40.0, 20.0),
            b(30.0, 50.0),
            ranged(60.0, 20.0, 10.0),
            Spec::Scroll((50.0, 60.0), (30.0, 15.0)),
        ]
    };
    compare(
        "top-aligned row",
        &at_every_level(group(Group::RowTop, 4.0, children())),
    );
    compare(
        "centred row",
        &at_every_level(group(Group::Row, 4.0, children())),
    );
}

#[test]
fn a_frame_counts_its_chrome_inside_the_fit() {
    compare(
        "frames",
        &at_every_level(group(
            Group::Row,
            4.0,
            vec![
                Spec::Frame(
                    [4.0, 6.0, 4.0, 6.0],
                    Box::new(group(
                        Group::Row,
                        2.0,
                        vec![ranged(80.0, 30.0, 20.0), b(20.0, 20.0)],
                    )),
                ),
                Spec::Frame(
                    [2.0, 3.0, 5.0, 1.0],
                    Box::new(with(ranged(50.0, 20.0, 16.0), &[Modifier::Weight(0.0)])),
                ),
                with(
                    Spec::Frame(
                        [1.0, 1.0, 1.0, 1.0],
                        Box::new(Spec::Scroll((60.0, 90.0), (30.0, 20.0))),
                    ),
                    &[Modifier::GrowHeight(1.0)],
                ),
                Spec::Frame(
                    [3.0, 3.0, 3.0, 3.0],
                    Box::new(with(b(30.0, 10.0), &[Modifier::HiddenFrom(STRONG)])),
                ),
            ],
        )),
    );
}

#[test]
fn preferred_and_minimum_widths_by_level() {
    compare(
        "widths by level",
        &at_every_level(group(
            Group::Row,
            4.0,
            vec![
                with(
                    Spec::Fill(20.0),
                    &[
                        Modifier::MinimumWidth([80.0, 60.0, 40.0, 20.0]),
                        Modifier::PreferredWidth([240.0, 160.0, 100.0, 0.0]),
                    ],
                ),
                with(
                    b(100.0, 20.0),
                    &[
                        Modifier::MinimumWidth([100.0, 50.0, 30.0, 30.0]),
                        Modifier::PreferredWidth([90.0, 90.0, 60.0, 60.0]),
                    ],
                ),
            ],
        )),
    );
}

fn toolbar_items() -> Vec<Spec> {
    vec![
        at_levels([80.0, 50.0, 50.0, 30.0], 20.0),
        at_levels([70.0, 40.0, 40.0, 30.0], 24.0),
        with(
            at_levels([60.0, 60.0, 30.0, 30.0], 20.0),
            &[Modifier::Weight(0.0)],
        ),
        with(ranged(90.0, 40.0, 18.0), &[Modifier::HiddenFrom(STRONG)]),
    ]
}

#[test]
fn stage_lists_mix_levels_and_arrangements() {
    let (_, unfitted) = compare(
        "stages",
        &[
            (0, group(Group::Row, 6.0, toolbar_items())),
            (MILD, group(Group::Row, 6.0, toolbar_items())),
            (MILD, group(Group::Wrap, 6.0, toolbar_items())),
            (STRONG, group(Group::Wrap, 6.0, toolbar_items())),
        ],
    );
    assert!(unfitted > 0, "some sizes fit no stage");
}

#[test]
fn a_stage_list_where_nothing_fits() {
    let (sizes, unfitted) = compare(
        "nothing fits",
        &[
            (
                0,
                group(Group::Row, 4.0, vec![b(400.0, 20.0), b(30.0, 20.0)]),
            ),
            (
                MILD,
                group(
                    Group::Wrap,
                    4.0,
                    vec![ranged(400.0, 380.0, 20.0), b(30.0, 20.0)],
                ),
            ),
            (
                STRONG,
                group(
                    Group::Column,
                    4.0,
                    vec![
                        b(30.0, 300.0),
                        with(b(30.0, 20.0), &[Modifier::Fixed(Some(370.0), None)]),
                    ],
                ),
            ),
        ],
    );
    // Only the widest offers fit any stage of this list.
    assert!(unfitted > sizes / 2);
}
