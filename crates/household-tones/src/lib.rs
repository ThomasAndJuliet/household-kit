//! The household's tone maths, in plain sRGB: no libcosmic or iced types.
//!
//! - [`color`]: sRGB and OKLab/OKLCH colours, as plain `f32` channels.
//! - [`tones`]: the adjustments every app makes to its theme (household rule
//!   25a): large surfaces held below glaring white, coloured text at a
//!   readable lightness, the muted accent and what goes on it, and the
//!   hover, selected and pressed states of a held surface.
//! - [`fill`]: what is drawn on a fill (the fill rule) and the selection
//!   tint, for solid fills and tints alike (rule 6).
//! - [`roles`]: Juliet's seven tones named by role, their light and dark
//!   values and the two arrangements (rule 25d).
//! - [`apca`]: the APCA-W3 0.0.98G-4g lightness contrast (rule 25c).
//! - [`fonts`]: APCA's font lookup table, the least Lc text of a size and
//!   weight needs, and [`passes`] to judge a text style by it.
//!
//! An app's glue turns its theme's colours into [`Srgb`]/[`Srgba`] and back
//! (`[f32; 3]` and `[f32; 4]` convert both ways), and keeps walking its own
//! libcosmic theme itself.

pub mod apca;
pub mod borders;
pub mod color;
pub mod fill;
pub mod fonts;
pub mod roles;
pub mod tones;

pub use apca::{apca, apca_over, apca_rgb8};
pub use borders::*;
pub use color::{Oklab, Oklch, Srgb, Srgba};
pub use fill::*;
pub use fonts::{Level, TextUse, level, max_lc, passes, passes_for, required_lc, required_lc_for};
pub use roles::{Arrangement, Mode, Role, Surface};
pub use tones::*;
