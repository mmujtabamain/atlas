//! Shared presentation pieces. Each carries the vocabulary of `plan.md` so
//! every screen speaks the same language: a [`figure::Figure`] is a value that
//! can always explain itself, [`labels`] render the three tag vocabularies,
//! [`explain`] is the sheet that shows a calculation chain, and [`grid`] is
//! the virtualised table for rows that grow with the horizon.

pub mod explain;
pub mod figure;
pub mod grid;
pub mod labels;
pub mod master;
pub mod table;
