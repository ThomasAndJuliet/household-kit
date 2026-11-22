// SPDX-License-Identifier: GPL-3.0-only

//! The application rebuilds between individual pointer events. Named popup
//! controls must reclaim their native state during UserInterface::build, before
//! its temporary named-state map is cleared and overlay layout begins.

use super::*;
use crate::layout::{Alignment, AnchorCandidate, Level, Side, child};
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::{Font, Pixels, Point};
use cosmic::iced::runtime::{UserInterface, user_interface};
use cosmic::iced::widget as native;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Message {
    Expand,
    Choose,
}

fn view(anchors: &Anchors, expanded: bool) -> Host<'static, Message> {
    let source = anchors.trigger("source", native::space().width(40).height(20));
    let base = native::column(vec![native::space().height(160).into(), source]);
    let content = child(move |_| {
        let mut rows: Vec<Element<'static, Message>> = vec![
            native::container(
                widget::button::custom(native::space().width(140).height(28))
                    .id(Id::new("expand"))
                    .on_press(Message::Expand),
            )
            .padding([4, 8])
            .into(),
        ];
        if expanded {
            rows.push(
                native::container(
                    widget::button::custom(native::space().width(140).height(28))
                        .id(Id::new("choice"))
                        .on_press(Message::Choose),
                )
                .padding([4, 8])
                .into(),
            );
        }
        rows.push(
            native::container(
                widget::button::custom(native::space().width(140).height(28))
                    .id(Id::new("disabled")),
            )
            .padding([4, 8])
            .into(),
        );
        native::container(native::column(rows)).width(180).into()
    });
    let attachment = Attachment::new(
        Rectangle::default(),
        vec![AnchorCandidate::new(Side::Above, Alignment::StartBounded)],
    );
    host(base).anchors(anchors.clone()).chain(vec![
        Panel::new(attachment, vec![Stage::new(Level::Preferred, content)])
            .source("source")
            .key(1),
    ])
}

const WINDOW: Size = Size::new(320.0, 240.0);

#[derive(Default)]
struct Find {
    target: Option<Id>,
    bounds: Option<Rectangle>,
}
impl Operation for Find {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, _: &mut dyn operation::Focusable) {
        if id == self.target.as_ref() {
            self.bounds = Some(bounds);
        }
    }
}
fn bounds(
    anchors: &Anchors,
    expanded: bool,
    cache: user_interface::Cache,
    renderer: &mut Renderer,
    target: &str,
) -> (user_interface::Cache, Rectangle) {
    let mut ui = UserInterface::build(view(anchors, expanded), WINDOW, cache, renderer);
    let mut find = Find {
        target: Some(Id::new(target.to_owned())),
        bounds: None,
    };
    ui.operate(renderer, &mut find);
    (
        ui.into_cache(),
        find.bounds.expect("selected popup control"),
    )
}
fn click(
    anchors: &Anchors,
    expanded: bool,
    mut cache: user_interface::Cache,
    renderer: &mut Renderer,
    point: Point,
) -> (user_interface::Cache, Vec<Message>) {
    let mut messages = Vec::new();
    for event in [
        Event::Mouse(mouse::Event::CursorMoved { position: point }),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ] {
        let mut ui = UserInterface::build(view(anchors, expanded), WINDOW, cache, renderer);
        ui.update(
            &[event],
            mouse::Cursor::Available(point),
            renderer,
            &mut cosmic::iced::advanced::clipboard::Null,
            &mut messages,
        );
        cache = ui.into_cache();
    }
    (cache, messages)
}

#[test]
fn named_popup_buttons_survive_per_event_rebuilds_and_expansion_requeries_geometry() {
    let mut renderer = cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
        Font::default(),
        Pixels(14.0),
        Some("tiny-skia"),
    ))
    .expect("headless tiny-skia renderer");
    let anchors = Anchors::default();
    let (cache, expand) = bounds(
        &anchors,
        false,
        user_interface::Cache::new(),
        &mut renderer,
        "expand",
    );
    let collapsed = anchors.surfaces()[0];
    let (cache, messages) = click(&anchors, false, cache, &mut renderer, expand.center());
    assert_eq!(
        messages,
        vec![Message::Expand],
        "container-wrapped native button keeps its pressed state across native UI rebuilds"
    );

    let (cache, choice) = bounds(&anchors, true, cache, &mut renderer, "choice");
    let expanded = anchors.surfaces()[0];
    assert!(
        expanded.height > collapsed.height,
        "state reconciliation never freezes old popup geometry"
    );
    assert_eq!(
        expanded.y + expanded.height,
        anchors.visible(&Id::new("source")).unwrap().y
    );
    let (cache, disabled) = bounds(&anchors, true, cache, &mut renderer, "disabled");
    let (cache, messages) = click(&anchors, true, cache, &mut renderer, disabled.center());
    assert!(
        messages.is_empty(),
        "disabled native controls stay inactive across rebuilds"
    );
    let (_, messages) = click(&anchors, true, cache, &mut renderer, choice.center());
    assert_eq!(
        messages,
        vec![Message::Choose],
        "the newly expanded native row dispatches once"
    );
}

/// A menu (`source` opens it) of one focusable row which is also the source
/// of a submenu. With `open` the submenu is open beside the menu, and the row
/// is `grown` wider, as a submenu's row is when its label turns semibold.
fn growing_view(anchors: &Anchors, open: bool) -> Host<'static, Message> {
    let source = anchors.trigger("source", native::space().width(40).height(20));
    let base = native::column(vec![native::space().height(20).into(), source]);
    let row_anchors = anchors.clone();
    let content = child(move |_| {
        let width = if open { 150.0 } else { 140.0 };
        let row = focus_target(
            "row",
            native::container(native::space().width(width).height(28)).padding([4, 8]),
        );
        native::container(native::column(vec![row_anchors.trigger("row-source", row)])).into()
    });
    let mut panels = vec![
        Panel::new(
            Attachment::new(
                Rectangle::default(),
                vec![AnchorCandidate::new(Side::Below, Alignment::StartBounded)],
            ),
            vec![Stage::new(Level::Preferred, content)],
        )
        .source("source")
        .key(1),
    ];
    if open {
        panels.push(
            Panel::new(
                Attachment::new(
                    Rectangle::default(),
                    vec![AnchorCandidate::new(Side::Right, Alignment::Start)],
                ),
                vec![Stage::new(
                    Level::Preferred,
                    child(|_| native::space().width(60).height(40).into()),
                )],
            )
            .source("row-source")
            .key(2),
        );
    }
    host(base).anchors(anchors.clone()).chain(panels)
}

fn frame(
    anchors: &Anchors,
    open: bool,
    cache: user_interface::Cache,
    renderer: &mut Renderer,
) -> user_interface::Cache {
    let mut ui = UserInterface::build(growing_view(anchors, open), WINDOW, cache, renderer);
    ui.update(
        &[Event::Window(cosmic::iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        ))],
        mouse::Cursor::Unavailable,
        renderer,
        &mut cosmic::iced::advanced::clipboard::Null,
        &mut Vec::new(),
    );
    ui.into_cache()
}

fn operate(
    anchors: &Anchors,
    open: bool,
    cache: user_interface::Cache,
    renderer: &mut Renderer,
    operation: &mut dyn Operation,
) -> user_interface::Cache {
    let mut ui = UserInterface::build(growing_view(anchors, open), WINDOW, cache, renderer);
    ui.operate(renderer, operation);
    ui.into_cache()
}

/// A menu wider in this view than its last plan, while its submenu opens
/// beside it: the menu's tree must still reclaim its named controls' state in
/// the build. It was left holding the build's extraction placeholders when
/// the old width no longer fitted, and the submenu's card then moved them
/// into the focus target as its state: "Downcast on stateless state" (the
/// menu bar's tests, in Open Sans, where an open row is semibold and wider).
#[test]
fn a_menu_grown_wider_as_its_submenu_opens_keeps_its_named_controls_state() {
    let mut renderer = cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
        Font::default(),
        Pixels(14.0),
        Some("tiny-skia"),
    ))
    .expect("headless tiny-skia renderer");
    let anchors = Anchors::default();
    let mut cache = user_interface::Cache::new();
    for _ in 0..2 {
        cache = frame(&anchors, false, cache, &mut renderer);
    }
    let narrow = anchors.surfaces()[0];
    cache = operate(
        &anchors,
        false,
        cache,
        &mut renderer,
        &mut operation::focusable::focus::<()>(Id::new("row")),
    );
    for _ in 0..2 {
        cache = frame(&anchors, true, cache, &mut renderer);
    }
    let surfaces = anchors.surfaces();
    assert_eq!(surfaces.len(), 2, "the submenu is open: {surfaces:?}");
    assert!(
        surfaces[0].width > narrow.width,
        "the menu is wider than its last plan: {surfaces:?} after {narrow:?}"
    );
    let mut probe = FocusProbe::default();
    let _ = operate(&anchors, true, cache, &mut renderer, &mut probe);
    assert_eq!(
        probe.focused,
        Some(Id::new("row")),
        "the row's focus is its state, kept across the rebuild"
    );
}
