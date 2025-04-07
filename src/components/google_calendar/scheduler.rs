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
use tracing::{debug, error, info, warn};

use super::handle::GoogleCalendarHandle;
use super::notifications::{
    send_daily_notification, send_new_events_notification, send_weekly_notification,
};
use super::time::next_notification_time;
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
    static ref DAILY_WEEKLY_TASK_RUNNING: AtomicBool = AtomicBool::new(false);
    static ref NEW_EVENTS_TASK_RUNNING: AtomicBool = AtomicBool::new(false);
    static ref DAILY_WEEKLY_TASK: RwLock<Option<JoinHandle<()>>> = RwLock::new(None);
    static ref NEW_EVENTS_TASK: RwLock<Option<JoinHandle<()>>> = RwLock::new(None);
    // Track the last time new events were checked
    static ref LAST_NEW_EVENTS_CHECK: RwLock<i64> = RwLock::new(0);
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

    fn component_type() -> String {
        "google_calendar".to_string()
    }

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

            // Create the notification handler
            let notification_handler = GoogleCalendarNotificationHandler {
                handle: handle.clone(),
            };
            let notification_handler = Arc::new(notification_handler);

            // Spawn task for daily/weekly notifications
            let ctx_clone = Arc::clone(&ctx);
            let handler_clone = Arc::clone(&notification_handler);

            // Only spawn the daily/weekly task if it's not already running
            if !DAILY_WEEKLY_TASK_RUNNING.swap(true, Ordering::SeqCst) {
                info!("Starting daily/weekly notification task");
                let task = tokio::spawn(async move {
                    run_daily_weekly_task(
                        ctx_clone,
                        &daily_time,
                        &weekly_time,
                        channel_id,
                        handler_clone,
                    )
                    .await;
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
                    run_new_events_task(
                        ctx_clone,
                        channel_id,
                        handle_clone,
                        new_events_check_interval,
                    )
                    .await;
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

/// Google Calendar notification handler implementation
struct GoogleCalendarNotificationHandler {
    handle: GoogleCalendarHandle,
}

impl NotificationHandler for GoogleCalendarNotificationHandler {
    fn component_type(&self) -> String {
        "google_calendar".to_string()
    }

    fn send_daily_notification<'a>(
        &'a self,
        ctx: &'a serenity::Context,
        channel_id: u64,
    ) -> Pin<Box<dyn Future<Output = BotResult<()>> + Send + 'a>> {
        let handle = self.handle.clone();

        Box::pin(async move {
            info!("Sending daily calendar notification");
            send_daily_notification(ctx, channel_id, &handle).await
        })
    }

    fn send_weekly_notification<'a>(
        &'a self,
        ctx: &'a serenity::Context,
        channel_id: u64,
    ) -> Pin<Box<dyn Future<Output = BotResult<()>> + Send + 'a>> {
        let handle = self.handle.clone();

        Box::pin(async move {
            info!("Sending weekly calendar notification");
            send_weekly_notification(ctx, channel_id, &handle).await
        })
    }
}

/// The main loop for daily and weekly notifications
async fn run_daily_weekly_task(
    ctx: Arc<serenity::Context>,
    daily_time: &str,
    weekly_time: &str,
    channel_id: u64,
    handler: Arc<dyn NotificationHandler>,
) {
    loop {
        let now = Local::now();
        let today = now.format("%Y-%m-%d").to_string();
        let (week_start_date, _) = get_weekly_date_range(&now);

        // Get component type
        let component_type = handler.component_type();

        // Update notification flags
        update_notification_flags(&today, &week_start_date, &component_type).await;

        // Calculate next notification times
        let next_daily = match next_notification_time(now, daily_time, false) {
            Ok(time) => time,
            Err(e) => {
                error!("Failed to calculate next daily notification time: {}", e);
                sleep(TokioDuration::from_secs(3600)).await; // Retry in an hour
                continue;
            }
        };

        let next_weekly = match next_notification_time(now, weekly_time, true) {
            Ok(time) => time,
            Err(e) => {
                error!("Failed to calculate next weekly notification time: {}", e);
                sleep(TokioDuration::from_secs(3600)).await; // Retry in an hour
                continue;
            }
        };

        // Check if notifications were already sent
        let daily_sent = is_notification_sent(NotificationType::Daily, &component_type).await;
        let weekly_sent = is_notification_sent(NotificationType::Weekly, &component_type).await;

        // Check if the current day/week needs notifications, or if we need to wait
        let daily_today = next_daily.date_naive().format("%Y-%m-%d").to_string() == today;
        let weekly_this_week = {
            let (current_week_start, _) = get_weekly_date_range(&now);
            let (next_week_start, _) = get_weekly_date_range(&next_weekly);
            current_week_start == next_week_start
        };

        // Determine which notification comes next and needs to be sent
        let (next_type, next_time) = if daily_today && !daily_sent {
            (NotificationType::Daily, next_daily)
        } else if weekly_this_week && !weekly_sent {
            (NotificationType::Weekly, next_weekly)
        } else if next_daily <= next_weekly {
            (NotificationType::Daily, next_daily)
        } else {
            (NotificationType::Weekly, next_weekly)
        };

        info!(
            "[{}] Next {:?} notification scheduled for {}",
            component_type, next_type, next_time
        );

        // Sleep until the target time
        if let Err(e) = sleep_until_target_time(next_time).await {
            error!("Error while waiting for target time: {:?}", e);
            sleep(TokioDuration::from_secs(60)).await; // Wait a minute before retrying
            continue;
        }

        // After waking, determine which notification(s) to send
        let now = Local::now();

        // Determine if we should send notifications
        let send_daily = now >= next_daily
            && !is_notification_sent(NotificationType::Daily, &component_type).await;
        let send_weekly = now >= next_weekly
            && !is_notification_sent(NotificationType::Weekly, &component_type).await;

        // Handle daily notification
        if send_daily {
            if try_claim_notification(NotificationType::Daily, &component_type).await {
                info!("[{}] Sending daily calendar notification", component_type);

                if let Err(e) = handler.send_daily_notification(&ctx, channel_id).await {
                    error!(
                        "[{}] Failed to send daily notification: {}",
                        component_type, e
                    );
                    reset_notification_flag(NotificationType::Daily, &component_type).await;
                } else {
                    info!(
                        "[{}] Successfully sent daily calendar notification",
                        component_type
                    );
                    update_last_sent_date(NotificationType::Daily, &today, &component_type).await;
                }
            } else {
                info!(
                    "[{}] Daily notification already claimed by another instance",
                    component_type
                );
            }
        }

        // Handle weekly notification
        if send_weekly {
            if try_claim_notification(NotificationType::Weekly, &component_type).await {
                info!("[{}] Sending weekly calendar notification", component_type);

                if let Err(e) = handler.send_weekly_notification(&ctx, channel_id).await {
                    error!(
                        "[{}] Failed to send weekly notification: {}",
                        component_type, e
                    );
                    reset_notification_flag(NotificationType::Weekly, &component_type).await;
                } else {
                    info!(
                        "[{}] Successfully sent weekly calendar notification",
                        component_type
                    );
                    update_last_sent_date(
                        NotificationType::Weekly,
                        &week_start_date,
                        &component_type,
                    )
                    .await;
                }
            } else {
                info!(
                    "[{}] Weekly notification already claimed by another instance",
                    component_type
                );
            }
        }

        // Small pause after sending to prevent immediate recalculation
        sleep(TokioDuration::from_secs(5)).await;
    }
}

/// The task for checking and notifying about new events
async fn run_new_events_task(
    ctx: Arc<serenity::Context>,
    channel_id: u64,
    handle: GoogleCalendarHandle,
    check_interval: u64,
) {
    loop {
        // Get current timestamp in seconds
        let now = chrono::Utc::now().timestamp();
        let last_check = *LAST_NEW_EVENTS_CHECK.read().await;

        // Enforce minimum interval between checks to avoid rate limiting
        if now - last_check < check_interval as i64 / 2 {
            let wait_time = (check_interval as i64 / 2) - (now - last_check);
            debug!("Too soon for new events check. Waiting {}s", wait_time);
            sleep(TokioDuration::from_secs(wait_time as u64)).await;
        }

        // Update last check time before checking
        *LAST_NEW_EVENTS_CHECK.write().await = now;

        debug!("Checking for new calendar events");
        match handle.check_new_events().await {
            Ok(new_events) => {
                if !new_events.is_empty() {
                    info!("Found {} new calendar events", new_events.len());
                    if let Err(e) =
                        send_new_events_notification(&ctx, channel_id, &new_events).await
                    {
                        error!("Failed to send new events notification: {}", e);
                    } else {
                        info!(
                            "Successfully sent notification for {} new events",
                            new_events.len()
                        );
                    }
                } else {
                    debug!("No new calendar events found");
                }
            }
            Err(e) => {
                error!("Failed to check for new events: {}", e);
            }
        }

        // Wait for the configured interval before checking again
        debug!("Waiting {}s before next new events check", check_interval);
        sleep(TokioDuration::from_secs(check_interval)).await;
    }
}
