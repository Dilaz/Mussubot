#[macro_use]
extern crate rust_i18n;

pub mod components;
pub mod config;
pub mod error;
pub mod utils;

// Initialize i18n
i18n!("locales", fallback = "en");
