//! Shared presentation pieces. Each carries the vocabulary of `plan.md` so
//! every screen speaks the same language: a [`figure::Figure`] is a value that
//! can always explain itself, [`labels`] render the three tag vocabularies,
//! and [`explain`] is the sheet that shows a calculation chain.

pub mod explain;
pub mod figure;
pub mod labels;
pub mod master;
pub mod table;
