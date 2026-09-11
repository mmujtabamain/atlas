//! Small helpers so every table in the app shares one geometry: compact rows,
//! right-aligned numbers, muted secondary text.

use atlas_core::Money;
use gpui_kit::component::{ActiveTheme as _, table::TableCell};
use gpui_kit::*;

/// A right-aligned money cell in the theme's monospace face (design guide:
/// monospace for aligned numeric data).
pub fn money_cell(money: Money, cx: &App) -> TableCell {
    let color = if money.is_negative() { cx.theme().danger } else { cx.theme().foreground };
    TableCell::new()
        .text_right()
        .font_family(cx.theme().mono_font_family.clone())
        .text_color(color)
        .child(money.format())
}

/// A muted secondary-text cell.
pub fn muted_cell(text: impl Into<SharedString>, cx: &App) -> TableCell {
    TableCell::new().text_color(cx.theme().muted_foreground).child(text.into())
}
