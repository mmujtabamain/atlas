//! A figure that can always explain itself.
//!
//! Every derived money value on a screen is rendered through [`Figure`]: the
//! label, the value in the monospace face, the vocabulary tags of its
//! provenance node and a "Why?" button that opens the explain sheet. There is
//! deliberately no way to render a bare derived number.

use std::rc::Rc;

use atlas_core::authz::Viewer;
use atlas_core::model::Household;
use atlas_core::provenance::{Operation, ProvNode};
use atlas_core::{Calc, Disclosure, Money};
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, button::{Button, ButtonVariants as _}, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::explain::{self, ExplainContent};
use super::labels;

/// One card in a row of figures (`h_flex().flex_wrap().gap_8()`).
///
/// Cards have a fixed width on purpose. With auto-width cards taffy has to
/// measure every card's whole content — label, value, tags — again for each
/// candidate wrap line, and that measurement repeats at every ancestor's
/// sizing pass: the Household money row alone cost 5,200 measure callbacks
/// per frame; fixed at 16 rem it costs 460 (see perf.rs / logs.log
/// `taffy` figures). Long labels wrap inside the card.
pub fn card(content: impl IntoElement) -> Div {
    div().w_64().flex_shrink_0().child(content)
}

/// A labelled, explainable money figure.
#[derive(IntoElement)]
pub struct Figure {
    id: SharedString,
    label: SharedString,
    calc: Calc<Money>,
    content: Rc<ExplainContent>,
    emphasis: bool,
}

impl Figure {
    /// `id` must be stable and unique on the screen (`free-cash`, …).
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>, calc: Calc<Money>, content: ExplainContent) -> Self {
        Figure { id: id.into(), label: label.into(), calc, content: Rc::new(content), emphasis: false }
    }

    /// Larger value text for the one figure a screen is about.
    pub fn emphasis(mut self, emphasis: bool) -> Self {
        self.emphasis = emphasis;
        self
    }
}

impl RenderOnce for Figure {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let money = self.calc.money();
        let node = self.calc.node();
        let value_color = if money.is_negative() { theme.danger } else { theme.foreground };
        let value_id = SharedString::from(format!("figure-{}", self.id));
        let why_id = SharedString::from(format!("why-{}", self.id));
        let content = self.content.clone();

        // Definite width: an auto-width row of tags is re-measured by taffy at
        // every ancestor pass (see perf.rs).
        let mut tags = h_flex().w_full().gap_1().flex_wrap().items_center();
        if let Some(class) = node.money_class_label() {
            tags = tags.child(labels::money_class_tag(class));
        }
        if let Some(certainty) = node.certainty_label() {
            tags = tags.child(labels::certainty_tag(certainty));
        }
        tags = tags.child(labels::strength_tag(node.result_strength())).child(
            Button::new(why_id)
                .xsmall()
                .ghost()
                .compact()
                .icon(IconName::CircleQuestionMark)
                .label("Why?")
                .tooltip("Show the calculation chain")
                .on_click(move |_, window, cx| explain::open_sheet(window, cx, content.clone())),
        );

        v_flex()
            .w_full()
            .gap_1()
            .min_w_0()
            .child(div().text_xs().text_color(theme.muted_foreground).child(self.label))
            .child(
                div()
                    .id(value_id)
                    .test_support()
                    .font_family(theme.mono_font_family.clone())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(value_color)
                    .when(self.emphasis, |this| this.text_2xl())
                    .when(!self.emphasis, |this| this.text_xl())
                    .child(money.format()),
            )
            .child(tags)
    }
}

/// A projected, explainable figure: the model half of a [`Figure`], computed
/// once per state change and rendered by any screen.
#[derive(Clone, Debug)]
pub struct ExplainedFigure {
    pub id: SharedString,
    pub label: SharedString,
    /// Node already projected for the viewer (M55).
    pub calc: Calc<Money>,
}

impl ExplainedFigure {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>, calc: &Calc<Money>, household: &Household, viewer: Viewer) -> Self {
        let projected = calc.node().project(&household.disclosure_fn(viewer));
        ExplainedFigure { id: id.into(), label: label.into(), calc: Calc::new(calc.money(), projected) }
    }

    /// The weakest disclosure level present in the projected chain.
    pub fn disclosure(&self) -> Disclosure {
        fn scan(node: &ProvNode) -> Disclosure {
            let own = match node.operation() {
                Operation::Aggregate { restricted_terms: 0 } => Disclosure::Hidden,
                Operation::Aggregate { .. } => Disclosure::Aggregate,
                _ => Disclosure::Full,
            };
            node.children().iter().map(scan).fold(own, Disclosure::min)
        }
        scan(self.calc.node())
    }

    pub fn content(&self, viewer_name: &str) -> ExplainContent {
        ExplainContent::new(self.label.to_string(), self.calc.money(), self.calc.node().clone(), viewer_name, self.disclosure())
    }

    pub fn figure(&self, viewer_name: &str, emphasis: bool) -> Figure {
        Figure::new(self.id.clone(), self.label.clone(), self.calc.clone(), self.content(viewer_name)).emphasis(emphasis)
    }
}
