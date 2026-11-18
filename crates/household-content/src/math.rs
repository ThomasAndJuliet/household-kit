// SPDX-License-Identifier: GPL-3.0-only

//! LaTeX set as KaTeX sets it, by RaTeX, a pure-Rust port of KaTeX, as SVG
//! whose glyphs are paths, with its size and baseline. This crate holds the
//! household's one RaTeX pin.
//!
//! RaTeX lays a formula out in well under a millisecond, so formulas are
//! set when a view lays them out, and kept in a [`FormulaCache`] bounded by
//! the bytes of SVG it holds. The cache holds whatever the app makes of a
//! drawn formula (an image handle, say), so that no type of a UI toolkit is
//! named here. Moved from Terminal's thread view (`math.rs`).

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

/// A formula drawn as SVG
#[derive(Clone, Debug, PartialEq)]
pub struct Drawn {
    pub svg: String,
    pub width: f32,
    pub height: f32,
    /// From the top to the baseline
    pub baseline: f32,
}

/// LaTeX numbers `align` and its kind, but each formula here stands alone,
/// so every one would be (1); set them unnumbered, as their starred forms
pub fn unnumbered(latex: &str) -> String {
    let mut out = latex.to_string();
    for env in ["align", "equation", "gather", "multline", "eqnarray"] {
        out = out
            .replace(&format!("\\begin{{{env}}}"), &format!("\\begin{{{env}*}}"))
            .replace(&format!("\\end{{{env}}}"), &format!("\\end{{{env}*}}"));
    }
    // `\tag` lands on the wrong side in RaTeX, and numbers mean little here
    out
}

/// `latex` drawn as SVG at `size` pixels per em in `color`, as display
/// maths or in a line of text, or why it can't be
pub fn draw(latex: &str, display: bool, size: f32, color: [u8; 3]) -> Result<Drawn, String> {
    // Parsing and SVG generation are part of the same untrusted-content
    // boundary as layout. No app state or cache lock is held inside it.
    std::panic::catch_unwind(|| draw_inner(latex, display, size, color))
        .map_err(|_| "RaTeX could not render this formula".to_string())?
}

/// `latex` set as display math, as SVG for a file: `size` pixels per em,
/// in `color`
pub fn svg(latex: &str, size: f32, color: [u8; 3]) -> Result<String, String> {
    draw(latex, true, size, color).map(|drawn| drawn.svg)
}

fn draw_inner(latex: &str, display: bool, size: f32, color: [u8; 3]) -> Result<Drawn, String> {
    use ratex_layout::{LayoutOptions, layout, to_display_list};
    use ratex_svg::{SvgOptions, render_to_svg};
    use ratex_types::{color::Color, math_style::MathStyle};

    let source = unnumbered(latex);
    let ast = ratex_parser::parser::parse(&source).map_err(|err| format!("{err:?}"))?;
    let style = if display {
        MathStyle::Display
    } else {
        MathStyle::Text
    };
    let [r, g, b] = color.map(|c| f32::from(c) / 255.0);
    let options = LayoutOptions::default()
        .with_style(style)
        .with_color(Color::rgb(r, g, b));
    let list = to_display_list(&layout(&ast, &options));
    let size = f64::from(size);
    let pad = 1.0;
    let svg = render_to_svg(
        &list,
        &SvgOptions {
            font_size: size,
            padding: pad,
            embed_glyphs: true,
            ..Default::default()
        },
    )
    // RaTeX gives its size in points, which would scale it by 4/3
    .replacen("pt\" height=", "\" height=", 1)
    .replacen("pt\">", "\">", 1);
    Ok(Drawn {
        svg,
        width: (list.width * size + 2.0 * pad) as f32,
        height: ((list.height + list.depth) * size + 2.0 * pad) as f32,
        baseline: (list.height * size + pad) as f32,
    })
}

/// What a formula is cached by
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FormulaKey {
    pub latex: String,
    pub display: bool,
    /// Pixels per em, in quarters
    pub size: u32,
    pub color: [u8; 3],
}

impl FormulaKey {
    pub fn new(latex: &str, display: bool, size: f32, color: [u8; 3]) -> Self {
        FormulaKey {
            latex: latex.to_string(),
            display,
            size: (size * 4.0).round() as u32,
            color,
        }
    }
}

/// A formula set or why it could not be, when it was last used, and its
/// bytes
type Entry<T> = (Result<Arc<T>, String>, u64, usize);

/// Formulas set, or why they could not be, kept while they fit in a budget
/// of bytes; when it is passed, the least recently used half goes. `T` is
/// what the app keeps of a drawn formula
pub struct FormulaCache<T> {
    entries: HashMap<FormulaKey, Entry<T>>,
    bytes: usize,
    clock: u64,
    budget: usize,
}

impl<T> FormulaCache<T> {
    /// An empty cache holding at most `budget` bytes
    pub fn new(budget: usize) -> Self {
        FormulaCache {
            entries: HashMap::new(),
            bytes: 0,
            clock: 0,
            budget,
        }
    }

    /// What is kept for `key`, marked as just used
    pub fn get(&mut self, key: &FormulaKey) -> Option<Result<Arc<T>, String>> {
        self.clock += 1;
        let clock = self.clock;
        let (result, used, _) = self.entries.get_mut(key)?;
        *used = clock;
        Some(result.clone())
    }

    /// Keep `result` for `key`, `svg_bytes` of SVG if it was set
    pub fn insert(&mut self, key: FormulaKey, result: Result<Arc<T>, String>, svg_bytes: usize) {
        self.clock += 1;
        let bytes = key.latex.len() + 64 + svg_bytes;
        self.bytes += bytes;
        if let Some((_, _, old)) = self.entries.insert(key, (result, self.clock, bytes)) {
            self.bytes -= old;
        }
        if self.bytes > self.budget {
            // Drop the least recently used half
            let mut uses: Vec<u64> = self.entries.values().map(|(_, used, _)| *used).collect();
            uses.sort_unstable();
            let cutoff = uses[uses.len() / 2];
            let mut freed = 0;
            self.entries.retain(|_, (_, used, bytes)| {
                let keep = *used > cutoff;
                if !keep {
                    freed += *bytes;
                }
                keep
            });
            self.bytes -= freed;
        }
    }

    /// Everything dropped
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
        self.clock = 0;
    }

    /// The bytes and entries kept, for a memory readout
    pub fn usage(&self) -> (usize, usize) {
        (self.bytes, self.entries.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_formulas_with_a_baseline_inside_them() {
        let formula = draw(r"\int_0^1 x^2\,dx = \frac{1}{3}", true, 20.0, [0, 0, 0]).unwrap();
        assert!(formula.width > 50.0 && formula.height > 20.0);
        assert!(formula.baseline > 0.0 && formula.baseline < formula.height);
        let inline = draw("x^2", false, 20.0, [0, 0, 0]).unwrap();
        assert!(inline.height < formula.height);
    }

    #[test]
    fn numbered_environments_are_set_unnumbered() {
        assert_eq!(
            unnumbered(r"\begin{align}a\end{align}"),
            r"\begin{align*}a\end{align*}"
        );
        assert!(
            draw(
                r"\begin{align}a &= b \\ c &= d\end{align}",
                true,
                20.0,
                [0, 0, 0]
            )
            .is_ok()
        );
    }

    #[test]
    fn errors_are_reported() {
        assert!(draw(r"\frac{1}{", true, 20.0, [0, 0, 0]).is_err());
    }

    /// PR 145's formulas (Terminal), every one of which RaTeX sets
    #[test]
    fn every_formula_in_pr_145s_set_is_set() {
        for formula in crate::maths::tests::FORMULAS {
            assert!(draw(formula, true, 20.0, [0, 0, 0]).is_ok(), "{formula:?}");
        }
    }

    #[test]
    fn the_cache_keeps_the_recently_used_within_its_budget() {
        let mut cache: FormulaCache<usize> = FormulaCache::new(1000);
        let key = |n: usize| FormulaKey::new(&format!("x_{n}"), false, 16.0, [0, 0, 0]);
        for n in 0..10 {
            cache.insert(key(n), Ok(Arc::new(n)), 100);
            // The first is used all along
            assert!(cache.get(&key(0)).is_some());
        }
        let (bytes, entries) = cache.usage();
        assert!(bytes <= 1000 && entries < 10, "{bytes} bytes in {entries}");
        assert_eq!(*cache.get(&key(0)).unwrap().unwrap(), 0);
        assert!(cache.get(&key(1)).is_none());
        cache.insert(key(99), Err("no".into()), 0);
        assert_eq!(cache.get(&key(99)), Some(Err("no".to_string())));
        cache.clear();
        assert_eq!(cache.usage(), (0, 0));
    }
}
