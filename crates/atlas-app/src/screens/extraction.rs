//! Decisions / Extraction timing: a bounded tax illustration — one entered
//! amount taken in one year or split evenly over two, on a stated bracket
//! schedule with a fixed baseline income. It says nothing about a company's
//! cash or its permission to pay anything out.

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
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
use crate::widgets::facts::facts;
use crate::widgets::explain;
use crate::widgets::record::{self, Lane};
use crate::widgets::states::{action_bar, columns, hairline, info_card, note, section};

const STEPS: [&str; 4] = ["Amount", "Timing", "Tax basis", "Result"];

/// The bracket schedule as one line, the way the result and the tax-basis step
/// both state it.
fn bracket_line(model: &TaxModel) -> String {
    model
        .e05_brackets
        .iter()
        .map(|b| match b.upper {
            Some(u) => format!("{}% {}–{}", b.rate_basis_points / 100, b.lower.format(), u.format()),
            None => format!("{}% above {}", b.rate_basis_points / 100, b.lower.format()),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Where the schedule comes from — the plan's example, or the household's own
/// packs named.
fn schedule_source(model: &TaxModel) -> String {
    match model.e05_schedule {
        E05Schedule::PlanExample => "Example schedule, fictitious".to_string(),
        E05Schedule::HouseholdPack => format!("This household's packs: {}", model.packs.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", ")),
    }
}

/// The assumptions the calculator fixes, beside every step.
///
/// They do not change between steps and they are what bounds the answer, so
/// they stand next to the form rather than being restated under each step —
/// the same place the purchase builder keeps its running summary.
fn fixed_assumptions(cx: &mut Context<AtlasApp>) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .id("extraction-assumptions")
        .test_support()
        .w(px(300.))
        .flex_shrink_0()
        .gap_2()
        .p_4()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Fixed assumptions"))
        .child(div().text_xs().text_color(theme.muted_foreground).child("The calculator holds these. They are not read from the household and no step changes them."))
        .child(
            facts()
                .pair("Years", "Year 1 and year 2 of the illustration")
                .pair("Baseline income · each year", "60,000 taxable"),
        )
        .child(hairline(cx))
        .child(div().text_xs().text_color(theme.muted_foreground).child("Taxes only: no company cash, reserve cover or permission to withdraw is tested here."))
        .into_any_element()
}

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
            .child(
                Form::vertical()
                    .columns(2)
                    .child(Field::new().label("Amount").required(true).description("The same amount is compared across every timing below.").child(Input::new(&app.tax_controls.e05_amount).id("e05-amount"))),
            )
            .into_any_element(),
        1 => section("extraction-step-2", "When it is taken")
            .description("All of it in year 1 is always compared. Add the even split to see what timing changes.")
            .child(Checkbox::new("e05-split").label("Also compare an even split over the two years").checked(model.e05_split).on_change(cx.listener(|this, v, _, cx| this.set_e05_split(*v, cx))))
            .child(note("The two years and the baseline income beside this step are read-only: they are what makes the comparison bounded.", cx))
            .into_any_element(),
        2 => {
            let brackets = bracket_line(model);
            section("extraction-step-3", "Which brackets")
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
                    // What the chosen schedule actually is, as a standing fact
                    // of the step rather than a sentence trailing under it.
                    info_card("extraction-schedule", IconName::Percent, brackets, schedule_source(model), cx)
                })
                .into_any_element()
        }
        _ => render_result(model, cx),
    };
    let theme = cx.theme();
    let leading: Vec<AnyElement> = if step == 3 {
        vec![
            Button::new("extraction-tax-packs").small().ghost().icon(IconName::Gavel).label("Tax packs").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::TaxPacks, cx))).into_any_element(),
            Button::new("extraction-test-purchase").small().ghost().icon(IconName::Target).label("Test a purchase").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Purchase, cx))).into_any_element(),
            div().text_xs().text_color(theme.muted_foreground).child("The purchase builder evaluates a real route, its reserve test and its fees.").into_any_element(),
        ]
    } else {
        Vec::new()
    };

    v_flex()
        .id("screen-extraction")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(v_flex().w_full().gap_0p5().child(div().text_lg().font_weight(FontWeight::MEDIUM).child("Extraction timing illustration")).child(div().text_xs().text_color(theme.muted_foreground).child("Taxes only, on the stated assumptions. Not a withdrawal recommendation and not tax advice.")))
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
        // The step beside what bounds it: the assumptions panel is what turns
        // this from a tax answer into a bounded illustration, so it is on
        // screen at every step rather than restated under each one.
        .child(h_flex().w_full().gap_8().items_start().child(v_flex().flex_1().min_w_0().child(body)).child(fixed_assumptions(cx)))
        .child(action_bar(
            "extraction-actions",
            leading,
            vec![
                Button::new("extraction-back").ghost().label("Back").disabled(step == 0).on_click(cx.listener(move |this, _, _, cx| {
                    this.extraction_step = step.saturating_sub(1);
                    cx.notify();
                })).into_any_element(),
                if step < 3 {
                    Button::new("extraction-next")
                        .primary()
                        .label(if step == 2 { "Compare taxes" } else { "Next" })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.extraction_step = step + 1;
                            cx.notify();
                        }))
                        .into_any_element()
                } else {
                    Button::new("extraction-change")
                        .outline()
                        .label("Change inputs")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.extraction_step = 0;
                            cx.notify();
                        }))
                        .into_any_element()
                },
            ],
            cx,
        ))
        .into_any_element()
}

fn render_result(model: &TaxModel, cx: &mut Context<AtlasApp>) -> AnyElement {
    let years = model.e05_strategies.first().map(|s| s.taxable_by_year.len()).unwrap_or(2);
    let mut lanes_def: Vec<(&'static str, Lane)> = vec![("Timing", Lane::fixed(200.))];
    let year_labels: [&str; 2] = ["Year 1 · taxable / tax", "Year 2 · taxable / tax"];
    for i in 0..years.min(2) {
        lanes_def.push((year_labels[i], Lane::fixed(200.)));
    }
    lanes_def.push(("Total tax", Lane::money(140.)));
    lanes_def.push(("Incremental tax", Lane::flex()));
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
    let brackets = bracket_line(model);
    let theme = cx.theme();
    // What the steps chose, on one line above the figures it bounds — the
    // same shape the forecast states its case and horizon in. The two years
    // and the baseline income are beside it in the assumptions panel, so they
    // are not repeated here.
    let scope = h_flex()
        .w_full()
        .flex_wrap()
        .gap_5()
        .items_center()
        .text_xs()
        .text_color(theme.muted_foreground)
        .child(div().flex_shrink_0().child(format!("Amount {}", amount.format())))
        .child(div().flex_shrink_0().child(format!("Brackets {}", if brackets.is_empty() { "none effective".to_string() } else { brackets })))
        .child(div().flex_shrink_0().child(schedule_source(model)));
    // The baseline is the plain two-year tax the comparison starts from and
    // the incrementals are what each timing adds to it, so they read as one
    // even grid rather than a leading fact and a wrapping row of cards.
    let mut figures: Vec<AnyElement> = vec![
        v_flex()
            .w_full()
            .min_w_0()
            .gap_1()
            .child(div().w_full().text_xs().text_color(theme.muted_foreground).child("Baseline tax over the two years"))
            .child(div().w_full().font_family(theme.mono_font_family.clone()).text_xl().font_weight(FontWeight::SEMIBOLD).child(model.e05_baseline_total.format()))
            .child(div().w_full().text_xs().text_color(theme.muted_foreground).child("Tax on the baseline income alone, before anything is taken out."))
            .into_any_element(),
    ];
    figures.extend(model.e05_incrementals.iter().map(|f| f.standard().into_any_element()));
    section("extraction-result", format!("Taking {} out", amount.format()))
        .badge("Taxes only · two fixed years")
        .child(scope)
        .child(div().id("extraction-figures").test_support().w_full().child(columns(figures)))
        .child(if model.e05_strategies.is_empty() {
            note("No annual bracket rule is effective, so no comparison can be made. Choose the example schedule, or add an annual rule in Rules & taxes.", cx).into_any_element()
        } else {
            v_flex()
                .w_full()
                .gap_3()
                .child(record::list("e05-strategies", record::header(&lanes_def, cx), rows))
                // The equation behind the first incremental tax rather than
                // its chain as a table: the chain is what `Full calculation…`
                // opens, and each row's own figure opens its own.
                .children(model.e05_incrementals.first().map(|f| explain::render_equation("extraction-incremental-equation", f.calc.node(), f.content(), cx)))
                .into_any_element()
        })
        // What the winning timing does *not* establish is as true when the
        // numbers are good as when they are bad, so it is a standing fact and
        // not a warning.
        .child(info_card(
            "extraction-bounds",
            IconName::ShieldCheck,
            "A lower incremental tax is not a withdrawal route",
            "Whichever timing costs less tax here, the company may still lack the cash or the legal capacity to pay it out, and a future year's law is not inferred from this schedule. A concrete route, its reserve test and its fees belong in a purchase.",
            cx,
        ))
        .into_any_element()
}
