//! Decisions / Extraction timing: a bounded tax illustration — one entered
//! amount taken in one year or split evenly over two, on a stated bracket
//! schedule with a fixed baseline income. It says nothing about a company's
//! cash or its permission to pay anything out.

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    description_list::{DescriptionItem, DescriptionList},
    form::{Field, Form},
    h_flex,
    input::Input,
    radio::RadioGroup,
    stepper::{Stepper, StepperItem},
    tag::Tag,
    v_flex,
};
use gpui_kit::*;

use super::common::workspace_header;
use crate::app::AtlasApp;
use crate::models::taxes::{E05Schedule, TaxModel};
use crate::nav::{Destination, Route};
use crate::widgets::explain;
use crate::widgets::figure::card;
use crate::widgets::record::{self, Lane};
use crate::widgets::states::{fact, lanes, note, section};

const STEPS: [&str; 4] = ["Amount", "Timing", "Tax basis", "Result"];

pub fn render(app: &AtlasApp, model: &TaxModel, cx: &mut Context<AtlasApp>) -> AnyElement {
    let step = app.extraction_step.min(3);
    let header = workspace_header(
        Destination::Decisions,
        Route::Extraction,
        vec![Tag::secondary().small().outline().child("Illustration only").into_any_element()],
        cx,
    );
    let body: AnyElement = match step {
        0 => section("extraction-step-1", "How much to compare")
            .description("An amount you might take out of a company. This is an example input, not a claim that the money is available.")
            .child(Form::vertical().child(Field::new().label("Amount").required(true).child(Input::new(&app.tax_controls.e05_amount).id("e05-amount"))))
            .child(note("Fixed baseline: 60,000 taxable income in each of two years. This illustration does not test company cash or permission to withdraw.", cx))
            .into_any_element(),
        1 => section("extraction-step-2", "When it is taken")
            .description("All of it in year 1 is always compared. Add the even split to see what timing changes.")
            .child(Checkbox::new("e05-split").label("Also compare an even split over the two years").checked(model.e05_split).on_change(cx.listener(|this, v, _, cx| this.set_e05_split(*v, cx))))
            .child(DescriptionList::new().columns(1).child(DescriptionItem::new("Years").value("Year 1 and year 2 of the illustration")).child(DescriptionItem::new("Baseline income").value("60,000 taxable in each year (read-only)")))
            .into_any_element(),
        2 => section("extraction-step-3", "Which brackets")
            .description("The schedule the comparison uses. No jurisdiction is inferred.")
            .child(
                RadioGroup::vertical("e05-schedule")
                    .children(["Example brackets: 10% up to 100,000, 30% above", "This household's annual brackets (current year)"])
                    .selected_index(Some(match model.e05_schedule {
                        E05Schedule::PlanExample => 0,
                        E05Schedule::HouseholdPack => 1,
                    }))
                    .on_change(cx.listener(|this, index: &usize, _, cx| this.set_e05_schedule(if *index == 1 { E05Schedule::HouseholdPack } else { E05Schedule::PlanExample }, cx))),
            )
            .child(if model.e05_brackets.is_empty() {
                note("No annual bracket rule is effective for the household, so that schedule cannot be used; the example schedule still works.", cx).into_any_element()
            } else {
                div()
                    .text_sm()
                    .child(format!(
                        "Brackets: {}",
                        model
                            .e05_brackets
                            .iter()
                            .map(|b| match b.upper {
                                Some(u) => format!("{}% {}–{}", b.rate_basis_points / 100, b.lower.format(), u.format()),
                                None => format!("{}% above {}", b.rate_basis_points / 100, b.lower.format()),
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                    .into_any_element()
            })
            .into_any_element(),
        _ => render_result(model, cx),
    };
    let theme = cx.theme();

    v_flex()
        .id("screen-extraction")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(v_flex().gap_0p5().child(div().text_lg().font_weight(FontWeight::MEDIUM).child("Extraction timing illustration")).child(div().text_xs().text_color(theme.muted_foreground).child("Taxes only, on the stated assumptions. Not a withdrawal recommendation and not tax advice.")))
        .child(
            Stepper::new("extraction-stepper")
                .selected_index(step)
                .items(STEPS.iter().enumerate().map(|(i, t)| StepperItem::new().child(div().text_sm().child(format!("{}. {t}", i + 1)))))
                .on_click(cx.listener(|this, target: &usize, _, cx| {
                    if *target <= this.extraction_step {
                        this.extraction_step = *target;
                        cx.notify();
                    }
                })),
        )
        .child(body)
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .pt_3()
                .border_t_1()
                .border_color(theme.border)
                .child(Button::new("extraction-back").ghost().label("Back").disabled(step == 0).on_click(cx.listener(move |this, _, _, cx| {
                    this.extraction_step = step.saturating_sub(1);
                    cx.notify();
                })))
                .child(if step < 3 {
                    Button::new("extraction-next").primary().label(if step == 2 { "Compare taxes" } else { "Next" }).on_click(cx.listener(move |this, _, _, cx| {
                        this.extraction_step = step + 1;
                        cx.notify();
                    })).into_any_element()
                } else {
                    Button::new("extraction-change").outline().label("Change inputs").on_click(cx.listener(|this, _, _, cx| {
                        this.extraction_step = 0;
                        cx.notify();
                    })).into_any_element()
                }),
        )
        .into_any_element()
}

fn render_result(model: &TaxModel, cx: &mut Context<AtlasApp>) -> AnyElement {
    let years = model.e05_strategies.first().map(|s| s.taxable_by_year.len()).unwrap_or(2);
    let mut lanes_def: Vec<(&'static str, Lane)> = vec![("Strategy", Lane::fixed(220.))];
    let year_labels: [&str; 2] = ["Year 1 · taxable / tax", "Year 2 · taxable / tax"];
    for i in 0..years.min(2) {
        lanes_def.push((year_labels[i], Lane::fixed(220.)));
    }
    lanes_def.push(("Total tax", Lane::money(150.)));
    lanes_def.push(("Incremental", Lane::flex()));
    let rows: Vec<_> = model
        .e05_strategies
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut cells: Vec<(Lane, AnyElement)> = vec![(lanes_def[0].1, record::text(s.name.clone()))];
            for y in 0..years.min(2) {
                cells.push((lanes_def[1 + y].1, record::muted(format!("{} / {}", s.taxable_by_year.get(y).map(|m| m.format()).unwrap_or_default(), s.tax_by_year.get(y).map(|c| c.money().format()).unwrap_or_default()), cx)));
            }
            cells.push((lanes_def[lanes_def.len() - 2].1, record::money(s.total_tax, cx)));
            cells.push((lanes_def[lanes_def.len() - 1].1, record::money(s.incremental.money(), cx)));
            record::row(SharedString::from(format!("e05-strategy-{i}")), false, cells, |_, _, _| {})
        })
        .collect();
    let amount = model.e05_amount;
    let brackets = model
        .e05_brackets
        .iter()
        .map(|b| match b.upper {
            Some(u) => format!("{}% {}–{}", b.rate_basis_points / 100, b.lower.format(), u.format()),
            None => format!("{}% above {}", b.rate_basis_points / 100, b.lower.format()),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let source = match model.e05_schedule {
        E05Schedule::PlanExample => "Example schedule, fictitious".to_string(),
        E05Schedule::HouseholdPack => format!("This household's packs: {}", model.packs.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", ")),
    };
    let theme = cx.theme();
    section("extraction-result", format!("Taking {} out", amount.format()))
        .child(lanes([
            fact("Baseline tax over the two years", model.e05_baseline_total.format(), cx).into_any_element(),
            fact("Brackets", if brackets.is_empty() { "None effective".to_string() } else { brackets }, cx).into_any_element(),
            fact("Schedule source", source, cx).into_any_element(),
        ]))
        .child(if model.e05_strategies.is_empty() {
            note("No annual bracket rule is effective, so no comparison can be made. Choose the example schedule, or add an annual rule in Rules & taxes.", cx).into_any_element()
        } else {
            v_flex()
                .w_full()
                .gap_3()
                .child(record::list("e05-strategies", record::header(&lanes_def, cx), rows))
                .child(lanes(model.e05_incrementals.iter().map(|f| card(f.standard()).into_any_element())))
                .children(model.e05_incrementals.first().map(|f| explain::render_preview(f.calc.node(), f.content(), cx)))
                .into_any_element()
        })
        .child(note("Taxes only. The illustration does not test whether the company has the cash, whether a payment is lawful, or what next year's law will be. A concrete funding route belongs in a purchase.", cx))
        .child(
            h_flex()
                .gap_2()
                .child(Button::new("extraction-tax-packs").small().ghost().icon(IconName::Gavel).label("Tax packs").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::TaxPacks, cx))))
                .child(Button::new("extraction-test-purchase").small().outline().icon(IconName::Target).label("Test a purchase").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Purchase, cx))))
                .child(div().text_xs().text_color(theme.muted_foreground).child("The purchase builder evaluates a real route, its reserve test and its fees.")),
        )
        .into_any_element()
}
