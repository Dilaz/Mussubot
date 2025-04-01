use chrono::Local;
use lazy_static::lazy_static;
use poise::serenity_prelude as serenity;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration as TokioDuration};
use tracing::{error, info, warn};

use super::handle::WorkScheduleHandle;
use super::notifications::{send_daily_notification, send_weekly_notification};
use super::time::{calculate_next_notification, calculate_wait_duration, get_weekly_date_range};
use crate::config::Config;

lazy_static! {
    static ref SCHEDULER_INSTANCES: AtomicU32 = AtomicU32::new(0);
    static ref SCHEDULER_TASK_RUNNING: AtomicBool = AtomicBool::new(false);
}

/// Start the notification scheduler
pub async fn start_scheduler(
    ctx: Arc<serenity::Context>,
    config: Arc<RwLock<Config>>,
    handle: WorkScheduleHandle,
) {
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
        let daily_time_clone = daily_time.clone();
        let weekly_time_clone = weekly_time.clone();
        let ctx_clone = Arc::clone(&ctx);
        let handle_clone = handle.clone();

        // Spawn the scheduler task
        tokio::spawn(async move {
            run_scheduler_loop(
                ctx_clone,
                daily_time_clone,
                weekly_time_clone,
                channel_id,
                handle_clone,
            )
            .await;
        });
    } else {
        warn!("Work Schedule notification task is already running, skipping initialization");
    }
}

/// Main scheduler loop that handles notification timing and sending
async fn run_scheduler_loop(
    ctx: Arc<serenity::Context>,
    daily_time: String,
    weekly_time: String,
    channel_id: u64,
    handle: WorkScheduleHandle,
) {
    loop {
        // Get the current time
        let now = Local::now();

        // Calculate the next notification time
        let (notification_type, next_time) =
            match calculate_next_notification(&now, &daily_time, &weekly_time) {
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

        // Wait until the next notification time
        sleep(TokioDuration::from_secs(wait_seconds as u64)).await;

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
