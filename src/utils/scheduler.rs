use chrono::{DateTime, Local};
use lazy_static::lazy_static;
use poise::serenity_prelude as serenity;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tokio::time::{sleep, sleep_until, Duration as TokioDuration, Instant};
use tracing::{debug, error};

use crate::config::Config;
use crate::error::BotResult;

lazy_static! {
    /// Track the last daily notification date by component type
    pub static ref LAST_DAILY_DATES: RwLock<HashMap<String, String>> = RwLock::new(HashMap::new());
    /// Track the last weekly notification start date by component type
    pub static ref LAST_WEEKLY_START_DATES: RwLock<HashMap<String, String>> = RwLock::new(HashMap::new());
    /// Flags to track if notifications have been sent by component type
    pub static ref DAILY_NOTIFICATIONS_SENT: RwLock<HashMap<String, bool>> = RwLock::new(HashMap::new());
    pub static ref WEEKLY_NOTIFICATIONS_SENT: RwLock<HashMap<String, bool>> = RwLock::new(HashMap::new());
}

/// Notification type
#[derive(Debug, Clone, PartialEq)]
pub enum NotificationType {
    Daily,
    Weekly,
}

/// Trait for component schedulers that handle periodic notifications
pub trait Scheduler: Send + 'static {
    /// The type of handle used by this scheduler
    type Handle: Clone + Send + Sync + 'static;

    /// The component type identifier
    #[allow(dead_code)]
    fn component_type() -> String;

    /// Start the scheduler with the necessary context
    fn start(
        ctx: Arc<serenity::Context>,
        config: Arc<RwLock<Config>>,
        handle: Self::Handle,
    ) -> Pin<Box<dyn Future<Output = BotResult<()>> + Send>>;

    /// Stop the scheduler gracefully
    fn stop(&self) -> Pin<Box<dyn Future<Output = BotResult<()>> + Send>>;
}

/// Common notification handler trait to be implemented by components
pub trait NotificationHandler: Send + Sync + 'static {
    /// Get the component type identifier
    fn component_type(&self) -> String;

    /// Send a daily notification
    fn send_daily_notification<'a>(
        &'a self,
        ctx: &'a serenity::Context,
        channel_id: u64,
    ) -> Pin<Box<dyn Future<Output = BotResult<()>> + Send + 'a>>;

    /// Send a weekly notification
    fn send_weekly_notification<'a>(
        &'a self,
        ctx: &'a serenity::Context,
        channel_id: u64,
    ) -> Pin<Box<dyn Future<Output = BotResult<()>> + Send + 'a>>;
}

/// Sleep until a target time (waking up slightly early and then waiting for exact time)
pub async fn sleep_until_target_time(target_time: DateTime<Local>) -> BotResult<()> {
    let target_utc = target_time.with_timezone(&chrono::Utc);

    // Calculate wait duration (aiming to wake up 2 seconds early)
    let wake_early_time = target_time - chrono::Duration::seconds(2);

    // Convert to system time for precise scheduling
    let wake_early_system_time =
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(wake_early_time.timestamp() as u64);

    // Calculate sleep duration
    let now_system_time = SystemTime::now();
    if let Ok(sleep_duration) = wake_early_system_time.duration_since(now_system_time) {
        debug!("Sleeping for {:?} (waking up early)", sleep_duration);
        sleep_until(Instant::now() + TokioDuration::from_secs_f64(sleep_duration.as_secs_f64()))
            .await;
    } else {
        error!("Target time is in the past, falling back to immediate execution");
        return Ok(());
    }

    // After waking up early, wait for the exact time
    let now = Local::now();
    let now_utc = now.with_timezone(&chrono::Utc);

    if now_utc < target_utc {
        if let Ok(remaining_duration) = (target_utc - now_utc).to_std() {
            debug!(
                "Woke up early, waiting additional {:?} to reach exact target time",
                remaining_duration
            );
            sleep(TokioDuration::from_secs_f64(
                remaining_duration.as_secs_f64(),
            ))
            .await;
        } else {
            // If conversion fails, wait a small amount of time
            sleep(TokioDuration::from_millis(500)).await;
        }
    }

    Ok(())
}

/// Check and update notification flags
pub async fn update_notification_flags(today: &str, week_start_date: &str, component_type: &str) {
    // Reset daily notification flag if it's a new day
    let mut daily_dates = LAST_DAILY_DATES.write().await;
    let last_daily_date = daily_dates.get(component_type).cloned().unwrap_or_default();
    if last_daily_date != today {
        debug!(
            "[{}] New day detected ({}), resetting daily notification flag",
            component_type, today
        );
        DAILY_NOTIFICATIONS_SENT
            .write()
            .await
            .insert(component_type.to_string(), false);
        daily_dates.insert(component_type.to_string(), today.to_string());
    }

    // Reset weekly notification flag if it's a new week
    let mut weekly_dates = LAST_WEEKLY_START_DATES.write().await;
    let last_weekly_start = weekly_dates
        .get(component_type)
        .cloned()
        .unwrap_or_default();
    if last_weekly_start != week_start_date {
        debug!(
            "[{}] New week detected (starting {}), resetting weekly notification flag",
            component_type, week_start_date
        );
        WEEKLY_NOTIFICATIONS_SENT
            .write()
            .await
            .insert(component_type.to_string(), false);
        weekly_dates.insert(component_type.to_string(), week_start_date.to_string());
    }
}

/// Try to claim a notification slot to prevent duplicates
pub async fn try_claim_notification(
    notification_type: NotificationType,
    component_type: &str,
) -> bool {
    match notification_type {
        NotificationType::Daily => {
            let mut daily_sent = DAILY_NOTIFICATIONS_SENT.write().await;
            if daily_sent.get(component_type).cloned().unwrap_or(false) {
                false
            } else {
                daily_sent.insert(component_type.to_string(), true);
                true
            }
        }
        NotificationType::Weekly => {
            let mut weekly_sent = WEEKLY_NOTIFICATIONS_SENT.write().await;
            if weekly_sent.get(component_type).cloned().unwrap_or(false) {
                false
            } else {
                weekly_sent.insert(component_type.to_string(), true);
                true
            }
        }
    }
}

/// Reset notification flag if sending failed
pub async fn reset_notification_flag(notification_type: NotificationType, component_type: &str) {
    match notification_type {
        NotificationType::Daily => {
            DAILY_NOTIFICATIONS_SENT
                .write()
                .await
                .insert(component_type.to_string(), false);
        }
        NotificationType::Weekly => {
            WEEKLY_NOTIFICATIONS_SENT
                .write()
                .await
                .insert(component_type.to_string(), false);
        }
    }
}

/// Check if a notification has already been sent
pub async fn is_notification_sent(
    notification_type: NotificationType,
    component_type: &str,
) -> bool {
    match notification_type {
        NotificationType::Daily => DAILY_NOTIFICATIONS_SENT
            .read()
            .await
            .get(component_type)
            .cloned()
            .unwrap_or(false),
        NotificationType::Weekly => WEEKLY_NOTIFICATIONS_SENT
            .read()
            .await
            .get(component_type)
            .cloned()
            .unwrap_or(false),
    }
}

/// Update the last sent date after successful notification
pub async fn update_last_sent_date(
    notification_type: NotificationType,
    date: &str,
    component_type: &str,
) {
    match notification_type {
        NotificationType::Daily => {
            LAST_DAILY_DATES
                .write()
                .await
                .insert(component_type.to_string(), date.to_string());
        }
        NotificationType::Weekly => {
            LAST_WEEKLY_START_DATES
                .write()
                .await
                .insert(component_type.to_string(), date.to_string());
        }
    }
}
