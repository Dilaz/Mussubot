use crate::components::google_calendar::handle::GoogleCalendarHandle;
use crate::components::google_calendar::models::CalendarEvent;
use crate::components::google_calendar::time::get_event_start;
use crate::error::BotResult;
use chrono::{Duration, Local};
use poise::serenity_prelude::{self as serenity, ChannelId, CreateEmbed, CreateMessage};
use rust_i18n::t;

// Icon URLs for calendar notifications
const CALENDAR_EMPTY_ICON: &str = "https://cdn-icons-png.flaticon.com/512/3652/3652191.png";
const CALENDAR_WITH_EVENTS_ICON: &str = "https://cdn-icons-png.flaticon.com/512/2693/2693507.png";
const NEW_EVENT_ICON: &str = "https://cdn-icons-png.flaticon.com/512/2965/2965879.png";
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

    // Create an embed for the notification
    let mut embed = CreateEmbed::new()
        .title(t!("calendar_daily_title"))
        .color(0x4285F4) // Google Blue color
        .timestamp(Local::now());

    if today_events.is_empty() {
        embed = embed
            .description(t!("calendar_no_events_today"))
            .thumbnail(CALENDAR_EMPTY_ICON);
    } else {
        // Sort events by time
        today_events.sort_by_key(|(_, start)| *start);

        let mut events_text = String::new();
        for (event, start) in today_events {
            let summary = event.summary.as_deref().unwrap_or("calendar_unnamed_event");
            let time = start.format("%H:%M").to_string();
            events_text.push_str(&format!("🕐 **{time}** - {summary}\n"));
        }

        embed = embed
            .description(events_text)
            .thumbnail(CALENDAR_WITH_EVENTS_ICON)
            .footer(serenity::CreateEmbedFooter::new(format!(
                "📅 {}",
                today.format("%A, %B %d, %Y")
            )));
    }

    ChannelId::new(channel_id)
        .send_message(ctx, CreateMessage::new().embed(embed))
        .await?;

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

    // Create an embed for the weekly notification
    let mut embed = CreateEmbed::new()
        .title(t!("calendar_weekly_title"))
        .color(0x34A853) // Google Green color
        .timestamp(Local::now())
        .footer(serenity::CreateEmbedFooter::new(format!(
            "📅 {} - {}",
            today.format("%d.%m.%Y"),
            (week_end - Duration::days(1)).format("%d.%m.%Y")
        )));

    if week_events.is_empty() {
        embed = embed
            .description(t!("calendar_no_events_week"))
            .thumbnail(CALENDAR_EMPTY_ICON);
    } else {
        // Sort events by time
        week_events.sort_by_key(|(_, start)| *start);

        embed = embed.thumbnail(CALENDAR_WITH_EVENTS_ICON);

        let mut current_date = today;
        while current_date < week_end {
            let day_events: Vec<_> = week_events
                .iter()
                .filter(|(_, start)| start.date_naive() == current_date)
                .collect();

            if !day_events.is_empty() {
                let day_name = current_date.format("%A").to_string();
                let day_date = current_date.format("%d.%m").to_string();

                let mut day_text = String::new();
                for (event, start) in day_events {
                    let summary = event.summary.as_deref().unwrap_or("calendar_unnamed_event");
                    let time = start.format("%H:%M").to_string();
                    day_text.push_str(&format!("🕐 **{time}** - {summary}\n"));
                }

                embed = embed.field(format!("{day_name} ({day_date})"), day_text, false);
            }

            current_date += Duration::days(1);
        }
    }

    ChannelId::new(channel_id)
        .send_message(ctx, CreateMessage::new().embed(embed))
        .await?;

    Ok(())
}

/// Send notification for new calendar events
pub async fn send_new_events_notification(
    ctx: &serenity::Context,
    channel_id: u64,
    events: &[CalendarEvent],
) -> BotResult<()> {
    if !events.is_empty() {
        // Create an embed for new events notification
        let mut embed = CreateEmbed::new()
            .title(t!("calendar_new_events_title"))
            .color(0xEA4335) // Google Red color
            .timestamp(Local::now())
            .thumbnail(NEW_EVENT_ICON);

        let mut events_text = String::new();
        for event in events {
            let summary = event.summary.as_deref().unwrap_or("calendar_unnamed_event");
            let time = if let Ok(Some(start)) = get_event_start(event) {
                format!("{}", start.format("%d.%m. %H:%M"))
            } else {
                t!("calendar_unknown_time").to_string()
            };
            events_text.push_str(&format!("🆕 **{time}** - {summary}\n"));
        }

        embed = embed.description(events_text);

        ChannelId::new(channel_id)
            .send_message(ctx, CreateMessage::new().embed(embed))
            .await?;
    }

    Ok(())
}
