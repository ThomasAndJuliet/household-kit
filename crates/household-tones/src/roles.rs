//! Seven tones, named by role (household rule 25d, Juliet 16 Oct; it
//! replaces the surface and text-tone parts of rule 25a).
//!
//! The names describe the light theme. In dark mode the text, hover,
//! selected and page/card roles take their mirror values, so they mean the
//! same contrast with the text. **Lightest does not mirror**: dropdowns and
//! menus stay lighter than the page, because in dark mode depth is shown by
//! lightening (shadows barely show), and a near-black menu would read as a
//! hole.
//!
//! | Role | Used for | Light | Dark |
//! |---|---|---|---|
//! | darkest | text under 18 px, headings, captions, interface items, icons | #222222 | #ebebeb |
//! | dark | body text of 18 px or more (in practice Paper body text) | #525252 | #d0d0d0 |
//! | darkish | hover: a step darker than the surface it sits on | wash over the surface | wash over the surface |
//! | mid | selected (tabs, rows, chosen parts), between hover and the page | ≈ #cbcbcb | #434343 |
//! | lightish | COSMIC's background grey | #d7d7d7 | #272727 |
//! | light | the 0.94 surface | #ebebeb | #1b1b1b |
//! | lightest | dropdowns, menus, tooltips, highlights; never long reading | #f5f5f5 | #343434 (not mirrored) |
//!
//! Placeholders and disabled text use a faded darkest (rule 25c's
//! spot-reading level, Lc 30), not a separate grey.

use crate::color::{Srgb, Srgba};
use crate::tones::over;

/// Light or dark
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mode {
    #[default]
    Light,
    Dark,
}

/// A tone by its role
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Role {
    Darkest,
    Dark,
    Darkish,
    Mid,
    Lightish,
    Light,
    Lightest,
}

impl Role {
    /// Darkest to lightest
    pub const ALL: [Role; 7] = [
        Role::Darkest,
        Role::Dark,
        Role::Darkish,
        Role::Mid,
        Role::Lightish,
        Role::Light,
        Role::Lightest,
    ];

    /// Its name, as the rule writes it
    pub fn name(self) -> &'static str {
        match self {
            Role::Darkest => "darkest",
            Role::Dark => "dark",
            Role::Darkish => "darkish",
            Role::Mid => "mid",
            Role::Lightish => "lightish",
            Role::Light => "light",
            Role::Lightest => "lightest",
        }
    }

    /// Its colour in `mode`. Darkish has none of its own: it is the hover
    /// wash laid over the surface below ([`hover`])
    pub fn tone(self, mode: Mode) -> Option<Srgb> {
        let (light, dark) = match self {
            Role::Darkest => (0x222222, 0xebebeb),
            Role::Dark => (0x525252, 0xd0d0d0),
            Role::Darkish => return None,
            Role::Mid => (0xcbcbcb, 0x434343),
            Role::Lightish => (0xd7d7d7, 0x272727),
            Role::Light => (0xebebeb, 0x1b1b1b),
            Role::Lightest => (0xf5f5f5, 0x343434),
        };
        Some(Srgb::hex(if mode == Mode::Dark { dark } else { light }))
    }

    /// Whether its dark value is its light one's mirror (the same contrast
    /// with the text). Lightest is not: it stays the lightest surface,
    /// keeping elevation
    pub fn mirrors(self) -> bool {
        self != Role::Lightest
    }
}

/// Darkish, the hover: the theme's hover wash over the surface it sits on
pub fn hover(surface: Srgb, wash: Srgba) -> Srgb {
    over(wash, surface.into()).rgb()
}

/// Where something sits, for the surface it is drawn on
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Surface {
    Page,
    PromptBox,
    Card,
    SidePanel,
    Sidebar,
    /// Dropdowns, menus and tooltips
    Popup,
}

/// Which surfaces take light and which lightish: a shared setting ("Page:
/// Light | Lightish"). Each pairs the lower-contrast surface with the
/// stronger text, and the choice is taste. Dark-mode arrangement A is
/// COSMIC's own dark theme
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Arrangement {
    /// Page and prompt box on light; cards, side panels and sidebars on
    /// lightish (the default)
    #[default]
    A,
    /// Page and prompt box on lightish; the others on light
    B,
}

impl Arrangement {
    /// The role of a surface's colour
    pub fn role(self, surface: Surface) -> Role {
        let page = matches!(surface, Surface::Page | Surface::PromptBox);
        match (surface, self) {
            (Surface::Popup, _) => Role::Lightest,
            (_, Arrangement::A) if page => Role::Light,
            (_, Arrangement::A) => Role::Lightish,
            (_, Arrangement::B) if page => Role::Lightish,
            (_, Arrangement::B) => Role::Light,
        }
    }

    /// A surface's colour in `mode`
    pub fn tone(self, surface: Surface, mode: Mode) -> Srgb {
        self.role(surface)
            .tone(mode)
            .expect("surfaces have tones of their own")
    }
}

/// The role of text: darkest below 18 px on any surface, and for headings,
/// captions and interface items at any size; dark only for body text
/// (`body`, text read in paragraphs) of 18 px or more
pub fn text_role(size_px: f32, body: bool) -> Role {
    if body && size_px >= 18.0 {
        Role::Dark
    } else {
        Role::Darkest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apca::apca;

    #[test]
    fn the_tones_are_the_rules_and_ordered() {
        assert_eq!(Role::Darkest.tone(Mode::Light).unwrap().to_hex(), "#222222");
        assert_eq!(Role::Light.tone(Mode::Dark).unwrap().to_hex(), "#1b1b1b");
        assert_eq!(Role::Darkish.tone(Mode::Light), None);
        let fixed: Vec<Role> = Role::ALL
            .into_iter()
            .filter(|r| *r != Role::Darkish)
            .collect();
        // Light mode runs darkest to lightest; dark mode mirrors it, except
        // that lightest stays the lightest surface
        let l = |role: Role, mode| role.tone(mode).unwrap().lightness();
        for pair in fixed.windows(2) {
            assert!(
                l(pair[0], Mode::Light) < l(pair[1], Mode::Light),
                "{pair:?}"
            );
        }
        for pair in fixed[..fixed.len() - 1].windows(2) {
            assert!(
                l(pair[0], Mode::Dark) > l(pair[1], Mode::Dark),
                "{pair:?} in dark"
            );
        }
        assert!(!Role::Lightest.mirrors() && Role::Light.mirrors());
        assert!(
            l(Role::Lightest, Mode::Dark) > l(Role::Lightish, Mode::Dark)
                && l(Role::Lightest, Mode::Dark) > l(Role::Light, Mode::Dark)
        );
        // Selected sits below the page and the cards in light mode
        assert!(l(Role::Mid, Mode::Light) < l(Role::Lightish, Mode::Light));
    }

    #[test]
    fn the_arrangements_swap_page_and_cards() {
        assert_eq!(Arrangement::default(), Arrangement::A);
        assert_eq!(
            (
                Arrangement::A.role(Surface::Page),
                Arrangement::A.role(Surface::Card)
            ),
            (Role::Light, Role::Lightish)
        );
        assert_eq!(
            (
                Arrangement::B.role(Surface::PromptBox),
                Arrangement::B.role(Surface::Sidebar)
            ),
            (Role::Lightish, Role::Light)
        );
        for a in [Arrangement::A, Arrangement::B] {
            assert_eq!(a.role(Surface::Popup), Role::Lightest);
            assert_eq!(a.role(Surface::SidePanel), a.role(Surface::Card));
            assert_eq!(a.role(Surface::PromptBox), a.role(Surface::Page));
        }
        // Dark arrangement A is COSMIC's dark theme: page #1b1b1b, cards #272727
        assert_eq!(
            Arrangement::A.tone(Surface::Page, Mode::Dark).to_hex(),
            "#1b1b1b"
        );
        assert_eq!(
            Arrangement::A.tone(Surface::Card, Mode::Dark).to_hex(),
            "#272727"
        );
    }

    #[test]
    fn small_text_is_darkest_and_large_body_text_dark() {
        assert_eq!(text_role(16.0, true), Role::Darkest);
        assert_eq!(text_role(20.0, true), Role::Dark);
        assert_eq!(text_role(24.0, false), Role::Darkest, "a heading");
        assert_eq!(text_role(12.0, false), Role::Darkest);
    }

    /// Judged by APCA's font table ([`crate::fonts`]), the pairs that pass:
    /// 16 px regular prose in darkest on the light surfaces. The README lists
    /// the rule 25d pairs that do not
    #[test]
    fn prose_in_darkest_passes_on_the_clearest_surfaces() {
        use crate::fonts::{TextUse, passes_for};
        let darkest = |mode| Role::Darkest.tone(mode).unwrap();
        for surface in [Role::Light, Role::Lightest] {
            assert!(
                passes_for(
                    darkest(Mode::Light),
                    surface.tone(Mode::Light).unwrap(),
                    16.0,
                    400,
                    TextUse::Body
                ),
                "{}",
                surface.name()
            );
        }
        for surface in [Role::Light, Role::Lightish] {
            assert!(
                passes_for(
                    darkest(Mode::Dark),
                    surface.tone(Mode::Dark).unwrap(),
                    16.0,
                    400,
                    TextUse::Body
                ),
                "{} in dark",
                surface.name()
            );
        }
        // Paper's 20 px body text in dark asks Lc 84.5; dark on light gives 75.3
        let lc = apca(
            Role::Dark.tone(Mode::Light).unwrap(),
            Role::Light.tone(Mode::Light).unwrap(),
        );
        assert!((lc - 75.3).abs() < 0.1, "{lc}");
    }
}
