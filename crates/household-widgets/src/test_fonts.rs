//! The text environment the apps' tests run in, so a width measured here is
//! the width an app draws: the real Open Sans the apps use, loaded as
//! household-test's pinned fonts load it (the same files, in `test-fonts`).
//! Without it the tests measure the system's fallback font, whose widths
//! differ from Open Sans (labels shaped basic and in full agree in that font
//! and not in this one)

use cosmic::iced::advanced::graphics::text::{cosmic_text, font_system};
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
