use crate::config::Config;
use crate::error::BotResult;
use async_trait::async_trait;
use poise::serenity_prelude as serenity;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

// Export components
pub mod google_calendar;

// Re-export Google Calendar handle
pub use google_calendar::GoogleCalendarHandle;

/// Component trait that all components must implement
#[async_trait]
pub trait Component: Send + Sync {
    /// Get the name of the component
    fn name(&self) -> &'static str;
    
    /// Initialize the component
    async fn init(&self, ctx: &serenity::Context, config: Arc<RwLock<Config>>) -> BotResult<()>;
    
    /// Shutdown the component
    async fn shutdown(&self) -> BotResult<()>;
}

/// Manager for all components
pub struct ComponentManager {
    components: Vec<Box<dyn Component>>,
    config: Arc<RwLock<Config>>,
}

impl ComponentManager {
    /// Create a new component manager
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        Self {
            components: Vec::new(),
            config,
        }
    }
    
    /// Get the configuration
    pub fn get_config(&self) -> Arc<RwLock<Config>> {
        Arc::clone(&self.config)
    }
    
    /// Register a component
    pub fn register<T: Component + 'static>(&mut self, component: T) {
        info!("Registering component: {}", component.name());
        self.components.push(Box::new(component));
    }
    
    /// Initialize all components
    pub async fn init_all(&self, ctx: &serenity::Context) -> BotResult<()> {
        info!("Initializing all components");
        
        for component in &self.components {
            let config = Arc::clone(&self.config);
            info!("Initializing component: {}", component.name());
            
            if let Err(e) = component.init(ctx, config).await {
                // Log error but continue with other components
                tracing::error!("Error initializing component {}: {:?}", component.name(), e);
            }
        }
        
        Ok(())
    }
    
    /// Shutdown all components
    pub async fn shutdown_all(&self) -> BotResult<()> {
        info!("Shutting down all components");
        
        for component in &self.components {
            info!("Shutting down component: {}", component.name());
            
            if let Err(e) = component.shutdown().await {
                // Log error but continue with other components
                tracing::error!("Error shutting down component {}: {:?}", component.name(), e);
            }
        }
        
        Ok(())
    }
} 