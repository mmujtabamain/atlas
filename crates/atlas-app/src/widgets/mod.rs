//! Shared compositions. Each carries the same vocabulary so every screen
//! speaks the same language: a [`figure::Figure`] is a value that can always
//! explain itself, [`explain`] is the calculation sheet, [`meanings`] the
//! reference sheet for every tag, [`labels`] the tag vocabularies, [`scope`]
//! the local analysis header, [`states`] the empty / not-disclosed / error
//! blocks, [`grid`] the virtualised table, [`chart`] the cash path.

pub mod chart;
pub mod copy;
pub mod explain;
pub mod figure;
pub mod grid;
pub mod labels;
pub mod master;
pub mod meanings;
pub mod scope;
pub mod states;
pub mod table;
