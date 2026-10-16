// Superseded (16 Oct): cosmic-term now carries this probe as its drift test,
// src/thread/rich/type_drift.rs, which compares with the reference and, run
// with CT_TYPE_REFERENCE=<dir>, writes <dir>/measured.rs. Kept for reference.
//
// The layout probe that measured household-type's reference
// (src/reference/measured.rs) in cosmic-term. It is not compiled here.
//
// To measure again: copy src/sample.md to cosmic-term's
// src/thread/paper_sample.md, append this module to src/thread/rich.rs, and
// run `cargo test --bin cosmic-term paper_probe_dump`. It writes
// ~/probe-out/measured.rs; copy that to src/reference/measured.rs here.
//
// It lays the sample out as the thread's rich text widget does: each block
// with `build_block` at the column's width, placed `gap_before` below the
// one before, with the Paper view's (and the Text view's) typography from
// `ThreadStyle::with_paper` on COSMIC's light theme and the default
// settings.

#[cfg(test)]
mod paper_probe {
    use super::*;

    // household-type's src/sample.md, copied beside rich.rs
    const SAMPLE: &str = include_str!("paper_sample.md");

    fn mark(kind: &str, x: f32, y: f32, w: f32, h: f32, text: &str, out: &mut Vec<String>) {
        out.push(format!("    Mark {{ kind: Kind::{kind}, x: {x:.2}, y: {y:.2}, w: {w:.2}, h: {h:.2}, text: {} }},", serde_json::to_string(text).unwrap()));
    }

    fn lines_of(s: &Shaped, kind: &str, out: &mut Vec<String>) {
        for run in s.buffer.layout_runs() {
            let Some(first) = run.glyphs.first() else { continue };
            let last = run.glyphs.last().unwrap();
            let text: String = run.text[first.start..last.end].chars().take(14).collect();
            let x = s.origin.x + first.x;
            mark(kind, x, s.origin.y + run.line_y, s.origin.x + last.x + last.w - x, first.font_size, &text, out);
        }
    }

    fn rect(kind: &str, r: Rectangle, out: &mut Vec<String>) {
        mark(kind, r.x, r.y, r.width, r.height, "", out);
    }

    fn probe(paper: bool, width: f32) -> String {
        crate::thread::load_paper_fonts();
        let mut config = crate::config::Config::default();
        config.thread_paper = paper;
        let style = crate::thread::view::ThreadStyle::with_paper(&config, &cosmic::Theme::light(), paper);
        let t = (*style.body).clone();
        let doc = super::super::markdown::parse(SAMPLE);
        let mut out = Vec::new();
        super::super::shaping::with_font_system(|fs| {
            let mut shaper = Shaper::new(fs);
            let mut y = 0.0;
            let mut previous: Option<&Block> = None;
            for block in &doc.blocks {
                y += gap_before(previous, block, t.size);
                previous = Some(block);
                let layout = build_block(&mut shaper, &t, block, None, &Images { base_dir: None, host: String::new() }, width).shifted(y);
                match &layout.laid {
                    Laid::Cells(cells) => cells.iter().for_each(|c| lines_of(c, "Text", &mut out)),
                    Laid::Math(Ok(_), r) => rect("Formula", *r, &mut out),
                    _ => {}
                }
                if let Some(m) = &layout.marker {
                    lines_of(m, "Marker", &mut out);
                }
                for (r, _, _) in &layout.panels {
                    rect("Bar", *r, &mut out);
                }
                for (r, _, _) in &layout.inner_panels {
                    rect("TableRule", *r, &mut out);
                }
                y = layout.bottom;
            }
        });
        out.join("\n")
    }

    #[test]
    fn paper_probe_dump() {
        let dir = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("probe-out");
        std::fs::create_dir_all(&dir).unwrap();
        let file = format!(
            "//! Generated from cosmic-term origin/main f79a869e by its layout probe (see\n//! the README): the Paper view at 20 px in 960 px, the Text view at 16 px in\n//! 832 px. Do not edit by hand.\n\nuse super::{{Kind, Mark}};\n\npub const PAPER: &[Mark] = &[\n{}\n];\n\npub const TEXT: &[Mark] = &[\n{}\n];\n",
            probe(true, 960.0),
            probe(false, 832.0)
        );
        std::fs::write(dir.join("measured.rs"), file).unwrap();
    }
}
