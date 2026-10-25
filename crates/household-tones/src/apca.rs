//! APCA-W3 0.0.98G-4g lightness contrast (household rule 25c), ported from
//! Andrew Somers' apca-w3 (`APCAcontrast`, `sRGBtoY`, `alphaBlend`), with
//! its constants, and checked against apca-w3's own test values.
//!
//! The result is Lc, signed: positive for dark text on a light background,
//! negative for light text on a dark one. Compare its absolute value with
//! the rule's levels (Lc 75 for prose, 60 for interface text, 45 for
//! meaningful non-text, 30 for text only spotted).

use crate::color::{Srgb, Srgba};

/// The 0.0.98G-4g constants
mod k {
    pub const MAIN_TRC: f64 = 2.4;
    pub const S_R: f64 = 0.2126729;
    pub const S_G: f64 = 0.7151522;
    pub const S_B: f64 = 0.0721750;
    pub const NORM_BG: f64 = 0.56;
    pub const NORM_TXT: f64 = 0.57;
    pub const REV_TXT: f64 = 0.62;
    pub const REV_BG: f64 = 0.65;
    pub const BLK_THRS: f64 = 0.022;
    pub const BLK_CLMP: f64 = 1.414;
    pub const SCALE_BOW: f64 = 1.14;
    pub const SCALE_WOB: f64 = 1.14;
    pub const LO_BOW_OFFSET: f64 = 0.027;
    pub const LO_WOB_OFFSET: f64 = 0.027;
    pub const DELTA_Y_MIN: f64 = 0.0005;
    pub const LO_CLIP: f64 = 0.1;
}

/// APCA's screen luminance Y of an sRGB colour, channels 0 to 1 (a plain
/// 2.4 power, not sRGB's piecewise curve, as APCA defines it)
pub fn screen_y(r: f64, g: f64, b: f64) -> f64 {
    k::S_R * r.powf(k::MAIN_TRC) + k::S_G * g.powf(k::MAIN_TRC) + k::S_B * b.powf(k::MAIN_TRC)
}

/// Lc of text of luminance `text_y` on a background of `bg_y` (each 0 to
/// 1.1, else 0). Polarity matters: do not swap them
pub fn contrast(text_y: f64, bg_y: f64) -> f64 {
    if text_y.is_nan() || bg_y.is_nan() || text_y.min(bg_y) < 0.0 || text_y.max(bg_y) > 1.1 {
        return 0.0;
    }
    let clamp = |y: f64| {
        if y > k::BLK_THRS {
            y
        } else {
            y + (k::BLK_THRS - y).powf(k::BLK_CLMP)
        }
    };
    let (text_y, bg_y) = (clamp(text_y), clamp(bg_y));
    if (bg_y - text_y).abs() < k::DELTA_Y_MIN {
        return 0.0;
    }
    let lc = if bg_y > text_y {
        let sapc = (bg_y.powf(k::NORM_BG) - text_y.powf(k::NORM_TXT)) * k::SCALE_BOW;
        if sapc < k::LO_CLIP {
            0.0
        } else {
            sapc - k::LO_BOW_OFFSET
        }
    } else {
        let sapc = (bg_y.powf(k::REV_BG) - text_y.powf(k::REV_TXT)) * k::SCALE_WOB;
        if sapc > -k::LO_CLIP {
            0.0
        } else {
            sapc + k::LO_WOB_OFFSET
        }
    };
    lc * 100.0
}

/// Lc of `text` on `background`
pub fn apca(text: Srgb, background: Srgb) -> f64 {
    let y = |c: Srgb| screen_y(f64::from(c.r), f64::from(c.g), f64::from(c.b));
    contrast(y(text), y(background))
}

/// Lc of eight-bit `text` on `background`, exactly as apca-w3 computes it
pub fn apca_rgb8(text: [u8; 3], background: [u8; 3]) -> f64 {
    let y = |[r, g, b]: [u8; 3]| {
        screen_y(
            f64::from(r) / 255.0,
            f64::from(g) / 255.0,
            f64::from(b) / 255.0,
        )
    };
    contrast(y(text), y(background))
}

/// Lc of translucent `text` on `background`: blended first, in sRGB, each
/// channel rounded to eight bits (apca-w3's `alphaBlend`)
pub fn apca_over(text: Srgba, background: Srgb) -> f64 {
    let a = f64::from(text.a.clamp(0.0, 1.0));
    let blend = |t: f32, b: f32| {
        ((f64::from(b) * 255.0 * (1.0 - a) + f64::from(t) * 255.0 * a)
            .round()
            .min(255.0)) as u8
    };
    let bg = background.to_rgb8();
    apca_rgb8(
        [
            blend(text.r, background.r),
            blend(text.g, background.g),
            blend(text.b, background.b),
        ],
        bg,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb8(hex: u32) -> [u8; 3] {
        [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8]
    }

    /// apca-w3's test values (test/index.js, version 0.1.9): text, then
    /// background, then Lc
    #[test]
    fn apca_w3s_own_test_values() {
        for (text, bg, want) in [
            (0x888888, 0xffffff, 63.056469930209424),
            (0xffffff, 0x888888, -68.54146436644962),
            (0x000000, 0xaaaaaa, 58.146262578561334),
            (0xaaaaaa, 0x000000, -56.24113336839742),
            (0x112233, 0xddeeff, 91.66830811481631),
            (0xddeeff, 0x112233, -93.06770049484275),
            (0x112233, 0x444444, 8.32326136957393),
            (0x444444, 0x112233, -7.526878460278154),
        ] {
            let got = apca_rgb8(rgb8(text), rgb8(bg));
            assert!(
                (got - want).abs() < 1e-9,
                "#{text:06x} on #{bg:06x}: {got}, not {want}"
            );
            // Through f32 channels, as near as they keep
            let near = apca(Srgb::hex(text), Srgb::hex(bg));
            assert!(
                (near - want).abs() < 1e-4,
                "#{text:06x} on #{bg:06x}: {near}"
            );
        }
        // #0006 on #fff: black at 0x66/255 alpha, blended to #999999
        let got = apca_over(Srgba::new(0.0, 0.0, 0.0, 0x66 as f32 / 255.0), Srgb::WHITE);
        assert!((got - 54.62184067441377).abs() < 1e-9, "{got}");
    }

    #[test]
    fn low_contrast_clips_to_zero() {
        assert_eq!(apca_rgb8([0x77; 3], [0x77; 3]), 0.0);
        assert_eq!(apca_rgb8([0x80; 3], [0x84; 3]), 0.0);
        assert_eq!(contrast(-0.1, 0.5), 0.0);
    }

    /// The pairs ~/.cache/ui-audit/apca/sheet.py printed, its port of the
    /// same formula
    #[test]
    fn the_audit_sheets_pairs() {
        for (text, bg, want) in [(0x555555, 0xebebeb, 74.1), (0x222222, 0xebebeb, 91.0)] {
            let got = apca_rgb8(rgb8(text), rgb8(bg));
            assert!((got - want).abs() < 0.05, "#{text:06x} on #{bg:06x}: {got}");
        }
    }
}
