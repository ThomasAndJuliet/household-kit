// SPDX-License-Identifier: GPL-3.0-only

//! Resolved household menus (rules 17, 21c, 27–32).
//! Application actions, bindings and icon tables are resolved by the caller.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use cosmic::iced::{Font, Length};
use cosmic::widget::{self, icon, menu::key_bind::KeyBind};
use cosmic::{Element, theme};

use crate::spacing::Kind;

/// The minimum canvas for menu icons, marks and submenu indicators.
pub const ICON_SIZE: f32 = 16.0;
/// The room a menu row's width estimate leaves at each side of its content,
/// beyond the icon and mark columns. The row itself takes only its side
/// inset ([`Kind::MenuRow`]: `space_xxs`); the estimate adds one more of the
/// row's gaps (also `space_xxs`) as slack, so a row measured to fit is never
/// laid out against its edge. Together: `space_s` at Standard density, the
/// 16 px the estimate has always used
fn side_room() -> f32 {
    let spacing = theme::spacing();
    Kind::MenuRow.inset(&spacing).left + Kind::MenuRow.gap(&spacing)
}
/// The caption size of a row's detail line
const CAPTION: f32 = 12.0;
pub const MENU_MIN_WIDTH: u16 = 240;

/// A check box or one choice among radio options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuMark {
    Check(bool),
    Radio(bool),
}

/// An item whose application-specific metadata has already been resolved.
#[derive(Clone)]
pub struct MenuItem<M> {
    pub label: String,
    pub message: M,
    pub icon: Option<icon::Handle>,
    pub shortcut: Option<KeyBind>,
    pub mark: Option<MenuMark>,
    pub disabled_reason: Option<String>,
    pub destructive: bool,
    /// A second line under the label, in the caption style: what the row
    /// acts on, such as the folder an "Open in …" row opens. It is measured
    /// into the menu's width and height, wraps rather than clips, and the row's
    /// selected, hovered and disabled states cover it with the label.
    pub detail: Option<String>,
}

impl<M> MenuItem<M> {
    /// A plain row: no icon, shortcut, mark, detail or disabled reason.
    pub fn new(label: impl Into<String>, message: M) -> Self {
        Self {
            label: label.into(),
            message,
            icon: None,
            shortcut: None,
            mark: None,
            disabled_reason: None,
            destructive: false,
            detail: None,
        }
    }
    /// The same row with a caption line under its label.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// One row of a [`Menu`].
#[derive(Clone)]
pub enum Row<M> {
    Item(MenuItem<M>),
    Divider,
    /// A submenu: a row that opens another card beside this one, or inline.
    Folder(String, Menu<M>),
    /// "Show the rest": a disclosure of more rows of this same menu.
    ///
    /// The label is a Tertiary action (rule 7: accent text, underlined, bold,
    /// no wash) in the menu's own list. Activating it (click, Enter or Space)
    /// keeps the menu open, removes this row and shows the rows of the inner
    /// menu in its place, in the same group of the same card: no frame, no
    /// submenu. Keyboard focus lands on the first revealed row. Nothing is
    /// shown again once expanded in the same session; the state is the window
    /// chain's disclosure state ([`ChainRequest::expand`](crate::window::ChainRequest::expand)),
    /// gone when the menu closes, so the menu opens collapsed every time.
    ///
    /// The rows a More reveals belong to this menu, as if they had been
    /// written at its position: they take part in its marks, icon column,
    /// dividers and destructive-last order, and the menu is as wide as its
    /// widest revealed row from the start, so expanding changes the card's
    /// height and never its width (rule 8b). A More inside the revealed rows
    /// is a second disclosure.
    ///
    /// Built by [`Menu::more`]. For the expansion state to reach the menu, the
    /// surface needs [`MenuSurface::disclosures`](crate::menu_surface::MenuSurface::disclosures);
    /// without it the rows are simply shown.
    More(String, Menu<M>),
}

#[derive(Clone)]
pub struct Menu<M> {
    rows: Vec<Row<M>>,
}

/// The app supplies its bundled marks and live body typography for tooltips.
#[derive(Clone, Copy)]
pub struct MenuStyle<M> {
    pub icon: fn(&'static str) -> icon::Handle,
    pub body: fn(String) -> Element<'static, M>,
}

impl<M> Default for Menu<M> {
    fn default() -> Self {
        Self { rows: Vec::new() }
    }
}

impl<M> Menu<M> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn item(mut self, item: MenuItem<M>) -> Self {
        if let Some(reason) = &item.disabled_reason {
            assert!(!reason.trim().is_empty(), "a disabled menu item says why");
        }
        self.rows.push(Row::Item(item));
        self
    }
    pub fn divider(mut self) -> Self {
        self.rows.push(Row::Divider);
        self
    }
    pub fn folder(mut self, label: impl Into<String>, menu: Self) -> Self {
        self.rows.push(Row::Folder(label.into(), menu));
        self
    }
    /// A "show the rest" row with `label`, revealing `rest` in place of itself
    /// when activated; see [`Row::More`].
    ///
    /// ```ignore
    /// // the newest model of each family, and a tertiary row for the others
    /// Menu::new()
    ///     .item(MenuItem::new("Opus 5.5", Pick(Opus55)))
    ///     .item(MenuItem::new("Sonnet 5.5", Pick(Sonnet55)))
    ///     .more("Show older models", Menu::new()
    ///         .item(MenuItem::new("Opus 5.1", Pick(Opus51))))
    /// ```
    pub fn more(mut self, label: impl Into<String>, rest: Self) -> Self {
        self.rows.push(Row::More(label.into(), rest));
        self
    }
    /// The row as written, for the surface to rebuild a level.
    pub(crate) fn push(mut self, row: Row<M>) -> Self {
        self.rows.push(row);
        self
    }
    pub fn append(mut self, other: Self) -> Self {
        if !other.is_empty() {
            self.rows.push(Row::Divider);
            self.rows.extend(other.rows);
        }
        self
    }
    pub fn is_empty(&self) -> bool {
        !self.rows.iter().any(|r| !matches!(r, Row::Divider))
    }
    pub fn rows(&self) -> &[Row<M>] {
        &self.rows
    }
    pub fn map<N>(self, f: impl Fn(M) -> N) -> Menu<N> {
        fn go<M, N>(menu: Menu<M>, f: &impl Fn(M) -> N) -> Menu<N> {
            Menu {
                rows: menu
                    .rows
                    .into_iter()
                    .map(|row| match row {
                        Row::Item(i) => Row::Item(MenuItem {
                            label: i.label,
                            message: f(i.message),
                            icon: i.icon,
                            shortcut: i.shortcut,
                            mark: i.mark,
                            disabled_reason: i.disabled_reason,
                            destructive: i.destructive,
                            detail: i.detail,
                        }),
                        Row::Divider => Row::Divider,
                        Row::Folder(label, menu) => Row::Folder(label, go(menu, f)),
                        Row::More(label, menu) => Row::More(label, go(menu, f)),
                    })
                    .collect(),
            }
        }
        go(self, &f)
    }
    /// The rows in display order, useful for inspecting a resolved menu.
    pub fn outline(&self) -> Vec<String> {
        fn add<M>(rows: &[Row<M>], indent: &str, lines: &mut Vec<String>) {
            let marks = has_marks(rows);
            for at in ordered(rows) {
                match at.map(|i| &rows[i]) {
                    Some(Row::Item(item)) => {
                        let mut line = format!("{indent}{}", item.label);
                        if marks {
                            line = format!(
                                "{indent}[{}] {}",
                                if matches!(
                                    item.mark,
                                    Some(MenuMark::Check(true) | MenuMark::Radio(true))
                                ) {
                                    "x"
                                } else {
                                    " "
                                },
                                item.label
                            );
                        }
                        if let Some(key) = &item.shortcut {
                            line.push_str(&format!("\t{key}"));
                        }
                        if let Some(reason) = &item.disabled_reason {
                            line.push_str(&format!("\tgreyed: {reason}"));
                        }
                        if item.destructive && item.mark.is_none() {
                            line.push_str("\tdestructive");
                        }
                        lines.push(line);
                        if let Some(detail) = &item.detail {
                            lines.push(format!("{indent}    {detail}"));
                        }
                    }
                    Some(Row::Folder(label, menu)) => {
                        lines.push(format!("{indent}{label} ▸"));
                        add(&menu.rows, &format!("{indent}  "), lines);
                    }
                    Some(Row::More(label, menu)) => {
                        lines.push(format!("{indent}{label} …"));
                        add(&menu.rows, &format!("{indent}  "), lines);
                    }
                    Some(Row::Divider) | None => lines.push(format!("{indent}—")),
                }
            }
        }
        let mut lines = Vec::new();
        add(&self.rows, "", &mut lines);
        lines
    }
    /// The common width required by this menu and all its submenus.
    pub fn width(&self) -> u16 {
        width(&self.rows).ceil() as u16
    }
}

pub(crate) fn ordered<M>(rows: &[Row<M>]) -> Vec<Option<usize>> {
    let destructive = |r: &Row<M>| matches!(r, Row::Item(i) if i.destructive && i.mark.is_none());
    let rest = (0..rows.len()).filter(|&i| !destructive(&rows[i]));
    let last: Vec<_> = (0..rows.len()).filter(|&i| destructive(&rows[i])).collect();
    let tail = (!last.is_empty())
        .then_some(None)
        .into_iter()
        .chain(last.into_iter().map(Some));
    let mut result = Vec::new();
    for at in rest.map(Some).chain(tail) {
        if at.is_none_or(|i| matches!(rows[i], Row::Divider)) {
            if matches!(result.last(), Some(Some(_))) {
                result.push(None);
            }
        } else {
            result.push(at);
        }
    }
    if result.last() == Some(&None) {
        result.pop();
    }
    result
}

fn has_marks<M>(rows: &[Row<M>]) -> bool {
    rows.iter()
        .any(|r| matches!(r, Row::Item(i) if i.mark.is_some()))
}

/// The rows of a menu with every [`Row::More`] replaced, recursively, by the
/// rows it reveals, and the labels of those disclosure rows.
fn revealed<M>(rows: &[Row<M>]) -> (Vec<&Row<M>>, Vec<&str>) {
    let (mut shown, mut labels) = (Vec::new(), Vec::new());
    for row in rows {
        if let Row::More(label, rest) = row {
            let (inner, inner_labels) = revealed(&rest.rows);
            shown.extend(inner);
            labels.push(label.as_str());
            labels.extend(inner_labels);
        } else {
            shown.push(row);
        }
    }
    (shown, labels)
}

/// `rows` with every disclosure expanded: what a menu without disclosure
/// state shows.
fn flattened<M>(rows: Vec<Row<M>>) -> Vec<Row<M>> {
    rows.into_iter()
        .flat_map(|row| match row {
            Row::More(_, rest) => flattened(rest.rows),
            other => vec![other],
        })
        .collect()
}

/// Resolved visual data for one household menu row.
#[derive(Clone)]
pub struct MenuRow {
    pub reserve_mark: bool,
    pub mark: Option<icon::Handle>,
    pub icon: Option<icon::Handle>,
    pub label: String,
    pub shortcut: String,
    pub destructive: bool,
    pub selected: bool,
    pub folder: bool,
    /// The caption line under the label
    pub detail: Option<String>,
}

impl MenuRow {
    pub fn label_indent(reserve_mark: bool) -> f32 {
        let space = f32::from(theme::spacing().space_xxs);
        ICON_SIZE + space + if reserve_mark { ICON_SIZE + space } else { 0.0 }
    }

    pub fn width(&self) -> f32 {
        let font = if self.selected {
            cosmic::font::semibold()
        } else {
            cosmic::font::default()
        };
        let label = measured_width(&self.label, 14.0, font);
        let label = label.max(self.detail.as_deref().map_or(0.0, |detail| {
            measured_width(detail, CAPTION, cosmic::font::default())
        }));
        let trailing = if self.folder {
            ICON_SIZE
        } else {
            measured_width(&self.shortcut, 14.0, cosmic::font::default())
        };
        2.0 * side_room()
            + Self::label_indent(self.reserve_mark)
            + label
            + if self.folder || !self.shortcut.is_empty() {
                f32::from(theme::spacing().space_l) + trailing
            } else {
                0.0
            }
    }
}

/// The width a level of rows needs, with every disclosure counted as
/// expanded: the revealed rows are measured as members of this level (its mark
/// and icon columns reserve space for them), and the disclosure rows' own
/// labels too, so expanding one never widens the card.
fn width<M>(rows: &[Row<M>]) -> f32 {
    let (shown, labels) = revealed(rows);
    let marks = shown
        .iter()
        .any(|r| matches!(r, Row::Item(i) if i.mark.is_some()));
    let more = labels.into_iter().map(|label| {
        2.0 * side_room()
            + MenuRow::label_indent(marks)
            + measured_width(label, 14.0, cosmic::font::bold())
    });
    let rows = shown.into_iter().map(|r| match r {
        Row::Item(i) => MenuRow {
            reserve_mark: marks,
            mark: None,
            icon: None,
            label: i.label.clone(),
            shortcut: i
                .shortcut
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
            destructive: false,
            selected: matches!(i.mark, Some(MenuMark::Radio(true))) && i.disabled_reason.is_none(),
            folder: false,
            detail: i.detail.clone(),
        }
        .width(),
        Row::Folder(label, menu) => MenuRow {
            reserve_mark: marks,
            mark: None,
            icon: None,
            label: label.clone(),
            shortcut: String::new(),
            destructive: false,
            selected: false,
            folder: true,
            detail: None,
        }
        .width()
        .max(width(&menu.rows)),
        Row::Divider | Row::More(_, _) => 0.0,
    });
    rows.chain(more).fold(f32::from(MENU_MIN_WIDTH), f32::max)
}

type Widths = HashMap<(u32, Font, String, bool), f32>;
static WIDTHS: LazyLock<Mutex<Widths>> = LazyLock::new(Default::default);
/// Shapes text at the requested size and font, retaining at most 4096 widths.
pub fn measured_width(content: &str, size: f32, font: Font) -> f32 {
    measured_width_shaped(content, size, font, true)
}

/// [`measured_width`] shaped as `advanced` (full shaping) or basic text.
/// The shaping decides the width a little (under a pixel a label), so a
/// measurement that must equal a widget's layout shapes as the widget does
/// ([`crate::button::label_width`])
pub(crate) fn measured_width_shaped(content: &str, size: f32, font: Font, advanced: bool) -> f32 {
    use cosmic::iced::advanced::text::{self, Paragraph as _};
    if content.is_empty() {
        return 0.0;
    }
    let key = (size.to_bits(), font, content.to_owned(), advanced);
    if let Some(width) = WIDTHS.lock().unwrap().get(&key) {
        return *width;
    }
    let paragraph = <<cosmic::Renderer as text::Renderer>::Paragraph as text::Paragraph>::with_text(
        text::Text {
            content,
            bounds: cosmic::iced::Size::new(f32::INFINITY, f32::INFINITY),
            size: cosmic::iced::Pixels(size),
            line_height: text::LineHeight::default(),
            font,
            align_x: text::Alignment::Default,
            align_y: cosmic::iced::alignment::Vertical::Top,
            shaping: if advanced {
                text::Shaping::Advanced
            } else {
                text::Shaping::Basic
            },
            wrapping: text::Wrapping::None,
            ellipsize: text::Ellipsize::default(),
        },
    );
    let result = paragraph.min_bounds().width;
    let mut widths = WIDTHS.lock().unwrap();
    if widths.len() >= 4096 {
        widths.clear();
    }
    widths.insert(key, result);
    result
}

impl<M: Clone + 'static> Menu<M> {
    /// The menu as libcosmic's own menu rows. Only a text box's menu
    /// ([`crate::text_menu`]) still opens as libcosmic's popup, until the
    /// editors move (package 2); every other menu is a window's chain
    /// ([`crate::window::menu_surface`]).
    pub(crate) fn trees(self, style: MenuStyle<M>) -> Vec<widget::menu::Tree<M>> {
        // libcosmic's tree has no disclosure state: a text box's menu shows
        // every row
        let all = flattened(self.rows);
        let marks = has_marks(&all);
        let order = ordered(&all);
        let mut rows: Vec<_> = all.into_iter().map(Some).collect();
        order
            .into_iter()
            .map(|at| match at.and_then(|i| rows[i].take()) {
                Some(Row::Item(item)) => draw(item, marks, &style),
                Some(Row::Folder(label, menu)) => {
                    let children = menu.trees(MenuStyle {
                        icon: style.icon,
                        body: style.body,
                    });
                    let row = MenuRow {
                        reserve_mark: marks,
                        mark: None,
                        icon: None,
                        label,
                        shortcut: String::new(),
                        destructive: false,
                        selected: false,
                        folder: true,
                        detail: None,
                    }
                    .contents(style.icon);
                    let button = widget::menu::menu_button(row).class(crate::button::unringed(
                        if children.is_empty() {
                            theme::Button::MenuItem
                        } else {
                            theme::Button::MenuFolder
                        },
                    ));
                    widget::menu::Tree::with_children(
                        widget::RcElementWrapper::new(Element::from(button)),
                        children,
                    )
                }
                Some(Row::Divider | Row::More(_, _)) | None => {
                    widget::menu::Tree::from(Element::from(widget::divider::horizontal::light()))
                }
            })
            .collect()
    }
}

fn draw<M: Clone + 'static>(
    item: MenuItem<M>,
    marks: bool,
    style: &MenuStyle<M>,
) -> widget::menu::Tree<M> {
    widget::menu::Tree::from(item_element(item, marks, style))
}

fn item_element<M: Clone + 'static>(
    item: MenuItem<M>,
    marks: bool,
    style: &MenuStyle<M>,
) -> Element<'static, M> {
    let disabled = item.disabled_reason.is_some();
    let chosen = matches!(item.mark, Some(MenuMark::Radio(true))) && !disabled;
    let mark = match item.mark {
        Some(MenuMark::Check(true)) => Some("object-select-symbolic"),
        Some(MenuMark::Radio(true)) => Some("radio-chosen-symbolic"),
        Some(MenuMark::Radio(false)) => Some("radio-empty-symbolic"),
        _ => None,
    };
    let destructive = item.destructive && item.mark.is_none() && !disabled;
    let row = MenuRow {
        reserve_mark: marks,
        mark: mark.map(style.icon),
        icon: item.icon,
        label: item.label,
        shortcut: item
            .shortcut
            .as_ref()
            .map_or_else(String::new, ToString::to_string),
        destructive,
        selected: chosen,
        folder: false,
        detail: item.detail,
    }
    .contents(style.icon);
    let mut button = widget::menu::menu_button(row);
    if chosen {
        button = button.selected(true).class(selected_menu_item());
    }
    match item.disabled_reason {
        // A row of a text box's menu, libcosmic's popup outside any window
        // owner: the one menu whose greyed rows still say why in the native
        // tooltip, until editor menus move to the window (package 2)
        Some(reason) => crate::native_tooltip(
            button,
            (style.body)(reason),
            widget::tooltip::Position::Bottom,
            false,
        ),
        None => Element::from(button.on_press(item.message)),
    }
}

impl MenuRow {
    pub fn contents<M: 'static>(
        self,
        resolve: fn(&'static str) -> icon::Handle,
    ) -> Vec<Element<'static, M>> {
        let Self {
            reserve_mark: marks,
            mark,
            icon,
            label,
            shortcut: key,
            destructive,
            selected: chosen,
            folder,
            detail,
        } = self;
        let space = theme::spacing().space_xxs;
        let mut row = Vec::with_capacity(7);
        if marks {
            row.push(match mark {
                Some(handle) => widget::icon::icon(handle).size(ICON_SIZE as u16).into(),
                None => widget::space::horizontal()
                    .width(Length::Fixed(ICON_SIZE))
                    .into(),
            });
            row.push(widget::space::horizontal().width(space).into());
        }
        row.push(match icon {
            Some(handle) => {
                let mut icon = widget::icon::icon(handle).size(ICON_SIZE as u16);
                if destructive {
                    icon = icon.class(theme::Svg::Custom(std::rc::Rc::new(|theme| {
                        cosmic::iced::widget::svg::Style {
                            color: Some(theme.cosmic().destructive_text_color().into()),
                        }
                    })));
                }
                icon.into()
            }
            None => widget::space::horizontal()
                .width(Length::Fixed(ICON_SIZE))
                .into(),
        });
        row.push(widget::space::horizontal().width(space).into());
        let mut text = widget::text(label)
            .shaping(cosmic::iced::advanced::text::Shaping::Advanced)
            .ellipsize(cosmic::iced::advanced::text::Ellipsize::Middle(
                cosmic::iced::advanced::text::EllipsizeHeightLimit::Lines(1),
            ));
        if chosen {
            text = text.font(cosmic::font::semibold());
        }
        if destructive {
            text = text.class(theme::Text::Custom(|theme| {
                cosmic::iced::widget::text::Style {
                    color: Some(theme.cosmic().destructive_text_color().into()),
                    ..Default::default()
                }
            }));
        }
        row.push(match detail {
            Some(detail) => widget::column::with_children(vec![
                text.into(),
                widget::text::caption(detail)
                    .shaping(cosmic::iced::advanced::text::Shaping::Advanced)
                    .into(),
            ])
            .into(),
            None => text.into(),
        });
        row.push(widget::space::horizontal().into());
        if folder {
            row.push(
                widget::icon::icon(resolve("pan-end-symbolic"))
                    .size(ICON_SIZE as u16)
                    .into(),
            );
        } else {
            row.push(
                widget::text(key)
                    .shaping(cosmic::iced::advanced::text::Shaping::Advanced)
                    .class(theme::Text::Custom(|theme| {
                        let cosmic = theme.cosmic();
                        let colour = cosmic.on_bg_color();
                        let body = household_tones::text_tone(
                            household_tones::Srgb::new(colour.red, colour.green, colour.blue),
                            cosmic.is_dark,
                        );
                        cosmic::iced::widget::text::Style {
                            color: Some(cosmic::iced::Color {
                                r: body.r,
                                g: body.g,
                                b: body.b,
                                a: 0.68,
                            }),
                            ..Default::default()
                        }
                    }))
                    .into(),
            );
        }
        row
    }
}

fn selected_menu_item() -> theme::Button {
    use cosmic::widget::button::{Catalog, Style};
    fn chosen(mut style: Style, theme: &cosmic::Theme, pressed: bool) -> Style {
        let cosmic = theme.cosmic();
        let component = &cosmic.background(theme.transparent).component;
        let fill = if pressed {
            component.pressed
        } else {
            component.selected_state_color()
        };
        let text = cosmic::iced::Color::from(cosmic.accent_text_color());
        style.background = Some(cosmic::iced::Background::Color(fill.into()));
        style.text_color = Some(text);
        style.icon_color = Some(text);
        style
    }
    theme::Button::Custom {
        active: Box::new(|_, theme| {
            chosen(
                theme.active(false, true, &theme::Button::MenuItem),
                theme,
                false,
            )
        }),
        disabled: Box::new(|theme| theme.disabled(&theme::Button::MenuItem)),
        hovered: Box::new(|_, theme| {
            chosen(
                theme.hovered(false, true, &theme::Button::MenuItem),
                theme,
                false,
            )
        }),
        pressed: Box::new(|_, theme| {
            chosen(
                theme.pressed(false, true, &theme::Button::MenuItem),
                theme,
                true,
            )
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    fn item(label: &str) -> MenuItem<Rc<str>> {
        MenuItem {
            label: label.into(),
            message: Rc::from(label),
            icon: None,
            shortcut: None,
            mark: None,
            disabled_reason: None,
            destructive: false,
            detail: None,
        }
    }

    /// Pin (Standard density): a row's width is its label, its icon column
    /// and mark column (16 + 8 each) and 16 of padding at each side
    #[test]
    fn pin_standard_density_row_width() {
        let row = |reserve_mark| MenuRow {
            reserve_mark,
            mark: None,
            icon: None,
            label: "Copy".into(),
            shortcut: String::new(),
            destructive: false,
            selected: false,
            folder: false,
            detail: None,
        };
        let label = measured_width("Copy", 14.0, cosmic::font::default());
        assert_eq!(row(false).width(), 32.0 + 24.0 + label);
        assert_eq!(row(true).width(), 32.0 + 48.0 + label);
    }

    #[test]
    fn measurement_tracks_size_and_weight() {
        let text = "A menu label with a width";
        for font in [cosmic::font::default(), cosmic::font::semibold()] {
            let small = measured_width(text, 14.0, font);
            assert!(small > 0.0);
            assert!(measured_width(text, 20.0, font) > small);
            assert_eq!(measured_width(text, 14.0, font), small);
            assert_eq!(measured_width("", 20.0, font), 0.0);
        }
    }

    #[test]
    fn native_menu_row_reserves_sixteen_pixel_icon_canvases() {
        use cosmic::iced::advanced::{Widget, layout, renderer::Headless, widget::Tree};
        use cosmic::iced::{Font, Pixels, Size};
        fn icon(name: &'static str) -> icon::Handle {
            widget::icon::from_name(name).handle()
        }
        let renderer = cosmic::iced::futures::executor::block_on(
            <cosmic::Renderer as Headless>::new(Font::default(), Pixels(14.0), Some("tiny-skia")),
        )
        .expect("headless renderer");
        let row = MenuRow {
            reserve_mark: true,
            mark: Some(icon("object-select-symbolic")),
            icon: Some(icon("preferences-color-symbolic")),
            label: "Appearance".into(),
            shortcut: String::new(),
            destructive: false,
            selected: false,
            folder: true,
            detail: None,
        };
        let mut widget = widget::row::with_children(row.contents::<()>(icon));
        let mut tree = Tree::new(&widget as &dyn Widget<(), cosmic::Theme, cosmic::Renderer>);
        let node = widget.layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(400.0, 80.0)),
        );
        let children = node.children();
        for index in [0, 2, 6] {
            assert_eq!(children[index].size().width, 16.0, "icon canvas {index}");
            assert_eq!(children[index].size().height, 16.0, "icon canvas {index}");
        }
        assert_eq!(
            MenuRow::label_indent(true),
            32.0 + 2.0 * f32::from(theme::spacing().space_xxs)
        );
    }

    #[test]
    fn row_width_fits_the_laid_out_label_and_shortcut() {
        use cosmic::iced::advanced::renderer::Headless;
        use cosmic::iced::advanced::widget::{Id, Operation};
        use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
        use cosmic::iced::{Font, Pixels, Rectangle, Size};
        struct Texts(Vec<(String, Rectangle)>);
        impl Operation for Texts {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn text(&mut self, _: Option<&Id>, bounds: Rectangle, text: &str) {
                self.0.push((text.to_owned(), bounds));
            }
        }
        fn icon(name: &'static str) -> icon::Handle {
            widget::icon::from_name(name).handle()
        }
        for selected in [false, true] {
            let row = MenuRow {
                reserve_mark: true,
                mark: None,
                icon: None,
                label: "A long menu label".into(),
                shortcut: "Ctrl + Shift + C".into(),
                destructive: false,
                selected,
                folder: false,
                detail: None,
            };
            let width = row.width().ceil();
            let element: Element<'static, ()> =
                widget::container(widget::menu::menu_button(row.contents(icon)))
                    .width(Length::Fixed(width))
                    .into();
            let mut renderer =
                cosmic::iced::futures::executor::block_on(<cosmic::Renderer as Headless>::new(
                    Font::default(),
                    Pixels(14.0),
                    Some("tiny-skia"),
                ))
                .expect("a headless renderer");
            let mut ui = UserInterface::build(
                element,
                Size::new(width, 60.0),
                Cache::default(),
                &mut renderer,
            );
            let mut texts = Texts(Vec::new());
            ui.operate(&renderer, &mut texts);
            let label = texts
                .0
                .iter()
                .find(|(text, _)| text == "A long menu label")
                .unwrap()
                .1;
            let key = texts
                .0
                .iter()
                .find(|(text, _)| text == "Ctrl + Shift + C")
                .unwrap()
                .1;
            let font = if selected {
                cosmic::font::semibold()
            } else {
                cosmic::font::default()
            };
            assert!(
                label.width + 0.5 >= measured_width("A long menu label", 14.0, font),
                "selected={selected}, row_width={width}, label={label:?}, measured_label={}",
                measured_width("A long menu label", 14.0, font)
            );
            assert!(
                key.width + 0.5
                    >= measured_width("Ctrl + Shift + C", 14.0, cosmic::font::default()),
                "selected={selected}, row_width={width}, shortcut={key:?}, measured_shortcut={}",
                measured_width("Ctrl + Shift + C", 14.0, cosmic::font::default())
            );
            assert!(label.x + label.width <= key.x);
        }
    }

    #[test]
    fn separators_and_destructive_tail_are_normalized_at_each_level() {
        let mut stop = item("Stop");
        stop.destructive = true;
        let sub = Menu::new().item(stop.clone()).item(item("Find"));
        let menu = Menu::new()
            .divider()
            .item(item("Open"))
            .item(stop)
            .divider()
            .divider()
            .folder("More", sub)
            .divider();
        assert_eq!(
            menu.outline(),
            [
                "Open",
                "—",
                "More ▸",
                "  Find",
                "  —",
                "  Stop\tdestructive",
                "—",
                "Stop\tdestructive"
            ]
        );
        assert!(Menu::<Rc<str>>::new().divider().is_empty());
    }

    #[test]
    fn mapping_preserves_marks_disabled_reasons_and_rc_messages() {
        let mut option = item("View");
        option.mark = Some(MenuMark::Radio(true));
        option.disabled_reason = Some("Not now".into());
        let menu = Menu::new()
            .item(option)
            .append(Menu::new().item(item("Copy")));
        let mapped = menu.clone().map(|message| message.len());
        assert_eq!(
            mapped.outline(),
            ["[x] View\tgreyed: Not now", "—", "[ ] Copy"]
        );
        assert!(
            matches!(&mapped.rows()[0], Row::Item(item) if item.message == 4 && item.mark == Some(MenuMark::Radio(true)))
        );
        assert_eq!(menu.rows().len(), 3);
    }

    #[test]
    fn disabled_checked_row_draws_its_mark_and_does_not_activate() {
        use cosmic::iced::advanced::clipboard;
        use cosmic::iced::advanced::renderer::{Headless, Style};
        use cosmic::iced::runtime::user_interface::{Cache, UserInterface};
        use cosmic::iced::{Color, Event, Font, Pixels, Point, Size, mouse};
        fn icon(_: &'static str) -> icon::Handle {
            widget::icon::from_svg_bytes(&b"<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16'><rect x='2' y='2' width='12' height='12' fill='white'/></svg>"[..])
        }
        fn body(text: String) -> Element<'static, Rc<str>> {
            widget::text(text).into()
        }
        fn paint(checked: bool, disabled: bool) -> (Vec<u8>, Vec<Rc<str>>) {
            let mut item = item("Option");
            item.mark = Some(MenuMark::Check(checked));
            item.disabled_reason = disabled.then(|| "Not now".into());
            let element = item_element(item, true, &MenuStyle { icon, body });
            let mut renderer =
                cosmic::iced::futures::executor::block_on(<cosmic::Renderer as Headless>::new(
                    Font::default(),
                    Pixels(14.0),
                    Some("tiny-skia"),
                ))
                .expect("a headless renderer");
            let size = Size::new(240.0, 48.0);
            let mut ui = UserInterface::build(element, size, Cache::default(), &mut renderer);
            let cursor = mouse::Cursor::Available(Point::new(80.0, 16.0));
            let mut messages = Vec::new();
            for event in [
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ] {
                let _ = ui.update(
                    &[event],
                    cursor,
                    &mut renderer,
                    &mut clipboard::Null,
                    &mut messages,
                );
            }
            ui.draw(
                &mut renderer,
                &cosmic::Theme::default(),
                &Style {
                    text_color: Color::WHITE,
                    ..Style::default()
                },
                mouse::Cursor::Unavailable,
            );
            let pixels = renderer.screenshot(Size::new(240, 48), 1.0, Color::BLACK);
            (pixels, messages)
        }
        let (checked, disabled_messages) = paint(true, true);
        let (unchecked, _) = paint(false, true);
        assert_ne!(
            checked, unchecked,
            "a disabled checked row still draws its mark"
        );
        assert!(
            disabled_messages.is_empty(),
            "a disabled row cannot activate"
        );
        let (_, enabled_messages) = paint(true, false);
        assert_eq!(enabled_messages, [Rc::<str>::from("Option")]);
    }

    #[test]
    fn native_trees_accept_non_send_messages_and_disabled_rows() {
        fn icon(name: &'static str) -> icon::Handle {
            widget::icon::from_name(name).handle()
        }
        fn body(text: String) -> Element<'static, Rc<str>> {
            widget::text(text).into()
        }
        let mut disabled = item("Copy");
        disabled.disabled_reason = Some("Select text".into());
        let trees = Menu::new()
            .item(item("Open"))
            .item(disabled)
            .trees(MenuStyle { icon, body });
        assert_eq!(trees.len(), 2);
    }

    #[test]
    #[should_panic(expected = "says why")]
    fn disabled_items_require_a_reason() {
        let mut disabled = item("Copy");
        disabled.disabled_reason = Some(" ".into());
        let _ = Menu::new().item(disabled);
    }

    #[test]
    fn long_labels_and_submenus_determine_width() {
        let long = "A label long enough to need more than the narrowest menu";
        let menu = Menu::new().folder("More", Menu::new().item(item(long)));
        assert!(menu.width() > MENU_MIN_WIDTH);
        assert_eq!(Menu::new().item(item("Copy")).width(), MENU_MIN_WIDTH);
    }
}
