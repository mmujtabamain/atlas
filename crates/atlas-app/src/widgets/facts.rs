//! Label/value facts: the plain statements a detail screen is mostly made of
//! — a rule's trigger, a grant's purpose, what a disclosure level permits.
//!
//! These cannot use gpui-kit's `DescriptionList`. That component wraps itself
//! and every value cell in `overflow_hidden()`, so the moment one value wraps
//! to a second line the rows below it are clipped out of the element — not
//! shortened, not ellipsized, *gone*, with no scrollbar to suggest anything is
//! missing. Settings was silently losing two contract-required facts that way
//! and the glossary was showing one term per section. `.bordered(false)` does
//! not change it; the `overflow_hidden` is unconditional.
//!
//! So the layout is here instead: a fixed label lane, the value beside it
//! taking the rest of the cell and wrapping as far as it needs, and rows that
//! are ordinary flex children of a container that never clips. A wrapping
//! value makes its list taller, which is the only thing it should ever do.
//!
//! Borderless on purpose, as Settings is: a border around every cell turns a
//! list of plain facts into a spreadsheet, and the label lane is what aligns
//! the column, so no cell needs one to be legible.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;

use super::states::columns;

/// The default label lane. Wide enough for `Economic owners and shares`
/// without wrapping the label itself, which would defeat the lane.
pub const LANE: Pixels = px(200.);

/// One fact, and whether it takes the whole row.
struct Fact {
    label: SharedString,
    value: SharedString,
    wide: bool,
}

/// A list of label/value facts, `per_row` to a row.
pub struct Facts {
    items: Vec<Fact>,
    per_row: usize,
    lane: Pixels,
    id: Option<SharedString>,
}

/// A fact list: one pair per row until [`Facts::columns`] says otherwise.
pub fn facts() -> Facts {
    Facts { items: Vec::new(), per_row: 1, lane: LANE, id: None }
}

impl Facts {
    /// How many pairs share a row. Two is right for short values on a wide
    /// detail; a value that runs to a sentence wants the whole width.
    pub fn columns(mut self, per_row: usize) -> Self {
        self.per_row = per_row.max(1);
        self
    }

    /// Name the list and its rows, so a test can assert that a row below a
    /// wrapping value is still there. Only lists under test need one.
    pub fn id(mut self, id: impl Into<SharedString>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// The width of the label lane. Widen it when the labels are whole
    /// phrases rather than words, so the values still start on one line.
    pub fn lane(mut self, lane: Pixels) -> Self {
        self.lane = lane;
        self
    }

    /// One fact.
    pub fn pair(mut self, label: impl Into<SharedString>, value: impl Into<SharedString>) -> Self {
        self.items.push(Fact { label: label.into(), value: value.into(), wide: false });
        self
    }

    /// A fact that takes the whole row, for a value too long to share one.
    pub fn wide(mut self, label: impl Into<SharedString>, value: impl Into<SharedString>) -> Self {
        self.items.push(Fact { label: label.into(), value: value.into(), wide: true });
        self
    }

    /// Several facts at once.
    pub fn pairs<L: Into<SharedString>, V: Into<SharedString>>(mut self, items: impl IntoIterator<Item = (L, V)>) -> Self {
        for (label, value) in items {
            self = self.pair(label, value);
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl IntoElement for Facts {
    type Element = AnyElement;
    fn into_element(self) -> Self::Element {
        FactsElement(self).into_any_element()
    }
}

#[derive(IntoElement)]
struct FactsElement(Facts);

impl RenderOnce for FactsElement {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let Facts { items, per_row, lane, id } = self.0;
        let row_id = id.clone();

        // A fact's own cell: the label in the lane, the value beside it. The
        // value is `min_w_0` so it wraps inside the cell instead of pushing
        // the row wider, and the row is top-aligned so a label stays beside
        // the first line of a value that took three.
        let cell = move |fact: Fact| -> AnyElement {
            h_flex()
                .w_full()
                .items_start()
                .gap_4()
                .child(div().w(lane).flex_shrink_0().text_sm().text_color(theme.muted_foreground).child(fact.label))
                .child(div().flex_1().min_w_0().text_sm().child(fact.value))
                .into_any_element()
        };

        // Fill rows to `per_row`, except that a wide fact claims a row of its
        // own — it is placed on the next row rather than splitting the one it
        // would overflow, so the pairs above it keep their alignment.
        let mut rows: Vec<Vec<AnyElement>> = Vec::new();
        let mut row: Vec<AnyElement> = Vec::new();
        for fact in items {
            let wide = fact.wide || per_row == 1;
            if wide && !row.is_empty() {
                rows.push(std::mem::take(&mut row));
            }
            row.push(cell(fact));
            if wide || row.len() == per_row {
                rows.push(std::mem::take(&mut row));
            }
        }
        if !row.is_empty() {
            rows.push(row);
        }

        // A short last row keeps its cells the width of the ones above by
        // padding with empty cells, so a trailing odd fact does not stretch
        // across the grid it belongs to.
        let rows = rows.into_iter().enumerate().map(move |(index, mut cells)| {
            if cells.len() > 1 {
                while cells.len() < per_row {
                    cells.push(div().into_any_element());
                }
            }
            let row = columns(cells);
            match &row_id {
                Some(name) => div().id(ElementId::Name(format!("{name}-row-{index}").into())).test_support().w_full().child(row).into_any_element(),
                None => row.into_any_element(),
            }
        });

        let list = v_flex().w_full().gap_3().children(rows);
        match id {
            Some(name) => div().id(ElementId::Name(name)).test_support().w_full().child(list).into_any_element(),
            None => list.into_any_element(),
        }
    }
}
