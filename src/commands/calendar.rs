use crate::commands::{CommandResult, Context};
use crate::components::GoogleCalendarHandle;
use crate::error::google_calendar_error;
use chrono_tz::Tz;

/// Get this week's calendar events
#[poise::command(slash_command, prefix_command)]
pub async fn this_week(
    ctx: Context<'_>,
    #[description = "Optional timezone (e.g. 'Europe/London')"] timezone: Option<String>,
) -> CommandResult {
    // Start response with waiting message
    let response = ctx.say("Fetching calendar events for this week...").await?;

    // Get the config from ctx.data()
    let config = ctx.data().config.clone();

    // Get Google Calendar handle
    let handle = if let Some(cm) = &ctx.data().component_manager {
        // Try to get the actual GoogleCalendar component from ComponentManager
        if let Some(component) = cm.get_component_by_name("google_calendar") {
            // Try to downcast to get the actual Google Calendar component
            if let Some(calendar_component) = component
                .as_any()
                .downcast_ref::<crate::components::google_calendar::GoogleCalendar>(
            ) {
                tracing::debug!("Using Google Calendar component from ComponentManager");
                // Get the handle from the component
                if let Some(handle) = calendar_component.get_handle().await {
                    handle
                } else {
                    // Create a new handle if we couldn't get one
                    tracing::debug!("No handle in Google Calendar component, creating new one");
                    let redis_handle = crate::components::redis_service::RedisActorHandle::empty();
                    GoogleCalendarHandle::new(config.clone(), redis_handle)
                }
            } else {
                tracing::debug!("Could not downcast Google Calendar component");
                let redis_handle = crate::components::redis_service::RedisActorHandle::empty();
                GoogleCalendarHandle::new(config.clone(), redis_handle)
            }
        } else {
            tracing::debug!("Google Calendar component not found in ComponentManager");
            let redis_handle = crate::components::redis_service::RedisActorHandle::empty();
            GoogleCalendarHandle::new(config.clone(), redis_handle)
        }
    } else {
        tracing::debug!("ComponentManager not available, creating standalone handle");
        let redis_handle = crate::components::redis_service::RedisActorHandle::empty();
        GoogleCalendarHandle::new(config.clone(), redis_handle)
    };

    // Get timezone from user input or default
    let timezone_str = match &timezone {
        Some(tz) => tz.clone(),
        None => {
            let config_read = config.read().await;
            config_read.timezone.clone()
        }
    };

    // Parse timezone
    let timezone: Tz = match timezone_str.parse() {
        Ok(tz) => tz,
        Err(_) => {
            // Simply send a new message instead of editing
            let error_msg = format!("❌ Error: Invalid timezone '{}'", timezone_str);
            ctx.send(
                poise::CreateReply::default()
                    .content(error_msg)
                    .ephemeral(true),
            )
            .await?;
            return Err(google_calendar_error(&format!(
                "Invalid timezone: {}",
                timezone_str
            )));
        }
    };

    // Get upcoming events and format them
    let events = match handle.get_upcoming_events().await {
        Ok(events) => events,
        Err(e) => {
            // Simply send a new message instead of editing
            let error_msg = format!("❌ Error fetching events: {}", e);
            ctx.send(
                poise::CreateReply::default()
                    .content(error_msg)
                    .ephemeral(true),
            )
            .await?;
            return Err(e);
        }
    };

    // Format events into a weekly view
    let now = chrono::Utc::now().with_timezone(&timezone);
    let week_start = now.date_naive();
    let week_end = week_start + chrono::Duration::days(7);

    // Filter events for this week
    let mut weekly_events = events
        .iter()
        .filter(|e| {
            if let Some(date_time) = &e.start_date_time {
                // date_time is a string in RFC3339 format
                if let Ok(event_time) = chrono::DateTime::parse_from_rfc3339(date_time) {
                    let event_date = event_time.with_timezone(&timezone).date_naive();
                    return event_date >= week_start && event_date < week_end;
                }
            } else if let Some(date) = &e.start_date {
                // date is a string in YYYY-MM-DD format
                if let Ok(event_date) = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
                    return event_date >= week_start && event_date < week_end;
                }
            }
            false
        })
        .collect::<Vec<_>>();

    // Sort by date
    weekly_events.sort_by(|a, b| {
        let a_date = get_event_date(a, &timezone);
        let b_date = get_event_date(b, &timezone);
        a_date.cmp(&b_date)
    });

    let mut message = format!("📊 **This Week's Calendar Events** ({})\n\n", timezone_str);

    if weekly_events.is_empty() {
        message.push_str("No events scheduled for this week!");
    } else {
        let mut current_date = None;

        for event in weekly_events {
            let event_date = get_event_date(event, &timezone);

            if current_date != Some(event_date) {
                current_date = Some(event_date);
                if let Some(date) = current_date {
                    message.push_str(&format!("\n**{}**\n", date.format("%A, %B %d")));
                }
            }

            let title = event.summary.as_deref().unwrap_or("Untitled Event");
            let start_time = if let Some(date_time) = &event.start_date_time {
                // date_time is a string in RFC3339 format
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_time) {
                    format!("{}", dt.with_timezone(&timezone).format("%H:%M"))
                } else {
                    "Unknown time".to_string()
                }
            } else {
                "All day".to_string()
            };

            message.push_str(&format!("• **{}** - {}\n", start_time, title));
        }
    }

    // Update with the calendar data by deleting and sending a new message
    let _ = response.delete(ctx).await;
    ctx.say(message).await?;

    Ok(())
}

// Helper function to get event date
fn get_event_date(
    event: &crate::components::google_calendar::CalendarEvent,
    timezone: &Tz,
) -> chrono::NaiveDate {
    if let Some(date_time) = &event.start_date_time {
        // date_time is a string in RFC3339 format
        if let Ok(event_time) = chrono::DateTime::parse_from_rfc3339(date_time) {
            return event_time.with_timezone(timezone).date_naive();
        }
    } else if let Some(date) = &event.start_date {
        // date is a string in YYYY-MM-DD format
        if let Ok(event_date) = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
            return event_date;
        }
    }
    // Default to today if we can't parse the date
    chrono::Utc::now().with_timezone(timezone).date_naive()
}
