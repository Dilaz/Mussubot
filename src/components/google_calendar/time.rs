use crate::error::{BotResult, google_calendar_error};
use chrono::{DateTime, Duration, Local, NaiveDateTime, NaiveDate, TimeZone, Datelike};
use super::models::CalendarEvent;

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

/// Calculate next notification time
pub fn next_notification_time(current_time: DateTime<Local>, target_time: &str, is_weekly: bool) -> BotResult<DateTime<Local>> {
    let (target_hour, target_minute) = parse_time(target_time)
        .ok_or_else(|| google_calendar_error("Invalid time format"))?;

    let next = current_time
        .date_naive()
        .and_hms_opt(target_hour, target_minute, 0)
        .ok_or_else(|| google_calendar_error("Failed to create datetime"))?;
    let mut next = match Local.from_local_datetime(&next) {
        chrono::LocalResult::Single(dt) => dt,
        chrono::LocalResult::Ambiguous(_, _) => {
            return Err(google_calendar_error("Ambiguous local time"));
        }
        chrono::LocalResult::None => {
            return Err(google_calendar_error("Invalid local time"));
        }
    };

    // If we've already passed the target time today, move to tomorrow
    if next <= current_time {
        next += Duration::days(1);
    }

    // For weekly notifications, ensure it's on Monday
    if is_weekly {
        while next.weekday() != chrono::Weekday::Mon {
            next += Duration::days(1);
        }
    }

    Ok(next)
}

/// Get event start time as DateTime
pub fn get_event_start(event: &CalendarEvent) -> BotResult<Option<DateTime<Local>>> {
    if let Some(start_time) = &event.start_date_time {
        let dt = NaiveDateTime::parse_from_str(start_time, "%Y-%m-%dT%H:%M:%S%z")
            .map_err(|e| google_calendar_error(&format!("Failed to parse datetime: {}", e)))?;
        let local_dt = match Local.from_local_datetime(&dt) {
            chrono::LocalResult::Single(dt) => dt,
            chrono::LocalResult::Ambiguous(_, _) => {
                return Err(google_calendar_error("Ambiguous local time"));
            }
            chrono::LocalResult::None => {
                return Err(google_calendar_error("Invalid local time"));
            }
        };
        Ok(Some(local_dt))
    } else if let Some(start_date) = &event.start_date {
        let date = NaiveDate::parse_from_str(start_date, "%Y-%m-%d")
            .map_err(|e| google_calendar_error(&format!("Failed to parse date: {}", e)))?;
        let dt = date.and_hms_opt(0, 0, 0)
            .ok_or_else(|| google_calendar_error("Failed to create datetime"))?;
        let local_dt = match Local.from_local_datetime(&dt) {
            chrono::LocalResult::Single(dt) => dt,
            chrono::LocalResult::Ambiguous(_, _) => {
                return Err(google_calendar_error("Ambiguous local time"));
            }
            chrono::LocalResult::None => {
                return Err(google_calendar_error("Invalid local time"));
            }
        };
        Ok(Some(local_dt))
    } else {
        Ok(None)
    }
} 