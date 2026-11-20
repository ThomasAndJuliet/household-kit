// SPDX-License-Identifier: GPL-3.0-only

//! Placing attached surfaces and hover cards, with a host of plain boxes and
//! no toolkit: every side and alignment, the insets between a paint box and
//! its visible surface, an obstacle, a pair already rejected, both
//! preferences, and a hover card in the clear regions round two obstacles.

use household_layout::{
    Alignment, AnchorCandidate, Attachment, Child, Host, Level, NoFit, Offer, Padding,
    PlacementPreference, Point, Rectangle, Side, Size, Stage, choose_attached,
    choose_attached_avoiding, choose_attached_excluding, choose_hover,
};

/// A host whose leaves are boxes of a fixed size.
struct Boxes;

impl Host for Boxes {
    type Leaf = Size;
    type Context = ();
    type Note = ();
    type Chrome = ();

    fn size(leaf: &Size, _: Level, _: &(), _: Offer) -> Size {
        *leaf
    }
}

fn r(x: f32, y: f32, width: f32, height: f32) -> Rectangle {
    Rectangle::new(Point::new(x, y), Size::new(width, height))
}

fn panel(width: f32, height: f32) -> Vec<Stage<'static, Boxes>> {
    vec![Stage::new(
        Level::Preferred,
        Child::leaf(Size::new(width, height)),
    )]
}

fn one(anchor: Rectangle, side: Side, alignment: Alignment) -> Attachment {
    Attachment::new(anchor, vec![AnchorCandidate::new(side, alignment)])
}

const ALIGNMENTS: [Alignment; 5] = [
    Alignment::Start,
    Alignment::End,
    Alignment::Center,
    Alignment::StartBounded,
    Alignment::CenterBounded,
];

#[test]
fn every_side_and_alignment_places_the_surface_against_the_anchor() {
    // A panel 60 × 30 round an anchor 80 × 40 at (150, 100), with room on
    // every side: across, Start lines up the starts, End the ends, Center
    // the centres, and the bounded alignments, with nothing to avoid, are
    // where Start and Center are.
    let window = r(0.0, 0.0, 400.0, 300.0);
    let anchor = r(150.0, 100.0, 80.0, 40.0);
    let across_x = [150.0, 170.0, 160.0, 150.0, 160.0];
    let across_y = [100.0, 110.0, 105.0, 100.0, 105.0];
    for (alignment, (x, y)) in ALIGNMENTS
        .into_iter()
        .zip(across_x.into_iter().zip(across_y))
    {
        for (side, expected) in [
            (Side::Below, r(x, 140.0, 60.0, 30.0)),
            (Side::Above, r(x, 70.0, 60.0, 30.0)),
            (Side::Right, r(230.0, y, 60.0, 30.0)),
            (Side::Left, r(90.0, y, 60.0, 30.0)),
        ] {
            let placed = choose_attached(
                &panel(60.0, 30.0),
                &(),
                window,
                &one(anchor, side, alignment),
            )
            .unwrap();
            assert_eq!(placed.bounds, expected, "{side:?} {alignment:?}");
            assert_eq!(placed.candidate_index, 0);
            assert_eq!(placed.stage.plan.size(), Size::new(60.0, 30.0));
        }
    }
}

#[test]
fn a_gap_separates_the_surface_from_the_anchor_on_its_side() {
    let window = r(0.0, 0.0, 400.0, 300.0);
    let anchor = r(150.0, 100.0, 80.0, 40.0);
    for (side, expected) in [
        (Side::Below, r(150.0, 144.0, 60.0, 30.0)),
        (Side::Above, r(150.0, 66.0, 60.0, 30.0)),
        (Side::Right, r(234.0, 100.0, 60.0, 30.0)),
        (Side::Left, r(86.0, 100.0, 60.0, 30.0)),
    ] {
        let attachment = Attachment::new(
            anchor,
            vec![AnchorCandidate::new(side, Alignment::Start).gap(4.0)],
        );
        let placed = choose_attached(&panel(60.0, 30.0), &(), window, &attachment).unwrap();
        assert_eq!(placed.bounds, expected, "{side:?}");
    }
}

#[test]
fn near_the_window_edge_only_the_alignments_with_room_fit_and_nothing_is_moved_back() {
    // The anchor runs from 270 to 290 in a window 300 wide; the panel is 60
    // wide. Start is offered 30 and Center 40 (twice the 20 from the
    // anchor's centre to the window's edge): neither fits, and neither is
    // moved left to make it. End is offered 290 and ends where the anchor
    // ends. The bounded alignments are offered the whole width and shift
    // left only as far as they must.
    let window = r(0.0, 0.0, 300.0, 200.0);
    let anchor = r(270.0, 20.0, 20.0, 20.0);
    let stages = panel(60.0, 30.0);
    for alignment in [Alignment::Start, Alignment::Center] {
        assert_eq!(
            choose_attached(&stages, &(), window, &one(anchor, Side::Below, alignment)),
            Err(NoFit { attempted: 1 }),
            "{alignment:?}"
        );
    }
    for (alignment, x) in [
        (Alignment::End, 230.0),
        (Alignment::StartBounded, 240.0),
        (Alignment::CenterBounded, 240.0),
    ] {
        let placed =
            choose_attached(&stages, &(), window, &one(anchor, Side::Below, alignment)).unwrap();
        assert_eq!(placed.bounds, r(x, 40.0, 60.0, 30.0), "{alignment:?}");
    }
}

#[test]
fn the_visible_surface_attaches_and_the_whole_paint_box_must_fit() {
    // A shadow of 2 above, 4 to the right, 6 below and 8 to the left of the
    // visible surface: the surface's visible edge meets the anchor's, and
    // the paint box reaches beyond it by the insets.
    let window = r(0.0, 0.0, 400.0, 300.0);
    let anchor = r(150.0, 100.0, 80.0, 40.0);
    let insets = Padding {
        top: 2.0,
        right: 4.0,
        bottom: 6.0,
        left: 8.0,
    };
    for (side, expected) in [
        (Side::Below, r(142.0, 138.0, 60.0, 30.0)),
        (Side::Above, r(142.0, 76.0, 60.0, 30.0)),
        (Side::Right, r(222.0, 98.0, 60.0, 30.0)),
        (Side::Left, r(94.0, 98.0, 60.0, 30.0)),
    ] {
        let mut attachment = one(anchor, side, Alignment::Start);
        attachment.surface_insets = insets;
        let placed = choose_attached(&panel(60.0, 30.0), &(), window, &attachment).unwrap();
        assert_eq!(placed.bounds, expected, "{side:?}");
        let visible = r(
            placed.bounds.x + insets.left,
            placed.bounds.y + insets.top,
            placed.bounds.width - insets.left - insets.right,
            placed.bounds.height - insets.top - insets.bottom,
        );
        match side {
            Side::Below => assert_eq!(visible.y, anchor.y + anchor.height),
            Side::Above => assert_eq!(visible.y + visible.height, anchor.y),
            Side::Right => assert_eq!(visible.x, anchor.x + anchor.width),
            Side::Left => assert_eq!(visible.x + visible.width, anchor.x),
        }
    }
    // At the window's left edge the visible surface would fit, but its
    // shadow would not: the candidate does not fit.
    let mut at_edge = one(r(0.0, 40.0, 20.0, 20.0), Side::Below, Alignment::Start);
    at_edge.surface_insets = insets;
    assert!(choose_attached(&panel(60.0, 30.0), &(), window, &at_edge).is_err());
}

#[test]
fn an_obstacle_rejects_a_placement_that_overlaps_it_but_not_one_that_touches_it() {
    let window = r(0.0, 0.0, 300.0, 300.0);
    let anchor = r(100.0, 100.0, 40.0, 20.0);
    let attachment = Attachment::new(
        anchor,
        vec![
            AnchorCandidate::new(Side::Below, Alignment::Start),
            AnchorCandidate::new(Side::Above, Alignment::Start),
        ],
    );
    let stages = panel(60.0, 30.0);
    // Below would be (100, 120, 60, 30); an earlier surface lies over it.
    let over = r(80.0, 130.0, 100.0, 50.0);
    let placed = choose_attached_avoiding(&stages, &(), window, &attachment, &[over]).unwrap();
    assert_eq!(placed.candidate_index, 1);
    assert_eq!(placed.bounds, r(100.0, 70.0, 60.0, 30.0));
    // One that only meets its bottom edge is not overlapped.
    let touching = r(80.0, 150.0, 100.0, 50.0);
    let placed = choose_attached_avoiding(&stages, &(), window, &attachment, &[touching]).unwrap();
    assert_eq!(placed.candidate_index, 0);
}

#[test]
fn a_pair_already_rejected_is_skipped() {
    let window = r(0.0, 0.0, 300.0, 300.0);
    let attachment = Attachment::new(
        r(100.0, 100.0, 40.0, 20.0),
        vec![
            AnchorCandidate::new(Side::Below, Alignment::Start),
            AnchorCandidate::new(Side::Above, Alignment::Start),
        ],
    );
    let stages = panel(60.0, 30.0);
    let placed =
        choose_attached_excluding(&stages, &(), window, &attachment, &[(0, 0)], &[]).unwrap();
    assert_eq!((placed.stage.stage_index, placed.candidate_index), (0, 1));
    assert_eq!(
        choose_attached_excluding(&stages, &(), window, &attachment, &[(0, 0), (0, 1)], &[]),
        Err(NoFit { attempted: 2 })
    );
}

#[test]
fn the_preference_orders_stages_against_candidates() {
    // Below the anchor there are 50; above it, 140. The fuller stage is 80
    // high and fits only above; the compact one, 40 high, fits below too.
    let stages: Vec<Stage<'_, Boxes>> = vec![
        Stage::new(Level::Preferred, Child::leaf(Size::new(100.0, 80.0))),
        Stage::new(Level::Strong, Child::leaf(Size::new(100.0, 40.0))),
    ];
    let window = r(0.0, 0.0, 200.0, 200.0);
    let attachment = Attachment::new(
        r(0.0, 140.0, 10.0, 10.0),
        vec![
            AnchorCandidate::new(Side::Below, Alignment::Start),
            AnchorCandidate::new(Side::Above, Alignment::Start),
        ],
    );
    // The fullest representation first, on whichever side it fits.
    let representation = choose_attached(&stages, &(), window, &attachment).unwrap();
    assert_eq!(
        (
            representation.stage.stage_index,
            representation.stage.level,
            representation.candidate_index
        ),
        (0, Level::Preferred, 1)
    );
    assert_eq!(representation.bounds, r(0.0, 60.0, 100.0, 80.0));
    // The declared side first, with whichever representation fits there.
    let anchor = choose_attached(
        &stages,
        &(),
        window,
        &attachment.preference(PlacementPreference::AnchorFirst),
    )
    .unwrap();
    assert_eq!(
        (
            anchor.stage.stage_index,
            anchor.stage.level,
            anchor.candidate_index
        ),
        (1, Level::Strong, 0)
    );
    assert_eq!(anchor.bounds, r(0.0, 150.0, 100.0, 40.0));
}

#[test]
fn a_hover_card_takes_the_clear_region_between_two_obstacles() {
    // A card 60 × 30 to the right of a source 20 high centred at y = 100.
    // Centred on the source it would run from 85 to 115, over the menu
    // above (which ends at 100). The clear regions round the two menus
    // include one below the first and left of the second; there the card,
    // still flush with the source, shifts down only as far as it must.
    let window = r(0.0, 0.0, 300.0, 200.0);
    let source = r(100.0, 90.0, 20.0, 20.0);
    let above = r(120.0, 0.0, 100.0, 100.0);
    let beside = r(240.0, 100.0, 60.0, 100.0);
    let attachment = one(source, Side::Right, Alignment::Start);
    let placed = choose_hover(
        &panel(60.0, 30.0),
        &(),
        window,
        &attachment,
        &[above, beside],
    )
    .unwrap();
    assert_eq!(placed.bounds, r(120.0, 100.0, 60.0, 30.0));
    assert_eq!(placed.candidate_index, 0);
}

#[test]
fn a_hover_card_with_no_clear_room_covers_a_menu_rather_than_not_showing() {
    // The second menu leaves only 10 clear beside the source below the
    // first: too narrow for the card, which is placed in the whole window,
    // centred on the source, over the first menu.
    let window = r(0.0, 0.0, 300.0, 200.0);
    let source = r(100.0, 90.0, 20.0, 20.0);
    let above = r(120.0, 0.0, 100.0, 100.0);
    let beside = r(130.0, 100.0, 170.0, 100.0);
    let attachment = one(source, Side::Right, Alignment::Start);
    let placed = choose_hover(
        &panel(60.0, 30.0),
        &(),
        window,
        &attachment,
        &[above, beside],
    )
    .unwrap();
    assert_eq!(placed.bounds, r(120.0, 85.0, 60.0, 30.0));
}
