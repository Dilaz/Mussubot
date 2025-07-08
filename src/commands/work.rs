use crate::commands::{
    create_error_embed, create_info_embed, create_success_embed, create_warning_embed,
    CommandResult, Context,
};
use crate::components::work_schedule::{WorkSchedule, WorkScheduleHandle};
use crate::config::Config;
use chrono::{Duration, Local, NaiveDate};
use poise::serenity_prelude as serenity;
use rust_i18n::t;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// Get work schedule for this week
#[poise::command(slash_command, prefix_command)]
pub async fn tyovuorot(
    ctx: Context<'_>,
    #[description = "Employee name (leave empty for all employees)"] employee: Option<String>,
) -> CommandResult {
    // Start response with waiting message
    let response = ctx
        .say(t!(
            "fetch_processing",
            resource = "work schedules for this week"
        ))
        .await?;

    // Get the handle to work schedule
    let handle = get_work_schedule_handle(
        ctx.data().component_manager.as_ref(),
        ctx.data().config.clone(),
    )
    .await;

    // Calculate the date range for this week (Monday to Sunday)
    let now = Local::now();
    let today = now.date_naive();
    let weekday_num = today.format("%u").to_string().parse::<u32>().unwrap_or(1); // 1=Monday, 7=Sunday
    let days_since_monday = weekday_num - 1;
    let monday = today
        .checked_sub_signed(Duration::days(days_since_monday as i64))
        .unwrap_or(today);
    let sunday = monday
        .checked_add_signed(Duration::days(6))
        .unwrap_or(monday);

    let start_date = monday.format("%Y-%m-%d").to_string();
    let end_date = sunday.format("%Y-%m-%d").to_string();

    if let Some(emp) = employee {
        // Get schedule for specific employee
        match handle
            .get_schedule_for_date_range(emp.clone(), start_date.clone(), end_date.clone())
            .await
        {
            Ok(schedule) => {
                let title = t!("work_schedule_employee_title", employee = emp);
                let mut embed = serenity::CreateEmbed::new()
                    .title(title)
                    .description(format!("{start_date} - {end_date}"))
                    .color(0x00_99_FF); // Blue color

                if schedule.schedule.is_empty() {
                    embed = embed
                        .description(t!("work_schedule_no_entries_for_employee", employee = emp));
                } else {
                    let mut field_content;

                    for entry in schedule.schedule {
                        // Parse date to get day of week
                        if let Ok(date) = NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d") {
                            let weekday_num =
                                date.format("%u").to_string().parse::<u32>().unwrap_or(0);
                            let day_name = match weekday_num {
                                1 => t!("day_monday"),
                                2 => t!("day_tuesday"),
                                3 => t!("day_wednesday"),
                                4 => t!("day_thursday"),
                                5 => t!("day_friday"),
                                6 => t!("day_saturday"),
                                7 => t!("day_sunday"),
                                _ => t!("day_unknown"),
                            };

                            // Format as field per day
                            field_content = entry.format();
                            embed = embed.field(
                                format!("{} ({})", day_name, entry.date),
                                field_content,
                                false,
                            );
                        } else {
                            // Fallback if we can't parse the date
                            field_content = entry.format();
                            embed = embed.field(entry.date, field_content, false);
                        }
                    }
                }

                // Delete the waiting message and send the embed
                let _ = response.delete(ctx).await;
                ctx.send(poise::CreateReply::default().embed(embed)).await?;
            }
            Err(e) => {
                // Delete the waiting message and send the error
                let _ = response.delete(ctx).await;
                ctx.send(
                    poise::CreateReply::default()
                        .embed(create_error_embed(
                            &t!("error_title", context = "schedule"),
                            &t!(
                                "work_schedule_error_fetching",
                                resource = "schedule",
                                error = e.to_string()
                            ),
                        ))
                        .ephemeral(true),
                )
                .await?;
            }
        }
    } else {
        // Get all employees
        match handle.get_employees().await {
            Ok(employees) => {
                if employees.is_empty() {
                    let _ = response.delete(ctx).await;
                    ctx.send(
                        poise::CreateReply::default()
                            .embed(create_info_embed(
                                &t!(
                                    "work_schedule_weekly_title",
                                    start_date = start_date,
                                    end_date = end_date
                                ),
                                &t!("work_schedule_no_employees"),
                            ))
                            .ephemeral(true),
                    )
                    .await?;
                    return Ok(());
                }

                let mut embed = serenity::CreateEmbed::new()
                    .title(t!(
                        "work_schedule_weekly_title",
                        start_date = start_date,
                        end_date = end_date
                    ))
                    .color(0x00_99_FF); // Blue color

                for emp in employees {
                    match handle
                        .get_schedule_for_date_range(
                            emp.clone(),
                            start_date.clone(),
                            end_date.clone(),
                        )
                        .await
                    {
                        Ok(schedule) => {
                            if schedule.schedule.is_empty() {
                                embed =
                                    embed.field(emp, t!("work_schedule_no_entries_found"), false);
                            } else {
                                let mut field_value = String::new();
                                for entry in schedule.schedule {
                                    // Parse date to get day of week
                                    if let Ok(date) =
                                        NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d")
                                    {
                                        let weekday_num = date
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

                                        field_value.push_str(&format!(
                                            "• **{}**: {}\n",
                                            day_name,
                                            entry.format()
                                        ));
                                    } else {
                                        field_value.push_str(&format!(
                                            "• {}: {}\n",
                                            entry.date,
                                            entry.format()
                                        ));
                                    }
                                }
                                embed = embed.field(emp, field_value, false);
                            }
                        }
                        Err(e) => {
                            embed = embed.field(
                                emp.clone(),
                                t!(
                                    "work_schedule_error_fetching",
                                    resource = emp.clone(),
                                    error = e.to_string()
                                ),
                                false,
                            );
                        }
                    }
                }

                // Delete the waiting message and send the embed
                let _ = response.delete(ctx).await;
                ctx.send(poise::CreateReply::default().embed(embed)).await?;
            }
            Err(e) => {
                // Delete the waiting message and send the error
                let _ = response.delete(ctx).await;
                ctx.send(
                    poise::CreateReply::default()
                        .embed(create_error_embed(
                            &t!("error_title", context = "employees"),
                            &t!(
                                "work_schedule_error_fetching",
                                resource = "employees",
                                error = e.to_string()
                            ),
                        ))
                        .ephemeral(true),
                )
                .await?;
            }
        }
    }

    Ok(())
}

/// Get work schedule for a specific date
#[poise::command(slash_command, prefix_command)]
pub async fn day(
    ctx: Context<'_>,
    #[description = "Date (YYYY-MM-DD)"] date: String,
    #[description = "Employee name (leave empty for all employees)"] employee: Option<String>,
) -> CommandResult {
    // Start response with waiting message
    let response = ctx
        .say(t!(
            "fetch_processing",
            resource = format!("work schedules for {}", date)
        ))
        .await?;

    // Get the handle to work schedule
    let handle = get_work_schedule_handle(
        ctx.data().component_manager.as_ref(),
        ctx.data().config.clone(),
    )
    .await;

    // Validate date format
    if NaiveDate::parse_from_str(&date, "%Y-%m-%d").is_err() {
        // Delete the waiting message and send the error
        let _ = response.delete(ctx).await;
        ctx.send(
            poise::CreateReply::default()
                .embed(create_warning_embed(
                    &t!("work_schedule_invalid_date"),
                    &t!("work_schedule_invalid_date"),
                ))
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    if let Some(emp) = employee {
        // Get schedule for specific employee on specific date
        match handle.get_entry_for_employee_date(&emp, &date).await {
            Ok(entry) => {
                let title = t!(
                    "work_schedule_employee_date_title",
                    employee = emp,
                    date = date
                );

                // Delete the waiting message and send the embed
                let _ = response.delete(ctx).await;
                ctx.send(
                    poise::CreateReply::default()
                        .embed(create_success_embed(&title, &entry.format())),
                )
                .await?;
            }
            Err(e) => {
                // Delete the waiting message and send the error
                let _ = response.delete(ctx).await;
                ctx.send(
                    poise::CreateReply::default()
                        .embed(create_error_embed(
                            &t!("error_title", context = "schedule"),
                            &t!(
                                "work_schedule_error_fetching",
                                resource = "schedule",
                                error = e.to_string()
                            ),
                        ))
                        .ephemeral(true),
                )
                .await?;
            }
        }
    } else {
        // Get schedule for all employees on specific date
        match handle.get_schedule_for_date(&date).await {
            Ok(schedules) => {
                if schedules.is_empty() {
                    // Delete the waiting message and send the info
                    let _ = response.delete(ctx).await;
                    ctx.send(poise::CreateReply::default().embed(create_info_embed(
                        &t!("work_schedule_date_title", date = date),
                        &t!("work_schedule_no_schedules_found", date = date),
                    )))
                    .await?;
                    return Ok(());
                }

                // Try to parse the date to get day of week
                let day_header =
                    if let Ok(parsed_date) = NaiveDate::parse_from_str(&date, "%Y-%m-%d") {
                        let weekday_num = parsed_date
                            .format("%u")
                            .to_string()
                            .parse::<u32>()
                            .unwrap_or(0);
                        let day_name = match weekday_num {
                            1 => t!("day_monday"),
                            2 => t!("day_tuesday"),
                            3 => t!("day_wednesday"),
                            4 => t!("day_thursday"),
                            5 => t!("day_friday"),
                            6 => t!("day_saturday"),
                            7 => t!("day_sunday"),
                            _ => t!("day_unknown"),
                        };

                        format!("{day_name} ({date})")
                    } else {
                        date.clone()
                    };

                let title = t!("work_schedule_date_title", date = day_header);
                let mut embed = serenity::CreateEmbed::new().title(title).color(0x00_99_FF); // Blue color

                // Check if everyone has a day off
                let all_day_off = schedules.values().all(|entry| entry.is_day_off);
                if all_day_off {
                    embed = embed
                        .description(t!("work_schedule_all_day_off"))
                        .image("https://media.giphy.com/media/v1.Y2lkPTc5MGI3NjExdG9nM3J1YnA1NHcxc2cwcmE5bjNqOWF1eHZsY3h3MDBxbDl5aGdldiZlcD12MV9pbnRlcm5hbF9naWZfYnlfaWQmY3Q9Zw/DKnMqdm9i980E/giphy.gif");
                } else {
                    // Add fields for each employee sorted alphabetically
                    let mut employees: Vec<(
                        &String,
                        &crate::components::work_schedule::models::WorkScheduleEntry,
                    )> = schedules.iter().collect();
                    employees.sort_by(|a, b| a.0.cmp(b.0));

                    for (emp, entry) in employees {
                        embed = embed.field(emp, entry.format(), false);
                    }
                }

                // Delete the waiting message and send the embed
                let _ = response.delete(ctx).await;
                ctx.send(poise::CreateReply::default().embed(embed)).await?;
            }
            Err(e) => {
                // Delete the waiting message and send the error
                let _ = response.delete(ctx).await;
                ctx.send(
                    poise::CreateReply::default()
                        .embed(create_error_embed(
                            &t!("error_title", context = "schedule"),
                            &t!(
                                "work_schedule_error_fetching",
                                resource = "schedules",
                                error = e.to_string()
                            ),
                        ))
                        .ephemeral(true),
                )
                .await?;
            }
        }
    }

    Ok(())
}

/// Get an employee's work schedule
#[poise::command(slash_command, prefix_command)]
pub async fn employee(
    ctx: Context<'_>,
    #[description = "Employee name"] employee: String,
) -> CommandResult {
    // Start response with waiting message
    let response = ctx
        .say(t!(
            "fetch_processing",
            resource = format!("work schedule for {}", employee)
        ))
        .await?;

    // Get the handle to work schedule
    let handle = get_work_schedule_handle(
        ctx.data().component_manager.as_ref(),
        ctx.data().config.clone(),
    )
    .await;

    // Get schedule for employee
    match handle.get_schedule_for_employee(employee.clone()).await {
        Ok(schedule) => {
            let title = t!("work_schedule_employee_title", employee = employee);
            let mut embed = serenity::CreateEmbed::new().title(title).color(0x00_99_FF); // Blue color

            if schedule.schedule.is_empty() {
                embed = embed.description(t!(
                    "work_schedule_no_entries_for_employee",
                    employee = employee
                ));
            } else {
                // Group entries by day
                let mut day_entries: std::collections::HashMap<
                    String,
                    Vec<&crate::components::work_schedule::models::WorkScheduleEntry>,
                > = std::collections::HashMap::new();

                for entry in &schedule.schedule {
                    if let Ok(date) = NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d") {
                        let weekday_num = date.format("%u").to_string().parse::<u32>().unwrap_or(0);
                        let day_name = match weekday_num {
                            1 => t!("day_monday"),
                            2 => t!("day_tuesday"),
                            3 => t!("day_wednesday"),
                            4 => t!("day_thursday"),
                            5 => t!("day_friday"),
                            6 => t!("day_saturday"),
                            7 => t!("day_sunday"),
                            _ => t!("day_unknown"),
                        };

                        let day_key = format!("{} ({})", day_name, entry.date);
                        day_entries.entry(day_key).or_default().push(entry);
                    } else {
                        // If we can't parse the date, just use the date string
                        day_entries
                            .entry(entry.date.clone())
                            .or_default()
                            .push(entry);
                    }
                }

                // Sort days by date
                let mut days: Vec<(
                    String,
                    Vec<&crate::components::work_schedule::models::WorkScheduleEntry>,
                )> = day_entries.into_iter().collect();
                days.sort_by(|a, b| {
                    let a_date = a.1.first().map(|e| e.date.clone()).unwrap_or_default();
                    let b_date = b.1.first().map(|e| e.date.clone()).unwrap_or_default();
                    a_date.cmp(&b_date)
                });

                // Add fields for each day
                for (day, entries) in days {
                    let entry_fmt = if entries.len() == 1 {
                        entries[0].format()
                    } else {
                        let mut times = String::new();
                        for (i, entry) in entries.iter().enumerate() {
                            if i > 0 {
                                times.push('\n');
                            }
                            times.push_str(&format!("• {}", entry.format()));
                        }
                        times
                    };

                    embed = embed.field(day, entry_fmt, false);
                }
            }

            // Delete the waiting message and send the embed
            let _ = response.delete(ctx).await;
            ctx.send(poise::CreateReply::default().embed(embed)).await?;
        }
        Err(e) => {
            // Delete the waiting message and send the error
            let _ = response.delete(ctx).await;
            ctx.send(
                poise::CreateReply::default()
                    .embed(create_error_embed(
                        &t!("error_title", context = "schedule"),
                        &t!(
                            "work_schedule_error_fetching",
                            resource = "schedule",
                            error = e.to_string()
                        ),
                    ))
                    .ephemeral(true),
            )
            .await?;
        }
    }

    Ok(())
}

/// Get work schedule for next week
#[poise::command(slash_command, prefix_command)]
pub async fn ensiviikko(
    ctx: Context<'_>,
    #[description = "Employee name (leave empty for all employees)"] employee: Option<String>,
) -> CommandResult {
    // Start response with waiting message
    let response = ctx
        .say(t!(
            "fetch_processing",
            resource = "work schedules for next week"
        ))
        .await?;

    // Get the handle to work schedule
    let handle = get_work_schedule_handle(
        ctx.data().component_manager.as_ref(),
        ctx.data().config.clone(),
    )
    .await;

    // Calculate the date range for next week (Monday to Sunday)
    let now = Local::now();
    let today = now.date_naive();
    let weekday_num = today.format("%u").to_string().parse::<u32>().unwrap_or(1); // 1=Monday, 7=Sunday
    let days_since_monday = weekday_num - 1;
    let this_monday = today
        .checked_sub_signed(Duration::days(days_since_monday as i64))
        .unwrap_or(today);
    let next_monday = this_monday
        .checked_add_signed(Duration::days(7))
        .unwrap_or(this_monday);
    let next_sunday = next_monday
        .checked_add_signed(Duration::days(6))
        .unwrap_or(next_monday);

    let start_date = next_monday.format("%Y-%m-%d").to_string();
    let end_date = next_sunday.format("%Y-%m-%d").to_string();

    if let Some(emp) = employee {
        // Get schedule for specific employee
        match handle
            .get_schedule_for_date_range(emp.clone(), start_date.clone(), end_date.clone())
            .await
        {
            Ok(schedule) => {
                let title = t!("work_schedule_employee_title", employee = emp);
                let mut embed = serenity::CreateEmbed::new()
                    .title(format!("{}: {}", t!("calendar_next_week"), title))
                    .description(format!("{start_date} - {end_date}"))
                    .color(0x00_99_FF); // Blue color

                if schedule.schedule.is_empty() {
                    embed = embed
                        .description(t!("work_schedule_no_entries_for_employee", employee = emp));
                } else {
                    // Group entries by day
                    let mut day_entries: std::collections::HashMap<
                        String,
                        Vec<&crate::components::work_schedule::models::WorkScheduleEntry>,
                    > = std::collections::HashMap::new();

                    for entry in &schedule.schedule {
                        if let Ok(date) = NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d") {
                            let weekday_num =
                                date.format("%u").to_string().parse::<u32>().unwrap_or(0);
                            let day_name = match weekday_num {
                                1 => t!("day_monday"),
                                2 => t!("day_tuesday"),
                                3 => t!("day_wednesday"),
                                4 => t!("day_thursday"),
                                5 => t!("day_friday"),
                                6 => t!("day_saturday"),
                                7 => t!("day_sunday"),
                                _ => t!("day_unknown"),
                            };

                            let day_key = format!("{} ({})", day_name, entry.date);
                            day_entries.entry(day_key).or_default().push(entry);
                        } else {
                            // If we can't parse the date, just use the date string
                            day_entries
                                .entry(entry.date.clone())
                                .or_default()
                                .push(entry);
                        }
                    }

                    // Sort days by date
                    let mut days: Vec<(
                        String,
                        Vec<&crate::components::work_schedule::models::WorkScheduleEntry>,
                    )> = day_entries.into_iter().collect();
                    days.sort_by(|a, b| {
                        let a_date = a.1.first().map(|e| e.date.clone()).unwrap_or_default();
                        let b_date = b.1.first().map(|e| e.date.clone()).unwrap_or_default();
                        a_date.cmp(&b_date)
                    });

                    // Add fields for each day
                    for (day, entries) in days {
                        let entry_fmt = if entries.len() == 1 {
                            entries[0].format()
                        } else {
                            let mut times = String::new();
                            for (i, entry) in entries.iter().enumerate() {
                                if i > 0 {
                                    times.push('\n');
                                }
                                times.push_str(&format!("• {}", entry.format()));
                            }
                            times
                        };

                        embed = embed.field(day, entry_fmt, false);
                    }
                }

                // Delete the waiting message and send the embed
                let _ = response.delete(ctx).await;
                ctx.send(poise::CreateReply::default().embed(embed)).await?;
            }
            Err(e) => {
                // Delete the waiting message and send the error
                let _ = response.delete(ctx).await;
                ctx.send(
                    poise::CreateReply::default()
                        .embed(create_error_embed(
                            &t!("error_title", context = "schedule"),
                            &t!(
                                "work_schedule_error_fetching",
                                resource = "schedule",
                                error = e.to_string()
                            ),
                        ))
                        .ephemeral(true),
                )
                .await?;
            }
        }
    } else {
        // Get all employees
        match handle.get_employees().await {
            Ok(employees) => {
                if employees.is_empty() {
                    let _ = response.delete(ctx).await;
                    ctx.send(
                        poise::CreateReply::default()
                            .embed(create_info_embed(
                                &format!(
                                    "{}: {}",
                                    t!("calendar_next_week"),
                                    t!(
                                        "work_schedule_week_title",
                                        start_date = start_date,
                                        end_date = end_date
                                    )
                                ),
                                &t!("work_schedule_no_employees"),
                            ))
                            .ephemeral(true),
                    )
                    .await?;
                    return Ok(());
                }

                let mut embed = serenity::CreateEmbed::new()
                    .title(format!(
                        "{}: {}",
                        t!("calendar_next_week"),
                        t!(
                            "work_schedule_week_title",
                            start_date = start_date,
                            end_date = end_date
                        )
                    ))
                    .color(0x00_99_FF); // Blue color

                for emp in employees {
                    match handle
                        .get_schedule_for_date_range(
                            emp.clone(),
                            start_date.clone(),
                            end_date.clone(),
                        )
                        .await
                    {
                        Ok(schedule) => {
                            if schedule.schedule.is_empty() {
                                embed =
                                    embed.field(&emp, t!("work_schedule_no_entries_found"), false);
                            } else {
                                let mut field_value = String::new();
                                for entry in schedule.schedule {
                                    // Parse date to get day of week
                                    if let Ok(date) =
                                        NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d")
                                    {
                                        let weekday_num = date
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

                                        field_value.push_str(&format!(
                                            "• **{}**: {}\n",
                                            day_name,
                                            entry.format()
                                        ));
                                    } else {
                                        field_value.push_str(&format!(
                                            "• {}: {}\n",
                                            entry.date,
                                            entry.format()
                                        ));
                                    }
                                }
                                embed = embed.field(emp, field_value, false);
                            }
                        }
                        Err(e) => {
                            embed = embed.field(
                                emp.clone(),
                                t!(
                                    "work_schedule_error_fetching",
                                    resource = emp.clone(),
                                    error = e.to_string()
                                ),
                                false,
                            );
                        }
                    }
                }

                // Delete the waiting message and send the embed
                let _ = response.delete(ctx).await;
                ctx.send(poise::CreateReply::default().embed(embed)).await?;
            }
            Err(e) => {
                // Delete the waiting message and send the error
                let _ = response.delete(ctx).await;
                ctx.send(
                    poise::CreateReply::default()
                        .embed(create_error_embed(
                            &t!("error_title", context = "employees"),
                            &t!(
                                "work_schedule_error_fetching",
                                resource = "employees",
                                error = e.to_string()
                            ),
                        ))
                        .ephemeral(true),
                )
                .await?;
            }
        }
    }

    Ok(())
}

/// Helper to get the work schedule handle
async fn get_work_schedule_handle(
    component_manager: Option<&Arc<crate::components::ComponentManager>>,
    config: Arc<RwLock<Config>>,
) -> WorkScheduleHandle {
    // Try to get the handle from the component manager
    if let Some(component_manager) = component_manager {
        if let Some(component) = component_manager.get_component_by_name("work_schedule") {
            // Try to downcast to get the actual Work Schedule component
            if let Some(work_schedule_component) = component.as_any().downcast_ref::<WorkSchedule>()
            {
                debug!("Using Work Schedule component from ComponentManager");
                // Get the handle from the component
                if let Some(handle) = work_schedule_component.get_handle().await {
                    handle
                } else {
                    // Create a new handle if we couldn't get one
                    debug!("No handle in Work Schedule component, creating new one");
                    let redis_handle = crate::components::redis_service::RedisActorHandle::empty();
                    WorkScheduleHandle::new(config.clone(), redis_handle)
                }
            } else {
                debug!("Could not downcast Work Schedule component");
                let redis_handle = crate::components::redis_service::RedisActorHandle::empty();
                WorkScheduleHandle::new(config.clone(), redis_handle)
            }
        } else {
            debug!("Work Schedule component not found in ComponentManager");
            let redis_handle = crate::components::redis_service::RedisActorHandle::empty();
            WorkScheduleHandle::new(config.clone(), redis_handle)
        }
    } else {
        debug!("ComponentManager not available, creating standalone handle");
        let redis_handle = crate::components::redis_service::RedisActorHandle::empty();
        WorkScheduleHandle::new(config.clone(), redis_handle)
    }
}
