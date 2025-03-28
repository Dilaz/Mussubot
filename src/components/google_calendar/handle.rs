use super::actor::GoogleCalendarActorHandle;
use super::models::CalendarEvent;
use crate::components::redis_service::RedisActorHandle;
use crate::config::Config;
use crate::error::BotResult;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

/// Handle for interacting with the Google Calendar actor
#[derive(Clone)]
pub struct GoogleCalendarHandle {
    actor_handle: GoogleCalendarActorHandle,
    _actor_task: Arc<JoinHandle<()>>,
}

impl GoogleCalendarHandle {
    /// Create a new GoogleCalendarHandle and spawn the actor
    pub fn new(config: Arc<RwLock<Config>>, redis_handle: RedisActorHandle) -> Self {
        use super::actor::GoogleCalendarActor;

        // Create the actor and get its handle
        let (mut actor, handle) = GoogleCalendarActor::new(config, redis_handle);

        // Spawn a task to run the actor
        let actor_task = tokio::spawn(async move {
            actor.run().await;
        });

        Self {
            actor_handle: handle,
            _actor_task: Arc::new(actor_task),
        }
    }

    /// Get upcoming events from the calendar
    pub async fn get_upcoming_events(&self) -> BotResult<Vec<CalendarEvent>> {
        self.actor_handle.get_upcoming_events().await
    }

    /// Check for new events since last check
    pub async fn check_new_events(&self) -> BotResult<Vec<CalendarEvent>> {
        self.actor_handle.check_new_events().await
    }

    /// Shutdown the actor
    pub async fn shutdown(&self) -> BotResult<()> {
        self.actor_handle.shutdown().await
    }
}
