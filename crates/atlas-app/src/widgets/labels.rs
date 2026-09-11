//! Tags for the plan's vocabularies (§2.4, §10.3, §32.2, M55).
//!
//! Emphasis is a budget (design guide): current money and exact accounting are
//! neutral; only the *future* and *conditional* labels — the distinction the
//! plan cares most about — use the warning variant, and only `Unresolved` is
//! danger.

use atlas_core::vocab::{Certainty, MoneyClass, ResultStrength};
use atlas_core::Disclosure;
use gpui_kit::component::{Sizable as _, tag::Tag};
use gpui_kit::ParentElement as _;

fn neutral(text: &'static str) -> Tag {
    Tag::secondary().xsmall().outline().child(text)
}

fn caution(text: &'static str) -> Tag {
    Tag::warning().xsmall().outline().child(text)
}

/// §2.4 money class.
pub fn money_class_tag(class: MoneyClass) -> Tag {
    if class.is_current() { neutral(class.label()) } else { caution(class.label()) }
}

/// §10.3 certainty.
pub fn certainty_tag(certainty: Certainty) -> Tag {
    match certainty {
        Certainty::ScenarioOnly | Certainty::Tentative => caution(certainty.label()),
        other => neutral(other.label()),
    }
}

/// §32.2 result strength.
pub fn strength_tag(strength: ResultStrength) -> Tag {
    match strength {
        ResultStrength::ExactAccounting | ResultStrength::SolverCertified => neutral(strength.label()),
        ResultStrength::Unresolved => Tag::danger().xsmall().outline().child(strength.label()),
        other => caution(other.label()),
    }
}

/// M55 disclosure level of the current viewer.
pub fn disclosure_tag(disclosure: Disclosure) -> Tag {
    match disclosure {
        Disclosure::Full => neutral("full details"),
        Disclosure::SelectedFields => neutral("selected fields"),
        Disclosure::BalanceOnly => neutral("balance only"),
        Disclosure::Aggregate => caution("aggregate only"),
        Disclosure::Hidden => caution("hidden"),
    }
}
