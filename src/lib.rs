#![allow(
    clippy::must_use_candidate,
    clippy::return_self_not_must_use,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions
)]

pub mod app;
pub mod cli;
pub mod copier;
pub mod filters;
pub mod i18n;
pub mod probe;
pub mod scanner;
pub mod transcoder;
pub mod tui;
pub mod types;
