#[macro_use]
extern crate rust_i18n;

mod commands;
mod components;
mod config;
mod error;
mod handlers;
mod shutdown;
mod startup;
mod utils;

use tracing::info;

// Initialize i18n
i18n!("locales", fallback = "en");

#[tokio::main]
async fn main() -> miette::Result<()> {
    // Initialize logging
    startup::init_logging()?;

    info!("Starting Mussubot");

    // Load configuration
    let config = startup::load_config().await?;

    // Start the bot
    startup::start_bot(config).await
}
