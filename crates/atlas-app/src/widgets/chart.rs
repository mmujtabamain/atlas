//! The cash path: a dated line chart with the hard floor, a `Values` tab that
//! lists every posting exactly (the guaranteed keyboard path), a legend and
//! the readout of the selected point.
//!
//! Screens build the chart element themselves (their point types differ);
//! this module owns the tab, the selection and the frame around them.

use gpui_kit::component::{ActiveTheme as _, Sizable as _, button::{Button, ButtonVariants as _}, h_flex, tab::{Tab, TabBar}, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Retained state of one cash path: which tab, which point is selected.
#[derive(Default, Debug, Clone)]
pub struct PathState {
    pub values: bool,
    pub selected: Option<usize>,
}

/// One legend entry.
pub struct Legend {
    pub name: SharedString,
    pub color: Hsla,
}

/// A command shown above the chart (`Show lowest point`, `Show first breach`).
pub struct PathCommand {
    pub id: &'static str,
    pub label: &'static str,
    pub select: usize,
}

/// Renders the frame: header line, tab bar, chart or values, legend, readout.
#[allow(clippy::too_many_arguments)]
pub fn cash_path(
    id: &'static str,
    state: &Entity<PathState>,
    context: impl Into<SharedString>,
    chart: AnyElement,
    values: AnyElement,
    legend: Vec<Legend>,
    commands: Vec<PathCommand>,
    readout: Option<AnyElement>,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let show_values = state.read(cx).values;
    let tabs = state.clone();
    v_flex()
        .id(id)
        .test_support()
        .w_full()
        .gap_3()
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_end()
                .gap_4()
                .child(div().text_xs().text_color(theme.muted_foreground).child(context.into()))
                .child(
                    TabBar::new(ElementId::Name(format!("{id}-tabs").into()))
                        .selected_index(if show_values { 1 } else { 0 })
                        .on_click(move |index: &usize, _, cx| tabs.update(cx, |s, cx| { s.values = *index == 1; cx.notify(); }))
                        .children([Tab::new().label("Chart"), Tab::new().label("Values")]),
                ),
        )
        .child(if show_values { values } else { chart })
        .child(
            h_flex()
                .w_full()
                .flex_wrap()
                .gap_4()
                .items_center()
                .text_xs()
                .text_color(theme.muted_foreground)
                .children(legend.into_iter().map(|l| h_flex().gap_1().items_center().child(div().size_2().rounded_full().bg(l.color)).child(l.name)))
                .children(commands.into_iter().map(|c| {
                    let state = state.clone();
                    Button::new(c.id).xsmall().ghost().compact().label(c.label).on_click(move |_, _, cx| state.update(cx, |s, cx| { s.selected = Some(c.select); cx.notify(); }))
                })),
        )
        .when_some(readout, |this, r| this.child(r))
}

/// The readout of the selected point: date and order, then the exact figures.
pub fn readout(label: impl Into<SharedString>, figures: Vec<AnyElement>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .w_full()
        .gap_1()
        .pt_2()
        .border_t_1()
        .border_color(theme.border)
        .child(div().text_xs().text_color(theme.muted_foreground).child(label.into()))
        .child(h_flex().w_full().flex_wrap().gap_6().items_start().children(figures))
        .into_any_element()
}
