//! Tags for the three vocabularies every figure carries: money class,
//! certainty and result strength, plus the viewer's disclosure level.
//!
//! Emphasis is a budget (design guide): current money and exact accounting are
//! neutral; only the *future* and *conditional* labels — the distinction that
//! matters most — use the warning variant, and only `Unresolved` is danger.

use atlas_core::model::Hardness;
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

/// Money class.
pub fn money_class_tag(class: MoneyClass) -> Tag {
    if class.is_current() { neutral(class.label()) } else { caution(class.label()) }
}

/// Certainty.
pub fn certainty_tag(certainty: Certainty) -> Tag {
    match certainty {
        Certainty::ScenarioOnly | Certainty::Tentative => caution(certainty.label()),
        other => neutral(other.label()),
    }
}

/// Result strength.
pub fn strength_tag(strength: ResultStrength) -> Tag {
    match strength {
        ResultStrength::ExactAccounting | ResultStrength::SolverCertified => neutral(strength.label()),
        ResultStrength::Unresolved => Tag::danger().xsmall().outline().child(strength.label()),
        other => caution(other.label()),
    }
}

/// Disclosure level of the current viewer.
pub fn disclosure_tag(disclosure: Disclosure) -> Tag {
    match disclosure {
        Disclosure::Full => neutral("full details"),
        Disclosure::SelectedFields => neutral("selected fields"),
        Disclosure::BalanceOnly => neutral("balance only"),
        Disclosure::Aggregate => caution("aggregate only"),
        Disclosure::Hidden => caution("hidden"),
    }
}

/// Assumption freshness at the reconciliation date.
pub fn freshness_tag(freshness: atlas_core::model::Freshness) -> Tag {
    use atlas_core::model::Freshness;
    match freshness {
        Freshness::Fresh => neutral(freshness.label()),
        Freshness::NotAccepted | Freshness::Stale => caution(freshness.label()),
        Freshness::Expired => Tag::danger().xsmall().outline().child(freshness.label()),
    }
}

/// Hard constraint vs user-relaxable preference.
pub fn hardness_tag(hardness: Hardness) -> Tag {
    match hardness {
        Hardness::Hard => neutral("Hard constraint"),
        Hardness::SoftUserRelaxable => neutral("User-relaxable"),
    }
}

/// An occurrence's live status: danger when overdue, warning when due or
/// partly fulfilled, quiet otherwise.
pub fn status_tag(status: atlas_core::timeline::OccurrenceStatus) -> Tag {
    use atlas_core::timeline::OccurrenceStatus;
    match status {
        OccurrenceStatus::Overdue => Tag::danger().xsmall().outline().child(status.label()),
        OccurrenceStatus::Due | OccurrenceStatus::PartiallyFulfilled => Tag::warning().xsmall().outline().child(status.label()),
        _ => Tag::secondary().xsmall().outline().child(status.label()),
    }
}
