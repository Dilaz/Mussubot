use crate::components::work_schedule::handle::WorkScheduleHandle;
use crate::error::{work_schedule_error, BotResult};
use chrono::{Duration, NaiveDate};
use poise::serenity_prelude::{self as serenity, ChannelId, CreateEmbed, CreateMessage};
use rust_i18n::t;
use tracing::info;

/// Send daily notification for today's work schedule
pub async fn send_daily_notification(
    ctx: &serenity::Context,
    channel_id: u64,
    handle: &WorkScheduleHandle,
    date: &str,
) -> BotResult<()> {
    info!("Sending daily work schedule notification for {}", date);

    // Get schedule for all employees for today
    let schedules = handle.get_schedule_for_date(date).await?;

    // Calculate tomorrow's date
    let today = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|e| work_schedule_error(&format!("Failed to parse date: {}", e)))?;
    let tomorrow = today + Duration::days(1);
    let tomorrow_str = tomorrow.format("%Y-%m-%d").to_string();

    // Get schedule for all employees for tomorrow
    let tomorrow_schedules = handle.get_schedule_for_date(&tomorrow_str).await?;

    // Create an embed for the notification
    let mut embed = CreateEmbed::new()
        .title(t!("work_schedule_daily_title", date = date))
        .color(0x00_FF_00); // Green color

    // Handle today's schedules
    if schedules.is_empty() {
        embed = embed.field(
            t!("work_schedule_today_section"),
            t!("work_schedule_daily_no_schedules", date = date),
            false,
        );
    } else {
        // Check if all employees have a day off
        let all_day_off = schedules.iter().all(|(_, entry)| entry.is_day_off);

        if all_day_off {
            embed = embed.field(
                t!("work_schedule_today_section"),
                t!("work_schedule_all_day_off"),
                false,
            );
        } else {
            // Add today's schedules
            embed = embed.field(t!("work_schedule_today_section"), "\u{200B}", false);
            for (employee, entry) in &schedules {
                let schedule_text = entry.format();
                embed = embed.field(employee, schedule_text, true);
            }
        }
    }

    // Handle tomorrow's schedules
    embed = embed.field("\u{200B}", "\u{200B}", false); // Empty field as separator

    if tomorrow_schedules.is_empty() {
        embed = embed.field(
            t!("work_schedule_tomorrow_section", date = tomorrow_str),
            t!("work_schedule_daily_no_schedules", date = tomorrow_str),
            false,
        );
    } else {
        // Check if all employees have a day off tomorrow
        let all_day_off_tomorrow = tomorrow_schedules.iter().all(|(_, entry)| entry.is_day_off);

        if all_day_off_tomorrow {
            embed = embed.field(
                t!("work_schedule_tomorrow_section", date = tomorrow_str),
                t!("work_schedule_all_day_off"),
                false,
            );
        } else {
            // Add tomorrow's schedules
            embed = embed.field(
                t!("work_schedule_tomorrow_section", date = tomorrow_str),
                "\u{200B}",
                false,
            );
            for (employee, entry) in &tomorrow_schedules {
                let schedule_text = entry.format();
                embed = embed.field(employee, schedule_text, true);
            }
        }
    }

    // Add a happy GIF if everyone has a day off today
    let all_day_off_today =
        schedules.is_empty() || schedules.iter().all(|(_, entry)| entry.is_day_off);

    if all_day_off_today {
        embed = embed.image("https://media2.giphy.com/media/v1.Y2lkPTc5MGI3NjExYnp2ZzRxZ2o3MDJ3Ymtrbm8wa25nZDA5a2N5a3V6eDY4cXBqMHhvaSZlcD12MV9pbnRlcm5hbF9naWZfYnlfaWQmY3Q9Zw/Xf8D9Qf8OCKnMvNnru/giphy.gif");
    }

    // Send the notification
    ChannelId::new(channel_id)
        .send_message(
            ctx,
            CreateMessage::new()
                .content(t!("work_schedule_daily_greeting"))
                .embed(embed),
        )
        .await
        .map_err(|e| work_schedule_error(&format!("Failed to send message: {}", e)))?;

    Ok(())
}

/// Send weekly notification for the upcoming week's work schedule
pub async fn send_weekly_notification(
    ctx: &serenity::Context,
    channel_id: u64,
    handle: &WorkScheduleHandle,
    start_date: &str,
    end_date: &str,
) -> BotResult<()> {
    info!(
        "Sending weekly work schedule notification for {} to {}",
        start_date, end_date
    );

    // Get all employees
    let employees = handle.get_employees().await?;

    if employees.is_empty() {
        // If there are no employees, send an embed message indicating that
        let embed = CreateEmbed::new()
            .title(t!(
                "work_schedule_weekly_title",
                start_date = start_date,
                end_date = end_date
            ))
            .description(t!("work_schedule_no_employees"))
            .color(0x00_AA_FF);

        ChannelId::new(channel_id)
            .send_message(
                ctx,
                CreateMessage::new()
                    .content(t!("work_schedule_weekly_greeting"))
                    .embed(embed),
            )
            .await
            .map_err(|e| work_schedule_error(&format!("Failed to send message: {}", e)))?;
        return Ok(());
    }

    // Create an embed for the notification
    let mut embed = CreateEmbed::new()
        .title(t!(
            "work_schedule_weekly_title",
            start_date = start_date,
            end_date = end_date
        ))
        .color(0x00_00_FF); // Blue color

    // For each employee, get their schedule for the week
    for employee in employees {
        let schedule = handle
            .get_schedule_for_date_range(&employee, start_date, end_date)
            .await?;

        // Create a string representation of the schedule
        let mut schedule_text = String::new();
        for entry in &schedule.schedule {
            // Parse date to get day of week
            let naive_date = NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d")
                .map_err(|e| work_schedule_error(&format!("Failed to parse date: {}", e)))?;

            // Format the day name (e.g., "Mon") and date (e.g., "2025-04-01")
            let weekday_num = naive_date
                .format("%u")
                .to_string()
                .parse::<u32>()
                .unwrap_or(0);
            let day_name = match weekday_num {
                1 => t!("day_short_monday"),
                2 => t!("day_short_tuesday"),
                3 => t!("day_short_wednesday"),
                4 => t!("day_short_thursday"),
                5 => t!("day_short_friday"),
                6 => t!("day_short_saturday"),
                7 => t!("day_short_sunday"),
                _ => t!("day_short_unknown"),
            };

            // Format the schedule entry with day name
            schedule_text.push_str(&format!(
                "**{}** ({}): {}\n",
                day_name,
                entry.date,
                entry.format()
            ));
        }

        // Add the employee's schedule to the embed or indicate no schedule
        if schedule_text.is_empty() {
            embed = embed.field(employee, t!("work_schedule_no_entries_found"), false);
        } else {
            embed = embed.field(employee, schedule_text, false);
        }
    }

    // Send the notification
    ChannelId::new(channel_id)
        .send_message(
            ctx,
            CreateMessage::new()
                .content(t!("work_schedule_weekly_greeting"))
                .embed(embed),
        )
        .await
        .map_err(|e| work_schedule_error(&format!("Failed to send message: {}", e)))?;

    Ok(())
}
