//! The calculation sheet: "Why is this number this number?"
//!
//! It renders a projected [`ProvNode`] as a calculation chain: one row per
//! term with its sign, the result, then the nested chains of any term that is
//! itself derived. The header states the viewer, the currency and the weakest
//! disclosure level so nobody mistakes an authorized total for a source
//! figure; the footer carries the permitted claim and a copy command.

use std::sync::Arc;

use atlas_core::provenance::{Operation, ProvNode, Sign};
use atlas_core::{Disclosure, Money};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    separator::Separator,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::figure::metadata_terms;

/// Everything the sheet needs; shared by the figure that opens it.
///
/// Built once per screen model (not per frame) and handed around as an
/// `Arc`: the figure, its `Explain…` handler and the open sheet all point at
/// the same content, and the chain inside is the engine's own graph
/// (`Calc::shared_node`), never a copy.
#[derive(Clone, Debug)]
pub struct ExplainContent {
    pub title: String,
    pub value: Money,
    /// Already projected for the viewer.
    pub node: Arc<ProvNode>,
    pub viewer_name: String,
    pub disclosure: Disclosure,
    /// Scope stated in the header (boundary, case, plan, dates), when known.
    pub context: Option<String>,
}

impl ExplainContent {
    pub fn new(title: impl Into<String>, value: Money, node: Arc<ProvNode>, viewer_name: impl Into<String>, disclosure: Disclosure) -> Self {
        ExplainContent { title: title.into(), value, node, viewer_name: viewer_name.into(), disclosure, context: None }
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// The text `Copy explanation` puts on the clipboard: identity, context,
    /// the projected chain and the claim footer.
    pub fn as_text(&self) -> String {
        let strength = self.node.result_strength();
        let mut text = format!("{} = {}\nLooking: {} · Currency: {} · Disclosure: {}\n", self.title, self.value.format(), self.viewer_name, self.value.currency(), self.disclosure.label());
        if let Some(context) = &self.context {
            text.push_str(context);
            text.push('\n');
        }
        text.push('\n');
        text.push_str(&self.node.render_chain());
        text.push_str(&format!("\n{}\nWhat this establishes: {}\nDoes not establish: {}\n", strength.label(), strength.permitted_claim(), strength.does_not_establish()));
        text
    }
}

/// Opens the calculation sheet on the right (WindowExt owns the overlay layer).
pub fn open_sheet(window: &mut Window, cx: &mut App, content: Arc<ExplainContent>) {
    log::info!("calculation sheet opened: {} = {} for {}", content.title, content.value.format(), content.viewer_name);
    window.open_sheet(cx, move |sheet, _window, cx| {
        let content = content.clone();
        let text = content.as_text();
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
            .footer(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .child(super::copy::copy_button("copy-chain", "Copy explanation", text))
                    .child(Button::new("close-explanation").outline().small().label("Close").on_click(|_, window, cx| window.close_sheet(cx))),
            )
    });
}

/// The sheet body; also usable inline.
pub fn render_explanation(content: &ExplainContent, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let node: &ProvNode = &content.node;
    let strength = node.result_strength();
    let mut header_tags = h_flex().gap_1().flex_wrap().items_center().children(metadata_terms("sheet", node));
    header_tags = header_tags.child(super::meanings::term_button("sheet-disclosure", content.disclosure.label(), matches!(content.disclosure, Disclosure::Aggregate | Disclosure::Hidden), super::meanings::Term::Disclosure(content.disclosure)));

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
                .child(div().text_xs().text_color(theme.muted_foreground).child(format!(
                    "Looking: {} · Currency: {} · Disclosure: {}",
                    content.viewer_name,
                    content.value.currency(),
                    content.disclosure.label()
                )))
                .when_some(content.context.clone(), |this, context| this.child(div().text_xs().text_color(theme.muted_foreground).child(context))),
        )
        .child(render_chain(node, 0, cx))
        .child(
            v_flex()
                .gap_1()
                .pt_2()
                .border_t_1()
                .border_color(theme.border)
                .text_xs()
                .child(div().font_weight(FontWeight::MEDIUM).child(strength.label()))
                .child(div().text_color(theme.muted_foreground).child(format!("What this establishes: {}", strength.permitted_claim())))
                .child(div().text_color(theme.muted_foreground).child(format!("Does not establish: {}", strength.does_not_establish()))),
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

/// Only the top block of a chain — for inline previews where the nested
/// blocks would repeat what the calculation sheet shows.
pub fn render_top_block(node: &ProvNode, cx: &App) -> AnyElement {
    render_chain(node, MAX_NESTING, cx)
}

/// An inline preview with its `Full calculation…` command.
pub fn render_preview(node: &ProvNode, content: Arc<ExplainContent>, cx: &App) -> AnyElement {
    v_flex()
        .w_full()
        .gap_2()
        .child(render_top_block(node, cx))
        .child(
            h_flex().w_full().justify_end().child(
                Button::new("full-calculation")
                    .xsmall()
                    .ghost()
                    .compact()
                    .icon(IconName::ListTree)
                    .label("Full calculation…")
                    .on_click(move |_, window, cx| open_sheet(window, cx, content.clone())),
            ),
        )
        .into_any_element()
}

/// The top of a chain as one sentence: `4,400,000 liquid cash − 1,750,000
/// reserved cash = 2,650,000 free current cash`.
///
/// A screen states the equation behind its leading figure; the terms of the
/// terms belong to the calculation sheet, which the line's own command opens.
/// Reading a chain as a table costs a screen thirty lines and answers a
/// question nobody asked yet.
pub fn equation_text(node: &ProvNode) -> String {
    // Only a sum reads as a chain of signed terms. A named formula (a median,
    // a min–max, a clamped difference) has children that are its *inputs*, not
    // addends: joining them with plus signs would state an arithmetic that
    // never happened. Such a node states its formula instead.
    if let Operation::Formula { text } = node.operation() {
        return format!("{text} = {} {}", node.value().render(), equation_label(node.label()));
    }
    // Excluded terms contribute nothing to the arithmetic; the sheet still
    // lists them with the reason they were left out.
    let terms: Vec<&ProvNode> = node.children().iter().filter(|c| !c.is_excluded()).collect();
    if terms.is_empty() {
        return format!("{} = {}", node.label(), node.value().render());
    }
    let mut labels: Vec<String> = terms.iter().map(|t| t.label().to_string()).collect();
    labels.push(node.label().to_string());
    let labels = strip_shared_prefix(labels);
    let (result_label, term_labels) = labels.split_last().expect("pushed the result label");
    let mut text = String::new();
    for (index, (term, label)) in terms.iter().zip(term_labels).enumerate() {
        if index > 0 {
            text.push_str(match term.sign() {
                Sign::Plus => " + ",
                Sign::Minus => " − ",
            });
        } else if term.sign() == Sign::Minus {
            text.push_str("− ");
        }
        text.push_str(&format!("{} {}", term.value().render(), equation_label(label)));
    }
    format!("{text} = {} {}", node.value().render(), equation_label(result_label))
}

/// Fits a chain label into an equation.
///
/// Two things are done to it. A qualifier after an em dash (`Conditional
/// projected cash — expected case`) is separated with a middle dot instead:
/// beside a minus sign that dash reads as arithmetic. And a trailing
/// parenthesis of per-term metadata (`Rent (4 postings, contractual)`) is
/// dropped — with twelve terms it is what turns the line into a paragraph,
/// and it is exactly what the calculation sheet, the Values view and the
/// Basis tab state in full. The term itself is never dropped: an equation
/// that omits a term would not add up.
fn equation_label(label: &str) -> String {
    let label = label.replace(" — ", " · ");
    match label.rfind(" (") {
        Some(at) if label.ends_with(')') => label[..at].to_string(),
        _ => label,
    }
}

/// Drops the words every label in a chain begins with (`Household liquid
/// cash`, `Household reserved cash` → `liquid cash`, `reserved cash`): the
/// boundary is already stated by the screen, and repeating it in every term
/// is what makes an equation too long to read.
fn strip_shared_prefix(labels: Vec<String>) -> Vec<String> {
    let mut words: Vec<Vec<&str>> = labels.iter().map(|l| l.split_whitespace().collect()).collect();
    if words.len() < 2 {
        return labels;
    }
    let mut shared = 0usize;
    loop {
        // Never strip a label down to nothing: every term keeps a name.
        if words.iter().any(|w| w.len() <= shared + 1) {
            break;
        }
        let first = words[0][shared];
        if !words.iter().all(|w| w[shared].eq_ignore_ascii_case(first)) {
            break;
        }
        shared += 1;
    }
    if shared == 0 {
        return labels;
    }
    words.iter_mut().map(|w| w.split_off(shared).join(" ")).collect()
}

/// The equation behind a figure, with the command that opens the whole chain.
pub fn render_equation(id: impl Into<ElementId>, node: &ProvNode, content: Arc<ExplainContent>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    h_flex()
        .w_full()
        .items_start()
        .justify_between()
        .gap_4()
        .child(div().flex_1().min_w_0().text_sm().text_color(theme.foreground).child(equation_text(node)))
        .child(
            div().flex_shrink_0().child(
                Button::new(id)
                    .xsmall()
                    .ghost()
                    .compact()
                    .icon(IconName::ListTree)
                    .label("Full calculation…")
                    .on_click(move |_, window, cx| open_sheet(window, cx, content.clone())),
            ),
        )
        .into_any_element()
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
            block = block.child(note_row(&format!("Excluded: {reason}"), cx));
        }
        if let Operation::Aggregate { .. } = child.operation() {
            block = block.child(note_row("Authorized total — shown only as a combined contribution", cx));
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
        block = block.child(note_row(&format!("Formula: {text}"), cx));
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

#[cfg(test)]
mod tests {
    // Not `use super::*`: this module globs `gpui_kit::*`, whose own `test`
    // attribute macro would shadow the one these tests need.
    use super::equation_text;
    use atlas_core::provenance::ProvNode;
    use atlas_core::{Currency, Money};

    fn money(units: i64) -> Money {
        Money::from_major(units, Currency::PKR)
    }

    fn term(label: &str, units: i64) -> ProvNode {
        ProvNode::input(label, money(units), "test")
    }

    #[test]
    fn a_sum_reads_as_its_signed_terms_without_the_word_they_share() {
        let node = ProvNode::sum(
            "Household free current cash",
            money(2_650_000),
            vec![term("Household liquid cash", 4_400_000), term("Household reserved cash", 1_750_000).minus()],
        );
        assert_eq!(
            equation_text(&node),
            "4,400,000 liquid cash − 1,750,000 reserved cash = 2,650,000 free current cash",
            "the word every term begins with is stated by the screen, not repeated in each term"
        );
    }

    #[test]
    fn a_named_formula_states_its_formula_and_never_a_sum_of_its_inputs() {
        // The children of a median are the sample it was taken over, not
        // addends: joining them with plus signs would claim 450,000.
        let node = ProvNode::formula(
            "Derived monthly salary",
            money(75_000),
            "median of the last 6 reconciled payments",
            vec![term("March payment", 72_000), term("April payment", 78_000)],
        );
        let text = equation_text(&node);
        assert_eq!(text, "median of the last 6 reconciled payments = 75,000 Derived monthly salary");
        assert!(!text.contains('+'), "a median is not a sum: {text}");
    }

    #[test]
    fn a_term_keeps_its_name_when_the_labels_share_no_word() {
        let node = ProvNode::sum("Net worth", money(5_315_000), vec![term("Total assets", 5_400_000), term("Amount owed", 85_000).minus()]);
        assert_eq!(equation_text(&node), "5,400,000 Total assets − 85,000 Amount owed = 5,315,000 Net worth");
    }

    #[test]
    fn a_terms_own_metadata_is_left_to_the_calculation_sheet() {
        // Twelve terms each carrying "(3 postings, contractual)" is what turns
        // the line into a paragraph; the sheet and the Basis tab state it.
        let node = ProvNode::sum("Projected cash — expected case", money(1_500_000), vec![term("Rent (4 postings, contractual)", 1_500_000)]);
        assert_eq!(equation_text(&node), "1,500,000 Rent = 1,500,000 Projected cash · expected case");
    }

    #[test]
    fn a_leaf_states_itself() {
        assert_eq!(equation_text(&term("Reconciled starting cash", 4_400_000)), "Reconciled starting cash = 4,400,000");
    }
}
