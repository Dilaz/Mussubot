use crate::components::ComponentManager;
use crate::config::Config;
use crate::error::BotResult;
use poise::serenity_prelude::CreateEmbed;
use std::sync::Arc;
use tokio::sync::RwLock;

// Export submodules
pub mod calendar;
pub mod util;
pub mod work;

/// Shared context for all commands
#[derive(Debug)]
pub struct CommandContext {
    // Add any shared command context here
    pub config: Arc<RwLock<Config>>,
    pub component_manager: Option<Arc<ComponentManager>>,
}

impl CommandContext {
    /// Create a new command context
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        Self {
            config,
            component_manager: None,
        }
    }

    /// Set the component manager
    pub fn with_component_manager(mut self, component_manager: Arc<ComponentManager>) -> Self {
        self.component_manager = Some(component_manager);
        self
    }
}

/// Type alias for command result
pub type CommandResult = BotResult<()>;

/// Type alias for poise context
pub type Context<'a> = poise::Context<'a, CommandContext, crate::error::Error>;

/// Helper function to create a success embed
pub fn create_success_embed(title: &str, description: &str) -> CreateEmbed {
    CreateEmbed::new()
        .title(title)
        .description(description)
        .color(0x00FF00) // Green color
}

/// Helper function to create an info embed
pub fn create_info_embed(title: &str, description: &str) -> CreateEmbed {
    CreateEmbed::new()
        .title(title)
        .description(description)
        .color(0x0099FF) // Blue color
}

/// Helper function to create a warning embed
pub fn create_warning_embed(title: &str, description: &str) -> CreateEmbed {
    CreateEmbed::new()
        .title(title)
        .description(description)
        .color(0xFFAA00) // Orange color
}

/// Helper function to create an error embed
pub fn create_error_embed(title: &str, description: &str) -> CreateEmbed {
    CreateEmbed::new()
        .title(title)
        .description(description)
        .color(0xFF0000) // Red color
}

/// All application commands and event listeners
pub fn get_all_application_commands() -> Vec<poise::Command<CommandContext, crate::error::Error>> {
    let mut commands = vec![
        // Utility commands
        util::ping(),
    ];

    // Add calendar commands
    commands.push(calendar::this_week());
    
    // Add work schedule commands
    commands.push(work::tyovuorot());
    commands.push(work::day());
    commands.push(work::employee());
    commands.push(work::ensiviikko());

    commands
}
