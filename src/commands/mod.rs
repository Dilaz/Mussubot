use crate::error::BotResult;
use crate::config::Config;
use crate::components::ComponentManager;
use std::sync::Arc;
use tokio::sync::RwLock;

// Export submodules
pub mod util;
pub mod calendar;

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

/// All application commands and event listeners
pub fn get_all_application_commands() -> Vec<poise::Command<CommandContext, crate::error::Error>> {
    let mut commands = vec![
        // Utility commands
        util::ping(),
        util::dummy(),
    ];

    // Add calendar commands
    commands.push(calendar::this_week());

    commands
}
