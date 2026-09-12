//! One module per screen of the design. Screens are pure rendering over the
//! derived models in [`crate::models`]; state changes go through `AtlasApp`.

pub mod accounts;
pub mod activity;
pub mod assumptions;
pub mod common;
pub mod companies;
pub mod earmarks;
pub mod forecast;
pub mod funding;
pub mod people;
pub mod settings;
pub mod today;
pub mod welcome;
