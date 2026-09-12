//! One module per screen of the design. Screens are pure rendering over the
//! derived models in [`crate::models`]; state changes go through `AtlasApp`.

pub mod accounts;
pub mod activity;
pub mod common;
pub mod earmarks;
pub mod funding;
pub mod settings;
pub mod today;
pub mod welcome;
