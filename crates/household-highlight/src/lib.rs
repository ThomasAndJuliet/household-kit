//! Code coloured by its language, the same way in every app (household
//! rule 25b): a token's syntax scope gives its [`Class`], and a class its
//! [`Colour`], from one table. Coloured by class, not by a syntect theme,
//! code takes the page's light or dark look like the text around it.
//!
//! From household-ui's `reference/code/highlight.rs` (and the copies in
//! Cerea's prose-render and cosmic-term's thread view, which have the same
//! table), without libcosmic's colours and without syntect: the app's glue
//! keeps its own parser, passes each token's scope names to [`class_of`],
//! and turns a [`Colour`] into its theme's colour. Some colours are the
//! theme's own (muted text, the strong link colour, added and removed);
//! the rest are fixed pairs, one for light plates and one for dark.

/// What a token is, as far as its colour goes
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    Keyword,
    Type,
    Function,
    String,
    Number,
    Constant,
    Comment,
    Tag,
    Attribute,
    Heading,
    /// A diff's added and removed lines, where a diff grammar parses them
    Inserted,
    Deleted,
}

impl Class {
    pub const ALL: [Class; 12] = [
        Class::Keyword,
        Class::Type,
        Class::Function,
        Class::String,
        Class::Number,
        Class::Constant,
        Class::Comment,
        Class::Tag,
        Class::Attribute,
        Class::Heading,
        Class::Inserted,
        Class::Deleted,
    ];
}

/// Where a class's colour comes from
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Colour {
    /// The household's code palette: `0xRRGGBB` for light plates (white or
    /// sepia paper among them) and for dark ones
    Fixed { light: u32, dark: u32 },
    /// The page's muted text colour
    Muted,
    /// The page's strong link colour
    LinkStrong,
    /// The theme's success text colour
    Added,
    /// The theme's destructive text colour
    Removed,
}

impl Colour {
    /// A fixed colour's `0xRRGGBB` on a dark plate or a light one; `None`
    /// for the theme's own colours
    pub fn fixed(self, dark: bool) -> Option<u32> {
        match self {
            Colour::Fixed { light, dark: on_dark } => Some(if dark { on_dark } else { light }),
            _ => None,
        }
    }
}

/// A class's colour (the household's code palette, from Cerea's, darkened
/// a little on light plates so each class reads on sepia paper's too)
pub fn colour(class: Class) -> Colour {
    let fixed = |light, dark| Colour::Fixed { light, dark };
    match class {
        Class::Comment => Colour::Muted,
        Class::Heading => Colour::LinkStrong,
        Class::Inserted => Colour::Added,
        Class::Deleted => Colour::Removed,
        Class::Keyword => fixed(0x7a2fd6, 0xc792ea),
        Class::Type | Class::Attribute => fixed(0x7f5500, 0xffcb6b),
        Class::Function => fixed(0x1a5aa8, 0x82aaff),
        Class::String => fixed(0x2a6e2e, 0xc3e88d),
        Class::Number | Class::Constant => fixed(0xa64500, 0xf78c6c),
        Class::Tag => fixed(0xb01a25, 0xf07178),
    }
}

/// Whether a class is set in italic
pub fn italic(class: Class) -> bool {
    class == Class::Comment
}

/// Whether a plate (the colour code is drawn on), sRGB channels 0 to 1, is
/// dark: which half of a [`Colour::Fixed`] pair it takes
pub fn is_dark(plate: [f32; 3]) -> bool {
    plate[0] * 0.299 + plate[1] * 0.587 + plate[2] * 0.114 < 0.5
}

/// Scope prefixes and their classes, the first match winning (Sublime
/// Text's scope names, as syntect's grammars give them). A scope matches a
/// prefix of whole dot-separated atoms: `keyword` matches
/// `keyword.control.rust`, not `keywords`
pub const SCOPES: &[(&str, Class)] = &[
    ("comment", Class::Comment),
    ("punctuation.definition.comment", Class::Comment),
    ("string", Class::String),
    ("constant.numeric", Class::Number),
    ("constant.character.escape", Class::Constant),
    ("constant.language", Class::Constant),
    ("constant", Class::Constant),
    ("keyword.operator", Class::Keyword),
    ("keyword", Class::Keyword),
    ("storage.type", Class::Keyword),
    ("storage.modifier", Class::Keyword),
    ("storage", Class::Keyword),
    ("entity.name.function", Class::Function),
    ("support.function", Class::Function),
    ("variable.function", Class::Function),
    ("meta.function-call", Class::Function),
    ("entity.name.type", Class::Type),
    ("entity.name.class", Class::Type),
    ("entity.name.struct", Class::Type),
    ("entity.name.enum", Class::Type),
    ("entity.name.trait", Class::Type),
    ("support.type", Class::Type),
    ("support.class", Class::Type),
    ("entity.name.tag", Class::Tag),
    ("entity.other.attribute-name", Class::Attribute),
    ("support.type.property-name", Class::Attribute),
    ("variable.other.member", Class::Attribute),
    ("markup.inserted", Class::Inserted),
    ("markup.deleted", Class::Deleted),
    ("meta.diff.header", Class::Comment),
    ("meta.diff.range", Class::Comment),
    ("markup.heading", Class::Heading),
    ("markup.bold", Class::Heading),
];

/// An operator is barely a keyword: a scope under this prefix is left plain
pub const PLAIN_SCOPE: &str = "keyword.operator";

/// Whether `prefix` is a prefix of `scope` in whole atoms
pub fn scope_has_prefix(scope: &str, prefix: &str) -> bool {
    scope.strip_prefix(prefix).is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}

/// One scope by [`SCOPES`]: `None` when no prefix matches (the search goes
/// on outwards), `Some(None)` for an operator (plain, the search stops),
/// else its class
fn scope_class(scope: &str) -> Option<Option<Class>> {
    let (_, class) = SCOPES.iter().find(|(prefix, _)| scope_has_prefix(scope, prefix))?;
    Some(if scope_has_prefix(scope, PLAIN_SCOPE) { None } else { Some(*class) })
}

/// The class of a token from its scope stack, outermost first: the
/// innermost scope that has a class decides, and an operator there leaves
/// the token plain
pub fn class_of<'a>(stack: impl IntoIterator<Item = &'a str, IntoIter: DoubleEndedIterator>) -> Option<Class> {
    stack.into_iter().rev().find_map(scope_class).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use household_tones::Srgb;

    #[test]
    fn the_innermost_scope_with_a_class_decides() {
        assert_eq!(class_of(["source.rust", "meta.function.rust", "storage.type.function.rust"]), Some(Class::Keyword));
        assert_eq!(class_of(["source.rust", "entity.name.function.rust"]), Some(Class::Function));
        assert_eq!(class_of(["source.rust", "comment.line.double-slash.rust", "punctuation.definition.comment.rust"]), Some(Class::Comment));
        assert_eq!(class_of(["source.rust", "string.quoted.double.rust", "constant.character.escape.rust"]), Some(Class::Constant));
        assert_eq!(class_of(["source.rust", "constant.numeric.integer.decimal.rust"]), Some(Class::Number));
        assert_eq!(class_of(["text.html.basic", "meta.tag", "entity.other.attribute-name.class.html"]), Some(Class::Attribute));
        assert_eq!(class_of(["source.diff", "markup.inserted.diff"]), Some(Class::Inserted));
        assert_eq!(class_of(["source.diff", "meta.diff.range.unified"]), Some(Class::Comment));
        // Operators are plain, even inside something coloured
        assert_eq!(class_of(["source.rust", "meta.function", "keyword.operator.arithmetic.rust"]), None);
        assert_eq!(class_of(["source.rust", "variable.other.rust"]), None);
        assert_eq!(class_of(Vec::<&str>::new()), None);
        // Whole atoms only
        assert!(scope_has_prefix("keyword.control", "keyword") && !scope_has_prefix("keywords.x", "keyword"));
    }

    #[test]
    fn colours_come_from_the_page_by_class() {
        assert_eq!(colour(Class::Comment), Colour::Muted);
        assert_eq!(colour(Class::Heading), Colour::LinkStrong);
        assert_eq!((colour(Class::Inserted), colour(Class::Deleted)), (Colour::Added, Colour::Removed));
        assert!(italic(Class::Comment) && !italic(Class::Keyword));
        let fixed: Vec<Class> = Class::ALL.into_iter().filter(|c| colour(*c).fixed(false).is_some()).collect();
        assert_eq!(fixed.len(), 8);
        let lightness = |hex: u32| {
            let c = Srgb::hex(hex);
            c.r * 0.299 + c.g * 0.587 + c.b * 0.114
        };
        for class in &fixed {
            let c = colour(*class);
            // Readable on its plate: dark on light, light on dark
            assert!(lightness(c.fixed(false).unwrap()) < 0.55, "{class:?} on light");
            assert!(lightness(c.fixed(true).unwrap()) > 0.45, "{class:?} on dark");
        }
        // The main classes tell apart
        let main = [Class::Keyword, Class::Type, Class::Function, Class::String, Class::Number, Class::Tag];
        for (i, a) in main.iter().enumerate() {
            for b in &main[i + 1..] {
                for dark in [false, true] {
                    assert_ne!(colour(*a).fixed(dark), colour(*b).fixed(dark), "{a:?} {b:?}");
                }
            }
        }
        assert!(is_dark([0.106, 0.106, 0.106]) && !is_dark([0.933, 0.933, 0.933]));
    }
}
