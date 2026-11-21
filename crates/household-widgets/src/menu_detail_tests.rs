// SPDX-License-Identifier: GPL-3.0-only
//! A menu row's second line ([`MenuItem::detail`](crate::menu::MenuItem)).
use super::*;
use crate::menu::MenuItem;
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::{Font, Pixels};

fn renderer() -> cosmic::Renderer {
    cosmic::iced::futures::executor::block_on(<cosmic::Renderer as Headless>::new(
        Font::default(),
        Pixels(14.0),
        Some("tiny-skia"),
    ))
    .expect("native renderer")
}
fn icon(name: &'static str) -> widget::icon::Handle {
    widget::icon::from_name(name).handle()
}
fn body(text: String) -> Element<'static, ()> {
    widget::text(text).size(14).into()
}
fn measure(menu: Menu<()>, renderer: &cosmic::Renderer) -> Size {
    MenuSurface::new(
        menu,
        MenuStyle { icon, body },
        Anchors::default(),
        "detail".into(),
    )
    .panels(Id::new("source"), vec![], |_| (), |_| ())
    .remove(0)
    .stages
    .remove(0)
    .content
    .query(Level::Preferred, renderer)
    .preferred
}
const PATH: &str = "/var/home/someone/Documents/A long project folder name/with a subfolder";

#[test]
fn a_detail_line_is_measured_into_the_menus_height_and_width() {
    let renderer = renderer();
    let plain = measure(
        Menu::new().item(MenuItem::new("Open in Files", ())),
        &renderer,
    );
    let with = measure(
        Menu::new().item(MenuItem::new("Open in Files", ()).detail(PATH)),
        &renderer,
    );
    assert!(
        with.height >= plain.height + 16.0,
        "a caption line of 17 px is added: {} vs {}",
        with.height,
        plain.height
    );
    assert!(
        with.width > plain.width,
        "the path's width counts: {} vs {}",
        with.width,
        plain.width
    );
    let menu = Menu::new().item(MenuItem::new("Open", ()).detail(PATH));
    assert!(
        f32::from(menu.width()) >= crate::menu::measured_width(PATH, 12.0, cosmic::font::default()),
        "the menu's own width probe covers the detail"
    );
    assert_eq!(
        Menu::new().item(MenuItem::new("Open", ())).width(),
        crate::menu::MENU_MIN_WIDTH
    );
}

#[test]
fn a_detail_line_is_shown_whole_and_the_state_covers_the_row() {
    use cosmic::iced::advanced::widget::{Operation, operation};
    #[derive(Default)]
    struct Texts(Vec<(String, Rectangle)>);
    impl Operation for Texts {
        fn text(&mut self, _: Option<&Id>, bounds: Rectangle, text: &str) {
            self.0.push((text.to_owned(), bounds));
        }
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
            visit(self);
        }
        fn focusable(&mut self, _: Option<&Id>, _: Rectangle, _: &mut dyn operation::Focusable) {}
    }
    let mut renderer = renderer();
    let anchors = Anchors::default();
    let mut item = MenuItem::new("Open in Files", ()).detail(PATH);
    item.disabled_reason = Some("No folder".into());
    for item in [MenuItem::new("Open in Files", ()).detail(PATH), item] {
        let trigger = anchors.trigger("source", widget::space().width(40).height(20));
        let base = widget::column(vec![widget::space().height(Length::Fill).into(), trigger])
            .height(Length::Fill);
        let panels = MenuSurface::new(
            Menu::new().item(item),
            MenuStyle { icon, body },
            anchors.clone(),
            "detail".into(),
        )
        .root_candidates(vec![AnchorCandidate::new(
            Side::Above,
            Alignment::StartBounded,
        )])
        .panels(Id::new("source"), vec![], |_| (), |m| m);
        let host = crate::popup::host(base)
            .anchors(anchors.clone())
            .chain(panels);
        let mut ui = cosmic::iced::runtime::UserInterface::build(
            host,
            Size::new(900.0, 600.0),
            Default::default(),
            &mut renderer,
        );
        let mut texts = Texts::default();
        ui.operate(&renderer, &mut texts);
        let label = texts
            .0
            .iter()
            .find(|(t, _)| t == "Open in Files")
            .unwrap()
            .1;
        let detail = texts.0.iter().find(|(t, _)| t == PATH).unwrap().1;
        assert!(detail.y >= label.y + label.height - 1.0, "under the label");
        assert!(
            detail.width + 0.5 >= crate::menu::measured_width(PATH, 12.0, cosmic::font::default())
                || detail.height > 17.5,
            "the detail is laid out whole or wraps onto more lines, never clipped: {detail:?}"
        );
    }
}
