use crate::commands::{create_error_embed, get_all_application_commands, CommandContext};
use crate::components::{
    google_calendar::GoogleCalendar, work_schedule::WorkSchedule, ComponentManager,
};
use crate::config::Config;
use crate::error::Error;
use crate::shutdown;
use poise::serenity_prelude as serenity;
use rust_i18n::t;
use serenity::model::user::OnlineStatus;
use std::sync::Arc;
use tokio::sync::{oneshot, RwLock};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

/// Initialize logging with environment-based configuration
pub fn init_logging() -> miette::Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,serenity=warn,poise=warn")),
        )
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .map_err(|e| Error::Other(format!("Failed to set up logging: {}", e)))?;

    Ok(())
}

/// Load and initialize the application config
pub async fn load_config() -> miette::Result<Arc<RwLock<Config>>> {
    match Config::load() {
        Ok(config) => Ok(Arc::new(RwLock::new(config))),
        Err(e) => {
            error!("Failed to load configuration: {:?}", e);
            Err(e.into())
        }
    }
}

/// Initialize and start the Discord bot
pub async fn start_bot(config: Arc<RwLock<Config>>) -> miette::Result<()> {
    // Get Discord token and activity
    let token = {
        let config_read = config.read().await;
        config_read.discord_token.clone()
    };

    let activity = {
        let config_read = config.read().await;
        config_read.activity.clone()
    };

    // Set locale from config
    {
        let config_read = config.read().await;
        crate::utils::i18n::set_locale(&config_read.bot_locale);
        info!("Setting locale to {}", config_read.bot_locale);
    }

    // Set up framework options
    let options = poise::FrameworkOptions {
        commands: get_all_application_commands(),
        on_error: |error| Box::pin(on_error(error)),
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: Some("!".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

    // Set intents
    let intents = serenity::GatewayIntents::non_privileged();

    // Initialize component manager
    let mut component_manager = ComponentManager::new(Arc::clone(&config));

    // Initialize Redis service
    let (mut redis_actor, redis_handle) =
        crate::components::redis_service::RedisActor::new(Arc::clone(&config));

    // Spawn Redis actor task
    tokio::spawn(async move {
        redis_actor.run().await;
    });

    // Register Google Calendar component
    component_manager.register(GoogleCalendar::new());

    // Register Work Schedule component
    component_manager.register(WorkSchedule::new());

    // Create a shared component manager
    let component_manager = Arc::new(component_manager);

    // Create a shared data context for commands
    let command_data = CommandContext::new(Arc::clone(&config))
        .with_component_manager(Arc::clone(&component_manager));

    // Create shutdown channel
    let (shutdown_send, shutdown_recv) = oneshot::channel();

    // Clone redis handle for shutdown handler
    let shutdown_redis = redis_handle.clone();

    // Clone component manager for shutdown handler
    let shutdown_components = Arc::clone(&component_manager);

    // Spawn signal handler task
    tokio::spawn(async move {
        shutdown::handle_signals(shutdown_send, shutdown_components, shutdown_redis).await;
    });

    // Create framework with new poise API
    let client_result = poise::serenity_prelude::ClientBuilder::new(token, intents)
        .framework(poise::Framework::new(
            options,
            move |ctx, ready, framework| {
                Box::pin(async move {
                    info!("{} is connected!", ready.user.name);

                    // Set the bot's status
                    ctx.set_presence(
                        Some(serenity::ActivityData::playing(&activity)),
                        OnlineStatus::Online,
                    );
                    info!("Setting activity to {}", activity);

                    // Initialize components
                    let components = Arc::clone(&component_manager);
                    if let Err(e) = components
                        .init_all(ctx, Arc::clone(&config), redis_handle.clone())
                        .await
                    {
                        error!("Failed to initialize components: {:?}", e);
                    }

                    // Register slash commands
                    if let Err(e) =
                        poise::builtins::register_globally(ctx, &framework.options().commands).await
                    {
                        error!("Failed to register slash commands: {:?}", e);
                    } else {
                        info!("Slash commands registered successfully");
                    }

                    Ok(command_data)
                })
            },
        ))
        .await;

    // Start the bot
    info!("Starting bot...");
    let mut client = client_result.map_err(Error::from)?;

    // Create a separate task to handle the client
    let client_handle = tokio::spawn(async move {
        if let Err(e) = client.start().await {
            Err(Error::from(e))
        } else {
            Ok(())
        }
    });

    // Wait for either the client to end or a shutdown signal
    tokio::select! {
        result = client_handle => {
            info!("Bot process ended");
            match result {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => Err(e.into()),
                Err(e) => {
                    error!("Client task error: {:?}", e);
                    Err(Error::Other(format!("Client task error: {}", e)).into())
                }
            }
        }
        _ = shutdown_recv => {
            info!("Received shutdown signal, shutting down bot...");
            Ok(())
        }
    }
}

/// Handle errors from commands
async fn on_error(error: poise::FrameworkError<'_, CommandContext, Error>) {
    match error {
        poise::FrameworkError::Setup { error, .. } => {
            error!("Error during setup: {:?}", error);
        }
        poise::FrameworkError::Command { error, ctx, .. } => {
            error!("Error in command '{}': {:?}", ctx.command().name, error);
            if let Err(e) = ctx
                .send(
                    poise::CreateReply::default()
                        .embed(create_error_embed(
                            &t!("error_title", context = "command"),
                            &format!("{}", error),
                        ))
                        .ephemeral(true),
                )
                .await
            {
                error!("Error while sending error message: {:?}", e);
            }
        }
        poise::FrameworkError::CommandCheckFailed { error, ctx, .. } => {
            if let Some(error) = error {
                error!("Command check failed: {:?}", error);
                if let Err(e) = ctx
                    .send(
                        poise::CreateReply::default()
                            .embed(create_error_embed(
                                &t!("error_title", context = "check"),
                                &format!("{}", error),
                            ))
                            .ephemeral(true),
                    )
                    .await
                {
                    error!("Error while sending error message: {:?}", e);
                }
            }
        }
        error => {
            error!("Other error: {:?}", error);
        }
    }
}
