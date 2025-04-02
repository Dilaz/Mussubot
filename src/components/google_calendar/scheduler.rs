use chrono::Local;
use lazy_static::lazy_static;
use poise::serenity_prelude as serenity;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration as TokioDuration};
use tracing::{error, info, warn};

use super::handle::GoogleCalendarHandle;
use super::notifications::{
    send_daily_notification, send_new_events_notification, send_weekly_notification,
};
use super::time::next_notification_time;
use crate::config::Config;
use crate::error::BotResult;
use crate::utils::scheduler::Scheduler;

lazy_static! {
    static ref SCHEDULER_INSTANCES: AtomicU32 = AtomicU32::new(0);
    static ref DAILY_WEEKLY_TASK_RUNNING: AtomicBool = AtomicBool::new(false);
    static ref NEW_EVENTS_TASK_RUNNING: AtomicBool = AtomicBool::new(false);
    // Add static task storage for spawned tasks
    static ref DAILY_WEEKLY_TASK: RwLock<Option<JoinHandle<()>>> = RwLock::new(None);
    static ref NEW_EVENTS_TASK: RwLock<Option<JoinHandle<()>>> = RwLock::new(None);
}

/// Google Calendar scheduler implementation
pub struct GoogleCalendarScheduler;

impl Default for GoogleCalendarScheduler {
    fn default() -> Self {
        Self
    }
}

impl Scheduler for GoogleCalendarScheduler {
    type Handle = GoogleCalendarHandle;

    /// Start the notification scheduler
    fn start(
        ctx: Arc<serenity::Context>,
        config: Arc<RwLock<Config>>,
        handle: Self::Handle,
    ) -> Pin<Box<dyn Future<Output = BotResult<()>> + Send>> {
        Box::pin(async move {
            // Increment instance counter and log
            let instance_count = SCHEDULER_INSTANCES.fetch_add(1, Ordering::SeqCst) + 1;
            if instance_count > 1 {
                warn!(
                    "Multiple Google Calendar schedulers detected! Instance count: {}",
                    instance_count
                );
            }
            info!(
                "Starting Google Calendar scheduler (instance {})",
                instance_count
            );

            // Read config values
            let config_read = config.read().await;
            let daily_time = config_read.daily_notification_time.clone();
            let weekly_time = config_read.weekly_notification_time.clone();
            let channel_id = config_read.calendar_channel_id;

            // Get the new events check interval
            let new_events_check_interval = config_read.new_events_check_interval;
            drop(config_read);

            // Spawn task for daily/weekly notifications
            let ctx_clone = Arc::clone(&ctx);
            let handle_clone = handle.clone();

            // Only spawn the daily/weekly task if it's not already running
            if !DAILY_WEEKLY_TASK_RUNNING.swap(true, Ordering::SeqCst) {
                info!("Starting daily/weekly notification task");
                let task = tokio::spawn(async move {
                    loop {
                        let now = Local::now();

                        // Calculate next notification times
                        let next_daily = match next_notification_time(now, &daily_time, false) {
                            Ok(time) => time,
                            Err(e) => {
                                error!("Failed to calculate next daily notification time: {}", e);
                                sleep(TokioDuration::from_secs(3600)).await; // Retry in an hour
                                continue;
                            }
                        };

                        let next_weekly = match next_notification_time(now, &weekly_time, true) {
                            Ok(time) => time,
                            Err(e) => {
                                error!("Failed to calculate next weekly notification time: {}", e);
                                sleep(TokioDuration::from_secs(3600)).await; // Retry in an hour
                                continue;
                            }
                        };

                        // Determine which notification comes next
                        let next = next_daily.min(next_weekly);
                        let wait_duration = next - now;

                        if wait_duration.num_seconds() <= 0 {
                            // If calculation resulted in negative time, wait an hour and retry
                            error!("Invalid wait duration calculated. Retrying in an hour.");
                            sleep(TokioDuration::from_secs(3600)).await;
                            continue;
                        }

                        info!("Next notification scheduled for {}", next);
                        sleep(TokioDuration::from_secs(wait_duration.num_seconds() as u64)).await;

                        // After waiting, check which notifications to send
                        let now = Local::now();

                        // Send daily notification if it's time
                        if now >= next_daily {
                            if let Err(e) =
                                send_daily_notification(&ctx_clone, channel_id, &handle_clone).await
                            {
                                error!("Failed to send daily notification: {}", e);
                            }
                        }

                        // Send weekly notification if it's time
                        if now >= next_weekly {
                            if let Err(e) =
                                send_weekly_notification(&ctx_clone, channel_id, &handle_clone)
                                    .await
                            {
                                error!("Failed to send weekly notification: {}", e);
                            }
                        }
                    }
                });

                // Store the task handle in the static storage
                *DAILY_WEEKLY_TASK.write().await = Some(task);
            } else {
                warn!("Daily/weekly notification task is already running, skipping initialization");
            }

            // Spawn task for checking new events
            let ctx_clone = Arc::clone(&ctx);
            let handle_clone = handle.clone();

            // Only spawn the new events task if it's not already running
            if !NEW_EVENTS_TASK_RUNNING.swap(true, Ordering::SeqCst) {
                info!("Starting new events check task");
                let task = tokio::spawn(async move {
                    loop {
                        // Check for new events at the configured interval
                        sleep(TokioDuration::from_secs(new_events_check_interval)).await;

                        match handle_clone.check_new_events().await {
                            Ok(new_events) => {
                                if !new_events.is_empty() {
                                    if let Err(e) = send_new_events_notification(
                                        &ctx_clone,
                                        channel_id,
                                        &new_events,
                                    )
                                    .await
                                    {
                                        error!("Failed to send new events notification: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to check for new events: {}", e);
                            }
                        }
                    }
                });

                // Store the task handle in the static storage
                *NEW_EVENTS_TASK.write().await = Some(task);
            } else {
                warn!("New events check task is already running, skipping initialization");
            }

            Ok(())
        })
    }

    /// Stop the scheduler gracefully
    fn stop(&self) -> Pin<Box<dyn Future<Output = BotResult<()>> + Send>> {
        Box::pin(async {
            // Abort the daily/weekly task if it exists
            if let Some(task) = DAILY_WEEKLY_TASK.write().await.take() {
                info!("Aborting daily/weekly notification task");
                task.abort();
                DAILY_WEEKLY_TASK_RUNNING.store(false, Ordering::SeqCst);
            }

            // Abort the new events task if it exists
            if let Some(task) = NEW_EVENTS_TASK.write().await.take() {
                info!("Aborting new events check task");
                task.abort();
                NEW_EVENTS_TASK_RUNNING.store(false, Ordering::SeqCst);
            }

            info!("Google Calendar scheduler stopped");
            Ok(())
        })
    }
}
