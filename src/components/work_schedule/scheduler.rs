use chrono::{Local, TimeZone};
use lazy_static::lazy_static;
use poise::serenity_prelude as serenity;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration as TokioDuration};
use tracing::{debug, error, info, warn};

use super::handle::WorkScheduleHandle;
use super::notifications::{send_daily_notification, send_weekly_notification};
use super::time::calculate_next_notification;
use crate::config::Config;
use crate::error::BotResult;
use crate::utils::scheduler::{
    is_notification_sent, reset_notification_flag, sleep_until_target_time, try_claim_notification,
    update_last_sent_date, update_notification_flags, NotificationHandler, NotificationType,
    Scheduler,
};
use crate::utils::time::get_weekly_date_range;

lazy_static! {
    static ref SCHEDULER_INSTANCES: AtomicU32 = AtomicU32::new(0);
    static ref SCHEDULER_TASK_RUNNING: AtomicBool = AtomicBool::new(false);
    static ref SCHEDULER_TASK: RwLock<Option<JoinHandle<()>>> = RwLock::new(None);
}

/// Work Schedule scheduler implementation
pub struct WorkScheduleScheduler;

impl Default for WorkScheduleScheduler {
    fn default() -> Self {
        Self
    }
}

/// WorkSchedule notification handler implementation
struct WorkScheduleNotificationHandler {
    handle: WorkScheduleHandle,
}

impl NotificationHandler for WorkScheduleNotificationHandler {
    fn send_daily_notification<'a>(
        &'a self,
        ctx: &'a serenity::Context,
        channel_id: u64,
    ) -> Pin<Box<dyn Future<Output = BotResult<()>> + Send + 'a>> {
        let handle = self.handle.clone();

        Box::pin(async move {
            let today = Local::now().format("%Y-%m-%d").to_string();
            info!("Sending daily work schedule notification for {}", today);
            send_daily_notification(ctx, channel_id, &handle, &today).await
        })
    }

    fn send_weekly_notification<'a>(
        &'a self,
        ctx: &'a serenity::Context,
        channel_id: u64,
    ) -> Pin<Box<dyn Future<Output = BotResult<()>> + Send + 'a>> {
        let handle = self.handle.clone();

        Box::pin(async move {
            let now = Local::now();
            let (start_date, end_date) = get_weekly_date_range(&now);
            info!(
                "Sending weekly work schedule notification for {} to {}",
                start_date, end_date
            );
            send_weekly_notification(ctx, channel_id, &handle, &start_date, &end_date).await
        })
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

                // Create the notification handler
                let notification_handler = WorkScheduleNotificationHandler {
                    handle: handle.clone(),
                };
                let notification_handler = Arc::new(notification_handler);

                // Clone values for the task
                let ctx_clone = Arc::clone(&ctx);
                let daily_time = daily_time.clone();
                let weekly_time = weekly_time.clone();

                // Spawn the scheduler task
                let task = tokio::spawn(async move {
                    run_scheduler_loop(
                        ctx_clone,
                        &daily_time,
                        &weekly_time,
                        channel_id,
                        notification_handler,
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
    handler: Arc<dyn NotificationHandler>,
) {
    loop {
        // Get the current time
        let now = Local::now();
        let today = now.format("%Y-%m-%d").to_string();
        let (week_start_date, _) = get_weekly_date_range(&now);

        // Update flags based on current date
        update_notification_flags(&today, &week_start_date).await;

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

        // Convert string notification type to enum
        let notification_type_enum = match notification_type.as_str() {
            "daily" => NotificationType::Daily,
            "weekly" => NotificationType::Weekly,
            _ => {
                error!("Unknown notification type: {}", notification_type);
                sleep(TokioDuration::from_secs(3600)).await; // Retry in an hour
                continue;
            }
        };

        // Check if notification was already sent
        if is_notification_sent(notification_type_enum.clone()) {
            debug!(
                "{:?} notification for {} already sent, recalculating next notification time",
                notification_type_enum,
                if matches!(notification_type_enum, NotificationType::Daily) {
                    &today
                } else {
                    &week_start_date
                }
            );
            sleep(TokioDuration::from_secs(60)).await; // Wait a minute before recalculating
            continue;
        }

        info!(
            "Next {:?} work schedule notification scheduled for {}",
            notification_type_enum, next_time
        );

        // Convert NaiveDateTime to DateTime<Local> for the sleep_until_target_time function
        let local_time = match Local.from_local_datetime(&next_time) {
            chrono::LocalResult::Single(dt) => dt,
            _ => {
                error!("Failed to convert NaiveDateTime to DateTime<Local>, using current time");
                Local::now()
            }
        };

        // Sleep until the target time
        if let Err(e) = sleep_until_target_time(local_time).await {
            error!("Error while waiting for target time: {:?}", e);
            sleep(TokioDuration::from_secs(60)).await; // Wait a minute before retrying
            continue;
        }

        // Try to claim the notification
        if !try_claim_notification(notification_type_enum.clone()) {
            info!(
                "{:?} notification already claimed by another instance",
                notification_type_enum
            );
            sleep(TokioDuration::from_secs(10)).await; // Short wait before loop continues
            continue;
        }

        // Send the appropriate notification
        let result = match notification_type_enum {
            NotificationType::Daily => handler.send_daily_notification(&ctx, channel_id).await,
            NotificationType::Weekly => handler.send_weekly_notification(&ctx, channel_id).await,
        };

        // Handle the notification result
        if let Err(e) = result {
            error!(
                "Failed to send {:?} work schedule notification: {}",
                notification_type_enum, e
            );
            // Reset the flag if sending failed so we can try again
            reset_notification_flag(notification_type_enum);
        } else {
            info!(
                "Successfully sent {:?} work schedule notification",
                notification_type_enum
            );
            // Update the last sent date
            let date = match notification_type_enum {
                NotificationType::Daily => today.clone(),
                NotificationType::Weekly => week_start_date.clone(),
            };
            update_last_sent_date(notification_type_enum, &date).await;
        }

        // Small pause after sending to prevent immediate recalculation
        sleep(TokioDuration::from_secs(5)).await;
    }
}
