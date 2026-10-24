//! What is drawn on a fill, and the selection tint (household rule 6,
//! "What is drawn on a fill" and "What is highlighted, and how").
//!
//! Text and symbolic icons on an accent or muted fill, or on the selection
//! tint, keep their hue and chroma; only their lightness moves, as far as
//! reading needs ([`ON_FILL_REACH`]). A grey has no hue for that move to
//! distort, so it goes on to a set colour: the fill's on colour on a solid
//! fill, the page's strongest text colour on a tint (Juliet, 24 and 25 Oct).
//! A colour in between goes part of the way, by its chroma
//! ([`ON_FILL_GREY_CHROMA`]).
//!
//! Solid accent marks text selected in something short being written (a
//! composer, a text box, a note); the tint ([`SELECTION_TINT`]) marks
//! everything else selected or marked in content. [`on_solid`] and
//! [`on_tint`] are the two; [`on_fill`] is what both come to.

use crate::color::{Oklch, Srgb, Srgba};
use crate::tones::{on_muted, over};

/// How far the fill rule moves a colour's OKLab lightness from a fill whose
/// on colour is lighter than it (a light theme's accent or muted fill), and
/// how far from one whose on colour is darker (a dark theme's). Juliet's
/// row C of household-ui `sheets/fill-rule-tuning.png` (19 Oct)
pub const ON_FILL_REACH: (f32, f32) = (0.34, 0.36);

/// The OKLCh chroma below which the fill rule takes a colour all the way to
/// the colour greys go to, and above which only as far as [`ON_FILL_REACH`];
/// between them, part of the way. Sheet `sheets/fill-rule-greys.png`
/// (Juliet, 24 Oct)
pub const ON_FILL_GREY_CHROMA: (f32, f32) = (0.02, 0.08);

/// How much of the accent the selection tint lays over what is beneath it
/// (sheet `sheets/highlight-standard.png`, Juliet 25 Oct)
pub const SELECTION_TINT: f32 = 0.32;

/// How much of a dimmed item's colour is kept over its fill, as the apps
/// fade disabled text ([`dimmed_over`])
pub const ON_FILL_DIMMED_KEEP: f32 = 0.4;

/// Whether text on `fill` is lighter than it: white reads better on it than
/// black ([`on_muted`]'s test). Worked out from the fill each time, never
/// from the theme's mode, as a custom accent may be very light or very dark
pub fn lighter_on(fill: Srgb) -> bool {
    on_muted(fill) == Srgb::WHITE
}

/// Whether OKLCh `l`, `chroma`, `hue` is a colour sRGB can show
fn in_gamut(l: f32, chroma: f32, hue: f32) -> bool {
    let rgb = Oklch::new(l, chroma, hue).to_oklab().to_srgb_unclamped();
    [rgb.r, rgb.g, rgb.b].into_iter().all(|c| (-1e-4..=1.0 + 1e-4).contains(&c))
}

/// The most chroma sRGB can show at OKLab lightness `l` and `hue` (degrees)
pub fn max_chroma(l: f32, hue: f32) -> f32 {
    if l <= 0.0 || l >= 1.0 {
        return 0.0;
    }
    let (mut lo, mut hi) = (0.0f32, 0.4f32);
    for _ in 0..20 {
        let mid = (lo + hi) / 2.0;
        if in_gamut(l, mid, hue) { lo = mid } else { hi = mid }
    }
    lo
}

/// OKLCh `l` and `hue` with `chroma`, or as much of it as sRGB can show
/// there: hue and lightness exact, chroma reduced only as far as the gamut
/// needs
pub fn in_srgb(l: f32, chroma: f32, hue: f32) -> Srgb {
    let l = l.clamp(0.0, 1.0);
    Oklch::new(l, chroma.max(0.0).min(max_chroma(l, hue)), hue).to_srgb()
}

/// How far past the fill rule's target towards the colour greys go to a
/// colour of OKLCh `chroma` goes: all the way (1) below
/// [`ON_FILL_GREY_CHROMA`]`.0`, none of it (0) above `.1`, smoothly between
pub fn grey_weight(chroma: f32) -> f32 {
    let (grey, coloured) = ON_FILL_GREY_CHROMA;
    let x = ((chroma - grey) / (coloured - grey)).clamp(0.0, 1.0);
    1.0 - x * x * (3.0 - 2.0 * x)
}

/// What `color` is drawn in on the opaque `fill`, a grey going to
/// `greys_to`: its hue and chroma kept, its lightness moved to the fill's
/// plus [`ON_FILL_REACH`]`.0` where the fill's on colour is lighter, minus
/// `.1` where darker, and a grey on to `greys_to`'s lightness (never short
/// of that target). A colour already as far from the fill as that keeps
/// its own lightness; chroma is kept as far as sRGB allows at the new lightness
pub fn on_fill(color: Srgb, fill: Srgb, greys_to: Srgb) -> Srgb {
    let fill_l = fill.lightness();
    let own = color.to_oklch();
    let up = lighter_on(fill);
    let target = if up { fill_l + ON_FILL_REACH.0 } else { fill_l - ON_FILL_REACH.1 }.clamp(0.0, 1.0);
    // The chroma it can keep at the target: a colour the gamut greys there
    // has no hue left to keep either
    let weight = grey_weight(own.chroma.min(max_chroma(target, own.hue)));
    let toward = target + weight * (greys_to.lightness() - target);
    // Never short of the target, and a grey judged against where greys go:
    // judged against the target, a grey just past it would keep its own
    // lightness while a lighter one went on beyond it, out of order
    let l = if up { toward.max(target) } else { toward.min(target) };
    let beyond = if up { own.l >= l } else { own.l <= l };
    if beyond {
        return color;
    }
    in_srgb(l, own.chroma, own.hue)
}

/// [`on_fill`] on a solid fill (an accent or muted fill, a solid
/// selection): greys go to the fill's on colour, white or black
pub fn on_solid(color: Srgb, fill: Srgb) -> Srgb {
    on_fill(color, fill, on_muted(fill))
}

/// The selection tint as it shows over `beneath`: `accent` at
/// [`SELECTION_TINT`]
pub fn tint(accent: Srgb, beneath: Srgb) -> Srgb {
    over(Srgba::new(accent.r, accent.g, accent.b, SELECTION_TINT), beneath.into()).rgb()
}

/// [`on_fill`] on the selection tint over `beneath`: greys go to
/// `strongest`, the page's strongest text colour (a light theme's darkest
/// text, not black)
pub fn on_tint(color: Srgb, accent: Srgb, beneath: Srgb, strongest: Srgb) -> Srgb {
    on_fill(color, tint(accent, beneath), strongest)
}

/// `color` dimmed over `fill`, keeping [`ON_FILL_DIMMED_KEEP`] of it: a
/// dim terminal cell, or a disabled item, on its fill
pub fn dimmed_over(color: Srgb, fill: Srgb) -> Srgb {
    let mix = |c: f32, f: f32| f + (c - f) * ON_FILL_DIMMED_KEEP;
    Srgb::new(mix(color.r, fill.r), mix(color.g, fill.g), mix(color.b, fill.b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(hex: &str) -> Srgb {
        Srgb::parse(hex).unwrap()
    }

    /// The largest 8-bit channel difference between two colours
    fn off(a: Srgb, b: Srgb) -> i32 {
        let (a, b) = (a.to_rgb8(), b.to_rgb8());
        (0..3).map(|i| (i32::from(a[i]) - i32::from(b[i])).abs()).max().unwrap()
    }

    /// Hue difference in degrees, round the circle
    fn hue_gap(a: f32, b: f32) -> f32 {
        let d = (a - b).rem_euclid(360.0);
        d.min(360.0 - d)
    }

    /// Each `(colour, expected)` row comes out within two units of each
    /// 8-bit channel: the values of an independent port, which is how the
    /// approved sheets were checked
    fn check(rows: &[(&str, &str)], draw: impl Fn(Srgb) -> Srgb, what: &str) {
        for (color, want) in rows {
            let got = draw(rgb(color));
            assert!(off(got, rgb(want)) <= 2, "{what}: {color} → {} not {want}", got.to_hex());
        }
    }

    /// On Juliet's dark-red accent (#7f2400), greys of any lightness go all
    /// the way to white; a colour keeps its hue at the fill's lightness plus
    /// 0.34; white, already beyond that, is left alone (rule 6, 19 and
    /// 24 Oct)
    #[test]
    fn on_the_dark_red_accent_greys_go_to_white_and_colours_keep_their_hue() {
        let fill = rgb("#7f2400");
        check(
            &[
                ("#222222", "#ffffff"),
                ("#555555", "#ffffff"),
                ("#8a8a8a", "#ffffff"),
                ("#5c5650", "#ffffff"),
                ("#56657a", "#e0edff"),
                ("#c01c28", "#ff7f78"),
                ("#26a269", "#52c589"),
                ("#c4a000", "#cca718"),
                ("#1c71d8", "#71adff"),
                ("#a347ba", "#dd7ff5"),
                ("#ffffff", "#ffffff"),
            ],
            |c| on_solid(c, fill),
            "solid on #7f2400",
        );
    }

    /// The same on the muted version of that accent (#954b33)
    #[test]
    fn on_the_muted_red_fill_the_same_holds() {
        let fill = rgb("#954b33");
        check(&[("#222222", "#ffffff"), ("#1c71d8", "#a9cdff")], |c| on_solid(c, fill), "solid on #954b33");
    }

    /// On a dark theme's light accent (#ff9f7d), black is the on colour:
    /// greys go to black, and colours are moved 0.36 darker (rule 6, 19 Oct)
    #[test]
    fn on_a_light_accent_greys_go_to_black_and_colours_darker() {
        let fill = rgb("#ff9f7d");
        assert!(!lighter_on(fill));
        check(
            &[("#e6e6e6", "#000000"), ("#9e9e9e", "#000000"), ("#e5c07b", "#684b00"), ("#79c0ff", "#00548a")],
            |c| on_solid(c, fill),
            "solid on #ff9f7d",
        );
    }

    /// The selection tint of the accent #7f2400 over a light theme's #ebebeb
    /// shows as #c8aba0, and of #ff9f7d over a dark theme's #1e1e1e as
    /// #66473c (rule 6, 25 Oct)
    #[test]
    fn the_tint_is_the_accent_at_a_third_over_what_is_beneath() {
        assert!(off(tint(rgb("#7f2400"), rgb("#ebebeb")), rgb("#c8aba0")) <= 2);
        assert!(off(tint(rgb("#ff9f7d"), rgb("#1e1e1e")), rgb("#66473c")) <= 2);
    }

    /// On the light theme's tint, greys go to the page's strongest text
    /// colour (#222222, not black), and a colour is darkened to the tint's
    /// lightness less 0.36; #222222 is already beyond that and stays
    /// (rule 6, 25 Oct)
    #[test]
    fn on_the_light_tint_greys_go_to_the_strongest_text_colour() {
        let (accent, beneath, strongest) = (rgb("#7f2400"), rgb("#ebebeb"), rgb("#222222"));
        check(
            &[
                ("#222222", "#222222"),
                ("#555555", "#222222"),
                ("#8a8a8a", "#222222"),
                ("#7f2400", "#7f2400"),
                ("#c01c28", "#8c0014"),
                ("#26a269", "#005734"),
                ("#c4a000", "#584700"),
                ("#1c71d8", "#004592"),
            ],
            |c| on_tint(c, accent, beneath, strongest),
            "tint of #7f2400 over #ebebeb",
        );
    }

    /// On the dark theme's tint the on colour is lighter, so greys go to its
    /// strongest text colour (#e6e6e6), and colours already light enough
    /// stay as they are (rule 6, 25 Oct)
    #[test]
    fn on_the_dark_tint_greys_go_to_the_strongest_text_colour() {
        let (accent, beneath, strongest) = (rgb("#ff9f7d"), rgb("#1e1e1e"), rgb("#e6e6e6"));
        check(
            &[
                ("#e6e6e6", "#e6e6e6"),
                ("#9e9e9e", "#e6e6e6"),
                ("#ff7b72", "#ff8f86"),
                ("#7ee787", "#7ee787"),
                ("#e5c07b", "#e5c07b"),
                ("#79c0ff", "#79c0ff"),
            ],
            |c| on_tint(c, accent, beneath, strongest),
            "tint of #ff9f7d over #1e1e1e",
        );
    }

    /// The terminal's light palette on the tint of #7f2400 over #e8e8e8,
    /// strongest text #1b1b1b: the grey goes to it, the palette colours stay
    /// themselves, darker (rule 6, 25 Oct)
    #[test]
    fn the_terminal_light_palette_on_the_tint() {
        let (accent, beneath, strongest) = (rgb("#7f2400"), rgb("#e8e8e8"), rgb("#1b1b1b"));
        check(
            &[("#5e5e5e", "#1b1b1b"), ("#A24F50", "#77282c"), ("#437E4F", "#175528"), ("#7D6E1E", "#534700"), ("#516D94", "#2a4469")],
            |c| on_tint(c, accent, beneath, strongest),
            "terminal palette on the tint",
        );
    }

    /// Every coloured colour that has to move keeps its hue (within 2.5
    /// degrees) and gains no chroma, and keeps as much chroma as sRGB allows
    /// at its new lightness, on a light fill and on a dark one: the fill rule
    /// moves lightness only (rule 6, 19 Oct)
    #[test]
    fn colours_keep_their_hue_and_as_much_chroma_as_the_gamut_allows() {
        let mut moved = 0;
        for (fill, greys_to) in [("#7f2400", "#ffffff"), ("#954b33", "#ffffff"), ("#ff9f7d", "#000000")] {
            let (fill, greys_to) = (rgb(fill), rgb(greys_to));
            for hue in (0..360).step_by(20) {
                for l in [0.35, 0.55, 0.75] {
                    let color = Oklch::new(l, 0.12, hue as f32).to_srgb();
                    let own = color.to_oklch();
                    let got = on_fill(color, fill, greys_to);
                    if got == color {
                        continue;
                    }
                    moved += 1;
                    let new = got.to_oklch();
                    assert!(hue_gap(own.hue, new.hue) <= 2.5, "hue {hue} l {l} on {fill:?}: {} → {}", own.hue, new.hue);
                    assert!(new.chroma <= own.chroma + 0.01, "chroma raised: {} → {}", own.chroma, new.chroma);
                    let possible = own.chroma.min(max_chroma(new.l, own.hue));
                    assert!(new.chroma >= possible - 0.015, "hue {hue} l {l}: chroma {} where {possible} fits", new.chroma);
                }
            }
        }
        assert!(moved > 20, "only {moved} colours moved: the sweep tests nothing");
    }

    /// A colour short of the target is moved to exactly it: the fill's
    /// lightness plus 0.34 where white reads, minus 0.36 where black does
    /// (rule 6, 19 Oct)
    #[test]
    fn a_colour_short_of_the_target_lands_on_it() {
        let fill = rgb("#7f2400");
        let got = on_solid(rgb("#1c71d8"), fill);
        assert!((got.lightness() - (fill.lightness() + ON_FILL_REACH.0)).abs() < 0.005);
        let fill = rgb("#ff9f7d");
        let got = on_solid(rgb("#e5c07b"), fill);
        assert!((got.lightness() - (fill.lightness() - ON_FILL_REACH.1)).abs() < 0.005);
    }

    /// A grey weighs 1 up to chroma 0.02, a colour 0 from 0.08, and between
    /// them it falls steadily (rule 6, 24 Oct, sheet fill-rule-greys)
    #[test]
    fn the_grey_weight_is_one_for_greys_none_for_colours_and_falls_between() {
        assert_eq!(grey_weight(0.0), 1.0);
        assert_eq!(grey_weight(0.02), 1.0);
        assert_eq!(grey_weight(0.08), 0.0);
        assert_eq!(grey_weight(0.3), 0.0);
        let mut last = 1.0;
        for i in 0..=60 {
            let w = grey_weight(0.02 + 0.06 * i as f32 / 60.0);
            assert!(w <= last + 1e-6, "weight rose at step {i}: {last} → {w}");
            assert!((0.0..=1.0).contains(&w));
            last = w;
        }
        let mid = grey_weight(0.05);
        assert!(mid > 0.0 && mid < 1.0);
    }

    /// Which way a colour moves comes from the fill, not the theme's mode: a
    /// very light accent has black on it, so colours go darker; a very dark
    /// one has white, so they go lighter (rule 6, 19 Oct)
    #[test]
    fn the_direction_comes_from_the_fill_not_the_mode() {
        let light = rgb("#f0e0a0");
        assert!(!lighter_on(light));
        let darker = on_solid(rgb("#1c71d8"), light);
        assert!(darker.lightness() < light.lightness() - 0.3, "{darker:?}");
        assert_eq!(on_solid(rgb("#9e9e9e"), light).to_hex(), "#000000");

        let dark = rgb("#202040");
        assert!(lighter_on(dark));
        let lighter = on_solid(rgb("#c01c28"), dark);
        assert!(lighter.lightness() > dark.lightness() + 0.3, "{lighter:?}");
        assert_eq!(on_solid(rgb("#555555"), dark).to_hex(), "#ffffff");
    }

    /// A `greys_to` that is nearer the fill than the target is does not stop
    /// a grey short: it still lands at the target (rule 6, 24 Oct: "never
    /// short of the target"); one beyond the target is gone on to
    #[test]
    fn greys_are_never_left_short_of_the_target() {
        let fill = rgb("#7f2400");
        let target = fill.lightness() + ON_FILL_REACH.0;
        // #b0b0b0 (0.757) is a little past the target (0.741); #909090 is short of it
        let past = on_fill(rgb("#555555"), fill, rgb("#b0b0b0"));
        assert!(past.lightness() >= target - 0.005, "{} is short of {target}", past.lightness());
        assert!((past.lightness() - rgb("#b0b0b0").lightness()).abs() < 0.005);
        let short = on_fill(rgb("#555555"), fill, rgb("#909090"));
        assert!((short.lightness() - target).abs() < 0.005, "{} is not the target {target}", short.lightness());
        // On a light fill the same, towards darker
        let fill = rgb("#ff9f7d");
        let target = fill.lightness() - ON_FILL_REACH.1;
        let short = on_fill(rgb("#e6e6e6"), fill, rgb("#707070"));
        assert!((short.lightness() - target).abs() < 0.005, "{} is not the target {target}", short.lightness());
    }

    /// Greys 0 to 1 in steps of a hundredth, as sRGB colours
    fn greys() -> impl Iterator<Item = Srgb> {
        (0..=100).map(|i| Srgb::new(i as f32 / 100.0, i as f32 / 100.0, i as f32 / 100.0))
    }

    /// Greys come out in the order they went in: none is drawn darker than
    /// one that was darker than it (no lighter grey comes out darker) when
    /// `draw` is the fill rule
    fn assert_grey_order_kept(draw: impl Fn(Srgb) -> Srgb, what: &str) {
        let mut last = (0.0, -1.0);
        for grey in greys() {
            let got = draw(grey).lightness();
            assert!(got >= last.1 - 0.002, "{what}: grey {} → {got}, after {} → {}", grey.to_hex(), last.0, last.1);
            last = (grey.lightness(), got);
        }
    }

    /// On the light tint every grey lighter than the strongest text (#222222)
    /// comes out #222222, #222222 itself and black stay, and the order of
    /// the greys is kept: a lighter grey is never drawn darker than a
    /// darker one (rule 6, 25 Oct; the #464646 and #4d4d4d case, fixed with
    /// the check against where greys go)
    #[test]
    fn on_the_light_tint_every_lighter_grey_goes_to_the_strongest_text_and_order_is_kept() {
        let (accent, beneath, strongest) = (rgb("#7f2400"), rgb("#ebebeb"), rgb("#222222"));
        let draw = |c| on_tint(c, accent, beneath, strongest);
        assert_grey_order_kept(draw, "light tint");
        for grey in greys() {
            let got = draw(grey);
            if grey.lightness() > strongest.lightness() + 0.002 {
                assert!(off(got, strongest) <= 2, "grey {} → {}, not #222222", grey.to_hex(), got.to_hex());
            } else if grey.lightness() < strongest.lightness() - 0.002 {
                assert_eq!(got, grey, "grey {} is darker than #222222 and should stay", grey.to_hex());
            }
        }
        assert_eq!(draw(rgb("#000000")), rgb("#000000"));
        assert!(off(draw(rgb("#222222")), strongest) <= 2);
        // The pair that came out in the wrong order
        assert!(draw(rgb("#464646")).lightness() <= draw(rgb("#4d4d4d")).lightness() + 0.002);
    }

    /// Mirrored on the dark tint: every grey darker than the strongest text
    /// (#e6e6e6) comes out #e6e6e6, white stays, and the order is kept
    /// (rule 6, 25 Oct)
    #[test]
    fn on_the_dark_tint_every_darker_grey_goes_to_the_strongest_text_and_order_is_kept() {
        let (accent, beneath, strongest) = (rgb("#ff9f7d"), rgb("#1e1e1e"), rgb("#e6e6e6"));
        let draw = |c| on_tint(c, accent, beneath, strongest);
        assert_grey_order_kept(draw, "dark tint");
        for grey in greys() {
            let got = draw(grey);
            if grey.lightness() < strongest.lightness() - 0.002 {
                assert!(off(got, strongest) <= 2, "grey {} → {}, not #e6e6e6", grey.to_hex(), got.to_hex());
            } else if grey.lightness() > strongest.lightness() + 0.002 {
                assert_eq!(got, grey, "grey {} is lighter than #e6e6e6 and should stay", grey.to_hex());
            }
        }
        assert_eq!(draw(rgb("#ffffff")), rgb("#ffffff"));
    }

    /// On the solid fill #7f2400 every grey goes to white, except white
    /// itself, which already is (rule 6, 24 Oct)
    #[test]
    fn on_the_solid_fill_every_grey_goes_to_white() {
        let fill = rgb("#7f2400");
        assert_grey_order_kept(|c| on_solid(c, fill), "solid #7f2400");
        for grey in greys() {
            let got = on_solid(grey, fill);
            assert!(off(got, Srgb::WHITE) <= 2, "grey {} → {}, not white", grey.to_hex(), got.to_hex());
        }
    }

    /// A dimmed colour keeps 0.4 of the way from the fill to itself
    /// (rule 6: a disabled item on a fill is the on colour, dimmed)
    #[test]
    fn dimming_white_over_a_fill_keeps_four_tenths_of_the_way() {
        let fill = rgb("#7f2400");
        let got = dimmed_over(Srgb::WHITE, fill);
        assert!((got.r - (fill.r + (1.0 - fill.r) * 0.4)).abs() < 1e-6);
        assert!((got.g - (fill.g + (1.0 - fill.g) * 0.4)).abs() < 1e-6);
        assert!((got.b - (fill.b + (1.0 - fill.b) * 0.4)).abs() < 1e-6);
        assert_eq!(dimmed_over(fill, fill), fill);
        assert_eq!(ON_FILL_DIMMED_KEEP, 0.4);
    }
}
