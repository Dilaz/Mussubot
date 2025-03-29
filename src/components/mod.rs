use crate::components::redis_service::RedisActorHandle;
use crate::config::Config;
use crate::error::BotResult;
use async_trait::async_trait;
use poise::serenity_prelude as serenity;
use std::any::Any;
use std::fmt;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

// Export components
pub mod google_calendar;
pub mod redis_service;
pub mod work_schedule;

// Re-export Google Calendar handle
pub use google_calendar::GoogleCalendarHandle;
// Re-export Work Schedule handle
pub use work_schedule::WorkScheduleHandle;

/// Component trait that all components must implement
#[async_trait]
pub trait Component: Send + Sync + Any {
    /// Get the name of the component
    fn name(&self) -> &'static str;

    /// Initialize the component
    async fn init(
        &self,
        ctx: &serenity::Context,
        config: Arc<RwLock<Config>>,
        redis_handle: RedisActorHandle,
    ) -> BotResult<()>;

    /// Shutdown the component
    async fn shutdown(&self) -> BotResult<()>;

    /// Convert to Any for downcasting
    fn as_any(&self) -> &dyn Any;
}

/// Manager for all components
pub struct ComponentManager {
    components: Vec<Box<dyn Component>>,
    config: Arc<RwLock<Config>>,
}

impl fmt::Debug for ComponentManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ComponentManager")
            .field("component_count", &self.components.len())
            .field("config", &self.config)
            .finish()
    }
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
    #[allow(dead_code)]
    pub fn get_config(&self) -> Arc<RwLock<Config>> {
        Arc::clone(&self.config)
    }

    /// Register a component
    pub fn register<T: Component + 'static>(&mut self, component: T) {
        info!("Registering component: {}", component.name());
        self.components.push(Box::new(component));
    }

    /// Initialize all registered components
    pub async fn init_all(
        &self,
        ctx: &serenity::Context,
        config: Arc<RwLock<Config>>,
        redis_handle: RedisActorHandle,
    ) -> BotResult<()> {
        for component in &self.components {
            info!("Initializing component: {}", component.name());

            if let Err(e) = component
                .init(ctx, config.clone(), redis_handle.clone())
                .await
            {
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
                tracing::error!(
                    "Error shutting down component {}: {:?}",
                    component.name(),
                    e
                );
            }
        }

        Ok(())
    }

    /// Get a component by name
    pub fn get_component_by_name(&self, name: &str) -> Option<&dyn Component> {
        self.components
            .iter()
            .find(|c| c.name() == name)
            .map(|c| c.as_ref())
    }
}
