//! APCA's font lookup tables: the least Lc text of a size and weight needs.
//!
//! Source: apca-w3 0.1.9 (Myndex, github.com/Myndex/apca-w3), APCA Public
//! Beta 0.1.7 (G), for the APCA-W3 0.0.98G-4g contrast in [`crate::apca`]:
//!
//! - [`required_lc`] reads the table **sorted by font size**, `byFontSize`
//!   in `data/LUT-GseriesMay28-2022.js` ("MAIN FONT LOOKUP May 25 2022
//!   EXPANDED"). It is the table apcacontrast.com and ARC's test method
//!   publish for looking up a size and weight ("Beta 0.1.7 (G) • Revised
//!   May 27, 2022"). Between its size rows it is interpolated linearly, as
//!   its `byFontSizeDelta` array is there for and as ARC permits
//!   ("Interpolation of the data in the font table is permitted"); a weight
//!   between two columns takes the lighter column's.
//! - [`font_lookup`] is the library's `fontLookupAPCA` (`src/apca-w3.js`),
//!   the same table **sorted by Lc**, giving the least font size at a
//!   contrast. It is a separate presentation and asks more at 14 and 15 px
//!   (14 px at weight 400: Lc 110 there, Lc 100 sorted by size), so it is
//!   not used to judge a style.
//!
//! In the tables, 999 marks "too low contrast for anything" (ARC's ⊘) and
//! 777 "non-text and spot text only" (ARC's ®©). The README's notes on use
//! (apca-w3 0.1.9 README, "Font Lookup Table", current as of May 27, 2022)
//! are [`TextUse`] and [`max_lc`].
//!
//! Copyright of the tables: © 2022 Myndex Research and Andrew Somers; the
//! apca-w3 package is under its W3 licence (see its LICENSE.md).

use crate::apca::apca;
use crate::color::Srgb;

/// The table: one row per Lc, from 0 to 125 by 5, and in each the least
/// font size in px for weights 100 to 900
const FONT_MATRIX: [[f64; 10]; 25] = [
    [
        0.0, 999.0, 999.0, 999.0, 999.0, 999.0, 999.0, 999.0, 999.0, 999.0,
    ],
    [
        10.0, 999.0, 999.0, 999.0, 999.0, 999.0, 999.0, 999.0, 999.0, 999.0,
    ],
    [
        15.0, 777.0, 777.0, 777.0, 777.0, 777.0, 777.0, 777.0, 777.0, 777.0,
    ],
    [
        20.0, 777.0, 777.0, 777.0, 777.0, 777.0, 777.0, 777.0, 777.0, 777.0,
    ],
    [
        25.0, 777.0, 777.0, 777.0, 120.0, 120.0, 108.0, 96.0, 96.0, 96.0,
    ],
    [
        30.0, 777.0, 777.0, 120.0, 108.0, 108.0, 96.0, 72.0, 72.0, 72.0,
    ],
    [
        35.0, 777.0, 120.0, 108.0, 96.0, 72.0, 60.0, 48.0, 48.0, 48.0,
    ],
    [40.0, 120.0, 108.0, 96.0, 60.0, 48.0, 42.0, 32.0, 32.0, 32.0],
    [45.0, 108.0, 96.0, 72.0, 42.0, 32.0, 28.0, 24.0, 24.0, 24.0],
    [50.0, 96.0, 72.0, 60.0, 32.0, 28.0, 24.0, 21.0, 21.0, 21.0],
    [55.0, 80.0, 60.0, 48.0, 28.0, 24.0, 21.0, 18.0, 18.0, 18.0],
    [60.0, 72.0, 48.0, 42.0, 24.0, 21.0, 18.0, 16.0, 16.0, 18.0],
    [65.0, 68.0, 46.0, 32.0, 21.75, 19.0, 17.0, 15.0, 16.0, 18.0],
    [70.0, 64.0, 44.0, 28.0, 19.5, 18.0, 16.0, 14.5, 16.0, 18.0],
    [75.0, 60.0, 42.0, 24.0, 18.0, 16.0, 15.0, 14.0, 16.0, 18.0],
    [
        80.0, 56.0, 38.25, 23.0, 17.25, 15.81, 14.81, 14.0, 16.0, 18.0,
    ],
    [
        85.0, 52.0, 34.5, 22.0, 16.5, 15.625, 14.625, 14.0, 16.0, 18.0,
    ],
    [90.0, 48.0, 32.0, 21.0, 16.0, 15.5, 14.5, 14.0, 16.0, 18.0],
    [95.0, 45.0, 28.0, 19.5, 15.5, 15.0, 14.0, 13.5, 16.0, 18.0],
    [100.0, 42.0, 26.5, 18.5, 15.0, 14.5, 13.5, 13.0, 16.0, 18.0],
    [105.0, 39.0, 25.0, 18.0, 14.5, 14.0, 13.0, 12.0, 16.0, 18.0],
    [110.0, 36.0, 24.0, 18.0, 14.0, 13.0, 12.0, 11.0, 16.0, 18.0],
    [
        115.0, 34.5, 22.5, 17.25, 12.5, 11.875, 11.25, 10.625, 14.5, 16.5,
    ],
    [
        120.0, 33.0, 21.0, 16.5, 11.0, 10.75, 10.5, 10.25, 13.0, 15.0,
    ],
    [125.0, 32.0, 20.0, 16.0, 10.0, 10.0, 10.0, 10.0, 12.0, 14.0],
];

/// The library's pre-calculated deltas to the next row, as it has them
/// (`fontDeltaAscend`)
const FONT_DELTA: [[f64; 10]; 25] = [
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [10.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [15.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [20.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [25.0, 0.0, 0.0, 0.0, 12.0, 12.0, 12.0, 24.0, 24.0, 24.0],
    [30.0, 0.0, 0.0, 12.0, 12.0, 36.0, 36.0, 24.0, 24.0, 24.0],
    [35.0, 0.0, 12.0, 12.0, 36.0, 24.0, 18.0, 16.0, 16.0, 16.0],
    [40.0, 12.0, 12.0, 24.0, 18.0, 16.0, 14.0, 8.0, 8.0, 8.0],
    [45.0, 12.0, 24.0, 12.0, 10.0, 4.0, 4.0, 3.0, 3.0, 3.0],
    [50.0, 16.0, 12.0, 12.0, 4.0, 4.0, 3.0, 3.0, 3.0, 3.0],
    [55.0, 8.0, 12.0, 6.0, 4.0, 3.0, 3.0, 2.0, 2.0, 0.0],
    [60.0, 4.0, 2.0, 10.0, 2.25, 2.0, 1.0, 1.0, 0.0, 0.0],
    [65.0, 4.0, 2.0, 4.0, 2.25, 1.0, 1.0, 0.5, 0.0, 0.0],
    [70.0, 4.0, 2.0, 4.0, 1.5, 2.0, 1.0, 0.5, 0.0, 0.0],
    [75.0, 4.0, 3.75, 1.0, 0.75, 0.188, 0.188, 0.0, 0.0, 0.0],
    [80.0, 4.0, 3.75, 1.0, 0.75, 0.188, 0.188, 0.0, 0.0, 0.0],
    [85.0, 4.0, 2.5, 1.0, 0.5, 0.125, 0.125, 0.0, 0.0, 0.0],
    [90.0, 3.0, 4.0, 1.5, 0.5, 0.5, 0.5, 0.5, 0.0, 0.0],
    [95.0, 3.0, 1.5, 1.0, 0.5, 0.5, 0.5, 0.5, 0.0, 0.0],
    [100.0, 3.0, 1.5, 0.5, 0.5, 0.5, 0.5, 1.0, 0.0, 0.0],
    [105.0, 3.0, 1.0, 0.0, 0.5, 1.0, 1.0, 1.0, 0.0, 0.0],
    [110.0, 1.5, 1.5, 0.75, 1.5, 1.125, 0.75, 0.375, 1.5, 1.5],
    [115.0, 1.5, 1.5, 0.75, 1.5, 1.125, 0.75, 0.375, 1.5, 1.5],
    [120.0, 1.0, 1.0, 0.5, 1.0, 0.75, 0.5, 0.25, 1.0, 1.0],
    [125.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
];

/// The weights of the table's columns
pub const WEIGHTS: [u16; 9] = [100, 200, 300, 400, 500, 600, 700, 800, 900];

/// `byFontSize`: one row per font size in px, and in each the least Lc for
/// weights 100 to 900 (999 prohibited, 777 non-text and spot text only)
const BY_FONT_SIZE: [[f64; 10]; 16] = [
    [
        10.0, 999.0, 999.0, 999.0, 999.0, 999.0, 999.0, 999.0, 999.0, 999.0,
    ],
    [
        12.0, 999.0, 999.0, 999.0, 777.0, 777.0, 777.0, 777.0, 999.0, 999.0,
    ],
    [
        14.0, 999.0, 999.0, 777.0, 100.0, 100.0, 90.0, 75.0, 999.0, 999.0,
    ],
    [
        15.0, 999.0, 999.0, 777.0, 100.0, 90.0, 75.0, 70.0, 999.0, 999.0,
    ],
    [
        16.0, 999.0, 999.0, 777.0, 90.0, 75.0, 70.0, 60.0, 60.0, 999.0,
    ],
    [
        18.0, 999.0, 777.0, 100.0, 75.0, 70.0, 60.0, 55.0, 55.0, 55.0,
    ],
    [21.0, 999.0, 777.0, 90.0, 70.0, 60.0, 55.0, 50.0, 50.0, 50.0],
    [24.0, 999.0, 777.0, 75.0, 60.0, 55.0, 50.0, 45.0, 45.0, 45.0],
    [28.0, 999.0, 100.0, 70.0, 55.0, 50.0, 45.0, 43.0, 43.0, 43.0],
    [32.0, 999.0, 90.0, 65.0, 50.0, 45.0, 43.0, 40.0, 40.0, 40.0],
    [36.0, 999.0, 75.0, 60.0, 45.0, 43.0, 40.0, 38.0, 38.0, 38.0],
    [42.0, 100.0, 70.0, 55.0, 43.0, 40.0, 38.0, 35.0, 35.0, 35.0],
    [48.0, 90.0, 60.0, 50.0, 40.0, 38.0, 35.0, 33.0, 33.0, 33.0],
    [60.0, 75.0, 55.0, 45.0, 38.0, 35.0, 33.0, 30.0, 30.0, 30.0],
    [72.0, 60.0, 50.0, 40.0, 35.0, 33.0, 30.0, 30.0, 30.0, 30.0],
    [96.0, 50.0, 45.0, 35.0, 33.0, 30.0, 30.0, 30.0, 30.0, 30.0],
];

/// What the size-sorted table says of a size and weight
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Level {
    /// Content text needs at least this Lc
    Lc(f32),
    /// Only spot text (copyright, placeholder, disabled) and non-text:
    /// the table's 777, ARC's ®©
    SpotOnly,
    /// Too small or too thin for any text: the table's 999, ARC's ⊘
    Prohibited,
}

/// The size-sorted table's level for `size_px` at `weight`. Between two
/// size rows with levels, the level is interpolated; below a weight's
/// first row with a level, the row below decides (a 777 or 999); above
/// 96 px, the 96 px row
pub fn level(size_px: f32, weight: u16) -> Level {
    let Some(w) = column(weight) else {
        return Level::Prohibited;
    };
    let size = f64::from(size_px);
    let cell = |row: &[f64; 10]| row[w + 1];
    let at = |v: f64| {
        if v == 999.0 {
            Level::Prohibited
        } else if v == 777.0 {
            Level::SpotOnly
        } else {
            Level::Lc(v as f32)
        }
    };
    let Some(i) = BY_FONT_SIZE.iter().rposition(|row| row[0] <= size) else {
        return Level::Prohibited;
    };
    let (row, next) = (&BY_FONT_SIZE[i], BY_FONT_SIZE.get(i + 1));
    match next {
        Some(next) if cell(row) < 400.0 && cell(next) < 400.0 => {
            let t = (size - row[0]) / (next[0] - row[0]);
            Level::Lc((cell(row) + (cell(next) - cell(row)) * t) as f32)
        }
        _ => at(cell(row)),
    }
}

/// The least Lc of anything that must be seen: below it (the table's 999)
/// nothing may use the pair
pub const NON_TEXT_MIN_LC: f32 = 15.0;
/// The least Lc of any text: below it (the table's 777) a pair is for
/// non-text only. Also the floor of [`TextUse::SubFluent`] and
/// [`TextUse::Spot`]
pub const TEXT_MIN_LC: f32 = 30.0;
/// The most Lc of text larger than 24 px at weight 300 or more
pub const LARGE_TEXT_MAX_LC: f32 = 90.0;

/// What the table gives for one weight at one Lc
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MinFont {
    /// Text of this many px or more
    Px(f32),
    /// Too little contrast for text: non-text only (777)
    NonTextOnly,
    /// Too little contrast for anything (999)
    Prohibited,
}

/// The library's `fontLookupAPCA(lc)`: for each of the weights 100 to 900,
/// the least font size at contrast `lc` (its sign is ignored), interpolated
/// between the table's rows and rounded as the library rounds
pub fn font_lookup(lc: f64) -> [MinFont; 9] {
    // The library reads its header row below Lc 5 and fails above 129.9;
    // below 14.5 its every answer is "prohibited" either way
    let lc = lc.abs().min(125.0);
    if lc < 14.5 {
        return [MinFont::Prohibited; 9];
    }
    let row = (lc * 0.2).floor() as usize - 1;
    let adj = (lc - FONT_MATRIX[row][0]) * 0.2;
    std::array::from_fn(|w| {
        let font = FONT_MATRIX[row][w + 1];
        let delta = FONT_DELTA[row][w + 1];
        if font > 400.0 || lc < 29.5 {
            // A 777 or 999 cell, or below the text level: non-text only
            if font == 999.0 {
                MinFont::Prohibited
            } else {
                MinFont::NonTextOnly
            }
        } else if font > 24.0 {
            MinFont::Px((font - delta * adj + 0.5).floor() as f32)
        } else {
            MinFont::Px((font - (2.0 * delta * adj).trunc() * 0.5) as f32)
        }
    })
}

/// The table's column for `weight`: a weight between two columns takes the
/// lighter one's, which asks more
fn column(weight: u16) -> Option<usize> {
    (weight >= 100).then(|| (usize::from(weight.min(900)) / 100) - 1)
}

/// The least Lc fluent text (primary content) of `size_px` at `weight`
/// needs, by the size-sorted table ([`level`]). `None` when no contrast is
/// enough for content text there (spot-only, prohibited, or a weight below
/// 100)
pub fn required_lc(size_px: f32, weight: u16) -> Option<f32> {
    match level(size_px, weight) {
        Level::Lc(lc) => Some(lc),
        Level::SpotOnly | Level::Prohibited => None,
    }
}

/// What text is for, which moves the table's level (apca-w3's README)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextUse {
    /// Body text, read at length: Lc 15 more where the table asks less
    /// than Lc 75. As the README (and ARC's Gold level) word it, this is
    /// not monotonic: a cell of Lc 70 asks 85, one of Lc 75 asks 75
    Body,
    /// Fluent text, primary content: the table as it is
    #[default]
    Content,
    /// Sub-fluent text, not primary content: Lc 15 less, never below
    /// [`TEXT_MIN_LC`]
    SubFluent,
    /// Spot text, not read fluently (a copyright line, disabled text, a
    /// placeholder): Lc 25 less, never below [`TEXT_MIN_LC`]
    Spot,
}

/// The least Lc text of `size_px` at `weight` needs for `text_use`
pub fn required_lc_for(size_px: f32, weight: u16, text_use: TextUse) -> Option<f32> {
    let lc = match level(size_px, weight) {
        Level::Lc(lc) => lc,
        // A size for spot text only: the data's "Min non-content text"
        // level (`fontScoreGmin`, Lc 30)
        Level::SpotOnly if text_use == TextUse::Spot => return Some(TEXT_MIN_LC),
        Level::SpotOnly | Level::Prohibited => return None,
    };
    Some(match text_use {
        TextUse::Body if lc < 75.0 => lc + 15.0,
        TextUse::Body | TextUse::Content => lc,
        TextUse::SubFluent => (lc - 15.0).max(TEXT_MIN_LC),
        TextUse::Spot => (lc - 25.0).max(TEXT_MIN_LC),
    })
}

/// The most Lc text of this size and weight should have: Lc 90 above 24 px
/// at weight 300 or more, else no limit
pub fn max_lc(size_px: f32, weight: u16) -> Option<f32> {
    (size_px > 24.0 && weight >= 300).then_some(LARGE_TEXT_MAX_LC)
}

/// Whether `fg` text on `bg`, at `size_px` and `weight`, meets the table as
/// fluent content text (and stays within [`max_lc`])
pub fn passes(fg: Srgb, bg: Srgb, size_px: f32, weight: u16) -> bool {
    passes_for(fg, bg, size_px, weight, TextUse::Content)
}

/// [`passes`], for text of another use
pub fn passes_for(fg: Srgb, bg: Srgb, size_px: f32, weight: u16, text_use: TextUse) -> bool {
    let Some(min) = required_lc_for(size_px, weight, text_use) else {
        return false;
    };
    let lc = apca(fg, bg).abs() as f32;
    lc >= min && max_lc(size_px, weight).is_none_or(|max| lc <= max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(sizes: [f32; 9]) -> [MinFont; 9] {
        sizes.map(MinFont::Px)
    }

    /// Values of the library's function, worked through by hand from its
    /// source in an independent transliteration
    #[test]
    fn font_lookup_is_the_librarys() {
        assert_eq!(
            font_lookup(60.0),
            px([72.0, 48.0, 42.0, 24.0, 21.0, 18.0, 16.0, 16.0, 18.0])
        );
        assert_eq!(
            font_lookup(-68.541),
            px([65.0, 45.0, 29.0, 20.25, 18.5, 16.5, 15.0, 16.0, 18.0])
        );
        assert_eq!(
            font_lookup(75.0),
            px([60.0, 42.0, 24.0, 18.0, 16.0, 15.0, 14.0, 16.0, 18.0])
        );
        assert_eq!(
            font_lookup(77.7),
            px([58.0, 40.0, 23.5, 18.0, 16.0, 15.0, 14.0, 16.0, 18.0])
        );
        assert_eq!(
            font_lookup(90.0),
            px([48.0, 32.0, 21.0, 16.0, 15.5, 14.5, 14.0, 16.0, 18.0])
        );
        assert_eq!(
            font_lookup(103.2),
            px([40.0, 26.0, 18.5, 15.0, 14.5, 13.5, 12.5, 16.0, 18.0])
        );
        assert_eq!(
            font_lookup(125.0),
            px([32.0, 20.0, 16.0, 10.0, 10.0, 10.0, 10.0, 12.0, 14.0])
        );
        assert_eq!(
            font_lookup(47.3),
            px([102.0, 85.0, 66.0, 37.0, 30.0, 26.0, 23.0, 23.0, 23.0])
        );
        let at_30 = font_lookup(30.0);
        assert_eq!(&at_30[..2], &[MinFont::NonTextOnly; 2]);
        assert_eq!(at_30[3], MinFont::Px(108.0));
        assert_eq!(font_lookup(20.0), [MinFont::NonTextOnly; 9]);
        assert_eq!(font_lookup(10.0), [MinFont::Prohibited; 9]);
    }

    /// The size-sorted table's own cells, and between them
    #[test]
    fn the_least_lc_for_a_size_and_weight() {
        for (size, weight, want) in [
            (16.0, 400, Some(90.0)),
            (14.0, 400, Some(100.0)),
            (14.0, 600, Some(90.0)),
            (14.0, 700, Some(75.0)),
            (15.0, 700, Some(70.0)),
            (18.0, 400, Some(75.0)),
            (24.0, 300, Some(75.0)),
            (24.0, 400, Some(60.0)),
            (20.0, 400, Some(75.0 - 5.0 * 2.0 / 3.0)),
            (96.0, 700, Some(30.0)),
            (120.0, 400, Some(33.0)),
            (12.0, 500, None),
            (13.0, 400, None),
            (9.0, 400, None),
        ] {
            let got = required_lc(size, weight);
            assert!(
                match (got, want) {
                    (Some(g), Some(w)) => (g - w).abs() < 1e-4,
                    (g, w) => g == w,
                },
                "{size} px at {weight}: {got:?}, not {want:?}"
            );
        }
        assert_eq!(level(12.0, 400), Level::SpotOnly);
        assert_eq!(level(12.0, 800), Level::Prohibited);
        assert_eq!(level(14.0, 300), Level::SpotOnly);
        // Between columns, the lighter weight's; none below 100
        assert_eq!(required_lc(16.0, 450), required_lc(16.0, 400));
        assert_eq!(required_lc(16.0, 50), None);
        assert_eq!(required_lc(20.0, 1000), required_lc(20.0, 900));
    }

    /// Monotonic: a larger size never asks more, nor a heavier weight up to
    /// 700 (the table holds 800 and 900 back at small sizes on purpose)
    #[test]
    fn larger_and_heavier_never_ask_more() {
        let ask = |size: f32, weight| match level(size, weight) {
            Level::Lc(lc) => lc,
            Level::SpotOnly => 200.0,
            Level::Prohibited => 300.0,
        };
        for weight in WEIGHTS {
            for tenths in 80..1200 {
                let size = tenths as f32 / 10.0;
                assert!(
                    ask(size + 0.1, weight) <= ask(size, weight) + 1e-4,
                    "{size} px at {weight}"
                );
            }
        }
        for tenths in 80..1200 {
            let size = tenths as f32 / 10.0;
            for pair in WEIGHTS[..7].windows(2) {
                assert!(
                    ask(size, pair[1]) <= ask(size, pair[0]) + 1e-4,
                    "{size} px, {} then {}",
                    pair[0],
                    pair[1]
                );
            }
        }
    }

    #[test]
    fn uses_move_the_level() {
        assert_eq!(required_lc_for(21.0, 400, TextUse::Body), Some(85.0));
        assert_eq!(required_lc_for(18.0, 400, TextUse::Body), Some(75.0));
        assert_eq!(required_lc_for(16.0, 400, TextUse::Body), Some(90.0));
        assert_eq!(required_lc_for(16.0, 400, TextUse::SubFluent), Some(75.0));
        assert_eq!(required_lc_for(16.0, 400, TextUse::Spot), Some(65.0));
        assert_eq!(required_lc_for(48.0, 700, TextUse::Spot), Some(TEXT_MIN_LC));
        // A spot-only size: spot text at Lc 30, nothing else
        assert_eq!(required_lc_for(12.0, 400, TextUse::Spot), Some(TEXT_MIN_LC));
        assert_eq!(required_lc_for(12.0, 400, TextUse::SubFluent), None);
        assert_eq!(required_lc_for(10.0, 400, TextUse::Spot), None);
        assert_eq!(
            (max_lc(32.0, 400), max_lc(24.0, 400), max_lc(32.0, 200)),
            (Some(90.0), None, None)
        );
    }

    #[test]
    fn passes_judges_by_size_and_weight() {
        let (darkest, light) = (Srgb::hex(0x222222), Srgb::hex(0xebebeb));
        // Lc 91.0: 16 px regular reads; 14 px regular asks Lc 100
        assert!(passes(darkest, light, 16.0, 400));
        assert!(!passes(darkest, light, 14.0, 400));
        assert!(passes(darkest, light, 14.0, 700));
        // 32 px headings above Lc 90 have too much
        assert!(!passes(darkest, light, 32.0, 400));
        assert!(passes(Srgb::hex(0x333333), light, 32.0, 400));
        assert!(!passes(darkest, light, 9.0, 400), "no contrast is enough");
    }
}
