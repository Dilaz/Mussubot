use crate::model::{WorkDay, WorkDayExtraction, WorkSchedule};
use base64::{self, engine::Engine};
use chrono::{Datelike, Duration, Local, NaiveDate};
use rig::completion::message::{Image, ImageMediaType};
use rig::completion::{Chat, Message};
use rig::message::ContentFormat;
use rig::providers::gemini::Client as GeminiClient;
use serde_json::from_str;
use std::env;
use tracing::{error, info, warn};

/// Prompt for the Gemini agent to extract the work schedule from the image
const PROMPT: &str = "Analyze the provided image, which is a work schedule.

First, identify the following details from the image:
1.  The **employee name** provided: **{NAME}**
2.  The **full date range** specified in the title (e.g., \"24.3.-13.4.2025\").
3.  The **year** from the date range.
4.  The **start date** and **end date** from the range.
5.  All the individual **day/month column headers** visible within that date range (e.g., \"{DAY1}\", \"{DAY2}\", ...).

Your main task is to extract the complete work schedule for the employee **{NAME}** covering the date range from **{START_DATE}** to **{END_DATE}** (weeks {WEEK_NUMBERS}).

Output the schedule as a JSON array where each object represents a single day within the identified date range. Each object must have the following exact structure:
{
  \"date\": \"YYYY-MM-DD\",
  \"work_hours\": \"VALUE_FROM_CELL\"
}

Follow these SUPER strict rules for generating the JSON output:
1.  **Date Format:** The 'date' value must be in \"YYYY-MM-DD\" format. Use the **year identified from the image title**. Correctly map the day and month from each column header identified (e.g., \"{DAY1}\" becomes \"{YEAR}-{MONTH1}-{DATE1}\", \"{DAY2}\" becomes \"{YEAR}-{MONTH2}-{DATE2}\").
2.  **Work Hours Value:** The 'work_hours' value must be the **primary text content** found in the specific cell corresponding to the employee **{NAME}** and the specific date column.
    *   If the cell contains a time range (e.g., \"7-15\", \"12-20.30\", \"8.35-16\"), use that exact string. Normalize commas in times to periods (e.g., \"7,30-15.30\" becomes \"7.30-15.30\").
    *   If the cell contains a code (e.g., \"v\", \"x\", \"vp\", \"vv\", \"VL\", \"S\", \"tst\"), use that exact code string.
    *   If the cell contains descriptive text (e.g., \"Toive\", \"Pai-kalla\", \"kuorma\"), use that exact text. If the text spans multiple lines within the cell (like \"Pai-\" above \"kalla\"), combine them into a single representative string (e.g., \"Pai-kalla\"). If it's like \"Toive\" above \"vp\", combine as \"Toive vp\".
    *   **Crucially:** Some cells contain both a primary entry (time/code/text) *and* a separate numerical value below it (often representing hours like '8', '7.5', '8.25'). **Use only the primary entry (time/code/text) as the 'work_hours' value.** Ignore the separate numerical value at the bottom *unless* it is the *only* content in that cell.
    *   If the cell for a specific date in the employee's row is **completely empty** (contains no text, code, or number), use an empty string `\"\"` for the 'work_hours' value.
3.  **Completeness:** Ensure you generate one JSON object for **every single date column header identified** within the start and end dates provided ({START_DATE} to {END_DATE}). The total number of objects in the array must match the number of days shown in the schedule grid for the specified range.
4.  **Accuracy:** Meticulously map the employee's specific row ({NAME}) to the correct date column using the day/month headers (e.g., \"{DAY1}\", \"{DAY2}\", etc.) across all the weeks shown (e.g., {WEEK_EXAMPLES}). Accuracy in matching the cell content to the correct date is paramount.

Provide **only** the final JSON array as the output. Do not include any introductory text, explanations, list of identified details, markdown formatting (`json`), or comments before or after the JSON data.";

pub async fn parse_schedule_image(
    employee_name: &str,
    image_data: &[u8],
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> Result<WorkSchedule, String> {
    // Log the parsing action
    info!("Parsing schedule image for employee: {}", employee_name);
    info!("Image size: {} bytes", image_data.len());

    if let (Some(start), Some(end)) = (start_date, end_date) {
        info!("Date range: {} to {}", start, end);
    }

    #[cfg(feature = "web-interface")]
    {
        // Use env to get API key
        let api_key = env::var("GEMINI_API_KEY")
            .map_err(|_| "GEMINI_API_KEY environment variable not set".to_string())?;

        // Get model name from environment variable
        let model =
            env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.5-pro-exp-03-25".to_string());

        info!("Using Gemini model: {}", model);

        // Initialize Gemini client with API key
        let gemini_client = GeminiClient::new(&api_key);
        let year = Local::now().year();

        // Calculate default dates if not provided
        let (start_date_str, end_date_str) = match (start_date, end_date) {
            (Some(start), Some(end)) => (start.to_string(), end.to_string()),
            _ => {
                // Get tomorrow's date
                let tomorrow = Local::now().date_naive() + Duration::days(1);

                // Find the next Monday from tomorrow
                let days_until_monday = (8 - tomorrow.weekday().num_days_from_monday()) % 7;
                let start = tomorrow + Duration::days(days_until_monday as i64);

                // Calculate end date (3 weeks from start date)
                let end = start + Duration::days(21 - 1);

                (
                    start.format("%Y-%m-%d").to_string(),
                    end.format("%Y-%m-%d").to_string(),
                )
            }
        };

        // Parse the start and end dates
        let start_date_obj = NaiveDate::parse_from_str(&start_date_str, "%Y-%m-%d")
            .map_err(|_| "Invalid start date format".to_string())?;
        let end_date_obj = NaiveDate::parse_from_str(&end_date_str, "%Y-%m-%d")
            .map_err(|_| "Invalid end date format".to_string())?;

        // Calculate week numbers
        let start_week = start_date_obj.iso_week().week();
        let end_week = end_date_obj.iso_week().week();

        // Create week numbers string (e.g., "10, 11, 12" or "10-12")
        let week_numbers = if start_week == end_week {
            format!("{}", start_week)
        } else if start_week + 1 == end_week {
            format!("{}, {}", start_week, end_week)
        } else {
            format!("{}-{}", start_week, end_week)
        };

        // Generate week examples (e.g., "vko 10, vko 11, vko 12")
        let mut week_examples = String::new();
        for week in start_week..=end_week {
            if !week_examples.is_empty() {
                week_examples.push_str(", ");
            }
            week_examples.push_str(&format!("vko {}", week));
        }

        // Format the first day (usually Monday)
        let day1 = format!("Ma {}.{}.", start_date_obj.day(), start_date_obj.month());

        // Format the second day (usually Tuesday)
        let day2 = format!(
            "Ti {}.{}.",
            (start_date_obj + Duration::days(1)).day(),
            (start_date_obj + Duration::days(1)).month()
        );

        // Format for example dates in prompt
        let month1 = format!("{:02}", start_date_obj.month());
        let date1 = format!("{:02}", start_date_obj.day());
        let month2 = format!("{:02}", (start_date_obj + Duration::days(1)).month());
        let date2 = format!("{:02}", (start_date_obj + Duration::days(1)).day());

        // Replace placeholders in prompt with actual values
        let prompt = PROMPT
            .replace("{NAME}", employee_name)
            .replace("{YEAR}", &year.to_string())
            .replace("{START_DATE}", &start_date_str)
            .replace("{END_DATE}", &end_date_str)
            .replace("{WEEK_NUMBERS}", &week_numbers)
            .replace("{WEEK_EXAMPLES}", &week_examples)
            .replace("{DAY1}", &day1)
            .replace("{DAY2}", &day2)
            .replace("{MONTH1}", &month1)
            .replace("{DATE1}", &date1)
            .replace("{MONTH2}", &month2)
            .replace("{DATE2}", &date2);

        // Base64 encode the image
        let base64_image = base64::engine::general_purpose::STANDARD.encode(image_data);

        let image = Image {
            data: base64_image,
            media_type: Some(ImageMediaType::JPEG),
            format: Some(ContentFormat::Base64),
            detail: None,
        };

        // Create an image message by direct conversion from Image struct
        let image_message = Message::from(image);

        // Create combined message vector
        let messages = vec![image_message];

        // Get the text response from Gemini
        let agent = gemini_client
            .agent(&model)
            .preamble("You are a helpful assistant that parses work schedules from images.")
            .temperature(0.0)
            .build();

        let response = agent
            .chat(prompt, messages)
            .await
            .map_err(|err| format!("Failed to complete request: {}", err))?;

        info!("Received response: {}", response);

        // Extract JSON array from the response
        let extracted_days: Vec<WorkDayExtraction> = extract_json_array(&response)
            .map_err(|e| format!("Failed to extract valid JSON from response: {}", e))?;

        // Convert to our internal WorkSchedule format
        let mut schedule = WorkSchedule::new(employee_name.to_string());

        for day in extracted_days {
            // Parse date
            if let Ok(_date) = NaiveDate::parse_from_str(&day.date, "%Y-%m-%d") {
                if day.work_hours.is_empty() {
                    // Empty work hours cell
                    schedule.add_day(WorkDay {
                        date: day.date,
                        start_time: None,
                        end_time: None,
                        is_day_off: false,
                        notes: None,
                    });
                } else if day.work_hours.contains('-') {
                    // Parse time range like "7-15"
                    let parts: Vec<&str> = day.work_hours.split('-').collect();
                    if parts.len() == 2 {
                        let start = normalize_time(parts[0]);
                        let end = normalize_time(parts[1]);

                        schedule.add_day(WorkDay {
                            date: day.date,
                            start_time: Some(start),
                            end_time: Some(end),
                            is_day_off: false,
                            notes: None,
                        });
                    } else {
                        // Can't parse time range, store as note
                        schedule.add_day(WorkDay {
                            date: day.date,
                            start_time: None,
                            end_time: None,
                            is_day_off: false,
                            notes: Some(day.work_hours),
                        });
                    }
                } else {
                    // Special codes like "x", "v", etc. or other text
                    let is_day_off = day.work_hours.to_lowercase() == "x"
                        || day.work_hours.to_lowercase() == "v";

                    schedule.add_day(WorkDay {
                        date: day.date,
                        start_time: None,
                        end_time: None,
                        is_day_off,
                        notes: Some(day.work_hours),
                    });
                }
            } else {
                warn!("Invalid date format: {}", day.date);
            }
        }

        Ok(schedule)
    }

    #[cfg(not(feature = "web-interface"))]
    {
        // Mock implementation for when the feature is not enabled
        mock_parse_schedule(employee_name)
    }
}

// New helper function to extract a valid JSON array from potentially messy text
#[cfg(feature = "web-interface")]
fn extract_json_array(text: &str) -> Result<Vec<WorkDayExtraction>, String> {
    // Try to explicitly handle ```json ... ``` format first
    let trimmed_text = text.trim();

    // Handle ```json ... ``` format
    let trimmed_text = if trimmed_text.starts_with("```json") && trimmed_text.ends_with("```") {
        trimmed_text
            .trim_start_matches("```json")
            .trim_end_matches("```")
            .trim()
    }
    // Handle ``` ... ``` format (without language specification)
    else if trimmed_text.starts_with("```") && trimmed_text.ends_with("```") {
        trimmed_text
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
    } else {
        trimmed_text
    };

    info!("Trimmed text: {}", trimmed_text);

    // Try to parse the trimmed text
    if let Ok(days) = from_str::<Vec<WorkDayExtraction>>(trimmed_text) {
        return Ok(days);
    }

    // Fall back to our previous methods
    // Try to parse the response as JSON directly first
    if let Ok(days) = from_str::<Vec<WorkDayExtraction>>(text) {
        return Ok(days);
    }

    // Look for the first '[' and last ']' to extract the JSON array
    if let (Some(start), Some(end)) = (text.find('['), text.rfind(']')) {
        if start < end {
            let json_part = &text[start..=end];
            match from_str(json_part) {
                Ok(days) => return Ok(days),
                Err(e) => {
                    error!("Found JSON-like structure but couldn't parse it: {}", e);
                    error!("Extracted JSON-like content: {}", json_part);
                }
            }
        }
    }

    // If we get here, try to clean up the text and look for code blocks
    let clean_text = text.replace("```json", "").replace("```", "");

    // Try again with cleaned text
    if let Ok(days) = from_str::<Vec<WorkDayExtraction>>(&clean_text) {
        return Ok(days);
    }

    // Look for array in cleaned text
    if let (Some(start), Some(end)) = (clean_text.find('['), clean_text.rfind(']')) {
        if start < end {
            let json_part = &clean_text[start..=end];
            match from_str(json_part) {
                Ok(days) => return Ok(days),
                Err(e) => {
                    error!(
                        "Found cleaned JSON-like structure but couldn't parse it: {}",
                        e
                    );
                    error!("Cleaned JSON-like content: {}", json_part);
                }
            }
        }
    }

    // Last resort - try to construct valid JSON manually from the parts
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();

    if lines
        .iter()
        .any(|l| l.contains("\"date\"") && l.contains("\"work_hours\""))
    {
        // This looks like it contains JSON objects - try to reconstruct the array
        let array_text = format!("[{}]", lines.join(""));
        if let Ok(days) = from_str::<Vec<WorkDayExtraction>>(&array_text) {
            return Ok(days);
        }
    }

    Err("Could not extract valid JSON from the model response".to_string())
}

#[cfg(not(feature = "web-interface"))]
fn mock_parse_schedule(employee_name: &str) -> Result<WorkSchedule, String> {
    let mut schedule = WorkSchedule::new(employee_name.to_string());

    // Get current date
    let today = Local::now().date_naive();

    // Find the Monday of this week
    let monday = today - Duration::days(today.weekday().num_days_from_monday() as i64);

    // Create a week's worth of workdays
    for i in 0..7 {
        let day = monday + Duration::days(i);
        let day_str = day.format("%Y-%m-%d").to_string();

        // Weekdays (Monday-Friday)
        if i < 5 {
            schedule.add_day(WorkDay {
                date: day_str,
                start_time: Some("08:00".to_string()),
                end_time: Some("16:00".to_string()),
                is_day_off: false,
                notes: None,
            });
        } else {
            // Weekend (Saturday-Sunday)
            schedule.add_day(WorkDay {
                date: day_str,
                start_time: None,
                end_time: None,
                is_day_off: true,
                notes: Some("Weekend".to_string()),
            });
        }
    }

    Ok(schedule)
}

#[cfg(feature = "web-interface")]
// Helper function to normalize time strings
// Converts formats like "7" to "07:00"
fn normalize_time(time_str: &str) -> String {
    let time_str = time_str.trim();

    // If it already has a colon, assume HH:MM format
    if time_str.contains(':') {
        return time_str.to_string();
    }

    // Otherwise assume it's just hours
    match time_str.parse::<u32>() {
        Ok(hours) if hours < 24 => format!("{:02}:00", hours),
        _ => time_str.to_string(), // Return as is if invalid
    }
}
