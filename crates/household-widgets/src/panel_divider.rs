// SPDX-License-Identifier: GPL-3.0-only

//! Native draggable boundary shared by panels and panes.
use std::time::Instant;

use cosmic::iced::advanced::widget::{Tree, tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, renderer};
use cosmic::iced::{Event, Length, Point, Rectangle, Size, keyboard, window};
use cosmic::{Element, Renderer, Theme};

use crate::panel_layout::SplitAxis;
use crate::panel_resize::{Boundary, Drag, DragUpdate, Requests};

/// Width of the interaction footprint, in logical pixels.
pub const GRAB: f32 = 7.0;
/// Direction in which the adjacent panes are arranged.
pub type Axis = SplitAxis;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    Preview(DragUpdate),
    Commit(Requests),
    Cancel,
}

/// The supplied boundary is the accepted geometry, excluding this widget.
pub struct PanelDivider<'a, Message> {
    axis: Axis,
    requests: Requests,
    boundary: Boundary,
    on_change: Box<dyn Fn(Change) -> Message + 'a>,
}

impl<'a, Message> PanelDivider<'a, Message> {
    pub fn new(
        axis: Axis,
        requests: Requests,
        boundary: Boundary,
        on_change: impl Fn(Change) -> Message + 'a,
    ) -> Self {
        Self {
            axis,
            requests,
            boundary,
            on_change: Box::new(on_change),
        }
    }
}

struct Capture {
    drag: Drag,
    axis: Axis,
    original_requests: Requests,
    original_boundary: Boundary,
    preview: Option<DragUpdate>,
    accepted: (Requests, Boundary),
    pointer: f32,
    time: Instant,
}

#[derive(Default)]
struct State {
    capture: Option<Capture>,
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= crate::layout::FIT_EPSILON
}

fn same_boundary(a: Boundary, b: Boundary) -> bool {
    near(a.before, b.before)
        && near(a.after, b.after)
        && near(a.before_min, b.before_min)
        && near(a.after_min, b.after_min)
}

impl Capture {
    fn accepts(&self, axis: Axis, requests: Requests, boundary: Boundary) -> bool {
        if axis != self.axis
            || !near(boundary.before_min, self.original_boundary.before_min)
            || !near(boundary.after_min, self.original_boundary.after_min)
        {
            return false;
        }
        let original =
            requests == self.original_requests && same_boundary(boundary, self.original_boundary);
        let preview = self.preview.is_some_and(|p| {
            (requests == self.original_requests || requests == p.requests)
                && near(boundary.before, p.before)
                && near(boundary.after, p.after)
        });
        original
            || preview
            || (requests == self.accepted.0 && same_boundary(boundary, self.accepted.1))
    }
}

impl<Message> Widget<Message, Theme, Renderer> for PanelDivider<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn size(&self) -> Size<Length> {
        match self.axis {
            Axis::Horizontal => Size::new(Length::Fixed(GRAB), Length::Fill),
            Axis::Vertical => Size::new(Length::Fill, Length::Fixed(GRAB)),
        }
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        let size = self.size();
        layout::Node::new(limits.resolve(size.width, size.height, Size::ZERO))
    }
    fn draw(
        &self,
        _: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _: &renderer::Style,
        layout: Layout<'_>,
        _: mouse::Cursor,
        _: &Rectangle,
    ) {
        use cosmic::iced::advanced::Renderer as _;
        let bounds = layout.bounds();
        let colour: cosmic::iced::Color = theme.cosmic().bg_divider().into();
        for offset in [-1.5, 0.5] {
            let line = match self.axis {
                Axis::Horizontal => Rectangle {
                    x: (bounds.center_x() + offset).round(),
                    y: bounds.y,
                    width: 1.0,
                    height: bounds.height,
                },
                Axis::Vertical => Rectangle {
                    x: bounds.x,
                    y: (bounds.center_y() + offset).round(),
                    width: bounds.width,
                    height: 1.0,
                },
            };
            renderer.fill_quad(
                renderer::Quad {
                    bounds: line,
                    ..Default::default()
                },
                colour,
            );
        }
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &Renderer,
        _: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        if state
            .capture
            .as_ref()
            .is_some_and(|c| !c.accepts(self.axis, self.requests, self.boundary))
        {
            state.capture = None;
            shell.publish((self.on_change)(Change::Cancel));
        }
        if let Some(capture) = &mut state.capture {
            capture.accepted = (self.requests, self.boundary);
        }
        let coord = |p: Point| match self.axis {
            Axis::Horizontal => p.x,
            Axis::Vertical => p.y,
        };
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if state.capture.is_none() && cursor.is_over(layout.bounds()) =>
            {
                if let (Some(position), Ok(drag)) =
                    (cursor.position(), Drag::begin(self.requests, self.boundary))
                {
                    state.capture = Some(Capture {
                        drag,
                        axis: self.axis,
                        original_requests: self.requests,
                        original_boundary: self.boundary,
                        preview: None,
                        accepted: (self.requests, self.boundary),
                        pointer: coord(position),
                        time: Instant::now(),
                    });
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(capture) = &mut state.capture {
                    let now = Instant::now();
                    let next = coord(*position);
                    let update = capture.drag.step(
                        next - capture.pointer,
                        now.duration_since(capture.time).as_secs_f32(),
                    );
                    capture.pointer = next;
                    capture.time = now;
                    capture.preview = Some(update);
                    shell.publish((self.on_change)(Change::Preview(update)));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(capture) = state.capture.take() {
                    shell.publish((self.on_change)(Change::Commit(capture.drag.finish())));
                    shell.capture_event();
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            })
            | Event::Window(window::Event::Unfocused)
                if state.capture.take().is_some() =>
            {
                shell.publish((self.on_change)(Change::Cancel));
                shell.capture_event();
            }
            _ => {}
        }
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &Rectangle,
        _: &Renderer,
    ) -> mouse::Interaction {
        if tree.state.downcast_ref::<State>().capture.is_some() || cursor.is_over(layout.bounds()) {
            match self.axis {
                Axis::Horizontal => mouse::Interaction::ResizingHorizontally,
                Axis::Vertical => mouse::Interaction::ResizingVertically,
            }
        } else {
            mouse::Interaction::None
        }
    }
}

impl<'a, Message: 'a> From<PanelDivider<'a, Message>> for Element<'a, Message> {
    fn from(divider: PanelDivider<'a, Message>) -> Self {
        Element::new(divider)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel_resize::Dimension;

    fn boundary() -> Boundary {
        Boundary {
            before: 200.0,
            after: 200.0,
            before_min: 80.0,
            after_min: 100.0,
        }
    }
    fn renderer() -> Renderer {
        crate::test_fonts::renderer().unwrap()
    }
    fn dispatch(
        widget: &mut PanelDivider<'_, Change>,
        tree: &mut Tree,
        renderer: &Renderer,
        event: Event,
        cursor: mouse::Cursor,
    ) -> Vec<Change> {
        let size = Size::new(7.0, 300.0);
        let node = widget.layout(tree, renderer, &layout::Limits::new(Size::ZERO, size));
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        widget.update(
            tree,
            &event,
            Layout::new(&node),
            cursor,
            renderer,
            &mut cosmic::iced::advanced::clipboard::Null,
            &mut shell,
            &Rectangle::with_size(size),
        );
        messages
    }
    fn press(widget: &mut PanelDivider<'_, Change>, tree: &mut Tree, renderer: &Renderer) {
        assert!(
            dispatch(
                widget,
                tree,
                renderer,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                mouse::Cursor::Available(Point::new(3.0, 50.0))
            )
            .is_empty()
        );
    }
    fn motion(
        widget: &mut PanelDivider<'_, Change>,
        tree: &mut Tree,
        renderer: &Renderer,
        x: f32,
    ) -> Change {
        dispatch(
            widget,
            tree,
            renderer,
            Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(x, 50.0),
            }),
            mouse::Cursor::Unavailable,
        )[0]
    }
    #[test]
    fn captured_motion_outside_resists_and_commits() {
        let renderer = renderer();
        let mut widget =
            PanelDivider::new(Axis::Horizontal, Requests::default(), boundary(), |x| x);
        let mut tree = Tree::new(&widget as &dyn Widget<Change, Theme, Renderer>);
        press(&mut widget, &mut tree, &renderer);
        let Change::Preview(first) = motion(&mut widget, &mut tree, &renderer, 23.0) else {
            panic!()
        };
        assert!(!first.manual && first.before > 200.0 && first.before < 212.0);
        // Rebuilding with accepted preview geometry must retain capture.
        widget = PanelDivider::new(
            Axis::Horizontal,
            Requests::default(),
            Boundary {
                before: first.before,
                after: first.after,
                ..boundary()
            },
            |x| x,
        );
        let Change::Preview(last) = motion(&mut widget, &mut tree, &renderer, 500.0) else {
            panic!()
        };
        assert_eq!(last.after, 100.0);
        let changes = dispatch(
            &mut widget,
            &mut tree,
            &renderer,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(changes, vec![Change::Commit(last.requests)]);
        assert_eq!(last.requests.after, Dimension::Manual(100.0));
    }
    #[test]
    fn accepted_preview_rounding_survives_release() {
        let renderer = renderer();
        let mut widget =
            PanelDivider::new(Axis::Horizontal, Requests::default(), boundary(), |x| x);
        let mut tree = Tree::new(&widget as &dyn Widget<Change, Theme, Renderer>);
        press(&mut widget, &mut tree, &renderer);
        let Change::Preview(preview) = motion(&mut widget, &mut tree, &renderer, 23.0) else {
            panic!()
        };
        let drift = crate::layout::FIT_EPSILON / 2.0;
        widget = PanelDivider::new(
            Axis::Horizontal,
            Requests::default(),
            Boundary {
                before: preview.before + drift,
                after: preview.after - drift,
                before_min: boundary().before_min + drift,
                after_min: boundary().after_min - drift,
            },
            |x| x,
        );
        let changes = dispatch(
            &mut widget,
            &mut tree,
            &renderer,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(changes, vec![Change::Commit(preview.requests)]);
    }

    #[test]
    fn external_geometry_and_focus_loss_cancel() {
        let renderer = renderer();
        let mut widget =
            PanelDivider::new(Axis::Horizontal, Requests::default(), boundary(), |x| x);
        let mut tree = Tree::new(&widget as &dyn Widget<Change, Theme, Renderer>);
        press(&mut widget, &mut tree, &renderer);
        let changes = dispatch(
            &mut widget,
            &mut tree,
            &renderer,
            Event::Window(window::Event::Unfocused),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(changes, vec![Change::Cancel]);
        press(&mut widget, &mut tree, &renderer);
        widget = PanelDivider::new(
            Axis::Horizontal,
            Requests::default(),
            Boundary {
                before: 190.0,
                after: 210.0,
                ..boundary()
            },
            |x| x,
        );
        let changes = dispatch(
            &mut widget,
            &mut tree,
            &renderer,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(changes, vec![Change::Cancel]);
    }
}
