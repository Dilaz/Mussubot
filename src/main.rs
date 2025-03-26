mod commands;
mod components;
mod config;
mod error;
mod handlers;
mod shutdown;
mod startup;
mod utils;

use tracing::info;

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
