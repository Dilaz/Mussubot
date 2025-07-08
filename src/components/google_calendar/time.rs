use super::models::CalendarEvent;
use crate::error::{google_calendar_error, BotResult};
use crate::utils::time;
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone};

/// Calculate next notification time
pub fn next_notification_time(
    current_time: DateTime<Local>,
    target_time: &str,
    is_weekly: bool,
) -> BotResult<DateTime<Local>> {
    let result = time::next_notification_time(current_time, target_time, is_weekly)
        .ok_or_else(|| google_calendar_error("Failed to calculate next notification time"))?;

    Ok(result)
}

/// Get event start time as DateTime
pub fn get_event_start(event: &CalendarEvent) -> BotResult<Option<DateTime<Local>>> {
    if let Some(start_time) = &event.start_date_time {
        let dt = NaiveDateTime::parse_from_str(start_time, "%Y-%m-%dT%H:%M:%S%z")
            .map_err(|e| google_calendar_error(&format!("Failed to parse datetime: {e}")))?;
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
            .map_err(|e| google_calendar_error(&format!("Failed to parse date: {e}")))?;
        let dt = date
            .and_hms_opt(0, 0, 0)
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
