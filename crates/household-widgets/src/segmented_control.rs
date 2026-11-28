// SPDX-License-Identifier: GPL-3.0-only

//! Native conjoined choices for forms.

use cosmic::iced::advanced::clipboard::DndDestinationRectangles;
use cosmic::iced::advanced::text::{Ellipsize, EllipsizeHeightLimit};
use cosmic::iced::advanced::widget::{Operation, Tree, tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use cosmic::iced::{Alignment, Border, Color, Event, Length, Point, Rectangle, Size, Vector};
use cosmic::widget::icon::{self, Icon};
use cosmic::widget::segmented_button::{
    Entity, HorizontalSegmentedButton, SingleSelect, SingleSelectModel, StyleSheet,
};
use cosmic::{Element, Renderer, Theme, theme, widget};

use crate::menu::measured_width;
use crate::{TIP_WIDTH, Tip, muted_colours, tooltip_with};

const HEIGHT: u16 = 32;
const TEXT_SIZE: f32 = 14.0;
const ICON_SIZE: u16 = 16;

/// One choice in a bar. A missing message disables activation.
pub struct Choice<M> {
    pub label: String,
    pub icon: Option<Icon>,
    pub message: Option<M>,
    pub tip: Option<String>,
}

/// A single-selection control. Supply the same `part_width` and `reserve_icon`
/// to align the segment edges and labels of neighbouring bars. With no width
/// supplied, the widest choice sets the preferred width of each segment.
pub fn view<'a, M: Clone + 'static>(
    choices: Vec<Choice<M>>,
    selected: Option<usize>,
    part_width: Option<f32>,
    reserve_icon: bool,
) -> Element<'a, M> {
    if choices.is_empty() {
        return widget::Space::new().width(0).height(HEIGHT).into();
    }

    let part_width = part_width.filter(|width| width.is_finite() && *width >= 0.0);
    let spacing = theme::spacing();
    let inset = spacing.space_s;
    let gap = spacing.space_xxs;
    let empty_icon = reserve_icon
        .then(|| icon::icon(icon::from_raster_pixels(1, 1, &[0, 0, 0, 0][..])).size(ICON_SIZE));
    let mut model = SingleSelectModel::default();
    let mut tips = Vec::with_capacity(choices.len());
    let mut icons = Vec::new();
    let mut widest = 0.0_f32;
    let mut can_activate = false;
    for (index, choice) in choices.into_iter().enumerate() {
        let icon = choice
            .icon
            .map(|icon| icon.size(ICON_SIZE))
            .or_else(|| empty_icon.clone());
        if part_width.is_none() {
            let label = measured_width(&choice.label, TEXT_SIZE, cosmic::font::semibold());
            let leading = if icon.is_some() {
                f32::from(ICON_SIZE + gap)
            } else {
                0.0
            };
            widest = widest.max(label + leading + 2.0 * f32::from(inset));
        }
        // Insertion order gives each index the same native entity on rebuild.
        let id = model.insert().text(choice.label).id();
        if let Some(icon) = icon {
            model.icon_set(id, icon.clone());
            icons.push((id, icon));
        }
        let enabled = choice.message.is_some();
        can_activate |= enabled;
        model.enable(id, enabled);
        if let Some(message) = choice.message {
            model.data_set(id, message);
        }
        if selected == Some(index) {
            model.activate(id);
        }
        tips.push(tip_target(choice.tip));
    }
    let part_width = part_width.unwrap_or_else(|| widest.ceil());

    Element::new(Control {
        model,
        icons,
        tips,
        part_width,
        inset,
        gap,
        can_activate,
        icons_fit: true,
    })
}

fn tip_target<M: 'static>(text: Option<String>) -> Element<'static, M> {
    let space = widget::Space::new()
        .width(Length::Fill)
        .height(Length::Fill);
    match text {
        Some(text) => tooltip_with(
            space,
            Tip::new(text.clone(), move || {
                widget::container(widget::text(text.clone()))
                    .max_width(TIP_WIDTH)
                    .into()
            }),
            widget::tooltip::Position::Bottom,
        ),
        None => space.into(),
    }
}

struct Control<M> {
    model: SingleSelectModel,
    icons: Vec<(Entity, Icon)>,
    tips: Vec<Element<'static, M>>,
    part_width: f32,
    inset: u16,
    gap: u16,
    can_activate: bool,
    icons_fit: bool,
}

impl<M: Clone + 'static> Control<M> {
    fn native(
        &self,
        tree: Option<&Tree>,
        width: f32,
    ) -> HorizontalSegmentedButton<'_, SingleSelect, Entity> {
        let inset = (width / (2.0 * self.model.len() as f32)) as u16;
        let inset = self.inset.min(inset);
        let only = self.model.len() == 1;
        let mut control = widget::segmented_control::horizontal(&self.model)
            .width(Length::Fill)
            .button_height(HEIGHT)
            .minimum_button_width(0)
            .button_alignment(Alignment::Start)
            .button_padding([inset, 0, inset, 0])
            .button_spacing(self.gap)
            .font_active(cosmic::font::semibold())
            .font_size(TEXT_SIZE)
            .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)))
            // Paint separators inside the allocated width. Native horizontal
            // dividers add a pixel to each stride after dividing the width.
            .dividers(false)
            .style(theme::SegmentedButton::Custom(Box::new(move |theme| {
                let mut look = theme.horizontal(&theme::SegmentedButton::Control);
                let (fill, on_fill) = muted_colours(theme);
                look.active.background = Some(fill.into());
                look.active.text_color = on_fill;
                look.hover.background = Some(Color::from(theme.cosmic().icon_button.hover).into());
                look.pressed.background =
                    Some(Color::from(theme.cosmic().icon_button.pressed).into());
                look.hover.text_color = look.inactive.text_color;
                look.pressed.text_color = look.inactive.text_color;
                look.border.width = 1.0;
                look.border.color = theme.cosmic().bg_divider().into();
                if only {
                    for status in [
                        &mut look.active,
                        &mut look.inactive,
                        &mut look.hover,
                        &mut look.pressed,
                    ] {
                        status.first.border.radius = look.border.radius;
                    }
                }
                look
            })));
        if self.can_activate {
            control = control.on_activate(|entity| entity);
        }
        if let Some(id) = tree.and_then(|tree| tree.id.clone()) {
            control.set_id(id);
        }
        control
    }

    fn fit_icons(&mut self, segment_width: f32) {
        let fits = segment_width >= f32::from(2 * self.inset + ICON_SIZE + self.gap);
        if self.icons_fit != fits {
            self.icons_fit = fits;
            for (entity, icon) in &self.icons {
                if fits {
                    self.model.icon_set(*entity, icon.clone());
                } else {
                    self.model.icon_remove(*entity);
                }
            }
        }
    }
}

impl<M: Clone + 'static> Widget<M, Theme, Renderer> for Control<M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Self>()
    }

    fn children(&self) -> Vec<Tree> {
        let native = self.native(None, f32::INFINITY);
        let mut tips = Tree::empty();
        tips.children = self
            .tips
            .iter()
            .map(|tip| Tree::new(tip.as_widget()))
            .collect();
        vec![
            Tree::new(&native as &dyn Widget<Entity, Theme, Renderer>),
            tips,
        ]
    }

    fn diff(&mut self, tree: &mut Tree) {
        let native_tree = &mut tree.children[0];
        self.native(Some(native_tree), f32::INFINITY)
            .diff(native_tree);
        tree.children[1].diff_children(&mut self.tips);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Fixed(f32::from(HEIGHT)))
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let count = self.model.len() as f32;
        let height = f32::from(HEIGHT);
        let preferred = Size::new((self.part_width * count).min(f32::MAX), height);
        let size = limits.resolve(Length::Shrink, self.size().height, preferred);
        self.fit_icons(size.width / count);

        let native_tree = &mut tree.children[0];
        let mut native = self.native(Some(native_tree), size.width);
        native.diff(native_tree);
        let control = native.layout(native_tree, renderer, &layout::Limits::new(size, size));
        let size = control.size();

        // The native geometry hook reports each visible segment, including
        // disabled ones, in model order. With a zero native minimum width all
        // choices stay visible as the bar narrows. This scratch list does
        // not register any drag targets with the window.
        let mut geometry = DndDestinationRectangles::new();
        native.drag_destinations(native_tree, Layout::new(&control), renderer, &mut geometry);
        let mut children = Vec::with_capacity(1 + self.tips.len());
        children.push(control);
        for destination in geometry.as_ref() {
            let bounds = &destination.rectangle;
            let size = Size::new(bounds.width as f32, bounds.height as f32);
            // The tooltip source is a filling Space. Its tree has no layout
            // state and shares the segment's exact rectangle.
            children.push(
                layout::Node::new(size).move_to(Point::new(bounds.x as f32, bounds.y as f32)),
            );
        }
        layout::Node::with_children(size, children)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let native_tree = &mut tree.children[0];
        self.native(Some(native_tree), layout.bounds().width)
            .operate(native_tree, layout.child(0), renderer, operation);
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
        let native_tree = &mut tree.children[0];
        let mut activated = Vec::new();
        let mut native_shell = Shell::new(&mut activated);
        self.native(Some(native_tree), layout.bounds().width)
            .update(
                native_tree,
                event,
                layout.child(0),
                cursor,
                renderer,
                clipboard,
                &mut native_shell,
                viewport,
            );
        crate::window::forward_status(&native_shell, shell);
        for entity in activated {
            // A native focus may outlive an availability change.
            if let Some(message) = self.model.data::<M>(entity) {
                shell.publish(message.clone());
            }
        }

        for ((tip, tip_tree), region) in self
            .tips
            .iter_mut()
            .zip(&mut tree.children[1].children)
            .zip(layout.children().skip(1))
        {
            tip.as_widget_mut().update(
                tip_tree, event, region, cursor, renderer, clipboard, shell, viewport,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let native_tree = &tree.children[0];
        self.native(Some(native_tree), layout.bounds().width)
            .mouse_interaction(native_tree, layout.child(0), cursor, viewport, renderer)
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
        use cosmic::iced::advanced::Renderer as _;

        let bounds = layout.bounds();
        let Some(clip) = bounds.intersection(viewport) else {
            return;
        };
        let native_tree = &tree.children[0];
        let native = self.native(Some(native_tree), bounds.width);
        renderer.with_layer(clip, |renderer| {
            native.draw(
                native_tree,
                renderer,
                theme,
                style,
                layout.child(0),
                cursor,
                viewport,
            );
            let divider: Color = theme.cosmic().small_widget_divider().into();
            let mut wash: Color = theme.current_container().component.base.into();
            wash.a = 0.6;
            let radius = theme.cosmic().corner_radii.radius_xl;
            let last = self.model.len() - 1;
            let mut prev_active = false;
            for (index, (entity, region)) in
                self.model.iter().zip(layout.children().skip(1)).enumerate()
            {
                let bounds = region.bounds();
                if !self.model.is_enabled(entity) {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds,
                            border: Border {
                                radius: [
                                    if index == 0 { radius[0] } else { 0.0 },
                                    if index == last { radius[1] } else { 0.0 },
                                    if index == last { radius[2] } else { 0.0 },
                                    if index == 0 { radius[3] } else { 0.0 },
                                ]
                                .into(),
                                ..Default::default()
                            },
                            shadow: Default::default(),
                            snap: true,
                        },
                        wash,
                    );
                }
                let active = self.model.is_active(entity);
                if index > 0 && !prev_active && !active {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle {
                                width: bounds.width.min(1.0),
                                y: bounds.y + bounds.height * 0.16,
                                height: bounds.height * 0.68,
                                ..bounds
                            },
                            border: Border::default(),
                            shadow: Default::default(),
                            snap: true,
                        },
                        divider,
                    );
                }
                prev_active = active;
            }
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        let tips: Vec<_> = self
            .tips
            .iter_mut()
            .zip(&mut tree.children[1].children)
            .zip(layout.children().skip(1))
            .filter_map(|((tip, tree), region)| {
                tip.as_widget_mut()
                    .overlay(tree, region, renderer, viewport, translation)
            })
            .collect();
        (!tips.is_empty()).then(|| overlay::Group::with_children(tips).overlay())
    }
}

#[cfg(test)]
#[path = "segmented_control_tests.rs"]
mod tests;
