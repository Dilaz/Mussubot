mod actor;
mod handle;
pub mod models;
pub mod token;
mod time;
mod notifications;
mod scheduler;

pub use handle::GoogleCalendarHandle;
pub use models::CalendarEvent;

use crate::config::Config;
use crate::error::BotResult;
use async_trait::async_trait;
use poise::serenity_prelude as serenity;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::google_calendar::scheduler::start_scheduler;
use super::redis_service::RedisActorHandle;

/// Google Calendar component for integration with Discord
#[derive(Default)]
pub struct GoogleCalendar {
    handle: RwLock<Option<GoogleCalendarHandle>>,
    ctx: RwLock<Option<Arc<serenity::Context>>>,
}

impl GoogleCalendar {
    /// Create a new Google Calendar component
    pub fn new() -> Self {
        Self {
            handle: RwLock::new(None),
            ctx: RwLock::new(None),
        }
    }
    
    /// Get the handle if it exists
    pub async fn get_handle(&self) -> Option<GoogleCalendarHandle> {
        let handle_lock = self.handle.read().await;
        handle_lock.clone()
    }
}

#[async_trait]
impl super::Component for GoogleCalendar {
    fn name(&self) -> &'static str {
        "google_calendar"
    }
    
    async fn init(&self, ctx: &serenity::Context, config: Arc<RwLock<Config>>, redis_handle: RedisActorHandle) -> BotResult<()> {
        // Store context for scheduler
        *self.ctx.write().await = Some(Arc::new(ctx.clone()));
        
        // Create a new handle if one doesn't exist
        let mut handle_lock = self.handle.write().await;
        if handle_lock.is_none() {
            // Pass the redis_handle to the GoogleCalendarHandle
            *handle_lock = Some(GoogleCalendarHandle::new(config.clone(), redis_handle));
        }

        // Get the handle and context for the scheduler
        let handle = handle_lock.as_ref().unwrap().clone();
        let ctx = Arc::new(ctx.clone());

        // Start the notification scheduler
        start_scheduler(ctx, config, handle).await;
        
        Ok(())
    }
    
    async fn shutdown(&self) -> BotResult<()> {
        // Shutdown the handle if it exists
        let handle_lock = self.handle.read().await;
        if let Some(handle) = &*handle_lock {
            handle.shutdown().await?;
        }
        Ok(())
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
