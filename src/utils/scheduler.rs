use chrono::{DateTime, Local};
use lazy_static::lazy_static;
use poise::serenity_prelude as serenity;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tokio::time::{sleep, sleep_until, Duration as TokioDuration, Instant};
use tracing::{debug, error};

use crate::config::Config;
use crate::error::BotResult;

lazy_static! {
    /// Track the last daily notification date
    pub static ref LAST_DAILY_DATE: RwLock<String> = RwLock::new(String::new());
    /// Track the last weekly notification start date
    pub static ref LAST_WEEKLY_START_DATE: RwLock<String> = RwLock::new(String::new());
    /// Flags to track if notifications have been sent
    pub static ref DAILY_NOTIFICATION_SENT: AtomicBool = AtomicBool::new(false);
    pub static ref WEEKLY_NOTIFICATION_SENT: AtomicBool = AtomicBool::new(false);
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
pub async fn update_notification_flags(today: &str, week_start_date: &str) {
    // Reset daily notification flag if it's a new day
    let last_daily_date = LAST_DAILY_DATE.read().await.clone();
    if last_daily_date != today {
        debug!(
            "New day detected ({}), resetting daily notification flag",
            today
        );
        DAILY_NOTIFICATION_SENT.store(false, Ordering::SeqCst);
        *LAST_DAILY_DATE.write().await = today.to_string();
    }

    // Reset weekly notification flag if it's a new week
    let last_weekly_start = LAST_WEEKLY_START_DATE.read().await.clone();
    if last_weekly_start != week_start_date {
        debug!(
            "New week detected (starting {}), resetting weekly notification flag",
            week_start_date
        );
        WEEKLY_NOTIFICATION_SENT.store(false, Ordering::SeqCst);
        *LAST_WEEKLY_START_DATE.write().await = week_start_date.to_string();
    }
}

/// Try to claim a notification slot to prevent duplicates
pub fn try_claim_notification(notification_type: NotificationType) -> bool {
    match notification_type {
        NotificationType::Daily => DAILY_NOTIFICATION_SENT
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok(),
        NotificationType::Weekly => WEEKLY_NOTIFICATION_SENT
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok(),
    }
}

/// Reset notification flag if sending failed
pub fn reset_notification_flag(notification_type: NotificationType) {
    match notification_type {
        NotificationType::Daily => DAILY_NOTIFICATION_SENT.store(false, Ordering::SeqCst),
        NotificationType::Weekly => WEEKLY_NOTIFICATION_SENT.store(false, Ordering::SeqCst),
    }
}

/// Check if a notification has already been sent
pub fn is_notification_sent(notification_type: NotificationType) -> bool {
    match notification_type {
        NotificationType::Daily => DAILY_NOTIFICATION_SENT.load(Ordering::SeqCst),
        NotificationType::Weekly => WEEKLY_NOTIFICATION_SENT.load(Ordering::SeqCst),
    }
}

/// Update the last sent date after successful notification
pub async fn update_last_sent_date(notification_type: NotificationType, date: &str) {
    match notification_type {
        NotificationType::Daily => {
            *LAST_DAILY_DATE.write().await = date.to_string();
        }
        NotificationType::Weekly => {
            *LAST_WEEKLY_START_DATE.write().await = date.to_string();
        }
    }
}
