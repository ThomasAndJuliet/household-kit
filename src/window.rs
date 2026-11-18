// SPDX-License-Identifier: GPL-3.0-only

//! The per-window interaction owner: one menu chain and one informational
//! hover per actual window.
//!
//! An app keeps one [`WindowUi`] for each window it opens, and mounts the
//! window's whole view through [`WindowUi::mount`]. The mounted root is the
//! ordinary [`popup::Host`](crate::popup::Host); while any of its widget or
//! overlay methods runs, it makes the window current. Declarations in this
//! module ([`anchor`], [`hover_box`], [`trigger`], [`focus_regions`], …) take
//! no registry: they reach the window being traversed, so the same
//! declaration tree can be mounted in two windows. A declaration traversed
//! outside any window root is a construction error (a debug assertion; inert
//! in release).
//!
//! The root renders the window's single chain. The app builds its panels for
//! the open root `R` it is asked for ([`WindowUi::chain`]) and hands them to
//! `mount`; a trigger anywhere in the window only declares which root it
//! opens. Deep widgets never publish the window's actions themselves: they
//! queue them on the current window, and the root publishes them as
//! [`UiEvent::Internal`] for the app to give back to [`WindowUi::apply`].

use crate::popup::{Anchors, Panel};
use cosmic::iced::advanced::widget::{Id, Operation, Tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use cosmic::iced::{Event, Length, Rectangle, Size, Vector};
use cosmic::{Element, Renderer, Theme};
use std::any::Any;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fmt::Debug;
use std::rc::Rc;

/// How a chain was opened. A keyboard opening shows the first row as the
/// keyboard's target and lets its information follow, as a keyboard opening
/// of a focused trigger does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Entry {
    #[default]
    Pointer,
    Keyboard,
}

/// What a [`trigger`]'s content publishes, typically from its button's press,
/// to open its root's chain, or close it when it is the one open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Toggle;

/// A window's own action. Opaque: the app hands it back to
/// [`WindowUi::apply`] for the window it came from, and interprets nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowAction<R>(Internal<R>);

#[derive(Clone, Debug, PartialEq)]
enum Internal<R> {
    Toggle {
        root: R,
        entry: Entry,
    },
    Path {
        session: u64,
        path: Vec<usize>,
    },
    Expand {
        session: u64,
        row: usize,
        open: bool,
    },
    Close {
        session: u64,
        keep: usize,
    },
    Dismiss {
        session: u64,
    },
    Activated {
        session: u64,
    },
    NoFit {
        session: u64,
        depth: usize,
    },
}

/// What a mounted window publishes: its own actions, separate from the app's
/// messages, so the app's message type never contains its own envelope.
#[derive(Clone, Debug)]
pub enum UiEvent<R, M> {
    Internal(WindowAction<R>),
    Application(M),
}

fn is_application<R, M>(event: &UiEvent<R, M>) -> bool {
    matches!(event, UiEvent::Application(_))
}

struct Session<R> {
    root: R,
    session: u64,
    path: Vec<usize>,
    expanded: BTreeSet<usize>,
    entry: Entry,
}

/// One actual native window's interaction state: its registry of sources,
/// hover owner and timer, the surfaces of its chain and its queue, and the
/// one chain open in it. Create it where the window opens and drop it where
/// the window closes; never in a view.
pub struct WindowUi<R> {
    context: Context,
    chain: Option<Session<R>>,
    next_session: u64,
}

impl<R> Default for WindowUi<R> {
    fn default() -> Self {
        Self {
            context: Context::default(),
            chain: None,
            next_session: 1,
        }
    }
}

impl<R: Clone + PartialEq + Debug + 'static> WindowUi<R> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply an action this window published. An action of an earlier chain
    /// session is ignored, so a late action cannot close or steer a newer one.
    pub fn apply(&mut self, action: WindowAction<R>) {
        match action.0 {
            Internal::Toggle { root, entry } => {
                if self.chain.as_ref().is_some_and(|chain| chain.root == root) {
                    self.chain = None;
                } else {
                    self.open(root, entry);
                }
            }
            Internal::Path { session, path } => {
                if let Some(chain) = self.session(session) {
                    chain.path = path;
                }
            }
            Internal::Expand { session, row, open } => {
                if let Some(chain) = self.session(session) {
                    if open {
                        chain.expanded.insert(row);
                    } else {
                        chain.expanded.remove(&row);
                    }
                }
            }
            Internal::Close { session, keep } => {
                if let Some(chain) = self.session(session) {
                    // Keeping n panels keeps the path to the (n - 1)th submenu
                    if keep == 0 {
                        self.chain = None;
                    } else {
                        chain.path.truncate(keep - 1);
                    }
                }
            }
            // A stale source (NoFit) closes the session rather than showing
            // a chain detached from what opened it
            Internal::Dismiss { session }
            | Internal::Activated { session }
            | Internal::NoFit { session, .. } => {
                if self.session(session).is_some() {
                    self.chain = None;
                }
            }
        }
    }

    fn session(&mut self, session: u64) -> Option<&mut Session<R>> {
        self.chain.as_mut().filter(|chain| chain.session == session)
    }

    /// Open `root`'s chain, replacing whatever chain is open: an app's own
    /// shortcut for a menu, for example.
    pub fn open(&mut self, root: R, entry: Entry) {
        let session = self.next_session;
        self.next_session += 1;
        self.chain = Some(Session {
            root,
            session,
            path: Vec::new(),
            expanded: BTreeSet::new(),
            entry,
        });
    }

    /// Close the open chain, if any: when something more important arrives.
    pub fn close(&mut self) {
        self.chain = None;
    }

    /// The open chain, for the app to build its panels.
    pub fn chain(&self) -> Option<ChainRequest<'_, R>> {
        self.chain.as_ref().map(|chain| ChainRequest {
            root: &chain.root,
            session: chain.session,
            path: &chain.path,
            expanded: &chain.expanded,
            entry: chain.entry,
        })
    }

    /// The window's registry, for queries (what is visible, open or hovered).
    /// Declarations reach it through the window root, not through this.
    pub fn anchors(&self) -> &Anchors {
        &self.context.anchors
    }

    /// Mount the window's whole view. `provided` are the panels of the open
    /// chain, built for [`WindowUi::chain`] in the window's message type;
    /// empty when none is open. An open chain given no panels closes as
    /// having no fit.
    ///
    /// Every application message a chain row publishes is an activation: the
    /// window closes the chain before the message is routed. Session state,
    /// the open submenus and disclosures, goes through the request's own
    /// messages ([`ChainRequest::on_path`], [`ChainRequest::expand`]).
    pub fn mount<'a, M: Clone + 'a>(
        &'a self,
        content: impl Into<Element<'a, M>>,
        provided: Vec<Panel<'a, UiEvent<R, M>>>,
    ) -> Mount<'a, R, M> {
        let session = self.chain.as_ref().map(|chain| chain.session);
        debug_assert!(
            session.is_some() || provided.is_empty(),
            "panels provided for a window with no open chain"
        );
        let panels: Vec<_> = match session {
            Some(session) => provided
                .into_iter()
                .map(|panel| panel.in_session(session))
                .collect(),
            None => Vec::new(),
        };
        let event = |action: Internal<R>| UiEvent::Internal(WindowAction(action));
        let session = session.unwrap_or(0);
        let missing = (self.chain.is_some() && panels.is_empty())
            .then(|| event(Internal::NoFit { session, depth: 0 }));
        let binding = Binding {
            context: self.context.clone(),
            publish: Rc::new(
                |queued: Box<dyn Any>| match queued.downcast::<Internal<R>>() {
                    Ok(action) => Some(UiEvent::Internal(WindowAction(*action))),
                    Err(_) => {
                        debug_assert!(false, "a window action for another root type");
                        None
                    }
                },
            ),
            activation: Some((event(Internal::Activated { session }), is_application)),
            missing_chain: missing,
            keyboard_open: self
                .chain
                .as_ref()
                .is_some_and(|chain| chain.entry == Entry::Keyboard),
        };
        let host = crate::popup::host(content.into().map(UiEvent::Application))
            .anchors(self.context.anchors.clone())
            .chain(panels)
            .on_dismiss(event(Internal::Dismiss { session }))
            .on_close(move |keep| {
                UiEvent::Internal(WindowAction(if keep == 0 {
                    Internal::Dismiss { session }
                } else {
                    Internal::Close { session, keep }
                }))
            })
            .on_no_fit(move |depth| {
                UiEvent::Internal(WindowAction(Internal::NoFit { session, depth }))
            })
            .window(binding);
        Mount { host }
    }
}

/// The open chain, as the app's panel provider sees it.
pub struct ChainRequest<'w, R> {
    root: &'w R,
    session: u64,
    path: &'w [usize],
    expanded: &'w BTreeSet<usize>,
    entry: Entry,
}

impl<'w, R: 'static> ChainRequest<'w, R> {
    pub fn root(&self) -> &'w R {
        self.root
    }
    /// The open submenus' row keys, as [`MenuSurface`](crate::menu_surface::MenuSurface) uses them.
    pub fn path(&self) -> &'w [usize] {
        self.path
    }
    /// Whether the session's disclosure `row` is expanded.
    pub fn is_expanded(&self, row: usize) -> bool {
        self.expanded.contains(&row)
    }
    pub fn entry(&self) -> Entry {
        self.entry
    }
    /// The message a row sends to change the open submenus' path, for
    /// `MenuSurface::panels`.
    pub fn on_path<M>(&self) -> impl Fn(Vec<usize>) -> UiEvent<R, M> + Clone + 'static {
        let session = self.session;
        move |path| UiEvent::Internal(WindowAction(Internal::Path { session, path }))
    }
    /// The message that expands or collapses the disclosure `row`; it is
    /// session state, gone when the chain closes.
    pub fn expand<M>(&self, row: usize, open: bool) -> UiEvent<R, M> {
        UiEvent::Internal(WindowAction(Internal::Expand {
            session: self.session,
            row,
            open,
        }))
    }
}

/// A mounted window root; an [`Element`] of the window's [`UiEvent`]s.
pub struct Mount<'a, R, M> {
    host: crate::popup::Host<'a, UiEvent<R, M>>,
}

impl<'a, R: Clone + 'static, M: Clone + 'a> Mount<'a, R, M> {
    /// While true, the root handles no key: Escape, Alt+H and keyboard
    /// openings all go to the protected input, such as an embedded browser
    /// page that receives keys only when the tree leaves them. An open chain
    /// still owns the keyboard.
    pub fn protected_keys(mut self, protected: bool) -> Self {
        self.host = self.host.protected_keys(protected);
        self
    }
    /// Temporary adapter A1: see [`Host::open_sources`](crate::popup::Host::open_sources).
    pub fn open_sources(mut self, sources: impl IntoIterator<Item = Id>) -> Self {
        self.host = self.host.open_sources(sources);
        self
    }
}

impl<'a, R: Clone + 'static, M: Clone + 'a> From<Mount<'a, R, M>> for Element<'a, UiEvent<R, M>> {
    fn from(mount: Mount<'a, R, M>) -> Self {
        mount.host.into()
    }
}

// --- The current window -----------------------------------------------------

type Queue = Rc<RefCell<Vec<Box<dyn Any>>>>;
/// How a root turns a queued action into its message
type Publish<'a, Message> = Rc<dyn Fn(Box<dyn Any>) -> Option<Message> + 'a>;
/// What a root publishes before an activation, and which messages are ones
pub(crate) type Activation<Message> = (Message, fn(&Message) -> bool);

/// A window's registry and queue, current while its root is traversed.
#[derive(Clone, Default)]
pub(crate) struct Context {
    anchors: Anchors,
    queue: Queue,
}

thread_local! {
    static CURRENT: RefCell<Vec<Context>> = const { RefCell::new(Vec::new()) };
}

/// The window stays current until this is dropped.
pub(crate) struct Scope(());

impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.with(|stack| stack.borrow_mut().pop());
    }
}

impl Context {
    pub(crate) fn enter(&self) -> Scope {
        CURRENT.with(|stack| stack.borrow_mut().push(self.clone()));
        Scope(())
    }
}

fn current() -> Option<Context> {
    CURRENT.with(|stack| stack.borrow().last().cloned())
}

fn queue<R: 'static>(action: Internal<R>) {
    match current() {
        Some(context) => context.queue.borrow_mut().push(Box::new(action)),
        None => debug_assert!(false, "a window trigger outside any window root"),
    }
}

/// A deliberate keyboard navigation moved the keyboard's target to `id`:
/// its information follows after the usual delay. Lists and other widgets
/// with their own keyboard target call this when they move it; outside a
/// window root it does nothing.
pub fn keyboard_target(id: &Id) {
    if let Some(context) = current() {
        context.anchors.keyboard_target(id);
    }
}

/// Whose registry a declaration uses: the current window's. The host tests
/// also name one explicitly.
#[derive(Clone)]
pub(crate) enum Owner {
    #[cfg(test)]
    Anchors(Anchors),
    Window,
    /// The current window if there is one: legacy tooltip sites also appear
    /// in views no window owner mounts (a native popup's menu), where they
    /// show nothing.
    Legacy,
}

impl Owner {
    /// Resolve during a traversal method, never at construction.
    pub(crate) fn resolve(&self) -> Option<Anchors> {
        match self {
            #[cfg(test)]
            Self::Anchors(anchors) => Some(anchors.clone()),
            Self::Legacy => current().map(|context| context.anchors),
            Self::Window => {
                let context = current();
                debug_assert!(
                    context.is_some(),
                    "a window declaration traversed outside any window root"
                );
                context.map(|context| context.anchors)
            }
        }
    }
}

/// What a window root adds to its `Host`.
pub(crate) struct Binding<'a, Message> {
    context: Context,
    publish: Publish<'a, Message>,
    /// Published before the first activation a chain row publishes in one
    /// event, and which messages are activations
    pub(crate) activation: Option<Activation<Message>>,
    /// Published while the window has a chain open but no panels for it
    pub(crate) missing_chain: Option<Message>,
    /// The open chain was entered from the keyboard
    pub(crate) keyboard_open: bool,
}

impl<'a, Message: 'a> Binding<'a, Message> {
    pub(crate) fn enter(&self) -> Scope {
        self.context.enter()
    }
    /// Publish what deep widgets queued during this event, in order.
    pub(crate) fn flush(&self, shell: &mut Shell<'_, Message>) {
        flush(&self.context, self.publish.as_ref(), shell);
    }
    /// Keep the window current for every method of the root's overlay.
    pub(crate) fn scope_overlay<'b>(
        &self,
        overlay: overlay::Element<'b, Message, Theme, Renderer>,
    ) -> overlay::Element<'b, Message, Theme, Renderer>
    where
        'a: 'b,
    {
        overlay::Element::new(Box::new(Scoped {
            inner: overlay,
            context: self.context.clone(),
            publish: self.publish.clone(),
        }))
    }
}

fn flush<Message>(
    context: &Context,
    publish: &dyn Fn(Box<dyn Any>) -> Option<Message>,
    shell: &mut Shell<'_, Message>,
) {
    let queued: Vec<_> = context.queue.borrow_mut().drain(..).collect();
    for item in queued {
        if let Some(message) = publish(item) {
            shell.publish(message);
        }
    }
}

/// Carry everything but the messages of a nested shell to its parent.
pub(crate) fn forward_status<A, B>(from: &Shell<'_, A>, to: &mut Shell<'_, B>) {
    if from.is_event_captured() {
        to.capture_event();
    }
    to.request_redraw_at(from.redraw_request());
    if from.is_layout_invalid() {
        to.invalidate_layout();
    }
    if from.are_widgets_invalid() {
        to.invalidate_widgets();
    }
    to.request_input_method(from.input_method());
}

struct Scoped<'b, 'a, Message> {
    inner: overlay::Element<'b, Message, Theme, Renderer>,
    context: Context,
    publish: Publish<'a, Message>,
}

impl<'a, Message: 'a> overlay::Overlay<Message, Theme, Renderer> for Scoped<'_, 'a, Message> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let _scope = self.context.enter();
        self.inner.as_overlay_mut().layout(renderer, bounds)
    }
    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        let _scope = self.context.enter();
        self.inner
            .as_overlay()
            .draw(renderer, theme, style, layout, cursor);
    }
    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        let _scope = self.context.enter();
        self.inner
            .as_overlay_mut()
            .operate(layout, renderer, operation);
    }
    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let _scope = self.context.enter();
        self.inner
            .as_overlay_mut()
            .update(event, layout, cursor, renderer, clipboard, shell);
        flush(&self.context, self.publish.as_ref(), shell);
    }
    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let _scope = self.context.enter();
        self.inner
            .as_overlay()
            .mouse_interaction(layout, cursor, renderer)
    }
    fn overlay<'c>(
        &'c mut self,
        layout: Layout<'c>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'c, Message, Theme, Renderer>> {
        let _scope = self.context.enter();
        let nested = self.inner.as_overlay_mut().overlay(layout, renderer)?;
        Some(overlay::Element::new(Box::new(Scoped {
            inner: nested,
            context: self.context.clone(),
            publish: self.publish.clone(),
        })))
    }
    fn index(&self) -> f32 {
        self.inner.as_overlay().index()
    }
}

// --- Declarations -----------------------------------------------------------

/// Register `content` as a source of the window: a menu's anchor or the
/// target of information.
pub fn anchor<'a, M: 'a>(id: impl Into<Id>, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    crate::popup::source(Owner::Window, id.into(), content.into(), false, None)
}

/// [`anchor`], collecting the source's accepted layout report for
/// [`hover_box_with_report`]; see `Anchors::reported_anchor`.
pub fn reported_anchor<'a, M: 'a>(
    id: impl Into<Id>,
    source: crate::layout::SourceId,
    revision: u64,
    content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    crate::popup::source(
        Owner::Window,
        id.into(),
        content.into(),
        false,
        Some((source, revision)),
    )
}

/// A control that opens the window's chain for `root`, or closes it when it
/// is the open one; opening it replaces any other chain in the window. Its
/// content publishes [`Toggle`] where it would otherwise publish the
/// opening message. Give the content's focusable control the same `id`, so
/// that focus returns to it when the chain closes, and anchor the provided
/// root panel at it with `Panel::source(id)`.
pub fn trigger<'a, M: 'a, R: Clone + 'static>(
    id: impl Into<Id>,
    root: R,
    content: impl Into<Element<'a, Toggle>>,
) -> Element<'a, M> {
    let opener = Element::new(Opener {
        root,
        content: content.into(),
    });
    crate::popup::source(Owner::Window, id.into(), opener, true, None)
}

/// Information attached to `source`; see [`crate::hover_card::hover`].
pub fn hover<'a, M: 'a>(
    source: impl Into<Id>,
    target: impl Into<Element<'a, M>>,
    stages: Vec<crate::layout::Stage<'a, M>>,
    candidates: Vec<crate::layout::AnchorCandidate>,
) -> Element<'a, M> {
    crate::hover_card::hover_owned(
        Owner::Window,
        source.into(),
        target.into(),
        stages,
        candidates,
    )
}

/// One prepared box of information; see [`crate::hover_card::hover_box`].
pub fn hover_box<'a, M: 'a>(
    source: impl Into<Id>,
    target: impl Into<Element<'a, M>>,
    content: crate::layout::Child<'a, M>,
    candidates: Vec<crate::layout::AnchorCandidate>,
) -> Element<'a, M> {
    crate::hover_card::hover_box_owned(
        Owner::Window,
        source.into(),
        target.into(),
        content,
        candidates,
    )
}

/// Information prepared from the source's accepted report; see
/// [`crate::hover_card::hover_box_with_report`].
pub fn hover_box_with_report<'a, M: 'a>(
    source: impl Into<Id>,
    target: impl Into<Element<'a, M>>,
    prepare: impl Fn(Option<crate::layout::SourceReport>) -> crate::layout::Child<'a, M> + 'a,
    candidates: Vec<crate::layout::AnchorCandidate>,
) -> Element<'a, M> {
    crate::hover_card::hover_with_report_owned(
        Owner::Window,
        source.into(),
        target.into(),
        Box::new(prepare),
        candidates,
    )
}

/// Keyboard movement among an editor and groups of controls; see
/// [`crate::focus_regions`].
pub fn focus_regions<'a, M: Clone + 'a>(
    content: impl Into<Element<'a, M>>,
    editor: Id,
    groups: Vec<crate::focus_regions::Group>,
) -> Element<'a, M> {
    crate::focus_regions::regions_owned(content.into(), editor, groups, Owner::Window)
}

/// A menu's panels whose rows are the window's sources; see
/// [`MenuSurface`](crate::menu_surface::MenuSurface).
pub fn menu_surface<M: Clone + 'static>(
    menu: crate::menu::Menu<M>,
    style: crate::menu::MenuStyle<M>,
    namespace: String,
) -> crate::menu_surface::MenuSurface<M> {
    crate::menu_surface::MenuSurface::owned(menu, style, Owner::Window, namespace)
}

// --- Triggers ---------------------------------------------------------------

fn entry_of(event: &Event) -> Entry {
    if matches!(event, Event::Keyboard(_)) {
        Entry::Keyboard
    } else {
        Entry::Pointer
    }
}

struct Opener<'a, R> {
    root: R,
    content: Element<'a, Toggle>,
}

impl<'a, M, R: Clone + 'static> Widget<M, Theme, Renderer> for Opener<'a, R> {
    forward_widget!(M, Theme, Renderer; children, diff, size, layout, draw, operate, mouse_interaction, drag_destinations);
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
        let mut toggles = Vec::new();
        let mut local = Shell::new(&mut toggles);
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            &mut local,
            viewport,
        );
        forward_status(&local, shell);
        drop(local);
        if !toggles.is_empty() {
            queue(Internal::Toggle {
                root: self.root.clone(),
                entry: entry_of(event),
            });
        }
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        let inner = self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )?;
        Some(overlay::Element::new(Box::new(Intercepted {
            inner,
            root: self.root.clone(),
        })))
    }
    #[cfg(feature = "a11y")]
    fn a11y_nodes(
        &self,
        layout: Layout<'_>,
        tree: &Tree,
        cursor: mouse::Cursor,
    ) -> iced_accessibility::A11yTree {
        self.content
            .as_widget()
            .a11y_nodes(layout, &tree.children[0], cursor)
    }
}

/// A trigger content's overlay (its information, say), turning its toggles
/// into the window's.
struct Intercepted<'b, R> {
    inner: overlay::Element<'b, Toggle, Theme, Renderer>,
    root: R,
}

impl<M, R: Clone + 'static> overlay::Overlay<M, Theme, Renderer> for Intercepted<'_, R> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        self.inner.as_overlay_mut().layout(renderer, bounds)
    }
    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.inner
            .as_overlay()
            .draw(renderer, theme, style, layout, cursor);
    }
    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        self.inner
            .as_overlay_mut()
            .operate(layout, renderer, operation);
    }
    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
    ) {
        let mut toggles = Vec::new();
        let mut local = Shell::new(&mut toggles);
        self.inner
            .as_overlay_mut()
            .update(event, layout, cursor, renderer, clipboard, &mut local);
        forward_status(&local, shell);
        drop(local);
        if !toggles.is_empty() {
            queue(Internal::Toggle {
                root: self.root.clone(),
                entry: entry_of(event),
            });
        }
    }
    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.inner
            .as_overlay()
            .mouse_interaction(layout, cursor, renderer)
    }
    fn overlay<'c>(
        &'c mut self,
        layout: Layout<'c>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'c, M, Theme, Renderer>> {
        let nested = self.inner.as_overlay_mut().overlay(layout, renderer)?;
        Some(overlay::Element::new(Box::new(Intercepted {
            inner: nested,
            root: self.root.clone(),
        })))
    }
    fn index(&self) -> f32 {
        self.inner.as_overlay().index()
    }
}

#[cfg(test)]
#[path = "window_tests.rs"]
mod tests;
