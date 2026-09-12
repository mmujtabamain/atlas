//! A figure that can always explain itself.
//!
//! Every derived money value on a screen is rendered through [`Figure`]: the
//! label, the exact value in the monospace face, a quiet metadata line with
//! the three vocabulary terms (each opens Figure meanings) and an `Explain…`
//! button that opens the calculation sheet. There is deliberately no way to
//! render a bare derived number.

use std::sync::Arc;

use atlas_core::authz::Viewer;
use atlas_core::model::Household;
use atlas_core::provenance::{Operation, ProvNode};
use atlas_core::{Calc, Disclosure, Money};
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, button::{Button, ButtonVariants as _}, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::explain::{self, ExplainContent};
use super::meanings::{self, Term};

/// One card in a row of figures (`h_flex().flex_wrap().gap_8()`).
///
/// Cards have a fixed width on purpose. With auto-width cards taffy has to
/// measure every card's whole content — label, value, tags — again for each
/// candidate wrap line, and that measurement repeats at every ancestor's
/// sizing pass. Long labels wrap inside the card.
pub fn card(content: impl IntoElement) -> Div {
    div().w_64().flex_shrink_0().child(content)
}

/// How much room the figure takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Variant {
    /// The one number a screen is about: large value, metadata on its own rows.
    Leading,
    /// A supporting figure: value with the metadata line below.
    Standard,
    /// Inside a table cell: small value, metadata stacked, icon-only Explain.
    Compact,
}

/// A labelled, explainable money figure.
#[derive(IntoElement)]
pub struct Figure {
    id: SharedString,
    label: SharedString,
    money: Money,
    node: Arc<ProvNode>,
    content: Arc<ExplainContent>,
    variant: Variant,
    /// A qualifier or date shown after the metadata, when needed.
    qualifier: Option<SharedString>,
}

impl Figure {
    /// `id` must be stable and unique on the screen (`free-cash`, …).
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>, calc: &Calc<Money>, content: Arc<ExplainContent>) -> Self {
        Figure { id: id.into(), label: label.into(), money: calc.money(), node: calc.shared_node(), content, variant: Variant::Standard, qualifier: None }
    }

    pub fn variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }

    /// Larger value text for the one figure a screen is about.
    pub fn emphasis(self, emphasis: bool) -> Self {
        self.variant(if emphasis { Variant::Leading } else { Variant::Standard })
    }

    pub fn qualifier(mut self, qualifier: impl Into<SharedString>) -> Self {
        self.qualifier = Some(qualifier.into());
        self
    }
}

/// The metadata terms of a node, as buttons that open Figure meanings.
pub fn metadata_terms(id: &str, node: &ProvNode) -> Vec<AnyElement> {
    let mut terms: Vec<AnyElement> = Vec::new();
    match node.money_class_label() {
        Some(class) => terms.push(meanings::term_button(SharedString::from(format!("{id}-class")), class.label(), !class.is_current(), Term::MoneyClass(class)).into_any_element()),
        // The source gives no class for this figure; say so rather than
        // leaving the line short or inventing one.
        None => terms.push(meanings::term_button(SharedString::from(format!("{id}-class")), "Kind not assigned", true, Term::MoneyClass(atlas_core::vocab::MoneyClass::ConditionalFuture)).into_any_element()),
    }
    if let Some(certainty) = node.certainty_label() {
        let caution = matches!(certainty, atlas_core::vocab::Certainty::ScenarioOnly | atlas_core::vocab::Certainty::Tentative);
        terms.push(meanings::term_button(SharedString::from(format!("{id}-certainty")), certainty.label(), caution, Term::Certainty(certainty)).into_any_element());
    }
    let strength = node.result_strength();
    let caution = !matches!(strength, atlas_core::vocab::ResultStrength::ExactAccounting | atlas_core::vocab::ResultStrength::SolverCertified);
    terms.push(meanings::term_button(SharedString::from(format!("{id}-strength")), strength.label(), caution, Term::Strength(strength)).into_any_element());
    if matches!(node.operation(), Operation::Aggregate { .. }) {
        terms.push(meanings::term_button(SharedString::from(format!("{id}-aggregate")), "Authorized total", true, Term::Disclosure(Disclosure::Aggregate)).into_any_element());
    }
    terms
}

impl RenderOnce for Figure {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let money = self.money;
        let node: &ProvNode = &self.node;
        let value_color = if money.is_negative() { theme.danger } else { theme.foreground };
        let value_id = SharedString::from(format!("figure-{}", self.id));
        let explain_id = SharedString::from(format!("explain-{}", self.id));
        // The click handler shares the content with the figure (pointer copy).
        let content = self.content;
        let compact = self.variant == Variant::Compact;
        if compact {
            // Inside a table cell: the value with its Explain icon, then the
            // terms as one quiet line (the column header carries the label).
            let mut terms: Vec<&'static str> = Vec::new();
            if let Some(class) = node.money_class_label() {
                terms.push(class.label());
            }
            if let Some(certainty) = node.certainty_label() {
                terms.push(certainty.label());
            }
            terms.push(node.result_strength().label());
            let first_term = node.money_class_label().map(Term::MoneyClass).unwrap_or(Term::Strength(node.result_strength()));
            let caution = node.money_class_label().is_some_and(|c| !c.is_current()) || matches!(node.operation(), Operation::Aggregate { .. });
            let terms_id = SharedString::from(format!("{}-terms", self.id));
            return v_flex()
                .min_w_0()
                .items_end()
                .child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .child(div().id(value_id).test_support().font_family(theme.mono_font_family.clone()).text_sm().text_color(value_color).child(money.format()))
                        .child(
                            Button::new(explain_id)
                                .xsmall()
                                .ghost()
                                .compact()
                                .icon(IconName::ListTree)
                                .tooltip("Show the calculation")
                                .on_click(move |_, window, cx| explain::open_sheet(window, cx, content.clone())),
                        ),
                )
                .child(
                    Button::new(terms_id)
                        .xsmall()
                        .ghost()
                        .compact()
                        .label(terms.join(" · "))
                        .when(caution, |b| b.warning().outline())
                        .tooltip("What these labels mean")
                        .on_click(move |_, window, cx| meanings::open_sheet(window, cx, Some(first_term))),
                )
                .into_any_element();
        }
        let terms = metadata_terms(&self.id, node);

        let explain = Button::new(explain_id)
            .xsmall()
            .ghost()
            .compact()
            .icon(IconName::ListTree)
            .label("Explain…")
            .tooltip("Show the calculation")
            .on_click(move |_, window, cx| explain::open_sheet(window, cx, content.clone()));

        // Definite width: an auto-width row of tags is re-measured by taffy at
        // every ancestor pass.
        let metadata = h_flex().w_full().gap_1().flex_wrap().items_center().children(terms).into_any_element();

        v_flex()
            .w_full()
            .gap_1()
            .min_w_0()
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_baseline()
                    .gap_2()
                    .child(div().flex_1().min_w_0().text_xs().text_color(theme.muted_foreground).whitespace_normal().child(self.label))
                    .child(div().flex_shrink_0().child(explain)),
            )
            .child(
                div()
                    .id(value_id)
                    .test_support()
                    .font_family(theme.mono_font_family.clone())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(value_color)
                    .map(|this| match self.variant {
                        Variant::Leading => this.text_2xl(),
                        Variant::Standard | Variant::Compact => this.text_xl(),
                    })
                    .child(money.format()),
            )
            .child(metadata)
            .when_some(self.qualifier, |this, q| this.child(div().text_xs().text_color(theme.muted_foreground).child(q)))
            .into_any_element()
    }
}

/// A projected, explainable figure: the model half of a [`Figure`], computed
/// once per state change and rendered by any screen.
///
/// Everything a frame needs is prepared here, once: the projected chain, its
/// weakest disclosure level and the calculation-sheet content. Rendering it
/// (see [`ExplainedFigure::figure`]) copies two `Arc` pointers, so a screen
/// with twenty figures costs the same per frame as one with none.
#[derive(Clone, Debug)]
pub struct ExplainedFigure {
    pub id: SharedString,
    pub label: SharedString,
    /// Node already projected for the viewer.
    pub calc: Calc<Money>,
    disclosure: Disclosure,
    content: Arc<ExplainContent>,
}

impl ExplainedFigure {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>, calc: &Calc<Money>, household: &Household, viewer: Viewer) -> Self {
        let id = id.into();
        let label = label.into();
        let projected = calc.node().project(&household.disclosure_fn(viewer));
        let calc = Calc::new(calc.money(), projected);
        let disclosure = weakest_disclosure(calc.node());
        let viewer_name = household.entity_name(atlas_core::ids::EntityRef::Person(viewer.person));
        let content = Arc::new(ExplainContent::new(label.to_string(), calc.money(), calc.shared_node(), viewer_name, disclosure));
        ExplainedFigure { id, label, calc, disclosure, content }
    }

    /// The weakest disclosure level present in the projected chain.
    pub fn disclosure(&self) -> Disclosure {
        self.disclosure
    }

    /// The calculation-sheet content, shared (the chain is not copied).
    pub fn content(&self) -> Arc<ExplainContent> {
        Arc::clone(&self.content)
    }

    pub fn figure(&self, emphasis: bool) -> Figure {
        Figure::new(self.id.clone(), self.label.clone(), &self.calc, self.content()).emphasis(emphasis)
    }

    pub fn leading(&self) -> Figure {
        self.figure(true)
    }

    pub fn standard(&self) -> Figure {
        self.figure(false)
    }

    pub fn compact(&self) -> Figure {
        Figure::new(self.id.clone(), self.label.clone(), &self.calc, self.content()).variant(Variant::Compact)
    }

    /// The same figure under another label (a table column already names it).
    pub fn compact_labelled(&self, label: impl Into<SharedString>) -> Figure {
        Figure::new(self.id.clone(), label, &self.calc, self.content()).variant(Variant::Compact)
    }

    pub fn money(&self) -> Money {
        self.calc.money()
    }
}

/// The weakest disclosure level anywhere in a projected chain.
fn weakest_disclosure(node: &ProvNode) -> Disclosure {
    let own = match node.operation() {
        Operation::Aggregate { restricted_terms: 0 } => Disclosure::Hidden,
        Operation::Aggregate { .. } => Disclosure::Aggregate,
        _ => Disclosure::Full,
    };
    node.children().iter().map(weakest_disclosure).fold(own, Disclosure::min)
}
