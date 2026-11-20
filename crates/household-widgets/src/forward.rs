// Forward only the hooks named by each wrapper; interception stays in its impl.
//
// A wrapper's content has its own tree, which `children` and `diff` keep as
// the first child of the wrapper's tree. Every hook that passes that tree on
// takes it by checked access (`first`/`first_mut`), never by index: a tree
// without it was not built for this wrapper, and a user application must
// never panic over one. There the hook does nothing and answers with its
// neutral value: a layout of no size, no interaction, no overlay, an empty
// tree of accessibility nodes. Where the child is present, the content is
// called exactly as before. The layout engine's own wrappers do the same in
// their hand-written hooks (`src/layout.rs`), and its modules deny unchecked
// indexing, which reaches into these expansions.
macro_rules! forward_widget {
    ($message:ty, $theme:ty, $renderer:ty; $($hook:ident),* $(,)?) => {
        $(forward_widget!(@hook $message, $theme, $renderer; $hook);)*
    };
    (@hook $message:ty, $theme:ty, $renderer:ty; children) => {
        fn children(&self) -> Vec<Tree> {
                vec![Tree::new(&self.content)]
            }
    };
    (@hook $message:ty, $theme:ty, $renderer:ty; diff) => {
        fn diff(&mut self, tree: &mut Tree) {
                tree.diff_children(std::slice::from_mut(&mut self.content));
            }
    };
    (@hook $message:ty, $theme:ty, $renderer:ty; size) => {
        fn size(&self) -> Size<Length> {
                self.content.as_widget().size()
            }
    };
    (@hook $message:ty, $theme:ty, $renderer:ty; layout) => {
        fn layout(&mut self, tree: &mut Tree, renderer: &$renderer, limits: &layout::Limits) -> layout::Node {
                let Some(content) = tree.children.first_mut() else {
                    return layout::Node::new(cosmic::iced::Size::ZERO);
                };
                self.content.as_widget_mut().layout(content, renderer, limits)
            }
    };
    (@hook $message:ty, $theme:ty, $renderer:ty; operate) => {
        fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &$renderer, operation: &mut dyn Operation) {
                if let Some(content) = tree.children.first_mut() {
                    self.content.as_widget_mut().operate(content, layout, renderer, operation);
                }
            }
    };
    (@hook $message:ty, $theme:ty, $renderer:ty; size_hint) => {
        fn size_hint(&self) -> Size<Length> {
            self.content.as_widget().size_hint()
        }
    };
    (@hook $message:ty, $theme:ty, $renderer:ty; a11y_nodes) => {
        #[cfg(feature = "a11y")]
        fn a11y_nodes(
            &self,
            layout: Layout<'_>,
            tree: &Tree,
            cursor: mouse::Cursor,
        ) -> iced_accessibility::A11yTree {
            let Some(content) = tree.children.first() else {
                return iced_accessibility::A11yTree::default();
            };
            self.content.as_widget().a11y_nodes(layout, content, cursor)
        }
    };
    (@hook $message:ty, $theme:ty, $renderer:ty; update) => {
        fn update(
                &mut self,
                tree: &mut Tree,
                event: &Event,
                layout: Layout<'_>,
                cursor: mouse::Cursor,
                renderer: &$renderer,
                clipboard: &mut dyn Clipboard,
                shell: &mut Shell<'_, $message>,
                viewport: &Rectangle,
            ) {
                if let Some(content) = tree.children.first_mut() {
                    self.content.as_widget_mut().update(content, event, layout, cursor, renderer, clipboard, shell, viewport);
                }
            }
    };
    (@hook $message:ty, $theme:ty, $renderer:ty; draw) => {
        fn draw(
                &self,
                tree: &Tree,
                renderer: &mut $renderer,
                theme: &$theme,
                style: &renderer::Style,
                layout: Layout<'_>,
                cursor: mouse::Cursor,
                viewport: &Rectangle,
            ) {
                if let Some(content) = tree.children.first() {
                    self.content.as_widget().draw(content, renderer, theme, style, layout, cursor, viewport);
                }
            }
    };
    (@hook $message:ty, $theme:ty, $renderer:ty; mouse_interaction) => {
        fn mouse_interaction(&self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &$renderer) -> mouse::Interaction {
                let Some(content) = tree.children.first() else {
                    return mouse::Interaction::default();
                };
                self.content.as_widget().mouse_interaction(content, layout, cursor, viewport, renderer)
            }
    };
    (@hook $message:ty, $theme:ty, $renderer:ty; overlay) => {
        fn overlay<'b>(
                &'b mut self,
                tree: &'b mut Tree,
                layout: Layout<'b>,
                renderer: &$renderer,
                viewport: &Rectangle,
                translation: Vector,
            ) -> Option<overlay::Element<'b, $message, $theme, $renderer>> {
                let content = tree.children.first_mut()?;
                self.content.as_widget_mut().overlay(content, layout, renderer, viewport, translation)
            }
    };
    (@hook $message:ty, $theme:ty, $renderer:ty; drag_destinations) => {
        fn drag_destinations(&self, tree: &Tree, layout: Layout<'_>, renderer: &$renderer, dnd_rectangles: &mut cosmic::iced::advanced::clipboard::DndDestinationRectangles) {
                if let Some(content) = tree.children.first() {
                    self.content.as_widget().drag_destinations(content, layout, renderer, dnd_rectangles);
                }
            }
    };
}
