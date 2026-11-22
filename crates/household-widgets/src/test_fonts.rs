//! The text environment the apps' tests run in, so a width measured here is
//! the width an app draws: the real Open Sans the apps use, loaded as
//! household-test's pinned fonts load it (the same files, in `test-fonts`).
//! Without it the tests measure the system's fallback font, whose widths
//! differ from Open Sans (labels shaped basic and in full agree in that font
//! and not in this one; a menu whose row turns semibold as its submenu
//! opens grows 2 px in Open Sans, and did not in the fallback).
//!
//! The font system is one per process and the tests share it, so nothing
//! may measure text before it is pinned: a test measuring in the fallback
//! font while another pins would see its widths change under it, and
//! `menu::measured_width` keeps its widths for the process. So it is pinned
//! where text is measured: every test's renderer comes from [`renderer`],
//! and the measurements made with no renderer (`menu::measured_width`, which
//! the label and part widths use, and the content boxes' shaping) pin in
//! test builds. The integration tests include this file as their own module.

use cosmic::iced::advanced::graphics::text::{cosmic_text, font_system};
use cosmic::iced::advanced::renderer::Headless;
use cosmic::iced::{Font, Pixels};
use std::sync::Once;

static PIN: Once = Once::new();

/// Loads Open Sans (regular, semibold, bold) as the sans-serif family into the
/// process's font system, once. Call before measuring or laying text out
pub(crate) fn pin() {
    PIN.call_once(|| {
        let mut db = cosmic_text::fontdb::Database::new();
        db.load_font_data(include_bytes!("../test-fonts/OpenSans-Regular.ttf").to_vec());
        db.load_font_data(include_bytes!("../test-fonts/OpenSans-Semibold.ttf").to_vec());
        db.load_font_data(include_bytes!("../test-fonts/OpenSans-Bold.ttf").to_vec());
        db.set_sans_serif_family("Open Sans");
        let mut system = font_system().write().unwrap_or_else(|p| p.into_inner());
        *system.raw() = cosmic_text::FontSystem::new_with_locale_and_db("en-US".into(), db);
    });
}

/// A headless tiny-skia renderer, at the default font and 14 px, with Open
/// Sans pinned: the renderer every test lays out and measures with
pub(crate) fn renderer() -> Option<cosmic::Renderer> {
    pin();
    cosmic::iced::futures::executor::block_on(<cosmic::Renderer as Headless>::new(
        Font::default(),
        Pixels(14.0),
        Some("tiny-skia"),
    ))
}
