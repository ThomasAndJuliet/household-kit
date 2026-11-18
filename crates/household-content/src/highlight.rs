// SPDX-License-Identifier: GPL-3.0-only

//! Code split into token classes by its language (household rule 25b):
//! syntect's grammars (Sublime Text's) find each token's scope, and
//! household-highlight's table the scope's [`Class`]; the app gives each
//! class its colour. Moved from Terminal's thread view (`highlight.rs`),
//! whose aliases, file names and first-line clues are the richest of the
//! household's copies.
//!
//! The language of a code block comes from its label (with the usual
//! aliases), else from a file's name, else from a first-line clue: a `#!`
//! line, `<?xml`, `<!DOCTYPE html>`. Code with none of these stays plain:
//! it is never guessed from its content.
//!
//! Each piece of code is parsed once: its tokens are kept by its text and
//! language, so a frame never parses, and a reply streaming in parses only
//! its growing block again. Long code is coloured only so far.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::{Arc, LazyLock, Mutex};

pub use household_highlight::Class;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

/// Code is coloured only up to this many lines, and this many bytes; the
/// rest stays plain, as a huge output would take long to parse
pub const MAX_LINES: usize = 3000;
pub const MAX_BYTES: usize = 256 * 1024;
/// A line longer than this (minified code, a data dump) ends the colouring
pub const MAX_LINE: usize = 4000;

/// What "Colour as…" offers an unlabelled block: each language's name and
/// the label it is given, as household-ui's reference has them; Plain is
/// offered after them
pub const COLOUR_AS: [(&str, &str); 18] = [
    ("Bash", "bash"),
    ("C", "c"),
    ("C++", "cpp"),
    ("CSS", "css"),
    ("Diff", "diff"),
    ("Go", "go"),
    ("HTML", "html"),
    ("Java", "java"),
    ("JavaScript", "js"),
    ("JSON", "json"),
    ("LaTeX", "latex"),
    ("Markdown", "markdown"),
    ("Python", "python"),
    ("Ruby", "ruby"),
    ("Rust", "rust"),
    ("SQL", "sql"),
    ("XML", "xml"),
    ("YAML", "yaml"),
];

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

/// Each line's tokens: byte ranges within the line and their classes, in
/// order
pub type Lines = Arc<Vec<Vec<(Range<usize>, Class)>>>;

static CACHE: LazyLock<Mutex<HashMap<u64, Lines>>> = LazyLock::new(Default::default);

/// How many times code has been parsed, for tests of the cache (here and
/// in the apps)
#[doc(hidden)]
pub static PARSES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The grammar for a language, by its name, alias or an extension
fn syntax(lang: &str) -> Option<&'static SyntaxReference> {
    let lang = lang.trim().to_lowercase();
    if lang.is_empty() {
        return None;
    }
    // The aliases of household-ui's reference highlighter
    let name = match lang.as_str() {
        "sh" | "shell" | "zsh" | "fish" | "ksh" | "console" | "shell-session" | "shellsession" => {
            "bash"
        }
        "py" | "python3" | "py3" | "pyw" => "python",
        "js" | "javascript" | "jsx" | "mjs" | "cjs" | "node" => "js",
        // Near enough: TypeScript's grammar is not among syntect's own
        "ts" | "typescript" | "tsx" | "mts" | "cts" => "js",
        "json5" | "jsonc" => "json",
        "yml" => "yaml",
        "c++" | "cxx" | "hpp" | "cc" => "cpp",
        "cs" | "csharp" | "c#" => "cs",
        "rs" => "rust",
        "rb" => "ruby",
        "pl" | "pm" => "perl",
        "golang" => "go",
        "md" | "markdown" => "md",
        "tex" | "latex" => "tex",
        "hs" => "haskell",
        "kt" | "kotlin" => "java",
        "ml" | "ocaml" | "sml" => "ml",
        "objc" | "objective-c" => "m",
        "make" | "mk" | "mak" => "makefile",
        "patch" => "diff",
        "htm" | "xhtml" => "html",
        "svg" | "xsd" | "xsl" | "plist" => "xml",
        "bat" | "batch" | "cmd" => "bat",
        "elisp" | "emacs-lisp" | "el" => "lisp",
        "clj" | "cljs" => "clojure",
        // Plain on purpose: a label that says the block is not code
        "text" | "txt" | "plain" | "plaintext" | "output" | "log" => return None,
        other => other,
    };
    SYNTAXES
        .find_syntax_by_token(name)
        .or_else(|| SYNTAXES.find_syntax_by_extension(name))
        .or_else(|| {
            SYNTAXES
                .syntaxes()
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(name))
        })
        // A label of the plain-text grammar is no language
        .filter(|s| s.name != "Plain Text")
}

/// The language a label, alias or extension names, by its grammar's name
pub fn language(lang: &str) -> Option<&'static str> {
    syntax(lang).map(|s| s.name.as_str())
}

/// Whether `lang` names a language that is coloured
pub fn knows(lang: &str) -> bool {
    syntax(lang).is_some()
}

/// The language of a file, by its name or its extension
pub fn file_language(path: &str) -> Option<&'static str> {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let by_name = match name {
        "Makefile" | "makefile" | "GNUmakefile" => Some("makefile"),
        ".bashrc" | ".bash_profile" | ".zshrc" | ".profile" | "PKGBUILD" | ".envrc" => Some("bash"),
        _ => None,
    };
    if let Some(lang) = by_name {
        return language(lang);
    }
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() && !matches!(ext, "bashrc" | "zshrc") {
        // A dot file with no other extension: no language from its name
        return None;
    }
    language(ext)
}

/// The language a block's first line names, as household-ui's reference
/// reads it: a `#!` line's interpreter, `<?xml`, `<!DOCTYPE html>`.
/// Nothing else is guessed from the content
pub fn first_line_language(code: &str) -> Option<&'static str> {
    let first = code.trim_start_matches('\u{feff}').lines().next()?.trim();
    let lower = first.to_lowercase();
    if lower.starts_with("<?xml") {
        return language("xml");
    }
    if lower.starts_with("<!doctype html") {
        return language("html");
    }
    let line = first.strip_prefix("#!")?;
    let mut words = line.split_whitespace();
    let mut program = words.next()?.rsplit('/').next()?;
    if program == "env" {
        // `env -S python3 -u`: the first word that is not an option
        program = words.find(|w| !w.starts_with('-') && !w.contains('='))?;
    }
    // python3.12 is python, and so on
    let name = program.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    language(match name {
        "sh" | "bash" | "zsh" | "ksh" | "dash" | "ash" | "fish" => "bash",
        "python" | "pypy" => "python",
        "node" | "nodejs" | "deno" | "bun" => "js",
        "lua" | "luajit" => "lua",
        "Rscript" => "r",
        "tclsh" | "wish" => "tcl",
        "make" => "makefile",
        other => other,
    })
}

/// The language a code block is coloured in: its label's, else its first
/// line's clue's, else none
pub fn block_language(label: &str, code: &str) -> Option<&'static str> {
    language(label).or_else(|| first_line_language(code))
}

/// Whether a block's label makes it a diff, and the language of the code
/// in it if the label names one too (`diff rust`, as the thread labels a
/// file's edit)
pub fn diff_label(label: &str) -> Option<&str> {
    let mut words = label.split_whitespace();
    let first = words.next()?;
    matches!(first.to_ascii_lowercase().as_str(), "diff" | "patch")
        .then(|| words.next().unwrap_or(""))
}

/// The class of the innermost scope in `stack` that has one, by
/// household-highlight's table
fn class_of(stack: &ScopeStack) -> Option<Class> {
    static PREFIXES: LazyLock<Vec<(Scope, Class)>> = LazyLock::new(|| {
        household_highlight::SCOPES
            .iter()
            .filter_map(|(s, c)| Some((Scope::new(s).ok()?, *c)))
            .collect()
    });
    static PLAIN: LazyLock<Scope> =
        LazyLock::new(|| Scope::new(household_highlight::PLAIN_SCOPE).expect("a scope"));
    for scope in stack.as_slice().iter().rev() {
        for (prefix, class) in PREFIXES.iter() {
            if prefix.is_prefix_of(*scope) {
                // An operator is barely a keyword: left plain
                if PLAIN.is_prefix_of(*scope) {
                    return None;
                }
                return Some(*class);
            }
        }
    }
    None
}

/// The coloured tokens of each line of `code` (split at `\n`), in `lang`.
/// Empty for a language without a grammar; lines past the limits have none
pub fn lines(code: &str, lang: &str) -> Lines {
    let key = {
        let mut hasher = std::hash::DefaultHasher::new();
        (code, lang).hash(&mut hasher);
        hasher.finish()
    };
    if let Some(found) = CACHE.lock().unwrap().get(&key) {
        return found.clone();
    }
    let lines = Arc::new(syntax(lang).map_or_else(Vec::new, |syntax| parse(code, syntax)));
    let mut cache = CACHE.lock().unwrap();
    if cache.len() > 2048 {
        cache.clear();
    }
    cache.insert(key, lines.clone());
    lines
}

fn parse(code: &str, syntax: &SyntaxReference) -> Vec<Vec<(Range<usize>, Class)>> {
    PARSES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut out = Vec::new();
    let mut bytes = 0;
    for (i, line) in code.split('\n').enumerate() {
        bytes += line.len() + 1;
        if i >= MAX_LINES || bytes > MAX_BYTES || line.len() > MAX_LINE {
            break;
        }
        // The grammars want each line with its newline
        let with_newline = format!("{line}\n");
        let Ok(ops) = state.parse_line(&with_newline, &SYNTAXES) else {
            break;
        };
        let mut tokens: Vec<(Range<usize>, Class)> = Vec::new();
        let mut push = |range: Range<usize>, class: Option<Class>| {
            let range = range.start.min(line.len())..range.end.min(line.len());
            let Some(class) = class else { return };
            if range.is_empty() {
                return;
            }
            match tokens.last_mut() {
                Some((last, c)) if *c == class && last.end == range.start => last.end = range.end,
                _ => tokens.push((range, class)),
            }
        };
        let mut from = 0;
        for (offset, op) in ops {
            push(from..offset, class_of(&stack));
            let _ = stack.apply(&op);
            from = offset;
        }
        push(from..with_newline.len(), class_of(&stack));
        out.push(tokens);
    }
    out
}

/// Whether a class is set in italic
pub fn italic(class: Class) -> bool {
    household_highlight::italic(class)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class_at(code: &str, lang: &str, word: &str) -> Option<Class> {
        let lines = lines(code, lang);
        let at = code.find(word).unwrap();
        let line = code[..at].matches('\n').count();
        let col = at - code[..at].rfind('\n').map_or(0, |i| i + 1);
        lines
            .get(line)?
            .iter()
            .find(|(r, _)| r.start <= col && col < r.end)
            .map(|(_, c)| *c)
    }

    #[test]
    fn rust_python_and_shell_are_coloured() {
        let code = "fn main() {\n    // hi\n    let x = \"s\" + 42;\n}\n";
        assert_eq!(class_at(code, "rust", "fn"), Some(Class::Keyword));
        assert_eq!(class_at(code, "rs", "main"), Some(Class::Function));
        assert_eq!(class_at(code, "rust", "// hi"), Some(Class::Comment));
        assert_eq!(class_at(code, "rust", "\"s\""), Some(Class::String));
        assert_eq!(class_at(code, "rust", "42"), Some(Class::Number));
        let py = "def f(x):\n    return 'a'  # note\n";
        assert_eq!(class_at(py, "py", "def"), Some(Class::Keyword));
        assert_eq!(class_at(py, "python", "'a'"), Some(Class::String));
        assert_eq!(class_at(py, "python", "# note"), Some(Class::Comment));
        let sh = "if [ -f x ]; then echo \"hi\"; fi # done\n";
        assert_eq!(class_at(sh, "sh", "if"), Some(Class::Keyword));
        assert_eq!(class_at(sh, "bash", "\"hi\""), Some(Class::String));
        assert_eq!(class_at(sh, "zsh", "# done"), Some(Class::Comment));
    }

    #[test]
    fn languages_come_from_labels_and_their_aliases() {
        let rust = language("rust");
        assert!(rust.is_some());
        for alias in ["rs", "RUST", " rust "] {
            assert_eq!(language(alias), rust, "{alias}");
        }
        let python = language("python");
        for alias in ["py", "python3"] {
            assert_eq!(language(alias), python, "{alias}");
        }
        let bash = language("bash");
        for alias in ["sh", "shell", "zsh", "console"] {
            assert_eq!(language(alias), bash, "{alias}");
        }
        assert_eq!(language("ts"), language("js"));
        assert_eq!(language("yml"), language("yaml"));
        for plain in [
            "",
            "text",
            "plaintext",
            "output",
            "no-such-language",
            "isabelle-ish",
        ] {
            assert_eq!(language(plain), None, "{plain}");
        }
        assert!(
            knows("py") && knows("sh") && knows("rs") && knows("json") && !knows("isabelle-ish")
        );
        assert_eq!(diff_label("diff"), Some(""));
        assert_eq!(diff_label("diff rust"), Some("rust"));
        assert_eq!(diff_label("patch"), Some(""));
        assert_eq!(diff_label("rust"), None);
    }

    #[test]
    fn the_references_aliases_and_colour_as_languages_are_known() {
        for lang in [
            "py",
            "python",
            "python3",
            "sh",
            "bash",
            "zsh",
            "shell",
            "console",
            "rs",
            "rust",
            "ts",
            "tsx",
            "typescript",
            "js",
            "jsx",
            "javascript",
            "json",
            "yaml",
            "yml",
            "html",
            "xml",
            "css",
            "c",
            "h",
            "cpp",
            "c++",
            "cs",
            "csharp",
            "go",
            "golang",
            "java",
            "kt",
            "rb",
            "ruby",
            "pl",
            "perl",
            "php",
            "lua",
            "sql",
            "diff",
            "patch",
            "make",
            "makefile",
            "tex",
            "latex",
            "md",
            "markdown",
            "hs",
            "haskell",
            "ml",
            "ocaml",
            "r",
            "scala",
            "clojure",
            "lisp",
            "bat",
            "erlang",
            "d",
            "tcl",
            "svg",
        ] {
            assert!(knows(lang), "{lang}");
        }
        for (name, label) in COLOUR_AS {
            assert!(knows(label), "Colour as {name}");
        }
    }

    #[test]
    fn a_diff_grammar_gives_inserted_and_deleted_lines() {
        let diff = "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-old\n+new\n same\n";
        assert_eq!(class_at(diff, "patch", "old"), Some(Class::Deleted));
        assert_eq!(class_at(diff, "diff", "new"), Some(Class::Inserted));
        assert_eq!(class_at(diff, "diff", "@@"), Some(Class::Comment));
        assert_eq!(class_at(diff, "diff", "same"), None);
    }

    #[test]
    fn languages_come_from_file_names() {
        assert_eq!(file_language("/p/src/main.rs"), language("rust"));
        assert_eq!(file_language("notes/plot.py"), language("python"));
        assert_eq!(file_language("build.sh"), language("bash"));
        assert_eq!(file_language("/home/x/.bashrc"), language("bash"));
        assert_eq!(file_language("Makefile"), language("makefile"));
        assert_eq!(file_language("a/b/config.yml"), language("yaml"));
        assert_eq!(file_language("index.html"), language("html"));
        assert_eq!(file_language("README"), None);
        assert_eq!(file_language("notes.txt"), None);
        assert_eq!(file_language(".gitignore"), None);
    }

    #[test]
    fn languages_come_from_first_line_clues_and_never_from_content() {
        assert_eq!(
            first_line_language("#!/bin/bash\necho hi"),
            language("bash")
        );
        assert_eq!(
            first_line_language("#!/usr/bin/env python3\nprint(1)"),
            language("python")
        );
        assert_eq!(
            first_line_language("#!/usr/bin/env -S python3.12 -u\n"),
            language("python")
        );
        assert_eq!(first_line_language("#!/usr/bin/env node\n"), language("js"));
        assert_eq!(
            first_line_language("<?xml version=\"1.0\"?>\n<a/>"),
            language("xml")
        );
        assert_eq!(
            first_line_language("<!DOCTYPE html>\n<html>"),
            language("html")
        );
        assert_eq!(first_line_language("<!doctype html>"), language("html"));
        assert_eq!(
            first_line_language("#!/usr/bin/python3.12\n"),
            language("python")
        );
        assert_eq!(
            first_line_language("#!/usr/bin/perl -w\n"),
            language("perl")
        );
        // An interpreter without a grammar, `<html>` and nothing: no clue
        assert_eq!(first_line_language("#!/usr/bin/env no-such-shell\n"), None);
        assert_eq!(first_line_language("<html>\n"), None);
        assert_eq!(first_line_language(""), None);
        // Code that looks like a language, but says not which, stays plain
        assert_eq!(
            first_line_language("fn main() {\n    println!(\"hi\");\n}"),
            None
        );
        assert_eq!(first_line_language("import os\nprint(os.getcwd())"), None);
        assert_eq!(block_language("", "def f():\n    pass"), None);
        // The label comes first, then the clue
        assert_eq!(block_language("python", "#!/bin/sh\n"), language("python"));
        assert_eq!(block_language("", "#!/bin/sh\n"), language("bash"));
        assert_eq!(block_language("text", "#!/bin/sh\n"), language("bash"));
    }

    #[test]
    fn unknown_languages_are_plain() {
        assert!(lines("whatever", "no-such-language").is_empty());
        assert!(lines("x = 1", "").is_empty());
    }

    #[test]
    fn code_is_parsed_once_however_often_it_is_drawn() {
        use std::sync::atomic::Ordering;
        let code = "fn cached_once() -> u32 { 7 }\n";
        let before = PARSES.load(Ordering::Relaxed);
        let first = lines(code, "rust");
        assert!(
            PARSES.load(Ordering::Relaxed) > before,
            "parsed the first time"
        );
        // The same tokens each time after: kept, not parsed again
        for _ in 0..50 {
            assert!(Arc::ptr_eq(&first, &lines(code, "rust")));
        }
    }

    #[test]
    fn long_code_is_coloured_only_so_far() {
        let line = "let x = 1;\n";
        let code = line.repeat(MAX_LINES + 100);
        let lines = lines(&code, "rust");
        assert_eq!(lines.len(), MAX_LINES);
        let long = format!(
            "let a = 1;\nlet b = \"{}\";\nlet c = 2;\n",
            "x".repeat(MAX_LINE + 1)
        );
        assert_eq!(super::lines(&long, "rust").len(), 1);
    }
}
