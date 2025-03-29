use crate::commands::{CommandResult, Context, create_error_embed, create_info_embed, create_success_embed, create_warning_embed};
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
    // Get the handle to work schedule
    let handle = get_work_schedule_handle(ctx.data().component_manager.as_ref(), ctx.data().config.clone()).await;

    // Calculate the date range for this week (Monday to Sunday)
    let now = Local::now();
    let today = now.date_naive();
    let weekday_num = today.format("%u").to_string().parse::<u32>().unwrap_or(1); // 1=Monday, 7=Sunday
    let days_since_monday = weekday_num - 1;
    let monday = today.checked_sub_signed(Duration::days(days_since_monday as i64)).unwrap_or(today);
    let sunday = monday.checked_add_signed(Duration::days(6)).unwrap_or(monday);

    let start_date = monday.format("%Y-%m-%d").to_string();
    let end_date = sunday.format("%Y-%m-%d").to_string();

    if let Some(emp) = employee {
        // Get schedule for specific employee
        match handle.get_schedule_for_date_range(emp.clone(), start_date.clone(), end_date.clone()).await {
            Ok(schedule) => {
                let mut response = format!("**Work Schedule for {} ({} to {})**\n\n", emp, start_date, end_date);
                
                if schedule.schedule.is_empty() {
                    response.push_str("No schedule entries found.");
                } else {
                    for entry in schedule.schedule {
                        // Parse date to get day of week
                        if let Ok(date) = NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d") {
                            let weekday_num = date.format("%u").to_string().parse::<u32>().unwrap_or(0);
                            let day_name = match weekday_num {
                                1 => "Monday",
                                2 => "Tuesday",
                                3 => "Wednesday",
                                4 => "Thursday",
                                5 => "Friday",
                                6 => "Saturday",
                                7 => "Sunday",
                                _ => "Unknown",
                            };
                            
                            response.push_str(&format!("**{}** ({}): {}\n", day_name, entry.date, entry.format()));
                        } else {
                            response.push_str(&format!("**{}**: {}\n", entry.date, entry.format()));
                        }
                    }
                }
                
                ctx.say(response).await?;
            }
            Err(e) => {
                ctx.say(format!("Error fetching schedule: {}", e)).await?;
            }
        }
    } else {
        // Get all employees
        match handle.get_employees().await {
            Ok(employees) => {
                if employees.is_empty() {
                    ctx.say("No employees found with schedules.").await?;
                    return Ok(());
                }
                
                let mut response = format!("**Work Schedules ({} to {})**\n\n", start_date, end_date);
                
                for emp in employees {
                    match handle.get_schedule_for_date_range(emp.clone(), start_date.clone(), end_date.clone()).await {
                        Ok(schedule) => {
                            response.push_str(&format!("**{}**\n", emp));
                            
                            if schedule.schedule.is_empty() {
                                response.push_str("No schedule entries found.\n\n");
                            } else {
                                for entry in schedule.schedule {
                                    // Parse date to get day of week
                                    if let Ok(date) = NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d") {
                                        let weekday_num = date.format("%u").to_string().parse::<u32>().unwrap_or(0);
                                        let day_name = match weekday_num {
                                            1 => "Mon",
                                            2 => "Tue",
                                            3 => "Wed",
                                            4 => "Thu",
                                            5 => "Fri",
                                            6 => "Sat",
                                            7 => "Sun",
                                            _ => "???",
                                        };
                                        
                                        response.push_str(&format!("  - {} ({}): {}\n", day_name, entry.date, entry.format()));
                                    } else {
                                        response.push_str(&format!("  - {}: {}\n", entry.date, entry.format()));
                                    }
                                }
                                response.push('\n');
                            }
                        }
                        Err(e) => {
                            response.push_str(&format!("Error fetching schedule for {}: {}\n\n", emp, e));
                        }
                    }
                }
                
                ctx.say(response).await?;
            }
            Err(e) => {
                ctx.say(format!("Error fetching employees: {}", e)).await?;
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
    // Get the handle to work schedule
    let handle = get_work_schedule_handle(ctx.data().component_manager.as_ref(), ctx.data().config.clone()).await;

    // Validate date format
    if NaiveDate::parse_from_str(&date, "%Y-%m-%d").is_err() {
        ctx.send(
            poise::CreateReply::default()
                .embed(create_warning_embed(
                    &t!("work_schedule_invalid_date"),
                    &t!("work_schedule_invalid_date")
                ))
                .ephemeral(true)
        ).await?;
        return Ok(());
    }

    if let Some(emp) = employee {
        // Get schedule for specific employee on specific date
        match handle.get_entry_for_employee_date(&emp, &date).await {
            Ok(entry) => {
                let title = t!("work_schedule_employee_date_title", employee = emp, date = date);
                ctx.send(
                    poise::CreateReply::default()
                        .embed(create_success_embed(
                            &title,
                            &entry.format()
                        ))
                ).await?;
            }
            Err(e) => {
                ctx.send(
                    poise::CreateReply::default()
                        .embed(create_error_embed(
                            &t!("error_title", context = "schedule"),
                            &t!("work_schedule_error_fetching", resource = "schedule", error = e.to_string())
                        ))
                        .ephemeral(true)
                ).await?;
            }
        }
    } else {
        // Get schedule for all employees on specific date
        match handle.get_schedule_for_date(&date).await {
            Ok(schedules) => {
                if schedules.is_empty() {
                    ctx.send(
                        poise::CreateReply::default()
                            .embed(create_info_embed(
                                &t!("work_schedule_date_title", date = date),
                                &t!("work_schedule_no_schedules_found", date = date)
                            ))
                    ).await?;
                    return Ok(());
                }
                
                let title = t!("work_schedule_date_title", date = date);
                let mut embed = serenity::CreateEmbed::new()
                    .title(title)
                    .color(0x00_99_FF); // Blue color
                
                for (emp, entry) in schedules {
                    embed = embed.field(emp, entry.format(), false);
                }
                
                ctx.send(
                    poise::CreateReply::default()
                        .embed(embed)
                ).await?;
            }
            Err(e) => {
                ctx.send(
                    poise::CreateReply::default()
                        .embed(create_error_embed(
                            &t!("error_title", context = "schedule"),
                            &t!("work_schedule_error_fetching", resource = "schedules", error = e.to_string())
                        ))
                        .ephemeral(true)
                ).await?;
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
    // Get the handle to work schedule
    let handle = get_work_schedule_handle(ctx.data().component_manager.as_ref(), ctx.data().config.clone()).await;

    // Get all entries for this employee (which will be for the current week after our fix)
    match handle.get_schedule_for_employee(employee.clone()).await {
        Ok(schedule) => {
            if schedule.schedule.is_empty() {
                ctx.say(format!("No schedule entries found for {} this week.", employee)).await?;
                return Ok(());
            }
            
            // Calculate the date range for this week to show in the response
            let now = chrono::Local::now();
            let today = now.date_naive();
            let weekday_num = today.format("%u").to_string().parse::<u32>().unwrap_or(1); 
            let days_since_monday = weekday_num - 1;
            let monday = today.checked_sub_signed(chrono::Duration::days(days_since_monday as i64)).unwrap_or(today);
            let sunday = monday.checked_add_signed(chrono::Duration::days(6)).unwrap_or(monday);
            
            let start_date = monday.format("%Y-%m-%d").to_string();
            let end_date = sunday.format("%Y-%m-%d").to_string();
            
            let mut response = format!("**Work Schedule for {} ({} to {})**\n\n", employee, start_date, end_date);
            
            for entry in schedule.schedule {
                // Parse date to get day of week
                if let Ok(date) = NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d") {
                    let weekday_num = date.format("%u").to_string().parse::<u32>().unwrap_or(0);
                    let day_name = match weekday_num {
                        1 => "Monday",
                        2 => "Tuesday",
                        3 => "Wednesday",
                        4 => "Thursday",
                        5 => "Friday",
                        6 => "Saturday",
                        7 => "Sunday",
                        _ => "Unknown",
                    };
                    
                    response.push_str(&format!("**{}** ({}): {}\n", day_name, entry.date, entry.format()));
                } else {
                    response.push_str(&format!("**{}**: {}\n", entry.date, entry.format()));
                }
            }
            
            ctx.say(response).await?;
        }
        Err(e) => {
            ctx.say(format!("Error fetching schedule: {}", e)).await?;
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
    // Get the handle to work schedule
    let handle = get_work_schedule_handle(ctx.data().component_manager.as_ref(), ctx.data().config.clone()).await;

    // Calculate the date range for next week (Monday to Sunday)
    let now = Local::now();
    let today = now.date_naive();
    let weekday_num = today.format("%u").to_string().parse::<u32>().unwrap_or(1); // 1=Monday, 7=Sunday
    let days_since_monday = weekday_num - 1;
    
    // Get this week's Monday, then add 7 days to get next week's Monday
    let this_monday = today.checked_sub_signed(Duration::days(days_since_monday as i64)).unwrap_or(today);
    let next_monday = this_monday.checked_add_signed(Duration::days(7)).unwrap_or(this_monday);
    let next_sunday = next_monday.checked_add_signed(Duration::days(6)).unwrap_or(next_monday);

    let start_date = next_monday.format("%Y-%m-%d").to_string();
    let end_date = next_sunday.format("%Y-%m-%d").to_string();

    if let Some(emp) = employee {
        // Get schedule for specific employee
        match handle.get_schedule_for_date_range(emp.clone(), start_date.clone(), end_date.clone()).await {
            Ok(schedule) => {
                let mut response = format!("**Next Week's Work Schedule for {} ({} to {})**\n\n", emp, start_date, end_date);
                
                if schedule.schedule.is_empty() {
                    response.push_str("No schedule entries found.");
                } else {
                    for entry in schedule.schedule {
                        // Parse date to get day of week
                        if let Ok(date) = NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d") {
                            let weekday_num = date.format("%u").to_string().parse::<u32>().unwrap_or(0);
                            let day_name = match weekday_num {
                                1 => "Monday",
                                2 => "Tuesday",
                                3 => "Wednesday",
                                4 => "Thursday",
                                5 => "Friday",
                                6 => "Saturday",
                                7 => "Sunday",
                                _ => "Unknown",
                            };
                            
                            response.push_str(&format!("**{}** ({}): {}\n", day_name, entry.date, entry.format()));
                        } else {
                            response.push_str(&format!("**{}**: {}\n", entry.date, entry.format()));
                        }
                    }
                }
                
                ctx.say(response).await?;
            }
            Err(e) => {
                ctx.say(format!("Error fetching schedule: {}", e)).await?;
            }
        }
    } else {
        // Get all employees
        match handle.get_employees().await {
            Ok(employees) => {
                if employees.is_empty() {
                    ctx.say("No employees found with schedules.").await?;
                    return Ok(());
                }
                
                let mut response = format!("**Next Week's Work Schedules ({} to {})**\n\n", start_date, end_date);
                
                for emp in employees {
                    match handle.get_schedule_for_date_range(emp.clone(), start_date.clone(), end_date.clone()).await {
                        Ok(schedule) => {
                            response.push_str(&format!("**{}**\n", emp));
                            
                            if schedule.schedule.is_empty() {
                                response.push_str("No schedule entries found.\n\n");
                            } else {
                                for entry in schedule.schedule {
                                    // Parse date to get day of week
                                    if let Ok(date) = NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d") {
                                        let weekday_num = date.format("%u").to_string().parse::<u32>().unwrap_or(0);
                                        let day_name = match weekday_num {
                                            1 => "Mon",
                                            2 => "Tue",
                                            3 => "Wed",
                                            4 => "Thu",
                                            5 => "Fri",
                                            6 => "Sat",
                                            7 => "Sun",
                                            _ => "???",
                                        };
                                        
                                        response.push_str(&format!("  - {} ({}): {}\n", day_name, entry.date, entry.format()));
                                    } else {
                                        response.push_str(&format!("  - {}: {}\n", entry.date, entry.format()));
                                    }
                                }
                                response.push('\n');
                            }
                        }
                        Err(e) => {
                            response.push_str(&format!("Error fetching schedule for {}: {}\n\n", emp, e));
                        }
                    }
                }
                
                ctx.say(response).await?;
            }
            Err(e) => {
                ctx.say(format!("Error fetching employees: {}", e)).await?;
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
            if let Some(work_schedule_component) = component
                .as_any()
                .downcast_ref::<WorkSchedule>()
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