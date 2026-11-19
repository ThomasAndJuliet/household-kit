// SPDX-License-Identifier: GPL-3.0-only
//! Native panel composition fixture. Domain data here is illustrative; controls,
//! layout, popup and resizing are the same shared components used by an app.
use crate::panel_state::{
    ContentId, OwnerId, PaneId, PaneState, PanelCapabilities, PanelId, PanelState,
};
use crate::{
    layout as engine, menu_parts,
    navigation_style::{NavigationButton, Prominence},
    panel_divider::{Change, PanelDivider},
    panel_header,
    panel_layout::{self, SplitAxis},
    panel_resize::{Boundary, Dimension, DragUpdate, Requests},
    popup,
};
use cosmic::iced::advanced::{
    mouse,
    renderer::{Headless, Style},
};
use cosmic::iced::runtime::{UserInterface, user_interface};
use cosmic::iced::{Event, Font, Length, Pixels, Point, Rectangle, Size, window};
use cosmic::{Element, Renderer, Theme, widget};

#[derive(Clone, Debug, PartialEq)]
enum Message {
    Section(usize),
    HidePanel,
    ShowPanel,
    CollapseLower,
    ExpandLower,
    Menu(u64),
    Resize(Change),
    Dismiss,
    Close(usize),
    NoFit(usize),
}

fn icon(data: &'static [u8]) -> Element<'static, Message> {
    widget::icon::icon(widget::icon::from_svg_bytes(data).symbolic(true))
        .size(16)
        .into()
}
fn control(data: &'static [u8], message: Message) -> engine::Child<'static, Message> {
    engine::child(move |_| {
        widget::button::custom(icon(data))
            .class(cosmic::theme::Button::Icon)
            .padding(8)
            .on_press(message.clone())
            .into()
    })
}
fn navigation(selected: usize) -> engine::Child<'static, Message> {
    // These labels and icons match Terminal's PanelTab catalogue.
    let names: [(&str, &[u8]); 5] = [
        (
            "Plan",
            include_bytes!("../tests/panel-assets/plan-symbolic.svg"),
        ),
        (
            "Tools",
            include_bytes!("../tests/panel-assets/tool-calls-symbolic.svg"),
        ),
        (
            "Shells",
            include_bytes!("../tests/panel-assets/terminal-symbolic.svg"),
        ),
        (
            "Files",
            include_bytes!("../tests/panel-assets/x-office-document-symbolic.svg"),
        ),
        (
            "Attached",
            include_bytes!("../tests/panel-assets/mail-attachment-symbolic.svg"),
        ),
    ];
    engine::wrap_child(
        names
            .into_iter()
            .enumerate()
            .map(|(i, (name, asset))| {
                engine::child(move |_| {
                    NavigationButton::new(
                        widget::row![
                            icon(asset),
                            widget::text(name).font(cosmic::font::semibold()).size(14)
                        ]
                        .spacing(6),
                        Prominence::Prominent,
                        selected == i,
                    )
                    .padding([7, 9])
                    .on_press(Message::Section(i))
                    .into()
                })
                .report_region(region(10 + i as u64))
            })
            .collect(),
        2.0,
    )
    .grow(1.0)
}
fn region(id: u64) -> engine::Region {
    engine::Region {
        id: engine::RegionId(id),
        role: engine::RegionRole::Section,
        revision: 0,
        section: None,
        column: None,
        order: None,
        viewport: None,
        clips: vec![],
    }
}
fn menu_button() -> engine::Child<'static, Message> {
    control(
        include_bytes!("../tests/panel-assets/view-more-symbolic.svg"),
        Message::Menu(3),
    )
    .report_region(region(3))
}
fn toggle_button() -> engine::Child<'static, Message> {
    control(
        include_bytes!("../tests/panel-assets/panel-right-shown-symbolic.svg"),
        Message::HidePanel,
    )
    .report_region(region(4))
}
fn body(title: &'static str, detail: &'static str) -> engine::Child<'static, Message> {
    engine::child(move |_| {
        widget::container(
            widget::column![widget::text(title).size(18), widget::text(detail).size(14)]
                .spacing(12),
        )
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    })
    .minimum_width(|_, _| 140.0)
    .grow(1.0)
    .grow_height(1.0)
    .grow(1.0)
}
fn upper(selected: usize) -> engine::Child<'static, Message> {
    panel_layout::panel(
        panel_header::header(navigation(selected), menu_button(), toggle_button()),
        body(
            ["Plan", "Tools", "Shells", "Files", "Attached"][selected],
            [
                "Steps for this conversation.",
                "Recent tool calls.",
                "Shells used by this conversation.",
                "Files in the working folder.",
                "Files attached to this conversation.",
            ][selected],
        ),
        0.0,
    )
    .grow_height(1.0)
    .grow(1.0)
}
fn lower(collapsed: bool) -> engine::Child<'static, Message> {
    let heading = engine::child(|_| {
        NavigationButton::new(
            widget::row![
                icon(include_bytes!(
                    "../tests/panel-assets/ai-session-symbolic.svg"
                )),
                widget::text("Agents")
                    .font(cosmic::font::semibold())
                    .size(14)
            ]
            .spacing(6),
            Prominence::Prominent,
            true,
        )
        .padding([7, 9])
        .on_press(Message::ExpandLower)
        .into()
    });
    let header = panel_header::header(
        heading,
        control(
            include_bytes!("../tests/panel-assets/view-more-symbolic.svg"),
            Message::Menu(5),
        )
        .report_region(region(5)),
        control(
            if collapsed {
                include_bytes!("../tests/panel-assets/go-next-symbolic.svg")
            } else {
                include_bytes!("../tests/panel-assets/go-up-symbolic.svg")
            },
            if collapsed {
                Message::ExpandLower
            } else {
                Message::CollapseLower
            },
        )
        .report_region(region(6)),
    );
    if collapsed {
        header
    } else {
        panel_layout::panel(
            header,
            body("Agents", "Agents working on this conversation."),
            0.0,
        )
        .grow_height(1.0)
        .grow(1.0)
    }
}

struct Demo {
    panel: PanelState,
    pane: PaneState,
    selected: usize,
    requests: Requests,
    preview: Option<DragUpdate>,
    menu: Option<u64>,
    bounds: std::collections::HashMap<u64, Rectangle>,
    minimum: (f32, f32),
}
impl Default for Demo {
    fn default() -> Self {
        let mut panel = PanelState::new(
            PanelId(1),
            ContentId(1),
            OwnerId(1),
            PanelCapabilities::default(),
        );
        panel.set_displayed(true).unwrap();
        Self {
            panel,
            pane: PaneState::new(PanelId(1), PaneId(2)),
            selected: 0,
            requests: Requests::default(),
            preview: None,
            menu: None,
            bounds: Default::default(),
            minimum: (0.0, 0.0),
        }
    }
}
impl Demo {
    fn collapsed(&self) -> bool {
        self.pane.snapshot().user_collapsed
    }
    fn boundary(&self) -> Option<Boundary> {
        if self.collapsed() {
            return None;
        }
        Some(Boundary {
            before: self.bounds.get(&1)?.height,
            after: self.bounds.get(&2)?.height,
            before_min: self.minimum.0,
            after_min: self.minimum.1,
        })
    }
    fn view(&self) -> Element<'static, Message> {
        if !self.panel.snapshot().displayed {
            return engine::responsive_child(
                engine::Level::Preferred,
                panel_header::header(
                    engine::spacer(0.0),
                    engine::child(|_| widget::Space::new().into()),
                    control(
                        include_bytes!("../tests/panel-assets/panel-right-show-symbolic.svg"),
                        Message::ShowPanel,
                    )
                    .report_region(region(4)),
                ),
            )
            .width(Length::Fill)
            .into();
        }
        let dimension = |request| match request {
            Dimension::Automatic => None,
            Dimension::Manual(v) => Some(v),
        };
        let sizes = if self.collapsed() {
            (None, None)
        } else if let Some(p) = self.preview {
            (Some(p.before), Some(p.after))
        } else {
            (
                dimension(self.requests.before),
                dimension(self.requests.after),
            )
        };
        let upper = upper(self.selected)
            .fixed_dimensions(None, sizes.0)
            .report_region(region(1));
        let lower = lower(self.collapsed())
            .fixed_dimensions(None, sizes.1)
            .report_region(region(2));
        let mut children = vec![upper];
        if !self.collapsed() {
            let accepted = self.boundary();
            let requests = self.requests;
            children.push(
                engine::child(move |_| {
                    if let Some(boundary) = accepted {
                        PanelDivider::new(SplitAxis::Vertical, requests, boundary, Message::Resize)
                            .into()
                    } else {
                        // First layout only: same footprint, disabled until geometry is accepted.
                        widget::Space::new()
                            .height(crate::panel_divider::GRAB)
                            .width(Length::Fill)
                            .into()
                    }
                })
                .grow(1.0)
                .fixed_dimensions(None, Some(crate::panel_divider::GRAB))
                .grow(1.0)
                .report_region(region(7)),
            );
        }
        children.push(lower);
        let base = engine::responsive_child(
            engine::Level::Preferred,
            panel_layout::split(SplitAxis::Vertical, children, 0.0),
        )
        .width(Length::Fill)
        .height(Length::Fill);
        let mut host = popup::host(widget::container(base).class(panel_header::style()))
            .on_dismiss(Message::Dismiss)
            .on_close(Message::Close)
            .on_no_fit(Message::NoFit);
        if let Some(anchor) = self.menu.and_then(|id| self.bounds.get(&id)).copied() {
            let collapsed = self.collapsed();
            let menu = crate::popup_surface::reserve_shadow(
                engine::child(move |_| {
                    let action = if collapsed {
                        Message::ExpandLower
                    } else {
                        Message::CollapseLower
                    };
                    let label = if collapsed {
                        "Expand Agents pane"
                    } else {
                        "Collapse Agents pane"
                    };
                    let row = menu_parts::row_content(
                        menu_parts::RowSlots {
                            mark_width: None,
                            icon_width: None,
                            gap: 8.0,
                        },
                        None,
                        None,
                        widget::text(label).size(14).into(),
                        None,
                    );
                    widget::container(menu_parts::action_row_with(
                        row,
                        menu_parts::TooltipChoice::NoTooltip,
                        Some(action),
                        |b| b.padding([8, 12]),
                    ))
                    .padding(4)
                    .class(crate::popup_surface::style())
                    .into()
                })
                .report_region(region(8)),
            );
            host = host.chain(vec![popup::Panel::new(
                engine::Attachment::below(anchor),
                vec![engine::Stage::new(engine::Level::Preferred, menu)],
            )]);
        }
        host.into()
    }
    fn apply(&mut self, message: Message) {
        match message {
            Message::Section(i) => self.selected = i,
            Message::HidePanel => {
                self.panel.set_displayed(false).unwrap();
                self.menu = None;
            }
            Message::ShowPanel => {
                self.panel.set_displayed(true).unwrap();
            }
            Message::CollapseLower | Message::ExpandLower => {
                self.pane
                    .set_user_collapsed(message == Message::CollapseLower);
                self.menu = None;
                self.preview = None;
            }
            Message::Menu(id) => {
                self.menu = if self.menu == Some(id) {
                    None
                } else {
                    Some(id)
                }
            }
            Message::Resize(Change::Preview(p)) => self.preview = Some(p),
            Message::Resize(Change::Commit(r)) => {
                self.requests = r;
                self.preview = None;
            }
            Message::Resize(Change::Cancel) => self.preview = None,
            Message::Dismiss | Message::Close(_) => self.menu = None,
            Message::NoFit(i) => panic!("fixture popup {i} does not fit"),
        }
    }
}

struct Harness {
    model: Demo,
    renderer: Renderer,
    size: Size,
    cache: user_interface::Cache,
}
impl Harness {
    fn new(size: Size) -> Self {
        let renderer = cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
            Font::default(),
            Pixels(14.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        let mut result = Self {
            model: Demo::default(),
            renderer,
            size,
            cache: user_interface::Cache::new(),
        };
        result.dispatch(&[], mouse::Cursor::Unavailable);
        result
    }
    fn dispatch(&mut self, events: &[Event], cursor: mouse::Cursor) -> Vec<Message> {
        // Bootstrap/rebuild from the previous accepted geometry, as an app view does.
        let mut ui = UserInterface::build(
            self.model.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut messages = Vec::new();
        ui.update(
            events,
            cursor,
            &mut self.renderer,
            &mut cosmic::iced::advanced::clipboard::Null,
            &mut messages,
        );
        self.cache = ui.into_cache();
        for message in &messages {
            self.model.apply(message.clone());
        }
        let mut ui = UserInterface::build(
            self.model.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut reports = engine::RegionReports::default();
        ui.operate(&self.renderer, &mut reports);
        self.model.bounds = reports
            .reports()
            .iter()
            .map(|r| (r.region.id.0, r.bounds))
            .collect();
        self.cache = ui.into_cache();
        if self.model.panel.snapshot().displayed && !self.model.collapsed() {
            let offer = Size::new(self.size.width, self.size.height);
            self.model.minimum = (
                upper(self.model.selected)
                    .query_at(engine::Level::Preferred, &self.renderer, offer)
                    .unwrap()
                    .minimum_height(),
                lower(false)
                    .query_at(engine::Level::Preferred, &self.renderer, offer)
                    .unwrap()
                    .minimum_height(),
            );
        }
        messages
    }
    fn click(&mut self, id: u64) -> Vec<Message> {
        let p = self.model.bounds[&id].center();
        self.dispatch(
            &[
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ],
            mouse::Cursor::Available(p),
        )
    }
    fn capture(&mut self, name: &str) {
        let mut ui = UserInterface::build(
            self.model.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let theme = Theme::light();
        let colour = theme.cosmic().on_bg_color().into();
        let style = Style {
            icon_color: colour,
            text_color: colour,
            scale_factor: 1.0,
        };
        let mut messages = Vec::new();
        ui.update(
            &[Event::Window(window::Event::RedrawRequested(
                std::time::Instant::now(),
            ))],
            mouse::Cursor::Unavailable,
            &mut self.renderer,
            &mut cosmic::iced::advanced::clipboard::Null,
            &mut messages,
        );
        ui.draw(
            &mut self.renderer,
            &theme,
            &style,
            mouse::Cursor::Unavailable,
        );
        let rgba = self.renderer.screenshot(
            Size::new(self.size.width as u32, self.size.height as u32),
            1.0,
            theme.cosmic().bg_color().into(),
        );
        assert_eq!(
            rgba.len(),
            self.size.width as usize * self.size.height as usize * 4
        );
        assert!(messages.iter().all(|m| !matches!(m, Message::NoFit(_))));
        if let Ok(directory) = std::env::var("HOUSEHOLD_PANEL_FIXTURE_OUT") {
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(
                std::path::Path::new(&directory).join(format!(
                    "{name}-{}x{}.rgba",
                    self.size.width, self.size.height
                )),
                rgba,
            )
            .unwrap();
        }
        self.cache = ui.into_cache();
    }
}

#[test]
fn native_panel_fixture_frames() {
    let mut h = Harness::new(Size::new(720.0, 600.0));
    h.capture("expanded");
    assert!(h.click(6).contains(&Message::CollapseLower));
    assert!(h.model.collapsed());
    h.capture("collapsed");
    assert!(h.click(6).contains(&Message::ExpandLower));
    assert!(!h.model.collapsed());
    let mut narrow = Harness::new(Size::new(420.0, 600.0));
    assert!(narrow.click(3).contains(&Message::Menu(3)));
    narrow.capture("menu-narrow");
    assert!(narrow.click(8).contains(&Message::CollapseLower));
    assert!(narrow.model.collapsed());
    narrow.click(6);
    assert!(!narrow.model.collapsed());
}

#[test]
fn native_composition_preserves_drag_through_rebuild_and_restores_collapsed_pane() {
    let mut h = Harness::new(Size::new(720.0, 600.0));
    let original = h.model.boundary().unwrap();
    let start = h.model.bounds[&7].center();
    h.dispatch(
        &[Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        ))],
        mouse::Cursor::Available(start),
    );
    let first = Point::new(start.x, start.y + 50.0);
    let messages = h.dispatch(
        &[Event::Mouse(mouse::Event::CursorMoved { position: first })],
        mouse::Cursor::Available(first),
    );
    assert!(
        matches!(messages.as_slice(),[Message::Resize(Change::Preview(p))] if !p.manual),
        "first motion: {messages:?}; boundary={:?}; start={start:?}",
        h.model.boundary()
    );
    assert!(h.model.bounds[&1].height > original.before);
    let second = Point::new(start.x, start.y + 180.0);
    let messages = h.dispatch(
        &[Event::Mouse(mouse::Event::CursorMoved { position: second })],
        mouse::Cursor::Available(second),
    );
    assert!(matches!(messages.as_slice(),[Message::Resize(Change::Preview(p))] if p.manual));
    assert!(
        h.model.bounds.contains_key(&1),
        "after breakthrough: {messages:?}; minimum={:?}",
        h.model.minimum
    );
    assert!(h.model.bounds[&1].height > original.before + 20.0);
    let messages = h.dispatch(
        &[Event::Mouse(mouse::Event::ButtonReleased(
            mouse::Button::Left,
        ))],
        mouse::Cursor::Available(second),
    );
    assert!(
        matches!(messages.as_slice(), [Message::Resize(Change::Commit(_))]),
        "release: {messages:?}"
    );
    let saved = h.model.requests;
    let height = h.model.bounds[&2].height;
    h.click(6);
    assert!(h.model.collapsed());
    assert!(!h.model.bounds.contains_key(&7));
    assert!(h.model.bounds[&1].height > original.before);
    h.click(6);
    assert!(!h.model.collapsed());
    assert_eq!(h.model.requests, saved);
    assert!((h.model.bounds[&2].height - height).abs() < 0.01);
    h.click(4);
    assert!(!h.model.panel.snapshot().displayed);
    assert!(!h.model.bounds.contains_key(&1));
    h.click(4);
    assert!(h.model.panel.snapshot().displayed);
    assert_eq!(h.model.requests, saved);
}
