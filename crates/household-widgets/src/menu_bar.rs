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

use crate::layout::{self as engine, Child, Level, Stage};
use crate::menu_parts::RowContour;
use crate::spacing::control_height;
use crate::window::{Entry, Toggle, UiEvent};
use cosmic::iced::advanced::widget::{Id, Operation, Tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use cosmic::iced::{Event, Length, Rectangle, Size, Vector};
use cosmic::{Element, Renderer, Theme};

/// A title's padding: a title is a control, so none above and below (it is
/// [`control_height`] tall, its label centred) and `space_xs` at the sides.
/// The shared button's looks give `space_s` or `space_xxs` and none of them
/// draws a menu title (libcosmic's `MenuRoot` paint), so the title keeps the
/// `space_xs` it has always had
fn title_padding(spacing: &cosmic::cosmic_theme::Spacing) -> cosmic::iced::Padding {
    cosmic::iced::Padding::from([0.0, f32::from(spacing.space_xs)])
}

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

/// One title: a trigger of its root, with the normal hover wash, kept while
/// its menu is open. `switch`: another of the bar's menus is open, and the
/// pointer reaching this title opens this one instead
fn title<'a, M: 'a, R: Clone + PartialEq + 'static>(
    id: Id,
    root: R,
    label: String,
    open: bool,
    switch: bool,
    body: fn(String) -> Element<'static, Toggle>,
) -> Element<'a, M> {
    // libcosmic's menu-title button: its label in the colour of the
    // header's controls (the accent, faded when the window is behind
    // others), as the libcosmic bar drew it
    let spacing = cosmic::theme::spacing();
    let centred = cosmic::widget::container(body(label))
        .height(Length::Fill)
        .align_y(cosmic::iced::alignment::Vertical::Center);
    let button = cosmic::widget::button::custom(header_coloured(centred.into()))
        .padding(title_padding(&spacing))
        .height(Length::Fixed(control_height(&spacing)))
        .class(crate::button::unringed(cosmic::theme::Button::MenuRoot))
        .on_press(Toggle);
    let target = crate::popup::focus_target(id.clone(), button)
        .target_border(|theme| RowContour::Rounded.target_border(theme))
        .on_activate(Toggle);
    Element::new(Title {
        root: root.clone(),
        open,
        switch,
        content: crate::window::trigger(id, root, target),
    })
}

/// `content` with its text in the colour of the header's controls.
///
/// libcosmic's button draws a `MenuRoot` button's text in the *icon* colour
/// it is given (the header's), but only when the class is exactly
/// `MenuRoot`; the title wears that class through [`crate::button::unringed`],
/// a `Custom` class, so that no focus ring is drawn outside the box, and a
/// `Custom` class draws text in the body-text colour. This hands the
/// content the icon colour as its text colour, which is what the plain
/// class does, so the title keeps its colour with no outside ring
fn header_coloured<'a, M: 'a>(content: Element<'a, M>) -> Element<'a, M> {
    Element::new(HeaderColoured { content })
}

struct HeaderColoured<'a, M> {
    content: Element<'a, M>,
}

impl<M> Widget<M, Theme, Renderer> for HeaderColoured<'_, M> {
    forward_widget!(M, Theme, Renderer; children, diff, size, layout, operate, update, mouse_interaction, overlay, drag_destinations, a11y_nodes);
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let style = renderer::Style {
            text_color: style.icon_color,
            ..*style
        };
        self.content
            .as_widget()
            .draw(&tree.children[0], renderer, theme, &style, layout, cursor, viewport);
    }
}

/// A title: the normal hover wash under the pointer and while its menu is
/// open; while another of its bar's menus is open, the pointer moving onto
/// it opens its own menu in that one's place
struct Title<'a, M, R> {
    root: R,
    open: bool,
    /// Another of the bar's menus is open now
    switch: bool,
    content: Element<'a, M>,
}

impl<M, R: Clone + 'static> Widget<M, Theme, Renderer> for Title<'_, M, R> {
    forward_widget!(M, Theme, Renderer; children, diff, size, layout, operate, mouse_interaction, overlay, drag_destinations, a11y_nodes);
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use cosmic::iced::advanced::Renderer as _;
        if self.open || cursor.is_over(layout.bounds()) {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: layout.bounds(),
                    border: cosmic::iced::Border {
                        radius: RowContour::Rounded.radius(theme),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                cosmic::iced::Color::from(theme.cosmic().icon_button.hover),
            );
        }
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }
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
        if matches!(event, Event::Mouse(mouse::Event::CursorMoved { .. })) {
            if self.switch && cursor.is_over(layout.bounds()) {
                crate::window::queue_open(self.root.clone(), Entry::Pointer);
            }
            // The wash follows the pointer
            shell.request_redraw();
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
