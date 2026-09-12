//! The conditional statement: what a result establishes, through when, on
//! which assumptions, with what coverage — and what it does not establish.
//! Forecasts, scenario comparisons and purchase verdicts all end in one.

use atlas_core::model::{Assumption, Freshness};
use atlas_core::vocab::{Certainty, ResultStrength};
use chrono::NaiveDate;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, button::{Button, ButtonVariants as _}, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::labels;

/// One assumption line, already projected for the viewer.
#[derive(Clone, Debug)]
pub struct Line {
    pub text: String,
    pub certainty: Certainty,
    pub freshness: Option<Freshness>,
    pub source: String,
}

impl Line {
    pub fn from_assumption(a: &Assumption, on: NaiveDate) -> Self {
        Line { text: a.text.clone(), certainty: a.certainty, freshness: Some(a.freshness(on)), source: a.source.describe() }
    }
}

#[derive(Clone, Debug)]
pub struct Statement {
    pub claim: String,
    pub through: NaiveDate,
    /// `<boundary> | <case> | <plan>`.
    pub scope: String,
    pub assumptions: Vec<Line>,
    /// Whether some supporting assumptions are withheld from this viewer.
    pub some_hidden: bool,
    pub strength: ResultStrength,
    pub coverage: String,
    pub does_not_establish: String,
    pub excluded_shocks: String,
}

impl Statement {
    /// The whole statement as text, for `Copy statement`.
    pub fn as_text(&self) -> String {
        let mut out = String::new();
        out.push_str("What this result establishes\n");
        out.push_str(&self.claim);
        out.push('\n');
        out.push_str(&format!("Through {} | {}\n", self.through.format("%d %b %Y"), self.scope));
        out.push_str("Assumptions\n");
        if self.assumptions.is_empty() {
            out.push_str("  No assumptions recorded\n");
        }
        for (i, a) in self.assumptions.iter().enumerate() {
            out.push_str(&format!("  {}. {} — {}{}\n", i + 1, a.text, a.certainty.label(), a.freshness.map(|f| format!(", {}", freshness_label(f))).unwrap_or_default()));
        }
        if self.some_hidden {
            out.push_str("  Some supporting assumptions are not disclosed\n");
        }
        out.push_str(&format!("Coverage: {} — {}\n", self.strength.label(), self.coverage));
        out.push_str(&format!("Does not establish: {}\n", self.does_not_establish));
        out.push_str(&format!("Excluded shocks: {}\n", self.excluded_shocks));
        out
    }
}

fn freshness_label(f: Freshness) -> &'static str {
    match f {
        Freshness::NotAccepted => "not yet accepted",
        Freshness::Fresh => "accepted",
        Freshness::Stale => "stale",
        Freshness::Expired => "expired",
    }
}

/// Assumptions shown before `Show all`.
pub const PREVIEW: usize = 3;

/// Renders the statement. `expanded` shows every assumption; `on_toggle`
/// flips it. `actions` sit at the foot (Copy statement, Review assumptions…).
pub fn render(id: &'static str, statement: &Statement, expanded: bool, on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static, actions: Vec<AnyElement>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let total = statement.assumptions.len();
    let shown: Vec<&Line> = statement.assumptions.iter().take(if expanded { total } else { PREVIEW }).collect();
    let label = |text: &'static str| div().w_40().flex_shrink_0().text_xs().text_color(theme.muted_foreground).child(text);
    v_flex()
        .id(id)
        .test_support()
        .w_full()
        .gap_3()
        .p_4()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .bg(theme.secondary)
        .child(div().text_xs().text_color(theme.muted_foreground).child("What this result establishes"))
        .child(div().id(ElementId::Name(format!("{id}-claim").into())).test_support().text_sm().font_weight(FontWeight::MEDIUM).child(statement.claim.clone()))
        .child(div().text_xs().text_color(theme.muted_foreground).child(format!("Through {} · {}", statement.through.format("%d %b %Y"), statement.scope)))
        .child(
            v_flex()
                .w_full()
                .gap_1()
                .child(div().text_xs().text_color(theme.muted_foreground).child("Assumptions"))
                .when(statement.assumptions.is_empty() && !statement.some_hidden, |this| this.child(div().text_sm().child("No assumptions recorded")))
                .children(shown.iter().enumerate().map(|(i, a)| {
                    v_flex()
                        .w_full()
                        .gap_0p5()
                        .child(div().w_full().text_sm().child(format!("{}. {}", i + 1, a.text)))
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(labels::certainty_tag(a.certainty))
                                .when_some(a.freshness, |this, f| this.child(labels::freshness_tag(f)))
                                .child(div().text_xs().text_color(theme.muted_foreground).child(a.source.clone())),
                        )
                }))
                .when(total > PREVIEW, |this| {
                    this.child(h_flex().child(Button::new(ElementId::Name(format!("{id}-show-all").into())).xsmall().ghost().compact().label(if expanded { "Show fewer".to_string() } else { format!("Show all {total}") }).on_click(on_toggle)))
                })
                .when(statement.some_hidden, |this| this.child(div().text_xs().text_color(theme.muted_foreground).child("Some supporting assumptions are not disclosed to this viewer."))),
        )
        .child(h_flex().w_full().gap_2().items_start().child(label("Coverage")).child(h_flex().flex_1().min_w_0().gap_2().items_start().child(labels::strength_tag(statement.strength)).child(div().flex_1().min_w_0().text_sm().child(statement.coverage.clone()))))
        .child(h_flex().w_full().gap_2().items_start().child(label("Does not establish")).child(div().flex_1().min_w_0().text_sm().child(statement.does_not_establish.clone())))
        .child(h_flex().w_full().gap_2().items_start().child(label("Excluded shocks")).child(div().flex_1().min_w_0().text_sm().child(statement.excluded_shocks.clone())))
        .when(!actions.is_empty(), |this| this.child(h_flex().w_full().justify_end().gap_2().flex_wrap().children(actions)))
        .into_any_element()
}
