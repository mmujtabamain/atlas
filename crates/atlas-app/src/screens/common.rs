//! Pieces every workspace shares: the workspace header with its local tabs,
//! breadcrumbs for detail screens, and the danger confirmation.

use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, WindowExt as _,
    breadcrumb::{Breadcrumb, BreadcrumbItem},
    button::{Button, ButtonVariants as _},
    dialog::{DialogButtonProps, DialogFooter},
    h_flex,
    tab::{Tab, TabBar},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::AtlasApp;
use crate::nav::{Destination, Route};

/// The header of a workspace screen: the destination's name, its local tabs
/// (the current route's tab selected) and trailing commands.
pub fn workspace_header(destination: Destination, route: Route, actions: Vec<AnyElement>, cx: &mut Context<AtlasApp>) -> AnyElement {
    let tabs = destination.tabs();
    let selected = route.tab_index().unwrap_or(0);
    let routes: Vec<Route> = tabs.iter().map(|(_, r)| *r).collect();
    v_flex()
        .w_full()
        .gap_3()
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_start()
                .gap_4()
                .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(destination.label()))
                .when(!actions.is_empty(), |this| this.child(h_flex().flex_shrink_0().gap_2().children(actions))),
        )
        .when(!tabs.is_empty(), |this| {
            this.child(
                TabBar::new(ElementId::Name(format!("tabs-{}", destination.home().slug()).into()))
                    .selected_index(selected)
                    .on_click(cx.listener(move |this, index: &usize, _, cx| {
                        if let Some(route) = routes.get(*index) {
                            this.navigate(*route, cx);
                        }
                    }))
                    .children(tabs.iter().map(|(label, _)| Tab::new().label(*label))),
            )
        })
        .into_any_element()
}

/// The header of a detail screen: breadcrumb to the collection, the object's
/// name, a subtitle line, trailing commands.
pub fn detail_header(destination: Destination, parent: Route, parent_label: &'static str, name: impl Into<SharedString>, subtitle: Option<AnyElement>, actions: Vec<AnyElement>, cx: &mut Context<AtlasApp>) -> AnyElement {
    let theme = cx.theme();
    let name = name.into();
    v_flex()
        .w_full()
        .gap_2()
        .child(
            Breadcrumb::new()
                .child(BreadcrumbItem::new(destination.label()).on_click(cx.listener(move |this, _, _, cx| this.navigate(destination.home(), cx))))
                // A workspace whose first tab carries its own name gets one crumb, not two.
                .when(parent_label != destination.label(), |this| this.child(BreadcrumbItem::new(parent_label).on_click(cx.listener(move |this, _, _, cx| this.navigate(parent, cx)))))
                .child(BreadcrumbItem::new(name.clone())),
        )
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_start()
                .gap_4()
                .child(v_flex().flex_1().min_w_0().gap_1().child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(name)).children(subtitle))
                .when(!actions.is_empty(), |this| this.child(h_flex().flex_shrink_0().gap_2().children(actions))),
        )
        .child(div().h_0().text_color(theme.muted_foreground))
        .into_any_element()
}

/// A `Back` command to the previous route.
pub fn back_button(cx: &mut Context<AtlasApp>) -> impl IntoElement {
    Button::new("back").small().ghost().icon(gpui_kit::assets::IconName::ArrowLeft).label("Back").on_click(cx.listener(|this, _, _, cx| this.go_back(cx)))
}

/// A confirmation for an irreversible action: title, the consequence, a
/// danger commit. `on_ok` runs only when confirmed.
pub fn confirm_danger(window: &mut Window, cx: &mut App, title: impl Into<SharedString>, body: impl Into<SharedString>, ok_label: &'static str, on_ok: impl Fn(&mut Window, &mut App) + 'static) {
    let title = title.into();
    let body = body.into();
    let on_ok = std::rc::Rc::new(on_ok);
    window.open_alert_dialog(cx, move |alert, _, _| {
        let on_ok = on_ok.clone();
        alert
            .title(title.clone())
            .description(body.clone())
            .show_cancel(true)
            .button_props(DialogButtonProps::default().ok_text(ok_label).ok_variant(gpui_kit::component::button::ButtonVariant::Danger).cancel_text("Cancel").on_ok(move |_, window, cx| {
                on_ok(window, cx);
                true
            }))
    });
}

/// A confirmation for a consequential but not destructive action: the facts
/// as lines, a primary commit. `on_ok` runs only when confirmed.
pub fn confirm_primary(window: &mut Window, cx: &mut App, title: impl Into<SharedString>, lines: Vec<String>, ok_label: &'static str, on_ok: impl Fn(&mut Window, &mut App) + 'static) {
    let title = title.into();
    let on_ok = std::rc::Rc::new(on_ok);
    let lines = std::rc::Rc::new(lines);
    window.open_dialog(cx, move |dialog, _, cx| {
        let on_ok = on_ok.clone();
        let muted = cx.theme().muted_foreground;
        dialog
            .title(title.clone())
            .w_96()
            .child(v_flex().gap_1().text_sm().children(lines.iter().enumerate().map(|(i, line)| div().when(i > 0, |d| d.text_xs().text_color(muted)).child(line.clone()))))
            .footer(
                DialogFooter::new()
                    .child(Button::new("confirm-cancel").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                    .child(Button::new("confirm-ok").primary().label(ok_label).on_click(move |_, window, cx| {
                        window.close_dialog(cx);
                        on_ok(window, cx);
                    })),
            )
    })
}
