use chrono::{Local, TimeZone, Utc};
use lazy_static::lazy_static;
use poise::serenity_prelude as serenity;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::{sleep, sleep_until, Duration as TokioDuration, Instant};
use tracing::{error, info, warn};

use super::handle::WorkScheduleHandle;
use super::notifications::{send_daily_notification, send_weekly_notification};
use super::time::calculate_next_notification;
use crate::config::Config;
use crate::error::BotResult;
use crate::utils::scheduler::Scheduler;
use crate::utils::time::{calculate_wait_duration, get_weekly_date_range};

lazy_static! {
    static ref SCHEDULER_INSTANCES: AtomicU32 = AtomicU32::new(0);
    static ref SCHEDULER_TASK_RUNNING: AtomicBool = AtomicBool::new(false);
    // Add static task storage for spawned task
    static ref SCHEDULER_TASK: RwLock<Option<JoinHandle<()>>> = RwLock::new(None);
}

/// Work Schedule scheduler implementation
pub struct WorkScheduleScheduler;

impl Default for WorkScheduleScheduler {
    fn default() -> Self {
        Self
    }
}

impl Scheduler for WorkScheduleScheduler {
    type Handle = WorkScheduleHandle;

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
                    "Multiple Work Schedule schedulers detected! Instance count: {}",
                    instance_count
                );
            }
            info!(
                "Starting Work Schedule scheduler (instance {})",
                instance_count
            );

            // Read config values
            let config_read = config.read().await;
            let daily_time = config_read.daily_notification_time.clone();
            let weekly_time = config_read.weekly_notification_time.clone();
            let channel_id = config_read.calendar_channel_id; // Reusing calendar channel for now
            drop(config_read);

            // Only spawn the scheduler task if it's not already running
            if !SCHEDULER_TASK_RUNNING.swap(true, Ordering::SeqCst) {
                info!("Starting Work Schedule notification task");

                // Clone values for the task
                let ctx_clone = Arc::clone(&ctx);
                let handle_clone = handle.clone();

                // Spawn the scheduler task
                let task = tokio::spawn(async move {
                    run_scheduler_loop(
                        ctx_clone,
                        &daily_time,
                        &weekly_time,
                        channel_id,
                        handle_clone,
                    )
                    .await;
                });

                // Store the task handle in the static storage
                *SCHEDULER_TASK.write().await = Some(task);
            } else {
                warn!(
                    "Work Schedule notification task is already running, skipping initialization"
                );
            }

            Ok(())
        })
    }

    /// Stop the scheduler gracefully
    fn stop(&self) -> Pin<Box<dyn Future<Output = BotResult<()>> + Send>> {
        Box::pin(async {
            // Abort the scheduler task if it exists
            if let Some(task) = SCHEDULER_TASK.write().await.take() {
                info!("Aborting Work Schedule notification task");
                task.abort();
                SCHEDULER_TASK_RUNNING.store(false, Ordering::SeqCst);
            }

            info!("Work Schedule scheduler stopped");
            Ok(())
        })
    }
}

/// Main scheduler loop that handles notification timing and sending
async fn run_scheduler_loop(
    ctx: Arc<serenity::Context>,
    daily_time: &str,
    weekly_time: &str,
    channel_id: u64,
    handle: WorkScheduleHandle,
) {
    loop {
        // Get the current time
        let now = Local::now();

        // Calculate the next notification time
        let (notification_type, next_time) =
            match calculate_next_notification(&now, daily_time, weekly_time) {
                Ok(result) => result,
                Err(e) => {
                    error!("Error calculating next notification time: {}", e);
                    sleep(TokioDuration::from_secs(3600)).await; // Retry in an hour
                    continue;
                }
            };

        info!(
            "Next {} work schedule notification scheduled for {}",
            notification_type, next_time
        );

        // Calculate how long to wait
        let wait_seconds = match calculate_wait_duration(&now, &next_time) {
            Ok(seconds) => seconds,
            Err(e) => {
                error!("Error calculating wait duration: {}", e);
                3600 // Default to an hour if we can't calculate
            }
        };

        // Convert NaiveDateTime to SystemTime then to Instant for precise scheduling
        let utc_timestamp = Utc
            .from_local_datetime(&next_time)
            .single()
            .unwrap()
            .timestamp();
        let next_system_time =
            SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(utc_timestamp as u64);
        if let Ok(duration_since_epoch) = next_system_time.duration_since(SystemTime::UNIX_EPOCH) {
            let now_duration = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
                Ok(now_secs) => now_secs,
                Err(_) => {
                    error!("Failed to get current system time, falling back to relative sleep");
                    sleep(TokioDuration::from_secs(wait_seconds as u64)).await;
                    continue;
                }
            };

            // Calculate the exact instant to wake up
            let sleep_duration = if duration_since_epoch > now_duration {
                duration_since_epoch - now_duration
            } else {
                error!("Calculated time is in the past, scheduling retry in an hour");
                sleep(TokioDuration::from_secs(3600)).await;
                continue;
            };

            sleep_until(Instant::now() + TokioDuration::from_secs(sleep_duration.as_secs())).await;
        } else {
            error!("Failed to convert time, falling back to relative sleep");
            sleep(TokioDuration::from_secs(wait_seconds as u64)).await;
        }

        // Send the appropriate notification
        let now = Local::now();
        match notification_type.as_str() {
            "daily" => {
                // Generate today's date in YYYY-MM-DD format
                let today = now.format("%Y-%m-%d").to_string();

                if let Err(e) = send_daily_notification(&ctx, channel_id, &handle, &today).await {
                    error!("Failed to send daily work schedule notification: {}", e);
                }
            }
            "weekly" => {
                // Get date range for the week
                let (start_date, end_date) = get_weekly_date_range(&now);

                if let Err(e) =
                    send_weekly_notification(&ctx, channel_id, &handle, &start_date, &end_date)
                        .await
                {
                    error!("Failed to send weekly work schedule notification: {}", e);
                }
            }
            _ => {
                error!("Unknown notification type: {}", notification_type);
            }
        }
    }
}
