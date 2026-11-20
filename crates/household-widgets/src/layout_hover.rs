// SPDX-License-Identifier: GPL-3.0-only

//! Informational attachments prefer obstacle-free regions before covering menus.
//!
//! Where a hover card goes is the engine's (`household_layout::hover`, which
//! says how a card is placed: one rule for every card, what counts as an
//! obstacle, clear regions first). Here the interface's rectangles convert
//! to the engine's at the edge, and the chosen plan is bound to its stage's
//! child; committing the card, region by region, is the widgets'.
use super::*;

/// Query the declared candidate order against unobstructed regions first,
/// then against the whole window. Popup rectangles are finite obstacles
/// (`household_layout::choose_hover`).
pub fn choose_hover<'a, Message: 'a>(
    stages: &[Stage<'a, Message>],
    renderer: &Renderer,
    window: Rectangle,
    attachment: &Attachment,
    obstacles: &[Rectangle],
) -> Result<AttachedPlan, NoFit> {
    let obstacles: Vec<_> = obstacles.iter().copied().map(core_rect).collect();
    household_layout::choose_hover(
        stages,
        renderer,
        core_rect(window),
        &attachment.to_core(),
        &obstacles,
    )
    .map(|chosen| AttachedPlan::from_core(stages, &chosen))
}

#[derive(Default)]
pub(crate) struct HoverRetained {
    geometry: RetainedGeometry,
    stage: Option<usize>,
}
impl HoverRetained {
    pub(crate) fn clear(&mut self) {
        self.geometry.clear();
        self.stage = None;
    }
}

// The engine's search for a card's place (`household_layout::place_hover`),
// with each region it offers committed here as commit_attached_retaining
// commits a popup; the first committed card clear of the obstacles (or, in
// the fallback pass, any committed card) is kept.
#[expect(clippy::too_many_arguments)]
pub(crate) fn commit_hover_retaining<'a, Message: 'a>(
    stages: &[Stage<'a, Message>],
    renderer: &Renderer,
    window: Rectangle,
    attachment: &Attachment,
    obstacles: &[Rectangle],
    content: &mut Element<'a, Message>,
    tree: &mut Tree,
    retained: &mut HoverRetained,
) -> Result<(AttachedPlan, layout::Node), NoFit> {
    let obstacles: Vec<_> = obstacles.iter().copied().map(core_rect).collect();
    let placed = household_layout::place_hover(
        stages,
        renderer,
        core_rect(window),
        &attachment.to_core(),
        &obstacles,
        |stage_index, stage, region, a| {
            // A retained one-stage plan uses a local stage index of zero;
            // invalidate it when the selected representation changes.
            if retained.stage != Some(stage_index) {
                retained.clear();
            }
            commit_attached_retaining(
                stage,
                renderer,
                toolkit_rect(region),
                &Attachment::from_core(a),
                content,
                tree,
                &mut retained.geometry,
            )
            .ok()
        },
        |(plan, _)| core_rect(plan.bounds),
    );
    match placed {
        Ok(household_layout::HoverChoice {
            value: (mut plan, node),
            stage_index,
            candidate_index,
        }) => {
            retained.stage = Some(stage_index);
            plan.stage.stage_index = stage_index;
            plan.candidate_index = candidate_index;
            Ok((plan, node))
        }
        Err(error) => {
            retained.clear();
            Err(error)
        }
    }
}

/// Whether `a` and `b` overlap with an area: the engine's
/// (`household_layout::hover::intersects`), on iced's rectangles.
#[cfg(test)]
fn intersects(a: Rectangle, b: Rectangle) -> bool {
    household_layout::hover::intersects(core_rect(a), core_rect(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::iced::advanced::renderer::Headless;
    use cosmic::iced::{Font, Pixels, Point};

    fn renderer() -> Renderer {
        cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
            Font::default(),
            Pixels(14.0),
            Some("tiny-skia"),
        ))
        .expect("headless renderer")
    }
    fn r(x: f32, y: f32, width: f32, height: f32) -> Rectangle {
        Rectangle {
            x,
            y,
            width,
            height,
        }
    }
    fn attachment(anchor: Rectangle) -> Attachment {
        Attachment::new(
            anchor,
            vec![
                AnchorCandidate::new(Side::Right, Alignment::Start),
                AnchorCandidate::new(Side::Left, Alignment::Start),
                AnchorCandidate::new(Side::Above, Alignment::End),
            ],
        )
    }
    #[test]
    fn finite_obstacles_leave_space_beyond_their_short_span() {
        let renderer = renderer();
        let stages: Vec<Stage<'_, ()>> = vec![Stage::new(
            Level::Preferred,
            child(|_| widget::space().width(60).height(30).into()),
        )];
        let window = r(0.0, 0.0, 300.0, 200.0);
        let source = r(100.0, 120.0, 20.0, 20.0);
        let obstacle = r(120.0, 0.0, 100.0, 100.0);
        let plan =
            choose_hover(&stages, &renderer, window, &attachment(source), &[obstacle]).unwrap();
        assert_eq!(plan.candidate_index, 0);
        assert!(!intersects(plan.bounds, obstacle));
        // Right of the source, centred on it vertically
        assert_eq!(plan.bounds.position(), Point::new(120.0, 115.0));
    }
    #[test]
    fn native_wrapped_card_falls_back_over_menu_when_clear_width_is_below_floor() {
        let renderer = renderer();
        let text =
            "A useful explanation wraps into several lines when the available width is limited.";
        let stages: Vec<Stage<'_, ()>> = vec![Stage::new(
            Level::Preferred,
            child(move |_| {
                widget::container(widget::text(text))
                    .width(Length::Shrink)
                    .into()
            })
            .minimum_width(|_, _| 100.0),
        )];
        let window = r(0.0, 0.0, 240.0, 180.0);
        let source = r(10.0, 10.0, 20.0, 20.0);
        let obstacle = r(90.0, 0.0, 150.0, 180.0);
        let a = Attachment::new(
            source,
            vec![AnchorCandidate::new(Side::Right, Alignment::Start)],
        );
        let plan = choose_hover(&stages, &renderer, window, &a, &[obstacle]).unwrap();
        assert!(intersects(plan.bounds, obstacle));
        assert!(plan.bounds.x >= 0.0 && plan.bounds.y >= 0.0);
        assert!(
            plan.bounds.x + plan.bounds.width <= window.width
                && plan.bounds.y + plan.bounds.height <= window.height
        );
        let mut body = widget::space().into();
        let (_, node) = commit_hover_retaining(
            &stages,
            &renderer,
            window,
            &a,
            &[obstacle],
            &mut body,
            &mut Tree::empty(),
            &mut HoverRetained::default(),
        )
        .unwrap();
        assert_eq!(node.bounds(), plan.bounds);
    }
    #[test]
    fn no_undeclared_flip_or_clipping() {
        let renderer = renderer();
        let stages: Vec<Stage<'_, ()>> = vec![Stage::new(
            Level::Preferred,
            child(|_| widget::space().width(100).height(50).into()),
        )];
        let a = Attachment::new(
            r(5.0, 5.0, 10.0, 10.0),
            vec![AnchorCandidate::new(Side::Above, Alignment::Start)],
        );
        assert!(choose_hover(&stages, &renderer, r(0.0, 0.0, 300.0, 200.0), &a, &[]).is_err());
    }
    #[test]
    fn declared_policy_orders_stages_and_candidates_within_clear_pass() {
        let renderer = renderer();
        let stages: Vec<Stage<'_, ()>> = vec![
            Stage::new(
                Level::Preferred,
                child(|_| widget::space().width(100).height(80).into()),
            ),
            Stage::new(
                Level::Strong,
                child(|_| widget::space().width(100).height(40).into()),
            ),
        ];
        let window = r(0.0, 0.0, 200.0, 200.0);
        let a = Attachment::new(
            r(0.0, 140.0, 10.0, 10.0),
            vec![
                AnchorCandidate::new(Side::Below, Alignment::Start),
                AnchorCandidate::new(Side::Above, Alignment::Start),
            ],
        );
        let preferred = choose_hover(&stages, &renderer, window, &a, &[]).unwrap();
        assert_eq!(
            (preferred.stage.stage_index, preferred.candidate_index),
            (0, 1)
        );
        let anchored = choose_hover(
            &stages,
            &renderer,
            window,
            &a.preference(PlacementPreference::AnchorFirst),
            &[],
        )
        .unwrap();
        assert_eq!(
            (anchored.stage.stage_index, anchored.candidate_index),
            (1, 0)
        );
    }
    #[test]
    fn bounded_cross_origin_avoids_a_finite_obstacle_without_changing_side() {
        let renderer = renderer();
        let window = r(0.0, 0.0, 300.0, 200.0);
        let obstacle = r(80.0, 0.0, 140.0, 80.0);
        let a = Attachment::new(
            r(80.0, 100.0, 20.0, 20.0),
            vec![AnchorCandidate::new(Side::Above, Alignment::StartBounded)],
        );
        let clear: Vec<Stage<'_, ()>> = vec![Stage::new(
            Level::Preferred,
            child(|_| widget::space().width(60).height(30).into()),
        )];
        let plan = choose_hover(&clear, &renderer, window, &a, &[obstacle]).unwrap();
        assert!(!intersects(plan.bounds, obstacle));
        assert_eq!(plan.bounds.y, 70.0);
        let wide: Vec<Stage<'_, ()>> = vec![Stage::new(
            Level::Preferred,
            child(|_| widget::space().width(100).height(30).into()).minimum_width(|_, _| 100.0),
        )];
        let fallback = choose_hover(&wide, &renderer, window, &a, &[obstacle]).unwrap();
        assert!(intersects(fallback.bounds, obstacle));
        assert_eq!(fallback.bounds.y, 70.0);
    }
    #[test]
    fn left_corridor_keeps_hover_flush_and_centred_on_menu_row() {
        let renderer = renderer();
        let stages: Vec<Stage<'_, ()>> = vec![Stage::new(
            Level::Preferred,
            child(|_| {
                widget::container(widget::text("Disabled action explanation"))
                    .width(168)
                    .height(40)
                    .into()
            })
            .minimum_width(|_, _| 168.0),
        )];
        let window = r(0.0, 0.0, 520.0, 700.0);
        let menu = r(272.0, 415.0, 240.0, 94.0);
        let source = r(276.0, 447.0, 232.0, 28.0);
        let attachment = Attachment::new(
            source,
            [Side::Right, Side::Left, Side::Above, Side::Below]
                .into_iter()
                .map(|side| AnchorCandidate::new(side, Alignment::StartBounded).gap(4.0))
                .collect(),
        );
        let selected = choose_hover(&stages, &renderer, window, &attachment, &[menu]).unwrap();
        assert_eq!(selected.candidate_index, 1);
        // flush with the row (not the menu), centred on it
        assert_eq!(selected.bounds, r(108.0, 441.0, 168.0, 40.0));
        let mut body = widget::space().into();
        let (committed, node) = commit_hover_retaining(
            &stages,
            &renderer,
            window,
            &attachment,
            &[menu],
            &mut body,
            &mut Tree::empty(),
            &mut HoverRetained::default(),
        )
        .unwrap();
        assert_eq!(committed, selected);
        assert_eq!(node.bounds(), selected.bounds);
    }
    fn card(width: f32, height: f32) -> Vec<Stage<'static, ()>> {
        vec![Stage::new(
            Level::Preferred,
            child(move |_| widget::space().width(width).height(height).into())
                .minimum_width(move |_, _| width),
        )]
    }
    fn placed(source: Rectangle, side: Side, declared: Alignment, gap: f32) -> Rectangle {
        let a = Attachment::new(source, vec![AnchorCandidate::new(side, declared).gap(gap)]);
        choose_hover(
            &card(60.0, 30.0),
            &renderer(),
            r(0.0, 0.0, 400.0, 300.0),
            &a,
            &[],
        )
        .unwrap()
        .bounds
    }
    #[test]
    fn a_card_above_a_control_is_centred_on_it_and_flush() {
        let source = r(100.0, 150.0, 40.0, 20.0);
        // whatever the source declared, and even with a declared gap
        for declared in [Alignment::Start, Alignment::StartBounded, Alignment::Center] {
            let bounds = placed(source, Side::Above, declared, 4.0);
            assert_eq!(bounds.x + bounds.width / 2.0, 120.0, "{declared:?}");
            assert_eq!(bounds.y + bounds.height, source.y, "no gap: {declared:?}");
            let below = placed(source, Side::Below, declared, 4.0);
            assert_eq!(below.x + below.width / 2.0, 120.0);
            assert_eq!(below.y, source.y + source.height);
        }
    }
    #[test]
    fn a_card_near_the_window_edge_shifts_inward_only_as_far_as_needed() {
        let left = placed(
            r(0.0, 150.0, 20.0, 20.0),
            Side::Above,
            Alignment::StartBounded,
            0.0,
        );
        assert_eq!(left.x, 0.0);
        let right = placed(
            r(370.0, 150.0, 20.0, 20.0),
            Side::Above,
            Alignment::StartBounded,
            0.0,
        );
        assert_eq!(right.x + right.width, 400.0);
        // a little off centre shifts by just the overflow
        let near = placed(
            r(10.0, 150.0, 20.0, 20.0),
            Side::Below,
            Alignment::Start,
            0.0,
        );
        assert_eq!(near.x, 0.0);
        let near = placed(
            r(30.0, 150.0, 20.0, 20.0),
            Side::Below,
            Alignment::Start,
            0.0,
        );
        assert_eq!(near.x + near.width / 2.0, 40.0);
    }
    #[test]
    fn a_card_beside_a_control_is_centred_vertically_and_flush() {
        let source = r(200.0, 100.0, 40.0, 20.0);
        let right = placed(source, Side::Right, Alignment::StartBounded, 4.0);
        assert_eq!(right.x, source.x + source.width);
        assert_eq!(right.y + right.height / 2.0, 110.0);
        let left = placed(source, Side::Left, Alignment::Start, 4.0);
        assert_eq!(left.x + left.width, source.x);
        assert_eq!(left.y + left.height / 2.0, 110.0);
    }
}
