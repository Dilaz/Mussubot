use chrono::{Local, Datelike};
use poise::serenity_prelude as serenity;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration as TokioDuration};
use tracing::{error, info};

use super::handle::WorkScheduleHandle;
use super::notifications::{send_daily_notification, send_weekly_notification};
use crate::config::Config;

/// Start the notification scheduler
pub async fn start_scheduler(
    ctx: Arc<serenity::Context>,
    config: Arc<RwLock<Config>>,
    handle: WorkScheduleHandle,
) {
    let config = config.read().await;
    let daily_time = config.daily_notification_time.clone();
    let weekly_time = config.weekly_notification_time.clone();
    let channel_id = config.calendar_channel_id; // Reusing calendar channel for now
    drop(config);

    // Spawn task for daily/weekly work schedule notifications
    let ctx_clone = Arc::clone(&ctx);
    let handle_clone = handle.clone();
    tokio::spawn(async move {
        loop {
            // Calculate time until next daily notification
            let now = Local::now();
            let next_notification_hour = daily_time.split(':').next().unwrap_or("8").parse::<u32>().unwrap_or(8);
            let next_notification_minute = daily_time.split(':').nth(1).unwrap_or("0").parse::<u32>().unwrap_or(0);
            
            let mut next_notification = now.date_naive().and_hms_opt(
                next_notification_hour,
                next_notification_minute,
                0,
            ).unwrap_or_else(|| now.naive_local());
            
            // If the notification time has already passed today, schedule for tomorrow
            if now.naive_local() >= next_notification {
                next_notification = next_notification
                    .checked_add_signed(chrono::Duration::days(1))
                    .unwrap_or_else(|| now.naive_local());
            }
            
            // Calculate time until next weekly notification (Monday)
            let days_until_monday = (8 - now.weekday().num_days_from_monday() % 7) % 7;
            let next_weekly_notification_hour = weekly_time.split(':').next().unwrap_or("8").parse::<u32>().unwrap_or(8);
            let next_weekly_notification_minute = weekly_time.split(':').nth(1).unwrap_or("0").parse::<u32>().unwrap_or(0);
            
            let mut next_weekly_notification = now.date_naive()
                .checked_add_signed(chrono::Duration::days(days_until_monday as i64))
                .unwrap_or_else(|| now.date_naive())
                .and_hms_opt(
                    next_weekly_notification_hour,
                    next_weekly_notification_minute,
                    0,
                )
                .unwrap_or_else(|| now.naive_local());
            
            // If today is Monday but the notification time has passed, schedule for next Monday
            if days_until_monday == 0 && now.naive_local() >= next_weekly_notification {
                next_weekly_notification = next_weekly_notification
                    .checked_add_signed(chrono::Duration::days(7))
                    .unwrap_or_else(|| now.naive_local());
            }
            
            // Determine the next notification time (daily or weekly, whichever comes first)
            let next_notification = if next_notification.and_utc().timestamp() <= next_weekly_notification.and_utc().timestamp() {
                ("daily", next_notification)
            } else {
                ("weekly", next_weekly_notification)
            };
            
            info!(
                "Next {} work schedule notification scheduled for {}",
                next_notification.0,
                next_notification.1
            );
            
            // Calculate wait duration
            let wait_duration = next_notification.1.signed_duration_since(now.naive_local());
            if wait_duration.num_seconds() <= 0 {
                // If something went wrong with calculation, wait for 1 hour
                sleep(TokioDuration::from_secs(3600)).await;
                continue;
            }
            
            // Wait until the next notification time
            sleep(TokioDuration::from_secs(wait_duration.num_seconds() as u64)).await;
            
            // Check which notification to send
            let now = Local::now();
            if next_notification.0 == "daily" {
                // Generate today's date in YYYY-MM-DD format
                let today = now.format("%Y-%m-%d").to_string();
                
                if let Err(e) = send_daily_notification(&ctx_clone, channel_id, &handle_clone, &today).await {
                    error!("Failed to send daily work schedule notification: {}", e);
                }
            } else {
                // Calculate the date range for the week (Monday to Sunday)
                let monday = now.date_naive()
                    .checked_sub_signed(chrono::Duration::days((now.weekday().num_days_from_monday() % 7) as i64))
                    .unwrap_or_else(|| now.date_naive());
                
                let sunday = monday
                    .checked_add_signed(chrono::Duration::days(6))
                    .unwrap_or(monday);
                
                let start_date = monday.format("%Y-%m-%d").to_string();
                let end_date = sunday.format("%Y-%m-%d").to_string();
                
                if let Err(e) = send_weekly_notification(&ctx_clone, channel_id, &handle_clone, &start_date, &end_date).await {
                    error!("Failed to send weekly work schedule notification: {}", e);
                }
            }
        }
    });
} 