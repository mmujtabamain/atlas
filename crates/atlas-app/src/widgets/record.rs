//! Record lists with fixed lanes: a header row, selectable rows, and the
//! `Open…` command that leads to the row's detail or inspector. Bounded
//! collections use this; unbounded ones use the virtualised [`super::grid`].

use gpui_kit::component::{ActiveTheme as _, h_flex, list::ListItem, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// A lane's geometry: a fixed width, or the flexible description lane.
#[derive(Clone, Copy, Debug)]
pub struct Lane {
    pub width: Option<f32>,
    pub right: bool,
}

impl Lane {
    pub const fn fixed(width: f32) -> Self {
        Lane { width: Some(width), right: false }
    }
    pub const fn money(width: f32) -> Self {
        Lane { width: Some(width), right: true }
    }
    pub const fn flex() -> Self {
        Lane { width: None, right: false }
    }
}

fn lane_cell(lane: Lane, content: AnyElement) -> Div {
    let cell = div().min_w_0().overflow_hidden().child(content);
    let cell = match lane.width {
        Some(w) => cell.w(px(w)).flex_shrink_0(),
        None => cell.flex_1(),
    };
    if lane.right { cell.flex().justify_end().text_right() } else { cell }
}

/// The header row: one label per lane.
pub fn header(columns: &[(&'static str, Lane)], cx: &App) -> AnyElement {
    let theme = cx.theme();
    h_flex()
        .w_full()
        .gap_4()
        .px_3()
        .py_1()
        .text_xs()
        .text_color(theme.muted_foreground)
        .children(columns.iter().map(|(label, lane)| lane_cell(*lane, div().child(*label).into_any_element())))
        .into_any_element()
}

/// A selectable row. `cells` must match the header's lanes.
pub fn row(id: impl Into<ElementId>, selected: bool, cells: Vec<(Lane, AnyElement)>, on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> ListItem {
    ListItem::new(id).selected(selected).on_click(on_click).child(h_flex().w_full().gap_4().py_1().items_center().children(cells.into_iter().map(|(lane, content)| lane_cell(lane, content))))
}

/// A list of rows under a header.
pub fn list(id: impl Into<ElementId>, header: AnyElement, rows: Vec<ListItem>) -> impl IntoElement {
    v_flex().id(id).w_full().gap_0p5().child(header).children(rows)
}

/// A plain text cell.
pub fn text(text: impl Into<SharedString>) -> AnyElement {
    div().text_sm().overflow_hidden().text_ellipsis().child(text.into()).into_any_element()
}

/// A muted secondary text cell.
pub fn muted(text: impl Into<SharedString>, cx: &App) -> AnyElement {
    div().text_xs().text_color(cx.theme().muted_foreground).overflow_hidden().text_ellipsis().child(text.into()).into_any_element()
}

/// A title over a muted subtitle.
pub fn stack(title: impl Into<SharedString>, subtitle: impl Into<SharedString>, cx: &App) -> AnyElement {
    v_flex()
        .min_w_0()
        .child(div().text_sm().overflow_hidden().text_ellipsis().child(title.into()))
        .child(div().text_xs().text_color(cx.theme().muted_foreground).overflow_hidden().text_ellipsis().child(subtitle.into()))
        .into_any_element()
}

/// A raw money value (not derived): mono, danger when negative.
pub fn money(money: atlas_core::Money, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .font_family(theme.mono_font_family.clone())
        .text_sm()
        .when(money.is_negative(), |d| d.text_color(theme.danger))
        .child(money.format())
        .into_any_element()
}
