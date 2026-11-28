// SPDX-License-Identifier: GPL-3.0-only

use cosmic::iced::advanced::renderer::{Headless, Style};
use cosmic::iced::advanced::widget::Tree;
use cosmic::iced::advanced::{Layout, Renderer as _, Widget, layout as native_layout, mouse};
use cosmic::iced::widget::text::Wrapping;
use cosmic::iced::{Font, Length, Pixels, Rectangle, Size};
use cosmic::{Renderer, Theme, widget};
use household_widgets::{form, layout, segmented_control};

#[test]
fn a_form_can_present_the_same_choices_and_editable_path_at_wide_and_narrow_widths() {
    let theme = Theme::light();
    let mut renderer = cosmic::iced::futures::executor::block_on(<Renderer as Headless>::new(
        Font::default(),
        Pixels(14.0),
        Some("tiny-skia"),
    ))
    .expect("headless tiny-skia renderer");
    let machines = layout::child(|_| {
        segmented_control::view(
            vec![
                segmented_control::Choice {
                    label: "This machine".into(),
                    icon: None,
                    message: Some(String::from("local")),
                    tip: None,
                },
                segmented_control::Choice {
                    label: "Workstation".into(),
                    icon: None,
                    message: Some(String::from("remote")),
                    tip: None,
                },
                segmented_control::Choice {
                    label: "Laptop".into(),
                    icon: None,
                    message: None,
                    tip: Some("This machine is offline".into()),
                },
            ],
            Some(0),
            None,
            false,
            14.0,
        )
    })
    .minimum_width(|level, preferred| match level {
        layout::Level::Preferred => preferred.width,
        _ => preferred.width.min(216.0),
    });
    let path = layout::child(|_| {
        widget::text_input("Folder", "/home/user/Worktrees/example")
            .on_input(|path| path)
            .width(Length::Fill)
            .into()
    })
    .minimum_width(|_, _| 150.0)
    .preferred_width(300.0);
    let help = layout::child(|_| {
        widget::text("The branch will be checked out in this folder.")
            .size(12)
            .wrapping(Wrapping::WordOrGlyph)
            .into()
    })
    .minimum_width(|_, _| 40.0);
    let mut form = form::fields(
        vec![
            form::Field::new("Where", machines).label_size(14.0),
            form::Field::new("Worktree folder", path)
                .label_size(14.0)
                .supporting(help),
        ],
        110.0,
    );
    for (width, expected) in [(540.0, 0), (380.0, 1), (238.0, 2)] {
        let size = Size::new(width, 330.0);
        let mut tree = Tree::empty();
        let node = form.layout(
            &mut tree,
            &renderer,
            &native_layout::Limits::new(Size::ZERO, size),
        );
        assert_eq!(
            form.selection()
                .expect("attempted")
                .as_ref()
                .unwrap()
                .stage_index,
            expected
        );

        if let Ok(directory) = std::env::var("HOUSEHOLD_RESPONSIVE_FIXTURE_OUT") {
            let crop = Size::new(width as u32, node.size().height.ceil() as u32);
            let colour = theme.cosmic().on_bg_color().into();
            let viewport = Rectangle::with_size(size);
            renderer.reset(viewport);
            form.draw(
                &tree,
                &mut renderer,
                &theme,
                &Style {
                    icon_color: colour,
                    text_color: colour,
                    scale_factor: 1.0,
                },
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &viewport,
            );
            let rgba = renderer.screenshot(crop, 1.0, theme.cosmic().bg_color().into());
            std::fs::write(
                std::path::Path::new(&directory)
                    .join(format!("{}x{}.rgba", crop.width, crop.height)),
                rgba,
            )
            .unwrap();
        }
    }
}
