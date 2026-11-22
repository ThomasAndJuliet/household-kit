// SPDX-License-Identifier: GPL-3.0-only

//! The household's keyboard-target outline, in one place.
//!
//! Juliet's decisions (rule 6a): 12 Nov, "the thin dark-grey keyboard-target
//! outline ... 1px inside the row bounds"; 13 Nov, approved and generalised:
//! "wherever a component provides a keyboard-target outline, use the shared
//! on-fill contrast rule", and on a solid fill dark enough for the normal
//! stroke to be indistinct, "1px white nearest the fill, then 1px dark".
//! 21 Nov: a border is part of its box, drawn inside its edge and taking no
//! layout room, so both strokes are inside the edge (household padding
//! system, section 4a).
//!
//! [`draw_outline`] is the one drawing: the dark-grey stroke on the box's
//! own edge, and where `fill` is a solid fill the dark stroke would not
//! show on, a white stroke just inside it, nearest the fill. The fill is
//! resolved from what is actually displayed (the fill and any wash over it:
//! [`displayed_solid`]), never from the kind of control. [`Target`] wraps a
//! native button, finds when it has the keyboard's focus and draws this
//! over it: every button look, a segmented bar's parts and a navigation
//! entry are it. Nothing is drawn outside the button's box and the box is
//! the same size focused or not.

use cosmic::iced::advanced::widget::{Operation, Tree, operation, tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use cosmic::iced::{Background, Color, Event, Length, Rectangle, Size, Vector};
use cosmic::theme;
use cosmic::widget::button;
use cosmic::{Element, Renderer, Theme};

// Contrast is resolved from what the native class actually draws. A translucent
// fill depends on its containing surface, so it retains the shared neutral line.
pub(crate) fn displayed_solid(style: &button::Style) -> Option<Color> {
    let Some(Background::Color(fill)) = style.background else {
        return None;
    };
    if fill.a < 1.0 {
        return None;
    }
    Some(
        style
            .overlay
            .map_or(fill, |overlay| crate::kept::flat(overlay, fill)),
    )
}

pub(crate) struct Target<'a, M> {
    pub(crate) content: Element<'a, M>,
    /// The native class the wrapped button draws with: what its fill is
    /// shown as, and its corners
    pub(crate) class: theme::Button,
    /// Whether the button can be pressed; a disabled one draws its disabled fill
    pub(crate) enabled: bool,
}
#[derive(Default)]
pub(crate) struct TargetState {
    pub(crate) focused: bool,
    pub(crate) window_active: bool,
}
#[derive(Default)]
struct Probe(bool);
impl Operation for Probe {
    fn traverse(&mut self, f: &mut dyn FnMut(&mut dyn Operation)) {
        f(self);
    }
    fn focusable(
        &mut self,
        _: Option<&cosmic::iced::advanced::widget::Id>,
        _: Rectangle,
        focus: &mut dyn operation::Focusable,
    ) {
        self.0 |= focus.is_focused();
    }
}
impl<M> Target<'_, M> {
    fn probe(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer) {
        let mut probe = Probe::default();
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, &mut probe);
        tree.state.downcast_mut::<TargetState>().focused = probe.0;
    }
}
impl<M> Widget<M, Theme, Renderer> for Target<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<TargetState>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(TargetState {
            focused: false,
            window_active: true,
        })
    }
    forward_widget!(M, Theme, Renderer; children, diff, size, size_hint, layout, mouse_interaction, overlay, drag_destinations, a11y_nodes);
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        op: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, op);
        self.probe(tree, layout, renderer);
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
        let state = tree.state.downcast_mut::<TargetState>();
        match event {
            Event::Window(cosmic::iced::window::Event::Unfocused) => state.window_active = false,
            Event::Window(cosmic::iced::window::Event::Focused) => state.window_active = true,
            _ => {}
        }
        self.probe(tree, layout, renderer);
    }
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
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        let state = tree.state.downcast_ref::<TargetState>();
        if !state.focused || !state.window_active {
            return;
        }
        let bounds = layout.bounds();
        use cosmic::widget::button::Catalog;
        let appearance = if !self.enabled {
            theme.disabled(&self.class)
        } else if cursor.is_over(bounds) {
            theme.hovered(false, false, &self.class)
        } else {
            theme.active(false, false, &self.class)
        };
        draw_outline(
            renderer,
            bounds,
            appearance.border_radius,
            displayed_solid(&appearance),
            theme,
        );
    }
}

/// The keyboard-target outline on `bounds`, inside its edge: the household's
/// dark-grey stroke on the edge, and on a `fill` that is solid and dark
/// enough for that stroke to be indistinct a white stroke inside it, nearest
/// the fill. `radius` is the box's own corner radii
pub(crate) fn draw_outline(
    renderer: &mut Renderer,
    bounds: Rectangle,
    radius: cosmic::iced::border::Radius,
    fill: Option<Color>,
    theme: &Theme,
) {
    use cosmic::iced::advanced::Renderer as _;
    let dark = crate::menu_parts::RowContour::Rounded
        .target_border(theme)
        .color;
    let double = fill.is_some_and(|fill| {
        household_tones::lighter_on(household_tones::Srgb::new(fill.r, fill.g, fill.b))
    });
    // In a layer of its own: a native button draws its border in one, and
    // this must lie over it, not under
    renderer.with_layer(bounds, |renderer| {
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: cosmic::iced::Border {
                    color: dark,
                    width: 1.0,
                    radius,
                },
                ..Default::default()
            },
            Color::TRANSPARENT,
        );
        if double {
            let [tl, tr, br, bl]: [f32; 4] = radius.into();
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: bounds.x + 1.0,
                        y: bounds.y + 1.0,
                        width: (bounds.width - 2.0).max(0.0),
                        height: (bounds.height - 2.0).max(0.0),
                    },
                    border: cosmic::iced::Border {
                        color: Color::WHITE,
                        width: 1.0,
                        radius: [
                            (tl - 1.0).max(0.0),
                            (tr - 1.0).max(0.0),
                            (br - 1.0).max(0.0),
                            (bl - 1.0).max(0.0),
                        ]
                        .into(),
                    },
                    ..Default::default()
                },
                Color::TRANSPARENT,
            );
        }
    });
}
