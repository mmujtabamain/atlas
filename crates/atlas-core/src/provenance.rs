//! Calculation provenance (§2.1, §2.6, §24, M55).
//!
//! Every derived figure the engine produces is a [`Calc`]: the value plus a
//! [`ProvNode`] graph describing exactly how it was obtained. The graph is
//! complete internally; what a viewer sees is a deterministic *projection*
//! of it ([`ProvNode::project`]) governed by [`Disclosure`] levels — the
//! authoritative value never changes to make an explanation convenient.
//!
//! [`ProvNode::render_chain`] prints the plan's own layout:
//!
//! ```text
//! Current reconciled liquid cash                     1,400,000
//! + contractually expected future salary               600,000
//! - future rent                                        240,000
//! ------------------------------------------------------------
//! Conditional projected cash                         1,760,000
//! ```

use crate::ids::ObjectRef;
use crate::money::Money;
use crate::vocab::{Certainty, MoneyClass, ResultStrength};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// M55 — `D_i(v, c)`: how much of one object a viewer may see in a context.
/// Ordered from most to least restrictive.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Disclosure {
    /// The viewer may not learn the object exists.
    Hidden,
    /// Only an authorized aggregate contribution may be shown.
    Aggregate,
    /// The balance/value is visible; transactions and derivations are not.
    BalanceOnly,
    /// Some fields are visible (details decided per policy).
    SelectedFields,
    /// Everything, including node-level provenance.
    Full,
}

impl Disclosure {
    pub fn label(self) -> &'static str {
        match self {
            Disclosure::Hidden => "hidden",
            Disclosure::Aggregate => "aggregate only",
            Disclosure::BalanceOnly => "balance only",
            Disclosure::SelectedFields => "selected fields",
            Disclosure::Full => "full details",
        }
    }
}

/// The value a provenance node carries.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ProvValue {
    Money(Money),
    Count(i64),
    /// A share or rate in basis points (10_000 = 100 %).
    BasisPoints(u32),
    Date(NaiveDate),
    Text(String),
    Empty,
}

impl ProvValue {
    /// The value as text, for the chain rendering and the UI.
    pub fn render(&self) -> String {
        match self {
            ProvValue::Money(money) => money.format(),
            ProvValue::Count(count) => count.to_string(),
            ProvValue::BasisPoints(bp) => {
                let whole = bp / 100;
                let fraction = bp % 100;
                if fraction == 0 {
                    format!("{whole}%")
                } else {
                    format!("{whole}.{fraction:02}%")
                }
            }
            ProvValue::Date(date) => date.format("%Y-%m-%d").to_string(),
            ProvValue::Text(text) => text.clone(),
            ProvValue::Empty => String::new(),
        }
    }

    pub fn as_money(&self) -> Option<Money> {
        match self {
            ProvValue::Money(money) => Some(*money),
            _ => None,
        }
    }
}

impl From<Money> for ProvValue {
    fn from(money: Money) -> Self {
        ProvValue::Money(money)
    }
}

/// How a term enters its parent's sum.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Sign {
    Plus,
    Minus,
}

/// What kind of step a node represents.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Operation {
    /// A recorded fact with its source ("reconciled balance of … on …").
    Input { source: String },
    /// `value = Σ sign · child` over the non-excluded children.
    Sum,
    /// A floor/reserve/rule check; the value is the headroom or shortfall.
    Constraint { satisfied: bool },
    /// An object deliberately left out; shown with zero weight so the reader
    /// can see what was *not* counted and why (§8.5, §15).
    Excluded { reason: String },
    /// Several restricted nodes replaced by one authorized aggregate (M55).
    Aggregate { restricted_terms: usize },
    /// A named formula, e.g. `[R − b]₊` (M13).
    Formula { text: String },
}

/// One node of a calculation graph.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ProvNode {
    label: String,
    value: ProvValue,
    sign: Sign,
    operation: Operation,
    children: Vec<ProvNode>,
    notes: Vec<String>,
    strength: ResultStrength,
    money_class: Option<MoneyClass>,
    certainty: Option<Certainty>,
    subject: Option<ObjectRef>,
}

impl ProvNode {
    /// A recorded fact.
    pub fn input(label: impl Into<String>, value: impl Into<ProvValue>, source: impl Into<String>) -> Self {
        ProvNode {
            label: label.into(),
            value: value.into(),
            sign: Sign::Plus,
            operation: Operation::Input { source: source.into() },
            children: Vec::new(),
            notes: Vec::new(),
            strength: ResultStrength::ExactAccounting,
            money_class: None,
            certainty: None,
            subject: None,
        }
    }

    /// A sum of signed terms. The caller supplies the already-computed value;
    /// [`ProvNode::verify_sums`] checks that it matches the terms.
    pub fn sum(label: impl Into<String>, value: impl Into<ProvValue>, children: Vec<ProvNode>) -> Self {
        ProvNode {
            label: label.into(),
            value: value.into(),
            sign: Sign::Plus,
            operation: Operation::Sum,
            children,
            notes: Vec::new(),
            strength: ResultStrength::ExactAccounting,
            money_class: None,
            certainty: None,
            subject: None,
        }
    }

    /// A constraint check whose value is the headroom (positive) or shortfall.
    pub fn constraint(label: impl Into<String>, value: impl Into<ProvValue>, satisfied: bool) -> Self {
        ProvNode {
            label: label.into(),
            value: value.into(),
            sign: Sign::Plus,
            operation: Operation::Constraint { satisfied },
            children: Vec::new(),
            notes: Vec::new(),
            strength: ResultStrength::ExactAccounting,
            money_class: None,
            certainty: None,
            subject: None,
        }
    }

    /// Something that was deliberately not counted.
    pub fn excluded(label: impl Into<String>, value: impl Into<ProvValue>, reason: impl Into<String>) -> Self {
        ProvNode {
            label: label.into(),
            value: value.into(),
            sign: Sign::Plus,
            operation: Operation::Excluded { reason: reason.into() },
            children: Vec::new(),
            notes: Vec::new(),
            strength: ResultStrength::ExactAccounting,
            money_class: None,
            certainty: None,
            subject: None,
        }
    }

    /// A named formula step.
    pub fn formula(label: impl Into<String>, value: impl Into<ProvValue>, text: impl Into<String>, children: Vec<ProvNode>) -> Self {
        ProvNode {
            label: label.into(),
            value: value.into(),
            sign: Sign::Plus,
            operation: Operation::Formula { text: text.into() },
            children,
            notes: Vec::new(),
            strength: ResultStrength::ExactAccounting,
            money_class: None,
            certainty: None,
            subject: None,
        }
    }

    // ----- builders -----------------------------------------------------------

    /// This term is subtracted by its parent.
    pub fn minus(mut self) -> Self {
        self.sign = Sign::Minus;
        self
    }

    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn strength(mut self, strength: ResultStrength) -> Self {
        self.strength = strength;
        self
    }

    pub fn money_class(mut self, class: MoneyClass) -> Self {
        self.money_class = Some(class);
        self
    }

    pub fn certainty(mut self, certainty: Certainty) -> Self {
        self.certainty = Some(certainty);
        self
    }

    /// The financial object this node derives from; drives projection.
    pub fn subject(mut self, subject: ObjectRef) -> Self {
        self.subject = Some(subject);
        self
    }

    // ----- readers ------------------------------------------------------------

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn value(&self) -> &ProvValue {
        &self.value
    }

    pub fn sign(&self) -> Sign {
        self.sign
    }

    pub fn operation(&self) -> &Operation {
        &self.operation
    }

    pub fn children(&self) -> &[ProvNode] {
        &self.children
    }

    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    pub fn result_strength(&self) -> ResultStrength {
        self.strength
    }

    pub fn money_class_label(&self) -> Option<MoneyClass> {
        self.money_class
    }

    pub fn certainty_label(&self) -> Option<Certainty> {
        self.certainty
    }

    pub fn subject_ref(&self) -> Option<ObjectRef> {
        self.subject
    }

    pub fn is_excluded(&self) -> bool {
        matches!(self.operation, Operation::Excluded { .. })
    }

    /// The signed money contribution of this node to its parent, if any.
    pub fn signed_money(&self) -> Option<Money> {
        let money = self.value.as_money()?;
        Some(match self.sign {
            Sign::Plus => money,
            Sign::Minus => money.negated(),
        })
    }

    // ----- integrity ------------------------------------------------------------

    /// Checks every `Sum` node in the graph: its money value must equal the
    /// signed sum of its non-excluded children. Returns one message per
    /// discrepancy; an empty list means the graph is arithmetically consistent.
    pub fn verify_sums(&self) -> Vec<String> {
        let mut problems = Vec::new();
        self.verify_into(&mut problems);
        problems
    }

    fn verify_into(&self, problems: &mut Vec<String>) {
        if matches!(self.operation, Operation::Sum)
            && let Some(expected) = self.value.as_money()
        {
            let mut total = Money::zero(expected.currency());
            let mut mixed = false;
            for child in self.children.iter().filter(|c| !c.is_excluded()) {
                match child.signed_money() {
                    Some(term) => match total.checked_add(term) {
                        Ok(sum) => total = sum,
                        Err(err) => problems.push(format!("{}: {err}", self.label)),
                    },
                    None => mixed = true,
                }
            }
            if !mixed && total != expected {
                problems.push(format!(
                    "{}: terms sum to {} but the node says {}",
                    self.label,
                    total.format(),
                    expected.format()
                ));
            }
        }
        for child in &self.children {
            child.verify_into(problems);
        }
    }

    // ----- rendering ------------------------------------------------------------

    /// The plan's chain layout (§2.1). Nested sums are rendered as their own
    /// blocks after the parent block, in order.
    pub fn render_chain(&self) -> String {
        let mut blocks = Vec::new();
        self.render_block(&mut blocks);
        blocks.join("\n\n")
    }

    fn render_block(&self, blocks: &mut Vec<String>) {
        if self.children.is_empty() {
            let mut block = format!("{}  {}", self.label, self.value.render());
            for note in &self.notes {
                block.push_str("\n    ");
                block.push_str(note);
            }
            blocks.push(block);
            return;
        }

        let label_width = self
            .children
            .iter()
            .map(|c| c.label.chars().count() + if c.is_excluded() { 11 } else { 0 })
            .chain(std::iter::once(self.label.chars().count()))
            .max()
            .unwrap_or(0)
            .clamp(24, 72);
        let value_width = self
            .children
            .iter()
            .map(|c| c.value.render().chars().count())
            .chain(std::iter::once(self.value.render().chars().count()))
            .max()
            .unwrap_or(0);
        let total_width = 2 + label_width + 2 + value_width;

        let mut lines: Vec<String> = Vec::new();
        let mut first_term = true;
        for child in &self.children {
            let (prefix, label) = if child.is_excluded() {
                ("  ", format!("(excluded) {}", child.label))
            } else if first_term {
                first_term = false;
                ("  ", child.label.clone())
            } else {
                match child.sign {
                    Sign::Plus => ("+ ", child.label.clone()),
                    Sign::Minus => ("- ", child.label.clone()),
                }
            };
            lines.push(format!(
                "{prefix}{label:<label_width$}  {value:>value_width$}",
                value = child.value.render()
            ));
            if let Operation::Excluded { reason } = &child.operation {
                lines.push(format!("      {reason}"));
            }
            for note in &child.notes {
                lines.push(format!("      {note}"));
            }
        }
        lines.push("-".repeat(total_width));
        lines.push(format!(
            "  {label:<label_width$}  {value:>value_width$}",
            label = self.label,
            value = self.value.render()
        ));
        if let Operation::Formula { text } = &self.operation {
            lines.push(format!("      formula: {text}"));
        }
        for note in &self.notes {
            lines.push(format!("      {note}"));
        }
        blocks.push(lines.join("\n"));

        for child in self.children.iter().filter(|c| !c.children.is_empty()) {
            child.render_block(blocks);
        }
    }

    // ----- M55 projection ---------------------------------------------------------

    /// `E_v(c) = Project(G(c), D(v, c))`: the explanation a viewer is allowed
    /// to see. Restricted nodes are collapsed into authorized aggregates, never
    /// removed from the arithmetic, so the projected chain still adds up to the
    /// authoritative result (M55 invariants 3 and 4).
    pub fn project(&self, disclosure_of: &dyn Fn(&ObjectRef) -> Disclosure) -> ProvNode {
        let own = self.subject.as_ref().map(disclosure_of).unwrap_or(Disclosure::Full);
        match own {
            Disclosure::Hidden | Disclosure::Aggregate => self.collapsed(own),
            Disclosure::BalanceOnly => ProvNode {
                children: Vec::new(),
                notes: vec!["derivation not disclosed to this viewer (balance-only policy)".into()],
                ..self.clone()
            },
            Disclosure::SelectedFields | Disclosure::Full => {
                let mut projected = Vec::with_capacity(self.children.len());
                let mut restricted: Vec<ProvNode> = Vec::new();
                let mut restricted_level = Disclosure::Aggregate;
                let mut restricted_subjects: Vec<ObjectRef> = Vec::new();
                for child in &self.children {
                    let level = child.subject.as_ref().map(disclosure_of).unwrap_or(Disclosure::Full);
                    if matches!(level, Disclosure::Hidden | Disclosure::Aggregate) {
                        // A restricted object that was excluded anyway contributes
                        // nothing; showing even an aggregate line would disclose
                        // that it exists (V062), so it is dropped.
                        if child.is_excluded() {
                            continue;
                        }
                        restricted_level = restricted_level.min(level);
                        if let Some(subject) = child.subject
                            && !restricted_subjects.contains(&subject)
                        {
                            restricted_subjects.push(subject);
                        }
                        restricted.push(child.clone());
                    } else {
                        if !restricted.is_empty() {
                            projected.push(merge_restricted(&restricted, restricted_level));
                            restricted.clear();
                            restricted_level = Disclosure::Aggregate;
                        }
                        projected.push(child.project(disclosure_of));
                    }
                }
                if !restricted.is_empty() {
                    projected.push(merge_restricted(&restricted, restricted_level));
                }
                // §7.6 / V073 — a difference attack: with exactly one restricted
                // term next to disclosed money terms and a disclosed total, the
                // restricted value is total − the rest. Coarsen: the breakdown
                // collapses into one authorized aggregate, only the total stays.
                let restricted_terms: Vec<usize> = projected.iter().enumerate().filter(|(_, n)| matches!(n.operation, Operation::Aggregate { .. })).map(|(i, _)| i).collect();
                let disclosed_money = projected.iter().filter(|n| !matches!(n.operation, Operation::Aggregate { .. }) && n.signed_money().is_some()).count();
                // Exactly one restricted *object* (several lines of the same object count once).
                let single_restricted_value = restricted_terms.len() == 1 && restricted_subjects.len() == 1 && projected[restricted_terms[0]].signed_money().is_some();
                let mut notes = if own == Disclosure::SelectedFields { Vec::new() } else { self.notes.clone() };
                if single_restricted_value && disclosed_money > 0 && matches!(self.operation, Operation::Sum) {
                    let level = restricted_level;
                    let (label, note) = match level {
                        Disclosure::Hidden => ("Contributions (breakdown suppressed)", "one restricted contribution sits among disclosed terms; listing the others would reveal it as the difference, so only the authorized total is shown (§7.6, V073)"),
                        _ => ("Contributions incl. an owner-authorized restricted one (breakdown suppressed)", "showing the other terms next to a single restricted contribution would reveal it as total − rest; the owner can authorize the disclosure explicitly (§7.6, V073)"),
                    };
                    // The single child carries the node's own value; the node's sign is its
                    // parent's business, so the child adds up with a plus.
                    let collapsed = ProvNode {
                        label: label.into(),
                        value: self.value.clone(),
                        sign: Sign::Plus,
                        operation: Operation::Aggregate { restricted_terms: projected.len() },
                        children: Vec::new(),
                        notes: vec![note.into()],
                        strength: self.strength,
                        money_class: self.money_class,
                        certainty: self.certainty,
                        subject: None,
                    };
                    notes.push("suppression applied: one restricted contribution (§7.6)".into());
                    return ProvNode { children: vec![collapsed], notes, ..self.clone() };
                }
                ProvNode {
                    children: projected,
                    notes,
                    ..self.clone()
                }
            }
        }
    }

    fn collapsed(&self, level: Disclosure) -> ProvNode {
        merge_restricted(std::slice::from_ref(self), level)
    }
}

/// Replaces restricted siblings by one aggregate node carrying their net
/// contribution (§2.6: "owner-authorized restricted contribution").
fn merge_restricted(nodes: &[ProvNode], level: Disclosure) -> ProvNode {
    let mut net: Option<Money> = None;
    let mut money_only = true;
    for node in nodes.iter().filter(|n| !n.is_excluded()) {
        match node.signed_money() {
            Some(term) => {
                net = Some(match net {
                    None => term,
                    Some(total) => total.checked_add(term).unwrap_or(total),
                });
            }
            None => money_only = false,
        }
    }
    let (value, sign) = match net {
        Some(total) if money_only => {
            if total.is_negative() {
                (ProvValue::Money(total.abs()), Sign::Minus)
            } else {
                (ProvValue::Money(total), Sign::Plus)
            }
        }
        _ => (ProvValue::Text("restricted".into()), Sign::Plus),
    };
    fn same<T: PartialEq + Copy>(nodes: &[ProvNode], pick: fn(&ProvNode) -> Option<T>) -> Option<T> {
        let first = pick(&nodes[0]);
        if nodes.iter().all(|n| pick(n) == first) { first } else { None }
    }
    let (label, note) = match level {
        Disclosure::Hidden => (
            "Restricted contribution",
            "the viewer is not authorized to learn what contributes here (§7.2)",
        ),
        _ => (
            "Owner-authorized restricted contribution",
            "underlying account details not disclosed (§2.6)",
        ),
    };
    ProvNode {
        label: label.into(),
        value,
        sign,
        operation: Operation::Aggregate {
            restricted_terms: if level == Disclosure::Hidden { 0 } else { nodes.len() },
        },
        children: Vec::new(),
        notes: vec![note.into()],
        strength: nodes[0].strength,
        money_class: same(nodes, |n| n.money_class),
        certainty: same(nodes, |n| n.certainty),
        subject: None,
    }
}

/// A derived value together with its complete calculation graph.
///
/// The graph is behind an [`Arc`]: a `Calc` is cloned wherever a figure is
/// shown (screen models, the "Why?" sheet, the click handler that opens it),
/// and a chain of a few hundred nodes copied on every frame was a measurable
/// share of the frame budget. Cloning a `Calc` now copies a pointer; the tree
/// itself is immutable once built.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Calc<T> {
    value: T,
    node: Arc<ProvNode>,
}

impl<T> Calc<T> {
    pub fn new(value: T, node: ProvNode) -> Self {
        Calc { value, node: Arc::new(node) }
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn node(&self) -> &ProvNode {
        &self.node
    }

    /// The graph as a shared handle — the cheap way to keep a chain around
    /// (an explain sheet, a closure) without copying it.
    pub fn shared_node(&self) -> Arc<ProvNode> {
        Arc::clone(&self.node)
    }

    /// Takes the value and the graph apart; copies the graph only when it is
    /// still shared with someone else.
    pub fn into_parts(self) -> (T, ProvNode) {
        (self.value, Arc::try_unwrap(self.node).unwrap_or_else(|shared| (*shared).clone()))
    }
}

impl Calc<Money> {
    /// The money value by copy.
    pub fn money(&self) -> Money {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::AccountId;
    use crate::money::Currency;

    fn pkr(major: i64) -> Money {
        Money::from_major(major, Currency::PKR)
    }

    fn section_2_1_chain() -> ProvNode {
        let projected = ProvNode::sum(
            "Conditional projected cash",
            pkr(1_590_000),
            vec![
                ProvNode::input("Current reconciled liquid cash", pkr(1_400_000), "reconciled balances"),
                ProvNode::input("contractually expected future salary", pkr(600_000), "series"),
                ProvNode::input("other assumed future salary", pkr(300_000), "series"),
                ProvNode::input("expected future freelance payment", pkr(250_000), "series"),
                ProvNode::input("future rent", pkr(240_000), "series").minus(),
                ProvNode::input("future ordinary expenses", pkr(320_000), "series").minus(),
                ProvNode::input("planned car down payment", pkr(400_000), "scenario").minus(),
            ],
        );
        ProvNode::sum(
            "Conditional projected unreserved cash",
            pkr(1_470_000),
            vec![
                projected,
                ProvNode::input("non-overlapping tax earmark", pkr(120_000), "reservation").minus(),
            ],
        )
    }

    #[test]
    fn renders_the_plan_layout() {
        let text = section_2_1_chain().render_chain();
        let expected = "  Conditional projected cash             1,590,000
- non-overlapping tax earmark              120,000
--------------------------------------------------
  Conditional projected unreserved cash  1,470,000

  Current reconciled liquid cash        1,400,000
+ contractually expected future salary    600,000
+ other assumed future salary             300,000
+ expected future freelance payment       250,000
- future rent                             240,000
- future ordinary expenses                320,000
- planned car down payment                400,000
-------------------------------------------------
  Conditional projected cash            1,590,000";
        assert_eq!(text, expected);
    }

    #[test]
    fn verifies_sums() {
        assert!(section_2_1_chain().verify_sums().is_empty());
        let wrong = ProvNode::sum("x", pkr(10), vec![ProvNode::input("a", pkr(4), "t"), ProvNode::input("b", pkr(5), "t")]);
        let problems = wrong.verify_sums();
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("terms sum to 9"));
    }

    #[test]
    fn excluded_terms_do_not_count() {
        let node = ProvNode::sum(
            "Household liquid cash",
            pkr(100),
            vec![
                ProvNode::input("Checking", pkr(100), "balance"),
                ProvNode::excluded("Company Alpha operating", pkr(2_000_000), "business cash is not household cash (§8.5)"),
            ],
        );
        assert!(node.verify_sums().is_empty());
        let text = node.render_chain();
        assert!(text.contains("(excluded) Company Alpha operating"));
        assert!(text.contains("§8.5"));
    }

    #[test]
    fn projects_restricted_accounts_into_an_aggregate() {
        let shared = AccountId::new(1);
        let private = AccountId::new(2);
        let node = ProvNode::sum(
            "Projected household-usable cash",
            pkr(1_200_000),
            vec![
                ProvNode::input("Current household-visible cash", pkr(1_400_000), "balance").subject(ObjectRef::Account(shared)),
                ProvNode::sum(
                    "Person A private savings",
                    pkr(500_000),
                    vec![ProvNode::input("settled balance", pkr(500_000), "balance").subject(ObjectRef::Account(private))],
                )
                .subject(ObjectRef::Account(private)),
                ProvNode::input("planned household obligations", pkr(700_000), "series").minus(),
            ],
        );
        let projected = node.project(&|object| match object {
            ObjectRef::Account(id) if *id == private => Disclosure::Aggregate,
            _ => Disclosure::Full,
        });
        assert!(projected.verify_sums().is_empty());
        // §7.6 / V073: one restricted object next to disclosed terms would be total − rest,
        // so the breakdown is suppressed and only the authorized total remains.
        assert_eq!(projected.children().len(), 1);
        let aggregate = &projected.children()[0];
        assert!(aggregate.label().contains("breakdown suppressed"));
        assert_eq!(aggregate.value().as_money(), Some(pkr(1_200_000)));
        assert!(aggregate.children().is_empty());
        let text = projected.render_chain();
        assert!(!text.contains("Person A private savings"));
        assert!(!text.contains("500,000"), "the private balance is not derivable");
        assert!(!text.contains("1,400,000") && !text.contains("700,000"), "the other terms are coarsened away too");
        assert!(text.contains("V073"));

        // With two restricted objects, the aggregate of both is an authorized disclosure.
        let other = AccountId::new(3);
        let two = ProvNode::sum(
            "Projected household-usable cash",
            pkr(1_500_000),
            vec![
                ProvNode::input("Current household-visible cash", pkr(1_400_000), "balance").subject(ObjectRef::Account(shared)),
                ProvNode::input("Person A private savings", pkr(500_000), "balance").subject(ObjectRef::Account(private)),
                ProvNode::input("Person A private deposit", pkr(300_000), "balance").subject(ObjectRef::Account(other)),
                ProvNode::input("planned household obligations", pkr(700_000), "series").minus(),
            ],
        );
        let projected = two.project(&|object| match object {
            ObjectRef::Account(id) if *id == private || *id == other => Disclosure::Aggregate,
            _ => Disclosure::Full,
        });
        assert!(projected.verify_sums().is_empty());
        let aggregate = &projected.children()[1];
        assert_eq!(aggregate.label(), "Owner-authorized restricted contribution");
        assert_eq!(aggregate.value().as_money(), Some(pkr(800_000)));
        assert!(matches!(aggregate.operation(), Operation::Aggregate { restricted_terms: 2 }));
        assert!(projected.render_chain().contains("underlying account details not disclosed"));

        // The owner sees everything.
        let full = node.project(&|_| Disclosure::Full);
        assert_eq!(full, node);
    }

    #[test]
    fn hidden_objects_do_not_reveal_their_count() {
        let secret = AccountId::new(9);
        let node = ProvNode::sum(
            "total",
            pkr(30),
            vec![
                ProvNode::input("visible", pkr(10), "b"),
                ProvNode::input("s1", pkr(5), "b").subject(ObjectRef::Account(secret)),
                ProvNode::input("s2", pkr(15), "b").subject(ObjectRef::Account(secret)),
            ],
        );
        // Both hidden lines belong to one object: showing "visible 10" next to a total of 30
        // would give it away, so the breakdown is suppressed (§7.6).
        let projected = node.project(&|_| Disclosure::Hidden);
        assert_eq!(projected.children().len(), 1);
        assert!(projected.children()[0].label().contains("suppressed"));
        assert_eq!(projected.children()[0].value().as_money(), Some(pkr(30)));
        assert!(projected.verify_sums().is_empty());
        // Two distinct hidden objects: their aggregate is shown without a count.
        let node = ProvNode::sum(
            "total",
            pkr(30),
            vec![
                ProvNode::input("visible", pkr(10), "b"),
                ProvNode::input("s1", pkr(5), "b").subject(ObjectRef::Account(secret)),
                ProvNode::input("s2", pkr(15), "b").subject(ObjectRef::Account(AccountId::new(10))),
            ],
        );
        let projected = node.project(&|_| Disclosure::Hidden);
        assert_eq!(projected.children().len(), 2);
        assert!(matches!(projected.children()[1].operation(), Operation::Aggregate { restricted_terms: 0 }));
        assert_eq!(projected.children()[1].value().as_money(), Some(pkr(20)));
        assert!(projected.verify_sums().is_empty());
    }

    #[test]
    fn restricted_excluded_objects_vanish_from_the_projection() {
        let secret = AccountId::new(9);
        let node = ProvNode::sum(
            "Household liquid cash",
            pkr(10),
            vec![
                ProvNode::input("visible", pkr(10), "b"),
                ProvNode::excluded("Company payroll", pkr(350), "business cash (§8.5)").subject(ObjectRef::Account(secret)),
            ],
        );
        let projected = node.project(&|_| Disclosure::Hidden);
        assert_eq!(projected.children().len(), 1);
        assert!(!projected.render_chain().contains("Company payroll"));
        assert!(projected.verify_sums().is_empty());
    }

    #[test]
    fn balance_only_drops_the_derivation() {
        let account = AccountId::new(4);
        let node = ProvNode::sum("Free cash", pkr(650_000), vec![ProvNode::input("settled", pkr(2_000_000), "b"), ProvNode::input("earmarks", pkr(1_350_000), "r").minus()])
            .subject(ObjectRef::Account(account));
        let projected = node.project(&|_| Disclosure::BalanceOnly);
        assert!(projected.children().is_empty());
        assert_eq!(projected.value().as_money(), Some(pkr(650_000)));
        assert!(projected.notes()[0].contains("balance-only"));
    }

    #[test]
    fn renders_basis_points_and_dates() {
        assert_eq!(ProvValue::BasisPoints(5_000).render(), "50%");
        assert_eq!(ProvValue::BasisPoints(60).render(), "0.60%");
        assert_eq!(ProvValue::Date(NaiveDate::from_ymd_opt(2026, 9, 11).unwrap()).render(), "2026-09-11");
    }
}
