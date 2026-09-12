//! Figure meanings — the reference sheet that explains every tag a figure
//! can carry. Reachable from any tag, from the calculation sheet and from the
//! sidebar footer; it opens on the family and term that was clicked.

use atlas_core::model::Hardness;
use atlas_core::vocab::{Certainty, MoneyClass, ResultStrength};
use atlas_core::Disclosure;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, WindowExt as _,
    accordion::Accordion,
    description_list::{DescriptionItem, DescriptionList},
    h_flex,
    tab::{Tab, TabBar},
    v_flex,
};
use gpui_kit::*;

/// A term the sheet can open on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Term {
    MoneyClass(MoneyClass),
    Certainty(Certainty),
    Strength(ResultStrength),
    Disclosure(Disclosure),
    Hardness(Hardness),
    /// The "Other labels" family with nothing focused.
    Other,
}

impl Term {
    fn family(self) -> usize {
        match self {
            Term::MoneyClass(_) => 0,
            Term::Certainty(_) => 1,
            Term::Strength(_) => 2,
            Term::Disclosure(_) | Term::Hardness(_) | Term::Other => 3,
        }
    }
}

/// The sheet's own state: the family tab and the term to expand.
pub struct MeaningsState {
    family: usize,
    focused: Option<Term>,
}

/// Opens the reference sheet, on `term` when given.
pub fn open_sheet(window: &mut Window, cx: &mut App, term: Option<Term>) {
    log::info!("figure meanings opened on {term:?}");
    let state = cx.new(|_| MeaningsState { family: term.map(Term::family).unwrap_or(0), focused: term });
    window.open_sheet(cx, move |sheet, _window, cx| {
        let state = state.clone();
        sheet.title("Figure meanings").size(relative(0.5)).child(render(state, cx))
    });
}

const FAMILIES: [&str; 4] = ["Money classes", "Certainty", "Result strength", "Other labels"];

fn render(state: Entity<MeaningsState>, cx: &mut App) -> impl IntoElement {
    let (family, focused) = {
        let s = state.read(cx);
        (s.family, s.focused)
    };
    let theme = cx.theme();
    let tabs = state.clone();
    v_flex()
        .id("figure-meanings")
        .py_4()
        .gap_4()
        .text_sm()
        .child(div().text_xs().text_color(theme.muted_foreground).child(
            "Every derived money figure carries three labels: what kind of money it is, how sure the assumption behind it is, and what the calculation may claim. Tags never change a figure; they say how to read it.",
        ))
        .child(
            TabBar::new("meanings-families")
                .selected_index(family)
                .on_click(move |index: &usize, _, cx| tabs.update(cx, |s, cx| { s.family = *index; s.focused = None; cx.notify(); }))
                .children(FAMILIES.iter().map(|f| Tab::new().label(*f))),
        )
        .child(match family {
            0 => render_money_classes(focused, cx),
            1 => render_certainties(focused, cx),
            2 => render_strengths(focused, cx),
            _ => render_other(focused, cx),
        })
}

fn render_money_classes(focused: Option<Term>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let mut accordion = Accordion::new("meanings-money-classes").multiple(true).bordered(true);
    for class in MoneyClass::ALL {
        let open = matches!(focused, Some(Term::MoneyClass(c)) if c == class);
        accordion = accordion.item(move |item| {
            item.title(class.label()).open(open).child(
                v_flex().gap_1().child(div().text_sm().child(class.description())).child(div().text_xs().text_color(theme.muted_foreground).child(if class.is_current() {
                    "Money present on the reconciliation date."
                } else {
                    "Not money in hand: a future class never means available cash."
                })),
            )
        });
    }
    v_flex()
        .gap_2()
        .child(accordion)
        .child(div().text_xs().text_color(theme.muted_foreground).child(
            "A future tax liability or reserve requirement needs its dates and explanation; its tag alone never establishes that an account balance has been reserved.",
        ))
        .into_any_element()
}

fn render_certainties(focused: Option<Term>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let mut accordion = Accordion::new("meanings-certainty").multiple(true).bordered(true);
    for certainty in Certainty::ALL {
        let open = matches!(focused, Some(Term::Certainty(c)) if c == certainty);
        accordion = accordion.item(move |item| item.title(certainty.label()).open(open).child(div().text_sm().child(certainty.description())));
    }
    v_flex()
        .gap_2()
        .child(accordion)
        .child(div().text_xs().text_color(theme.muted_foreground).child("No certainty tag is a percentage or a probability."))
        .into_any_element()
}

fn render_strengths(focused: Option<Term>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let mut accordion = Accordion::new("meanings-strength").multiple(true).bordered(true);
    for strength in ResultStrength::ALL {
        let open = matches!(focused, Some(Term::Strength(s)) if s == strength);
        accordion = accordion.item(move |item| {
            item.title(strength.label()).open(open).child(
                DescriptionList::new()
                    .columns(1)
                    .child(DescriptionItem::new("May claim").value(strength.permitted_claim()))
                    .child(DescriptionItem::new("Does not establish").value(strength.does_not_establish())),
            )
        });
    }
    v_flex()
        .gap_2()
        .child(accordion)
        .child(div().text_xs().text_color(theme.muted_foreground).child(
            "These are definitions, not a promise that every method exists in this application. A result never gets a stronger label than the calculation that produced it.",
        ))
        .into_any_element()
}

fn render_other(focused: Option<Term>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let disclosure_open = matches!(focused, Some(Term::Disclosure(_)));
    let hardness_open = matches!(focused, Some(Term::Hardness(_)));
    let accordion = Accordion::new("meanings-other")
        .multiple(true)
        .bordered(true)
        .item(move |item| {
            item.title("Disclosure").open(disclosure_open).child(
                DescriptionList::new()
                    .columns(1)
                    .child(DescriptionItem::new("Full details").value("All authorized detail and provenance."))
                    .child(DescriptionItem::new("Selected fields").value("Existence, balance and transactions; not a promise of every derivation."))
                    .child(DescriptionItem::new("Balance only").value("The recorded balance without transactions or derived figures."))
                    .child(DescriptionItem::new("Aggregate only").value("An authorized combined contribution, never an individual row."))
                    .child(DescriptionItem::new("Hidden").value("No individual existence is disclosed; a missing policy fails closed.")),
            )
        })
        .item(move |item| {
            item.title("Earmarks").open(hardness_open).child(
                DescriptionList::new()
                    .columns(1)
                    .child(DescriptionItem::new("Hard constraint").value("Contributes to the hard floor."))
                    .child(DescriptionItem::new("User-relaxable preference").value("Reserves money but is not a hard floor."))
                    .child(DescriptionItem::new("Separate amount").value("Adds to every other earmark on the account."))
                    .child(DescriptionItem::new("Includes the bank minimum").value("Avoids counting the bank minimum twice."))
                    .child(DescriptionItem::new("Inside another earmark").value("Adds nothing on top of the earmark it sits in.")),
            )
        })
        .item(|item| {
            item.title("Assumption freshness").child(
                DescriptionList::new()
                    .columns(1)
                    .child(DescriptionItem::new("Fresh").value("Accepted within 90 days of the reconciliation date."))
                    .child(DescriptionItem::new("Stale").value("Accepted more than 90 days before the reconciliation date."))
                    .child(DescriptionItem::new("Not accepted").value("No acceptance date."))
                    .child(DescriptionItem::new("Expired").value("Past its expiry date, even if accepted recently.")),
            )
        })
        .item(|item| {
            item.title("Occurrence status").child(
                DescriptionList::new()
                    .columns(1)
                    .child(DescriptionItem::new("Planned · Due today · Overdue").value("Live movements that still post their remaining amount."))
                    .child(DescriptionItem::new("Partially fulfilled").value("Some of it was received or paid; the remainder stays planned."))
                    .child(DescriptionItem::new("Fulfilled · Skipped · Cancelled").value("Post nothing."))
            )
        })
        .item(|item| {
            item.title("Visibility and calculation use").child(
                DescriptionList::new()
                    .columns(1)
                    .child(DescriptionItem::new("Private · Shared summary · Shared balance · Fully shared").value("Presets for who may learn an object exists and what of it they see; Custom when the aspects match no preset."))
                    .child(DescriptionItem::new("Excluded · May contribute under restricted disclosure · Fully available").value("Whether an object counts in calculations — separate from what is shown."))
                    .child(DescriptionItem::new("Purpose").value("A grant for one purpose (household forecasts, one scenario, decisions, funding searches, tax, extraction) widens nothing else.")),
            )
        })
        .item(|item| {
            item.title("Forecast and decision").child(
                DescriptionList::new()
                    .columns(1)
                    .child(DescriptionItem::new("Floor held · Below floor from a date · Negative from a date").value("An account can stay positive and still breach its floor."))
                    .child(DescriptionItem::new("Keeps / breaches the reserve").value("The purchase verdict, always in the Conservative case."))
                    .child(DescriptionItem::new("Feasible · Infeasible · Preferred").value("A funding strategy's status under the chosen objective; best among the tested ones, never a global optimum."))
                    .child(DescriptionItem::new("Verified · Unverified").value("Whether a tax pack cites an official source. It is never a declaration of applicable law.")),
            )
        });
    v_flex()
        .gap_2()
        .child(accordion)
        .child(div().text_xs().text_color(theme.muted_foreground).child("Roles are labels, not passwords: a household role does not authenticate anyone, and a company role gives no household access."))
        .into_any_element()
}

/// A tag that opens the meanings sheet on its term.
pub fn term_button(id: impl Into<ElementId>, label: &'static str, caution: bool, term: Term) -> impl IntoElement {
    use gpui_kit::component::button::{Button, ButtonVariants as _};
    let button = Button::new(id).xsmall().compact().label(label).tooltip("What this label means").on_click(move |_, window, cx| open_sheet(window, cx, Some(term)));
    // Caution terms (future, tentative, restricted, unresolved) keep the
    // warning outline; everything else is quiet.
    if caution { button.warning().outline() } else { button.ghost() }
}

/// Small helper for the spacing between metadata terms.
pub fn dot(cx: &App) -> impl IntoElement {
    h_flex().text_xs().text_color(cx.theme().muted_foreground).child("·")
}
