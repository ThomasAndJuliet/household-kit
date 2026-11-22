// SPDX-License-Identifier: GPL-3.0-only

//! A window's menu bar (household rule 2): its top menus' titles where there
//! is room, and one three-bar menu in their place where there is not, never
//! both. The layout engine chooses between the two forms from the titles as
//! they are drawn, so no app states a width at which the bar folds.
//!
//! Each title, and the three-bar button, is a [`trigger`](crate::window::trigger)
//! of a root of the window's one chain; the app provides the menus' panels for
//! those roots as for any other ([`WindowUi::chain`](crate::window::WindowUi::chain)).
//! The bar keeps the conventions of a menu bar:
//!
//! - a press on a title opens its menu, and a press on the open menu's title
//!   closes it;
//! - while one of its menus is open, the pointer moving onto another title
//!   opens that one instead;
//! - Left and Right move to the neighbouring menus, wrapping at the ends
//!   (the app gives each menu's root panel [`MenuBar::lateral`]);
//! - Escape closes the innermost open menu, and focus returns to its title;
//! - the titles are ordinary focusable controls: Enter or Space opens a menu
//!   from the keyboard, with its first row the keyboard's target;
//! - F10 opens the first menu, its first row the keyboard's target, and
//!   closes it again; there are no Alt+letter mnemonics.

use crate::button::{Button, ButtonLook, State};
use crate::layout::{self as engine, Child, Level, Stage};
use crate::window::{Entry, Toggle, UiEvent};
use cosmic::iced::advanced::widget::{Id, Operation, Tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use cosmic::iced::{Event, Length, Rectangle, Size, Vector};
use cosmic::{Element, Renderer, Theme};

/// A window's menu bar: its menus' titles and the roots they open, in order.
pub struct MenuBar<R> {
    namespace: String,
    menus: Vec<(String, R)>,
    open: Option<R>,
    body: fn(String) -> Element<'static, Toggle>,
}

impl<R: Clone + PartialEq + 'static> MenuBar<R> {
    /// `menus` are the titles and the roots of the window's chain they
    /// open; `body` sets a title in the app's typography. Ids are made under
    /// `namespace`, which tells apart two bars of one window.
    pub fn new(
        namespace: impl Into<String>,
        menus: Vec<(String, R)>,
        body: fn(String) -> Element<'static, Toggle>,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            menus,
            open: None,
            body,
        }
    }

    /// The root open in the window now, if any: its title is drawn open,
    /// and the pointer reaching another title while one of this bar's menus
    /// is open opens that title's menu.
    pub fn open(mut self, open: Option<R>) -> Self {
        self.open = open;
        self
    }

    /// The id of the title of menu `index`: its menu's root panel anchors at it.
    pub fn title_id(namespace: &str, index: usize) -> Id {
        Id::new(format!("{namespace}:menu-bar:{index}"))
    }

    /// The previous and next menus of `root`, for its root panel's
    /// [`Panel::lateral`](crate::popup::Panel::lateral): Left and Right wrap
    /// at the ends of the bar. None for a root this bar does not open.
    pub fn neighbours(menus: &[R], root: &R) -> Option<(R, R)> {
        let at = menus.iter().position(|menu| menu == root)?;
        let count = menus.len();
        Some((
            menus[(at + count - 1) % count].clone(),
            menus[(at + 1) % count].clone(),
        ))
    }

    /// [`Self::neighbours`] as the messages a root panel's Left and Right
    /// publish, for the chain `request` asks for
    pub fn lateral<M>(
        menus: &[R],
        request: &crate::window::ChainRequest<'_, R>,
    ) -> Option<(UiEvent<R, M>, UiEvent<R, M>)> {
        let (previous, next) = Self::neighbours(menus, request.root())?;
        Some((
            request.open(previous, Entry::Keyboard),
            request.open(next, Entry::Keyboard),
        ))
    }

    /// The bar's titles in a row, the form where there is room
    pub fn titles<'a, M: 'a>(&self) -> Child<'a, M> {
        let roots: Vec<R> = self.menus.iter().map(|(_, root)| root.clone()).collect();
        let switching = self.open.as_ref().is_some_and(|open| roots.contains(open));
        let children = self
            .menus
            .iter()
            .enumerate()
            .map(|(index, (label, root))| {
                let id = Self::title_id(&self.namespace, index);
                let open = self.open.as_ref() == Some(root);
                let (label, root, body) = (label.clone(), root.clone(), self.body);
                engine::child(move |_| {
                    let title = title(
                        id.clone(),
                        root.clone(),
                        label.clone(),
                        open,
                        switching && !open,
                        body,
                    );
                    // F10 opens the first menu
                    if index == 0 {
                        crate::window::first_menu(root.clone(), title)
                    } else {
                        title
                    }
                })
            })
            .collect();
        engine::row_child(
            children,
            f32::from(cosmic::theme::active().cosmic().spacing.space_xxxs),
        )
    }

    /// The bar in the window: its titles where they fit, else `collapsed`,
    /// the app's three-bar button opening one menu of all of them. Only as
    /// wide as the form shown. The three-bar button is declared with
    /// [`first_menu`](crate::window::first_menu), as the first title is, so
    /// that F10 opens the menu the bar shows first.
    pub fn view<'a, M: 'a>(self, collapsed: Child<'a, M>) -> Element<'a, M> {
        let forms = engine::forms(vec![
            (Level::Preferred, self.titles()),
            (Level::Strong, collapsed),
        ]);
        engine::stages(vec![
            Stage::new(Level::Preferred, forms.clone()),
            Stage::new(Level::Strong, forms),
        ])
        .width(Length::Shrink)
        .into()
    }
}

/// One title: a trigger of its root, the shared button's Plain look (Juliet,
/// 22 Nov 2026) with the normal hover wash, kept while its menu is open.
/// `switch`: another of the bar's menus is open, and the pointer reaching
/// this title opens this one instead.
///
/// The label is in the colour of the header's controls (the accent, faded
/// when the window is behind others), as the libcosmic bar drew it
/// ([`Button::label_in_icon_colour`]).
///
/// Its corners are the theme's small radius, not the button's pill: a title
/// belongs to the menu family whose rows it opens, and their washes are
/// `radius_s` (approved 12 Nov). Juliet, 22 Nov 2026, choosing between the
/// pill and the small radius for the titles' hover: "keep the small radius".
/// The corner shape is part of a look's styling, so the title is still the
/// Plain look's box and states
pub(crate) fn title<'a, M: 'a, R: Clone + PartialEq + 'static>(
    id: Id,
    root: R,
    label: String,
    open: bool,
    switch: bool,
    body: fn(String) -> Element<'static, Toggle>,
) -> Element<'a, M> {
    let small = cosmic::theme::active().cosmic().corner_radii.radius_s;
    let button = Button::with_content(ButtonLook::Plain, body(label.clone()), label)
        .corners(small)
        .label_in_icon_colour(true)
        .id(id.clone())
        .shown_as(open.then_some(State::Hovered))
        .on_press(Toggle);
    Element::new(Title {
        root: root.clone(),
        switch,
        content: crate::window::trigger(id, root, Element::from(button)),
    })
}

/// A title: while another of its bar's menus is open, the pointer moving
/// onto it opens its own menu in that one's place. Its washes are the
/// button's
struct Title<'a, M, R> {
    root: R,
    /// Another of the bar's menus is open now
    switch: bool,
    content: Element<'a, M>,
}

impl<M, R: Clone + 'static> Widget<M, Theme, Renderer> for Title<'_, M, R> {
    forward_widget!(M, Theme, Renderer; children, diff, size, layout, draw, operate, mouse_interaction, overlay, drag_destinations, a11y_nodes);
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        if matches!(event, Event::Mouse(mouse::Event::CursorMoved { .. }))
            && self.switch
            && cursor.is_over(layout.bounds())
        {
            crate::window::queue_open(self.root.clone(), Entry::Pointer);
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }
}

#[cfg(test)]
#[path = "menu_bar_tests.rs"]
mod tests;
