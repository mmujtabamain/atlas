//! Empty, filtered-empty, not-disclosed and error states, the honest count
//! line, and the section scaffold every screen is built from.
//!
//! Empty values, hidden values, excluded values and errors are four different
//! states; none of them is rendered as a zero or a dash.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, button::{Button, ButtonVariants as _}, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::meanings::{self, Term};

/// A section: a heading row (title, optional trailing commands) over its
/// content, separated from the previous section by the page's `xl` gap.
pub struct Section {
    id: SharedString,
    title: SharedString,
    actions: Vec<AnyElement>,
    children: Vec<AnyElement>,
    description: Option<SharedString>,
}

pub fn section(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Section {
    Section { id: id.into(), title: title.into(), actions: Vec::new(), children: Vec::new(), description: None }
}

impl Section {
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }

    /// One sentence under the heading, only when it changes how the section
    /// is read.
    pub fn description(mut self, text: impl Into<SharedString>) -> Self {
        self.description = Some(text.into());
        self
    }
}

impl ParentElement for Section {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl IntoElement for Section {
    type Element = AnyElement;
    fn into_element(self) -> Self::Element {
        SectionElement(self).into_any_element()
    }
}

#[derive(IntoElement)]
struct SectionElement(Section);

impl RenderOnce for SectionElement {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let s = self.0;
        v_flex()
            .id(ElementId::Name(s.id))
            .test_support()
            .w_full()
            .gap_3()
            .child(
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .items_center()
                            .gap_4()
                            .child(div().text_base().font_weight(FontWeight::MEDIUM).child(s.title))
                            .when(!s.actions.is_empty(), |this| this.child(h_flex().flex_shrink_0().gap_2().children(s.actions))),
                    )
                    .when_some(s.description, |this, d| this.child(div().text_xs().text_color(theme.muted_foreground).child(d))),
            )
            .children(s.children)
    }
}

/// The page header: title, optional subtitle, trailing commands.
pub fn page_header(title: impl Into<SharedString>, subtitle: Option<SharedString>, actions: Vec<AnyElement>, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    h_flex()
        .w_full()
        .justify_between()
        .items_start()
        .gap_4()
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_1()
                .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(title.into()))
                .when_some(subtitle, |this, s| this.child(div().text_sm().text_color(theme.muted_foreground).child(s))),
        )
        .when(!actions.is_empty(), |this| this.child(h_flex().flex_shrink_0().gap_2().children(actions)))
}

/// `Showing 5 of 9 accounts · 4 not disclosed` — omitted counts when nothing
/// is hidden; never a claim that a filtered zero means the household has none.
pub fn count_line(visible: usize, total: usize, noun: &str, cx: &App) -> impl IntoElement {
    let hidden = total.saturating_sub(visible);
    let text = if hidden == 0 { format!("{visible} {noun}") } else { format!("Showing {visible} of {total} {noun} · {hidden} not disclosed") };
    div().id(ElementId::Name(format!("count-{}", noun.replace(' ', "-")).into())).test_support().text_xs().text_color(cx.theme().muted_foreground).child(text)
}

/// A truly empty collection: the heading, one sentence, one next action.
pub fn empty_state(id: &'static str, heading: impl Into<SharedString>, body: impl Into<SharedString>, action: Option<AnyElement>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .id(id)
        .test_support()
        .w_full()
        .gap_2()
        .py_4()
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(heading.into()))
        .child(div().text_xs().text_color(theme.muted_foreground).child(body.into()))
        .when_some(action, |this, a| this.child(h_flex().pt_1().child(a)))
        .into_any_element()
}

/// A collection whose entries are all hidden from this viewer.
pub fn none_disclosed(id: &'static str, noun: &str, hidden: usize, cx: &App) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .id(id)
        .test_support()
        .w_full()
        .gap_2()
        .py_4()
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(format!("No {noun} are disclosed to this viewer")))
        .child(div().text_xs().text_color(theme.muted_foreground).child(format!("{hidden} not disclosed.")))
        .child(h_flex().pt_1().child(about_access_button(ElementId::Name(format!("{id}-about-access").into()))))
        .into_any_element()
}

/// The `About this access…` command: opens the disclosure meanings.
pub fn about_access_button(id: impl Into<ElementId>) -> impl IntoElement {
    Button::new(id).xsmall().ghost().compact().icon(IconName::Info).label("About this access…").on_click(|_, window, cx| meanings::open_sheet(window, cx, Some(Term::Disclosure(atlas_core::Disclosure::Hidden))))
}

/// A value the viewer may not see: `Not disclosed` with its safe reason.
pub fn not_disclosed(id: impl Into<ElementId>, reason: impl Into<SharedString>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .gap_1()
        .child(div().text_sm().text_color(theme.muted_foreground).child("Not disclosed"))
        .child(div().text_xs().text_color(theme.muted_foreground).child(reason.into()))
        .child(about_access_button(id))
        .into_any_element()
}

/// A muted one-line note.
pub fn note(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div().text_xs().text_color(cx.theme().muted_foreground).child(text.into())
}

/// A labelled plain fact (a raw value, not a derived figure): label over value.
pub fn fact(label: impl Into<SharedString>, value: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    v_flex()
        .gap_1()
        .w_64()
        .flex_shrink_0()
        .child(div().text_xs().text_color(theme.muted_foreground).child(label.into()))
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(value.into()))
}

/// A row of fixed-width lanes that wraps.
pub fn lanes(children: impl IntoIterator<Item = AnyElement>) -> impl IntoElement {
    h_flex().w_full().flex_wrap().gap_8().items_start().children(children)
}
