// SPDX-License-Identifier: GPL-3.0-only

//! The numeric assertions of household-widgets' engine tests that need no
//! widget (`layout_tests.rs`, `layout_engine_tests.rs`,
//! `layout_contract_tests.rs` and the renderer tests in `layout.rs`), with
//! their leaves given as numbers, and tests of the port's three additions.

use super::*;

/// A length as a native fixed-size box declares it: a fixed length is
/// clamped to the room offered; a fill length takes a finite offer and
/// has none when unbounded.
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

/// The leaves the engine's tests build: a box, possibly of a different
/// width at each level, whose intrinsic size is its layout unbounded.
struct Block {
    widths: [Len; 4],
    height: Len,
}

impl Leaf for Block {
    fn query(&self, level: Level) -> MeasuredSize {
        let size = self.layout(level, Size::INFINITE);
        MeasuredSize {
            preferred: size,
            minimum: size,
        }
    }

    fn layout(&self, level: Level, maximum: Size) -> Size {
        Size::new(
            self.widths[level as usize].resolve(maximum.width),
            self.height.resolve(maximum.height),
        )
    }
}

fn block_leaf(width: f64, height: f64) -> Block {
    Block {
        widths: [Len::Fixed(width); 4],
        height: Len::Fixed(height),
    }
}

fn block(width: f64, height: f64) -> Child<'static> {
    child(block_leaf(width, height))
}

fn levels(widths: [f64; 4], height: f64) -> Child<'static> {
    child(Block {
        widths: widths.map(Len::Fixed),
        height: Len::Fixed(height),
    })
}

fn fill(height: f64) -> Child<'static> {
    child(Block {
        widths: [Len::Fill; 4],
        height: Len::Fixed(height),
    })
}

fn all_levels(content: &Child<'static>) -> Vec<Stage<'static>> {
    Level::ALL
        .into_iter()
        .map(|level| Stage::new(level, content.clone()))
        .collect()
}

fn capacity(preferred: f64, minimum: f64, weight: f64) -> WidthCapacity {
    WidthCapacity {
        preferred,
        minimum,
        weight,
    }
}

#[test]
fn first_fit_respects_preference_bounds_height_and_explicit_failure() {
    let measured = |level| match level {
        Level::Preferred | Level::Mild => Size::new(160.0, 48.0),
        Level::Strong => Size::new(100.0, 40.0),
        Level::AllPermitted => Size::new(60.0, 40.0),
    };
    assert_eq!(
        first_fitting(Level::AllPermitted, Size::new(160.0, 48.0), measured),
        Some(Level::Preferred)
    );
    assert_eq!(
        first_fitting(Level::AllPermitted, Size::new(160.0, 40.0), measured),
        Some(Level::Strong)
    );
    assert_eq!(
        first_fitting(Level::Mild, Size::new(100.0, 48.0), measured),
        None
    );
    assert_eq!(
        first_fitting(Level::AllPermitted, Size::new(59.0, 48.0), measured),
        None
    );
    assert_eq!(
        first_fitting(Level::AllPermitted, Size::INFINITE, |_| Size::new(
            f64::NAN,
            0.0
        )),
        None
    );
    // The engine's first fit has no tolerance.
    assert_eq!(
        first_fitting(Level::Preferred, Size::new(159.9995, 48.0), measured),
        None
    );
}

#[test]
fn capacity_sharing_uses_original_rates_clamps_and_protects_zero_weight() {
    let children = [
        capacity(100.0, 80.0, 3.0),
        capacity(100.0, 0.0, 1.0),
        capacity(40.0, 0.0, 0.0),
    ];
    assert_eq!(allocate_width(160.0, &children), Ok(vec![80.0, 40.0, 40.0]));
    assert_eq!(
        allocate_width(300.0, &children),
        Ok(vec![100.0, 100.0, 40.0])
    );
    assert_eq!(
        allocate_width(119.0, &children),
        Err(AllocationError::NoFit {
            required: 120.0,
            available: 119.0
        })
    );
    let weighted = allocate_width(
        150.0,
        &[capacity(100.0, 0.0, 2.0), capacity(100.0, 0.0, 1.0)],
    )
    .unwrap();
    assert!((weighted.iter().sum::<f64>() - 150.0).abs() < 0.001);
    assert!(((100.0 - weighted[0]) / (100.0 - weighted[1]) - 2.0).abs() < 0.001);
    assert_eq!(allocate_width(0.0, &[]), Ok(vec![]));
    assert_eq!(
        allocate_width(10.0, &[capacity(1.0, 2.0, 1.0)]),
        Err(AllocationError::InvalidInput)
    );
}

#[test]
fn requested_dimension_roundoff_uses_the_fit_tolerance() {
    let third = 1.0_f64 / 3.0;
    let capacities = [capacity(third, third, 0.0); 3];
    assert_eq!(allocate_width(1.0, &capacities), Ok(vec![third; 3]));
    assert!(matches!(
        allocate_width(1.0 - 2.0 * FIT_EPSILON, &capacities),
        Err(AllocationError::NoFit { .. })
    ));
}

#[test]
fn three_nested_rows_query_and_choose_one_parent_selected_level() {
    let flexible = block(160.0, 20.0).minimum_width(|level, preferred| {
        if level >= Level::Mild {
            60.0
        } else {
            preferred.width
        }
    });
    let fixed = || block(10.0, 20.0).weight(0.0);
    let inner = row_child(vec![flexible, fixed()], 0.0);
    let middle = row_child(vec![inner, fixed()], 0.0);
    let outer = row_child(vec![middle, fixed()], 0.0);
    assert_eq!(outer.query(Level::Preferred).minimum.width, 190.0);
    assert_eq!(outer.query(Level::Mild).minimum.width, 90.0);
    let stages = all_levels(&outer);
    let narrow = choose_stage(&stages, Size::new(100.0, 40.0)).unwrap();
    assert_eq!(narrow.level, Level::Mild);
    assert_eq!(narrow.geometry.size.width, 100.0);
    let wide = choose_stage(&stages, Size::new(220.0, 40.0)).unwrap();
    assert_eq!(wide.level, Level::Preferred);
}

#[test]
fn elastic_gap_is_measured_at_its_minimum_and_keeps_actions_at_the_right() {
    let component = row_child(vec![block(40.0, 32.0), spacer(8.0), block(32.0, 32.0)], 0.0);
    assert_eq!(component.query(Level::Preferred).minimum.width, 80.0);
    let stages = all_levels(&component);
    for width in [200.0, 80.0] {
        let plan = choose_stage(&stages, Size::new(width, 32.0))
            .unwrap()
            .geometry;
        assert_eq!(plan.children[0].geometry.size.width, 40.0);
        assert_eq!(plan.children[2].geometry.size.width, 32.0);
        assert_eq!(plan.children[2].position.x, width - 32.0);
        assert_eq!(plan.children[1].geometry.size.width, width - 72.0);
    }
}

#[test]
fn surplus_shares_follow_grow_weights_without_changing_intrinsic_queries() {
    let component = row_child(vec![spacer(4.0).grow(1.0), spacer(8.0).grow(3.0)], 0.0);
    assert_eq!(component.query(Level::Preferred).minimum.width, 12.0);
    let plan = choose_stage(&all_levels(&component), Size::new(52.0, 32.0))
        .unwrap()
        .geometry;
    assert_eq!(plan.children[0].geometry.size.width, 14.0);
    assert_eq!(plan.children[1].geometry.size.width, 38.0);
}

#[test]
fn optional_whole_pixel_offers_keep_fractional_content_from_shifting_neighbors() {
    let leaf = || block(10.25, 32.0);
    assert_eq!(leaf().query(Level::Preferred).preferred.width, 10.25);
    let snapped = leaf().round_width_up();
    assert_eq!(snapped.query(Level::Preferred).preferred.width, 11.0);
    let component = row_child(vec![spacer(0.0), snapped, leaf().round_width_up()], 8.0);
    let plan = choose_stage(&all_levels(&component), Size::new(100.0, 32.0))
        .unwrap()
        .geometry;
    assert_eq!(plan.children[1].position.x, 70.0);
    assert_eq!(plan.children[2].position.x, 89.0);
}

#[test]
fn preferred_width_keeps_the_useful_minimum_and_negotiates_allocation() {
    let make = || fill(20.0).minimum_width(|_, _| 80.0);
    let family = make().preferred_width(240.0);
    let measured = family.query(Level::Preferred);
    assert_eq!(
        (measured.preferred.width, measured.minimum.width),
        (240.0, 80.0)
    );
    let narrowed = family
        .query_at(Level::Preferred, Size::new(180.0, 100.0))
        .unwrap();
    assert_eq!(narrowed.size.width, 180.0);
    assert!(
        family
            .query_at(Level::Preferred, Size::new(79.0, 100.0))
            .is_err()
    );
    let instance = family.clone().preferred_width(160.0);
    let measured = instance.query(Level::Preferred);
    assert_eq!(
        (measured.preferred.width, measured.minimum.width),
        (160.0, 80.0)
    );
    let wider_minimum = make().minimum_width(|_, _| 260.0).preferred_width(240.0);
    let measured = wider_minimum.query(Level::Preferred);
    assert_eq!(
        (measured.preferred.width, measured.minimum.width),
        (260.0, 260.0)
    );
    assert!(
        wider_minimum
            .query_at(Level::Preferred, Size::new(240.0, 100.0))
            .is_err()
    );
}

#[test]
fn invalid_query_constraints_never_produce_a_geometry_plan() {
    let content = block(40.0, 20.0);
    for available in [
        Size::new(f64::NAN, 30.0),
        Size::new(60.0, f64::NAN),
        Size::new(-1.0, 30.0),
        Size::new(60.0, -1.0),
        Size::new(f64::NEG_INFINITY, 30.0),
    ] {
        assert_eq!(
            content.query_at(Level::Preferred, available),
            Err(AllocationError::InvalidInput)
        );
    }
    assert_eq!(
        content
            .query_at(Level::Preferred, Size::INFINITE)
            .unwrap()
            .size,
        Size::new(40.0, 20.0)
    );
}

#[test]
fn invalid_measured_floors_do_not_produce_a_plan() {
    for invalid in [f64::NAN, f64::INFINITY, -1.0] {
        let width = block(40.0, 20.0).minimum_width_with(move |_, _| invalid);
        assert_eq!(
            width.query_at(Level::Preferred, Size::new(300.0, 300.0)),
            Err(AllocationError::InvalidInput)
        );
        let scrolling =
            scroll_child_with_minimum(block_leaf(40.0, 20.0), move |_| Size::new(invalid, 32.0));
        assert_eq!(
            scrolling.query_at(Level::Preferred, Size::new(300.0, 300.0)),
            Err(AllocationError::InvalidInput)
        );
    }
}

#[test]
fn column_height_growth_expands_a_short_viewport_after_the_fixed_header() {
    let header = block(80.0, 20.0);
    let viewport = scroll_child(block_leaf(80.0, 20.0), Size::new(80.0, 10.0)).grow_height(1.0);
    let column = column_child(vec![header, viewport], 5.0);
    let query = column
        .query_at(Level::Preferred, Size::new(80.0, 100.0))
        .unwrap();
    assert_eq!(query.size.height, 100.0);
    assert_eq!(query.children[0].geometry.size.height, 20.0);
    assert_eq!(query.children[1].position.y, 25.0);
    assert_eq!(query.children[1].geometry.size.height, 75.0);
}

#[test]
fn structured_forms_inherit_continuous_minima_and_forward_the_requested_level() {
    let title =
        block(100.0, 20.0).minimum_width(|level, _| if level >= Level::Mild { 20.0 } else { 60.0 });
    let family = forms(vec![
        (
            Level::Preferred,
            row_child(vec![title, block(40.0, 20.0)], 0.0),
        ),
        (Level::Strong, block(24.0, 20.0)),
    ]);
    assert_eq!(family.query(Level::Preferred).minimum.width, 100.0);
    assert_eq!(family.query(Level::Mild).minimum.width, 60.0);
    assert_eq!(family.query(Level::Strong).minimum.width, 24.0);
    assert_eq!(family.query(Level::AllPermitted).minimum.width, 24.0);
    let outer = row_child(vec![family.weight(3.0), block(10.0, 20.0)], 0.0);
    assert!(
        outer
            .query_at(Level::Preferred, Size::new(90.0, 30.0))
            .is_err()
    );
    let mild = outer.query_at(Level::Mild, Size::new(90.0, 30.0)).unwrap();
    assert_eq!(mild.children[0].geometry.size.width, 80.0);
    assert_eq!(
        mild.children[0].geometry.children[0].geometry.size.width,
        40.0
    );
    let strong = outer
        .query_at(Level::Strong, Size::new(90.0, 30.0))
        .unwrap();
    assert_eq!(strong.size.width, 34.0);
    assert!(strong.children[0].geometry.children.is_empty());
    let stages = vec![
        Stage::new(Level::Preferred, outer.clone()),
        Stage::new(Level::Mild, outer),
    ];
    let selected = choose_stage(&stages, Size::new(90.0, 30.0)).unwrap();
    assert_eq!(selected.level, Level::Mild);
    assert_eq!(selected.geometry, mild);
}

#[test]
fn structured_root_preserves_selected_composite_elastic_allocation() {
    let make_row = || row_child(vec![block(40.0, 20.0), spacer(0.0), block(32.0, 20.0)], 0.0);
    let direct = make_row();
    let family = forms(vec![
        (Level::Preferred, make_row()),
        (Level::Strong, block(24.0, 20.0)),
    ]);
    let available = Size::new(200.0, 40.0);
    let expected = direct.query_at(Level::Mild, available).unwrap();
    let inherited = family.query_at(Level::Mild, available).unwrap();
    assert_eq!(inherited, expected);
    assert_eq!(inherited.size.width, 200.0);
    assert_eq!(inherited.children[1].geometry.size.width, 128.0);
    assert_eq!(inherited.children[2].position.x, 168.0);
    assert_eq!(
        family
            .query_at(Level::Strong, available)
            .unwrap()
            .size
            .width,
        24.0
    );
}

fn box_at_levels() -> Child<'static> {
    levels([80.0, 50.0, 50.0, 50.0], 20.0)
}

fn pair_row() -> Child<'static> {
    row_child(vec![box_at_levels(), box_at_levels()], 0.0)
}

fn pair_wrap() -> Child<'static> {
    wrap_child(vec![box_at_levels(), box_at_levels()], 0.0)
}

#[test]
fn stage_order_controls_whether_layout_or_compaction_happens_first() {
    let layout_first = vec![
        Stage::new(Level::Preferred, pair_row()),
        Stage::new(Level::Preferred, pair_wrap()),
        Stage::new(Level::Mild, pair_row()),
    ];
    let compact_first = vec![
        Stage::new(Level::Preferred, pair_row()),
        Stage::new(Level::Mild, pair_row()),
        Stage::new(Level::Preferred, pair_wrap()),
    ];
    let available = Size::new(100.0, 40.0);
    let layout = choose_stage(&layout_first, available).unwrap();
    assert_eq!(layout.stage_index, 1);
    assert_eq!(layout.level, Level::Preferred);
    let compact = choose_stage(&compact_first, available).unwrap();
    assert_eq!(compact.stage_index, 1);
    assert_eq!(compact.level, Level::Mild);
}

#[test]
fn width_fitting_wrap_is_rejected_when_its_total_height_does_not_fit() {
    let alternatives = vec![
        Stage::new(Level::Preferred, pair_wrap()),
        Stage::new(Level::Mild, pair_row()),
    ];
    let selected = choose_stage(&alternatives, Size::new(100.0, 25.0)).unwrap();
    assert_eq!(selected.stage_index, 1);
    assert_eq!(selected.level, Level::Mild);
    assert!(choose_stage(&alternatives[..1], Size::new(100.0, 25.0)).is_err());
}

#[test]
fn hidden_child_costs_no_width_or_spacing_and_primary_remains() {
    let optional = block(30.0, 20.0).hidden_when(|level| level >= Level::Strong);
    let row = row_child(vec![block(40.0, 20.0), optional, block(20.0, 20.0)], 5.0);
    let shown = row
        .query_at(Level::Preferred, Size::new(200.0, 40.0))
        .unwrap();
    let hidden = row.query_at(Level::Strong, Size::new(200.0, 40.0)).unwrap();
    assert_eq!(shown.size.width, 100.0);
    assert_eq!(hidden.size.width, 65.0);
    assert_eq!(hidden.size.height, 20.0);
}

#[test]
fn nested_groups_receive_the_exact_level_and_wrap_as_immediate_children() {
    let nested = row_child(vec![box_at_levels(), box_at_levels()], 0.0);
    let outer = wrap_child(vec![nested, block(40.0, 20.0)], 0.0);
    let geometry = outer.query_at(Level::Mild, Size::new(120.0, 40.0)).unwrap();
    assert_eq!(geometry.size.height, 40.0);
    assert!(
        outer
            .query_at(Level::Preferred, Size::new(120.0, 40.0))
            .is_err()
    );
}

fn scrolling_content() -> Child<'static> {
    scroll_child(block_leaf(500.0, 500.0), Size::new(80.0, 40.0))
}

#[test]
fn scroll_viewport_must_fit_its_declared_outer_minimum_not_all_content() {
    let scroll = scrolling_content();
    assert!(
        scroll
            .query_at(Level::Preferred, Size::new(79.0, 100.0))
            .is_err()
    );
    assert!(
        scroll
            .query_at(Level::Preferred, Size::new(100.0, 39.0))
            .is_err()
    );
    assert!(
        scroll
            .query_at(Level::Preferred, Size::new(80.0, 40.0))
            .is_ok()
    );
}

#[test]
fn each_stage_restarts_allocation_and_resize_restores_preferred_forms() {
    let title = || block(100.0, 20.0).minimum_width(|_, _| 40.0);
    let menu = || levels([100.0, 40.0, 40.0, 40.0], 20.0);
    let make_row = || row_child(vec![title(), menu()], 0.0);
    let alternatives = vec![
        Stage::new(Level::Preferred, make_row()),
        Stage::new(Level::Mild, make_row()),
    ];
    let narrow = choose_stage(&alternatives, Size::new(130.0, 40.0)).unwrap();
    assert_eq!(narrow.level, Level::Mild);
    assert!((narrow.geometry.children[0].geometry.size.width - 90.0).abs() < 0.01);
    let wide = choose_stage(&alternatives, Size::new(200.0, 40.0)).unwrap();
    assert_eq!(wide.level, Level::Preferred);
    assert_eq!(wide.geometry.children[0].geometry.size.width, 100.0);
    assert_eq!(wide.geometry.children[1].geometry.size.width, 100.0);
}

#[test]
fn nested_scroll_viewport_respects_the_remaining_column_height() {
    let column = column_child(vec![block(80.0, 20.0), scrolling_content()], 5.0);
    assert!(
        column
            .query_at(Level::Preferred, Size::new(100.0, 64.0))
            .is_err()
    );
    let geometry = column
        .query_at(Level::Preferred, Size::new(100.0, 75.0))
        .unwrap();
    assert!(geometry.size.height <= 75.0);
    assert!(geometry.children[1].geometry.size.height >= 40.0);
    assert!(geometry.children[1].geometry.size.height < 500.0);
}

#[test]
fn entirely_hidden_nested_group_does_not_leave_an_outer_gap() {
    let hidden = row_child(vec![block(30.0, 20.0).hidden_when(|_| true)], 8.0);
    let row = row_child(vec![block(40.0, 20.0), hidden, block(20.0, 20.0)], 5.0);
    let plan = row
        .query_at(Level::Preferred, Size::new(200.0, 40.0))
        .unwrap();
    assert_eq!(plan.size.width, 65.0);
    assert_eq!(plan.children.len(), 2);
}

#[test]
fn identities_are_carried_by_plan_nodes_and_found() {
    let title = block(60.0, 20.0).identity(Id::new("title"), Some(Source::new("/title/text")));
    let family = forms(vec![
        (Level::Preferred, title),
        (
            Level::Strong,
            block(20.0, 20.0).identity(Id::new("short"), None),
        ),
    ])
    .identity(Id::new("family"), None);
    let row = row_child(vec![block(30.0, 10.0), family], 10.0);
    let plan = row
        .query_at(Level::Preferred, Size::new(200.0, 40.0))
        .unwrap();
    let (position, node) = plan.find(&Id::new("title")).unwrap();
    assert_eq!(position, Point::new(40.0, 0.0));
    assert_eq!(node.size, Size::new(60.0, 20.0));
    assert_eq!(
        node.identities(),
        &[
            (Id::new("title"), Some(Source::new("/title/text"))),
            (Id::new("family"), None),
        ]
    );
    assert!(plan.find(&Id::new("short")).is_none());
    let strong = row.query_at(Level::Strong, Size::new(200.0, 40.0)).unwrap();
    assert!(strong.find(&Id::new("short")).is_some());
    assert!(strong.find(&Id::new("family")).is_some());
}

#[test]
fn no_fit_names_each_stages_error_and_the_nearest_identified_child() {
    // A nested child's own failure is named by it; a failure of the whole
    // (the root's allocation, a column's heights) by the nearest identified
    // child on the way, if any.
    let fixed = block(30.0, 20.0)
        .fixed_dimensions(None, Some(50.0))
        .identity(Id::new("fixed"), None);
    let wide = block(120.0, 20.0).identity(Id::new("wide"), None);
    let anonymous = row_child(vec![block(120.0, 20.0)], 0.0).identity(Id::new("group"), None);
    let tall = column_child(vec![block(40.0, 60.0)], 0.0);
    let stages = vec![
        Stage::new(Level::Preferred, row_child(vec![fixed], 0.0)),
        Stage::new(Level::Mild, wide),
        Stage::new(Level::Strong, anonymous),
        Stage::new(Level::AllPermitted, tall),
    ];
    let failure = choose_stage(&stages, Size::new(100.0, 40.0)).unwrap_err();
    assert_eq!(failure.attempted, 4);
    assert_eq!(
        failure.failures,
        vec![
            StageFailure {
                stage_index: 0,
                level: Level::Preferred,
                error: AllocationError::NoFit {
                    required: 50.0,
                    available: 40.0
                },
                child: Some(Id::new("fixed")),
            },
            StageFailure {
                stage_index: 1,
                level: Level::Mild,
                error: AllocationError::NoFit {
                    required: 120.0,
                    available: 100.0
                },
                child: Some(Id::new("wide")),
            },
            StageFailure {
                stage_index: 2,
                level: Level::Strong,
                error: AllocationError::NoFit {
                    required: 120.0,
                    available: 100.0
                },
                child: Some(Id::new("group")),
            },
            StageFailure {
                stage_index: 3,
                level: Level::AllPermitted,
                error: AllocationError::NoFit {
                    required: 60.0,
                    available: 40.0
                },
                child: None,
            },
        ]
    );
}

#[test]
fn information_loss_is_reported_for_the_planned_form_and_gathered() {
    let label = block(80.0, 20.0)
        .minimum_width(|_, _| 30.0)
        .identity(Id::new("label"), None)
        .report_information(|_, plan| {
            if plan.size.width < 80.0 {
                vec![InformationLoss {
                    identity: Id::new("label/text"),
                    kind: LossKind::Shortened,
                }]
            } else {
                Vec::new()
            }
        });
    let row = row_child(vec![label, block(20.0, 20.0)], 0.0);
    let full = row
        .query_at(Level::Preferred, Size::new(200.0, 20.0))
        .unwrap();
    let reports = full.reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].id, Some(Id::new("label")));
    assert!(reports[0].losses.is_empty());
    let short = row
        .query_at(Level::Preferred, Size::new(70.0, 20.0))
        .unwrap();
    let reports = short.reports();
    assert_eq!(reports[0].size.width, 50.0);
    assert_eq!(
        reports[0].losses,
        vec![InformationLoss {
            identity: Id::new("label/text"),
            kind: LossKind::Shortened,
        }]
    );
    let hidden = block(10.0, 10.0)
        .hidden_when(|_| true)
        .report_information(|_, _| Vec::new());
    assert!(
        hidden
            .query_at(Level::Preferred, Size::new(10.0, 10.0))
            .unwrap()
            .reports()
            .is_empty()
    );
}

#[test]
fn a_frame_counts_its_chrome_inside_the_fit() {
    let framed = framed_child(block(40.0, 20.0).minimum_width(|_, _| 20.0), [4.0, 6.0]);
    let measured = framed.query(Level::Preferred);
    assert_eq!(measured.preferred, Size::new(52.0, 28.0));
    assert_eq!(measured.minimum, Size::new(32.0, 28.0));
    let plan = framed
        .query_at(Level::Preferred, Size::new(40.0, 30.0))
        .unwrap();
    assert_eq!(plan.size, Size::new(40.0, 28.0));
    assert_eq!(plan.children[0].position, Point::new(6.0, 4.0));
    assert_eq!(plan.children[0].geometry.size.width, 28.0);
    assert!(
        framed
            .query_at(Level::Preferred, Size::new(31.0, 30.0))
            .is_err()
    );
}
