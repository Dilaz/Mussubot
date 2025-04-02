use crate::error::BotResult;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDateTime, TimeZone};

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
pub fn next_daily_time(current_time: &DateTime<Local>, time_str: &str) -> Option<NaiveDateTime> {
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
pub fn next_weekly_time(current_time: &DateTime<Local>, time_str: &str) -> Option<NaiveDateTime> {
    let (hour, minute) = parse_time(time_str)?;

    // Calculate days until next Monday
    let days_until_monday = (7 - current_time.weekday().num_days_from_monday()) % 7;

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

/// Calculate next notification time (generic version for calendar)
pub fn next_notification_time(
    current_time: DateTime<Local>,
    target_time: &str,
    is_weekly: bool,
) -> Option<DateTime<Local>> {
    let (target_hour, target_minute) = parse_time(target_time)?;

    let next = current_time
        .date_naive()
        .and_hms_opt(target_hour, target_minute, 0)?;

    let mut next = match Local.from_local_datetime(&next) {
        chrono::LocalResult::Single(dt) => dt,
        _ => return None,
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

    Some(next)
}

/// Calculate the wait duration until the next notification
pub fn calculate_wait_duration(now: &DateTime<Local>, next_time: &NaiveDateTime) -> BotResult<i64> {
    let wait_duration = next_time.signed_duration_since(now.naive_local());
    let seconds = wait_duration.num_seconds();

    if seconds <= 0 {
        // Instead of returning an error, we'll set a minimum wait time (1 minute)
        // This handles cases where the time calculation is very close to the scheduled time
        return Ok(60);
    }

    Ok(seconds)
}

/// Get date range for weekly schedule (Monday to Sunday)
pub fn get_weekly_date_range(now: &DateTime<Local>) -> (String, String) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone};

    #[test]
    fn test_parse_time() {
        // Valid cases
        assert_eq!(parse_time("00:00"), Some((0, 0)));
        assert_eq!(parse_time("12:30"), Some((12, 30)));
        assert_eq!(parse_time("23:59"), Some((23, 59)));

        // Invalid cases
        assert_eq!(parse_time("24:00"), None); // Hour out of range
        assert_eq!(parse_time("12:60"), None); // Minute out of range
        assert_eq!(parse_time("12:30:45"), None); // Too many parts
        assert_eq!(parse_time("12"), None); // Too few parts
        assert_eq!(parse_time("12:ab"), None); // Invalid minute
        assert_eq!(parse_time("ab:30"), None); // Invalid hour
    }

    #[test]
    fn test_next_daily_time() {
        // Sunday, 2023-01-01 at 10:00 AM
        let now = Local.with_ymd_and_hms(2023, 1, 1, 10, 0, 0).unwrap();

        // Test time later today
        let result = next_daily_time(&now, "15:30").unwrap();
        assert_eq!(
            result.format("%Y-%m-%d %H:%M").to_string(),
            "2023-01-01 15:30"
        );

        // Test time earlier today (should be tomorrow)
        let result = next_daily_time(&now, "09:30").unwrap();
        assert_eq!(
            result.format("%Y-%m-%d %H:%M").to_string(),
            "2023-01-02 09:30"
        );

        // Test exactly current time (should be tomorrow)
        let result = next_daily_time(&now, "10:00").unwrap();
        assert_eq!(
            result.format("%Y-%m-%d %H:%M").to_string(),
            "2023-01-02 10:00"
        );

        // Test invalid time
        assert_eq!(next_daily_time(&now, "25:00"), None);
    }

    #[test]
    fn test_next_weekly_time() {
        // Sunday, 2023-01-01 at 10:00 AM
        let sunday = Local.with_ymd_and_hms(2023, 1, 1, 10, 0, 0).unwrap();

        // Next Monday from Sunday
        let result = next_weekly_time(&sunday, "15:30").unwrap();
        assert_eq!(
            result.format("%Y-%m-%d %H:%M").to_string(),
            "2023-01-02 15:30"
        );

        // Monday, 2023-01-02 at 10:00 AM
        let monday = Local.with_ymd_and_hms(2023, 1, 2, 10, 0, 0).unwrap();

        // Test time later on Monday
        let result = next_weekly_time(&monday, "15:30").unwrap();
        assert_eq!(
            result.format("%Y-%m-%d %H:%M").to_string(),
            "2023-01-02 15:30"
        );

        // Test time earlier on Monday (should be next Monday)
        let result = next_weekly_time(&monday, "09:30").unwrap();
        assert_eq!(
            result.format("%Y-%m-%d %H:%M").to_string(),
            "2023-01-09 09:30"
        );

        // Wednesday, 2023-01-04 at 10:00 AM
        let wednesday = Local.with_ymd_and_hms(2023, 1, 4, 10, 0, 0).unwrap();

        // Next Monday from Wednesday
        let result = next_weekly_time(&wednesday, "15:30").unwrap();
        assert_eq!(
            result.format("%Y-%m-%d %H:%M").to_string(),
            "2023-01-09 15:30"
        );
    }

    #[test]
    fn test_next_notification_time() {
        // Sunday, 2023-01-01 at 10:00 AM
        let sunday = Local.with_ymd_and_hms(2023, 1, 1, 10, 0, 0).unwrap();

        // Daily notification, later today
        let result = next_notification_time(sunday, "15:30", false).unwrap();
        assert_eq!(
            result.format("%Y-%m-%d %H:%M").to_string(),
            "2023-01-01 15:30"
        );

        // Daily notification, earlier today (should be tomorrow)
        let result = next_notification_time(sunday, "09:30", false).unwrap();
        assert_eq!(
            result.format("%Y-%m-%d %H:%M").to_string(),
            "2023-01-02 09:30"
        );

        // Weekly notification on Monday
        let result = next_notification_time(sunday, "15:30", true).unwrap();
        assert_eq!(
            result.format("%Y-%m-%d %H:%M").to_string(),
            "2023-01-02 15:30"
        );

        // Wednesday, 2023-01-04
        let wednesday = Local.with_ymd_and_hms(2023, 1, 4, 10, 0, 0).unwrap();

        // Weekly notification from Wednesday (should be next Monday)
        let result = next_notification_time(wednesday, "15:30", true).unwrap();
        assert_eq!(
            result.format("%Y-%m-%d %H:%M").to_string(),
            "2023-01-09 15:30"
        );
    }

    #[test]
    fn test_calculate_wait_duration() {
        // Current time
        let now = Local.with_ymd_and_hms(2023, 1, 1, 10, 0, 0).unwrap();

        // Target time 1 hour later
        let target = now.naive_local() + Duration::hours(1);
        let wait = calculate_wait_duration(&now, &target).unwrap();
        assert_eq!(wait, 3600); // 3600 seconds = 1 hour

        // Target time 1 minute later
        let target = now.naive_local() + Duration::minutes(1);
        let wait = calculate_wait_duration(&now, &target).unwrap();
        assert_eq!(wait, 60); // 60 seconds = 1 minute

        // Target time in the past (should return minimum wait time)
        let target = now.naive_local() - Duration::minutes(5);
        let wait = calculate_wait_duration(&now, &target).unwrap();
        assert_eq!(wait, 60); // Minimum wait time of 60 seconds

        // Target time is calculated with next_notification_time
        let result = next_notification_time(now, "9:30", false).unwrap();
        let wait = calculate_wait_duration(&now, &result.naive_local()).unwrap();
        assert_eq!(wait, 23 * 3600 + 30 * 60);
    }

    #[test]
    fn test_get_weekly_date_range() {
        // Monday, 2023-01-02
        let monday = Local.with_ymd_and_hms(2023, 1, 2, 10, 0, 0).unwrap();
        let (start, end) = get_weekly_date_range(&monday);
        assert_eq!(start, "2023-01-02");
        assert_eq!(end, "2023-01-08");

        // Wednesday, 2023-01-04
        let wednesday = Local.with_ymd_and_hms(2023, 1, 4, 10, 0, 0).unwrap();
        let (start, end) = get_weekly_date_range(&wednesday);
        assert_eq!(start, "2023-01-02");
        assert_eq!(end, "2023-01-08");

        // Sunday, 2023-01-08
        let sunday = Local.with_ymd_and_hms(2023, 1, 8, 10, 0, 0).unwrap();
        let (start, end) = get_weekly_date_range(&sunday);
        assert_eq!(start, "2023-01-02");
        assert_eq!(end, "2023-01-08");
    }
}
