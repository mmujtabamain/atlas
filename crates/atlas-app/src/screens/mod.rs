//! One module per screen of the design. Screens are pure rendering over the
//! derived models in [`crate::models`]; state changes go through `AtlasApp`.

pub mod accounts;
pub mod activity;
pub mod assumptions;
pub mod common;
pub mod decisions;
pub mod companies;
pub mod earmarks;
pub mod extraction;
pub mod forecast;
pub mod funding;
pub mod people;
pub mod scenarios;
pub mod settings;
pub mod today;
pub mod welcome;
