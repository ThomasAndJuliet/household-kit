// SPDX-License-Identifier: GPL-3.0-only

//! Declared roots: menus whose content the declaring widget holds (a row's
//! right-click menu, a ⋮ button's menu) rather than the app building it
//! from its state. The declaring widget is the root's source; while its
//! root is open it gives the window its declaration on every traversal, and
//! the app's panel provider builds the panels from what it last gave
//! ([`ChainRequest::declaration`](super::ChainRequest::declaration)).

use super::{Context, Entry, Internal, Toggle, current, queue};
use crate::popup::{FocusProbe, Ident};
use cosmic::iced::advanced::widget::{Id, Operation, Tree, tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use cosmic::iced::{Event, Length, Rectangle, Size, Vector, keyboard};
use cosmic::{Element, Renderer, Theme};
use std::any::Any;
use std::marker::PhantomData;
use std::rc::Rc;

/// Where a declared menu was asked for, relative to its source's top-left
/// corner, and how.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Invocation {
    /// The pointer's point, or the control focused when a key asked; None
    /// for the whole source (a button's menu)
    pub region: Option<Rectangle>,
    pub entry: Entry,
}

/// A declared root: the source that declares its menu, and where that menu
/// was asked for. An app's root type takes it in through `From`, so that
/// one of its roots is "the menu this source declares".
#[derive(Clone, Debug, PartialEq)]
pub struct Declared {
    pub(super) source: Id,
    pub(super) invocation: Invocation,
}

impl Declared {
    /// The source, whose identity is its target's: a row's, a card's
    pub fn source(&self) -> &Id {
        &self.source
    }
    pub fn invocation(&self) -> Invocation {
        self.invocation
    }
}

/// What a declaring widget gives the window: the app's own menu data, with
/// what identifies it.
pub trait Declaration: 'static {
    /// What the menu acts on. A source without a key of its own keeps its
    /// place in the widget tree; a different target in that place is
    /// another menu, so the one open closes rather than act on it.
    fn target(&self) -> u64;
    /// What it shows and does now: a change while it is open shows at once.
    fn revision(&self) -> u64;
}

/// Store the declaration of `source`, whose root is opening now
fn declare(context: &Context, source: &Id, declaration: Rc<dyn Any>, target: u64, revision: u64) {
    let mut slot = context.declared.borrow_mut();
    slot.open = Some(source.clone());
    slot.payload = Some(declaration);
    slot.target = Some(target);
    slot.revision = Some(revision);
    slot.served = None;
    slot.stale = false;
}

/// The right-click menu of `content`: a press of the right button on it
/// that nothing inside took, or the Menu key or Shift+F10 while a control
/// inside it has the focus, opens the window's chain for `declaration`. The
/// menu opens at the pointer, or at the focused control from the keyboard.
///
/// `source` is the menu's identity in the window, made from its target's
/// (a row's key); None keeps its place in the widget tree as its identity,
/// checked against the declaration's [`Declaration::target`].
pub fn context_menu<'a, M: 'a, R, T>(
    source: Option<Id>,
    declaration: T,
    content: impl Into<Element<'a, M>>,
) -> Element<'a, M>
where
    R: From<Declared> + Clone + 'static,
    T: Declaration,
{
    let ident = Ident::new(source.clone().unwrap_or_else(Id::unique));
    let inner = crate::popup::declared_source(ident.clone(), content.into(), false, true);
    Element::new(Declarer::<M, R, T> {
        ident,
        explicit: source,
        declaration: Rc::new(declaration),
        context: true,
        inner,
        root: PhantomData,
    })
}

/// A button whose press opens the window's chain for `declaration`, below
/// it, or closes it when it is the one open (a ⋮ or ▾ menu). Its content
/// publishes [`Toggle`] where it would publish its press. `source` as for
/// [`context_menu`].
pub fn menu_button<'a, M: 'a, R, T>(
    source: Option<Id>,
    declaration: T,
    content: impl Into<Element<'a, Toggle>>,
) -> Element<'a, M>
where
    R: From<Declared> + Clone + 'static,
    T: Declaration,
{
    let ident = Ident::new(source.clone().unwrap_or_else(Id::unique));
    let declaration = Rc::new(declaration);
    let opening = {
        let (ident, declaration) = (ident.clone(), declaration.clone());
        Rc::new(move |entry: Entry| {
            let source = ident.get();
            if let Some(context) = current() {
                let payload: Rc<dyn Any> = declaration.clone();
                declare(
                    &context,
                    &source,
                    payload,
                    declaration.target(),
                    declaration.revision(),
                );
            }
            let declared = Declared {
                source,
                invocation: Invocation {
                    region: None,
                    entry,
                },
            };
            queue(Internal::Toggle {
                root: R::from(declared.clone()),
                entry,
                declared: Some(declared),
            });
        }) as Rc<dyn Fn(Entry)>
    };
    let opener = Element::new(super::Opener {
        on_toggle: opening,
        content: content.into(),
    });
    let inner = crate::popup::declared_source(ident.clone(), opener, true, false);
    Element::new(Declarer::<M, R, T> {
        ident,
        explicit: source,
        declaration,
        context: false,
        inner,
        root: PhantomData,
    })
}

/// Declare `root` the window's first top menu, the one F10 opens, unless a
/// menu bar traversed before has: the first title of a
/// [`MenuBar`](crate::menu_bar::MenuBar) is, and so must be the three-bar
/// button that takes the titles' place.
pub fn first_menu<'a, M: 'a, R: Clone + 'static>(
    root: R,
    content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    Element::new(FirstMenu {
        opener: Rc::new(move || {
            Box::new(Internal::Toggle {
                root: root.clone(),
                entry: Entry::Keyboard,
                declared: None,
            }) as Box<dyn Any>
        }),
        content: content.into(),
    })
}

struct State {
    id: Id,
}

struct Declarer<'a, M, R, T> {
    ident: Ident,
    explicit: Option<Id>,
    declaration: Rc<T>,
    /// A right-click menu, rather than a button's
    context: bool,
    inner: Element<'a, M>,
    root: PhantomData<R>,
}

impl<M, R: From<Declared> + Clone + 'static, T: Declaration> Declarer<'_, M, R, T> {
    fn open(&self, invocation: Invocation) {
        let source = self.ident.get();
        if let Some(context) = current() {
            let payload: Rc<dyn Any> = self.declaration.clone();
            declare(
                &context,
                &source,
                payload,
                self.declaration.target(),
                self.declaration.revision(),
            );
        }
        let declared = Declared { source, invocation };
        queue(Internal::Open {
            root: R::from(declared.clone()),
            entry: invocation.entry,
            declared: Some(declared),
        });
    }
}

/// The Menu key, or Shift+F10
fn asks_for_menu(event: &Event) -> bool {
    use keyboard::key::Named;
    let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event else {
        return false;
    };
    match key {
        keyboard::Key::Named(Named::ContextMenu) => modifiers.is_empty(),
        keyboard::Key::Named(Named::F10) => *modifiers == keyboard::Modifiers::SHIFT,
        _ => false,
    }
}

impl<M, R: From<Declared> + Clone + 'static, T: Declaration> Widget<M, Theme, Renderer>
    for Declarer<'_, M, R, T>
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State {
            id: self.explicit.clone().unwrap_or_else(Id::unique),
        })
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.inner)]
    }
    fn diff(&mut self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<State>();
        if let Some(explicit) = &self.explicit {
            state.id = explicit.clone();
        }
        self.ident.set(state.id.clone());
        if let Some(context) = current() {
            let mut slot = context.declared.borrow_mut();
            if slot.open.as_ref() == Some(&state.id) {
                if self.explicit.is_none() && slot.target != Some(self.declaration.target()) {
                    // Another target in this place: the open menu's source
                    // is gone, and that menu closes
                    state.id = Id::unique();
                    self.ident.set(state.id.clone());
                } else {
                    let revision = self.declaration.revision();
                    let payload: Rc<dyn Any> = self.declaration.clone();
                    slot.payload = Some(payload);
                    slot.revision = Some(revision);
                    if slot.served.is_some_and(|served| served != revision) {
                        slot.stale = true;
                    }
                }
            }
        }
        tree.diff_children(std::slice::from_mut(&mut self.inner));
    }
    fn size(&self) -> Size<Length> {
        self.inner.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.inner
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
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
        self.inner.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.inner
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        self.inner.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        // What lies inside has first refusal: a row's own menu, a text
        // box's, an editor's keys
        if !self.context || shell.is_event_captured() {
            return;
        }
        let bounds = layout.bounds();
        let invocation = match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                cursor.position_over(bounds).map(|at| Invocation {
                    region: Some(Rectangle::new(
                        cosmic::iced::Point::new(at.x - bounds.x, at.y - bounds.y),
                        Size::new(1.0, 1.0),
                    )),
                    entry: Entry::Pointer,
                })
            }
            _ if asks_for_menu(event) => {
                let mut probe = FocusProbe::default();
                self.inner.as_widget_mut().operate(
                    &mut tree.children[0],
                    layout,
                    renderer,
                    &mut probe,
                );
                probe.focused_bounds().map(|focused| Invocation {
                    region: Some(Rectangle {
                        x: focused.x - bounds.x,
                        y: focused.y - bounds.y,
                        ..focused
                    }),
                    entry: Entry::Keyboard,
                })
            }
            _ => None,
        };
        if let Some(invocation) = invocation {
            self.open(invocation);
            shell.capture_event();
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
        self.inner.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        self.inner.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
    fn drag_destinations(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        dnd_rectangles: &mut cosmic::iced::advanced::clipboard::DndDestinationRectangles,
    ) {
        self.inner.as_widget().drag_destinations(
            &tree.children[0],
            layout,
            renderer,
            dnd_rectangles,
        );
    }
    #[cfg(feature = "a11y")]
    fn a11y_nodes(
        &self,
        layout: Layout<'_>,
        tree: &Tree,
        cursor: mouse::Cursor,
    ) -> iced_accessibility::A11yTree {
        self.inner
            .as_widget()
            .a11y_nodes(layout, &tree.children[0], cursor)
    }
}

struct FirstMenu<'a, M> {
    opener: Rc<dyn Fn() -> Box<dyn Any>>,
    content: Element<'a, M>,
}

impl<M> Widget<M, Theme, Renderer> for FirstMenu<'_, M> {
    forward_widget!(M, Theme, Renderer; children, diff, size, layout, draw, operate, update, mouse_interaction, drag_destinations, a11y_nodes);
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        // Only a bar the window shows declares it: a traversal reaches only
        // the form its layout chose
        if let Some(context) = current() {
            let mut first = context.first_menu.borrow_mut();
            if first.is_none() {
                *first = Some(self.opener.clone());
            }
        }
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}
