use chrono::Local;
use poise::serenity_prelude as serenity;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration as TokioDuration};
use tracing::{error, info};

use super::handle::GoogleCalendarHandle;
use super::notifications::{send_daily_notification, send_weekly_notification, send_new_events_notification};
use super::time::next_notification_time;
use crate::config::Config;

/// Start the notification scheduler
pub async fn start_scheduler(
    ctx: Arc<serenity::Context>,
    config: Arc<RwLock<Config>>,
    handle: GoogleCalendarHandle,
) {
    let config = config.read().await;
    let daily_time = config.daily_notification_time.clone();
    let weekly_time = config.weekly_notification_time.clone();
    let channel_id = config.calendar_channel_id;
    drop(config);

    // Spawn task for daily/weekly notifications
    let ctx_clone = Arc::clone(&ctx);
    let handle_clone = handle.clone();
    tokio::spawn(async move {
        loop {
            let now = Local::now();
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
            
            let next = next_daily.min(next_weekly);
            let wait_duration = next - now;
            
            info!("Next notification scheduled for {}", next);
            sleep(TokioDuration::from_secs(wait_duration.num_seconds() as u64)).await;

            let now = Local::now();
            if now >= next_daily {
                if let Err(e) = send_daily_notification(&ctx_clone, channel_id, &handle_clone).await {
                    error!("Failed to send daily notification: {}", e);
                }
            }
            
            if now >= next_weekly {
                if let Err(e) = send_weekly_notification(&ctx_clone, channel_id, &handle_clone).await {
                    error!("Failed to send weekly notification: {}", e);
                }
            }
        }
    });

    // Spawn task for checking new events
    let ctx_clone = Arc::clone(&ctx);
    let handle_clone = handle.clone();
    tokio::spawn(async move {
        loop {
            // Check for new events every 5 minutes
            sleep(TokioDuration::from_secs(300)).await;

            match handle_clone.check_new_events().await {
                Ok(new_events) => {
                    if let Err(e) = send_new_events_notification(&ctx_clone, channel_id, &new_events).await {
                        error!("Failed to send new events notification: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to check for new events: {}", e);
                }
            }
        }
    });
} 