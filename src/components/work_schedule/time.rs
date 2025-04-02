use crate::error::{work_schedule_error, BotResult};
use crate::utils::time;
use chrono::{Local, NaiveDateTime};

/// Calculate the next notification time (either daily or weekly)
pub fn calculate_next_notification(
    now: &chrono::DateTime<Local>,
    daily_time: &str,
    weekly_time: &str,
) -> BotResult<(String, NaiveDateTime)> {
    // Calculate next daily notification time
    let next_daily = time::next_daily_time(now, daily_time)
        .ok_or_else(|| work_schedule_error("Failed to calculate next daily notification time"))?;

    // Calculate next weekly notification time
    let next_weekly = time::next_weekly_time(now, weekly_time)
        .ok_or_else(|| work_schedule_error("Failed to calculate next weekly notification time"))?;

    // Determine which notification comes next
    if next_daily <= next_weekly {
        Ok(("daily".to_string(), next_daily))
    } else {
        Ok(("weekly".to_string(), next_weekly))
    }
}
