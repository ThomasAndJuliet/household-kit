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
/// of that target). A colour already that far from the fill keeps its own
/// lightness; chroma is kept as far as sRGB allows at the new lightness
pub fn on_fill(color: Srgb, fill: Srgb, greys_to: Srgb) -> Srgb {
    let fill_l = fill.lightness();
    let own = color.to_oklch();
    let up = lighter_on(fill);
    let target = if up { fill_l + ON_FILL_REACH.0 } else { fill_l - ON_FILL_REACH.1 }.clamp(0.0, 1.0);
    let beyond = if up { own.l >= target } else { own.l <= target };
    if beyond {
        return color;
    }
    // The chroma it can keep at the target: a colour the gamut greys there
    // has no hue left to keep either
    let weight = grey_weight(own.chroma.min(max_chroma(target, own.hue)));
    let toward = target + weight * (greys_to.lightness() - target);
    let l = if up { toward.max(target) } else { toward.min(target) };
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
