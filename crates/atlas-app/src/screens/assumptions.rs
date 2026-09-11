//! Assumptions (§2.5, §10, §32.2): the register with kinds, sources,
//! acceptance and freshness; deterministic derivation from history with the
//! formula and sample disclosed; the vocabulary legends every value is
//! tagged with; and one-at-a-time sensitivity with the joint caveat.

use atlas_core::assumptions::{ConditionalStatement, DerivedAssumption, derive, Derivation};
use atlas_core::authz::Viewer;
use atlas_core::forecast::{Case, ForecastOptions};
use atlas_core::ids::{EntityRef, ObjectRef, SeriesId};
use atlas_core::liquidity::Boundary;
use atlas_core::model::{Freshness, Household};
use atlas_core::sensitivity::{SensitivityReport, one_at_a_time};
use atlas_core::vocab::{Certainty, MoneyClass, ResultStrength};
use atlas_core::{Disclosure, EngineError, EngineResult};
use chrono::NaiveDate;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    alert::Alert,
    button::Button,
    checkbox::Checkbox,
    group_box::GroupBox, h_flex,
    tab::{Tab, TabBar},
    tag::Tag, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::AtlasApp;
use crate::widgets::figure::ExplainedFigure;
use crate::widgets::labels;
use crate::widgets::master::page_header;

#[derive(Clone, Debug)]
pub struct AssumptionsModel {
    pub horizon: NaiveDate,
    /// Series with reconciled history the derivation panel can use.
    pub derivable: Vec<SeriesId>,
    pub derivation_series: SeriesId,
    pub derivation: Derivation,
    pub derived: Result<DerivedAssumption, EngineError>,
    pub derived_figure: Option<ExplainedFigure>,
    pub sensitivity_boundaries: Vec<Boundary>,
    pub sensitivity_boundary: Boundary,
    pub sensitivity_scenario: bool,
    pub sensitivity: SensitivityReport,
    pub statement: ConditionalStatement,
}

impl AssumptionsModel {
    pub fn compute(
        household: &Household,
        viewer: Viewer,
        derivation_series: Option<SeriesId>,
        derivation: Derivation,
        sensitivity_boundary: Boundary,
        sensitivity_scenario: bool,
        horizon: NaiveDate,
    ) -> EngineResult<Self> {
        log::info!("computing assumptions model: derivation {:?} boundary {:?}", derivation, sensitivity_boundary);
        let mut derivable: Vec<SeriesId> = household
            .series
            .iter()
            .filter(|s| !household.history_of(s.id).is_empty())
            .filter(|s| !matches!(household.disclosure_for(viewer, ObjectRef::Series(s.id)), Disclosure::Hidden | Disclosure::Aggregate))
            .map(|s| s.id)
            .collect();
        derivable.sort_by_key(|id| id.raw());
        let derivation_series = derivation_series.filter(|id| derivable.contains(id)).or_else(|| derivable.first().copied()).unwrap_or(SeriesId::new(0));
        let derived = derive(household, derivation_series, derivation);
        let derived_figure = derived.as_ref().ok().map(|d| {
            ExplainedFigure::new(format!("derived-{}", derivation_series.raw()), "Derived expected amount", &d.calc, household, viewer)
        });

        let mut sensitivity_boundaries = vec![Boundary::Household];
        sensitivity_boundaries.extend(
            household
                .accounts
                .iter()
                .filter(|a| a.kind.is_cash() && !a.is_company_account())
                .filter(|a| matches!(household.disclosure_for(viewer, ObjectRef::Account(a.id)), Disclosure::Full | Disclosure::SelectedFields))
                .map(|a| Boundary::Account(a.id)),
        );
        let sensitivity_boundary = if sensitivity_boundaries.contains(&sensitivity_boundary) { sensitivity_boundary } else { Boundary::Household };
        let options = ForecastOptions {
            through: horizon,
            scenario: if sensitivity_scenario { Some(atlas_core::fixtures::ids::BUY_CAR) } else { None },
            case: Case::Expected,
        };
        let sensitivity = one_at_a_time(household, sensitivity_boundary, options)?;
        let forecast = atlas_core::forecast::forecast(household, sensitivity_boundary, options)?;
        let statement = ConditionalStatement {
            claim: if sensitivity.baseline_breaches {
                format!(
                    "The {} path falls below its {} floor (lowest {})",
                    sensitivity_boundary.label(household),
                    sensitivity.floor.format(),
                    sensitivity.baseline_lowest.format()
                )
            } else {
                format!(
                    "The {} path stays above its {} floor (lowest {})",
                    sensitivity_boundary.label(household),
                    sensitivity.floor.format(),
                    sensitivity.baseline_lowest.format()
                )
            },
            horizon,
            coverage: ResultStrength::ConditionalPath,
            assumptions: ConditionalStatement::assumption_texts(&forecast.assumptions.iter().filter(|a| a.private_to.is_none_or(|p| p == viewer.person)).cloned().collect::<Vec<_>>()),
            excluded_shocks: vec![
                "unplanned expenses beyond the one-off limit below".into(),
                "several assumptions failing together (joint stress arrives with M14)".into(),
                "tax and rule changes (M6, M7)".into(),
            ],
        };
        Ok(AssumptionsModel {
            horizon,
            derivable,
            derivation_series,
            derivation,
            derived,
            derived_figure,
            sensitivity_boundaries,
            sensitivity_boundary,
            sensitivity_scenario,
            sensitivity,
            statement,
        })
    }
}

fn freshness_tag(freshness: Freshness) -> Tag {
    match freshness {
        Freshness::Fresh => Tag::secondary().xsmall().outline().child(freshness.label()),
        Freshness::NotAccepted | Freshness::Stale => Tag::warning().xsmall().outline().child(freshness.label()),
        Freshness::Expired => Tag::danger().xsmall().outline().child(freshness.label()),
    }
}

pub fn render(model: &AssumptionsModel, household: &Household, viewer: Viewer, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let mono = cx.theme().mono_font_family.clone();
    let muted = cx.theme().muted_foreground;
    let viewer_name = household.entity_name(EntityRef::Person(viewer.person));
    let visible: Vec<_> = household.assumptions.iter().filter(|a| a.private_to.is_none_or(|p| p == viewer.person)).collect();

    v_flex()
        .id("screen-assumptions")
        .test_support()
        .gap_6()
        .child(page_header(
            "Assumptions",
            format!(
                "{} assumptions visible to {} · the application never invents one: each is entered, derived by a disclosed formula, imported from a rule pack or created by a scenario (§2.5)",
                visible.len(),
                viewer_name
            ),
            cx,
        ))
        .child(render_register(model, household, &visible, cx))
        .child(render_derivation(model, household, &viewer_name, cx))
        .child(render_sensitivity(model, household, cx))
        .child(
            GroupBox::new().id("conditional-statement").title("How this conclusion is phrased (§10.1, V031)").child(
                v_flex()
                    .gap_2()
                    .child(div().id("conditional-statement-text").test_support().text_sm().font_family(mono).whitespace_normal().child(model.statement.render()))
                    .child(div().text_xs().text_color(muted).child("Never “you can afford it”; always the claim, the horizon, the coverage label, the assumptions and what was left out.")),
            ),
        )
        .child(render_legends(cx))
}

fn render_register(model: &AssumptionsModel, household: &Household, visible: &[&atlas_core::model::Assumption], cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let _ = model;
    GroupBox::new().id("assumption-register").title("Register (§5.9, §10.3, F117)").child(
        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_4()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(div().w_6().flex_shrink_0().child("#"))
                    .child(div().flex_1().min_w_0().child("Assumption · source (§2.5) · applies to"))
                    .child(div().w_40().flex_shrink_0().child("Kind (§10.3)"))
                    .child(div().w_48().flex_shrink_0().child("Accepted · expires"))
                    .child(div().w_32().flex_shrink_0().child("Freshness"))
                    .child(div().w_20().flex_shrink_0()),
            )
            .children(visible.iter().enumerate().map(|(index, a)| {
                let id = a.id;
                let applies: Vec<String> = a.applies_to.iter().filter_map(|s| household.series_by_id(*s)).map(|s| s.name.clone()).collect();
                let freshness = a.freshness(household.as_of);
                h_flex()
                    .id(SharedString::from(format!("assumption-row-{}", id.raw())))
                    .gap_4()
                    .items_start()
                    .px_2()
                    .py_2()
                    .rounded(theme.radius)
                    .when(index % 2 == 1, |row| row.bg(theme.table_even))
                    .child(div().w_6().flex_shrink_0().text_sm().text_color(theme.muted_foreground).child((index + 1).to_string()))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(div().text_sm().child(a.text.clone()))
                            .child(div().text_xs().text_color(theme.muted_foreground).child(a.source.describe()))
                            .child(div().text_xs().text_color(theme.muted_foreground).child(format!(
                                "Applies to: {}",
                                if applies.is_empty() { "every forecast".to_string() } else { applies.join(", ") }
                            ))),
                    )
                    .child(div().w_40().flex_shrink_0().child(h_flex().child(labels::certainty_tag(a.certainty))))
                    .child(
                        v_flex()
                            .w_48()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("accepted {}", a.accepted_on.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "—".into())))
                            .child(format!("expires {}", a.expires_on.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "never".into()))),
                    )
                    .child(div().w_32().flex_shrink_0().child(h_flex().child(freshness_tag(freshness))))
                    .child(div().w_20().flex_shrink_0().child(if freshness == Freshness::Fresh {
                        div().into_any_element()
                    } else {
                        Button::new(SharedString::from(format!("accept-assumption-{}", id.raw())))
                            .xsmall()
                            .outline()
                            .label("Accept")
                            .tooltip("Record your acceptance today (§2.5)")
                            .on_click(cx.listener(move |this, _, window, cx| this.accept_assumption(id, window, cx)))
                            .into_any_element()
                    }))
            })),
    )
}

fn render_derivation(model: &AssumptionsModel, household: &Household, viewer_name: &str, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let series_index = model.derivable.iter().position(|s| *s == model.derivation_series).unwrap_or(0);
    let derivation_index = Derivation::ALL.iter().position(|d| *d == model.derivation).unwrap_or(0);
    let derivable = model.derivable.clone();
    GroupBox::new().id("derivation").title("Derive an assumption from reconciled history (§2.5, §10.7)").child(
        v_flex()
            .gap_4()
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_6()
                    .items_end()
                    .child(
                        v_flex().gap_1().child(div().text_xs().text_color(theme.muted_foreground).child("Series with history")).child(
                            TabBar::new("derivation-series")
                                .selected_index(series_index)
                                .on_click(cx.listener(move |this, index: &usize, _, cx| {
                                    if let Some(series) = derivable.get(*index) {
                                        this.select_derivation_series(*series, cx);
                                    }
                                }))
                                .children(model.derivable.iter().map(|s| Tab::new().label(household.series_by_id(*s).map(|x| x.name.clone()).unwrap_or_default()))),
                        ),
                    )
                    .child(
                        v_flex().gap_1().child(div().text_xs().text_color(theme.muted_foreground).child("Formula")).child(
                            TabBar::new("derivation-formula")
                                .selected_index(derivation_index)
                                .on_click(cx.listener(|this, index: &usize, _, cx| {
                                    if let Some(derivation) = Derivation::ALL.get(*index) {
                                        this.select_derivation(*derivation, cx);
                                    }
                                }))
                                .children(Derivation::ALL.iter().map(|d| Tab::new().label(d.label()))),
                        ),
                    ),
            )
            .child(match (&model.derived, &model.derived_figure) {
                (Ok(derived), Some(figure)) => {
                    let series = model.derivation_series;
                    let target = household.assumptions.iter().find(|a| a.applies_to.contains(&series)).map(|a| a.id);
                    v_flex()
                        .gap_3()
                        .child(
                            h_flex()
                                .flex_wrap()
                                .gap_8()
                                .items_start()
                                .child(div().min_w_64().child(figure.figure(viewer_name, true)))
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .gap_1()
                                        .child(div().text_sm().child(derived.statement.clone()))
                                        .child(div().text_xs().text_color(theme.muted_foreground).child(format!(
                                            "Range/amount: {} · sample {} payments {} – {}",
                                            derived.amount.describe(),
                                            derived.sample_size,
                                            derived.sample_from.format("%d %b %Y"),
                                            derived.sample_to.format("%d %b %Y")
                                        ))),
                                ),
                        )
                        .child(
                            h_flex().gap_3().items_center().child(match target {
                                Some(id) => Button::new("apply-derivation")
                                    .small()
                                    .outline()
                                    .label(format!("Apply to assumption #{} (then accept it)", id.raw()))
                                    .on_click(cx.listener(move |this, _, window, cx| this.apply_derivation(id, window, cx)))
                                    .into_any_element(),
                                None => div().text_xs().text_color(theme.muted_foreground).child("No assumption applies to this series yet.").into_any_element(),
                            }),
                        )
                        .into_any_element()
                }
                (Err(err), _) => Alert::warning("derivation-error", err.to_string()).title("Not enough history").into_any_element(),
                _ => div().into_any_element(),
            }),
    )
}

fn render_sensitivity(model: &AssumptionsModel, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let report = &model.sensitivity;
    let boundary_index = model.sensitivity_boundaries.iter().position(|b| *b == model.sensitivity_boundary).unwrap_or(0);
    let boundaries = model.sensitivity_boundaries.clone();
    GroupBox::new().id("sensitivity").title("Which assumptions would have to fail? One-at-a-time breakpoints (§10.8)").child(
        v_flex()
            .gap_4()
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_6()
                    .items_end()
                    .child(
                        v_flex().gap_1().child(div().text_xs().text_color(theme.muted_foreground).child("Path")).child(
                            TabBar::new("sensitivity-boundaries")
                                .selected_index(boundary_index)
                                .on_click(cx.listener(move |this, index: &usize, _, cx| {
                                    if let Some(boundary) = boundaries.get(*index) {
                                        this.select_sensitivity_boundary(*boundary, cx);
                                    }
                                }))
                                .children(model.sensitivity_boundaries.iter().map(|b| Tab::new().label(b.label(household)))),
                        ),
                    )
                    .child(
                        Checkbox::new("sensitivity-buy-car")
                            .label("With scenario “Buy car”")
                            .checked(model.sensitivity_scenario)
                            .on_change(cx.listener(|this, checked, _, cx| this.set_sensitivity_scenario(*checked, cx))),
                    ),
            )
            .child(div().id("sensitivity-summary").test_support().text_sm().child(format!(
                "Floor {} · lowest on the expected path {} · {} · the path absorbs a one-off unplanned expense of at most {} on its worst day.",
                report.floor.format(),
                report.baseline_lowest.format(),
                if report.baseline_breaches { "already breached at the expected values" } else { "floor held at the expected values" },
                report.unplanned_spending_limit.format()
            )))
            .child(
                v_flex().gap_2().children(report.breakpoints.iter().map(|b| {
                    h_flex()
                        .gap_3()
                        .items_start()
                        .child(div().w_4().flex_shrink_0().text_color(theme.muted_foreground).child("•"))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(div().text_sm().child(b.statement.clone()))
                                .child(div().text_xs().text_color(theme.muted_foreground).child(format!("searched {} · everything else held at its expected value", b.searched))),
                        )
                })),
            )
            .child(Alert::warning("joint-caveat", report.caveat).title("Single-assumption limits only"))
            .child(
                h_flex().gap_2().items_center().child(labels::strength_tag(report.coverage)).child(div().text_xs().text_color(theme.muted_foreground).child(
                    "Bisection is exact to the minor unit and valid because the lowest balance is monotone in one amount or arrival date in this additive cash model; threshold taxes and rules (M6, M7) will require enumeration and say so.",
                )),
            ),
    )
}

fn render_legends(cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    GroupBox::new().id("legends").title("What the tags on every figure mean (§2.4, §10.3, §32.2)").child(
        v_flex()
            .gap_6()
            .child(
                v_flex()
                    .gap_2()
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Result strength — permitted claim and what it does not establish"))
                    .children(ResultStrength::ALL.iter().enumerate().map(|(index, s)| {
                        h_flex()
                            .gap_4()
                            .items_start()
                            .px_2()
                            .py_2()
                            .rounded(theme.radius)
                            .when(index % 2 == 1, |row| row.bg(theme.table_even))
                            .child(div().w_64().flex_shrink_0().child(h_flex().child(labels::strength_tag(*s))))
                            .child(div().flex_1().min_w_0().text_xs().child(s.permitted_claim()))
                            .child(div().flex_1().min_w_0().text_xs().text_color(theme.muted_foreground).child(format!("Does not establish: {}", s.does_not_establish())))
                    })),
            )
            .child(
                h_flex()
                    .gap_8()
                    .items_start()
                    .flex_wrap()
                    .child(
                        v_flex().gap_2().min_w_80().flex_1().child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Money classes (§2.4)")).children(MoneyClass::ALL.iter().map(|c| {
                            h_flex().gap_2().items_start().child(div().w_40().flex_shrink_0().child(h_flex().child(labels::money_class_tag(*c)))).child(div().flex_1().min_w_0().text_xs().text_color(theme.muted_foreground).child(c.description()))
                        })),
                    )
                    .child(
                        v_flex().gap_2().min_w_80().flex_1().child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Certainty (§10.3)")).children(Certainty::ALL.iter().map(|c| {
                            h_flex().gap_2().items_start().child(div().w_40().flex_shrink_0().child(h_flex().child(labels::certainty_tag(*c)))).child(div().flex_1().min_w_0().text_xs().text_color(theme.muted_foreground).child(c.description()))
                        })),
                    ),
            ),
    )
}
