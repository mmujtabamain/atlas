//! The scope bar: the local analysis header — `Whose money`, `Case`, `Plan`
//! where an analysis accepts them, followed by the fixed facts it does not.
//! It is explicitly local: `Who is looking` stays in the title bar.

use gpui_kit::component::{ActiveTheme as _, IndexPath, Sizable as _, h_flex, select::{Select, SelectState}, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// A retained single-choice control.
pub type Choice = Entity<SelectState<Vec<SharedString>>>;

/// Creates a choice with `selected` preselected.
pub fn choice(items: Vec<SharedString>, selected: usize, window: &mut Window, cx: &mut App) -> Choice {
    let index = if items.is_empty() { None } else { Some(IndexPath::default().row(selected.min(items.len().saturating_sub(1)))) };
    cx.new(|cx| SelectState::new(items, index, window, cx))
}

/// The selected row of a choice (0 when nothing is selected).
pub fn selected(choice: &Choice, cx: &App) -> usize {
    choice.read(cx).selected_index(cx).map(|p| p.row).unwrap_or(0)
}

/// Replaces a choice's items, keeping the selection when it still exists.
pub fn reset(choice: &Choice, items: Vec<SharedString>, selected_row: usize, window: &mut Window, cx: &mut App) {
    let index = if items.is_empty() { None } else { Some(IndexPath::default().row(selected_row.min(items.len().saturating_sub(1)))) };
    choice.update(cx, |state, cx| {
        state.set_items(items, window, cx);
        state.set_selected_index(index, window, cx);
    });
}

/// One labelled control of the bar.
pub fn control(label: &'static str, element: impl IntoElement, cx: &App) -> impl IntoElement {
    v_flex().gap_1().child(div().text_xs().text_color(cx.theme().muted_foreground).child(label)).child(element)
}

/// A labelled select of the bar.
pub fn select(label: &'static str, choice: &Choice, width: Pixels, cx: &App) -> impl IntoElement {
    control(label, Select::new(choice).small().w(width), cx)
}

/// A read-only fact of the bar (`Expected case`, a date range): text, not a
/// disabled picker that suggests a future choice.
pub fn fixed(label: &'static str, value: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    v_flex().gap_1().child(div().text_xs().text_color(theme.muted_foreground).child(label)).child(div().text_sm().py_1().child(value.into()))
}

/// The bar: controls on one wrapping row, the description line beneath.
pub fn bar(controls: Vec<AnyElement>, description: Option<String>, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    v_flex()
        .w_full()
        .gap_2()
        .child(h_flex().flex_wrap().gap_6().items_end().children(controls))
        .when_some(description, |this, text| this.child(div().text_xs().text_color(theme.muted_foreground).child(text)))
}

/// The one-line description of a forecast case.
pub fn case_description(case: atlas_core::forecast::Case) -> &'static str {
    use atlas_core::forecast::Case;
    match case {
        Case::Conservative => "Conservative uses low incomes, late receipts and high expenses.",
        Case::Expected => "Expected uses the entered expected values.",
        Case::Optimistic => "Optimistic uses high incomes, early receipts and low expenses.",
    }
}
