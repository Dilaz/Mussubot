use crate::error::{work_schedule_error, BotResult};
use chrono::{Datelike, Local, NaiveDateTime};

/// Parse time string in HH:MM format
pub fn parse_time(time_str: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let hour = parts[0].parse::<u32>().ok()?;
    let minute = parts[1].parse::<u32>().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some((hour, minute))
}

/// Calculate next daily notification time
pub fn next_daily_time(
    current_time: &chrono::DateTime<Local>,
    time_str: &str,
) -> Option<NaiveDateTime> {
    let (hour, minute) = parse_time(time_str)?;

    // Create a datetime for today at the specified time
    let mut next_time = current_time.date_naive().and_hms_opt(hour, minute, 0)?;

    // If the time has already passed today, schedule for tomorrow
    if current_time.naive_local() >= next_time {
        next_time = next_time.checked_add_signed(chrono::Duration::days(1))?;
    }

    Some(next_time)
}

/// Calculate next weekly notification time (for Monday)
pub fn next_weekly_time(
    current_time: &chrono::DateTime<Local>,
    time_str: &str,
) -> Option<NaiveDateTime> {
    let (hour, minute) = parse_time(time_str)?;

    // Calculate days until next Monday
    let days_until_monday = (8 - current_time.weekday().num_days_from_monday() % 7) % 7;

    // Create a datetime for the next Monday at the specified time
    let mut next_time = current_time
        .date_naive()
        .checked_add_signed(chrono::Duration::days(days_until_monday as i64))?
        .and_hms_opt(hour, minute, 0)?;

    // If today is Monday but the time has passed, schedule for next Monday
    if days_until_monday == 0 && current_time.naive_local() >= next_time {
        next_time = next_time.checked_add_signed(chrono::Duration::days(7))?;
    }

    Some(next_time)
}

/// Calculate the next notification time (either daily or weekly)
pub fn calculate_next_notification(
    now: &chrono::DateTime<Local>,
    daily_time: &str,
    weekly_time: &str,
) -> BotResult<(String, NaiveDateTime)> {
    // Calculate next daily notification time
    let next_daily = next_daily_time(now, daily_time)
        .ok_or_else(|| work_schedule_error("Failed to calculate next daily notification time"))?;

    // Calculate next weekly notification time
    let next_weekly = next_weekly_time(now, weekly_time)
        .ok_or_else(|| work_schedule_error("Failed to calculate next weekly notification time"))?;

    // Determine which notification comes next
    if next_daily <= next_weekly {
        Ok(("daily".to_string(), next_daily))
    } else {
        Ok(("weekly".to_string(), next_weekly))
    }
}

/// Calculate the wait duration until the next notification
pub fn calculate_wait_duration(
    now: &chrono::DateTime<Local>,
    next_time: &NaiveDateTime,
) -> BotResult<i64> {
    let wait_duration = next_time.signed_duration_since(now.naive_local());
    let seconds = wait_duration.num_seconds();

    if seconds <= 0 {
        return Err(work_schedule_error("Invalid wait duration calculated"));
    }

    Ok(seconds)
}

/// Get date range for weekly schedule (Monday to Sunday)
pub fn get_weekly_date_range(now: &chrono::DateTime<Local>) -> (String, String) {
    // Calculate Monday of the current week
    let monday = now
        .date_naive()
        .checked_sub_signed(chrono::Duration::days(
            (now.weekday().num_days_from_monday() % 7) as i64,
        ))
        .unwrap_or_else(|| now.date_naive());

    // Calculate Sunday of the current week (Monday + 6 days)
    let sunday = monday
        .checked_add_signed(chrono::Duration::days(6))
        .unwrap_or(monday);

    // Format dates as YYYY-MM-DD
    let start_date = monday.format("%Y-%m-%d").to_string();
    let end_date = sunday.format("%Y-%m-%d").to_string();

    (start_date, end_date)
}
