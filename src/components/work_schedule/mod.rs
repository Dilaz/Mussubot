mod actor;
mod handle;
pub mod models;
mod notifications;
mod scheduler;
pub mod time;

pub use handle::WorkScheduleHandle;

use super::redis_service::RedisActorHandle;
use super::work_schedule::scheduler::WorkScheduleScheduler;
use crate::config::Config;
use crate::error::BotResult;
use crate::utils::scheduler::Scheduler;
use async_trait::async_trait;
use lazy_static::lazy_static;
use poise::serenity_prelude as serenity;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

lazy_static! {
    static ref SCHEDULER_STARTED: AtomicBool = AtomicBool::new(false);
}

/// Work Schedule component for tracking employee work hours
#[derive(Default)]
pub struct WorkSchedule {
    handle: RwLock<Option<WorkScheduleHandle>>,
    ctx: RwLock<Option<Arc<serenity::Context>>>,
}

impl WorkSchedule {
    /// Create a new Work Schedule component
    pub fn new() -> Self {
        Self {
            handle: RwLock::new(None),
            ctx: RwLock::new(None),
        }
    }

    /// Get the handle if it exists
    pub async fn get_handle(&self) -> Option<WorkScheduleHandle> {
        let handle_lock = self.handle.read().await;
        handle_lock.clone()
    }
}

#[async_trait]
impl super::Component for WorkSchedule {
    fn name(&self) -> &'static str {
        "work_schedule"
    }

    async fn init(
        &self,
        ctx: &serenity::Context,
        config: Arc<RwLock<Config>>,
        redis_handle: RedisActorHandle,
    ) -> BotResult<()> {
        // Store context for scheduler
        *self.ctx.write().await = Some(Arc::new(ctx.clone()));

        // Create a new handle if one doesn't exist
        let mut handle_lock = self.handle.write().await;
        if handle_lock.is_none() {
            // Pass the redis_handle to the WorkScheduleHandle
            *handle_lock = Some(WorkScheduleHandle::new(config.clone(), redis_handle));
        }

        // Get the handle and context for the scheduler
        let handle = handle_lock.as_ref().unwrap().clone();
        let ctx = Arc::new(ctx.clone());

        // Start the notification scheduler only if it hasn't been started yet
        if !SCHEDULER_STARTED.swap(true, Ordering::SeqCst) {
            info!("Starting Work Schedule notification scheduler");
            if let Err(e) = WorkScheduleScheduler::start(ctx, config, handle).await {
                error!("Failed to start Work Schedule scheduler: {}", e);
            }
        } else {
            warn!("Work Schedule scheduler is already running, skipping initialization");
        }

        Ok(())
    }

    async fn shutdown(&self) -> BotResult<()> {
        // Shutdown the handle if it exists
        let handle_lock = self.handle.read().await;
        if let Some(handle) = &*handle_lock {
            handle.shutdown().await?;
        }

        // Stop the scheduler
        let scheduler = WorkScheduleScheduler;
        scheduler.stop().await?;

        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
