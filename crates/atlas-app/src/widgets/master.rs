//! Master–detail scaffolding shared by the entity screens: a selectable list
//! on the leading side, the surplus to the detail pane (design guide
//! "Proportion and layer hierarchy").

use gpui_kit::component::{ActiveTheme as _, h_flex, list::ListItem, v_flex};
use gpui_kit::*;

/// One row of the master list.
pub fn master_item(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    subtitle: impl Into<SharedString>,
    trailing: impl Into<SharedString>,
    selected: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> ListItem {
    let theme = cx.theme();
    ListItem::new(id).selected(selected).on_click(on_click).child(
        h_flex()
            .w_full()
            .items_center()
            .gap_3()
            .py_1()
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().child(title.into()))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(subtitle.into())),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_sm()
                    .font_family(theme.mono_font_family.clone())
                    .text_color(theme.muted_foreground)
                    .child(trailing.into()),
            ),
    )
}

/// The two-pane frame: `master` fixed-width, `detail` takes the surplus.
pub fn master_detail(id: &'static str, master: impl IntoElement, detail: impl IntoElement, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    h_flex()
        .id(id)
        .items_start()
        .gap_6()
        .w_full()
        .child(
            v_flex()
                .w_80()
                .flex_shrink_0()
                .gap_1()
                .border_r_1()
                .border_color(theme.border)
                .pr_4()
                .child(master),
        )
        .child(v_flex().flex_1().min_w_0().gap_6().child(detail))
}

/// Page header shared by every screen.
pub fn page_header(title: impl Into<SharedString>, subtitle: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    v_flex()
        .flex_1()
        .min_w_0()
        .gap_1()
        .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(title.into()))
        .child(div().text_sm().text_color(cx.theme().muted_foreground).child(subtitle.into()))
}
