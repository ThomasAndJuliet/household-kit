//! The adjustments every household app makes to its theme's colours
//! (household rule 25a and the muted accent of rule 6). From the apps'
//! `tones.rs`: cosmic-term's where the copies differ (its held-surface
//! states), with Cerea's `grey` and `dark_tone`.
//!
//! - Large surfaces (cards, menus, dropdowns, fields) are held below white:
//!   a light theme's components are near white, which glares across areas
//!   that size on its grey page.
//! - Text in a colour, as a tab's title or a link on paper, is set at a
//!   lightness that reads on the page.
//! - Toggles that are on, a segmented control's chosen part and the chosen
//!   side-panel tab are filled with the muted accent.

use crate::color::{Oklch, Srgb, Srgba};

/// The most OKLab lightness a large surface is drawn with: above it, white
/// glares on the grey page
pub const SURFACE_LIGHTNESS: f32 = 0.94;

/// `color` with its OKLab lightness held to [`SURFACE_LIGHTNESS`], its hue
/// and chroma kept; a colour below the cap is returned as it is
pub fn surface(color: Srgb) -> Srgb {
    let mut lch = color.to_oklch();
    if lch.l <= SURFACE_LIGHTNESS {
        return color;
    }
    lch.l = SURFACE_LIGHTNESS;
    lch.to_srgb()
}

/// `top` laid over `bottom`, as libcosmic composites a state over a base
/// (source-over, in sRGB)
pub fn over(top: Srgba, bottom: Srgba) -> Srgba {
    let a = top.a + bottom.a * (1.0 - top.a);
    if a <= 0.0 {
        return Srgba::new(0.0, 0.0, 0.0, 0.0);
    }
    let mix = |t: f32, b: f32| (t * top.a + b * bottom.a * (1.0 - top.a)) / a;
    Srgba::new(
        mix(top.r, bottom.r),
        mix(top.g, bottom.g),
        mix(top.b, bottom.b),
        a,
    )
}

/// `color` at a lightness for text: its hue and chroma, as dark as label
/// text on a light page (OKLab 0.42), as light on a dark one (0.80)
pub fn text_tone(color: Srgb, dark: bool) -> Srgb {
    let mut lch = color.to_oklch();
    lch.l = if dark { 0.80 } else { 0.42 };
    lch.to_srgb()
}

/// A grey of OKLab lightness `lightness`
pub fn grey(lightness: f32) -> Srgb {
    Oklch::new(lightness, 0.0, 0.0).to_srgb()
}

/// A colour's dark tone, for body text, from its darkest: a step lighter
/// (OKLab +0.10) and a little softer (chroma × 0.85), as body text is to
/// headings
pub fn dark_tone(darkest: Srgb) -> Srgb {
    let mut lch = darkest.to_oklch();
    lch.l = (lch.l + 0.10).min(1.0);
    lch.chroma *= 0.85;
    lch.to_srgb()
}

/// The OKLab lightness of the muted accent: mid on a light theme, so it
/// keeps white text at 4.5:1 or more; lighter on a dark one, for dark text
pub const MUTED_LIGHTNESS: (f32, f32) = (0.50, 0.70);
/// The most chroma the muted accent keeps
pub const MUTED_CHROMA: f32 = 0.105;
/// How much a dull accent's chroma is raised towards that, as a dark accent
/// brought up to the muted lightness would otherwise look greyer
pub const MUTED_CHROMA_GAIN: f32 = 1.2;

/// The muted accent (household rule 6, Juliet 15 Oct): what toggles that
/// are on, a segmented control's chosen part and the chosen side-panel tab
/// are filled with, so that a row of them doesn't catch the eye as a
/// Primary button does. The accent's hue, at a set lightness
/// ([`MUTED_LIGHTNESS`]), its chroma raised by [`MUTED_CHROMA_GAIN`] up to
/// [`MUTED_CHROMA`] (a grey stays grey), then held in sRGB. Juliet's
/// reference pairs come out within a few units (tested): light #7f2400 →
/// #954b33, #005b3d → #00755a, #484848 → #636363; dark #ff9f7d → #d7876d,
/// #5cd7b0 → #55b394, #bdbdbd → #9e9e9e
pub fn muted_accent(accent: Srgb, dark: bool) -> Srgb {
    let mut lch = accent.to_oklch();
    lch.l = if dark {
        MUTED_LIGHTNESS.1
    } else {
        MUTED_LIGHTNESS.0
    };
    lch.chroma = (lch.chroma * MUTED_CHROMA_GAIN).min(MUTED_CHROMA);
    lch.to_srgb()
}

/// Text and icons on the muted accent: white or black, whichever has the
/// higher WCAG 2 ratio on it (at least 4.5:1 for every accent, tested)
pub fn on_muted(muted: Srgb) -> Srgb {
    if contrast_ratio(muted, Srgb::WHITE) >= contrast_ratio(muted, Srgb::BLACK) {
        Srgb::WHITE
    } else {
        Srgb::BLACK
    }
}

/// WCAG 2's relative luminance of an sRGB colour
pub fn luminance(color: Srgb) -> f32 {
    let linear = |c: f32| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

/// WCAG 2's contrast ratio of two colours, 1 to 21. Readable contrast is
/// judged with [`crate::apca`] (rule 25c); this is kept for [`on_muted`]
pub fn contrast_ratio(a: Srgb, b: Srgb) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// A container component's colours, as libcosmic's theme has them
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Component {
    pub base: Srgba,
    pub hover: Srgba,
    pub selected: Srgba,
    pub pressed: Srgba,
}

/// The theme's icon-button washes, the dark neutral of its text colour that
/// a hover or a press lays over a surface
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Washes {
    pub hover: Option<Srgba>,
    pub pressed: Option<Srgba>,
}

/// A container component held below white (household rule 6a): its base
/// capped by [`surface`], and on a light theme its hover, selected and
/// pressed made again as the washes over the base wherever the hover is not
/// darker than it (libcosmic's light theme makes them the white the base
/// was, a white patch on the surface). Pressing happens while hovering, so
/// pressed is its wash over the hovered colour, darker than hover whatever
/// the two washes are. `None` when nothing changes: the glue leaves the
/// theme alone. A state with no wash is left as it was
pub fn held_component(component: Component, washes: Washes, light: bool) -> Option<Component> {
    let base = component.base;
    let capped = surface(base.rgb()).with_alpha(base.a);
    let mut held = Component {
        base: capped,
        ..component
    };
    // On a dark theme a hover is lighter than its surface, as it should be;
    // so it is on a light one where the base was not capped and the hover
    // is darker already
    let hover_darker = component.hover.rgb().lightness() < capped.rgb().lightness() - 0.005;
    if light && !(capped == base && hover_darker) {
        let hovered = washes.hover.map(|wash| over(wash, capped));
        if let Some(hovered) = hovered {
            held.hover = hovered;
            held.selected = hovered;
        }
        if let Some(wash) = washes.pressed {
            held.pressed = over(wash, hovered.unwrap_or(capped));
        }
    }
    (held != component).then_some(held)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(hex: &str) -> Srgb {
        Srgb::parse(hex).unwrap()
    }

    /// Juliet's reference pairs (household-ui sheets/muted-toggles.png)
    /// come out within a few units of each channel, and text on every
    /// muted accent, of any hue, reads at 4.5:1 or more
    #[test]
    fn the_muted_accent_matches_juliets_pairs_and_keeps_its_text_readable() {
        for (accent, muted, dark) in [
            ("#7f2400", "#954b33", false),
            ("#005b3d", "#00755a", false),
            ("#484848", "#636363", false),
            ("#ff9f7d", "#d7876d", true),
            ("#5cd7b0", "#55b394", true),
            ("#bdbdbd", "#9e9e9e", true),
        ] {
            let got = muted_accent(rgb(accent), dark);
            let want = rgb(muted);
            let off = [
                (got.r - want.r).abs(),
                (got.g - want.g).abs(),
                (got.b - want.b).abs(),
            ]
            .into_iter()
            .fold(0.0, f32::max)
                * 255.0;
            assert!(
                off <= 14.5,
                "{accent} → {got:?}, not {muted} (off by {off})"
            );
            let distance = got.to_oklab().distance(want.to_oklab());
            assert!(distance < 0.025, "{accent}: OKLab distance {distance}");
            assert!(
                contrast_ratio(got, on_muted(got)) >= 4.5,
                "{accent}: text on {got:?}"
            );
        }
        // Every hue, light and dark, from dark and bright accents alike
        for hue in (0..360).step_by(15) {
            for l in [0.3, 0.5, 0.7, 0.9] {
                for dark in [false, true] {
                    let accent = Oklch::new(l, 0.15, hue as f32).to_srgb();
                    let got = muted_accent(accent, dark);
                    assert!(
                        contrast_ratio(got, on_muted(got)) >= 4.5,
                        "hue {hue} l {l} dark {dark}: {got:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn surfaces_come_down_to_the_cap_keeping_their_hue() {
        let near_white = Srgb::new(0.98, 0.97, 1.0);
        let capped = surface(near_white);
        assert!((capped.lightness() - SURFACE_LIGHTNESS).abs() < 0.002);
        let (before, after) = (near_white.to_oklch(), capped.to_oklch());
        assert!((before.hue - after.hue).abs() < 2.0);
        assert!((before.chroma - after.chroma).abs() < 0.003);
        // The cap only lowers
        for color in [
            Srgb::new(0.9, 0.9, 0.9),
            Srgb::new(0.2, 0.3, 0.6),
            Srgb::BLACK,
        ] {
            assert_eq!(surface(color), color);
        }
        // White comes down to the light tone of rule 25d
        assert_eq!(surface(Srgb::WHITE).to_hex(), "#ebebeb");
    }

    #[test]
    fn greys_and_dark_tones() {
        assert_eq!(grey(0.25).to_hex(), "#222222");
        assert_eq!(grey(0.45).to_hex(), "#555555");
        assert!((grey(0.25).lightness() - 0.25).abs() < 0.001);
        // A colour's dark tone: a step lighter, a little softer, its hue kept
        let clear = Srgb::new(0.47058824, 0.16078432, 0.18039216);
        assert_eq!(dark_tone(clear).to_hex(), "#914c4d");
        let (a, b) = (clear.to_oklch(), dark_tone(clear).to_oklch());
        assert!((b.l - a.l - 0.10).abs() < 0.002);
        assert!(b.chroma < a.chroma);
        assert!((a.hue - b.hue).abs() < 1.0);
        // Coloured text: its hue, at the text lightness
        let link = text_tone(rgb("#3584e4"), false);
        assert!((link.lightness() - 0.42).abs() < 0.005);
        // (a saturated blue at 0.80 leaves sRGB and is held, so lands near it)
        assert!((text_tone(rgb("#6a7f99"), true).lightness() - 0.80).abs() < 0.005);
        assert!(text_tone(rgb("#3584e4"), true).lightness() > 0.7);
    }

    #[test]
    fn over_composites_source_over() {
        let wash = Srgba::new(0.0, 0.0, 0.0, 0.1);
        let white = Srgba::new(1.0, 1.0, 1.0, 1.0);
        let got = over(wash, white);
        assert!((got.r - 0.9).abs() < 1e-6 && got.a == 1.0);
        assert_eq!(over(Srgba::default(), Srgba::default()), Srgba::default());
    }

    /// The washes libcosmic's light theme gives its icon buttons (dark
    /// translucent neutrals; its pressed wash alone is lighter than hover)
    fn light_washes() -> Washes {
        Washes {
            hover: Some(Srgba::new(0.0, 0.0, 0.0, 0.1)),
            pressed: Some(Srgba::new(0.0, 0.0, 0.0, 0.08)),
        }
    }

    #[test]
    fn a_held_surface_darkens_when_hovered_pressed_or_selected() {
        let white = Srgba::new(1.0, 1.0, 1.0, 1.0);
        // libcosmic's light component: near-white base, white hover
        let component = Component {
            base: Srgba::new(0.98, 0.98, 0.98, 1.0),
            hover: white,
            selected: white,
            pressed: white,
        };
        let held = held_component(component, light_washes(), true).expect("changed");
        let base = held.base.rgb().lightness();
        assert!((base - SURFACE_LIGHTNESS).abs() < 0.002);
        for (state, color) in [
            ("hover", held.hover),
            ("selected", held.selected),
            ("pressed", held.pressed),
        ] {
            assert!(
                color.rgb().lightness() < base - 0.02,
                "{state} {color:?} is not darker than {:?}",
                held.base
            );
        }
        assert!(
            held.pressed.rgb().lightness() <= held.hover.rgb().lightness(),
            "pressed is at least as dark as hover"
        );
    }

    #[test]
    fn an_uncapped_surface_with_a_white_hover_is_still_fixed_and_a_good_one_left() {
        let grey = Srgba::new(0.85, 0.85, 0.85, 1.0);
        let white = Srgba::new(1.0, 1.0, 1.0, 1.0);
        let bad = Component {
            base: grey,
            hover: white,
            selected: white,
            pressed: white,
        };
        let fixed = held_component(bad, light_washes(), true).expect("a white hover is redone");
        assert_eq!(fixed.base, grey);
        assert!(fixed.hover.rgb().lightness() < grey.rgb().lightness());
        let darker = Srgba::new(0.8, 0.8, 0.8, 1.0);
        let good = Component {
            base: grey,
            hover: darker,
            selected: darker,
            pressed: Srgba::new(0.75, 0.75, 0.75, 1.0),
        };
        assert_eq!(held_component(good, light_washes(), true), None);
        // Nothing is held on a dark theme
        let dark = Component {
            base: Srgba::new(0.15, 0.15, 0.15, 1.0),
            hover: Srgba::new(0.2, 0.2, 0.2, 1.0),
            ..good
        };
        assert_eq!(held_component(dark, light_washes(), false), None);
    }
}
