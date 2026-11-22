// SPDX-License-Identifier: GPL-3.0-only

//! Panel and pane composition using the ordinary layout engine.
//!
//! These constructors only arrange already prepared children. A caller owns
//! visibility, identity and persistence and prepares a new snapshot after an
//! interaction. Queries do not change those states.

use crate::layout::{Child, Level, column_child, forms, row_child};
use crate::panel_resize::Dimension;

/// The direction in which sibling panes and their divider are arranged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}

/// Stack an arbitrary header and body, each retaining its own layout rules.
pub fn panel<'a, Message: 'a>(
    header: Child<'a, Message>,
    body: Child<'a, Message>,
    spacing: f32,
) -> Child<'a, Message> {
    column_child(vec![header, body], spacing)
}

/// Apply prepared manual requests to an expanded panel. Automatic dimensions
/// retain the children's ordinary sizing. For explicit reopening, callers can
/// pass the temporary dimension returned by `panel_resize::reopen` while
/// retaining the original saved request in interaction state.
pub fn sized_panel<'a, Message: 'a>(
    header: Child<'a, Message>,
    body: Child<'a, Message>,
    spacing: f32,
    width: Dimension,
    height: Dimension,
) -> Child<'a, Message> {
    let requested = |dimension| match dimension {
        Dimension::Automatic => None,
        Dimension::Manual(value) => Some(value),
    };
    panel(header, body, spacing).fixed_dimensions(requested(width), requested(height))
}

/// Declare an expanded panel and a collapsed strip at an explicit level.
///
/// The root remains responsible for selecting levels. For a user-collapsed
/// pane use the strip directly until a deliberate expand or reveal prepares a
/// new snapshot; an automatic level selection must not undo that intent.
pub fn panel_forms<'a, Message: 'a>(
    header: Child<'a, Message>,
    body: Child<'a, Message>,
    strip: Child<'a, Message>,
    collapse_at: Level,
    spacing: f32,
) -> Child<'a, Message> {
    assert!(
        collapse_at > Level::Preferred,
        "collapse must follow the expanded form"
    );
    forms(vec![
        (Level::Preferred, panel(header, body, spacing)),
        (collapse_at, strip),
    ])
}

/// Arrange arbitrary panes and divider children along one axis.
///
/// Dividers are supplied by the caller and measured as ordinary children.
pub fn split<'a, Message: 'a>(
    axis: SplitAxis,
    children: Vec<Child<'a, Message>>,
    spacing: f32,
) -> Child<'a, Message> {
    match axis {
        SplitAxis::Horizontal => row_child(children, spacing),
        SplitAxis::Vertical => column_child(children, spacing),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Stage, child, choose_stage, stages};
    use cosmic::iced::advanced::layout;
    use cosmic::iced::advanced::widget::{Tree, Widget};
    use cosmic::iced::{Size, widget};
    use cosmic::{Renderer, Theme};

    fn renderer() -> Renderer {
        crate::test_fonts::renderer().expect("headless renderer")
    }

    fn box_child(width: f32, height: f32) -> Child<'static, ()> {
        child(move |_| widget::Space::new().width(width).height(height).into())
    }

    #[test]
    fn declared_collapse_is_selected_and_committed_without_mutating_snapshot() {
        let renderer = renderer();
        let pane = panel_forms(
            box_child(100.0, 20.0),
            box_child(100.0, 80.0),
            box_child(20.0, 20.0),
            Level::Strong,
            0.0,
        );
        let declared = || {
            vec![
                Stage::new(Level::Preferred, pane.clone()),
                Stage::new(Level::Strong, pane.clone()),
            ]
        };
        let small = Size::new(50.0, 100.0);
        let first = choose_stage(&declared(), &renderer, small).unwrap();
        assert_eq!(first.stage_index, 1);
        assert_eq!(first, choose_stage(&declared(), &renderer, small).unwrap());
        let mut root = stages(declared());
        let node = <_ as Widget<(), Theme, Renderer>>::layout(
            &mut root,
            &mut Tree::empty(),
            &renderer,
            &layout::Limits::new(Size::ZERO, small),
        );
        assert_eq!(root.selection().unwrap().as_ref().unwrap().stage_index, 1);
        assert_eq!(node.children()[0].size(), first.geometry.size);
        assert_eq!(
            choose_stage(&declared(), &renderer, Size::new(200.0, 200.0))
                .unwrap()
                .stage_index,
            0
        );
    }

    #[test]
    fn split_measures_and_commits_real_header_body_and_divider() {
        let renderer = renderer();
        for axis in [SplitAxis::Horizontal, SplitAxis::Vertical] {
            let composition = split(
                axis,
                vec![
                    panel(box_child(40.0, 10.0), box_child(40.0, 30.0), 0.0),
                    box_child(4.0, 4.0),
                    box_child(20.0, 20.0),
                ],
                0.0,
            );
            let offer = Size::new(200.0, 200.0);
            let plan = composition
                .query_at(Level::Preferred, &renderer, offer)
                .unwrap();
            assert_eq!(plan.children.len(), 3);
            let mut root = stages(vec![Stage::new(Level::Preferred, composition)]);
            let node = <_ as Widget<(), Theme, Renderer>>::layout(
                &mut root,
                &mut Tree::empty(),
                &renderer,
                &layout::Limits::new(Size::ZERO, offer),
            );
            assert!(root.selection().unwrap().is_ok());
            assert_eq!(node.children()[0].size(), plan.size);
        }
    }
    #[test]
    fn manual_sizes_collapse_and_reopen_without_overwriting_requests() {
        use crate::panel_resize::{Reopen, reopen};
        let renderer = renderer();
        for axis in [SplitAxis::Horizontal, SplitAxis::Vertical] {
            let saved = Dimension::Manual(120.0);
            let make = |effective| {
                let (width, height) = match axis {
                    SplitAxis::Horizontal => (effective, Dimension::Automatic),
                    SplitAxis::Vertical => (Dimension::Automatic, effective),
                };
                let expanded = sized_panel(
                    box_child(40.0, 10.0),
                    box_child(40.0, 30.0),
                    0.0,
                    width,
                    height,
                );
                forms(vec![
                    (Level::Preferred, expanded),
                    (Level::Strong, box_child(10.0, 10.0)),
                ])
            };
            let offer = match axis {
                SplitAxis::Horizontal => Size::new(80.0, 200.0),
                SplitAxis::Vertical => Size::new(200.0, 80.0),
            };
            let manual = make(saved);
            assert!(manual.query_at(Level::Preferred, &renderer, offer).is_err());
            let declared = || {
                vec![
                    Stage::new(Level::Preferred, manual.clone()),
                    Stage::new(Level::Strong, manual.clone()),
                ]
            };
            assert_eq!(
                choose_stage(&declared(), &renderer, offer)
                    .unwrap()
                    .stage_index,
                1
            );
            let mut root = stages(declared());
            let _ = <_ as Widget<(), Theme, Renderer>>::layout(
                &mut root,
                &mut Tree::empty(),
                &renderer,
                &layout::Limits::new(Size::ZERO, offer),
            );
            assert_eq!(root.selection().unwrap().as_ref().unwrap().stage_index, 1);
            let minimum = 40.0;
            let Reopen::Expanded(effective) = reopen(120.0, minimum, 80.0) else {
                panic!("minimum fits")
            };
            let reopened = make(Dimension::Manual(effective));
            let plan = reopened
                .query_at(Level::Preferred, &renderer, offer)
                .unwrap();
            let mut root = stages(vec![Stage::new(Level::Preferred, reopened)]);
            let node = <_ as Widget<(), Theme, Renderer>>::layout(
                &mut root,
                &mut Tree::empty(),
                &renderer,
                &layout::Limits::new(Size::ZERO, offer),
            );
            assert!(root.selection().unwrap().is_ok());
            assert_eq!(node.children()[0].size(), plan.size);
            assert_eq!(
                match axis {
                    SplitAxis::Horizontal => plan.size.width,
                    SplitAxis::Vertical => plan.size.height,
                },
                80.0
            );
            assert_eq!(saved, Dimension::Manual(120.0));
            assert_eq!(reopen(120.0, minimum, 39.0), Reopen::NoFit);
            let large = Size::new(200.0, 200.0);
            let restored = make(saved)
                .query_at(Level::Preferred, &renderer, large)
                .unwrap();
            assert_eq!(
                match axis {
                    SplitAxis::Horizontal => restored.size.width,
                    SplitAxis::Vertical => restored.size.height,
                },
                120.0
            );
        }
    }

    #[test]
    fn selected_form_preserves_child_information_reports() {
        use crate::layout::SourceId;
        let renderer = renderer();
        let header = box_child(40.0, 10.0).report_information(SourceId(1), 7, |_, _| vec![]);
        let body = box_child(40.0, 30.0).report_information(SourceId(2), 8, |_, _| vec![]);
        let strip = box_child(10.0, 10.0).report_information(SourceId(3), 9, |_, _| vec![]);
        let pane = panel_forms(header, body, strip, Level::Strong, 0.0);
        let expanded = pane
            .query_at(Level::Preferred, &renderer, Size::new(100.0, 100.0))
            .unwrap();
        assert_eq!(
            expanded
                .reports()
                .iter()
                .map(|r| r.source)
                .collect::<Vec<_>>(),
            vec![SourceId(1), SourceId(2)]
        );
        let collapsed = pane
            .query_at(Level::Strong, &renderer, Size::new(100.0, 100.0))
            .unwrap();
        assert_eq!(
            collapsed
                .reports()
                .iter()
                .map(|r| r.source)
                .collect::<Vec<_>>(),
            vec![SourceId(3)]
        );
    }
    #[test]
    fn accepted_regions_follow_the_selected_panel_form() {
        use crate::layout::{Region, RegionId, RegionReports, RegionRole};
        use cosmic::iced::advanced::Layout;
        let renderer = renderer();
        let region = |id| Region {
            id: RegionId(id),
            role: RegionRole::Section,
            revision: 1,
            section: None,
            column: None,
            order: None,
            viewport: None,
            clips: vec![],
        };
        let pane = panel_forms(
            box_child(40.0, 10.0).report_region(region(1)),
            box_child(40.0, 30.0).report_region(region(2)),
            box_child(10.0, 10.0).report_region(region(3)),
            Level::Strong,
            0.0,
        );
        let mut root = stages(vec![
            Stage::new(Level::Preferred, pane.clone()),
            Stage::new(Level::Strong, pane),
        ]);
        let mut tree = Tree::empty();
        for (offer, expected) in [
            (Size::new(100.0, 100.0), vec![RegionId(1), RegionId(2)]),
            (Size::new(20.0, 20.0), vec![RegionId(3)]),
        ] {
            let node = <_ as Widget<(), Theme, Renderer>>::layout(
                &mut root,
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, offer),
            );
            assert!(root.selection().unwrap().is_ok());
            let mut reports = RegionReports::default();
            <_ as Widget<(), Theme, Renderer>>::operate(
                &mut root,
                &mut tree,
                Layout::new(&node),
                &renderer,
                &mut reports,
            );
            assert_eq!(
                reports
                    .reports()
                    .iter()
                    .map(|r| r.region.id)
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }
}
