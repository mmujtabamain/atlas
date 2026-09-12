//! The "Why is this number this number?" sheet (§2.1, §2.6).
//!
//! It renders a projected [`ProvNode`] as the plan's chain: one row per term
//! with its sign, a rule, the result, then the nested chains of any term that
//! is itself derived. The viewer and their disclosure level are stated at the
//! top so nobody mistakes an aggregate for a source figure.

use std::sync::Arc;

use atlas_core::provenance::{Operation, ProvNode, Sign};
use atlas_core::{Disclosure, Money};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, WindowExt as _, clipboard::Clipboard, h_flex, separator::Separator, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::labels;

/// Everything the sheet needs; shared by the figure that opens it.
///
/// Built once per screen model (not per frame) and handed around as an
/// `Arc`: the figure, its "Why?" click handler and the open sheet all point
/// at the same content, and the chain inside is the engine's own graph
/// (`Calc::shared_node`), never a copy.
#[derive(Clone, Debug)]
pub struct ExplainContent {
    pub title: String,
    pub value: Money,
    /// Already projected for the viewer (M55).
    pub node: Arc<ProvNode>,
    pub viewer_name: String,
    pub disclosure: Disclosure,
}

impl ExplainContent {
    pub fn new(title: impl Into<String>, value: Money, node: Arc<ProvNode>, viewer_name: impl Into<String>, disclosure: Disclosure) -> Self {
        ExplainContent { title: title.into(), value, node, viewer_name: viewer_name.into(), disclosure }
    }
}

/// Opens the explain sheet on the right (WindowExt owns the overlay layer).
pub fn open_sheet(window: &mut Window, cx: &mut App, content: Arc<ExplainContent>) {
    log::info!("explain sheet opened: {} = {} for {}", content.title, content.value.format(), content.viewer_name);
    window.open_sheet(cx, move |sheet, _window, cx| {
        let content = content.clone();
        sheet
            .title(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(Icon::new(IconName::ListTree).small())
                    .child(format!("Why is {} {}?", content.title, content.value.format())),
            )
            .size(relative(0.5))
            .child(render_explanation(&content, cx))
    });
}

/// The sheet body; also usable inline.
pub fn render_explanation(content: &ExplainContent, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let node: &ProvNode = &content.node;
    let strength = node.result_strength();
    let mut header_tags = h_flex().gap_1().flex_wrap();
    if let Some(class) = node.money_class_label() {
        header_tags = header_tags.child(labels::money_class_tag(class));
    }
    if let Some(certainty) = node.certainty_label() {
        header_tags = header_tags.child(labels::certainty_tag(certainty));
    }
    header_tags = header_tags.child(labels::strength_tag(strength)).child(labels::disclosure_tag(content.disclosure));

    v_flex()
        .id("explain-chain")
        .test_support()
        .py_4()
        .gap_4()
        .text_sm()
        .child(
            v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .items_baseline()
                        .justify_between()
                        .gap_4()
                        .child(div().text_color(theme.muted_foreground).child(content.title.clone()))
                        .child(
                            div()
                                .text_xl()
                                .font_weight(FontWeight::SEMIBOLD)
                                .font_family(theme.mono_font_family.clone())
                                .child(content.value.format()),
                        ),
                )
                .child(header_tags)
                .child(
                    div().text_xs().text_color(theme.muted_foreground).child(format!(
                        "Viewer: {} · disclosure: {} · every value below is in {}",
                        content.viewer_name,
                        content.disclosure.label(),
                        content.value.currency()
                    )),
                ),
        )
        .child(render_chain(node, 0, cx))
        .child(
            v_flex()
                .gap_1()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format!("{}: {}", strength.label(), strength.permitted_claim()))
                .child(format!("Does not establish: {}", strength.does_not_establish())),
        )
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Chain as text (§2.1 layout)")
                .child(Clipboard::new("copy-chain").value(node.render_chain()).tooltip("Copy the chain as text")),
        )
}

fn sign_glyph(node: &ProvNode, first: bool) -> &'static str {
    if node.is_excluded() {
        "×"
    } else if first {
        ""
    } else {
        match node.sign() {
            Sign::Plus => "+",
            Sign::Minus => "−",
        }
    }
}

/// Only the top block of a chain — for inline use where the nested blocks
/// would repeat what the explain sheet shows.
pub fn render_top_block(node: &ProvNode, cx: &App) -> AnyElement {
    render_chain(node, MAX_NESTING, cx)
}

/// Nested blocks deeper than this are left to the sheet's own scrolling.
const MAX_NESTING: usize = 6;

/// One block: the terms, a rule, the result; nested blocks indented below.
pub fn render_chain(node: &ProvNode, depth: usize, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let mono = theme.mono_font_family.clone();
    // `w_full` everywhere in the chain: a definite width stops taffy from
    // re-measuring every term for the block's content width at each level.
    let mut block = v_flex().w_full().gap_1();

    if node.children().is_empty() {
        return block
            .child(term_row("", node.label(), node.value().render(), false, &mono, cx))
            .children(node.notes().iter().map(|n| note_row(n, cx)))
            .into_any_element();
    }

    let mut first = true;
    for child in node.children() {
        let glyph = sign_glyph(child, first && !child.is_excluded());
        if !child.is_excluded() {
            first = false;
        }
        block = block.child(term_row(glyph, child.label(), child.value().render(), child.is_excluded(), &mono, cx));
        if let Operation::Excluded { reason } = child.operation() {
            block = block.child(note_row(&format!("excluded: {reason}"), cx));
        }
        if let Operation::Aggregate { .. } = child.operation() {
            block = block.child(note_row("restricted contribution (M55 projection)", cx));
        }
        for note in child.notes() {
            block = block.child(note_row(note, cx));
        }
    }
    block = block.child(Separator::horizontal()).child(
        h_flex()
            .w_full()
            .gap_2()
            .items_baseline()
            .font_weight(FontWeight::SEMIBOLD)
            .child(div().w_4().flex_shrink_0().child("="))
            .child(div().flex_1().min_w_0().child(node.label().to_string()))
            .child(div().w_32().flex_shrink_0().text_right().font_family(mono.clone()).child(node.value().render())),
    );
    if let Operation::Formula { text } = node.operation() {
        block = block.child(note_row(&format!("formula: {text}"), cx));
    }
    for note in node.notes() {
        block = block.child(note_row(note, cx));
    }

    let nested: Vec<&ProvNode> = node.children().iter().filter(|c| !c.children().is_empty()).collect();
    if !nested.is_empty() && depth < MAX_NESTING {
        block = block.child(
            v_flex().w_full().mt_2().gap_3().children(nested.into_iter().map(|child| {
                v_flex()
                    .w_full()
                    .pl_3()
                    .border_l_1()
                    .border_color(theme.border)
                    .gap_1()
                    .child(div().text_xs().text_color(theme.muted_foreground).child(format!("Where “{}” comes from", child.label())))
                    .child(render_chain(child, depth + 1, cx))
            })),
        );
    }
    block.into_any_element()
}

fn term_row(glyph: &str, label: &str, value: String, excluded: bool, mono: &SharedString, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let color = if excluded { theme.muted_foreground } else { theme.foreground };
    h_flex()
        .w_full()
        .gap_2()
        .items_baseline()
        .text_color(color)
        .child(div().w_4().flex_shrink_0().text_color(theme.muted_foreground).child(glyph.to_string()))
        .child(div().flex_1().min_w_0().child(label.to_string()))
        .child(
            div()
                .w_32()
                .flex_shrink_0()
                .text_right()
                .font_family(mono.clone())
                .when(excluded, |this| this.line_through())
                .child(value),
        )
}

fn note_row(note: &str, cx: &App) -> impl IntoElement {
    h_flex()
        .w_full()
        .gap_2()
        .child(div().w_4().flex_shrink_0())
        .child(div().flex_1().min_w_0().text_xs().text_color(cx.theme().muted_foreground).child(note.to_string()))
}
