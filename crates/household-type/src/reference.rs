//! A sample, and where the terminal sets it: each line of text (its left
//! edge, baseline, width and size), each bullet and number, the quotation's
//! bar, the table's rules and the display formula's box, in the column's
//! coordinates (its left edge, and the first block's top, at 0).
//!
//! Measured from cosmic-term origin/main f79a869e (16 Oct) with
//! `ThreadStyle::with_paper` on COSMIC's light theme and the default
//! settings: the Paper view at 20 px in a column 960 px wide (its measure),
//! the Text view at 16 px in one 832 px wide. The fonts are the ones both
//! apps bundle (Libertinus Serif) or find on the system (the interface's
//! sans-serif, Noto Sans Mono); both apps shape with cosmic-text 0.19.
//!
//! An app's test lays [`SAMPLE`] out in its own view, measures the same
//! things, and passes them to [`compare`]. A value changed in one app
//! alone, or here alone, makes the other fail. To change the typography,
//! change [`crate::paper`] or [`crate::text`], measure the terminal again
//! (`docs` in the README), and update both apps in step.

pub const SAMPLE: &str = include_str!("sample.md");

/// What is measured
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A line of text: `x` its left edge, `y` its baseline, `w` its width,
    /// `h` its first glyph's size
    Text,
    /// A list item's bullet or number, measured as a line of text
    Marker,
    /// A quotation's bar
    Bar,
    /// A rule under a table's row
    TableRule,
    /// A display formula's box
    Formula,
}

/// One thing, where the reference sets it
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub kind: Kind,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// A line's first characters (up to 14), to tell lines apart
    pub text: &'static str,
}

/// One thing, where an app sets it
#[derive(Clone, Debug, PartialEq)]
pub struct Measured {
    pub kind: Kind,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub text: String,
}

/// A view's reference: its text size and line spacing, the column's
/// width, and where everything goes
#[derive(Clone, Copy, Debug)]
pub struct Reference {
    pub size: f32,
    pub line: f32,
    pub width: f32,
    pub marks: &'static [Mark],
}

pub const PAPER: Reference = Reference { size: 20.0, line: 1.5, width: 960.0, marks: measured::PAPER };
pub const TEXT: Reference = Reference { size: 16.0, line: 1.5, width: 832.0, marks: measured::TEXT };

/// How far apart two positions may be, in pixels, and still match
pub const TOLERANCE: f32 = 0.5;

/// Where `measured` differs from `reference` by more than `tolerance`
/// pixels, one line per difference; empty if it matches. Each kind is
/// compared in reading order (down, then across)
pub fn compare(reference: &Reference, measured: &[Measured], tolerance: f32) -> Vec<String> {
    let mut out = Vec::new();
    let order = |a: &(f32, f32), b: &(f32, f32)| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1));
    for kind in [Kind::Text, Kind::Marker, Kind::Bar, Kind::TableRule, Kind::Formula] {
        let mut want: Vec<&Mark> = reference.marks.iter().filter(|m| m.kind == kind).collect();
        want.sort_by(|a, b| order(&(a.y, a.x), &(b.y, b.x)));
        let mut got: Vec<&Measured> = measured.iter().filter(|m| m.kind == kind).collect();
        got.sort_by(|a, b| order(&(a.y, a.x), &(b.y, b.x)));
        if want.len() != got.len() {
            out.push(format!("{kind:?}: {} in the reference, {} set", want.len(), got.len()));
        }
        for (w, g) in want.iter().zip(&got) {
            let far = [(w.x, g.x), (w.y, g.y), (w.w, g.w), (w.h, g.h)].iter().any(|(a, b)| (a - b).abs() > tolerance);
            let same_text = matches!(kind, Kind::Text | Kind::Marker).then(|| g.text.starts_with(w.text) || w.text.starts_with(g.text.as_str())).unwrap_or(true);
            if far || !same_text {
                out.push(format!(
                    "{kind:?} {:?}: reference x {:.2} y {:.2} w {:.2} h {:.2}, set {:?} x {:.2} y {:.2} w {:.2} h {:.2}",
                    w.text, w.x, w.y, w.w, w.h, g.text, g.x, g.y, g.w, g.h
                ));
            }
        }
    }
    out
}

mod measured;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reference_matches_itself_and_catches_a_shift() {
        for r in [PAPER, TEXT] {
            let same: Vec<Measured> = r.marks.iter().map(|m| Measured { kind: m.kind, x: m.x, y: m.y, w: m.w, h: m.h, text: m.text.to_string() }).collect();
            assert!(compare(&r, &same, TOLERANCE).is_empty());
            let mut moved = same.clone();
            moved[3].y += 1.0;
            assert_eq!(compare(&r, &moved, TOLERANCE).len(), 1);
            assert!(!compare(&r, &same[1..], TOLERANCE).is_empty());
        }
    }

    #[test]
    fn the_reference_follows_the_styles() {
        // Its first line is Heading 1, at the Paper view's scale; the code
        // block's lines at the code size
        let p = crate::paper();
        let first = PAPER.marks.iter().find(|m| m.kind == Kind::Text).unwrap();
        assert_eq!((first.text, first.h), ("Heading one", p.heading_size(1, 20.0)));
        let code = PAPER.marks.iter().find(|m| m.text.starts_with("fn main")).unwrap();
        assert!((code.h - p.code_block_size(20.0)).abs() < 0.01);
        assert!((code.x - p.plate_pad * 20.0).abs() < 0.01);
    }
}
