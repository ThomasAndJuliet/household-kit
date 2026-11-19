// SPDX-License-Identifier: GPL-3.0-only

//! Terminal PR 145's tests (`markdown.rs` at 031393ac), ported to the
//! shared analysis: a display formula containing a line such as `=` or
//! `- x` is one formula, never Markdown, in both dialects; and every
//! formula's range is its source.

use crate::Dialect;
use crate::maths::tests::FORMULAS;
use crate::syntax::{Analysis, ContainerKind, InlineKind, LeafKind, analyse_in};

/// The reply in Juliet's screenshot of 30 Oct, as Codex wrote it
const HER_REPLY: &str = "Classically, conjugation acts by algebra automorphisms:\n\\[\nh\\cdot(ab)=(h\\cdot a)(h\\cdot b).\n\\]\nFor a Hopf algebra \\(H\\), the corresponding requirement is\n\\[\nx\\cdot(a\\star b)\n=\n(x_{(1)}\\cdot a)\\star(x_{(2)}\\cdot b).\n\\]\nThe coproduct replaces the instruction “apply the same group element to both factors”.\n\nNow suppose \\(H\\) is quasitriangular.\n";

const DIALECTS: [Dialect; 2] = [Dialect::Chat, Dialect::Document];

/// Every formula, display or not, with whether it is display maths
fn formulas(a: &Analysis) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for leaf in &a.leaves {
        match &leaf.kind {
            LeafKind::Math { latex } => out.push((latex.trim().to_string(), true)),
            _ => {
                for inline in &leaf.inlines {
                    if let InlineKind::Math { latex, display } = &inline.kind {
                        out.push((latex.trim().to_string(), *display));
                    }
                }
            }
        }
    }
    out
}

fn display(a: &Analysis) -> Vec<String> {
    formulas(a)
        .into_iter()
        .filter(|(_, d)| *d)
        .map(|(l, _)| l)
        .collect()
}

/// The kinds of the leaves that are not spacers
fn kinds(a: &Analysis) -> Vec<&LeafKind> {
    a.leaves
        .iter()
        .map(|l| &l.kind)
        .filter(|k| **k != LeafKind::Spacer)
        .collect()
}

/// A leaf's text as written, less its container prefixes
fn text_of(text: &str, a: &Analysis, kind: fn(&LeafKind) -> bool) -> Vec<String> {
    a.leaves
        .iter()
        .filter(|l| kind(&l.kind))
        .map(|l| crate::syntax::plain_text(text, l))
        .collect()
}

#[test]
fn her_reply_is_sentences_and_formulas_with_no_titles() {
    for dialect in DIALECTS {
        let a = analyse_in(HER_REPLY, dialect);
        assert!(
            !a.leaves
                .iter()
                .any(|l| matches!(l.kind, LeafKind::Heading(_))),
            "{a:#?}"
        );
        assert_eq!(
            display(&a),
            [
                "h\\cdot(ab)=(h\\cdot a)(h\\cdot b).",
                "x\\cdot(a\\star b)\n=\n(x_{(1)}\\cdot a)\\star(x_{(2)}\\cdot b)."
            ]
        );
        assert_eq!(
            formulas(&a).iter().filter(|(l, d)| !d && l == "H").count(),
            2
        );
        let paragraphs = text_of(HER_REPLY, &a, |k| *k == LeafKind::Paragraph);
        assert_eq!(paragraphs.len(), 4, "{paragraphs:?}");
        assert_eq!(
            paragraphs[0],
            "Classically, conjugation acts by algebra automorphisms:"
        );
        // Each formula leaf is its source, delimiters and all
        let sources: Vec<&str> = a
            .leaves
            .iter()
            .filter(|l| matches!(l.kind, LeafKind::Math { .. }))
            .map(|l| &HER_REPLY[l.range.clone()])
            .collect();
        assert_eq!(
            sources[1],
            "\\[\nx\\cdot(a\\star b)\n=\n(x_{(1)}\\cdot a)\\star(x_{(2)}\\cdot b).\n\\]"
        );
    }
}

/// A line under a formula's line is the formula's: whatever it looks like
/// to Markdown, and however the formula is written
#[test]
fn a_display_formula_is_a_formula_whatever_its_lines_contain() {
    for line in [
        "=",
        "==",
        "===",
        "-",
        "--",
        "---",
        "- b",
        "+ b",
        "* b",
        "1. b",
        "2) b",
        "> b",
        "# b",
        "## b",
        "| a | b |",
        "|---|---|",
        "<div>",
        "<!-- b",
        "    b",
        "[b]: c",
        "[^b]: c",
        "",
    ] {
        let formula = format!("a\n{line}\nz");
        for text in [
            format!("Some text\n$$\n{formula}\n$$\nMore text.\n"),
            format!("Some text\n\n$$\n{formula}\n$$\n\nMore text.\n"),
            format!("Some text\n\\[\n{formula}\n\\]\nMore text.\n"),
            format!("Some text\n\n\\[\n{formula}\n\\]\n\nMore text.\n"),
            format!("Some text\n\\[{formula}\\]\nMore text.\n"),
            format!("Some text $$\n{formula}\n$$ More text.\n"),
            if line.is_empty() {
                format!("Some text\n$$\n{formula}\n$$\nMore text.\n")
            } else {
                format!("Some text\n$${formula}$$\nMore text.\n")
            },
        ] {
            for dialect in DIALECTS {
                let a = analyse_in(&text, dialect);
                assert_eq!(
                    display(&a),
                    std::slice::from_ref(&formula),
                    "{dialect:?} {text:?}: {a:#?}"
                );
                assert!(
                    kinds(&a)
                        .iter()
                        .all(|k| matches!(k, LeafKind::Paragraph | LeafKind::Math { .. })),
                    "{dialect:?} {text:?}: {a:#?}"
                );
                assert!(a.containers.is_empty(), "{dialect:?} {text:?}: {a:#?}");
            }
        }
    }
}

/// Each of them, however it is delimited, and in a list and a quote, is
/// one display formula; on lines of its own, a leaf of its own
#[test]
fn formulas_are_parsed_as_formulas() {
    for formula in FORMULAS {
        for text in [
            format!("$$\n{formula}\n$$\n"),
            format!("$${formula}$$\n"),
            format!("\\[\n{formula}\n\\]\n"),
            format!("Before\n$$\n{formula}\n$$\nAfter\n"),
            format!("Before\n\\[\n{formula}\n\\]\nAfter\n"),
            format!("Before\n\n$$\n{formula}\n$$\n\nAfter\n"),
        ] {
            for dialect in DIALECTS {
                let a = analyse_in(&text, dialect);
                assert_eq!(
                    formulas(&a),
                    [(formula.to_string(), true)],
                    "{dialect:?} {text:?}: {a:#?}"
                );
                assert_eq!(
                    a.leaves
                        .iter()
                        .filter(|l| matches!(l.kind, LeafKind::Math { .. }))
                        .count(),
                    1,
                    "{dialect:?} {text:?}: {a:#?}"
                );
            }
        }
        // In a list item and a quote: the lines' indent or marks aren't the
        // formula's, but its leaf's prefixes
        let lines = |prefix: &str| formula.replace('\n', &format!("\n{prefix}"));
        for (text, container) in [
            (
                format!("- An item:\n\n  $$\n  {}\n  $$\n", lines("  ")),
                "item",
            ),
            (
                format!("> A quote:\n> $$\n> {}\n> $$\n", lines("> ")),
                "quote",
            ),
        ] {
            let a = analyse_in(&text, Dialect::Chat);
            assert_eq!(
                display(&a),
                [formula.to_string()],
                "in a {container}: {a:#?}"
            );
            let leaf = a
                .leaves
                .iter()
                .find(|l| matches!(l.kind, LeafKind::Math { .. }))
                .unwrap();
            let kind = &a.containers[*leaf.containers.last().unwrap()].kind;
            assert_eq!(
                matches!(kind, ContainerKind::Quote),
                container == "quote",
                "{a:#?}"
            );
            // What is shown of the leaf (its source less prefixes) is the
            // formula between its delimiters
            let shown = crate::display::show_with(
                &text,
                &a,
                a.leaves.iter().position(|l| l == leaf).unwrap(),
                Some(leaf.range.start),
                crate::display::Mode::Literate,
                Dialect::Chat,
            );
            assert_eq!(shown.text, format!("$$\n{formula}\n$$"), "in a {container}");
        }
    }
}

/// An environment on lines of its own, with no delimiters at all
#[test]
fn bare_environments_are_display_math() {
    for formula in FORMULAS.iter().filter(|f| f.starts_with("\\begin{")) {
        let name = formula["\\begin{".len()..].split('}').next().unwrap();
        if formula.matches(&format!("\\end{{{name}}}")).count() != 1
            || !formula.ends_with(&format!("\\end{{{name}}}"))
        {
            continue;
        }
        let text = format!("So:\n{formula}\nhence.\n");
        for dialect in DIALECTS {
            let a = analyse_in(&text, dialect);
            assert_eq!(
                formulas(&a),
                [(formula.to_string(), true)],
                "{text:?}: {a:#?}"
            );
            assert_eq!(
                text_of(&text, &a, |k| *k == LeafKind::Paragraph),
                ["So:", "hence."]
            );
        }
    }
    // One environment inside another of its name
    let nested = "\\begin{array}{c}\\begin{array}{c}a\\end{array}\\\\b\\end{array}";
    assert_eq!(
        formulas(&analyse_in(nested, Dialect::Chat)),
        [(nested.to_string(), true)]
    );
    // Set in a line of text it stays there
    let a = analyse_in(
        "Let $A =\n\\begin{pmatrix}a\\\\b\\end{pmatrix}$ be it.\n",
        Dialect::Chat,
    );
    assert_eq!(
        formulas(&a),
        [(
            "A =\n\\begin{pmatrix}a\\\\b\\end{pmatrix}".to_string(),
            false
        )]
    );
    // Not closed yet, as while it streams in, its text shows
    assert!(formulas(&analyse_in("\\begin{cases}\na", Dialect::Chat)).is_empty());
}

/// `$$\n\begin{align}…` has its own delimiters: none are added
#[test]
fn an_environment_inside_delimiters_is_one_formula() {
    for text in [
        "$$\n\\begin{align}\na&=b\n\\end{align}\n$$\n",
        "\\[\n\\begin{align}\na&=b\n\\end{align}\n\\]\n",
    ] {
        assert_eq!(
            formulas(&analyse_in(text, Dialect::Chat)),
            [("\\begin{align}\na&=b\n\\end{align}".to_string(), true)],
            "{text:?}"
        );
    }
}

/// Real titles keep their maths
#[test]
fn maths_is_kept_in_real_headings() {
    for (text, level) in [
        ("# The $x$ title\n", 1),
        ("## The \\(x\\) title\n", 2),
        ("The $x$ title\n===\n", 1),
        ("The \\(x\\) title\n---\n", 2),
        ("Over two lines,\nthe $x$ title\n===\n", 1),
    ] {
        let a = analyse_in(text, Dialect::Chat);
        assert_eq!(kinds(&a), [&LeafKind::Heading(level)], "{text:?}");
        assert_eq!(formulas(&a), [("x".to_string(), false)], "{text:?}");
    }
}

/// What only looks like a formula is left as it is
#[test]
fn dollars_that_open_no_formula_are_left_alone() {
    for text in [
        "A stray $$ in a line.\n\nAnother $$ in a line.\n",
        "```\n$$\na\n=\nb\n$$\n```\n",
        "~~~latex\n\\[\na\n=\nb\n\\]\n~~~\n",
        "> ```\n> $$\n> a\n> =\n> b\n> $$\n> ```\n",
        "`$$` opens a formula, and `$$` closes it.\n\n`\\[`, likewise.\n",
        "\\$\\$ escaped\n\\$\\$ twice\n",
    ] {
        for dialect in DIALECTS {
            assert!(
                formulas(&analyse_in(text, dialect)).is_empty(),
                "{dialect:?} {text:?}"
            );
        }
    }
}

/// The marker standing for a formula's line breaks is not in the text, and
/// a character reference that decodes to it is not a line break
#[test]
fn private_use_characters_are_not_formula_line_breaks() {
    for literal in [
        "a\u{e000}b",
        "\u{e000}0\u{e001}",
        "\u{e000}0\u{e001}\u{e000}1\u{e001}",
    ] {
        let formula = format!("a\n=\n{literal}");
        let text = format!("Before {literal}.\n\n$$\n{formula}\n$$\n\nAfter {literal}.");
        let a = analyse_in(&text, Dialect::Chat);
        assert_eq!(formulas(&a), [(formula, true)]);
        assert_eq!(
            text_of(&text, &a, |k| *k == LeafKind::Paragraph),
            [format!("Before {literal}."), format!("After {literal}.")]
        );
    }
    let a = analyse_in(
        "Before &#xE000;0&#xE001;.\n\n$$\na\n=\nb\n$$\n",
        Dialect::Chat,
    );
    assert_eq!(formulas(&a), [("a\n=\nb".to_string(), true)]);
}
