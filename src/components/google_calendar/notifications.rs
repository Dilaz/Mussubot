use crate::components::google_calendar::handle::GoogleCalendarHandle;
use crate::components::google_calendar::models::CalendarEvent;
use crate::components::google_calendar::time::get_event_start;
use crate::error::BotResult;
use chrono::{Duration, Local};
use poise::serenity_prelude as serenity;
/// Send daily notification of calendar events
pub async fn send_daily_notification(
    ctx: &serenity::Context,
    channel_id: u64,
    handle: &GoogleCalendarHandle,
) -> BotResult<()> {
    let events = handle.get_upcoming_events().await?;
    let today = Local::now().date_naive();

    let mut today_events = Vec::new();
    for event in events {
        if let Ok(Some(start)) = get_event_start(&event) {
            if start.date_naive() == today {
                today_events.push((event, start));
            }
        }
    }

    if !today_events.is_empty() {
        let mut message = t!("calendar_daily_title").to_string();
        message.push('\n');
        for (event, start) in today_events {
            let summary = event.summary.as_deref().unwrap_or("calendar_unnamed_event");
            let time = start.format("%H:%M").to_string();
            message.push_str(&format!("• {summary} ({time})\n"));
        }

        let channel_id = serenity::ChannelId::new(channel_id);
        channel_id
            .send_message(&ctx.http, serenity::CreateMessage::new().content(&message))
            .await?;
    }

    Ok(())
}

/// Send weekly notification of calendar events
pub async fn send_weekly_notification(
    ctx: &serenity::Context,
    channel_id: u64,
    handle: &GoogleCalendarHandle,
) -> BotResult<()> {
    let events = handle.get_upcoming_events().await?;
    let today = Local::now().date_naive();
    let week_end = today + Duration::days(7);

    let mut week_events = Vec::new();
    for event in events {
        if let Ok(Some(start)) = get_event_start(&event) {
            let event_date = start.date_naive();
            if event_date >= today && event_date < week_end {
                week_events.push((event, start));
            }
        }
    }

    if !week_events.is_empty() {
        let mut message = t!("calendar_weekly_title").to_string();
        let mut current_date = today;

        while current_date < week_end {
            let day_events: Vec<_> = week_events
                .iter()
                .filter(|(_, start)| start.date_naive() == current_date)
                .collect();

            if !day_events.is_empty() {
                message.push_str(&format!("\n**{}:**\n", current_date.format("%A %d.%m.")));
                for (event, start) in day_events {
                    let summary = event.summary.as_deref().unwrap_or("calendar_unnamed_event");
                    let time = start.format("%H:%M").to_string();
                    message.push_str(&format!("• {summary} ({time})\n"));
                }
            }

            current_date += Duration::days(1);
        }

        let channel_id = serenity::ChannelId::new(channel_id);
        channel_id
            .send_message(&ctx.http, serenity::CreateMessage::new().content(&message))
            .await?;
    }

    Ok(())
}

/// Send notification for new calendar events
pub async fn send_new_events_notification(
    ctx: &serenity::Context,
    channel_id: u64,
    events: &[CalendarEvent],
) -> BotResult<()> {
    if !events.is_empty() {
        let mut message = t!("calendar_new_events_title").to_string();
        message.push('\n');
        for event in events {
            let summary = event.summary.as_deref().unwrap_or("calendar_unnamed_event");
            let time = if let Ok(Some(start)) = get_event_start(event) {
                format!("{}", start.format("%d.%m. %H:%M"))
            } else {
                t!("calendar_unknown_time").to_string()
            };
            message.push_str(&format!("• {summary} ({time})\n"));
        }

        let channel_id = serenity::ChannelId::new(channel_id);
        channel_id
            .send_message(&ctx.http, serenity::CreateMessage::new().content(&message))
            .await?;
    }

    Ok(())
}
