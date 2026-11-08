//! The borders of text boxes and cards (household rule 25a, "The dark-mode
//! border" and "Text boxes come in two kinds"), from COSMIC Terminal's
//! `tones.rs`, where they were first decided.
//!
//! - At rest: [`rest_border`], #5c5c5c on a dark theme
//!   ([`DARK_REST_BORDER`], Juliet 19 Oct), libcosmic's divider on a light one.
//! - Hovered (a normal text box, a card): [`hovered_border`], the rest
//!   colour [`HOVER_STEP`] of the way to the text.
//! - A window behind others: [`faded_accent`], the accent faded
//!   [`FADED_ACCENT`] of the way to the element's neutral colour (Tom,
//!   2 Nov; the prominent text box's ring, Juliet 8 Nov).
//!
//! A normal text box's focused border is the muted accent
//! ([`crate::muted_accent`]): the palette has only primary and muted.

use crate::color::{Srgb, Srgba};
use crate::tones::over;

/// The border of a card or a text box at rest on a dark theme, in place of
/// libcosmic's divider (#444444), which barely shows on the dark page
/// (Juliet, 19 Oct, from a preview)
pub const DARK_REST_BORDER: Srgb = Srgb {
    r: 0x5c as f32 / 255.0,
    g: 0x5c as f32 / 255.0,
    b: 0x5c as f32 / 255.0,
};

/// A card's or a text box's border at rest: [`DARK_REST_BORDER`] on a dark
/// theme, keeping the divider's alpha, else `divider`, libcosmic's own
pub fn rest_border(divider: Srgba, dark: bool) -> Srgba {
    match dark {
        true => DARK_REST_BORDER.with_alpha(divider.a),
        false => divider,
    }
}

/// How far a hovered border goes from the rest colour towards the text
/// (the terminal's cards, and the normal text box of the 8 Nov sheet)
pub const HOVER_STEP: f32 = 0.35;

/// A hovered border: `rest`, as it shows on the page, [`HOVER_STEP`] of the
/// way to `text`
pub fn hovered_border(rest: Srgb, text: Srgb) -> Srgb {
    let step = |r: f32, t: f32| r + (t - r) * HOVER_STEP;
    Srgb::new(
        step(rest.r, text.r),
        step(rest.g, text.g),
        step(rest.b, text.b),
    )
}

/// How far [`faded_accent`] goes towards its neutral
pub const FADED_ACCENT: f32 = 0.5;

/// A coloured element of a window behind others (Tom, 2 Nov: option C of
/// the preview): `accent` faded [`FADED_ACCENT`] of the way to `neutral`,
/// the same element's neutral colour, both as they show on `page`: opaque
pub fn faded_accent(accent: Srgba, neutral: Srgba, page: Srgb) -> Srgb {
    let (from, to) = (over(accent, page.into()), over(neutral, page.into()));
    let step = |f: f32, t: f32| f + (t - f) * FADED_ACCENT;
    Srgb::new(step(from.r, to.r), step(from.g, to.g), step(from.b, to.b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Mode, Role};

    fn hex(c: Srgb) -> String {
        c.to_hex()
    }

    /// The values of the 8 Nov sheet Juliet chose from
    /// (`sheets/text-box-look-sheet-*.png`), on COSMIC's default accents
    #[test]
    fn the_sheets_borders() {
        let page_dark = Role::Light.tone(Mode::Dark).unwrap();
        let page_light = Role::Light.tone(Mode::Light).unwrap();
        let divider_light = Srgb::hex(0xb0b0b0).with_alpha(1.0);
        assert_eq!(
            hex(rest_border(Srgba::new(0.0, 0.0, 0.0, 1.0), true).rgb()),
            "#5c5c5c"
        );
        assert_eq!(rest_border(divider_light, false), divider_light);
        assert_eq!(
            hex(hovered_border(Srgb::hex(0x5c5c5c), Srgb::hex(0xe8e8e8))),
            "#8d8d8d"
        );
        assert_eq!(
            hex(hovered_border(Srgb::hex(0xb0b0b0), Srgb::hex(0x101010))),
            "#787878"
        );
        let dark_accent = Srgb::hex(0x63d0df).with_alpha(1.0);
        let rest = DARK_REST_BORDER.with_alpha(1.0);
        assert_eq!(hex(faded_accent(dark_accent, rest, page_dark)), "#60969e");
        let light_accent = Srgb::hex(0x00525a).with_alpha(1.0);
        assert_eq!(
            hex(faded_accent(light_accent, divider_light, page_light)),
            "#588185"
        );
    }
}
