// SPDX-License-Identifier: GPL-3.0-only

//! The household apps' shared libcosmic widgets: "use shared code as shared
//! UI component code to prevent having to maintain multiple versions of the
//! same code and having inconsistent UI" (Juliet, 24 Oct).
//!
//! - [`switch()`], [`switch_maybe`], [`toggler()`] and [`SwitchItem`]:
//!   libcosmic's switch in the muted accent (household rule 6).
//! - [`muted_colours`]: the muted accent of a theme, and what goes on it.
//! - [`text_box`]: a text box that does what a text box does: [`TextBox`],
//!   iced's text and caret with an undo history, its standard keys and
//!   Shift+click, and its selection drawn dimmed while unfocused.
//! - [`short_text_box`]: the one-line text box, [`ShortTextBox`] and its
//!   widget, sharing the text box's history, steps and keys.
//! - [`dropdown`]: a dropdown whose list is the window's one menu chain.
//! - [`outlined_list`]: rows in a form field with empty content and an error outline.
//! - [`scroll_list`]: a scrollable list of boxes, optionally virtualised,
//!   keeping the reader's place.
//! - [`segmented_control`]: native single-choice controls in the household style.
//! - [`tab_strip`]: the user-content tab strip (tabs the user opens and
//!   closes), distinct from the fixed-section [`tabs`].
//! - `content` (feature `content`): the content boxes, inline text and
//!   formulas from household-kit's content model.
//!
//! Plain colour maths stays in household-kit's `household-tones`; this
//! crate turns it into libcosmic's types.

#[macro_use]
mod forward;
#[cfg(feature = "content")]
pub mod content;
pub mod dropdown;
pub mod focus_regions;
#[doc(hidden)]
pub mod history;
pub mod hover_card;
mod kept;
pub mod layout;
mod level;
pub mod list_navigation;
pub mod look;
pub mod menu;
pub mod menu_bar;
pub mod menu_parts;
pub mod menu_surface;
pub mod navigation_style;
pub mod outlined_list;
pub mod panel_divider;
pub mod panel_header;
pub mod panel_layout;
pub mod panel_resize;
pub mod panel_state;
pub mod popup;
mod scroll;
pub mod scroll_intent;
pub mod scroll_list;
pub mod segmented_control;
pub mod short_text_box;
mod switch;
pub mod tab_strip;
pub mod tabs;

pub mod text_box;
mod text_button;
pub mod text_menu;
mod tooltip;
pub mod window;

pub use history::{Turn, Undoable};
pub use level::{LEVEL_CAP, Level};
pub use look::Look;
pub use scroll::horizontal_scroll;
pub use short_text_box::{PasswordBox, ShortTextBox};
pub use switch::{SwitchItem, Toggler, muted_colours, switch, switch_maybe, toggler};
pub use text_box::TextBox;
pub use text_button::{BUTTON_HEIGHT, Tertiary, TextButton, button_padding};
pub use text_menu::{TextMenu, TextMenuHistory, TextMenuWords, text_menu};
pub(crate) use tooltip::native_tooltip;
pub use tooltip::{
    TIP_WIDTH, Tip, link_tooltip, tip_bubble, tip_padding, tooltip_at, tooltip_unless, tooltip_with,
};

#[cfg(test)]
mod menu_fixture_tests;
#[cfg(test)]
mod menu_parts_tests;
#[cfg(test)]
mod panel_fixture_tests;
#[cfg(test)]
mod short_text_box_tests;
#[cfg(test)]
mod switch_tests;
#[cfg(test)]
mod text_box_tests;

/// The common menu and hover-card surface.
pub mod popup_surface;
