//! A virtualised table for the rows that grow with the horizon.
//!
//! The plain `Table` builds one element tree per row, every frame: a year of
//! occurrences is a thousand rows and ~25 taffy nodes each, and the whole
//! screen is laid out again on every scroll tick. gpui-kit's `DataTable` is
//! delegate-driven and virtualised (`uniform_list` inside): only the rows in
//! view exist as elements, it owns its own scroll region and scrollbar, and
//! its columns are resizable.
//!
//! Screens do not implement the delegate themselves. A screen model prepares
//! its rows once, at compute time, as [`Row`]s of [`Cell`]s — strings and
//! enums that are ready to paint, no lookups or formatting left — and shares
//! them through an `Arc`. The [`TableState`] entity behind each grid lives in
//! `AtlasApp` (it needs a window to be created) and is brought up to date at
//! render time by [`sync`], which compares the `Arc` pointer: a recomputed
//! model is a new allocation, an untouched one is the same rows and costs
//! nothing.

use std::sync::Arc;

use atlas_core::timeline::OccurrenceStatus;
use atlas_core::vocab::Certainty;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, Size,
    table::{Column, DataTable, TableDelegate, TableState},
    h_flex,
    tag::Tag,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::labels;

/// Row height: two lines of text (sm over xs) plus the medium cell padding.
pub const ROW_HEIGHT: Pixels = px(50.);

/// How a column lays out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
}

/// One column of a grid; fixed per grid, so the delegate can hand gpui-kit
/// its [`Column`]s without touching the rows.
#[derive(Clone, Debug)]
pub struct GridColumn {
    pub key: &'static str,
    pub name: &'static str,
    pub width: f32,
    pub align: Align,
}

impl GridColumn {
    pub const fn new(key: &'static str, name: &'static str, width: f32) -> Self {
        GridColumn { key, name, width, align: Align::Left }
    }

    pub const fn right(mut self) -> Self {
        self.align = Align::Right;
        self
    }
}

/// The colour a money cell paints in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Foreground,
    Muted,
    Success,
    Danger,
}

/// One cell, ready to paint.
#[derive(Clone, Debug)]
pub enum Cell {
    Text(SharedString),
    Muted(SharedString),
    /// A label over a muted subtitle.
    Stack { title: SharedString, subtitle: SharedString },
    /// A right-aligned figure in the monospace face, with an optional muted
    /// detail line beneath it.
    Money { text: SharedString, tone: Tone, line_through: bool, detail: Option<SharedString> },
    Certainty(Certainty),
    Status(OccurrenceStatus),
    /// A small outlined tag.
    Chip(SharedString),
}

impl Cell {
    pub fn text(text: impl Into<SharedString>) -> Self {
        Cell::Text(text.into())
    }

    pub fn muted(text: impl Into<SharedString>) -> Self {
        Cell::Muted(text.into())
    }

    pub fn stack(title: impl Into<SharedString>, subtitle: impl Into<SharedString>) -> Self {
        Cell::Stack { title: title.into(), subtitle: subtitle.into() }
    }

    /// A plain money figure: danger when negative, foreground otherwise.
    pub fn money(money: atlas_core::Money) -> Self {
        Cell::Money { text: money.format().into(), tone: if money.is_negative() { Tone::Danger } else { Tone::Foreground }, line_through: false, detail: None }
    }

    /// The cell's text, for exports and tests.
    pub fn as_text(&self) -> String {
        match self {
            Cell::Text(text) | Cell::Muted(text) | Cell::Chip(text) => text.to_string(),
            Cell::Stack { title, subtitle } => format!("{title} · {subtitle}"),
            Cell::Money { text, detail: Some(detail), .. } => format!("{text} ({detail})"),
            Cell::Money { text, .. } => text.to_string(),
            Cell::Certainty(certainty) => certainty.label().to_string(),
            Cell::Status(status) => status.label().to_string(),
        }
    }

    fn render(&self, align: Align, cx: &App) -> AnyElement {
        let theme = cx.theme();
        match self {
            Cell::Text(text) => div().overflow_hidden().text_ellipsis().child(text.clone()).into_any_element(),
            Cell::Muted(text) => div().overflow_hidden().text_ellipsis().text_color(theme.muted_foreground).child(text.clone()).into_any_element(),
            Cell::Stack { title, subtitle } => v_flex()
                .min_w_0()
                .child(div().overflow_hidden().text_ellipsis().child(title.clone()))
                .child(div().text_xs().text_color(theme.muted_foreground).overflow_hidden().text_ellipsis().child(subtitle.clone()))
                .into_any_element(),
            Cell::Money { text, tone, line_through, detail } => {
                let color = match tone {
                    Tone::Foreground => theme.foreground,
                    Tone::Muted => theme.muted_foreground,
                    Tone::Success => theme.success,
                    Tone::Danger => theme.danger,
                };
                v_flex()
                    .min_w_0()
                    .when(align == Align::Right, |this| this.items_end())
                    .child(
                        div()
                            .font_family(theme.mono_font_family.clone())
                            .text_color(color)
                            .when(*line_through, |this| this.line_through())
                            .child(text.clone()),
                    )
                    .when_some(detail.clone(), |this, detail| this.child(div().text_xs().text_color(theme.muted_foreground).child(detail)))
                    .into_any_element()
            }
            // Tags are centred in the row like the text cells are.
            Cell::Certainty(certainty) => centred(labels::certainty_tag(*certainty)),
            Cell::Status(status) => centred(match status {
                OccurrenceStatus::Overdue => Tag::danger().xsmall().outline().child(status.label()),
                OccurrenceStatus::Due | OccurrenceStatus::PartiallyFulfilled => Tag::warning().xsmall().outline().child(status.label()),
                _ => Tag::secondary().xsmall().outline().child(status.label()),
            }),
            Cell::Chip(text) => centred(Tag::secondary().xsmall().outline().child(text.clone())),
        }
    }
}

fn centred(tag: impl IntoElement) -> AnyElement {
    h_flex().h_full().items_center().child(tag).into_any_element()
}

/// One row, ready to paint. `muted` greys the whole row (a skipped or
/// fulfilled occurrence, a tax payable after the horizon).
#[derive(Clone, Debug)]
pub struct Row {
    pub cells: Vec<Cell>,
    pub muted: bool,
}

impl Row {
    pub fn new(cells: Vec<Cell>) -> Self {
        Row { cells, muted: false }
    }

    pub fn muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }
}

/// Rows shared between a screen model and the grid that shows them.
pub type Rows = Arc<Vec<Row>>;

/// The delegate gpui-kit's table asks for columns and cells.
pub struct GridDelegate {
    columns: Vec<GridColumn>,
    rows: Rows,
}

impl GridDelegate {
    /// The rows currently shown.
    pub fn rows(&self) -> &Rows {
        &self.rows
    }
}

impl TableDelegate for GridDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        let column = &self.columns[col_ix];
        let mut built = Column::new(column.key, column.name).width(px(column.width)).min_width(px(48.));
        if column.align == Align::Right {
            built = built.text_right();
        }
        built
    }

    fn render_tr(&mut self, row_ix: usize, _: &mut Window, cx: &mut Context<TableState<Self>>) -> Stateful<Div> {
        let muted = self.rows.get(row_ix).is_some_and(|row| row.muted);
        div().id(("row", row_ix)).when(muted, |this| this.text_color(cx.theme().muted_foreground))
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, _: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        let align = self.columns.get(col_ix).map(|c| c.align).unwrap_or(Align::Left);
        match self.rows.get(row_ix).and_then(|row| row.cells.get(col_ix)) {
            Some(cell) => cell.render(align, cx),
            None => div().into_any_element(),
        }
    }

    fn cell_text(&self, row_ix: usize, col_ix: usize, _: &App) -> String {
        self.rows.get(row_ix).and_then(|row| row.cells.get(col_ix)).map(Cell::as_text).unwrap_or_default()
    }
}

/// The retained state behind one grid.
pub type Grid = Entity<TableState<GridDelegate>>;

/// Creates a grid with fixed columns and no rows; [`sync`] fills it.
pub fn new_grid(columns: Vec<GridColumn>, window: &mut Window, cx: &mut App) -> Grid {
    cx.new(|cx| TableState::new(GridDelegate { columns, rows: Arc::new(Vec::new()) }, window, cx).row_selectable(false).col_selectable(false).col_movable(false).sortable(false))
}

/// A grid whose rows select on click (the table emits `TableEvent::SelectRow`).
pub fn new_selectable_grid(columns: Vec<GridColumn>, window: &mut Window, cx: &mut App) -> Grid {
    cx.new(|cx| TableState::new(GridDelegate { columns, rows: Arc::new(Vec::new()) }, window, cx).row_selectable(true).col_selectable(false).col_movable(false).sortable(false))
}

/// Points the grid at `rows` if they are not the rows it already shows.
/// Called from a screen's render: a recomputed model is a new `Arc`, so the
/// comparison is a pointer check and an untouched model costs nothing.
pub fn sync(grid: &Grid, rows: &Rows, cx: &mut App) {
    if Arc::ptr_eq(grid.read(cx).delegate().rows(), rows) {
        return;
    }
    log::debug!("perf: grid rows replaced ({} rows)", rows.len());
    let rows = Arc::clone(rows);
    // Columns are fixed, so no `refresh`: it would rebuild the column groups
    // and drop any width the person dragged.
    grid.update(cx, |state, _| state.delegate_mut().rows = rows);
}

/// Rows shown before the grid scrolls inside itself.
pub const MAX_VISIBLE_ROWS: usize = 11;

/// The grid element. Its height is definite — the grid owns the scroll
/// region inside it — and follows the row count up to [`MAX_VISIBLE_ROWS`],
/// so a short table does not leave a box of empty stripes.
pub fn render(id: &'static str, grid: &Grid, cx: &App) -> impl IntoElement {
    let rows = grid.read(cx).delegate().rows().len();
    let height = ROW_HEIGHT * (rows.clamp(1, MAX_VISIBLE_ROWS) + 1) as f32 + px(2.);
    div().id(id).test_support().w_full().h(height).flex_none().child(DataTable::new(grid).stripe(true).with_size(Size::Size(ROW_HEIGHT)))
}
