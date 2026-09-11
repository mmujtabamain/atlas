//! A figure that can always explain itself.
//!
//! Every derived money value on a screen is rendered through [`Figure`]: the
//! label, the value in the monospace face, the vocabulary tags of its
//! provenance node and a "Why?" button that opens the explain sheet. There is
//! deliberately no way to render a bare derived number.

use std::rc::Rc;

use atlas_core::{Calc, Money};
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, button::{Button, ButtonVariants as _}, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::explain::{self, ExplainContent};
use super::labels;

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

        let mut tags = h_flex().gap_1().flex_wrap().items_center();
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
