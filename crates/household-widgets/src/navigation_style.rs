// SPDX-License-Identifier: GPL-3.0-only
//! Presentation for navigation between fixed sections. Content tabs and action
//! buttons have different selection and typography rules.
//!
//! # The keyboard target is drawn inside the entry
//!
//! A border is part of its box and takes no layout room (household padding
//! system, section 4a). The entry's keyboard-target outline is drawn on the
//! entry's own edge, inside it: a single 1 px stroke, or, on a solid fill
//! dark enough for that stroke to be indistinct, 1 px of white nearest the
//! fill and 1 px of the dark stroke outside it, both inside the edge. The
//! entry is the same size focused or not, and nothing is drawn outside it.

use cosmic::iced::{Background, Color, Vector};
use cosmic::{theme, widget::button};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prominence {
    Prominent,
    NonProminent,
}

#[derive(Clone, Copy)]
enum State {
    Rest,
    Hovered,
    Pressed,
    Disabled,
}

/// Native button class. Hover is supplied by the button, never inferred from
/// selection or cached separately. Labels should use `cosmic::font::semibold()`.
fn button_style(prominence: Prominence, current: bool, fallback_focus: bool) -> theme::Button {
    let style = move |state: State, focused: bool, theme: &theme::Theme| {
        let c = theme.cosmic();
        let hovered = matches!(state, State::Hovered | State::Pressed);
        let disabled = matches!(state, State::Disabled);
        let mut wash: Color = c.palette.neutral_5.into();
        wash.a = if hovered { 0.30 } else { 0.20 };
        let (mut fill, on) = crate::muted_colours(theme);
        if disabled {
            fill.a *= 0.5;
            wash.a *= 0.5;
        }
        let prominent = prominence == Prominence::Prominent && current;
        let ink: Color = if prominent {
            on
        } else if current {
            c.accent_text_color().into()
        } else {
            c.on_bg_color().into()
        };
        let mut ink = ink;
        if disabled {
            ink.a *= 0.5;
        }
        let mut result = button::Style {
            shadow_offset: Vector::default(),
            background: if prominent {
                Some(Background::Color(fill))
            } else if current || hovered {
                Some(Background::Color(wash))
            } else {
                None
            },
            overlay: if prominent && hovered {
                Some(Background::Color(wash))
            } else {
                None
            },
            border_radius: c.corner_radii.radius_s.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            outline_width: 0.0,
            outline_color: Color::TRANSPARENT,
            icon_color: Some(ink),
            text_color: Some(ink),
        };
        if focused && fallback_focus {
            let border = crate::menu_parts::RowContour::Rounded.target_border(theme);
            result.border_width = border.width;
            result.border_color = border.color;
        }
        result
    };
    theme::Button::Custom {
        active: Box::new(move |f, t| style(State::Rest, f, t)),
        hovered: Box::new(move |f, t| style(State::Hovered, f, t)),
        pressed: Box::new(move |f, t| style(State::Pressed, f, t)),
        disabled: Box::new(move |t| style(State::Disabled, false, t)),
    }
}

/// Standalone native style with a single neutral focus border. Prefer `NavigationButton`
/// for the full adaptive target contour.
pub fn button(prominence: Prominence, current: bool) -> theme::Button {
    button_style(prominence, current, true)
}

/// A fixed-section button. The builder keeps the native button and its target
/// decoration paired, including when the consumer configures activation.
pub struct NavigationButton<'a, M> {
    inner: button::Button<'a, M>,
    prominence: Prominence,
    current: bool,
    press_enabled: bool,
    press_down_enabled: bool,
}

impl<'a, M: Clone + 'a> NavigationButton<'a, M> {
    pub fn new(
        content: impl Into<cosmic::Element<'a, M>>,
        prominence: Prominence,
        current: bool,
    ) -> Self {
        Self {
            inner: button::custom(content).class(button_style(prominence, current, false)),
            prominence,
            current,
            press_enabled: false,
            press_down_enabled: false,
        }
    }
    pub fn id(mut self, id: cosmic::iced::advanced::widget::Id) -> Self {
        self.inner = self.inner.id(id);
        self
    }
    pub fn padding(mut self, padding: impl Into<cosmic::iced::Padding>) -> Self {
        self.inner = self.inner.padding(padding);
        self
    }
    pub fn on_press(mut self, message: M) -> Self {
        self.press_enabled = true;
        self.inner = self.inner.on_press(message);
        self
    }
    pub fn on_press_down(mut self, message: M) -> Self {
        self.press_down_enabled = true;
        self.inner = self.inner.on_press_down(message);
        self
    }
    pub fn on_press_maybe(mut self, message: Option<M>) -> Self {
        self.press_enabled = message.is_some();
        self.inner = self.inner.on_press_maybe(message);
        self
    }
    pub fn width(mut self, width: impl Into<cosmic::iced::Length>) -> Self {
        self.inner = self.inner.width(width);
        self
    }
}

impl<'a, M: Clone + 'a> From<NavigationButton<'a, M>> for cosmic::Element<'a, M> {
    fn from(button: NavigationButton<'a, M>) -> Self {
        cosmic::Element::new(Target {
            content: button.inner.into(),
            class: button_style(button.prominence, button.current, false),
            enabled: button.press_enabled || button.press_down_enabled,
        })
    }
}

use crate::keyboard_target::Target;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard_target::{TargetState, displayed_solid};
    use cosmic::iced::advanced::widget::{Operation, Tree, operation};
    use cosmic::iced::advanced::{Layout, layout, mouse, renderer};
    use cosmic::iced::{Length, Rectangle, Size};
    use cosmic::{Element, Theme};
    use cosmic::widget::button::Catalog;

    #[test]
    fn native_hover_replaces_the_current_wash_and_keeps_prominent_fill() {
        for theme in [Theme::light(), Theme::dark()] {
            let mut current: Color = theme.cosmic().palette.neutral_5.into();
            current.a = 0.20;
            let mut hover = current;
            hover.a = 0.30;
            let ordinary = button(Prominence::NonProminent, false);
            assert_eq!(theme.active(false, false, &ordinary).background, None);
            assert_eq!(
                theme.hovered(false, false, &ordinary).background,
                Some(Background::Color(hover))
            );
            let selected = button(Prominence::NonProminent, true);
            assert_eq!(
                theme.active(false, false, &selected).background,
                Some(Background::Color(current))
            );
            let hovered = theme.hovered(false, false, &selected);
            assert_eq!(hovered.background, Some(Background::Color(hover)));
            assert_eq!(hovered.overlay, None);
            let prominent = button(Prominence::Prominent, true);
            let rest = theme.active(false, false, &prominent);
            let hovered = theme.hovered(false, false, &prominent);
            assert_eq!(
                rest.background,
                Some(Background::Color(crate::muted_colours(&theme).0))
            );
            assert_eq!(hovered.background, rest.background);
            assert_eq!(hovered.overlay, Some(Background::Color(hover)));
            assert_eq!(hovered.text_color, rest.text_color);
        }
    }
    #[test]
    fn solid_fill_resolution_uses_overlay_and_ignores_translucent_surfaces() {
        let mut style = button::Style {
            background: Some(Background::Color(Color::from_rgb(0.1, 0.1, 0.1))),
            overlay: Some(Background::Color(Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.3,
            })),
            ..Default::default()
        };
        let shown = displayed_solid(&style).unwrap();
        assert!((shown.r - 0.37).abs() < 0.001);
        style.background = Some(Background::Color(Color {
            a: 0.2,
            ..Color::BLACK
        }));
        assert_eq!(displayed_solid(&style), None);
        for theme in [Theme::light(), Theme::dark()] {
            let class = button(Prominence::Prominent, true);
            let disabled = theme.disabled(&class);
            let Some(Background::Color(fill)) = disabled.background else {
                panic!("current fill")
            };
            assert_eq!(fill.a, crate::muted_colours(&theme).0.a * 0.5);
            assert!(displayed_solid(&disabled).is_none());
        }
    }

    #[test]
    fn optional_press_does_not_hide_a_press_down_action() {
        let button =
            NavigationButton::new(cosmic::widget::text("Section"), Prominence::Prominent, true)
                .on_press_down(())
                .on_press_maybe(None);
        assert!(!button.press_enabled && button.press_down_enabled);
        let button =
            NavigationButton::new(cosmic::widget::text("Section"), Prominence::Prominent, true)
                .on_press_maybe(None)
                .on_press_down(());
        assert!(!button.press_enabled && button.press_down_enabled);
    }

    /// The target is drawn on the entry's own edge, inside it: a focused
    /// entry is the same size and paints nothing outside its box, in the
    /// single and in the double outline
    #[test]
    fn the_keyboard_target_is_drawn_inside_the_entry() {
        use cosmic::iced::advanced::renderer::Headless;
        use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
        use cosmic::iced::Point;
        struct Focus;
        impl Operation for Focus {
            fn traverse(&mut self, f: &mut dyn FnMut(&mut dyn Operation)) {
                f(self);
            }
            fn focusable(
                &mut self,
                _: Option<&cosmic::iced::advanced::widget::Id>,
                _: Rectangle,
                state: &mut dyn operation::Focusable,
            ) {
                state.focus();
            }
        }
        for (prominence, current) in [
            (Prominence::NonProminent, false),
            (Prominence::NonProminent, true),
            (Prominence::Prominent, true),
        ] {
            let build = || -> Element<'static, ()> {
                cosmic::widget::container(
                    NavigationButton::new(cosmic::widget::text("Section"), prominence, current)
                        .width(Length::Fixed(80.0))
                        .on_press(()),
                )
                .center(Length::Fill)
                .into()
            };
            let draw = |focus: bool| {
                let mut renderer = crate::test_fonts::renderer().expect("headless renderer");
                let mut ui = UserInterface::build(
                    build(),
                    Size::new(120.0, 60.0),
                    Cache::default(),
                    &mut renderer,
                );
                if focus {
                    ui.operate(&renderer, &mut Focus);
                }
                ui.draw(
                    &mut renderer,
                    &Theme::dark(),
                    &renderer::Style::default(),
                    mouse::Cursor::Available(Point::new(1.0, 1.0)),
                );
                renderer
                    .screenshot(Size::new(120, 60), 1.0, Color::from_rgb(0.5, 0.5, 0.5))
                    .as_chunks::<4>()
                    .0
                    .to_vec()
            };
            let (rest, focused) = (draw(false), draw(true));
            assert_ne!(rest, focused, "{prominence:?} {current}: the target shows");
            // The entry's box, as the layout reports it
            let mut element = build();
            let mut tree = Tree::new(element.as_widget());
            let renderer = crate::test_fonts::renderer().expect("headless renderer");
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(120.0, 60.0)),
            );
            let entry = node.children()[0].bounds();
            assert_eq!(entry.width, 80.0, "no footprint added to the entry's width");
            let changed = (0..60 * 120)
                .filter(|n| rest[*n] != focused[*n])
                .map(|n| Point::new((n % 120) as f32 + 0.5, (n / 120) as f32 + 0.5))
                .collect::<Vec<_>>();
            assert!(!changed.is_empty());
            assert!(
                changed.iter().all(|p| entry.contains(*p)),
                "{prominence:?} {current}: every target pixel is inside the entry {entry:?}"
            );
            assert!(
                changed.iter().any(|p| p.x < entry.x + 1.0),
                "{prominence:?} {current}: it is on the entry's own edge"
            );
        }
    }

    #[test]
    fn native_focus_operation_updates_target_without_relayout() {
        let renderer = crate::test_fonts::renderer().expect("headless renderer");
        let mut element: Element<'_, ()> =
            NavigationButton::new(cosmic::widget::text("Section"), Prominence::Prominent, true)
                .on_press(())
                .into();
        let mut tree = Tree::new(element.as_widget());
        let limits = layout::Limits::new(Size::ZERO, Size::new(300.0, 100.0));
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        let before = node.size();
        assert!(!tree.state.downcast_ref::<TargetState>().focused);
        struct Focus;
        impl Operation for Focus {
            fn traverse(&mut self, f: &mut dyn FnMut(&mut dyn Operation)) {
                f(self);
            }
            fn focusable(
                &mut self,
                _: Option<&cosmic::iced::advanced::widget::Id>,
                _: Rectangle,
                state: &mut dyn operation::Focusable,
            ) {
                state.focus();
            }
        }
        element
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut Focus);
        assert!(tree.state.downcast_ref::<TargetState>().focused);
        let after = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .size();
        assert_eq!(before, after);
    }
}
