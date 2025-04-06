use chrono::NaiveTime;

/// Normalize a time string to the HH:MM format
pub fn normalize_time(time_str: &str) -> String {
    // Remove any extra whitespace
    let time_str = time_str.trim();

    // Replace commas with periods
    let time_str = time_str.replace(',', ".");

    // Try to parse as a time with or without a colon
    if time_str.contains(':') {
        // Time already has a colon, just format it properly
        if let Ok(time) = NaiveTime::parse_from_str(&time_str, "%H:%M") {
            return time.format("%H:%M").to_string();
        }
    } else if time_str.contains('.') {
        // Time has a period (e.g., "8.30")
        let parts: Vec<&str> = time_str.split('.').collect();
        if parts.len() == 2 {
            if let (Ok(hours), Ok(minutes)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                if hours < 24 && minutes < 60 {
                    return format!("{:02}:{:02}", hours, minutes);
                }
            }
        }
    } else {
        // Just a number (e.g., "8"), assume it's hours
        if let Ok(hours) = time_str.parse::<u32>() {
            if hours < 24 {
                return format!("{:02}:00", hours);
            }
        }
    }

    // If all parsing fails, return the original string
    time_str.to_string()
}
