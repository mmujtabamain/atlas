//! Decisions: a step-by-step decision builder — the purchase, the down
//! payment and its funding, the recurring payment, other costs — and then the
//! result: the graph, the affordability metrics, the funding strategies, the
//! purchase-month × down-payment grid, goal trade-offs, the conditional
//! statement and the recommendation contract. Deterministic search, not
//! advice.

use gpui_kit::*;


/// Columns of the result's `Values` table.
pub const PATH_COLUMNS: [crate::widgets::grid::GridColumn; 4] = [
    crate::widgets::grid::GridColumn::new("date", "Date", 130.),
    crate::widgets::grid::GridColumn::new("baseline", "Baseline", 170.).right(),
    crate::widgets::grid::GridColumn::new("purchase", "With the purchase", 190.).right(),
    crate::widgets::grid::GridColumn::new("delta", "Difference", 150.).right(),
];

#[derive(Clone, Debug)]
pub struct DecisionPoint {
    pub label: SharedString,
    pub baseline: f64,
    pub decision: f64,
    pub reserve: f64,
}
