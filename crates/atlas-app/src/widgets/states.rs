//! Empty, filtered-empty, not-disclosed and error states, the honest count
//! line, and the section scaffold every screen is built from.
//!
//! Empty values, hidden values, excluded values and errors are four different
//! states; none of them is rendered as a zero or a dash.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, button::{Button, ButtonVariants as _}, h_flex, tag::Tag, v_flex};
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
    badge: Option<SharedString>,
    divider: bool,
}

pub fn section(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Section {
    Section { id: id.into(), title: title.into(), actions: Vec::new(), children: Vec::new(), description: None, badge: None, divider: false }
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

    /// A qualifier beside the heading (`Expected · Baseline`) — the reading
    /// the section is under, where the heading alone would not say it.
    pub fn badge(mut self, text: impl Into<SharedString>) -> Self {
        self.badge = Some(text.into());
        self
    }

    /// Rule the section off from the one above it. Bands of one screen that
    /// answer different questions are separated by a line, not by a gap
    /// alone (the current band, the outlook, the assumptions behind it).
    pub fn divider(mut self, divider: bool) -> Self {
        self.divider = divider;
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
        let badge = s.badge;
        v_flex()
            .id(ElementId::Name(s.id))
            .test_support()
            .w_full()
            .gap_3()
            .when(s.divider, |this| this.pt_6().border_t_1().border_color(theme.border))
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
                            .child(
                                h_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .items_center()
                                    .gap_3()
                                    .child(div().text_base().font_weight(FontWeight::MEDIUM).child(s.title))
                                    .when_some(badge, |this, b| this.child(Tag::secondary().xsmall().child(b))),
                            )
                            .when(!s.actions.is_empty(), |this| this.child(h_flex().flex_shrink_0().gap_2().children(s.actions))),
                    )
                    .when_some(s.description, |this, d| this.child(div().text_xs().text_color(theme.muted_foreground).child(d))),
            )
            .children(s.children)
    }
}

/// The page header: title, optional subtitle, trailing commands.
pub fn page_header(title: impl Into<SharedString>, subtitle: Option<SharedString>, actions: Vec<AnyElement>, cx: &App) -> impl IntoElement {
    header(None, title, subtitle, actions, cx)
}

/// The page header of a detail: the trail that reaches it above the title,
/// so a screen opened from a register says where it sits.
pub fn detail_header(trail: impl Into<SharedString>, title: impl Into<SharedString>, subtitle: Option<SharedString>, actions: Vec<AnyElement>, cx: &App) -> impl IntoElement {
    header(Some(trail.into()), title, subtitle, actions, cx)
}

fn header(trail: Option<SharedString>, title: impl Into<SharedString>, subtitle: Option<SharedString>, actions: Vec<AnyElement>, cx: &App) -> impl IntoElement {
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
                .when_some(trail, |this, t| this.child(div().text_xs().text_color(theme.muted_foreground).child(t)))
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

/// An even row of columns: every cell the same fraction of the width, so a
/// row of six figures reads as one grid instead of a ragged wrap.
///
/// The gutter is padding *inside* the cell rather than a `gap` on the row: a
/// gap on top of relative widths sums past 100 % and overflows. The width is
/// definite (a fraction of a parent that is itself definite), which is what
/// keeps taffy from measuring each cell's content at every ancestor pass
/// (`docs/perf.md` §3.2).
pub fn columns(children: impl IntoIterator<Item = AnyElement>) -> impl IntoElement {
    let children: Vec<AnyElement> = children.into_iter().collect();
    let count = children.len().max(1);
    let width = relative(1. / count as f32);
    let last = count - 1;
    h_flex().w_full().items_start().children(
        children
            .into_iter()
            .enumerate()
            .map(move |(index, child)| div().w(width).flex_shrink_0().min_w_0().when(index != last, |this| this.pr_6()).child(child)),
    )
}

/// The same grid, with the leading cell taking `lead` of the width and the
/// rest sharing what is left — a leading figure beside its supporting ones.
pub fn columns_leading(lead: f32, children: impl IntoIterator<Item = AnyElement>) -> impl IntoElement {
    let children: Vec<AnyElement> = children.into_iter().collect();
    let count = children.len().max(1);
    let rest = if count > 1 { (1. - lead) / (count - 1) as f32 } else { lead };
    let last = count - 1;
    h_flex().w_full().items_start().children(children.into_iter().enumerate().map(move |(index, child)| {
        div()
            .w(relative(if index == 0 { lead } else { rest }))
            .flex_shrink_0()
            .min_w_0()
            .when(index != last, |this| this.pr_6())
            .child(child)
    }))
}

/// A rule between two bands of one screen.
pub fn hairline(cx: &App) -> impl IntoElement {
    div().w_full().h(px(1.)).bg(cx.theme().border)
}

/// A bordered card that states one fact about the screen: an icon, a heading
/// and the sentence under it (`No hard-floor breach`, `Precedence is
/// explicit`). Not an alert — it is as true when nothing is wrong.
pub fn info_card(id: &'static str, icon: IconName, title: impl Into<SharedString>, body: impl Into<SharedString>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .id(id)
        .test_support()
        .w_full()
        .gap_2()
        .p_4()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .items_center()
                .child(Icon::new(icon).small().text_color(theme.muted_foreground))
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title.into())),
        )
        .child(div().w_full().text_xs().text_color(theme.muted_foreground).child(body.into()))
        .into_any_element()
}

/// The commands that belong to the whole screen, ruled off at its foot: what
/// this screen can do on the right, where it can take you on the left.
pub fn action_bar(id: &'static str, leading: Vec<AnyElement>, trailing: Vec<AnyElement>) -> impl IntoElement {
    h_flex()
        .id(id)
        .test_support()
        .w_full()
        .justify_between()
        .items_center()
        .gap_4()
        .pt_3()
        .child(h_flex().gap_2().items_center().children(leading))
        .child(h_flex().gap_2().items_center().children(trailing))
}
