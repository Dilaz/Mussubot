mod commands;
mod components;
mod config;
mod error;
mod handlers;
mod utils;

use crate::commands::CommandContext;
use crate::components::{ComponentManager, GoogleCalendarHandle};
use crate::config::Config;
use crate::error::Error;
use poise::serenity_prelude as serenity;
use serenity::model::user::OnlineStatus;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use tracing_subscriber::{FmtSubscriber, EnvFilter};

#[tokio::main]
async fn main() -> miette::Result<()> {
    // Initialize logging with env filter
    // You can set the RUST_LOG environment variable to control log levels
    // e.g., RUST_LOG=debug,serenity=info,poise=info
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,serenity=warn,poise=warn"))
        )
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set up logging");
    
    // Load configuration
    let config = match Config::load() {
        Ok(config) => Arc::new(RwLock::new(config)),
        Err(e) => {
            error!("Failed to load configuration: {:?}", e);
            return Err(e.into());
        }
    };
    info!("Starting Mussubot");
    
    // Access token for bot usage
    let token = {
        let config_read = config.read().await;
        config_read.discord_token.clone()
    };
    
    // Get activity status
    let activity = {
        let config_read = config.read().await;
        config_read.activity.clone()
    };
    
    // Set up framework options
    let options = poise::FrameworkOptions {
        commands: commands::get_all_application_commands(),
        on_error: |error| Box::pin(on_error(error)),
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: Some("!".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    
    // Set intents
    let intents = serenity::GatewayIntents::non_privileged();
    
    // Create a shared data context for commands
    let command_data = CommandContext::new(Arc::clone(&config));
    
    // Initialize component manager
    let mut component_manager = ComponentManager::new(Arc::clone(&config));
    
    // Create Google Calendar handle for commands to use
    let _calendar_handle = GoogleCalendarHandle::new(Arc::clone(&config));
    
    // Create a shared component manager
    let component_manager = Arc::new(component_manager);
    
    // Create framework with new poise API
    let client_result = poise::serenity_prelude::ClientBuilder::new(token, intents)
        .framework(poise::Framework::new(options, move |ctx, ready, framework| {
            Box::pin(async move {
                info!("{} is connected!", ready.user.name);
                
                // Set the bot's status
                ctx.set_presence(
                    Some(serenity::ActivityData::playing(&activity)), 
                    OnlineStatus::Online
                );
                info!("Setting activity to {}", activity);
                
                // Initialize components
                let components = Arc::clone(&component_manager);
                if let Err(e) = components.init_all(ctx).await {
                    error!("Failed to initialize components: {:?}", e);
                }
                
                // Register slash commands
                if let Err(e) = poise::builtins::register_globally(ctx, &framework.options().commands).await {
                    error!("Failed to register slash commands: {:?}", e);
                } else {
                    info!("Slash commands registered successfully");
                }
                
                Ok(command_data)
            })
        }))
        .await;
    
    // Start the bot
    info!("Starting bot...");
    let mut client = client_result.map_err(Error::from)?;
    client.start().await.map_err(|e| Error::from(e).into())
}

/// Handle errors from commands
async fn on_error(error: poise::FrameworkError<'_, CommandContext, Error>) {
    match error {
        poise::FrameworkError::Setup { error, .. } => {
            error!("Error during setup: {:?}", error);
        }
        poise::FrameworkError::Command { error, ctx, .. } => {
            error!("Error in command '{}': {:?}", ctx.command().name, error);
            if let Err(e) = ctx.say(format!("❌ Error: {}", error)).await {
                error!("Error while sending error message: {:?}", e);
            }
        }
        poise::FrameworkError::CommandCheckFailed { error, ctx, .. } => {
            if let Some(error) = error {
                error!("Command check failed: {:?}", error);
                if let Err(e) = ctx.say(format!("❌ Check failed: {}", error)).await {
                    error!("Error while sending error message: {:?}", e);
                }
            }
        }
        error => {
            error!("Other error: {:?}", error);
        }
    }
}
